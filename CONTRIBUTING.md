# Contributing

Thank you for helping with Safay. Security issues go through [SECURITY.md](SECURITY.md), not public issues.

## Before you start

- For anything beyond a small fix, open an issue first to agree on the approach.
- Read [Architecture](docs/ARCHITECTURE.md) and, for changes to the vault, unlocking, the clipboard, import/export,
  networking, sync or launchers, [SECURITY.md](SECURITY.md). Changes must not weaken a protection described there; if a
  trade-off is unavoidable, the pull request must state it and update SECURITY.md.
- Set up the toolchain as described in [Development](docs/DEVELOPMENT.md).

## Making a change

- **Interface**: use NSH components and theme tokens; follow [Design](docs/DESIGN.md). Check light and dark, all three
  languages, keyboard navigation, larger text and Reduce Motion.
- **Platform features**: put operating-system code in `src/platform/`, expose it through `platform::supports`, and add
  a row to the table in [Architecture](docs/ARCHITECTURE.md#nsh-and-what-is-built-outside-it) when it fills a gap in
  NSH. Update [Platforms](docs/PLATFORMS.md).
- **Core behaviour**: `crates/safay-core` is the only implementation of the formats and cryptography. If behaviour
  changes on purpose, update the affected `golden/` vectors in the same commit and explain why.
- **Text**: English source strings are the keys. Add every new key to `locales/zh-Hans.json` and `locales/zh-Hant.json`;
  `locales/en.json` holds only keys whose English differs from the key. A release's notes in `CHANGELOG.md` are
  translated in `locales/release-notes/`.
- **Tests**: add tests at the lowest layer that can catch the bug — core, session, headless UI, then native. Use public
  RFC vectors and synthetic data only.
- **Documentation**: keep the docs in step with the code; record user-visible changes under `[Unreleased]` in
  [CHANGELOG.md](CHANGELOG.md).

## Pull requests

Run `make ci` before opening a pull request; it runs what CI runs on macOS (formatting, clippy with warnings as
errors, Rust tests, NSH checks, headless and native UI tests). CI also runs `cargo deny` advisories and licenses for
every platform; `make audit` runs it for this host. Describe what changed, why, and how you tested it, including the
platforms you actually ran. Keep commits focused, with a short imperative subject.

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
