# Changelog

All notable changes to Safay are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/). A release's section is also its update notes.

## [Unreleased]

### Changed

- **Select on Screen**: Safay's own selection replaces the system's screenshot crosshair. It finds the 2FA QR codes on
  every display and outlines each with its account's name, so one click adds it; dragging still selects any area, and
  Esc or Cancel cancels. Codes the vault already has are marked "Already in Safay".
- **Desktop notices** use a card with Safay's icon at the top of the screen.
- **Add Account** marks scanned accounts the vault already has.

### Fixed

- Capturing a QR code the vault already has from the shortcut or the menu bar now says it already exists instead of
  showing nothing.

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
