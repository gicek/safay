//! Android camera scanning for the phone's "Scan QR Code" (NSH has no camera API). `com.gicek.safay.Camera`
//! (`android/java`) runs the back camera through Camera2: its preview goes straight into the `TextureView` of the
//! native component `safay.camera` ([`register`]), drawn by Android, and it keeps the latest frame's luma, which a
//! thread here reads QR codes from with `rqrr`. The UI polls `poll()`; nothing is written to disk and `stop()` ends it
//! all. The camera permission is asked for with the system's prompt the first time.
use super::android::{class, with_env};
use jni::{
    JNIEnv,
    objects::{GlobalRef, JByteArray, JClass, JValue},
};
use safay_app::Zeroizing;
use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

const PERMISSION: &str = "android.permission.CAMERA";
/// `Camera.state()`.
const RUNNING: i32 = 1;
const FAILED: i32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraError {
    /// The user declined camera access (Settings → Apps → Safay → Permissions).
    Denied,
    /// No camera, it is in use, or it could not be opened.
    Unavailable,
}

/// The native component that shows the camera's preview: place it with `native(PREVIEW)`.
pub const PREVIEW: &str = "safay.camera";

/// What the UI reads each tick.
#[derive(Default)]
pub struct Poll {
    /// A preview to show as an image: never here, where the preview is the native component [`PREVIEW`].
    pub frame: Option<String>,
    /// Frames are arriving.
    pub ready: bool,
    pub error: Option<CameraError>,
    /// A QR code's text, handed over once.
    pub result: Option<Zeroizing<String>>,
}

#[derive(Default)]
struct Shared {
    frame: Option<String>,
    ready: bool,
    error: Option<CameraError>,
    result: Option<Zeroizing<String>>,
    last_text: Option<Zeroizing<String>>,
}

static SHARED: Mutex<Option<Shared>> = Mutex::new(None);
/// Which session is current: a stopped one's thread sees a newer number and ends.
static SESSION: AtomicU64 = AtomicU64::new(0);
static RUNNING_THREAD: AtomicBool = AtomicBool::new(false);

/// `com.gicek.safay.Camera`.
fn camera<'a>(env: &mut JNIEnv<'a>) -> jni::errors::Result<JClass<'a>> {
    class(env, "com.gicek.safay.Camera")
}

fn with_shared<R>(f: impl FnOnce(&mut Shared) -> R) -> R {
    f(SHARED.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Shared::default))
}

/// Asks for the camera permission if need be, then opens the camera and reads its frames until [`stop`].
pub fn start() {
    with_shared(|s| *s = Shared::default());
    let session = SESSION.fetch_add(1, Ordering::SeqCst) + 1;
    let asked = nsh::permissions::request(&[PERMISSION]);
    std::thread::spawn(move || {
        use nsh::permissions::Permission;
        let granted = matches!(block_on(asked).as_deref(), Ok([Permission::Granted]));
        if SESSION.load(Ordering::SeqCst) != session {
            return;
        }
        if !granted {
            with_shared(|s| s.error = Some(CameraError::Denied));
            return;
        }
        let opened = with_env(|env| {
            let camera = camera(env)?;
            env.call_static_method(camera, "open", "()V", &[])?;
            Ok(())
        });
        if opened.is_none() {
            with_shared(|s| s.error = Some(CameraError::Unavailable));
            return;
        }
        // Left while it opened: what `stop` closed has just been opened again.
        if SESSION.load(Ordering::SeqCst) != session {
            close();
            return;
        }
        // One reader at a time: a session started again waits for the last one's thread to see it ended.
        while RUNNING_THREAD.swap(true, Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(20));
        }
        read(session);
        RUNNING_THREAD.store(false, Ordering::SeqCst);
    });
}

/// Stops the camera and forgets what it saw.
pub fn stop() {
    SESSION.fetch_add(1, Ordering::SeqCst);
    with_shared(|s| *s = Shared::default());
    close();
}

fn close() {
    with_env(|env| {
        let camera = camera(env)?;
        env.call_static_method(camera, "close", "()V", &[])?;
        Ok(())
    });
}

pub fn poll() -> Poll {
    with_shared(|s| Poll { frame: s.frame.clone(), ready: s.ready, error: s.error, result: s.result.take() })
}

