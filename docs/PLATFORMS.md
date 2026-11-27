# Platforms

macOS is the primary platform. Android has a phone interface of its own (`src/mobile`) over the same core and is
supported from Android 12. iOS and iPadOS share that interface in the system's look; they run in the simulator and on an
iPhone with a development package, and have not been released. Windows and Linux share every page and the whole core, but CI does not build
them yet.
Windows has not been run end to end; Linux has, on one system (see [Linux details](#linux-details)). The parts below
marked *coded* are written and untested on a real system. The interface shows a capability only where
`platform::supports` (in `src/platform/mod.rs`) says the platform has it.

## Capabilities

| Capability | macOS | Windows | Linux | Android | iOS |
|---|---|---|---|---|---|
| Vault, accounts, categories, codes, import and export | Yes | Coded | Coded | Yes (exports go to the share sheet) | Yes (exports go to the share sheet) |
| Folder backup | Yes | Coded | Coded | — | — |
| WebDAV, Nextcloud and Synology backup | Yes | Coded (Credential Manager) | Yes (the Secret Service: GNOME Keyring, KWallet) | Yes | Synology: yes; WebDAV, Nextcloud: coded (the Keychain) |
| Dropbox, OneDrive, Google Drive | Yes | Coded (sign-in in the browser) | Coded (sign-in in the browser) | Yes (sign-in in a Custom Tab) | Google Drive: yes; Dropbox, OneDrive: coded (the system sign-in session) |
| iCloud | Yes | — | — | — | Yes (development packages so far) |
| Touch ID / hardware-protected unlock | Touch ID | Planned: Windows Hello key release | Not planned (no common hardware key API) | Strong biometrics (a Keystore key) | Coded: Face ID or Touch ID (a Secure Enclave key) |
| Lock on sleep, screen lock, user switch | Yes | Coded (`WM_WTSSESSION_CHANGE`, power broadcasts) | Coded (logind, screensaver) | Yes (screen off) | Coded (device lock) |
| Auto-lock on app input | Yes | Partial (input inside the page only) | Partial | Yes (also in the background) | Yes |
| Secret clipboard | Yes | Coded (excluded from history and cloud clipboard) | Coded (password-manager hint, no PRIMARY) | Yes (marked sensitive, expires) | Coded (this device only, expires) |
| Menu bar / tray panel | Yes | Not yet wired (NSH supports it) | Yes (StatusNotifierItem) | — | — |
| Global shortcuts | Yes | Not yet wired (NSH supports them) | Yes (the GlobalShortcuts portal) | — | — |
| Launch at login, background launch | Yes | Not yet | Yes (XDG autostart; "Start in Tray") | — | — |
| Select a QR code on screen; camera | Yes | Not yet | Yes (the Screenshot and Camera portals) | Camera, and a photo from the gallery | Coded: camera; a photo from the library |
| Launchers (Alfred, Raycast, Tinycast) | Yes | — | — | — | — |
| App updates | Yes (signed packages with an update source) | Coded (the app's folder) | Yes (the app's folder) | Google Play, or a newer APK | The App Store (not yet) |

On Windows and Linux, Settings opens as a dialog over the main window; macOS uses its own window.

## macOS details

- Requires macOS 14 or later; tested on Apple silicon with macOS 26 and 27. The sidebar follows the running release:
  a floating sidebar on macOS 26, a flush sidebar on 15 and earlier and on 27 and later.
- iCloud and push notifications need a package signed with a provisioning profile for the app ID
  ([Development](DEVELOPMENT.md#signing-and-icloud)); ad-hoc packages show iCloud as unavailable.
- Dropbox, OneDrive and Google Drive appear only when the build has OAuth client IDs (`cloud-keys.toml`), on every
  system.
- Dropbox, OneDrive and Google Drive keep the backup in Safay's app folder: "Apps › Safay", or the folder named for
  it in My Drive.
- Selecting a QR code on screen needs Screen Recording permission; the camera needs a packaged app.

## Android details

- Requires Android 12 (API 31) or later; targets API 36. Packages: one APK for each ABI (`arm64-v8a` for nearly all
  phones, `armeabi-v7a` for 32-bit ones) and an App Bundle for Google Play. The APKs and Google Play share one signing
  key, so an install from either updates from the other.
- The vault, settings and sign-ins stay out of Android's backup and device transfer: back up and move the vault with
  Safay's own backup.
- Unlocking with biometrics needs a strong biometric (Class 3) enrolled; after fingerprints or faces change, turn it on
  again in Settings.
- Safay locks when the screen goes off, and after the auto-lock time in the background. The unlocked app is kept out of
  screenshots and the recent apps' pictures.
- Exports open the share sheet, where you choose Files, a drive or another app. Dropbox, OneDrive and Google Drive sign
  in in a Custom Tab.
- A window 700 dp wide or more (a tablet, a foldable open) shows the categories in a sidebar and the accounts as cards.

## iOS and iPadOS details

- Built for the simulator (`make ios-simulator`) and checked there on iPhone 18 Pro and iPad Pro 11-inch (iOS 27.2):
  every page of `previews/mobile.test.tsx`, light and dark.
- On an iPhone (iOS 27, `make ios-run`): iCloud backup with silent pushes from other devices, Google Drive sign-in,
  Synology, scanning a QR code with the camera, and the copied-code message. Face ID and Touch ID, the clipboard's
  expiry and the cover in the background are coded and untried on a device.
- Sign-ins open the system's sign-in session. A callback scheme NSH's session refuses (Google's, longer than its
  `MAX_SCHEME_LEN`) goes to `ASWebAuthenticationSession` directly (`platform::web_auth`).
- The interface follows the system: its navigation bar, swipe back, menus, sheets, switches and segmented controls, and
  the alert for a category's name, are UIKit's. Dialogs with password fields are drawn in the look of an iOS alert,
  since the system's alert has no secure field.
- iPhone: portrait only. iPad: every orientation; a window 700 pt wide or more shows the categories in a sidebar and the
  accounts as cards, and sheets (an account's details, "Add to", a server's setup) open as a card in the middle of the
  screen; a narrower one (Split View, Slide Over) shows the phone's layout and sheets.
- Safay locks when the device locks with a passcode, and after the auto-lock time. While unlocked, a blurred cover hides
  it in the app switcher. A copied code stays on this device (never Universal Clipboard) and expires.
- The app icon is `assets/ios/icon.png`, edge to edge without transparency (`scripts/mobile_icons.py`).

## Linux details

- Checked on Arch Linux with Hyprland 0.56 (Omarchy) and GTK 4.24: the build, every test (`make ci`'s steps and
  `nsh test --target linux`), creating, locking and unlocking a vault, both ways of scanning a QR code, the tray, the
  global shortcuts, launching at login, and tiled and floating layouts. Other desktops are untested.
- Windows: there is no least size, since tiling compositors choose it; the layout narrows instead. The window buttons
  follow `gtk-decoration-layout`; where the compositor also draws title bars on floating windows (Omarchy),
  `gsettings set org.gnome.desktop.wm.preferences button-layout 'appmenu:'` removes GTK's.
- Scanning: "Select on Screen" takes the Screenshot portal's interactive screenshot, whose area the desktop's own tool
  picks (`slurp` on Hyprland). The camera goes through the Camera portal and PipeWire.
- Tray: a StatusNotifierItem, which Plasma, Waybar and Quickshell show (GNOME needs the AppIndicator extension). A click
  opens the quick panel; closing the main window keeps Safay in the tray, and "Start in Tray" starts it there, when the
  desktop shows a tray (otherwise closing quits). Wayland lets no app place a window: on Hyprland the panel opens at the
  focused monitor's top right and closes on a click outside it; elsewhere the compositor places it, and only Escape or
  another window taking the keyboard closes it.
- Global shortcuts: the GlobalShortcuts portal knows an app outside a sandbox by its desktop entry, which the runner
  writes (`~/.local/share/applications/<app ID>.desktop`; it also opens `safay://` links and sign-in callbacks). Plasma
  and GNOME 48+ bind the keys; Hyprland's portal binds none, so Safay binds them over its socket (again after a config
  reload, and off when it quits), leaving keys the user's config binds and lifting them while a shortcut is recorded.
  Defaults: Super+Alt+Z, X and A. Capturing follows macOS: select, import, then a desktop notification.
- Hyprland: the panel's placement, outside clicks and key binds use its socket's Lua requests (`eval`), which older
  releases without Lua configuration lack; there they do nothing.
- Notifications go to the desktop's notification service, silent, a newer one replacing the last; do-not-disturb hides
  them.
- Safay runs once: launching it again or opening a `safay://` link brings the running one forward, from the tray too.
  "Launch at Login" writes an XDG autostart entry (`~/.config/autostart/<app ID>.desktop`).
- Updates replace the folder the archive was unpacked to, where the user can write; installed elsewhere (a system
  location, a package manager's), Safay offers the new version as a download instead. "Check for Updates" is in the
  tray menu, and a new version opens the update window.
- Dropbox, OneDrive and Google Drive sign in in the default browser. The provider redirects to its callback scheme,
  which the desktop entry claims; the link starts Safay again, which hands it to the running one. Google Drive, when
  the build has a "Desktop app" client (`[google.desktop]` in `cloud-keys.toml`), redirects to a port of `127.0.0.1`
  that Safay listens on for the sign-in, and the browser shows a page saying the tab can be closed. Leaving the page
  in Safay ends the sign-in; the browser tab stays open.

## Known issues

- Android 16 and later: Back while the keyboard shows closes the page along with the keyboard.
- iOS: the system's sidebar of a split view is not used on iPad (NSH's `SplitView` ends the app there); Safay draws its
  own. Switches are on in the accent color rather than the system's green.
- Opening the menu bar panel with its shortcut before the menu bar icon has ever been clicked places it at the top right
  of the display the pointer is on, not under the icon: NSH reports the icon's position only on clicks.
- The menu bar panel and the desktop notice use flat colours rather than a blur material: NSH's window material has no
  rounded mask for transparent windows.

## Windows and Linux: next steps

1. Run the build and `nsh test --target windows` on a real Windows system, and Linux on GNOME and KDE; check the title
   bar areas and window chrome.
2. Windows: enable the desktop integration (NSH's tray, windows and global shortcuts work there), as Linux's
   `src/desktop_linux.rs` does, and turn on `supports::TRAY` and `GLOBAL_HOTKEYS`. Both: `SETTINGS_WINDOW`.
3. Store shortcuts in a platform-neutral form (today they are macOS key codes and Carbon modifiers); turn on launch
   at login on Windows (NSH's login item covers its `Run` key).
4. Windows Hello, only as a real key release through `KeyCredentialManager` or a TPM-backed key; never a yes/no check.
5. Verify lock events, the clipboard, file dialogs and backup folders under sync clients on each system.
6. Windows: update a release package end to end (its folder is replaced entry by entry; the restarted Safay waits for
   the old process, `platform::windows::wait_for_replaced`).
7. Sign in to Dropbox, OneDrive and Google Drive with Safay's client IDs on both systems; on Windows, the callback
   schemes are registered under `HKCU\Software\Classes` and a second launch hands its link over (`platform::windows`).
