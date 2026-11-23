//! Android services NSH has no API for, through JNI: a clipboard entry marked sensitive that expires, windows kept out
//! of screenshots, and the lock when the screen goes off. APIs that take a class of the app go through
//! `com.gicek.safay.Bridge` (`android/java`), which the package carries as its own dex.
use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JClass, JObject, JValue},
};
use safay_app::controller::Controller;
use std::sync::{Arc, OnceLock};

static CONTROLLER: OnceLock<Arc<Controller>> = OnceLock::new();
static DIRECTORY: OnceLock<std::path::PathBuf> = OnceLock::new();

/// The app's own directory (the vault's): what [`start`] was given.
pub(super) fn private_directory() -> Option<&'static std::path::Path> {
    DIRECTORY.get().map(std::path::PathBuf::as_path)
}

/// The process's Java VM. NSH's runner is started by it and keeps its handle to itself, so it is asked of the runtime:
/// `JNI_GetCreatedJavaVMs`, in libnativehelper from Android 12 and in libart before.
fn vm() -> Option<&'static JavaVM> {
    static VM: OnceLock<Option<JavaVM>> = OnceLock::new();
    VM.get_or_init(|| {
        type Created = unsafe extern "C" fn(*mut *mut jni::sys::JavaVM, jni::sys::jsize, *mut jni::sys::jsize) -> jni::sys::jint;
        for library in [c"libnativehelper.so", c"libart.so"] {
            // SAFETY: a system library that is loaded in every app process; the symbol has this signature in the JNI
            // invocation API, and it writes at most the one pointer it is given room for.
            unsafe {
                let handle = libc::dlopen(library.as_ptr(), libc::RTLD_NOW);
                if handle.is_null() {
                    continue;
                }
                let symbol = libc::dlsym(handle, c"JNI_GetCreatedJavaVMs".as_ptr());
                if symbol.is_null() {
                    continue;
                }
                let created: Created = std::mem::transmute(symbol);
                let (mut raw, mut count) = (std::ptr::null_mut(), 0);
                if created(&mut raw, 1, &mut count) == 0 && count == 1 && !raw.is_null() {
                    return JavaVM::from_raw(raw).ok();
                }
            }
        }
        None
    })
    .as_ref()
}

/// Runs `f` on this thread, attached to the VM for as long as it lives, in a frame of local references of its own: the
/// thread stays attached, so the references a call makes would otherwise pile up. `None` when the VM or a call fails; a
/// pending Java exception is cleared so the next call starts clean.
pub(super) fn with_env<R>(f: impl FnOnce(&mut JNIEnv<'_>) -> jni::errors::Result<R>) -> Option<R> {
    let mut env = vm()?.attach_current_thread_as_daemon().ok()?;
    let result = env.with_local_frame(32, |env| f(env));
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    result.ok()
}

/// A class of the app's own dex (`com.gicek.safay.…`), found once through the application's class loader and kept: a
/// thread Rust started sees only the system's classes through `FindClass`.
pub(super) fn class<'a>(env: &mut JNIEnv<'a>, name: &'static str) -> jni::errors::Result<JClass<'a>> {
    static CLASSES: std::sync::Mutex<Vec<(&str, GlobalRef)>> = std::sync::Mutex::new(Vec::new());
    let known = CLASSES.lock().ok().and_then(|classes| classes.iter().find(|(n, _)| *n == name).map(|(_, c)| c.clone()));
    if let Some(class) = known {
        return Ok(JClass::from(env.new_local_ref(class.as_obj())?));
    }
    let found = load_class(env, name)?;
    if let Ok(mut classes) = CLASSES.lock() {
        classes.push((name, env.new_global_ref(&found)?));
    }
    Ok(found)
}
fn load_class<'a>(env: &mut JNIEnv<'a>, name: &str) -> jni::errors::Result<JClass<'a>> {
    let application = application(env)?;
    let loader = env.call_method(&application, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?.l()?;
    let name = env.new_string(name)?;
    let class = env.call_method(&loader, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;", &[JValue::Object(&name)])?.l()?;
    Ok(JClass::from(class))
}
fn application<'a>(env: &mut JNIEnv<'a>) -> jni::errors::Result<JObject<'a>> {
    let thread = env.find_class("android/app/ActivityThread")?;
    env.call_static_method(thread, "currentApplication", "()Landroid/app/Application;", &[])?.l()
}

