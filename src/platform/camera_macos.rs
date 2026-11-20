//! macOS camera scanning for "Use Camera" (NSH has no camera API). AVFoundation delivers 640×480 BGRA
//! frames on a private serial queue; each frame becomes a small mirrored JPEG preview for the NSH image and every few frames
//! a grayscale image for `rqrr`. The UI polls `poll()`; nothing is written to disk and the session stops with `stop()`.
//! Needs the bundle's `NSCameraUsageDescription` (added by scripts/package.py); without it the camera is reported as
//! unavailable instead of letting TCC end the process.
use base64::Engine;
use nsh_platform_macos::{
    objc2::{
        AllocAnyThread, define_class, msg_send,
        rc::Retained,
        runtime::{AnyObject, Bool, ProtocolObject},
    },
    objc2_foundation::{NSBundle, NSDictionary, NSNumber, NSObject, NSObjectProtocol, NSString},
};
use objc2_av_foundation::{
    AVAuthorizationStatus, AVCaptureConnection, AVCaptureDevice, AVCaptureDeviceInput, AVCaptureOutput, AVCaptureSession,
    AVCaptureSessionPreset640x480, AVCaptureVideoDataOutput, AVCaptureVideoDataOutputSampleBufferDelegate, AVMediaTypeVideo,
};
use objc2_core_media::CMSampleBuffer;
use objc2_core_video::{
    CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight, CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress,
    CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress, kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_32BGRA,
};
use safay_app::Zeroizing;
use std::{
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraError {
    /// The user declined camera access (System Settings → Privacy & Security → Camera).
    Denied,
    /// No camera, it is in use, or this build lacks the usage description.
    Unavailable,
}

/// What the UI reads each tick.
#[derive(Default)]
pub struct Poll {
    /// The latest preview as a `data:` JPEG URI.
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
    last_preview: Option<Instant>,
    running: bool,
}
static SHARED: Mutex<Option<Shared>> = Mutex::new(None);
static FRAMES: AtomicU64 = AtomicU64::new(0);

struct Running {
    session: Retained<AVCaptureSession>,
    _delegate: Retained<Delegate>,
    _queue: dispatch2::DispatchRetained<dispatch2::DispatchQueue>,
}
// SAFETY: AVCaptureSession is documented as usable from any thread; it is only started and stopped here, one at a time.
unsafe impl Send for Running {}
static SESSION: Mutex<Option<Running>> = Mutex::new(None);

fn with_shared<R>(f: impl FnOnce(&mut Shared) -> R) -> R {
    let mut guard = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Shared::default))
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "SafayCameraDelegate"]
    struct Delegate;
    unsafe impl NSObjectProtocol for Delegate {}
    unsafe impl AVCaptureVideoDataOutputSampleBufferDelegate for Delegate {
        #[unsafe(method(captureOutput:didOutputSampleBuffer:fromConnection:))]
        fn did_output(&self, _output: &AVCaptureOutput, buffer: &CMSampleBuffer, _connection: &AVCaptureConnection) {
            on_frame(buffer);
        }
    }
);

/// Whether this build can ask for the camera at all.
fn has_usage_description() -> bool {
    NSBundle::mainBundle().objectForInfoDictionaryKey(&NSString::from_str("NSCameraUsageDescription")).is_some()
}

/// Starts the camera (asking for access the first time). Results arrive through `poll()`.
pub fn start() {
    with_shared(|s| *s = Shared { running: true, ..Default::default() });
    if !has_usage_description() {
        fail(CameraError::Unavailable);
        return;
    }
    // SAFETY: a framework constant.
    let Some(video) = (unsafe { AVMediaTypeVideo }) else {
        fail(CameraError::Unavailable);
        return;
    };
    // SAFETY: querying and requesting authorization for the video media type is valid from any thread.
    match unsafe { AVCaptureDevice::authorizationStatusForMediaType(video) } {
        AVAuthorizationStatus::Authorized => begin(),
        AVAuthorizationStatus::NotDetermined => {
            let handler = block2::RcBlock::new(|granted: Bool| if granted.as_bool() { begin() } else { fail(CameraError::Denied) });
            unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(video, &handler) };
        }
        _ => fail(CameraError::Denied),
    }
}

fn fail(error: CameraError) {
    with_shared(|s| {
        s.error = Some(error);
        s.running = false;
    });
}

/// Configures and starts the session off the main thread: opening the camera can take over a second.
fn begin() {
    std::thread::spawn(|| {
        if !with_shared(|s| s.running) {
            return;
        }
        match configure() {
            Some(running) => {
                // SAFETY: the session is fully configured; startRunning blocks this worker thread only.
                unsafe { running.session.startRunning() };
                let keep = with_shared(|s| s.running);
                if keep {
                    *SESSION.lock().unwrap_or_else(|e| e.into_inner()) = Some(running);
                } else {
                    unsafe { running.session.stopRunning() };
                }
            }
            None => fail(CameraError::Unavailable),
        }
    });
}

