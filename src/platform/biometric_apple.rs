//! Touch ID and Face ID unlock on macOS and iOS: a P-256 key generated in the Secure Enclave with
//! `privateKeyUsage | biometryCurrentSet` wraps the 32-byte vault key (ECIES, X9.63 SHA-256, AES-GCM). Only the enclave's
//! key handle (usable by this device's enclave alone) and the ciphertext are stored. Every unwrap needs the biometric on
//! the spot, and enrolling or removing a finger or face invalidates the handle for good. No Keychain item, so ad-hoc
//! builds work too. What the biometric releases is the key itself: unlocking still opens the vault with it, never
//! trusting a "yes".
use base64::Engine;
use objc2::{
    msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, Bool},
};
use objc2_foundation::{NSData, NSError, NSMutableDictionary, NSNumber, NSString};
use safay_app::Zeroizing;
use std::ffi::c_void;

type Cf = *const c_void;
#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    static kSecAttrKeyType: Cf;
    static kSecAttrKeyTypeECSECPrimeRandom: Cf;
    static kSecAttrKeySizeInBits: Cf;
    static kSecAttrTokenID: Cf;
    static kSecAttrTokenIDSecureEnclave: Cf;
    static kSecPrivateKeyAttrs: Cf;
    static kSecAttrIsPermanent: Cf;
    static kSecAttrAccessControl: Cf;
    static kSecAttrKeyClass: Cf;
    static kSecAttrKeyClassPrivate: Cf;
    static kSecUseAuthenticationContext: Cf;
    static kSecAttrAccessibleWhenUnlockedThisDeviceOnly: Cf;
    static kSecKeyAlgorithmECIESEncryptionCofactorVariableIVX963SHA256AESGCM: Cf;
    fn SecAccessControlCreateWithFlags(alloc: Cf, protection: Cf, flags: usize, error: *mut *mut c_void) -> *mut AnyObject;
    fn SecKeyCreateRandomKey(parameters: Cf, error: *mut *mut c_void) -> *mut AnyObject;
    fn SecKeyCopyAttributes(key: Cf) -> *mut AnyObject;
    fn SecKeyCopyPublicKey(key: Cf) -> *mut AnyObject;
    fn SecKeyCreateEncryptedData(key: Cf, algorithm: Cf, plaintext: Cf, error: *mut *mut c_void) -> *mut AnyObject;
    fn SecKeyCreateDecryptedData(key: Cf, algorithm: Cf, ciphertext: Cf, error: *mut *mut c_void) -> *mut AnyObject;
    fn SecKeyCreateWithData(data: Cf, attributes: Cf, error: *mut *mut c_void) -> *mut AnyObject;
}
#[link(name = "LocalAuthentication", kind = "framework")]
unsafe extern "C" {}

/// `kSecAttrTokenOID`: the enclave key's opaque handle (not exported as a symbol).
const TOKEN_OID: &str = "toid";
const PRIVATE_KEY_USAGE: usize = 1 << 30;
const BIOMETRY_CURRENT_SET: usize = 1 << 3;
/// `LAPolicyDeviceOwnerAuthenticationWithBiometrics`.
const BIOMETRICS: isize = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    Unavailable,
    /// Cancelled by the user, the app or the system: nothing else should appear.
    Cancelled,
    /// The prompt's "Use Password…": the master password takes over.
    Fallback,
    Failed,
    Lockout,
    /// The handle no longer opens (fingerprints changed) or the file is damaged: delete it and use the password.
    Stale,
}

impl Failure {
    /// From an `LAError` code: -2 user cancel, -4 system cancel, -9 app cancel, -3 "Use Password…", -8 lockout, -5…-7
    /// not available, anything else failed.
    fn of(code: isize) -> Self {
        match code {
            -2 | -4 | -9 => Failure::Cancelled,
            -3 => Failure::Fallback,
            -8 => Failure::Lockout,
            -7..=-5 => Failure::Unavailable,
            _ => Failure::Failed,
        }
    }
}

