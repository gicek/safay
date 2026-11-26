# Development

## Toolchain

- **Rust 1.100.0**, pinned in `rust-toolchain.toml` (the minimum supported version is 1.97).
- **Python 3** for the scripts in `scripts/`.
- macOS: Xcode Command Line Tools. Linux: GTK 4, WebKitGTK 6, GStreamer and clang development packages. On Ubuntu
  24.04:

  ```sh
  sudo apt-get install libgtk-4-dev libwebkitgtk-6.0-dev libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
    gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-pipewire libdbus-1-dev clang libclang-dev
  # Native UI tests under a virtual display:
  sudo apt-get install xvfb xauth at-spi2-core libxtst6 dbus-x11
  ```

  On Arch Linux: `gtk4 webkitgtk-6.0 gstreamer gst-plugins-base gst-plugins-good gst-plugin-pipewire dbus clang`.
  Selecting a QR code on screen and the camera also need `xdg-desktop-portal` with a backend that has Screenshot (on
  Hyprland, `xdg-desktop-portal-hyprland` with `grim` and `slurp`); cloud backup needs a Secret Service (GNOME Keyring,
  KWallet).
- Node.js 22 or later, only for the Raycast extension (`integrations/raycast`).

Development builds optimize dependencies (including NSH and Argon2id) and leave Safay's own crates unoptimized; without
that, the UI backend and key derivation are an order of magnitude slower.

## NSH

Safay tracks NSH's `main` branch. Every `nsh-*` dependency is a Git dependency on `main`, and `Cargo.lock` pins the
commit. The NSH command-line tool must come from the same commit: `scripts/install_nsh.py` builds it from source into
`.nsh/toolchain/`, and `scripts/nsh.py` refuses to run a CLI built from another commit (`NSH_BIN` can point to one,
checked the same way). Make targets that need the CLI build it on first use; `make nsh-install` rebuilds it.

API questions are answered by the Rust sources of the pinned commit. To upgrade:

```sh
make nsh-update    # cargo update -p nsh-core, then rebuild the CLI
make all           # every check, test and the development package
```

