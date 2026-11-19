# Platforms

macOS is the primary platform. Windows and Linux share every page and the whole core, but CI does not build them yet.
Windows has not been run end to end; Linux has, on one system (see [Linux details](#linux-details)). The parts below
marked *coded* are written and untested on a real system. The interface shows a capability only where
`platform::supports` (in `src/platform/mod.rs`) says the platform has it.

## Capabilities

| Capability | macOS | Windows | Linux |
|---|---|---|---|
| Vault, accounts, categories, codes, import and export | Yes | Coded | Coded |
| Folder backup | Yes | Coded | Coded |
| WebDAV and Nextcloud backup | Yes | Coded (Credential Manager) | Not yet (needs a Secret Service credential store) |
| Dropbox, OneDrive, Google Drive | Yes | Not yet (OAuth sign-in) | Not yet |
| iCloud | Yes | — | — |
| Touch ID / hardware-protected unlock | Touch ID | Planned: Windows Hello key release | Not planned (no common hardware key API) |
| Lock on sleep, screen lock, user switch | Yes | Coded (`WM_WTSSESSION_CHANGE`, power broadcasts) | Coded (logind, screensaver) |
| Auto-lock on app input | Yes | Partial (input inside the page only) | Partial |
| Secret clipboard | Yes | Coded (excluded from history and cloud clipboard) | Coded (password-manager hint, no PRIMARY) |
| Menu bar / tray panel, global shortcuts, settings window | Yes | Not yet wired (NSH supports them) | Not yet wired (shortcuts via the GlobalShortcuts portal) |
| Launch at login, background launch | Yes | Not yet | Not yet |
| Select a QR code on screen; camera | Yes | Not yet | Yes (the Screenshot and Camera portals) |
| Launchers (Alfred, Raycast, Tinycast) | Yes | — | — |
| App updates | Yes (signed packages with an update source) | Not yet | Not yet |

On Windows and Linux, Settings opens as a dialog over the main window; macOS uses its own window.

## macOS details

- Requires macOS 14 or later; tested on Apple silicon with macOS 26 and 27. The sidebar follows the running release:
  a floating sidebar on macOS 26, a flush sidebar on 15 and earlier and on 27 and later.
- iCloud and push notifications need a package signed with a provisioning profile for the app ID
  ([Development](DEVELOPMENT.md#signing-and-icloud)); ad-hoc packages show iCloud as unavailable.
- Dropbox, OneDrive and Google Drive appear only when the build has OAuth client IDs (`cloud-keys.toml`).
- Cloud locations use the provider's app folder (`Safay`); choosing an arbitrary remote folder is not implemented.
- Selecting a QR code on screen needs Screen Recording permission; the camera needs a packaged app.

## Linux details

- Checked on Arch Linux with Hyprland 0.56 (Omarchy) and GTK 4.24: the build, every test (`make ci`'s steps and
  `nsh test --target linux`), first launch, creating, locking and unlocking a vault, adding accounts by selecting a QR
  code on screen and with the camera, and the window in tiled and floating layouts. Other desktops are untested.
- Selecting a QR code on screen uses the desktop portal's interactive screenshot: the desktop's own tool picks the area
  (`slurp` on Hyprland, the screenshot tool on GNOME and KDE). Without a Screenshot portal, Safay says it can't take a
  screenshot.
- The camera goes through the Camera portal, which may ask for permission first, and PipeWire.
- There is no least window size: tiling compositors give a window the tile they choose, so the layout narrows instead.
- The window buttons follow the desktop's `gtk-decoration-layout`. On a tiling desktop that also draws title bars on
  floating windows (Omarchy), a floating Safay shows two sets; `gsettings set org.gnome.desktop.wm.preferences
  button-layout 'appmenu:'` removes GTK's.

## Known issues

- With the Settings window over the main window, dragging in the Settings window's title-bar strip occasionally moves
  the main window. NSH chooses the window to drag from shared input state instead of the event's own window
  (`chrome::mouse_down`); a double click there may act on the main window too. Fix expected upstream.
- Opening the menu bar panel with its shortcut before the menu bar icon has ever been clicked places it at the top right
  of the main display: NSH reports the icon's position only on clicks.
- The menu bar panel and the desktop notice use flat colours rather than a blur material: NSH's window material has no
  rounded mask for transparent windows.

## Windows and Linux: next steps

1. Run the build and `nsh test --target windows` on a real Windows system, and Linux on GNOME and KDE; check the title
   bar areas and window chrome.
2. Enable `src/desktop.rs` on both platforms (NSH's tray, windows and global shortcuts work there), dispatch the Dock
   and pointer calls per platform, and turn on `supports::TRAY`, `SETTINGS_WINDOW` and `GLOBAL_HOTKEYS`.
3. Store shortcuts in a platform-neutral form (today they are macOS key codes and Carbon modifiers); turn on launch
   at login (NSH's login item covers the Windows `Run` key and Linux autostart).
4. Windows Hello, only as a real key release through `KeyCredentialManager` or a TPM-backed key; never a yes/no check.
5. Verify lock events, the clipboard, file dialogs and backup folders under sync clients on each system.
