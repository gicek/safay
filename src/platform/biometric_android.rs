//! Unlocking with a fingerprint or a face on Android: an RSA key in the hardware keystore wraps the 32-byte vault key
//! (`com.gicek.safay.Biometric`). Wrapping asks nothing; every unwrap needs a strong biometric on the spot, and a change
//! of the enrolled biometrics ends the key for good. What the prompt releases is the key itself: unlocking still opens
//! the vault with it, never trusting a "yes".
use jni::{
    JNIEnv,
    objects::{JByteArray, JClass, JValue},
};
use safay_app::Zeroizing;
use std::{
    collections::HashMap,
    sync::{
        Mutex,
        atomic::{AtomicI64, Ordering},
        mpsc::{Sender, channel},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    Unavailable,
    /// Cancelled by the user, the app or the system: nothing else should appear.
    Cancelled,
    /// The prompt's "Use Master Password": the master password takes over.
    Fallback,
    Failed,
    Lockout,
    /// The keystore key is gone (the enrolled biometrics changed) or the file is damaged: delete it and use the password.
    Stale,
}

type Answer = Result<Zeroizing<Vec<u8>>, Failure>;
static WAITING: Mutex<Option<HashMap<i64, Sender<Answer>>>> = Mutex::new(None);
static NEXT: AtomicI64 = AtomicI64::new(1);

fn class<'a>(env: &mut JNIEnv<'a>) -> jni::errors::Result<JClass<'a>> {
    super::android::class(env, "com.gicek.safay.Biometric")
}

/// Whether a strong biometric is enrolled and usable.
pub fn available() -> bool {
    super::android::with_env(|env| {
        let class = class(env)?;
        env.call_static_method(class, "available", "()Z", &[])?.z()
    })
    .unwrap_or(false)
}

/// Wraps the vault key with a new keystore key; the file to keep.
pub fn seal(vault_key: &[u8]) -> Result<Vec<u8>, Failure> {
    super::android::with_env(|env| {
        let class = class(env)?;
        let key = env.byte_array_from_slice(vault_key)?;
        let sealed = env.call_static_method(class, "seal", "([B)[B", &[JValue::Object(&key)]).and_then(|v| v.l());
        super::android::wipe(env, &key);
        let sealed = sealed?;
        if sealed.is_null() {
            return Ok(None);
        }
        Ok(Some(env.convert_byte_array(JByteArray::from(sealed))?))
    })
    .flatten()
    .ok_or(Failure::Unavailable)
}

/// Asks for the biometric and unwraps `file`. Blocks until the prompt is answered; never call it on the interface's
/// thread.
pub fn open(file: &[u8], reason: &str, cancel_title: &str) -> Result<Zeroizing<Vec<u8>>, Failure> {
    let token = NEXT.fetch_add(1, Ordering::Relaxed);
    let (sender, receiver) = channel();
    WAITING.lock().map_err(|_| Failure::Failed)?.get_or_insert_with(HashMap::new).insert(token, sender);
    let asked = super::android::with_env(|env| {
        let class = class(env)?;
        let sealed = env.byte_array_from_slice(file)?;
        let title = env.new_string(reason)?;
        let cancel = env.new_string(cancel_title)?;
        env.call_static_method(
            class,
            "open",
            "([BLjava/lang/String;Ljava/lang/String;J)V",
            &[JValue::Object(&sealed), JValue::Object(&title), JValue::Object(&cancel), JValue::Long(token)],
        )?;
        Ok(())
    });
    if asked.is_none() {
        if let Ok(mut waiting) = WAITING.lock() {
            waiting.as_mut().and_then(|w| w.remove(&token));
        }
        return Err(Failure::Unavailable);
    }
    receiver.recv().unwrap_or(Err(Failure::Failed))
}

/// Forgets the keystore key (the wrapped file is deleted by the caller).
pub fn forget() {
    super::android::with_env(|env| {
        let class = class(env)?;
        env.call_static_method(class, "forget", "()V", &[])?;
        Ok(())
    });
}

/// `Biometric.result`: the prompt's answer.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_gicek_safay_Biometric_result(
    env: JNIEnv<'_>,
    _class: JClass<'_>,
    token: i64,
    code: i32,
    key: JByteArray<'_>,
) {
    let answer = match code {
        0 if !key.is_null() => env.convert_byte_array(key).map(Zeroizing::new).map_err(|_| Failure::Failed),
        1 => Err(Failure::Unavailable),
        2 => Err(Failure::Cancelled),
        3 => Err(Failure::Fallback),
        5 => Err(Failure::Lockout),
        6 => Err(Failure::Stale),
        _ => Err(Failure::Failed),
    };
    if let Some(sender) = WAITING.lock().ok().and_then(|mut waiting| waiting.as_mut().and_then(|w| w.remove(&token))) {
        let _ = sender.send(answer);
    }
}