unsafe fn retained(ptr: *mut AnyObject) -> Option<Retained<AnyObject>> {
    unsafe { Retained::from_raw(ptr) }
}
fn key(s: Cf) -> &'static NSString {
    unsafe { &*(s as *const NSString) }
}
fn ns(s: &str) -> Retained<NSString> {
    NSString::from_str(s)
}
fn insert(dict: &NSMutableDictionary<NSString, AnyObject>, k: &NSString, v: &AnyObject) {
    let _: () = unsafe { msg_send![dict, setObject: v, forKey: k] };
}

fn context() -> Option<Retained<AnyObject>> {
    let class = AnyClass::get(c"LAContext")?;
    let ctx: *mut AnyObject = unsafe { msg_send![class, new] };
    unsafe { retained(ctx) }
}
/// Touch ID or Face ID is present and enrolled.
pub fn available() -> bool {
    let Some(ctx) = context() else { return false };
    let ok: Bool = unsafe { msg_send![&*ctx, canEvaluatePolicy: BIOMETRICS, error: std::ptr::null_mut::<*mut NSError>()] };
    ok.as_bool()
}
/// Whether this device's biometric is Face ID (or Optic ID, named the same way) rather than Touch ID
/// (`LABiometryType`, known once a policy was evaluated).
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn face() -> bool {
    let Some(ctx) = context() else { return false };
    let _: Bool = unsafe { msg_send![&*ctx, canEvaluatePolicy: BIOMETRICS, error: std::ptr::null_mut::<*mut NSError>()] };
    let kind: isize = unsafe { msg_send![&*ctx, biometryType] };
    kind == 2 || kind == 4
}

/// Wraps the vault key under a new enclave key; returns the file to store (no secret in it).
pub fn seal(vault_key: &[u8]) -> Result<Vec<u8>, Failure> {
    unsafe {
        let access = retained(SecAccessControlCreateWithFlags(
            std::ptr::null(),
            kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
            PRIVATE_KEY_USAGE | BIOMETRY_CURRENT_SET,
            std::ptr::null_mut(),
        ))
        .ok_or(Failure::Unavailable)?;
        let private_attrs = NSMutableDictionary::<NSString, AnyObject>::new();
        insert(&private_attrs, key(kSecAttrIsPermanent), &NSNumber::new_bool(false));
        insert(&private_attrs, key(kSecAttrAccessControl), &access);
        let params = NSMutableDictionary::<NSString, AnyObject>::new();
        insert(&params, key(kSecAttrKeyType), &*(kSecAttrKeyTypeECSECPrimeRandom as *const AnyObject));
        insert(&params, key(kSecAttrKeySizeInBits), &NSNumber::new_i32(256));
        insert(&params, key(kSecAttrTokenID), &*(kSecAttrTokenIDSecureEnclave as *const AnyObject));
        insert(&params, key(kSecPrivateKeyAttrs), &private_attrs);
        let private =
            retained(SecKeyCreateRandomKey(Retained::as_ptr(&params).cast(), std::ptr::null_mut())).ok_or(Failure::Unavailable)?;
        let attrs = retained(SecKeyCopyAttributes(Retained::as_ptr(&private).cast())).ok_or(Failure::Unavailable)?;
        let handle: *mut AnyObject = msg_send![&*attrs, objectForKey: &*ns(TOKEN_OID)];
        let handle = (handle as *const NSData).as_ref().ok_or(Failure::Unavailable)?.to_vec();
        let public = retained(SecKeyCopyPublicKey(Retained::as_ptr(&private).cast())).ok_or(Failure::Unavailable)?;
        let plain = NSData::with_bytes(vault_key);
        let sealed = retained(SecKeyCreateEncryptedData(
            Retained::as_ptr(&public).cast(),
            kSecKeyAlgorithmECIESEncryptionCofactorVariableIVX963SHA256AESGCM,
            Retained::as_ptr(&plain).cast(),
            std::ptr::null_mut(),
        ))
        .ok_or(Failure::Unavailable)?;
        let ciphertext = (&*sealed as *const AnyObject as *const NSData).as_ref().ok_or(Failure::Unavailable)?.to_vec();
        let b64 = base64::engine::general_purpose::STANDARD;
        serde_json::to_vec(&serde_json::json!({ "version": 1, "handle": b64.encode(handle), "ciphertext": b64.encode(ciphertext) }))
            .map_err(|_| Failure::Unavailable)
    }
}

