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
  with a large vault (`build/debug/Safay.app/Contents/MacOS/Safay --preview`).

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

## Packaging (macOS)

Packages go to `build/` at the repository root (untracked):

| Command | Output | What it is |
|---|---|---|
| `make package` | `build/debug/Safay.app` | Development package (`com.gicek.safay.debug`) |
| `make package-release` | `build/release/Safay.app`, `Safay-<version>-macos-<arch>.zip` | Developer ID release package, not notarized: for testing on this Mac |
| `make release` | `build/dist/` | Notarized app, zip and DMG, update archive and manifest: what is published |

Release builds use the app ID `com.gicek.safay`; development builds use `com.gicek.safay.debug`, with their own data
directory, keychain items and provisioning profile. `python3 scripts/package.py --out <folder>` writes elsewhere.

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

Dropbox, OneDrive and Google Drive need public OAuth client IDs registered for Safay, in an untracked
`cloud-keys.toml` (`[dropbox] app_key`, `[microsoft] client_id`, `[google] client_id`). `build.rs` compiles them in;
services without one are hidden. Every system uses the same redirect URIs, which each client registers: Dropbox
`db-<app key>://2/token`, Microsoft `msal<client id>://auth` (a "Mobile and desktop applications" platform), Google
`com.googleusercontent.apps.<id>:/oauth2redirect` (an iOS client).

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
- `Safay-<version>.env`: version, build and SHA-256 of each file.

The update notes follow the app's language. Each version's notes are its `CHANGELOG.md` section, translated item for
item in `locales/release-notes/zh-Hans.md` and `zh-Hant.md` under the same `## [<version>]` heading; `make release`
stops when one of them is missing. The manifest carries the English section, then each translation after a
`<!-- lang: <code> -->` line, all under its signature; the update window renders the part in the app's language, and
English where a translation is missing. `python3 scripts/release.py --notes` prints them as the manifest will carry
them.

`python3 scripts/release.py --manifest-only` writes the manifest again for the update archive already in `build/dist`,
without building or notarizing, after the version's notes changed.

To see a version's notes in the update window before releasing, start a development package with
`SAFAY_PREVIEW_UPDATE=<version> SAFAY_PREVIEW_NOTES=<file>`, the file holding the output of `release.py --notes`
(`build/debug/Safay.app/Contents/MacOS/Safay --preview`). Switch the language in Settings to see each translation. Its
"Install Update" simulates a download, so the progress and "Install and Relaunch" can be seen too.

Nothing is uploaded. Releases are published on GitHub Releases: upload the DMG, the zip, the update archive,
`latest.json` and `latest.json.sig` to the release. Release packages read their update manifest from
`https://github.com/gicek/safay/releases/latest/download/latest.json`, so the newest release is always the update.

Self-update is signed with Safay's Ed25519 update key. Its public key is `UPDATE_PUBLIC_KEY` in `scripts/package.py`;
the secret stays outside the repository, and `local.toml` names its path. `make release` refuses to run when the secret
does not match that public key. Losing the secret key means installed copies can no longer verify updates, so keep a
backup of it. `make update-keygen KEY=<path outside the repository>` creates a key pair.

Development packages are a separate app (`<id>.debug`) and do not update from the release source. To test updating end
to end, package one with `SAFAY_UPDATE_URL` pointing at a local update source and `SAFAY_UPDATE_PUBLIC_KEY` set to a
throwaway key's public key, then run `Safay --update-check`: it checks, downloads and replaces without the UI.

## Launcher integrations

`python3 scripts/integrations.py` regenerates the Raycast extension's text and bridge script from `locales/` and
`integrations/shared/`; `make lint` fails when they are stale. `make raycast` builds and lints the extension;
`cd integrations/raycast && npm run dev` loads it into Raycast. The Alfred workflow is built into every package. See
[integrations/README.md](../integrations/README.md).

## Dependency audit

```sh
cargo metadata --format-version 1 --filter-platform aarch64-apple-darwin --locked > /tmp/safay-metadata.json
cargo deny --metadata-path /tmp/safay-metadata.json --target aarch64-apple-darwin --locked check advisories licenses
```

Run it per target triple; one platform's result does not cover the others.
