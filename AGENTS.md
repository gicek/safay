# Agent guide

Safay is a desktop two-factor authenticator in Rust with the NSH UI framework. Start with
[README.md](README.md), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and [CONTRIBUTING.md](CONTRIBUTING.md); everything
there applies. This file adds what an automated agent most often gets wrong.

## NSH

- Take NSH's API from the Rust sources and `.nsh/types` of the commit pinned in `Cargo.lock`, and from NSH's Coding and
  Design Guides (the `nsh` and `nsh-design-guides` skills, if installed). Never guess an API from React, SwiftUI, CSS or
  GPUI names.
- Keep NSH's project conventions: `nsh.json`, `src/`, `assets/`, `.nsh/` (generated, never edited or committed),
  `target/`. Do not add per-platform UI projects, UniFFI or XcodeGen.
- Never modify NSH's sources, the generated runner in `.nsh/`, or maintain a fork. Missing capabilities go in
  `src/platform/`, gated by `platform::supports`, and are listed in the Architecture table with the condition for
  moving back. Check NSH `main` for the capability first.
- Name files in `src/platform/` by where they are built: no suffix for a module built on every system, `_macos`,
  `_linux` or `_windows` for one built on that system only, with one name for the implementations of one feature
  (`camera_macos.rs`, `camera_linux.rs`). Module names carry no suffix (`#[path]` in `platform/mod.rs`).
- Upgrading NSH is a deliberate step ([Development](docs/DEVELOPMENT.md#nsh)): update the lock, rebuild the CLI from the
  same commit, run `make all`, review the Architecture table.

## Code

- Interface through NSH only; the system host handles only lifecycle, windows and OS services.
- One implementation of formats and cryptography: `crates/safay-core`. Do not reimplement cryptography.
- Stores issue commands to the controller; pages do no file or network I/O. Components run once; dynamic reads go in
  bindings. Use stable account IDs. Every timer, subscription and task has an owner and a cleanup.
- One controller per process; each window has its own `Store` and never a second unlocked session.
- Code and comments are English and describe Safay itself.
- Interface changes follow [docs/DESIGN.md](docs/DESIGN.md) and the NSH Design Guides.
- Before working on Windows or Linux, read [docs/PLATFORMS.md](docs/PLATFORMS.md); update it and `platform::supports`
  when a capability lands.

## Security

- Read [SECURITY.md](SECURITY.md) before touching the vault, unlocking, the clipboard, import/export, networking, sync
  or launcher commands. Never weaken a protection to fit a framework API.
- Passwords, secrets, account data and cloud credentials never go to NSH's generic storage, logs, traces, screenshots,
  crash reports or plaintext caches. Do not claim that framework text buffers or the OS clipboard are wiped.
- Site icons are never loaded through a remote `Image`.
- Tests inject memory file systems, credential stores, network, clipboard and clocks; NSH's own mocks do not intercept
  Safay's Rust I/O. Native tests never touch a real vault, keys or production directories. Only public RFC vectors and
  synthetic fixtures may appear in traces, screenshots or prompts.

## Verifying

- Test pure logic, then NSH headless, then the affected native backends; permissions, hardware authentication and
  signing are checked by hand.
- Check keyboard, focus, screen readers, light and dark, larger text, window resizing, failure and cancel paths. A
  build that compiles or a screenshot that looks right is not proof.
- Report what actually ran. Never describe a platform as verified without running it there.

## Repository

- Keep documentation current with the code (Architecture, Platforms, Security, CHANGELOG).
- Do not commit, push or publish unless asked.
