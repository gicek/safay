//! iOS camera scanning for the phone's "Scan QR Code" (NSH has no live camera API). One `AVCaptureSession` runs the
//! back camera on a serial queue of its own: its picture goes straight into the native component `safay.camera`
//! ([`register`]), a view whose layer is an `AVCaptureVideoPreviewLayer`, and its `AVCaptureMetadataOutput` reads QR
//! codes, whose text the UI polls with [`poll`]. Nothing is written to disk and [`stop`] ends it all. The camera
//! permission is asked for with the system's prompt the first time (`NSCameraUsageDescription`, nsh.json).
use objc2::{
    AnyThread, ClassType, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, Bool, NSObject, NSObjectProtocol, ProtocolObject},
};
use objc2_av_foundation::{
    AVAuthorizationStatus, AVCaptureConnection, AVCaptureDevice, AVCaptureDeviceInput, AVCaptureMetadataOutput,
    AVCaptureMetadataOutputObjectsDelegate, AVCaptureOutput, AVCaptureSession, AVCaptureVideoPreviewLayer,
    AVLayerVideoGravityResizeAspectFill, AVMediaTypeVideo, AVMetadataMachineReadableCodeObject, AVMetadataObject,
    AVMetadataObjectTypeQRCode,
};
use objc2_foundation::NSArray;
use objc2_ui_kit::{UIResponder, UIView};
use safay_app::Zeroizing;
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraError {
    /// The user declined camera access (Settings → Safay → Camera).
    Denied,
    /// No camera (the simulator), it is in use, or it could not be opened.
    Unavailable,
}

/// The native component that shows the camera's preview: place it with `native(PREVIEW)`.
pub const PREVIEW: &str = "safay.camera";

/// What the UI reads each tick.
#[derive(Default)]
pub struct Poll {
    /// A preview to show as an image: never here, where the preview is the native component [`PREVIEW`].
    pub frame: Option<String>,
    /// The camera is running.
    pub ready: bool,
    pub error: Option<CameraError>,
    /// A QR code's text, handed over once.
    pub result: Option<Zeroizing<String>>,
}

#[derive(Default)]
struct Shared {
    ready: bool,
    error: Option<CameraError>,
    result: Option<Zeroizing<String>>,
    /// The code in view: it is handed over once while it stays there.
    last_text: Option<Zeroizing<String>>,
}

static SHARED: Mutex<Option<Shared>> = Mutex::new(None);
/// Which start is current: a permission answer or a setup that arrives after [`stop`] sees a newer number.
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn with_shared<R>(f: impl FnOnce(&mut Shared) -> R) -> R {
    f(SHARED.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Shared::default))
}

/// The session and what keeps it reading, made once and kept: the preview and the reader attach to the same one.
struct Capture {
    session: Retained<AVCaptureSession>,
    queue: dispatch2::DispatchRetained<dispatch2::DispatchQueue>,
    /// Whether the camera's input and the reader were added (false where there is no camera).
    configured: bool,
    _delegate: Option<Retained<Reader>>,
}
// The session's methods are safe from any thread; it is only started, stopped and configured on its own queue.
unsafe impl Send for Capture {}

static CAPTURE: Mutex<Option<Capture>> = Mutex::new(None);

fn with_capture<R>(f: impl FnOnce(&mut Capture) -> R) -> R {
    let mut capture = CAPTURE.lock().unwrap_or_else(|e| e.into_inner());
    let capture = capture.get_or_insert_with(|| Capture {
        session: unsafe { AVCaptureSession::new() },
        queue: dispatch2::DispatchQueue::new("com.gicek.safay.camera", None),
        configured: false,
        _delegate: None,
    });
    f(capture)
}

/// Asks for the camera permission if need be, then runs the camera until [`stop`].
pub fn start() {
    with_shared(|s| *s = Shared::default());
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let Some(video) = (unsafe { AVMediaTypeVideo }) else {
        with_shared(|s| s.error = Some(CameraError::Unavailable));
        return;
    };
    match unsafe { AVCaptureDevice::authorizationStatusForMediaType(video) } {
        AVAuthorizationStatus::Authorized => run(generation),
        AVAuthorizationStatus::NotDetermined => {
            let answer = block2::RcBlock::new(move |granted: Bool| {
                if GENERATION.load(Ordering::SeqCst) != generation {
                    return;
                }
                if granted.as_bool() {
                    run(generation);
                } else {
                    with_shared(|s| s.error = Some(CameraError::Denied));
                }
            });
            unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(video, &answer) };
        }
        _ => with_shared(|s| s.error = Some(CameraError::Denied)),
    }
}

/// Configures the session the first time, then starts it on its queue.
fn run(generation: u64) {
    let queue = with_capture(|c| c.queue.clone());
    queue.exec_async(move || {
        if GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        let ok = with_capture(|c| {
            if !c.configured {
                c.configured = configure(c);
            }
            if c.configured {
                unsafe { c.session.startRunning() };
            }
            c.configured
        });
        with_shared(|s| if ok { s.ready = true } else { s.error = Some(CameraError::Unavailable) });
        // Stopped while it started: what `stop` stopped has just been started again.
        if GENERATION.load(Ordering::SeqCst) != generation {
            with_capture(|c| unsafe { c.session.stopRunning() });
        }
    });
}

