# Architecture

Safay is a Rust workspace. The cryptography and data formats live in plain Rust crates with no UI or I/O; the
application layer owns the single unlocked session; the interface is written with
[NSH](https://github.com/longbridge/nsh); operating-system services NSH does not provide are thin adapters in
`src/platform/`.

## Layout

| Path | Contents |
|---|---|
| `crates/safay-core` | Vault format and encryption, TOTP, Steam Guard, and Battle.net codes, Base32, `otpauth://` and Google migration parsing, canonical account fields, search and masking, password rules, imports (Safay exports and other apps' backups and CSV exports, `import`), sync decisions, site-icon fetch planning. No I/O. |
| `crates/safay-app` | The session and its controller, storage (`Repository`), preferences, sync and cloud backends, the launcher search index, the secret clipboard port. No UI and no implicit production I/O. |
| `src/` | The NSH application: pages, components, the per-window `Store`, desktop windows, platform adapters. |
| `golden/` | Behaviour baseline: JSON vectors every core change is checked against (`crates/safay-core/tests/golden.rs`). |
| `integrations/` | Launcher integrations: the Raycast/Tinycast extension and the Alfred workflow. |
| `previews/` | Debug-only, in-memory entry points for UI tests and screenshots. |
| `locales/`, `theme*.json`, `assets/` | Translations, design tokens per platform, fonts and images. |

Cloud storage protocols (WebDAV, Nextcloud, Synology, Dropbox, OneDrive, Google Drive) come from the
[cloudset](https://crates.io/crates/cloudset) crate, a sans-IO client: it builds requests, parses responses and
advances state machines, and `src/platform/http.rs` sends the requests.

## Session model

```
Controller (one per process)
 └─ Mutex<Session>        unlocked contents and key; every domain operation runs here, serialized
 └─ Launcher              search index readable without the session lock
Store (one per window: main window, menu bar panel, settings window)
 └─ projections (signals) and commands that dispatch operations to the Controller
```

- **One session.** Only the `Controller`'s `Session` holds decrypted contents and the vault key. Windows never get
  their own copy: each window's content creates a `Store` that publishes a projection (what that window shows) and
  sends commands. Key derivation and file work run on background threads.
- **Epochs.** Locking increments an epoch before anything else, so every window hides its content at once and any
  result that arrives later for the old epoch is discarded instead of reviving the session.
- **Revisions.** After a user operation the controller bumps a revision and every other window's store re-projects
  (unlocking in the menu bar panel updates the main window). Periodic refreshes do not bump it.
- **Ownership.** Tasks and timers belong to the window content that owns the store, not to the button that started
  them, so locking (which unmounts the button) cannot strand them.
- **Ports.** The session talks to the outside only through injected ports: `Repository` (files), `Clock`, the sync
  `Backend`, the HTTP executor and the secret clipboard. Tests inject memory implementations.

## Data on disk

The data directory is named after the app ID — `com.gicek.safay`, or `com.gicek.safay.debug` for development builds,
so a development build never touches a release build's data:

| Platform | Directory |
|---|---|
| macOS | `~/Library/Application Support/<app ID>` |
| Windows | `%LOCALAPPDATA%\<app ID>` |
| Linux | `$XDG_DATA_HOME/<app ID>` (default `~/.local/share/<app ID>`) |

| File | Contents |
|---|---|
| `vault.safayvault` | The encrypted vault (format v2, see [Security](../SECURITY.md#vault)) |
| `last-good.safayvault` | The previous authenticated vault, restorable from Settings |
| `preferences.json` | Non-secret settings |
| `attempts.json` | The number of consecutive failed unlocks |
| `sync.json` | The backup location, sync markers and history; credentials are only referenced by ID |
| `biometric-key.json` | The Touch ID–wrapped vault key (macOS) |
| `instance.lock` | Keeps a second Safay process from writing |

Exports are encrypted files of the same format with their own salt, marked `"app": "safay"`. A backup location holds
`vault.safayvault` (the vault as is) and `vault.metadata.json` (device name and time): in a `Safay` folder inside the
chosen folder or the WebDAV and Nextcloud account, in the folder picked on a Synology NAS, and at the root of Safay's
app folder on Dropbox, OneDrive and Google Drive ("Apps › Safay", or the folder named for it in My Drive). iCloud keeps
them in one CloudKit record.

A Synology sign-in reaches the whole NAS (`cloud::synology_sign_in`) and stays in memory while the user picks a folder
inside a shared folder: an empty one or one with a Safay backup when backing up, one with a backup when restoring. The
connection then moves to that folder (`Connection::with_root_folder`, keeping the session) and is stored like any
other. Until then the sign-in outlives the sheet, so choosing Synology again returns to the folders; choosing another
service, linking another location, creating the vault on first run, going back to the sign-in form or locking signs
it out.

## Backup and sync

A backup location implements `sync::Backend` (read, conditional write, revision). The folder backend writes to a
directory; `CloudBackend` drives `cloudset`; the iCloud backend uses CloudKit. The core's `sync::decide` compares the
local vault, the remote copy and the last synced state and returns upload, download, conflict or "cannot open". Saves
are debounced into one sync after two seconds; unlocking and linking sync at once. Only the main window runs syncs.

On a conflict the user reviews the differences and merges (`safay_core::merge`). A deleted account is never a
difference: a merge keeps the deletion of an account deleted on either side, so it reaches the other devices.

### Cloud backup consistency

An upload that replaces a backup is conditional on the version the sync compared against. Where the provider enforces
the condition atomically (Dropbox, OneDrive single requests, WebDAV and Nextcloud with strong ETags, iCloud), a change
from another device makes the write fail. Where it cannot (Google Drive, Synology, OneDrive upload sessions, WebDAV
with weak ETags), `CloudBackend` checks the version just before the upload and reads the file back after it; a
difference either way is reported as `SyncError::RemoteChanged`, and the sync reads and compares again, so the other
device's change becomes a conflict. Only another device's upload within those moments can still be replaced. Synology
files have no revision: their modification time is the version. A backup with no version at all (a WebDAV server
without ETags) cannot be checked, so its upload waits (`SyncError::ManualWriteRequired`, shown as a warning and not
recorded as a failure) until the user approves it in Backup & Sync (`sync::replace`, which compares once more first).
A completed upload with an uncertain outcome is checked by reading back the ciphertext. Sync records its baseline
only when the upload succeeded or the read returned the same bytes. The cloud upload and restore limits both use
`storage::MAX_FILE_BYTES` (16 MiB). Refreshed credentials go to the credential store before the next HTTP request.

## Recently Deleted

Deleting an account sets its `deletedAt` (`safay_core::trash`); it keeps its secret, place and categories, and is left
out of every list, count, search, export, launcher index and duplicate check. Recovering clears the field. Thirty days
after deletion — checked when the vault opens and hourly — or when the user deletes it from Recently Deleted or empties
it, the account is removed and its secret wiped. Versions 1.1 and earlier ignore the field: they show such an account as in
use, and drop the deletion if they save the vault.

## Networking

Requests are built by the core and executed by `src/platform/http.rs` (`ureq` + rustls with the platform verifier),
which sends them exactly as built: no redirects, no cookies, bounded bodies, fixed timeouts. `src/platform/tls.rs` adds
the pinned-certificate path for self-signed servers. The rules are in [Security](../SECURITY.md#network).

## Launcher integration

```
Alfred / Raycast / Tinycast ── osascript ── integrations/shared/safay.applescript
        ── Apple event ──► src/platform/automation_macos.rs (NSScriptCommand)
                             ├─ find accounts ► Controller::launcher() (index, masked)
                             └─ copy code     ► src/launcher.rs: Touch ID ► Session ► clipboard ► desktop notice
                                                └─ no Touch ID: main window asks for the master password
```

The commands are declared in `resources/Safay.sdef`. The bridge script addresses them by raw event codes, so it
compiles without Safay installed and never launches it. The extension's text and the bridge are generated into the
extension and the Alfred workflow by `scripts/integrations.py`.

## NSH and what is built outside it

All interface code uses NSH: components, windows (main window, settings window, menu bar panel, desktop notice), the
tray, global shortcuts, the application menu. NSH's API is taken from the Rust sources of the commit pinned in
`Cargo.lock`. Capabilities NSH does not offer are implemented in `src/platform/` and listed below with the condition for
moving back to NSH. Review this list on every NSH upgrade.

The app's root declares its one main window (`shell::main_window`, id `main`); the settings window, the panel and the
notice are opened from it. A root mounted from TSX (the previews that `nsh test` and the screenshots run) returns
`shell::main_content` instead: NSH gives such a root its first window, and one declared inside it would be a second. NSH's own first window is an app host that never shows, so nothing reads AppKit's window
list to find the main window: `shell::main_ref` is its handle, and `platform::macos` keeps whether it is on screen
(the Dock icon setting follows that). Its close button hides it while the menu bar or tray icon keeps the app running.
The panel, and the main window when it comes back from hiding, open on the display the pointer is on.

A file's name in `src/platform/` says where it is built: without a suffix on every system, with `_macos`, `_ios`,
`_linux`, `_windows` or `_android` on that system only, with `_apple` on macOS and iOS. The implementations of one
feature share its name, and the module is named without the suffix: `camera_macos.rs`, `camera_linux.rs`,
`camera_android.rs` and `camera_ios.rs` are all `platform::camera`.

### Phones and tablets

Android and iOS share the phone and tablet interface (`src/mobile`): the same pages, store and controller, drawn by the
controls of `mobile::ui`. On iOS (`ui::IOS`) those controls take the system's look and tokens (`theme-ios.json`), and
the interface uses the system's own parts where NSH offers them: the pages are a navigation stack
(`mobile::native_stack`, `UINavigationController`: its header, large titles, the swipe back; `Nav` follows the pops it
reports), the list's buttons, the account's menu and the settings' choices are system menus, the account's details and
"Add to" are system sheets, switches and segmented controls are the system's, and the category name is asked in a system
alert. Dialogs with password fields stay drawn, in the look of the system's alerts (`ui::dialog`, `ui::compact`): the
system's alert has no secure field.

Each platform gets its own form of the same part. A dialog holds one field at most; a task with more (changing the
master password, an export's password, choosing the backup location) is a page. Fields are a form of the platform's
(`ui::form`): on iOS the rows of one inset group, with what a field's value means (the strength meter, a mismatch) said
under the group; on Android Material's outlined fields on the page. A choice among a few values is the system's pull-down
menu from the value at the row's end on iOS, a dialog of radio buttons on Android. Android's page titles start at the
leading edge, its drawn dialogs take the system alerts' surface, and its sheets' actions are icons; on iOS the search
field and its button float in Liquid Glass (`ui::glass`, NSH's `glass_effect`), categories are deleted in an edit mode,
and a sheet's actions are words in glass (`ui::glass_text_button`). Both keep the search at the bottom corner, in reach of
the thumb. A tick in "Add to" takes effect at once; a page whose task went to the background (linking a backup location,
resolving a conflict) goes back to settings, whose backup card shows how it ends. The pages over the list
change as one (`Nav`: the stack and the page in one `batch`), so the native stack never sees a route in between. A
window 700 pt wide or more (`ui::is_wide`: a tablet) lists the categories in a
sidebar and the accounts as cards in columns, and centers the pages over it. The `ios-look` feature shows the iOS look
in a desktop's development build (`SAFAY_MOBILE=1`, or `<width>x<height>`).

### Working around NSH issues

| Part | Where | NSH issue | Move back when |
|---|---|---|---|
| Alerts (macOS) | `src/alert.rs`, `platform::macos::alert` | `AlertDialog` is presented inside the UI batch's Core Animation transaction, so its appearance animation is lost; one button cannot answer both Return and Escape | NSH presents alerts outside the transaction and supports such a default button |
| Open, save and folder panels (macOS) | `platform::files::{open_paths, save_path}`, `platform::macos::file_panel` | Same presentation issue as alerts | Together with alerts |
| Showing the master password on the lock screen | `pages::unlock::capsule_password`, `platform::macos::scroll_to_caret` | A centered native field misplaces text set by the app, clips the last glyph and does not scroll to the caret; replacing the field on `masked` changes moves the text | NSH fixes the centered field; then one field toggling `masked` |
| HTTP for backups and icons | `platform::http`, `platform::tls` | NSH's `fetch` cannot disable redirects, bound the body, or pin a certificate | NSH's network API offers these controls |
| Data directory | `platform::data_directory` | NSH's `data_dir()` is available only once the engine runs; the vault and the instance lock open before it. NSH is given the same directory (`AppOptions::data_dir`) | NSH exposes it before the engine starts (the path must not change) |
| Update checks and downloads | `platform::updates` | `nsh-updater`'s complete flow is TSX-only; Rust apps bring their own HTTP. The app uses the crate for the manifest, its signature, the download's checks and the install (staged when downloaded, swapped in on quit) | NSH offers the flow to Rust apps |
| Update notes in each language | `scripts/release.py`, `pages::update::notes_in` | NSH's `localizedNotes` puts each language in a file beside the manifest: one more release asset per language. The manifest's `notes` carry every language instead, after `<!-- lang: <code> -->` lines | NSH can carry each language's notes inside the manifest |
| OAuth sign-in (macOS) | `platform::web_auth` (`ASWebAuthenticationSession`) | NSH's `open_auth_session` shows an in-app sign-in session only on iOS and Android; on macOS it opens the default browser and waits for a link of a claimed URL scheme. Linux and Windows use it (`platform::web_sign_in`) | NSH shows `ASWebAuthenticationSession` on macOS |
| OAuth sign-in with a long callback scheme (Android, iOS) | Android: `platform::web_sign_in_android` (a Custom Tab through `Bridge.openCustomTab`; the callback arrives through `next_url_scheme_open`). iOS: `platform::web_auth` (`ASWebAuthenticationSession`) | NSH's auth session refuses callback schemes longer than `MAX_SCHEME_LEN` (64), and Google's reversed client IDs are longer | NSH accepts such schemes |
| Saving an export (Android) | `platform::files::save_export` (written to `exports/` in Safay's no-backup directory, then the share sheet) | NSH has no save dialog on Android | NSH offers one |
| Preferred action of an alert (iOS) | `platform::ios::prefer_alert_action`, from `src/alert.rs`: marks the alert's default action once the system shows it, and redraws its buttons | NSH marks a preferred action only in alerts with a text field | NSH marks a dialog's default button |
| Alerts with a password field (iOS) | `ui::dialog`, drawn as the system's alert (Liquid Glass, gray capsule field, the default action gray until it can be taken) | `AlertDialogInput` has no secure entry: the system's alert would show the master password | `AlertDialogInput` masks its text; then `nsh_base::alert_dialog` with an input |
| A tablet's sidebar (iOS) | `mobile::home` draws its own sidebar beside the navigation stack | `SplitView` on iOS ends the app as UIKit adds a column's navigation controller that another controller already holds (`addChildViewController` hierarchy check) | NSH's `SplitView` runs on iPadOS |
| The list's title at the bar's start (iOS) | `mobile::native_stack`: a title view as wide as the window, which UIKit narrows to the room the buttons leave | A leading header item sits on a glass capsule on iOS 26 (no `hidesSharedBackground`); the system centers the title | NSH places a title at the bar's start, or a leading item without its background |
| Handing a second launch to the running Safay (Linux, Windows) | `platform::linux::hand_to_running` (GApplication's command line), `platform::windows::hand_to_running` (NSH's own `WM_COPYDATA` hand-over to the window it marks — recheck on every NSH upgrade); after an update's restart, `platform::windows::wait_for_replaced` waits for the old process (`NSH_RESTART_AFTER`) instead | NSH hands a launch over, and waits for the process a restart replaces, inside its runner, after `main` has opened the vault, whose instance lock the running Safay holds | NSH offers the hand-over before the runner starts |

### Capabilities NSH does not have

| Capability | Where | Platforms |
|---|---|---|
| Biometric unlock (key wrapping: the Secure Enclave and Touch ID or Face ID on macOS and iOS, `biometric_apple.rs`; a Keystore key that needs a strong biometric on Android, `android/java`'s `Biometric`) | `platform::biometric` | macOS, iOS, Android |
| Credential storage | `platform::credentials` (`keyring`: the Keychain on macOS and iOS, the Secret Service on Linux; on Android, files sealed with a hardware-backed key) | all |
| Secret clipboard (concealed, expiring) | `platform::clipboard` (iOS: `platform::ios::copy_sensitive`, local only and expiring) | all |
| Lock when the device locks; a cover over the open vault in the background (iOS) | `platform::ios` (UIKit notifications; a blurred window over the others) | iOS |
| iCloud backup; silent pushes | `platform::icloud` (`icloud_apple.rs`, CloudKit); macOS: `platform::macos::enable_remote_push` (adds the push callback to NSH's app delegate class at runtime — recheck on every NSH upgrade); iOS: `platform::ios::enable_remote_push`, the push heard through NSH's `on_application_event` | macOS, iOS |
| Camera and on-screen QR selection | macOS: `platform::camera`, `platform::screen_capture`, `scan` (the overlay). Linux: `platform::camera` (the Camera portal, PipeWire and GStreamer) and `platform::screenshot` (`screen_capture_linux.rs`: the Screenshot portal, the desktop's own selection). Android: `platform::camera` (`camera_android.rs` and `android/java`'s `Camera`: Camera2, the preview drawn into the `TextureView` of the native component `safay.camera`, the luma read with `rqrr`). iOS: `platform::camera` (`camera_ios.rs`: AVFoundation, the preview layer in the native component `safay.camera`, QR codes read by `AVCaptureMetadataOutput`) | macOS, Linux, Android, iOS |
| QR decoding and generation | `platform::files` (`rqrr`, `qrcode`) | all |
| SF Symbols | `platform::symbols` (rendered to template PNGs; SVG icons elsewhere) | macOS |
| Lock on sleep, screen lock and user switch | `platform::macos::observe`, `platform::windows`, `platform::linux` | all |
| Placing the tray panel (Wayland lets no app place a window) and closing it on a click outside; binding the global shortcuts' keys (Hyprland's portal binds none) | `platform::hyprland` (a window rule and binds over Hyprland's socket; the binds name NSH's private portal shortcut ids — recheck on every NSH upgrade) | Linux on Hyprland |
| Windows' color-scheme class for their menus, as each opens (NSH gives it only when the environment changes; recheck on every NSH upgrade) | `platform::linux::follow_scheme` | Linux |
| Dock policy at runtime | `platform::macos` | macOS |
| Closing the menu bar panel on clicks in other apps; pointer position | `platform::macos` | macOS |
| Roman-only input in password and key fields | `platform::macos::roman_input_only` | macOS |
| Shortcut recorder | `platform::hotkeys` (registration uses NSH; macOS records through AppKit, Linux from NSH's key events) | all |
| Device name for backup history | `platform::device_name` | all |
| Launcher commands (AppleScript) | `platform::automation`, `src/launcher.rs` | macOS |

Interface code shows a capability only when `platform::supports` says the platform has it; it never infers capabilities
from the visual style (`chrome::look()`).
