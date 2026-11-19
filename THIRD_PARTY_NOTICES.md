# Third-party notices

## Verification-code font

Atkinson Hyperlegible Mono, Braille Institute / Applied Design Works. SIL Open Font License 1.1. The font and its complete license are bundled in assets/fonts.


Safay, including `safay-core`, `safay-app` and `cloudset`, is licensed under MIT. NSH is licensed under Apache-2.0.

## Backup location brand marks

- Source: the iCloud, Dropbox and Nextcloud shapes come from Simple Icons 11.15.0 with brand colors added; the OneDrive and Google Drive marks were drawn for this project.
- License: the Simple Icons shapes are CC0 1.0. The marks are trademarks of Apple, Dropbox, Microsoft, Google and Nextcloud, used only to identify the corresponding service in the backup location list; no partnership or endorsement is implied.
- Location: `assets/brands/`; used only by `src/pages/backup.rs::location_mark`. Removal: delete the files and fall back to the generic symbols.

## Direct application dependencies

Versions below are exact pins. Cargo.lock records the full production build graph. Removing NSH replaces the UI framework; the other dependencies can be removed by replacing the named application service without changing the vault format.

| Dependency | Version | License | Purpose / removal boundary |
|---|---|---|---|
| nsh-core, nsh-base, nsh-platform-macos/windows/linux | main @ Cargo.lock commit (crate version 0.5.0) | Apache-2.0 | Native UI, components and desktop embedding; Git source https://github.com/longbridge/nsh |
| nsh-engine, nsh-testing | main @ Cargo.lock commit (crate version 0.5.0) | Apache-2.0 | Headless tests; not application data storage |
| nsh-updater | main @ Cargo.lock commit (crate version 0.5.0) | Apache-2.0 | App self-update: signed manifests, verified downloads, replacing the app; replace the update service |
| image | 0.25.10 | MIT OR Apache-2.0 | Bounded QR processing; site icon decoding (PNG, ICO, JPEG, GIF, WebP) and PNG re-encoding |
| qrcode | 0.14.1 | MIT OR Apache-2.0 | Generate a single-account QR; replace QR generation |
| rqrr | 0.11.0 | MIT | Decode local QR images; replace QR recognition |
| base64 | 0.22.1 | MIT OR Apache-2.0 | In-memory QR image URI encoding |
| serde_json | 1.0.151 | MIT OR Apache-2.0 | Localized resources and non-sensitive preferences |
| zeroize | 1.9.0 | Apache-2.0 OR MIT | Application-owned secret buffers |
| tempfile | 3.27.0 | MIT OR Apache-2.0 | Exclusive temporary files and atomic ciphertext replacement |
| uuid | 1.26.1 | Apache-2.0 OR MIT | Stable domain identifiers; randomness comes from the core |
| serde | 1.0.229 | MIT OR Apache-2.0 | Preference serialization |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | Typed, non-sensitive application errors |
| dispatch2 | 0.3.1 | MIT | macOS main-thread work |
| block2 | 0.6.2 | MIT | macOS lifecycle callbacks |
| libc | 0.2.189 | MIT OR Apache-2.0 | macOS process information (the app that sent a launcher's Apple event) |
| ureq | 3.5.1 | MIT OR Apache-2.0 | HTTP executor for cloud backup and site icons (rustls, system certificate verification); already used by NSH |
| rustls, rustls-pki-types, rustls-platform-verifier | 0.23.46, 1.15.1, 0.7.1 | Apache-2.0 OR ISC OR MIT, MIT OR Apache-2.0, MIT OR Apache-2.0 | The pinned-certificate TLS step for self-signed WebDAV / Nextcloud servers (`src/platform/tls.rs`); the same versions ureq already uses |
| sha2 | 0.11.0 | MIT OR Apache-2.0 | Certificate fingerprints; already used by the core |
| keyring | 3.6.3 | MIT OR Apache-2.0 | Cloud sign-ins in the macOS Keychain / Windows Credential Manager; replace the credential store adapter |
| objc2-app-kit, objc2-authentication-services | 0.3.2 | Zlib OR Apache-2.0 OR MIT | macOS system APIs (AppKit, web sign-in sessions) |
| objc2-foundation | 0.3.2 | MIT | macOS Foundation APIs (including AppleScript commands for launchers) |
| objc2-av-foundation, objc2-core-media, objc2-core-video | 0.3.2 | Zlib OR Apache-2.0 OR MIT | macOS camera scanning (AVFoundation capture, pixel buffers) |
| objc2-cloud-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT | macOS iCloud backup (CloudKit private database); replace the iCloud store adapter |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | Windows session events and sensitive clipboard |
| ashpd | 0.14.0 | MIT | Linux desktop portal requests (Screenshot, Camera) on zbus; replace the screen selection and camera adapters |
| gstreamer, gstreamer-app | 0.25.4, 0.25.2 | MIT OR Apache-2.0 | Linux camera scanning (the portal's PipeWire stream); the same versions NSH uses for sound |
| futures-executor | 0.3.34 | MIT OR Apache-2.0 | Waiting for Linux portal requests on a worker thread |

All registry dependencies are sourced from crates.io at `https://crates.io/crates/<name>/<version>`. Exact licenses below come from resolved crate metadata. Platform-specific dependencies must be re-enumerated on their target before public distribution. This file does not reproduce every third-party license text; installers must ship them alongside.

## Resolved macOS development graph

This includes test tools; the production graph is narrower. NSH-generated test runners have their own generated Cargo workspace and add QuickJS solely for the TypeScript test harness. The shipped host is built from the root Cargo.lock without QuickJS. The core's direct dependencies are pinned too, so generated harnesses cannot silently select a different crypto release.

| Crate | Version | Declared license | Source |
|---|---|---|---|
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/adler2/2.0.1) |
| aead | 0.6.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/aead/0.6.1) |
| aes | 0.9.3 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/aes/0.9.3) |
| aes-gcm | 0.11.1 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/aes-gcm/0.11.1) |
| aho-corasick | 1.1.5 | Unlicense OR MIT | [crates.io](https://crates.io/crates/aho-corasick/1.1.5) |
| alloc-no-stdlib | 2.0.4 | BSD-3-Clause | [crates.io](https://crates.io/crates/alloc-no-stdlib/2.0.4) |
| ambient-authority | 0.0.2 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/ambient-authority/0.0.2) |
| annotate-snippets | 0.12.18 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/annotate-snippets/0.12.18) |
| anstyle | 1.0.14 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/anstyle/1.0.14) |
| anyhow | 1.0.104 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/anyhow/1.0.104) |
| arc-swap | 1.9.5 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/arc-swap/1.9.5) |
| argon2 | 0.6.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/argon2/0.6.0) |
| arraydeque | 0.5.1 | MIT/Apache-2.0 | [crates.io](https://crates.io/crates/arraydeque/0.5.1) |
| arrayref | 0.3.9 | BSD-2-Clause | [crates.io](https://crates.io/crates/arrayref/0.3.9) |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/arrayvec/0.7.8) |
| autocfg | 1.5.1 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/autocfg/1.5.1) |
| base62 | 2.2.6 | MIT | [crates.io](https://crates.io/crates/base62/2.2.6) |
| base64 | 0.22.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/base64/0.22.1) |
| base64 | 0.23.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/base64/0.23.1) |
| base64ct | 1.8.3 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/base64ct/1.8.3) |
| bit-set | 0.8.0 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/bit-set/0.8.0) |
| bit-set | 0.9.1 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/bit-set/0.9.1) |
| bit-vec | 0.8.0 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/bit-vec/0.8.0) |
| bit-vec | 0.9.1 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/bit-vec/0.9.1) |
| bitflags | 1.3.2 | MIT/Apache-2.0 | [crates.io](https://crates.io/crates/bitflags/1.3.2) |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/bitflags/2.13.2) |
| blake2 | 0.11.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/blake2/0.11.0) |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/block-buffer/0.10.4) |
| block-buffer | 0.12.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/block-buffer/0.12.1) |
| block-padding | 0.4.2 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/block-padding/0.4.2) |
| block2 | 0.6.2 | MIT | [crates.io](https://crates.io/crates/block2/0.6.2) |
| brotli-decompressor | 5.0.3 | BSD-3-Clause/MIT | [crates.io](https://crates.io/crates/brotli-decompressor/5.0.3) |
| bstr | 1.13.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/bstr/1.13.1) |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/bumpalo/3.20.3) |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/bytemuck/1.25.2) |
| byteorder-lite | 0.1.0 | Unlicense OR MIT | [crates.io](https://crates.io/crates/byteorder-lite/0.1.0) |
| bytes | 1.12.1 | MIT | [crates.io](https://crates.io/crates/bytes/1.12.1) |
| cap-primitives | 4.0.3 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/cap-primitives/4.0.3) |
| cap-std | 4.0.3 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/cap-std/4.0.3) |
| caseless | 0.2.2 | MIT | [crates.io](https://crates.io/crates/caseless/0.2.2) |
| cbc | 0.2.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/cbc/0.2.1) |
| cc | 1.10.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/cc/1.10.0) |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/cfg-if/1.0.5) |
| cfg_aliases | 0.2.2 | MIT | [crates.io](https://crates.io/crates/cfg_aliases/0.2.2) |
| chacha20 | 0.10.2 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/chacha20/0.10.2) |
| chrono | 0.4.45 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/chrono/0.4.45) |
| cipher | 0.5.2 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/cipher/0.5.2) |
| cmov | 0.5.4 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/cmov/0.5.4) |
| codespan-reporting | 0.13.1 | Apache-2.0 | [crates.io](https://crates.io/crates/codespan-reporting/0.13.1) |
| color_quant | 1.1.0 | MIT | [crates.io](https://crates.io/crates/color_quant/1.1.0) |
| const-oid | 0.10.2 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/const-oid/0.10.2) |
| const-oid | 0.9.6 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/const-oid/0.9.6) |
| constant_time_eq | 0.6.1 | CC0-1.0 OR MIT-0 OR Apache-2.0 | [crates.io](https://crates.io/crates/constant_time_eq/0.6.1) |
| core-foundation | 0.10.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/core-foundation/0.10.1) |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/core-foundation-sys/0.8.7) |
| core_maths | 0.1.1 | MIT | [crates.io](https://crates.io/crates/core_maths/0.1.1) |
| cpubits | 0.1.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/cpubits/0.1.1) |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/cpufeatures/0.2.17) |
| cpufeatures | 0.3.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/cpufeatures/0.3.1) |
| crc32fast | 1.5.4 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/crc32fast/1.5.4) |
| crossbeam-channel | 0.5.17 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/crossbeam-channel/0.5.17) |
| crossbeam-deque | 0.8.8 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/crossbeam-deque/0.8.8) |
| crossbeam-epoch | 0.9.21 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/crossbeam-epoch/0.9.21) |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/crossbeam-utils/0.8.23) |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/crypto-common/0.1.7) |
| crypto-common | 0.2.2 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/crypto-common/0.2.2) |
| ctr | 0.10.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/ctr/0.10.1) |
| ctutils | 0.4.5 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/ctutils/0.4.5) |
| curve25519-dalek | 4.1.3 | BSD-3-Clause | [crates.io](https://crates.io/crates/curve25519-dalek/4.1.3) |
| data-encoding | 2.11.1 | MIT | [crates.io](https://crates.io/crates/data-encoding/2.11.1) |
| data-url | 0.3.2 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/data-url/0.3.2) |
| der | 0.7.10 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/der/0.7.10) |
| deranged | 0.5.8 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/deranged/0.5.8) |
| digest | 0.10.7 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/digest/0.10.7) |
| digest | 0.11.3 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/digest/0.11.3) |
| dispatch2 | 0.3.1 | Zlib OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/dispatch2/0.3.1) |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/displaydoc/0.2.7) |
| ed25519 | 2.2.3 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/ed25519/2.2.3) |
| ed25519-dalek | 2.2.0 | BSD-3-Clause | [crates.io](https://crates.io/crates/ed25519-dalek/2.2.0) |
| either | 1.18.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/either/1.18.0) |
| encoding_rs | 0.8.44 | (Apache-2.0 OR MIT) AND BSD-3-Clause | [crates.io](https://crates.io/crates/encoding_rs/0.8.44) |
| encoding_rs_io | 0.1.8 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/encoding_rs_io/0.1.8) |
| env_filter | 2.0.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/env_filter/2.0.0) |
| equivalent | 1.0.2 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/equivalent/1.0.2) |
| errno | 0.3.14 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/errno/0.3.14) |
| euclid | 0.22.14 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/euclid/0.22.14) |
| fancy-regex | 0.18.0 | MIT | [crates.io](https://crates.io/crates/fancy-regex/0.18.0) |
| fastrand | 2.5.0 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/fastrand/2.5.0) |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/fdeflate/0.3.7) |
| filetime | 0.2.29 | MIT/Apache-2.0 | [crates.io](https://crates.io/crates/filetime/0.2.29) |
| find-msvc-tools | 0.1.15 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/find-msvc-tools/0.1.15) |
| flate2 | 1.1.10 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/flate2/1.1.10) |
| float-cmp | 0.9.0 | MIT | [crates.io](https://crates.io/crates/float-cmp/0.9.0) |
| foldhash | 0.2.0 | Zlib | [crates.io](https://crates.io/crates/foldhash/0.2.0) |
| fontdb | 0.23.0 | MIT | [crates.io](https://crates.io/crates/fontdb/0.23.0) |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/form_urlencoded/1.2.2) |
| fs-set-times | 0.20.3 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/fs-set-times/0.20.3) |
| futf | 0.1.5 | MIT / Apache-2.0 | [crates.io](https://crates.io/crates/futf/0.1.5) |
| futures-core | 0.3.34 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/futures-core/0.3.34) |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/futures-macro/0.3.34) |
| futures-sink | 0.3.34 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/futures-sink/0.3.34) |
| futures-task | 0.3.34 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/futures-task/0.3.34) |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/futures-util/0.3.34) |
| g2gen | 1.2.2 | MIT/Apache-2.0 | [crates.io](https://crates.io/crates/g2gen/1.2.2) |
| g2p | 1.2.2 | MIT/Apache-2.0 | [crates.io](https://crates.io/crates/g2p/1.2.2) |
| g2poly | 1.2.2 | MIT/Apache-2.0 | [crates.io](https://crates.io/crates/g2poly/1.2.2) |
| generic-array | 0.14.7 | MIT | [crates.io](https://crates.io/crates/generic-array/0.14.7) |
| getrandom | 0.2.17 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/getrandom/0.2.17) |
| getrandom | 0.3.4 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/getrandom/0.3.4) |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/getrandom/0.4.3) |
| ghash | 0.6.0 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/ghash/0.6.0) |
| gif | 0.14.2 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/gif/0.14.2) |
| glob | 0.3.4 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/glob/0.3.4) |
| globset | 0.4.21 | Unlicense OR MIT | [crates.io](https://crates.io/crates/globset/0.4.21) |
| globwalk | 0.8.1 | MIT | [crates.io](https://crates.io/crates/globwalk/0.8.1) |
| granit-parser | 1.3.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/granit-parser/1.3.0) |
| half | 2.7.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/half/2.7.1) |
| hashbrown | 0.16.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/hashbrown/0.16.1) |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/hashbrown/0.17.1) |
| hexf-parse | 0.2.1 | CC0-1.0 | [crates.io](https://crates.io/crates/hexf-parse/0.2.1) |
| hmac | 0.13.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/hmac/0.13.0) |
| html5ever | 0.27.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/html5ever/0.27.0) |
| http | 1.5.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/http/1.5.0) |
| httparse | 1.10.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/httparse/1.10.1) |
| httpdate | 1.0.3 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/httpdate/1.0.3) |
| hybrid-array | 0.4.15 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/hybrid-array/0.4.15) |
| iana-time-zone | 0.1.65 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/iana-time-zone/0.1.65) |
| icu_collections | 2.3.0 | Unicode-3.0 | [crates.io](https://crates.io/crates/icu_collections/2.3.0) |
| icu_locale_core | 2.3.0 | Unicode-3.0 | [crates.io](https://crates.io/crates/icu_locale_core/2.3.0) |
| icu_normalizer | 2.3.0 | Unicode-3.0 | [crates.io](https://crates.io/crates/icu_normalizer/2.3.0) |
| icu_normalizer_data | 2.3.0 | Unicode-3.0 | [crates.io](https://crates.io/crates/icu_normalizer_data/2.3.0) |
| icu_properties | 2.3.0 | Unicode-3.0 | [crates.io](https://crates.io/crates/icu_properties/2.3.0) |
| icu_properties_data | 2.3.0 | Unicode-3.0 | [crates.io](https://crates.io/crates/icu_properties_data/2.3.0) |
| icu_provider | 2.3.1 | Unicode-3.0 | [crates.io](https://crates.io/crates/icu_provider/2.3.1) |
| idna | 1.1.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/idna/1.1.0) |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/idna_adapter/1.2.2) |
| ignore | 0.4.34 | Unlicense OR MIT | [crates.io](https://crates.io/crates/ignore/0.4.34) |
| image | 0.25.10 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/image/0.25.10) |
| image-webp | 0.2.4 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/image-webp/0.2.4) |
| imagesize | 0.14.0 | MIT | [crates.io](https://crates.io/crates/imagesize/0.14.0) |
| imagesize | 0.15.0 | MIT | [crates.io](https://crates.io/crates/imagesize/0.15.0) |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/indexmap/2.14.2) |
| inout | 0.2.2 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/inout/0.2.2) |
| io-extras | 0.19.0 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/io-extras/0.19.0) |
| io-lifetimes | 2.0.4 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/io-lifetimes/2.0.4) |
| io-lifetimes | 3.0.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/io-lifetimes/3.0.1) |
| ipnet | 2.12.2 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/ipnet/2.12.2) |
| itertools | 0.11.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/itertools/0.11.0) |
| itertools | 0.14.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/itertools/0.14.0) |
| itoa | 1.0.18 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/itoa/1.0.18) |
| keyring | 3.6.3 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/keyring/3.6.3) |
| kurbo | 0.13.1 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/kurbo/0.13.1) |
| lazy_static | 1.5.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/lazy_static/1.5.1) |
| libc | 0.2.189 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/libc/0.2.189) |
| libm | 0.2.16 | MIT | [crates.io](https://crates.io/crates/libm/0.2.16) |
| litemap | 0.8.3 | Unicode-3.0 | [crates.io](https://crates.io/crates/litemap/0.8.3) |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/lock_api/0.4.14) |
| log | 0.4.34 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/log/0.4.34) |
| lru | 0.18.5 | MIT | [crates.io](https://crates.io/crates/lru/0.18.5) |
| mac | 0.1.1 | MIT/Apache-2.0 | [crates.io](https://crates.io/crates/mac/0.1.1) |
| markdown | 1.0.0 | MIT | [crates.io](https://crates.io/crates/markdown/1.0.0) |
| markup5ever | 0.12.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/markup5ever/0.12.1) |
| markup5ever_rcdom | 0.3.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/markup5ever_rcdom/0.3.0) |
| maybe-owned | 0.3.4 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/maybe-owned/0.3.4) |
| md-5 | 0.11.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/md-5/0.11.0) |
| memchr | 2.8.3 | Unlicense OR MIT | [crates.io](https://crates.io/crates/memchr/2.8.3) |
| memmap2 | 0.9.12 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/memmap2/0.9.12) |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 | [crates.io](https://crates.io/crates/miniz_oxide/0.8.9) |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 | [crates.io](https://crates.io/crates/miniz_oxide/0.9.1) |
| mio | 1.2.4 | MIT | [crates.io](https://crates.io/crates/mio/1.2.4) |
| moxcms | 0.8.1 | BSD-3-Clause OR Apache-2.0 | [crates.io](https://crates.io/crates/moxcms/0.8.1) |
| multiversion_no_op | 1.0.0 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/multiversion_no_op/1.0.0) |
| naga | 29.0.4 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/naga/29.0.4) |
| new_debug_unreachable | 1.0.6 | MIT | [crates.io](https://crates.io/crates/new_debug_unreachable/1.0.6) |
| nix | 0.31.3 | MIT | [crates.io](https://crates.io/crates/nix/0.31.3) |
| nohash-hasher | 0.2.0 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/nohash-hasher/0.2.0) |
| normpath | 1.5.2 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/normpath/1.5.2) |
| nsh-base | 0.5.0 (main) | Apache-2.0 | [GitHub](https://github.com/longbridge/nsh) |
| nsh-core | 0.5.0 (main) | Apache-2.0 | [GitHub](https://github.com/longbridge/nsh) |
| nsh-engine | 0.5.0 (main) | Apache-2.0 | [GitHub](https://github.com/longbridge/nsh) |
| nsh-macros | 0.5.0 (main) | Apache-2.0 | [GitHub](https://github.com/longbridge/nsh) |
| nsh-platform-apple | 0.5.0 (main) | Apache-2.0 | [GitHub](https://github.com/longbridge/nsh) |
| nsh-platform-macos | 0.5.0 (main) | Apache-2.0 | [GitHub](https://github.com/longbridge/nsh) |
| nsh-testing | 0.5.0 (main) | Apache-2.0 | [GitHub](https://github.com/longbridge/nsh) |
| nsh-updater | 0.5.0 (main) | Apache-2.0 | [GitHub](https://github.com/longbridge/nsh) |
| num-conv | 0.2.2 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/num-conv/0.2.2) |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/num-traits/0.2.19) |
| objc2 | 0.6.5 | MIT | [crates.io](https://crates.io/crates/objc2/0.6.5) |
| objc2-app-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/objc2-app-kit/0.3.2) |
| objc2-authentication-services | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/objc2-authentication-services/0.3.2) |
| objc2-av-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/objc2-av-foundation/0.3.2) |
| objc2-cloud-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/objc2-cloud-kit/0.3.2) |
| objc2-core-audio | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/objc2-core-audio/0.3.2) |
| objc2-core-audio-types | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/objc2-core-audio-types/0.3.2) |
| objc2-core-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/objc2-core-foundation/0.3.2) |
| objc2-core-graphics | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/objc2-core-graphics/0.3.2) |
| objc2-core-media | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/objc2-core-media/0.3.2) |
| objc2-core-text | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/objc2-core-text/0.3.2) |
| objc2-core-video | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/objc2-core-video/0.3.2) |
| objc2-encode | 4.1.0 | MIT | [crates.io](https://crates.io/crates/objc2-encode/4.1.0) |
| objc2-foundation | 0.3.2 | MIT | [crates.io](https://crates.io/crates/objc2-foundation/0.3.2) |
| objc2-image-io | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/objc2-image-io/0.3.2) |
| objc2-io-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/objc2-io-kit/0.3.2) |
| objc2-io-surface | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/objc2-io-surface/0.3.2) |
| objc2-quartz-core | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/objc2-quartz-core/0.3.2) |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/once_cell/1.21.4) |
| ordered-float | 5.5.1 | MIT | [crates.io](https://crates.io/crates/ordered-float/5.5.1) |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/parking_lot/0.12.5) |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/parking_lot_core/0.9.12) |
| password-hash | 0.6.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/password-hash/0.6.1) |
| pbkdf2 | 0.13.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/pbkdf2/0.13.0) |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/percent-encoding/2.3.2) |
| phc | 0.6.1 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/phc/0.6.1) |
| phf | 0.11.3 | MIT | [crates.io](https://crates.io/crates/phf/0.11.3) |
| phf_codegen | 0.11.3 | MIT | [crates.io](https://crates.io/crates/phf_codegen/0.11.3) |
| phf_generator | 0.11.3 | MIT | [crates.io](https://crates.io/crates/phf_generator/0.11.3) |
| phf_shared | 0.11.3 | MIT | [crates.io](https://crates.io/crates/phf_shared/0.11.3) |
| pico-args | 0.5.0 | MIT | [crates.io](https://crates.io/crates/pico-args/0.5.0) |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/pin-project-lite/0.2.17) |
| pkcs8 | 0.10.2 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/pkcs8/0.10.2) |
| png | 0.17.16 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/png/0.17.16) |
| png | 0.18.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/png/0.18.1) |
| poly1305 | 0.9.1 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/poly1305/0.9.1) |
| polycool | 0.4.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/polycool/0.4.0) |
| polyval | 0.7.3 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/polyval/0.7.3) |
| potential_utf | 0.1.6 | Unicode-3.0 | [crates.io](https://crates.io/crates/potential_utf/0.1.6) |
| powerfmt | 0.2.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/powerfmt/0.2.0) |
| ppv-lite86 | 0.2.21 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/ppv-lite86/0.2.21) |
| precomputed-hash | 0.1.1 | MIT | [crates.io](https://crates.io/crates/precomputed-hash/0.1.1) |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/proc-macro2/1.0.107) |
| prost | 0.14.4 | Apache-2.0 | [crates.io](https://crates.io/crates/prost/0.14.4) |
| prost-derive | 0.14.4 | Apache-2.0 | [crates.io](https://crates.io/crates/prost-derive/0.14.4) |
| pxfm | 0.1.30 | BSD-3-Clause OR Apache-2.0 | [crates.io](https://crates.io/crates/pxfm/0.1.30) |
| qrcode | 0.14.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/qrcode/0.14.1) |
| quick-error | 2.0.1 | MIT/Apache-2.0 | [crates.io](https://crates.io/crates/quick-error/2.0.1) |
| quick-xml | 0.42.0 | MIT | [crates.io](https://crates.io/crates/quick-xml/0.42.0) |
| quote | 1.0.47 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/quote/1.0.47) |
| rand | 0.8.8 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/rand/0.8.8) |
| rand | 0.9.5 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/rand/0.9.5) |
| rand_chacha | 0.9.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/rand_chacha/0.9.0) |
| rand_core | 0.10.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/rand_core/0.10.1) |
| rand_core | 0.6.4 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/rand_core/0.6.4) |
| rand_core | 0.9.5 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/rand_core/0.9.5) |
| regex | 1.13.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/regex/1.13.1) |
| regex-automata | 0.4.18 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/regex-automata/0.4.18) |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/regex-syntax/0.8.11) |
| resvg | 0.46.0 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/resvg/0.46.0) |
| rgb | 0.8.53 | MIT | [crates.io](https://crates.io/crates/rgb/0.8.53) |
| ring | 0.17.14 | Apache-2.0 AND ISC | [crates.io](https://crates.io/crates/ring/0.17.14) |
| roxmltree | 0.21.2 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/roxmltree/0.21.2) |
| rqrr | 0.11.0 | (MIT OR Apache-2.0) AND ISC | [crates.io](https://crates.io/crates/rqrr/0.11.0) |
| rust-i18n | 4.2.4 | MIT | [crates.io](https://crates.io/crates/rust-i18n/4.2.4) |
| rust-i18n-macro | 4.2.4 | MIT | [crates.io](https://crates.io/crates/rust-i18n-macro/4.2.4) |
| rust-i18n-support | 4.2.4 | MIT | [crates.io](https://crates.io/crates/rust-i18n-support/4.2.4) |
| rustc-hash | 1.1.0 | Apache-2.0/MIT | [crates.io](https://crates.io/crates/rustc-hash/1.1.0) |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/rustc-hash/2.1.3) |
| rustc_version | 0.4.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/rustc_version/0.4.1) |
| rustix | 1.1.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/rustix/1.1.5) |
| rustls | 0.23.46 | Apache-2.0 OR ISC OR MIT | [crates.io](https://crates.io/crates/rustls/0.23.46) |
| rustls-pki-types | 1.15.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/rustls-pki-types/1.15.1) |
| rustls-platform-verifier | 0.7.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/rustls-platform-verifier/0.7.1) |
| rustls-webpki | 0.103.16 | ISC | [crates.io](https://crates.io/crates/rustls-webpki/0.103.16) |
| rustversion | 1.0.23 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/rustversion/1.0.23) |
| rustybuzz | 0.20.1 | MIT | [crates.io](https://crates.io/crates/rustybuzz/0.20.1) |
| salsa20 | 0.11.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/salsa20/0.11.0) |
| same-file | 1.0.6 | Unlicense/MIT | [crates.io](https://crates.io/crates/same-file/1.0.6) |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/scopeguard/1.2.0) |
| scrypt | 0.12.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/scrypt/0.12.0) |
| security-framework | 3.7.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/security-framework/3.7.0) |
| security-framework-sys | 2.17.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/security-framework-sys/2.17.0) |
| semver | 1.0.28 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/semver/1.0.28) |
| serde | 1.0.229 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/serde/1.0.229) |
| serde-saphyr | 1.3.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/serde-saphyr/1.3.0) |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/serde_core/1.0.229) |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/serde_derive/1.0.229) |
| serde_json | 1.0.151 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/serde_json/1.0.151) |
| serde_spanned | 0.6.9 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/serde_spanned/0.6.9) |
| sha1 | 0.10.7 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/sha1/0.10.7) |
| sha1 | 0.11.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/sha1/0.11.0) |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/sha2/0.10.9) |
| sha2 | 0.11.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/sha2/0.11.0) |
| shlex | 2.0.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/shlex/2.0.1) |
| signature | 2.2.0 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/signature/2.2.0) |
| simd-adler32 | 0.3.10 | MIT | [crates.io](https://crates.io/crates/simd-adler32/0.3.10) |
| simdutf8 | 0.1.5 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/simdutf8/0.1.5) |
| simplecss | 0.2.2 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/simplecss/0.2.2) |
| siphasher | 1.0.4 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/siphasher/1.0.4) |
| slab | 0.4.12 | MIT | [crates.io](https://crates.io/crates/slab/0.4.12) |
| slotmap | 1.1.1 | Zlib | [crates.io](https://crates.io/crates/slotmap/1.1.1) |
| smallvec | 1.16.2 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/smallvec/1.16.2) |
| socket2 | 0.6.5 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/socket2/0.6.5) |
| spki | 0.7.3 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/spki/0.7.3) |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/stable_deref_trait/1.2.1) |
| strict-num | 0.1.1 | MIT | [crates.io](https://crates.io/crates/strict-num/0.1.1) |
| string_cache | 0.8.9 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/string_cache/0.8.9) |
| string_cache_codegen | 0.5.4 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/string_cache_codegen/0.5.4) |
| subtle | 2.6.1 | BSD-3-Clause | [crates.io](https://crates.io/crates/subtle/2.6.1) |
| svgtypes | 0.16.1 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/svgtypes/0.16.1) |
| syn | 2.0.119 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/syn/2.0.119) |
| syn | 3.0.6 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/syn/3.0.6) |
| synstructure | 0.14.0 | MIT | [crates.io](https://crates.io/crates/synstructure/0.14.0) |
| taffy | 0.15.1 | MIT | [crates.io](https://crates.io/crates/taffy/0.15.1) |
| tar | 0.4.46 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/tar/0.4.46) |
| tempfile | 3.27.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/tempfile/3.27.0) |
| tendril | 0.4.3 | MIT/Apache-2.0 | [crates.io](https://crates.io/crates/tendril/0.4.3) |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/thiserror/2.0.21) |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/thiserror-impl/2.0.21) |
| time | 0.3.55 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/time/0.3.55) |
| time-core | 0.1.9 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/time-core/0.1.9) |
| time-macros | 0.2.32 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/time-macros/0.2.32) |
| tiny-skia | 0.11.4 | BSD-3-Clause | [crates.io](https://crates.io/crates/tiny-skia/0.11.4) |
| tiny-skia-path | 0.11.4 | BSD-3-Clause | [crates.io](https://crates.io/crates/tiny-skia-path/0.11.4) |
| tinystr | 0.8.4 | Unicode-3.0 | [crates.io](https://crates.io/crates/tinystr/0.8.4) |
| tinyvec | 1.13.3 | Zlib OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/tinyvec/1.13.3) |
| tokio | 1.54.1 | MIT | [crates.io](https://crates.io/crates/tokio/1.54.1) |
| tokio-macros | 2.7.2 | MIT | [crates.io](https://crates.io/crates/tokio-macros/2.7.2) |
| tokio-rustls | 0.26.6 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/tokio-rustls/0.26.6) |
| tokio-tungstenite | 0.28.0 | MIT | [crates.io](https://crates.io/crates/tokio-tungstenite/0.28.0) |
| toml | 0.8.23 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/toml/0.8.23) |
| toml_datetime | 0.6.11 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/toml_datetime/0.6.11) |
| toml_edit | 0.22.27 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/toml_edit/0.22.27) |
| toml_write | 0.1.2 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/toml_write/0.1.2) |
| triomphe | 0.1.17 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/triomphe/0.1.17) |
| ttf-parser | 0.25.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/ttf-parser/0.25.1) |
| tungstenite | 0.28.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/tungstenite/0.28.0) |
| typed-path | 0.13.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/typed-path/0.13.0) |
| typenum | 1.20.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/typenum/1.20.1) |
| unicode-bidi | 0.3.18 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/unicode-bidi/0.3.18) |
| unicode-bidi-mirroring | 0.4.0 | MIT/Apache-2.0 | [crates.io](https://crates.io/crates/unicode-bidi-mirroring/0.4.0) |
| unicode-ccc | 0.4.0 | MIT/Apache-2.0 | [crates.io](https://crates.io/crates/unicode-ccc/0.4.0) |
| unicode-id | 0.3.7 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/unicode-id/0.3.7) |
| unicode-ident | 1.0.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | [crates.io](https://crates.io/crates/unicode-ident/1.0.26) |
| unicode-linebreak | 0.1.5 | Apache-2.0 | [crates.io](https://crates.io/crates/unicode-linebreak/0.1.5) |
| unicode-normalization | 0.1.25 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/unicode-normalization/0.1.25) |
| unicode-properties | 0.1.5 | (MIT OR Apache-2.0) AND Unicode-3.0 | [crates.io](https://crates.io/crates/unicode-properties/0.1.5) |
| unicode-script | 0.5.9 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/unicode-script/0.5.9) |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/unicode-segmentation/1.13.3) |
| unicode-vo | 0.1.0 | MIT/Apache-2.0 | [crates.io](https://crates.io/crates/unicode-vo/0.1.0) |
| unicode-width | 0.2.3 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/unicode-width/0.2.3) |
| universal-hash | 0.6.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/universal-hash/0.6.1) |
| untrusted | 0.9.0 | ISC | [crates.io](https://crates.io/crates/untrusted/0.9.0) |
| ureq | 3.5.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/ureq/3.5.1) |
| ureq-proto | 0.7.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/ureq-proto/0.7.1) |
| url | 2.5.8 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/url/2.5.8) |
| usvg | 0.46.0 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/usvg/0.46.0) |
| utf-8 | 0.7.6 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/utf-8/0.7.6) |
| utf8-zero | 0.8.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/utf8-zero/0.8.1) |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/utf8_iter/1.0.4) |
| uuid | 1.26.1 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/uuid/1.26.1) |
| version_check | 0.9.5 | MIT/Apache-2.0 | [crates.io](https://crates.io/crates/version_check/0.9.5) |
| walkdir | 2.5.0 | Unlicense/MIT | [crates.io](https://crates.io/crates/walkdir/2.5.0) |
| webpki-roots | 0.26.11 | CDLA-Permissive-2.0 | [crates.io](https://crates.io/crates/webpki-roots/0.26.11) |
| webpki-roots | 1.0.11 | CDLA-Permissive-2.0 | [crates.io](https://crates.io/crates/webpki-roots/1.0.11) |
| weezl | 0.1.12 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/weezl/0.1.12) |
| winnow | 0.7.15 | MIT | [crates.io](https://crates.io/crates/winnow/0.7.15) |
| writeable | 0.6.4 | Unicode-3.0 | [crates.io](https://crates.io/crates/writeable/0.6.4) |
| wuff | 0.2.9 | MIT | [crates.io](https://crates.io/crates/wuff/0.2.9) |
| xml5ever | 0.18.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/xml5ever/0.18.1) |
| xmlwriter | 0.1.0 | MIT | [crates.io](https://crates.io/crates/xmlwriter/0.1.0) |
| yoke | 0.8.3 | Unicode-3.0 | [crates.io](https://crates.io/crates/yoke/0.8.3) |
| yoke-derive | 0.8.4 | Unicode-3.0 | [crates.io](https://crates.io/crates/yoke-derive/0.8.4) |
| zerocopy | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/zerocopy/0.8.63) |
| zerocopy-derive | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/zerocopy-derive/0.8.63) |
| zerofrom | 0.1.8 | Unicode-3.0 | [crates.io](https://crates.io/crates/zerofrom/0.1.8) |
| zerofrom-derive | 0.1.8 | Unicode-3.0 | [crates.io](https://crates.io/crates/zerofrom-derive/0.1.8) |
| zeroize | 1.9.0 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/zeroize/1.9.0) |
| zeroize_derive | 1.5.0 | Apache-2.0 OR MIT | [crates.io](https://crates.io/crates/zeroize_derive/1.5.0) |
| zerotrie | 0.2.5 | Unicode-3.0 | [crates.io](https://crates.io/crates/zerotrie/0.2.5) |
| zerovec | 0.11.8 | Unicode-3.0 | [crates.io](https://crates.io/crates/zerovec/0.11.8) |
| zerovec-derive | 0.11.6 | Unicode-3.0 | [crates.io](https://crates.io/crates/zerovec-derive/0.11.6) |
| zip | 9.5.1 | MIT | [crates.io](https://crates.io/crates/zip/9.5.1) |
| zlib-rs | 0.6.8 | Zlib | [crates.io](https://crates.io/crates/zlib-rs/0.6.8) |
| zmij | 1.0.23 | MIT | [crates.io](https://crates.io/crates/zmij/1.0.23) |
| zopfli | 0.8.4 | Apache-2.0 | [crates.io](https://crates.io/crates/zopfli/0.8.4) |
| zune-core | 0.5.3 | MIT OR Apache-2.0 OR Zlib | [crates.io](https://crates.io/crates/zune-core/0.5.3) |
| zune-jpeg | 0.5.15 | MIT OR Apache-2.0 OR Zlib | [crates.io](https://crates.io/crates/zune-jpeg/0.5.15) |
| zxcvbn | 3.1.1 | MIT | [crates.io](https://crates.io/crates/zxcvbn/3.1.1) |
## Assets

Safay's app and menu bar icons are the project's own artwork. Atkinson Hyperlegible Mono is redistributed under OFL 1.1, with the complete license in assets/fonts and in development packages. No remote icon service or stock imagery is used.