/// Adds the back camera and a QR reader to the session. False where there is no camera.
fn configure(c: &mut Capture) -> bool {
    unsafe {
        let Some(video) = AVMediaTypeVideo else { return false };
        let Some(device) = AVCaptureDevice::defaultDeviceWithMediaType(video) else { return false };
        let Ok(input) = AVCaptureDeviceInput::deviceInputWithDevice_error(&device) else { return false };
        let session = &c.session;
        session.beginConfiguration();
        if !session.canAddInput(&input) {
            session.commitConfiguration();
            return false;
        }
        session.addInput(&input);
        let output = AVCaptureMetadataOutput::new();
        if !session.canAddOutput(&output) {
            session.commitConfiguration();
            return false;
        }
        session.addOutput(&output);
        let reader = Reader::new();
        output.setMetadataObjectsDelegate_queue(Some(ProtocolObject::from_ref(&*reader)), Some(&c.queue));
        // The types an output reads exist only once it is in a session with a camera.
        output.setMetadataObjectTypes(Some(&NSArray::from_slice(&[AVMetadataObjectTypeQRCode])));
        session.commitConfiguration();
        c._delegate = Some(reader);
        true
    }
}

/// Stops the camera and forgets what it saw.
pub fn stop() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    with_shared(|s| *s = Shared::default());
    let queue = with_capture(|c| c.queue.clone());
    queue.exec_async(|| with_capture(|c| unsafe { c.session.stopRunning() }));
}

pub fn poll() -> Poll {
    with_shared(|s| Poll { frame: None, ready: s.ready, error: s.error, result: s.result.take() })
}

define_class!(
    /// The session's QR reader: hands over each code once while it stays in view.
    #[unsafe(super(NSObject))]
    #[name = "SafayQRReader"]
    struct Reader;

    unsafe impl NSObjectProtocol for Reader {}

    unsafe impl AVCaptureMetadataOutputObjectsDelegate for Reader {
        #[unsafe(method(captureOutput:didOutputMetadataObjects:fromConnection:))]
        fn did_output(&self, _output: &AVCaptureOutput, objects: &NSArray<AVMetadataObject>, _connection: &AVCaptureConnection) {
            let text = objects.iter().find_map(|object| {
                let code = object.downcast::<AVMetadataMachineReadableCodeObject>().ok()?;
                unsafe { code.stringValue() }.map(|value| Zeroizing::new(value.to_string()))
            });
            with_shared(|s| match text {
                Some(text) if s.last_text.as_ref().is_none_or(|last| **last != *text) => {
                    s.last_text = Some(text.clone());
                    s.result = Some(text);
                }
                Some(_) => {}
                None => s.last_text = None,
            });
        }
    }
);

impl Reader {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

define_class!(
    /// The preview: a view whose layer is the session's preview layer, turned with the interface.
    #[unsafe(super(UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SafayCameraView"]
    struct PreviewView;

    impl PreviewView {
        #[unsafe(method(layerClass))]
        fn layer_class() -> &'static AnyClass {
            AVCaptureVideoPreviewLayer::class()
        }

        #[unsafe(method(layoutSubviews))]
        fn layout_subviews(&self) {
            let _: () = unsafe { msg_send![super(self), layoutSubviews] };
            self.turn();
        }
    }
);

impl PreviewView {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        let view: Retained<Self> = unsafe { msg_send![super(this), init] };
        let layer = view.layer();
        let layer: &AVCaptureVideoPreviewLayer = unsafe { &*(Retained::as_ptr(&layer) as *const AVCaptureVideoPreviewLayer) };
        with_capture(|c| unsafe { layer.setSession(Some(&c.session)) });
        if let Some(gravity) = unsafe { AVLayerVideoGravityResizeAspectFill } {
            unsafe { layer.setVideoGravity(gravity) };
        }
        view
    }

    /// The picture upright in the interface's orientation (`UIInterfaceOrientation` and `AVCaptureVideoOrientation`
    /// number the four the same way).
    fn turn(&self) {
        let Some(window) = self.window() else { return };
        let scene: *mut AnyObject = unsafe { msg_send![&*window, windowScene] };
        let Some(scene) = (unsafe { scene.as_ref() }) else { return };
        let orientation: isize = unsafe { msg_send![scene, interfaceOrientation] };
        if !(1..=4).contains(&orientation) {
            return;
        }
        let layer = self.layer();
        let connection: *mut AnyObject = unsafe { msg_send![&*layer, connection] };
        if let Some(connection) = unsafe { connection.as_ref() } {
            let supported: Bool = unsafe { msg_send![connection, isVideoOrientationSupported] };
            if supported.as_bool() {
                let _: () = unsafe { msg_send![connection, setVideoOrientation: orientation] };
            }
        }
    }
}

/// Registers [`PREVIEW`].
pub fn register(components: &nsh_platform_ios::Components) {
    components.register(
        PREVIEW,
        std::rc::Rc::new(|_context: nsh_platform_ios::ComponentContext| -> Box<dyn nsh_platform_ios::NativeComponent> {
            let mtm = MainThreadMarker::new().expect("components are made on the main thread");
            Box::new(Preview { view: PreviewView::new(mtm) })
        }),
    );
}

/// The preview's component: its view only; the camera starts and stops with the page ([`start`], [`stop`]).
struct Preview {
    view: Retained<PreviewView>,
}

impl nsh_platform_ios::NativeComponent for Preview {
    fn view(&self) -> &UIView {
        &self.view
    }

    fn set_prop(&mut self, _name: &str, _value: Option<&nsh_core::JsonValue>) {}
}
