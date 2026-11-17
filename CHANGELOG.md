# Changelog

All notable changes to Safay are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/). A release's section is also its update notes.

## [Unreleased]

### Added

- **Import from other apps**: bring your codes over from Aegis, 2FAS, Ente Auth, Proton Authenticator, and Bitwarden, encrypted backups included.
- **Import from password managers**: choose a CSV export from Apple Passwords, Bitwarden, or another password manager to import its codes.
- **Groups and favorites**: groups, tags, and folders become categories, and favorites go into "Pinned".
- **Steam Guard and Battle.net**: pick the account type under "Advanced Options", or add these accounts from a link, QR code, or import.

### Changed

- **Import results**: entries Safay can't use, such as HOTP codes, are skipped and counted instead of failing the import.

## [1.1.3] - 2026-11-17

Touch ID no longer flashes the password dialog, and QR codes get simpler.

### Changed

- **Verify It’s You**: a narrower dialog that simply asks for your master password.
- **QR codes**: links leave out default settings, so codes are easier to scan.
- **Empty categories**: an empty category or search shows a simple icon and title.

### Fixed

- After Touch ID verifies you, the master password dialog no longer flashes.
- Turning off "Delay After Failed Attempts" with your master password closes the dialog.
- After "Use Password…" in the Touch ID prompt, the cursor is in the password field.

## [1.1.2] - 2026-11-17

The menu bar panel is ready for typing as soon as it opens.

### Fixed

- The menu bar panel's search field is ready for typing whenever the panel opens, also right after you unlock it there.
- Escape closes the menu bar panel at once, and the main window's search field keeps its focus ring after you use the panel.

## [1.1.1] - 2026-11-17

Deleted accounts wait in Recently Deleted for 30 days, so you can bring them back.

### Added

- **Recently Deleted**: deleted accounts wait in the sidebar's "Recently Deleted" for 30 days, where you can recover them or delete them for good.

### Changed

- **Settings**: clearer names and descriptions, "Start in Menu Bar" next to the menu bar icon, and a 15-minute auto-lock.

### Fixed

- Recording a new keyboard shortcut in Settings no longer quits Safay.

## [1.1.0] - 2026-11-17

Safay takes on the indigo of its icon, and new versions open in an update window with their notes.

### Changed

- **Update window**: each new version shows its notes and the download's progress, and you can skip it.
- **Check for Updates**: now also in the menu bar icon's menu.
- **New look**: an indigo accent taken from the Safay icon, in light and dark mode.
- **Sidebar**: "All" and your categories form one list; with no accounts yet, arrows show where to start.
- **Notices**: confirmations appear in the window's bottom right corner.

### Fixed

- The eye in password fields is crossed out while the text is shown.

## [1.0.0] - 2026-11-17

Safay now finds 2FA QR codes on your screen for you, even while it is locked.

### Changed

- **Select on Screen**: Safay outlines every 2FA QR code on your displays; click one to add it, or drag over any area.
- **Capture while locked**: the capture shortcut reads the QR code first, then unlocks with Touch ID and adds it.
- **Already in Safay**: codes and scanned accounts you have already added are marked as such.
- **Desktop notices**: results show in a card at the top of the screen.

### Fixed

- Capturing a QR code that is already in Safay from the shortcut or the menu bar now says so.

## [0.1.0] - 2026-11-16

The first release, for macOS 14 or later on Apple silicon.

- **Encrypted local vault**: TOTP accounts are encrypted with Argon2id and AES-256-GCM and never leave the device
  unencrypted. Auto-lock, locking on sleep and screen lock, and delays after failed attempts protect it; Touch ID
  unlocks it with a key held by the Secure Enclave.
- **Re-verification**: showing a secret or QR code, exporting and copying from a launcher ask for Touch ID or the
  master password again.
- **Adding accounts**: enter a key, paste `otpauth://` links or Google Authenticator migration codes, drop QR images,
  select a QR code on screen, or scan one with the camera.
- **Codes and organizing**: SHA-1/256/512, 6 or 8 digits, 30 or 60 seconds; copy with one click; cards or a list;
  search, categories and drag to reorder.
- **Backup and sync**: the encrypted vault to a folder, iCloud, WebDAV or Nextcloud. Conflicts are compared and
  resolved, never silently overwritten.
- **Import and export**: encrypted Safay exports and Authenticator browser extension backups.
- **macOS integration**: a menu bar panel, global shortcuts, launch at login, and Alfred, Raycast and Tinycast
  launchers that find an account and copy its code.
- **Signed updates**: Safay checks for new versions after you agree, and verifies each update's signature before it
  installs.
- English, Simplified Chinese and Traditional Chinese; light and dark appearance.
