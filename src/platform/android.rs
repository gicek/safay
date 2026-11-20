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
static BRIDGE: OnceLock<GlobalRef> = OnceLock::new();

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

/// Runs `f` on this thread, attached to the VM for as long as it lives. `None` when the VM or a call fails; a pending
/// Java exception is cleared so the next call starts clean.
fn with_env<R>(f: impl FnOnce(&mut JNIEnv<'_>) -> jni::errors::Result<R>) -> Option<R> {
    let mut env = vm()?.attach_current_thread_as_daemon().ok()?;
    let result = f(&mut env);
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    result.ok()
}

/// `com.gicek.safay.Bridge`, found through the application's class loader: a thread Rust started sees only the
/// system's classes through `FindClass`.
fn bridge<'a>(env: &mut JNIEnv<'a>) -> jni::errors::Result<JClass<'a>> {
    if let Some(class) = BRIDGE.get() {
        return Ok(JClass::from(env.new_local_ref(class.as_obj())?));
    }
    let application = application(env)?;
    let loader = env.call_method(&application, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?.l()?;
    let name = env.new_string("com.gicek.safay.Bridge")?;
    let class = env.call_method(&loader, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;", &[JValue::Object(&name)])?.l()?;
    let _ = BRIDGE.set(env.new_global_ref(&class)?);
    Ok(JClass::from(class))
}
fn application<'a>(env: &mut JNIEnv<'a>) -> jni::errors::Result<JObject<'a>> {
    let thread = env.find_class("android/app/ActivityThread")?;
    env.call_static_method(thread, "currentApplication", "()Landroid/app/Application;", &[])?.l()
}

/// Starts the bridge: it follows the activity in front and tells when the screen goes off, which locks `controller`'s
/// vault at once.
pub fn start(controller: Arc<Controller>) {
    if CONTROLLER.set(controller).is_err() {
        return;
    }
    with_env(|env| {
        let application = application(env)?;
        let bridge = bridge(env)?;
        env.call_static_method(bridge, "start", "(Landroid/app/Application;)V", &[JValue::Object(&application)])?;
        Ok(())
    });
}

/// Keeps the app's windows out of screenshots, screen recordings and the recent apps' pictures while `on`.
pub fn set_secure(on: bool) {
    with_env(|env| {
        let bridge = bridge(env)?;
        env.call_static_method(bridge, "setSecure", "(Z)V", &[JValue::Bool(on.into())])?;
        Ok(())
    });
}

/// Puts `text` on the clipboard marked sensitive, to be taken off again after `ttl`. Whether the call was made.
pub fn copy_sensitive(text: &str, ttl: std::time::Duration) -> bool {
    with_env(|env| {
        let bridge = bridge(env)?;
        let text = env.new_string(text)?;
        let ttl = i64::try_from(ttl.as_millis()).unwrap_or(i64::MAX);
        env.call_static_method(bridge, "copySensitive", "(Ljava/lang/String;J)V", &[JValue::Object(&text), JValue::Long(ttl)])?;
        Ok(())
    })
    .is_some()
}

/// `Bridge.screenOff`: the screen went off.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_gicek_safay_Bridge_screenOff(_env: JNIEnv<'_>, _class: JClass<'_>) {
    if let Some(controller) = CONTROLLER.get() {
        controller.request_lock();
    }
}
