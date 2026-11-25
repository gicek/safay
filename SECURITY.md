# Security

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub's **private vulnerability reporting** for this repository
(Security → Report a vulnerability). Do not open a public issue. Include the affected version or commit, the platform,
and steps to reproduce. Please allow time for a fix to be released before publishing details.

## Threat model

**In scope**: a lost device, or someone briefly using an unlocked computer; other local processes — including apps the
user has allowed to automate Safay — trying to read codes while Safay is unlocked; corrupt or malicious files and server
responses causing data loss or resource exhaustion; backup locations and cloud services seeing plaintext; offline
brute force against a stolen vault or backup.

**Out of scope**: an attacker who can already run code as the user and stay resident, and a compromised operating
system. Touch ID is a convenience on the local device; the master password remains the root of trust.

**Known limits**: ciphertext kept in a backup provider's version history, or in local snapshots, still opens with the
master password it was written with after the password changes. Memory is wiped for the buffers Safay owns; the UI
framework's text and image buffers and the system clipboard cannot be guaranteed to be wiped.

## Vault

- The vault is a single encrypted file. Plaintext exists only in memory and is encrypted before it is written.
- Format v2: AES-256-GCM with a key derived from the master password by Argon2id (time 3, memory 64 MiB, parallelism 1,
  32-byte key), a random 32-byte salt and a 12-byte nonce per write. The envelope holds only the version, algorithm
  names, KDF parameters, salt, nonce, timestamps and ciphertext.
- Only v2 with Argon2id is accepted. KDF parameters read from a file are checked against upper limits (time 10, memory
  256 MiB, parallelism 4) before any work, so a crafted file cannot run for hours or exhaust memory. Files are read up
  to 16 MiB; images up to 4096 × 4096 pixels.
- Imports from other apps decrypt in memory with the password typed for that file and never store it. Their KDF
  parameters are checked before any work too: Aegis scrypt up to N·r = 2^21 and N·r·p = 2^22, Ente Auth Argon2id up to
  1 GiB of memory, 64 passes and 4 GiB·passes in total; 2FAS (PBKDF2, 10,000 iterations) and Proton Authenticator
  (Argon2id, 19 MiB) use fixed parameters. CSV exports also hold passwords: only the one-time password, name, username,
  website, folder and favorite columns are read, and each row's fields are wiped once it is read.
- Writes go to a temporary file in the same directory, are synced and atomically renamed, and compare-and-swap against
  the file last read, so a change made outside the session is detected. The previous authenticated vault is kept as a
  last-good copy that can be restored. A lock file keeps two Safay processes from writing at once. On Unix the data
  directory is `0700` and its files `0600`.
- TOTP secrets, account names, websites, site icons and categories live only inside the encrypted payload.
- A deleted account stays in the encrypted vault, secret included, for 30 days in Recently Deleted, so it can be
  recovered; then, or at once when the user deletes it from there, it is removed and its secret wiped from memory.
  Backups written meanwhile still hold it, encrypted. The
  preferences file holds non-secret settings only; the failed-attempt file holds only a count.

## Master password and unlocking

- Master and export passwords: 8–128 printable ASCII characters (no spaces) and at least *Fair* by zxcvbn, with the
  product name counted as a user-related word. *Fair* asks for confirmation. The master password also protects backups,
  which can be attacked offline, where no attempt delay applies.
- From the fifth consecutive failure, the next attempt waits 30 seconds, doubling with each further failure up to 300. The wait is measured on a
  monotonic clock that keeps counting through sleep and restarts in full after a relaunch. Turning the delay off
  requires the master password.
- Auto-lock after 1, 5, 15 or 30 minutes without input to Safay (no "never"); locking also happens on sleep, screen lock,
  screensaver and user switching. Locking advances an epoch first: every window hides sensitive content at once, and a
  key derivation or file operation that finishes later cannot revive the session.
- Showing a secret or a QR code, exporting, changing the password, resetting, turning off the attempt delay, and
  copying a code for a launcher all verify the user again. The system login password is never accepted instead of the
  master password: it proves nothing about this vault.

### Touch ID (macOS)

A non-exportable P-256 key in the Secure Enclave (`privateKeyUsage | biometryCurrentSet`) wraps the 32-byte vault key
with ECIES (X9.63 SHA-256 KDF, AES-GCM). `biometric-key.json` (`0600`) holds only the enclave key handle and the
ciphertext; it is useless on another Mac and becomes permanently invalid when fingerprints change. Touch ID releases the
key itself: unlocking still has to open the vault with it, and re-verification requires the released key to equal the
session key byte for byte. Changing the master password or restoring from a backup deletes the wrapped key; it is
wrapped again at the next unlock. An invalid wrapped key is deleted and never counts as a failed attempt.