fn configure() -> Option<Running> {
    // SAFETY: standard AVFoundation setup; every object is created and wired on this one thread before the session starts.
    unsafe {
        let video = AVMediaTypeVideo?;
        let device = AVCaptureDevice::defaultDeviceWithMediaType(video)?;
        let input = AVCaptureDeviceInput::deviceInputWithDevice_error(&device).ok()?;
        let session = AVCaptureSession::new();
        if session.canSetSessionPreset(AVCaptureSessionPreset640x480) {
            session.setSessionPreset(AVCaptureSessionPreset640x480);
        }
        if !session.canAddInput(&input) {
            return None;
        }
        session.addInput(&input);
        let output = AVCaptureVideoDataOutput::new();
        let key: &NSString = &*(kCVPixelBufferPixelFormatTypeKey as *const _ as *const NSString);
        let format = NSNumber::new_u32(kCVPixelFormatType_32BGRA);
        let value: &AnyObject = &format;
        let settings = NSDictionary::<NSString, AnyObject>::from_slices(&[key], &[value]);
        output.setVideoSettings(Some(&settings));
        output.setAlwaysDiscardsLateVideoFrames(true);
        let delegate: Retained<Delegate> = msg_send![Delegate::alloc(), init];
        let queue = dispatch2::DispatchQueue::new("safay.camera", None);
        output.setSampleBufferDelegate_queue(Some(ProtocolObject::from_ref(&*delegate)), Some(&queue));
        if !session.canAddOutput(&output) {
            return None;
        }
        session.addOutput(&output);
        Some(Running { session, _delegate: delegate, _queue: queue })
    }
}

/// Stops and releases the camera; the preview disappears.
pub fn stop() {
    with_shared(|s| *s = Shared::default());
    if let Some(running) = SESSION.lock().unwrap_or_else(|e| e.into_inner()).take() {
        std::thread::spawn(move || {
            // The whole `Running` moves (not just its session field), so the Send wrapper applies.
            let running = running;
            unsafe { running.session.stopRunning() }
        });
    }
}

pub fn poll() -> Poll {
    with_shared(|s| Poll { frame: s.frame.clone(), ready: s.ready, error: s.error, result: s.result.take() })
}

fn on_frame(buffer: &CMSampleBuffer) {
    if !with_shared(|s| s.running) {
        return;
    }
    // SAFETY: the image buffer is retained for this call; the base address is read only between lock and unlock.
    let Some(image) = (unsafe { buffer.image_buffer() }) else { return };
    let (luma, preview, w, h) = unsafe {
        if CVPixelBufferLockBaseAddress(&image, CVPixelBufferLockFlags::ReadOnly) != 0 {
            return;
        }
        let (w, h, row) = (CVPixelBufferGetWidth(&image), CVPixelBufferGetHeight(&image), CVPixelBufferGetBytesPerRow(&image));
        let base = CVPixelBufferGetBaseAddress(&image) as *const u8;
        let out = if base.is_null() || w == 0 || h == 0 || w > 4096 || h > 4096 {
            None
        } else {
            let data = std::slice::from_raw_parts(base, row * h);
            Some(convert(data, w, h, row))
        };
        CVPixelBufferUnlockBaseAddress(&image, CVPixelBufferLockFlags::ReadOnly);
        match out {
            Some((luma, preview)) => (luma, preview, w, h),
            None => return,
        }
    };
    let n = FRAMES.fetch_add(1, Ordering::Relaxed);
    let due = with_shared(|s| {
        s.ready = true;
        let due = s.last_preview.is_none_or(|t| t.elapsed() >= Duration::from_millis(80));
        if due {
            s.last_preview = Some(Instant::now());
        }
        due
    });
    if due && let Some(uri) = jpeg(&preview, w / 2, h / 2) {
        with_shared(|s| s.frame = Some(uri));
    }
    if n.is_multiple_of(3)
        && let Some(gray) = image::GrayImage::from_raw(w as u32, h as u32, luma.to_vec())
    {
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
                break;
            }
        }
    }
}

/// Full-size luma for decoding, and a half-size RGB preview mirrored as a camera preview is.
fn convert(data: &[u8], w: usize, h: usize, row: usize) -> (Zeroizing<Vec<u8>>, Zeroizing<Vec<u8>>) {
    let mut luma = Zeroizing::new(vec![0u8; w * h]);
    for y in 0..h {
        let line = &data[y * row..y * row + w * 4];
        for x in 0..w {
            let (b, g, r) = (line[x * 4] as u32, line[x * 4 + 1] as u32, line[x * 4 + 2] as u32);
            luma[y * w + x] = ((r * 299 + g * 587 + b * 114) / 1000) as u8;
        }
    }
    let (pw, ph) = (w / 2, h / 2);
    let mut preview = Zeroizing::new(vec![0u8; pw * ph * 3]);
    for y in 0..ph {
        let line = &data[(y * 2) * row..(y * 2) * row + w * 4];
        for x in 0..pw {
            let source = (w - 1 - x * 2) * 4;
            let target = (y * pw + x) * 3;
            preview[target] = line[source + 2];
            preview[target + 1] = line[source + 1];
            preview[target + 2] = line[source];
        }
    }
    (luma, preview)
}

fn jpeg(rgb: &[u8], w: usize, h: usize) -> Option<String> {
    let mut out = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 70);
    image::ImageEncoder::write_image(encoder, rgb, w as u32, h as u32, image::ExtendedColorType::Rgb8).ok()?;
    Some(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(out)))
}