/// Asks for the biometric, then unwraps the vault key with the same authentication (one prompt). Blocks: run off the UI thread.
pub fn open(file: &[u8], reason: &str, cancel_title: &str) -> Result<Zeroizing<Vec<u8>>, Failure> {
    let parsed: serde_json::Value = serde_json::from_slice(file).map_err(|_| Failure::Stale)?;
    let b64 = base64::engine::general_purpose::STANDARD;
    let field = |name: &str| parsed.get(name).and_then(|v| v.as_str()).and_then(|v| b64.decode(v).ok()).ok_or(Failure::Stale);
    let (handle, ciphertext) = (field("handle")?, field("ciphertext")?);
    let ctx = context().ok_or(Failure::Unavailable)?;
    unsafe {
        let _: () = msg_send![&*ctx, setLocalizedCancelTitle: &*ns(cancel_title)];
    }
    let (tx, rx) = std::sync::mpsc::sync_channel::<Result<(), Failure>>(1);
    let reply = block2::RcBlock::new(move |ok: Bool, error: *mut NSError| {
        let result = if ok.as_bool() { Ok(()) } else { Err(Failure::of(unsafe { error.as_ref() }.map(|e| e.code()).unwrap_or(-1))) };
        let _ = tx.send(result);
    });
    unsafe {
        let _: () = msg_send![&*ctx, evaluatePolicy: BIOMETRICS, localizedReason: &*ns(reason), reply: &*reply];
    }
    rx.recv().map_err(|_| Failure::Failed)??;
    unsafe {
        let attrs = NSMutableDictionary::<NSString, AnyObject>::new();
        insert(&attrs, key(kSecAttrKeyType), &*(kSecAttrKeyTypeECSECPrimeRandom as *const AnyObject));
        insert(&attrs, key(kSecAttrKeyClass), &*(kSecAttrKeyClassPrivate as *const AnyObject));
        insert(&attrs, key(kSecAttrTokenID), &*(kSecAttrTokenIDSecureEnclave as *const AnyObject));
        insert(&attrs, &ns(TOKEN_OID), &NSData::with_bytes(&handle));
        insert(&attrs, key(kSecUseAuthenticationContext), &ctx);
        let empty = NSData::new();
        let private =
            retained(SecKeyCreateWithData(Retained::as_ptr(&empty).cast(), Retained::as_ptr(&attrs).cast(), std::ptr::null_mut()))
                .ok_or(Failure::Stale)?;
        let sealed = NSData::with_bytes(&ciphertext);
        let plain = retained(SecKeyCreateDecryptedData(
            Retained::as_ptr(&private).cast(),
            kSecKeyAlgorithmECIESEncryptionCofactorVariableIVX963SHA256AESGCM,
            Retained::as_ptr(&sealed).cast(),
            std::ptr::null_mut(),
        ))
        .ok_or(Failure::Stale)?;
        let bytes = Zeroizing::new((&*plain as *const AnyObject as *const NSData).as_ref().ok_or(Failure::Stale)?.to_vec());
        if bytes.len() != 32 {
            return Err(Failure::Stale);
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::Failure;
    #[test]
    fn cancelling_is_not_using_the_password() {
        assert_eq!(Failure::of(-2), Failure::Cancelled);
        assert_eq!(Failure::of(-4), Failure::Cancelled);
        assert_eq!(Failure::of(-9), Failure::Cancelled);
        assert_eq!(Failure::of(-3), Failure::Fallback);
        assert_eq!(Failure::of(-8), Failure::Lockout);
        assert_eq!(Failure::of(-1), Failure::Failed);
    }
    /// Creates a real (non-persistent) enclave key; needs Apple silicon. Unwrapping needs a finger, so it is manual.
    #[test]
    #[ignore = "uses the Secure Enclave"]
    fn sealing_produces_a_handle_and_ciphertext_without_the_key() {
        let file = super::seal(&[7u8; 32]).expect("enclave available");
        let text = String::from_utf8(file).unwrap();
        assert!(text.contains("handle") && text.contains("ciphertext"));
        assert!(!text.contains("BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc"));
    }
}