## Clipboard

Copied codes are marked as concealed and transient so clipboard managers that honour the convention do not record them
(Windows: excluded from clipboard history and cloud clipboard; Linux: the KDE password-manager hint, never the PRIMARY
selection). When a code's period ends, Safay clears the clipboard only if it still holds what Safay wrote. These markers
cannot bind every third-party clipboard tool.

## Network

Safay has no server, account system or telemetry. It makes exactly four kinds of request; everything else works
offline.

1. **Backup locations the user enables**: WebDAV, Nextcloud, Synology, Dropbox, OneDrive and Google Drive. Requests are
   built only by the core (`cloudset`, a sans-IO client) and sent as built by `src/platform/http.rs`: no redirects
   followed, no cookies, no caching, response bodies read up to the core's limit, system certificate verification,
   fixed timeouts. URLs, headers and bodies are never logged.
2. **Site icons** (optional, on by default per device): fetched only when an account is added or its website changes,
   only from that account's own website, never through a third-party icon service. HTTPS only, public DNS names only,
   at most 3 redirects each re-checked, HTML read up to 256 KiB, images up to 512 KiB and 32–1024 px per side. SVG is
   never accepted. Images are decoded, re-encoded as PNG of at most 96 × 96 px, validated by the core and stored only
   in the encrypted vault.
3. **iCloud** (macOS), through the system's CloudKit.
4. **App updates**, only in signed packages built with an update source.

Remote storage receives only the encrypted vault and a metadata file with a device name and a timestamp.

### Cloud accounts

- Dropbox, OneDrive and Google Drive use OAuth 2.0 with PKCE as a public client and ask for access to Safay's app
  folder, where the backup is kept: on macOS in the system's authentication session (`ASWebAuthenticationSession`), on
  Linux and Windows in the default browser, whose callback comes back as a link of the provider's scheme that Safay
  claims. Any app can open such a link, or claim the scheme too: only a sign-in in progress takes one, the code is
  useless without the PKCE verifier that never leaves Safay, and `state` and the callback are verified by the core.
  Google Drive on Linux and Windows uses Google's "Desktop app" client instead, whose callback comes back to a loopback
  address: Safay listens on a free port of `127.0.0.1` only while a sign-in is in progress, reads nothing but the
  request line, answers with one fixed page, and takes only the request the core accepts for that sign-in. The client
  secret Google issues to such clients ships inside the app and protects nothing; PKCE does. Nextcloud uses Login Flow
  v2, with the login page, polling and result required to share an origin.
- Credentials (the serialized connection, including tokens or app passwords) are stored only in the system credential
  store (Keychain, Windows Credential Manager, the Secret Service on Linux); `sync.json` holds the entry's ID and a
  display name. Turning a backup off revokes the token where the provider supports it and deletes the entry.
- A WebDAV, Nextcloud or Synology connection stopped midway (the user leaves it, or Safay locks) keeps nothing: a
  sign-in that arrives afterwards is revoked and its entry deleted.
- Synology signs in to DSM with the user's password, and the code from an authenticator app when the account uses
  two-step verification; DSM then trusts the device. Until the user picks the backup folder, the sign-in is held in
  memory only; choosing another service or location, going back to the sign-in form, or locking signs it out, and
  nothing of it is written anywhere. The stored connection keeps the password and that device token,
  because DSM sessions expire and are renewed without the user. With a QuickConnect ID, Safay asks Synology's
  QuickConnect servers (`quickconnect.to`, or `quickconnect.cn` for mainland China) for the NAS's addresses, sending
  only the ID, and probes them without credentials; when none answers, it opens Synology's relay. A probe only tells
  addresses apart: the password is protected by TLS with a certificate the system trusts, and an address found this
  way is never pinned. When the stored address stops answering, a sync looks the ID up again.
- Plain `http://` WebDAV and Synology addresses require explicit consent in the interface.
- **Self-signed servers** (WebDAV, Nextcloud, Synology): when the system does not trust a server's certificate, Safay
  shows the leaf certificate's SHA-256 fingerprint for the user to compare. Once confirmed, the location accepts
  either a certificate the system trusts or exactly that leaf certificate (constant-time comparison within the same
  handshake that carries the request). A different certificate is reported as changed, with no one-click trust;
  changing the address discards the pin. Pinned locations always show "Certificate not verified".

### Backup and sync

- Sync decisions are made by the core and trust only the local key. A remote copy the local key cannot open is never
  uploaded over, downloaded or merged; it can only be restored explicitly with that backup's master password, after it
  opens in memory. Linking a location first checks it read-only and refuses to overwrite another vault's backup.