Then review the [NSH workarounds](ARCHITECTURE.md#nsh-and-what-is-built-outside-it) — move back to NSH where it now
covers a gap — and recheck the platforms you can run.

NSH's agent skills (`nsh`, `nsh-design-guides`) are not vendored. Coding agents can install them with
`npx skills add longbridge/nsh` (into `.claude/skills/`, which is ignored).

## Commands

`make` lists every target. The main ones:

| Target | What it does |
|---|---|
| `make preview` | Run with an in-memory vault and credential store; nothing is written, discarded on quit |
| `make run` | Run with the real local vault (development app ID, so release data is untouched) |
| `make ci` | Formatting, clippy, `cargo test`, `nsh check`, headless and native UI tests — the CI sequence (CI runs it on macOS) |
| `make all` | `make ci`, then the development package |
| `make lint` | Clippy with warnings as errors; checks generated launcher files are current and that the app links NSH's `svg` feature |
| `make test` / `test-ui` / `test-native` | Rust tests; NSH headless UI tests; NSH tests on this host's native backend |
| `make screenshots` | Capture every page into `docs/screenshots` (macOS; takes the foreground, the screen must be unlocked) |
| `make package` | Development app bundle (macOS) |
| `make android` | Development APK (Android) |
| `make raycast` | Build and lint the Raycast/Tinycast extension |
| `make audit` | `cargo deny` advisories and licenses for this host's platform (CI checks all three platforms) |

The app must be started through its own launcher (`make run`, `make preview`, or a package). NSH's default runner does
not run `src/main.rs`: it shows a notice instead of opening a vault, and has no tray, settings window or global
shortcuts. The `previews/` entry points exist only in debug builds and keep everything in memory.

On Linux the app runs once per app ID, so `make run` and `make preview` cannot run side by side (the second brings the
first forward), and both put the icon beside the debug executable for the desktop entry the runner writes.

## Testing

Tests run in layers: pure logic and the golden vectors (`cargo test`), the application in NSH's headless engine
(`tests/ui.rs`, `App.test.tsx`), then native backends (`nsh test --target macos|windows|linux`). Permissions, hardware
authentication and signed distribution are checked by hand.

- Use public RFC vectors and synthetic data only. Never use a real vault, key or production data directory.
- Headless tests inject memory implementations of storage, keychain, network, clipboard and clock. NSH's own storage
  and network mocks do not intercept Safay's Rust I/O.
- `golden/` is the behaviour baseline. Do not regenerate it; when behaviour changes on purpose, change the
  implementation and the affected vectors in the same commit and say why.
- Cloud backends are tested against `cloudset`'s in-memory WebDAV server and OAuth stubs, never the network.
- `make preview` starts with an empty in-memory vault. The debug-only `previews/Demo*.tsx` entry points load a synthetic
  demo vault; `SAFAY_DEMO_APPEARANCE=light|dark` and `SAFAY_DEMO_SCENE=<page>` (see `src/demo.rs`) choose the
  appearance and the page to open.
- A development package started with `SAFAY_PREVIEW_DEMO=<accounts>` and `--preview` opens the demo vault, unlocked
  and in memory, with made-up accounts after its eight samples up to that number: for checking memory and scrolling
  with a large vault (`build/macos/debug/Safay.app/Contents/MacOS/Safay --preview`).

### Screenshots

`previews/screenshots.test.tsx` captures every page on the macOS backend from synthetic accounts into
`docs/screenshots` (not tracked):

```sh
python3 scripts/nsh.py test previews/screenshots.test.tsx --target macos --foreground
```

Check every image after a UI change. `--foreground` gives the window focus so selected rows and segments show their
active colors. Run it with the screen unlocked: where the window server gives no picture, NSH draws the window in
process, without its title bar buttons.

`python3 scripts/readme_images.py` (needs Pillow) copies the pictures the README shows into `docs/img`, with the
window's rounded corners at all four corners and transparency outside them.

## Packaging

Packages go to `build/` at the repository root (untracked), each system in a folder of its own (`macos`, `linux`,
`windows`, `android`, `ios-simulator`); `build/dist/` holds what a release publishes, for every system:

| Command | Output | What it is |
|---|---|---|
| `make package` | `build/macos/debug/Safay.app` | Development package (`com.gicek.safay.debug`) |
| `make package-release` | `build/macos/release/Safay.app`, `Safay-<version>-macos-<arch>.zip` | Developer ID release package, not notarized: for testing on this Mac |
| `make release` | `build/dist/` | Notarized app, zip and DMG, update archive and manifest: what is published |

Development packages for phones and tablets go there too. Both start in the phone interface (`src/mobile`); Android is
supported from Android 12, and the iOS one runs in the simulator (not yet on a device or in a store).

| Command | Output | Needs |
|---|---|---|
| `make android` | `build/android/debug/Safay.apk` (debug key, `arm64-v8a` and `armeabi-v7a`) | The Android SDK with `platforms;android-36`, build tools and an NDK; a JDK; `rustup target add aarch64-linux-android armv7-linux-androideabi` |
| `make android-run` | Installs it on the running emulator or the connected device, and starts it | `adb` |
| `python3 scripts/android.py --demo` | `build/android/demo/Safay Demo.apk` (`com.gicek.safay.demo`, "Safay Demo"): a package of its own, for trying a build on a device without touching the development package's vault | Same as `make android` |
| `python3 scripts/android_ui_test.py` | Installs the demo package and walks every page of the phone interface on a device or emulator, through its accessibility tree: onboarding, adding accounts, reordering by dragging, details, categories, search, settings and their dialogs, export, locking, an import of 150 accounts (made by `cargo run -p safay-core --example export_fixture`) whose list it scrolls and narrows by letter and erasing. It reports each failed check with a screenshot in `build/android/ui-test/`, and erases the demo package's data before and after | `adb` and the `android` command-line tool (cmdline-tools); one device online, or `--serial` |
| `make ios-simulator` | `build/ios-simulator/debug/Safay.app` | Xcode (`DEVELOPER_DIR` when `xcode-select` names the command line tools); `rustup target add aarch64-apple-ios-sim` |
| `make ios-simulator-run` | Builds it, then installs and starts it in the booted simulator | An iOS simulator runtime |
| `python3 scripts/nsh.py test previews/mobile.test.tsx --target ios` | Walks every page of the phone and tablet interface over the demo vault in the simulator (`--device "iPad Pro 11-inch (M5)"` for a tablet; `--target android` on a device or emulator) and saves a picture of each in `build/screenshots/mobile/` | Same as `make ios-simulator`; the test answers system alerts itself, so they show in no picture |

The phone and tablet icons come from `assets/icon.png`: `python3 scripts/mobile_icons.py` (needs Pillow) writes
Android's adaptive layers (`assets/android/`) and the iOS icon, edge to edge (`assets/ios/icon.png`); run it again after
the icon changes.

A desktop's development build can show the phone interface: `SAFAY_MOBILE=1 SAFAY_PREVIEW_DEMO=8 cargo run -- --preview`
(`SAFAY_MOBILE=1180x820` for a tablet's window), in iOS's look with `cargo run --features ios-look`.
`SAFAY_DEMO_PAGE=<page>` opens the demo on one of its pages, sheets, menus or alerts (`mobile::demo_page` lists them);
in the simulator, pass it to `nsh run --target ios previews/DemoMobile.tsx` as `SIMCTL_CHILD_SAFAY_DEMO_PAGE`.

The Android package carries Java classes of its own (`android/java`, compiled by `make android` into the dex that
`nsh.json` lists): Android's APIs that take a class of the app, such as the clipboard entry marked sensitive and the
lock when the screen goes off, and the camera, whose preview is a native component of the app's crate
(`safay.camera`). Rust reaches them through JNI (`src/platform/android.rs`, `src/platform/camera_android.rs`).

`make android` runs `scripts/android.py`, which packages from `build/android/app/nsh.json`: the project's `nsh.json`
with the sign-in callback schemes of the local `cloud-keys.toml` added, since Android hands a callback link only to an
app whose manifest claims its scheme, and the adaptive launcher icon in `assets/android/` in place of the app icon.
Development and demo packages allow screenshots of the unlocked app (`SAFAY_ALLOW_SCREENSHOTS`); release packages keep
it out of screenshots and the recent apps' pictures. Release packages (`--release`) are built for size: NSH builds them
in a workspace of its own without a release profile, so `scripts/android.py` sets one through `CARGO_PROFILE_RELEASE_*`
(`opt-level = "s"`, fat LTO, one codegen unit), while `.cargo/config.toml` keeps the cryptography at `opt-level = 3`,
since Argon2 runs on every unlock.

On Android, Safay keeps everything in the directory the system's backup and device transfer skip
(`getNoBackupFilesDir()`, `platform::android::storage_directory`), and moves files an earlier build left in NSH's data
directory there on start. Cloud credentials are files in it, sealed with an AES-GCM key kept in the hardware key store
(`android/java/.../Secrets.java`). HTTPS certificates are checked by rustls against the system's CA store
(`platform::tls::system_verifier`): the platform verifier there reports a certificate without an OCSP responder as
revoked.

Release builds use the app ID `com.gicek.safay`; development builds use `com.gicek.safay.debug`, with their own data
directory, keychain items and provisioning profile. `python3 scripts/package.py --out <folder>` writes elsewhere. On
Linux and Windows it writes, in `build/linux/` or `build/windows/`, the `Safay/` folder (the executable at its top)
and its archive, `Safay-<version>-linux-<arch>.tar.gz` or `Safay-<version>-windows-<arch>.zip`, which is also the
update archive.

### Local settings

Machine-specific settings live in an untracked `local.toml` at the repository root; environment variables of the same
meaning override it.

| `local.toml` | Environment | Purpose |
|---|---|---|
| `[signing] identity` | `SAFAY_SIGN_IDENTITY` | Developer ID Application certificate (release packages) |
| `[signing] development_identity` | `SAFAY_DEVELOPMENT_IDENTITY` | Development certificate (default "Apple Development"; `-` for ad-hoc) |
| `[signing] team` | `SAFAY_TEAM_ID` | Team ID, to find provisioning profiles |
| `[release] notary_profile` | `SAFAY_NOTARY_PROFILE` | `xcrun notarytool store-credentials` profile |
| `[release] update_key` | `SAFAY_UPDATE_KEY` | Path to the update-signing secret key (kept outside the repository) |
| `[release] update_url` | `SAFAY_UPDATE_URL` | Release packages' update manifest (`latest.json`, HTTPS); empty: no self-update |
| `[release] download_url_prefix` | `SAFAY_DOWNLOAD_URL_PREFIX` | Where update archives are hosted; empty: next to the manifest |
| `[android] keystore`, `key_alias` | `SAFAY_ANDROID_KEYSTORE`, `SAFAY_ANDROID_KEY_ALIAS` | The Android app-signing key (kept outside the repository): signs the APKs; Play App Signing holds the same key |
| `[android] upload_keystore`, `upload_key_alias` | `SAFAY_ANDROID_UPLOAD_KEYSTORE`, `SAFAY_ANDROID_UPLOAD_KEY_ALIAS` | The Google Play upload key: signs the App Bundle |
| `[android] keychain`, `upload_keychain` | `SAFAY_ANDROID_KEYCHAIN`, `SAFAY_ANDROID_UPLOAD_KEYCHAIN` | Service names of the login keychain's generic passwords that hold the two keystores' passwords |

Dropbox, OneDrive and Google Drive need public OAuth clients registered for Safay. Everything about them lives in
an untracked `cloud-keys.toml`, copied from `cloud-keys.example.toml`: each service's client ID, the redirect URI
registered for it, and the name of the app folder on Google Drive. `build.rs` compiles the values in; nothing about
a service is written in the code. A service without a client ID is hidden, and a client ID without the values that go
with it stops the build. The redirect URIs are: Dropbox `db-<app key>://2/token`, Microsoft a custom scheme of your
choice (a "Mobile and desktop applications" platform), Google `com.googleusercontent.apps.<id>:/oauth2redirect` (an
iOS client).

Linux and Windows sign in to Google Drive with the client under `[google.desktop]` when there is one: a "Desktop app"
client with its client secret and the loopback redirect `http://127.0.0.1`, to which Safay adds the port it listens
on. Without it they use the `[google]` client like macOS.

### Signing and iCloud

- `make package`: development package signed with "Apple Development" and the development provisioning profile; iCloud
  uses the CloudKit Development environment. Without a certificate or profile the bundle is ad-hoc signed and has no
  iCloud (`make package-adhoc` forces this).
- `make package-release`: Developer ID, Hardened Runtime, secure timestamp and the Developer ID profile; iCloud
  Production.
- iCloud and push are restricted entitlements: the app ID must have the container `iCloud.com.gicek.safay` and Push
  Notifications enabled, with matching profiles in `~/Library/Developer/Xcode/UserData/Provisioning Profiles`. The
  script picks the profile by app ID, type and expiry, and adds the push entitlement only when the profile has it.
- `Safay.app/Contents/MacOS/Safay --icloud-check` prints the CloudKit account status of a signed package without
  opening the vault.

### Releases and updates

`make release` (`scripts/release.py`) runs the tests, builds the release package, notarizes and staples the app, and
writes into `build/dist/`:

- `Safay-<version>.dmg`: signed and notarized, for first installs;
- `Safay-<version>-macos-<arch>.zip`: the stapled app;
- `Safay-<version>-macos-<arch>.tar.gz`: the update archive;
- `latest.json` and `latest.json.sig`: with an update source configured, the signed manifest with the version's
  update notes (below);
- `Safay-<version>-macos.env`: version, build and SHA-256 of each file.

On Linux and Windows, `python3 scripts/release.py` runs the tests and writes that system's release archive and
`Safay-<version>-<os>.env` into `build/dist/`; a release removes only its own system's earlier outputs. The manifest
lists every platform's archive of the version found in `build/dist/`: copy the Linux and Windows archives there on the
Mac before `make release` (or run `--manifest-only` afterwards), since the update-signing key stays on one machine. A
system without the key writes no manifest and says so.

The update notes follow the app's language. Each version's notes are its `CHANGELOG.md` section, translated item for
item in `locales/release-notes/zh-Hans.md` and `zh-Hant.md` under the same `## [<version>]` heading; `make release`
stops when one of them is missing. The manifest carries the English section, then each translation after a
`<!-- lang: <code> -->` line, all under its signature; the update window renders the part in the app's language, and
English where a translation is missing. `python3 scripts/release.py --notes` prints them as the manifest will carry
them.

`make release-android` (`release.py --android`, on the Mac) runs the tests and writes the Android packages into
`build/dist/`: `Safay-<version>-android-arm64-v8a.apk` and `-armeabi-v7a.apk`, signed with the app-signing key, for
GitHub Releases; `Safay-<version>-android.aab`, signed with the upload key, for Google Play; and
`Safay-<version>-android.env`. It checks each package's certificate against its key, and that the APKs carry the
release app ID, the version, target API 36 and one ABI each, and are not debuggable. Android packages update through
Google Play or a newer APK, not through the update manifest; `versionCode` is `nsh.json`'s `build`, which every release
raises. Losing the app-signing key means the APKs can no longer update installed copies; keep a backup of it and of its
password.

`python3 scripts/release.py --manifest-only` writes the manifest again for the update archives already in `build/dist`,
without building or notarizing: after the version's notes changed, or another system's archive was added.

To see a version's notes in the update window before releasing, start a development package with
`SAFAY_PREVIEW_UPDATE=<version> SAFAY_PREVIEW_NOTES=<file>`, the file holding the output of `release.py --notes`
(`build/macos/debug/Safay.app/Contents/MacOS/Safay --preview`). Switch the language in Settings to see each translation. Its
"Install Update" simulates a download, so the progress and "Install and Relaunch" can be seen too.

Nothing is uploaded. Releases are published on GitHub Releases: upload the DMG, the zip, every platform's update
archive, the Android APKs, `latest.json` and `latest.json.sig` to the release; the App Bundle goes to Google Play. Release packages read their update manifest from
`https://github.com/gicek/safay/releases/latest/download/latest.json`, so the newest release is always the update.

Self-update is signed with Safay's Ed25519 update key. Its public key is `UPDATE_PUBLIC_KEY` in `scripts/package.py`;
the secret stays outside the repository, and `local.toml` names its path. `make release` refuses to run when the secret
does not match that public key. Losing the secret key means installed copies can no longer verify updates, so keep a
backup of it. `make update-keygen KEY=<path outside the repository>` creates a key pair.

Development packages are a separate app (`<id>.debug`) and do not update from the release source. To test updating end
to end, package one with `SAFAY_UPDATE_URL` pointing at a local update source and `SAFAY_UPDATE_PUBLIC_KEY` set to a
throwaway key's public key, then run `Safay --update-check`: it checks, downloads and replaces without the UI. On Linux
and Windows the package is the folder `scripts/package.py` writes; the new version's archive and its manifest
(`python3 scripts/nsh.py update-manifest`) can be served from this machine over plain HTTP, which debug builds accept
for loopback only.

## Launcher integrations

`python3 scripts/integrations.py` regenerates the Raycast extension's text and bridge script from `locales/` and
`integrations/shared/`; `make lint` fails when they are stale. `make raycast` builds and lints the extension;
`cd integrations/raycast && npm run dev` loads your copy into Raycast in place of the store version. The Alfred
workflow is built into every package. Updating the store extension is described in
[integrations/README.md](../integrations/README.md#updating-the-raycast-store-extension).

## Website

`site/` is the website at safay.gicek.com: a home page and a privacy policy, each in English, Simplified Chinese and
Traditional Chinese (`/`, `/zh-hans/`, `/zh-hant/`). Pages are templates in `site/src/`, copy is in
`site/i18n/<lang>.json`, and `site/build.py` (standard library only) renders them into `build/site/dist/`, taking the
version from `Cargo.toml` and the brand marks and the code font from `assets/`. Every outbound link is in
`site/links.toml`; the download button points at `releases/latest/download/Safay-<version>.dmg`, so redeploy the site
after each release. Copy speaks of "your device", not of one system; the download section lists the systems, with
those not published yet marked "Coming soon".

- `make site` builds it and `make site-serve` serves it at `http://localhost:8787`.
- `make site-shots` refreshes `site/shots/*.webp` and the logo sizes from `docs/img/` and `docs/screenshots/` (needs
  Pillow). Commit the results; the site build never runs the app.
- `make site-deploy` builds the site and runs the pinned wrangler from `build/site/`. The first run opens a browser to
  sign in to Cloudflare. The `gicek.com` zone must be on that account; wrangler then binds the custom domain
  (`[[routes]]` in `site/wrangler.toml`) and creates its DNS record. Headers, including the Content Security Policy,
  are in `site/src/_headers`.

The privacy policy is what the cloud services' app reviews read: when Safay starts to send, store or read something
it did not before, change `site/i18n/*.json` in the same change and redeploy.

## Dependency audit

```sh
cargo metadata --format-version 1 --filter-platform aarch64-apple-darwin --locked > /tmp/safay-metadata.json
cargo deny --metadata-path /tmp/safay-metadata.json --target aarch64-apple-darwin --locked check advisories licenses
```

Run it per target triple; one platform's result does not cover the others.