/// Starts the bridge: it follows the activity in front and tells when the screen goes off, which locks `controller`'s
/// vault at once.
pub fn start(controller: Arc<Controller>, directory: std::path::PathBuf) {
    if CONTROLLER.set(controller).is_err() {
        return;
    }
    let _ = DIRECTORY.set(directory);
    with_env(|env| {
        let application = application(env)?;
        let bridge = class(env, "com.gicek.safay.Bridge")?;
        env.call_static_method(bridge, "start", "(Landroid/app/Application;)V", &[JValue::Object(&application)])?;
        Ok(())
    });
}

/// Keeps the app's windows out of screenshots, screen recordings and the recent apps' pictures while `on`.
pub fn set_secure(on: bool) {
    with_env(|env| {
        let bridge = class(env, "com.gicek.safay.Bridge")?;
        env.call_static_method(bridge, "setSecure", "(Z)V", &[JValue::Bool(on.into())])?;
        Ok(())
    });
}

/// Puts `text` on the clipboard marked sensitive, to be taken off again after `ttl`. False when it can't.
pub fn copy_sensitive(text: &str, ttl: std::time::Duration) -> bool {
    with_env(|env| {
        let bridge = class(env, "com.gicek.safay.Bridge")?;
        let text = env.new_string(text)?;
        let ttl = i64::try_from(ttl.as_millis()).unwrap_or(i64::MAX);
        env.call_static_method(bridge, "copySensitive", "(Ljava/lang/String;J)Z", &[JValue::Object(&text), JValue::Long(ttl)])?.z()
    })
    .unwrap_or(false)
}

/// `Bridge.screenOff`: the screen went off.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_gicek_safay_Bridge_screenOff(_env: JNIEnv<'_>, _class: JClass<'_>) {
    if let Some(controller) = CONTROLLER.get() {
        controller.request_lock();
    }
}

/// Seals `plain` with the keystore's key for cloud sign-ins (`Secrets.seal`).
pub(super) fn seal(plain: &[u8]) -> Option<Vec<u8>> {
    secrets("seal", plain).map(|sealed| sealed.to_vec())
}
/// Opens what [`seal`] sealed on this device.
pub(super) fn unseal(sealed: &[u8]) -> Option<safay_app::Zeroizing<Vec<u8>>> {
    secrets("open", sealed)
}
fn secrets(method: &str, input: &[u8]) -> Option<safay_app::Zeroizing<Vec<u8>>> {
    with_env(|env| {
        let class = class(env, "com.gicek.safay.Secrets")?;
        let input = env.byte_array_from_slice(input)?;
        let output = env.call_static_method(class, method, "([B)[B", &[JValue::Object(&input)]).and_then(|v| v.l());
        wipe(env, &input);
        let output = jni::objects::JByteArray::from(output?);
        if output.is_null() {
            return Ok(None);
        }
        let read = env.convert_byte_array(&output).map(safay_app::Zeroizing::new);
        wipe(env, &output);
        Ok(Some(read?))
    })
    .flatten()
}

/// Writes zeros over a Java array that held a secret: Java's copy would otherwise stay in its heap until collected.
pub(super) fn wipe(env: &mut JNIEnv<'_>, array: &jni::objects::JByteArray<'_>) {
    if let Ok(length) = env.get_array_length(array) {
        let _ = env.set_byte_array_region(array, 0, &vec![0_i8; length as usize]);
    }
}

/// Opens `url` in a Custom Tab over Safay (the browser's own tab for an app, with its cookies). Whether it opened.
pub fn open_custom_tab(url: &str) -> bool {
    with_env(|env| {
        let bridge = class(env, "com.gicek.safay.Bridge")?;
        let url = env.new_string(url)?;
        env.call_static_method(bridge, "openCustomTab", "(Ljava/lang/String;)Z", &[JValue::Object(&url)])?.z()
    })
    .unwrap_or(false)
}
