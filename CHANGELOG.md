# Changelog

All notable changes to Safay are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/). A release's section is also its update notes.

## [Unreleased]

### Changed

- **Update window**: new versions open in their own window with formatted release notes, "Skip This Version" and "Remind Me Later", and the download's progress.
- **Check for Updates**: the menu bar icon's menu can check for a new version, and tells you when Safay is up to date.
- **Sidebar**: "All" and your categories form one list, and the new-category button sits next to the sidebar toggle.

### Fixed

- The eye in password and secret fields is crossed out while the text is shown, so you can tell which state you are in.

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
