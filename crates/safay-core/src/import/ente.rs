//! Ente Auth encrypted exports (version 1).
//!
//! `kdfParams` gives libsodium `crypto_pwhash` parameters (Argon2id, `memLimit` in bytes, `opsLimit` passes, one lane,
//! Base64 `salt`) for a 32-byte key. `encryptedData` is one `crypto_secretstream_xchacha20poly1305` message whose stream
//! header is `encryptionNonce`; the plaintext is one `otpauth://` link per line, read as [`super::text`] reads Ente's
//! plain export.

use chacha20::cipher::{KeyIvInit, StreamCipher, StreamCipherSeek};
use chacha20::{ChaCha20, hchacha};
use poly1305::Poly1305;
use poly1305::universal_hash::KeyInit;
use serde_json::{Map, Value};
use zeroize::Zeroizing;

use super::{Collector, ImportError, base64_field, int_field};
use crate::vault;

/// Ente derives with 256 MiB × 16 passes (128 MiB × 32 on devices short of memory); older versions used libsodium's
/// sensitive limits (1 GiB × 4). Anything costlier than 1 GiB of memory or 4 GiB·passes in total is refused.
const MAX_MEMORY_BYTES: u64 = 1 << 30;
const MAX_MEMORY_PASSES: u64 = 4 << 30;
const MAX_PASSES: u64 = 64;

const HEADER_BYTES: usize = 24;
const TAG_MESSAGE: u8 = 0x00;
const TAG_FINAL: u8 = 0x03;

pub(super) fn detect(root: &Map<String, Value>) -> bool {
    root.get("kdfParams").is_some_and(Value::is_object)
        && root.get("encryptedData").is_some_and(Value::is_string)
        && root.get("encryptionNonce").is_some_and(Value::is_string)
}

pub(super) fn import(root: &Map<String, Value>, password: &str, collector: &mut Collector<'_>) -> Result<(), ImportError> {
    if int_field(root, "version") != Some(1) {
        return Err(ImportError::InvalidFile);
    }
    let Some(Value::Object(kdf)) = root.get("kdfParams") else { return Err(ImportError::InvalidFile) };
    let (Some(memory), Some(passes)) = (int_field(kdf, "memLimit"), int_field(kdf, "opsLimit")) else {
        return Err(ImportError::InvalidFile);
    };
    let (Ok(memory), Ok(passes)) = (u64::try_from(memory), u64::try_from(passes)) else { return Err(ImportError::InvalidFile) };
    if !(8 * 1024..=MAX_MEMORY_BYTES).contains(&memory) || !(1..=MAX_PASSES).contains(&passes) || memory * passes > MAX_MEMORY_PASSES {
        return Err(ImportError::InvalidFile);
    }
    let salt = base64_field(kdf, "salt")?;
    let header = base64_field(root, "encryptionNonce")?;
    let sealed = base64_field(root, "encryptedData")?;
    if header.len() != HEADER_BYTES {
        return Err(ImportError::InvalidFile);
    }
    let key =
        vault::argon2id_raw(password.as_bytes(), &salt, passes as u32, (memory / 1024) as u32, 1, 32).ok_or(ImportError::InvalidFile)?;
    let (plaintext, tag) = secretstream_pull(&key, &header, &sealed).ok_or(ImportError::WrongPassword)?;
    if tag != TAG_FINAL && tag != TAG_MESSAGE {
        return Err(ImportError::InvalidFile);
    }
    let text = std::str::from_utf8(&plaintext).map_err(|_| ImportError::InvalidFile)?;
    super::text::import(text, collector)
}

/// Decrypts the first message of a libsodium `crypto_secretstream_xchacha20poly1305` stream: returns the plaintext and
/// its tag, or `None` when authentication fails.
pub(super) fn secretstream_pull(key: &[u8], header: &[u8], sealed: &[u8]) -> Option<(Zeroizing<Vec<u8>>, u8)> {
    if key.len() != 32 || header.len() != HEADER_BYTES || sealed.len() < 1 + 16 {
        return None;
    }
    let message_len = sealed.len() - 1 - 16;
    let subkey = Zeroizing::new(hchacha::<chacha20::R20>(key.try_into().ok()?, header[..16].try_into().ok()?));
    let mut nonce = [0u8; 12];
    nonce[0] = 1;
    nonce[4..].copy_from_slice(&header[16..24]);
    let stream = || ChaCha20::new(subkey.as_slice().try_into().expect("32-byte key"), (&nonce).into());

    // Block 0 keys Poly1305; block 1 encrypts the tag byte; the message starts at block 2.
    let mut mac_key = Zeroizing::new([0u8; 64]);
    stream().apply_keystream(mac_key.as_mut_slice());
    let mut tag_block = [0u8; 64];
    tag_block[0] = sealed[0];
    let mut cipher = stream();
    cipher.seek(64u64);
    cipher.apply_keystream(&mut tag_block);
    let tag = tag_block[0];
    tag_block[0] = sealed[0];

    // No associated data. libsodium pads the ciphertext by (0x10 - 64 + mlen) & 0xf bytes, which is mlen mod 16.
    let ciphertext = &sealed[1..1 + message_len];
    let mut authenticated = Vec::with_capacity(64 + message_len + 16 + 16);
    authenticated.extend_from_slice(&tag_block);
    authenticated.extend_from_slice(ciphertext);
    authenticated.resize(authenticated.len() + (message_len & 0xf), 0);
    authenticated.extend_from_slice(&0u64.to_le_bytes());
    authenticated.extend_from_slice(&((64 + message_len) as u64).to_le_bytes());
    let expected = Poly1305::new(mac_key[..32].try_into().ok()?).compute_unpadded(&authenticated);
    let stored = &sealed[1 + message_len..];
    if !constant_time_eq(expected.as_slice(), stored) {
        return None;
    }

    let mut plaintext = Zeroizing::new(ciphertext.to_vec());
    let mut cipher = stream();
    cipher.seek(128u64);
    cipher.apply_keystream(&mut plaintext);
    Some((plaintext, tag))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len() && left.iter().zip(right).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}