/// The frames of `session`, until it is no longer the current one.
fn read(session: u64) {
    let mut frames = 0_u64;
    while SESSION.load(Ordering::SeqCst) == session {
        // `with_env` gives each round its own frame of local references (each frame's array among them).
        let taken = with_env(|env| {
            let camera = camera(env)?;
            let state = env.call_static_method(&camera, "state", "()I", &[])?.i()?;
            if state == FAILED {
                return Ok(Err(()));
            }
            if state != RUNNING {
                return Ok(Ok(None));
            }
            let w = env.call_static_method(&camera, "width", "()I", &[])?.i()?.max(0) as usize;
            let h = env.call_static_method(&camera, "height", "()I", &[])?.i()?.max(0) as usize;
            let luma = env.call_static_method(&camera, "takeLuma", "()[B", &[])?.l()?;
            if luma.is_null() {
                return Ok(Ok(Some(None)));
            }
            Ok(Ok(Some(Some((Zeroizing::new(env.convert_byte_array(JByteArray::from(luma))?), w, h)))))
        });
        let luma = match taken {
            Some(Ok(Some(luma))) => {
                with_shared(|s| s.ready = true);
                luma
            }
            Some(Ok(None)) => None,
            None | Some(Err(())) => {
                if SESSION.load(Ordering::SeqCst) == session {
                    with_shared(|s| s.error = Some(CameraError::Unavailable));
                }
                return;
            }
        };
        if let Some((luma, w, h)) = luma
            && w > 0
            && h > 0
            && luma.len() >= w * h
        {
            frames += 1;
            if frames.is_multiple_of(2) {
                decode(&luma[..w * h], w, h);
            }
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}

/// Reads a QR code from the luma; a text is handed over once, until another one shows.
fn decode(luma: &[u8], w: usize, h: usize) {
    let Some(gray) = image::GrayImage::from_raw(w as u32, h as u32, luma.to_vec()) else { return };
    let mut prepared = rqrr::PreparedImage::prepare(gray);
    for grid in prepared.detect_grids() {
        if let Ok((_, text)) = grid.decode() {
            let text = Zeroizing::new(text);
            with_shared(|s| {
                if s.last_text.as_ref() != Some(&text) {
                    s.last_text = Some(text.clone());
                    s.result = Some(text);
                }
            });
            return;
        }
    }
}

/// Waits for `future` on this thread: the permission's answer arrives from Android's main thread.
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    use std::task::{Context, Poll as TaskPoll, Wake, Waker};
    struct Thread(std::thread::Thread);
    impl Wake for Thread {
        fn wake(self: std::sync::Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(std::sync::Arc::new(Thread(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            TaskPoll::Ready(output) => return output,
            TaskPoll::Pending => std::thread::park(),
        }
    }
}

/// Registers [`PREVIEW`]: a `TextureView` the camera draws its preview into (`Camera.view`).
pub fn register(components: &nsh::Components) {
    components.register(
        PREVIEW,
        std::rc::Rc::new(|context: nsh::ComponentContext| -> Box<dyn nsh::NativeComponent> {
            let android = context.android_context();
            let view = context.with_env(|env| {
                let made = camera(env).and_then(|class| {
                    env.call_static_method(
                        class,
                        "view",
                        "(Landroid/content/Context;)Landroid/view/View;",
                        &[JValue::Object(android.as_obj())],
                    )?
                    .l()
                });
                // Without the preview (the app's classes missing from the package), a plain view holds its place: the
                // page says the camera can't be opened.
                let view = match made {
                    Ok(view) => view,
                    Err(_) => {
                        let _ = env.exception_clear();
                        env.new_object("android/view/View", "(Landroid/content/Context;)V", &[JValue::Object(android.as_obj())])?
                    }
                };
                env.new_global_ref(view)
            });
            Box::new(Preview { view: view.ok() })
        }),
    );
}

/// The preview's component: its view only, set up by Java; the camera starts and stops with the page (`start`, `stop`).
struct Preview {
    view: Option<GlobalRef>,
}

impl nsh::NativeComponent for Preview {
    fn view(&self) -> &GlobalRef {
        // The factory makes a plain view when the preview can't be made; only a VM that fails outright leaves none.
        self.view.as_ref().expect("the preview's view")
    }

    fn set_prop(&mut self, _env: &mut JNIEnv<'_>, _name: &str, _value: Option<&nsh_core::JsonValue>) {}

    /// The page is gone: Java lets the view go.
    fn destroy(&mut self, env: &mut JNIEnv<'_>) {
        if let Some(view) = &self.view
            && let Ok(class) = camera(env)
        {
            let _ = env.call_static_method(class, "forget", "(Landroid/view/View;)V", &[JValue::Object(view.as_obj())]);
            let _ = env.exception_clear();
        }
    }
}