- Conflicts show both sides and let the user merge, keep the local vault or use the backup; nothing is overwritten
  silently. Remote work runs outside the session lock, and results arriving after a lock are discarded.
- Uploads that replace a backup are conditional on the version last compared. Google Drive and Synology cannot make a
  write conditional, so Safay checks the version just before the upload and reads the result back; only another
  device's upload within those moments can be replaced without a conflict.
- **iCloud**: the private database, a zone `Safay`, one record whose asset is the encrypted envelope as is (device name
  and timestamp in `encryptedValues`). Every write is conditional on the record's change tag. Safay records the iCloud
  user's opaque ID when iCloud is chosen and stops syncing if the account changes. Silent pushes carry no data and only
  trigger a sync. "Turn Off and Delete iCloud Backup" deletes the zone for all devices. Development packages use the
  CloudKit Development environment; only release packages use Production.

### Updates

Updates use NSH's `nsh-updater`. The update source and the public key are written into the package and cannot be
changed by code. The manifest must carry a valid Ed25519 signature from Safay's update key, name this app, and offer a
higher version; the archive is checked against the manifest's SHA-256 and size while it streams, and the new app must
carry this app's ID and executable. On macOS its code signature is verified too before the bundle is replaced; Linux and
Windows packages carry no code signature, so the manifest's signature and the archive's hash are what vouch for them.
On Linux and Windows, Safay replaces itself only where the user can write; installed elsewhere, it offers the new
version as a download. On macOS an app that cannot be replaced reports it as soon as the update downloads.
Requests go only to the update source over HTTPS and carry no vault or device information. Automatic checks start only
after the user agrees on the second launch.

## Launcher integration (macOS)

Alfred, Raycast and Tinycast talk to Safay only through two AppleScript commands, so macOS Automation (TCC) decides,
per app, who may send them. There is no local socket, port or other channel.

- The **Launcher Access** setting is off by default and turned off by a reset. While off, both commands are
  refused and the search index is cleared.
- `find accounts` returns at most 50 matches with service names and masked account names (`al***@example.com`), never
  a secret or a code. To search a locked Safay, the account IDs and names stay in memory after locking; they are
  cleared on quit, reset, or when the setting is turned off, and are never written anywhere.
- `copy code` requires proof every time, whether or not Safay is unlocked: Touch ID — the prompt names the requesting
  app and the account — or the master password in Safay's window. The code goes only to the clipboard, and Safay shows
  a notice naming the account. The command returns a status, never the code.
- The requesting app's name is found by walking up from the `osascript` process; it is shown to the user and never used
  to grant access.
- `safay://` links only bring the main window forward; they can never search or copy.

## Platform permissions

- **macOS**: not sandboxed, Hardened Runtime. Entitlements: camera; iCloud and push notifications only in packages built
  with a matching provisioning profile. Screen Recording, camera and Automation are requested through the system.
  Selecting a QR code on screen pictures each display once with the system's `screencapture` into a private temporary
  directory and deletes each picture after one read; the pictures stay in memory, in gray, only until the user picks a
  code or cancels. Only codes that hold a valid account are outlined, named by their issuer. From the capture shortcut
  while Safay is locked, the picked code is read first and imported only after Touch ID or the master password unlocks
  the vault; until then (two minutes at most) its text stays in memory and nothing is written. Camera frames are
  processed in memory only.
- **Linux**: not sandboxed. "Select on Screen" asks the desktop portal for an interactive screenshot, which the desktop
  saves where it chooses; Safay reads it once, deletes it at once and keeps only the QR codes' text in memory. Camera
  frames come over PipeWire and stay in memory; the stream closes with the preview. On Hyprland, Safay uses Hyprland's
  socket only to read the monitors' layout and binds, place the tray panel, bind the global shortcuts' keys, and, while
  the panel shows, close it on a click outside (the click still reaches what is under it). A captured code's
  notification names its issuer, never a secret.
- **Android**: permissions for the camera and biometrics only. The vault, settings, sign-ins and exports waiting for
  the share sheet live in the app's no-backup directory: Android's backup and device-to-device transfer never copy
  them, so the vault leaves the device only through the backup chosen in Safay. Sign-ins are sealed by a key in the
  hardware key store; the unlocked app is kept out of screenshots and the recent apps' pictures.
- **Windows / Linux**: see [Platforms](docs/PLATFORMS.md) for what is implemented; file permissions on Windows follow the
  user profile's ACLs.

## Development and testing

Tests use public RFC vectors and synthetic data only. Real vaults, keys and production data directories must never
appear in tests, traces, screenshots or bug reports. The NSH test and preview runners, and `make preview`, use an
in-memory vault and credential store.
