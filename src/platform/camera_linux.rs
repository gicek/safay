//! Linux camera scanning for "Use Camera" (NSH has no camera API). The desktop portal's Camera grants access and opens a
//! PipeWire remote that shows only cameras; GStreamer reads it as 640×480 BGRx frames (letterboxed, never stretched) on its
//! streaming thread. Each frame becomes a small mirrored JPEG preview for the NSH image and every few frames a grayscale
//! image for `rqrr`, as on macOS. The UI polls `poll()`; nothing is written to disk and the session stops with `stop()`.
use base64::Engine;
use gstreamer::{self as gst, prelude::*};
use safay_app::Zeroizing;
use std::{
    os::fd::IntoRawFd,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraError {
    /// The user declined camera access in the portal's dialog, or it was turned off for Safay.
    Denied,
    /// No camera, no camera portal, or the camera is in use.
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
/// Which session is current: a session that finishes starting after `stop()` (or a newer `start()`) shuts itself down.
static GENERATION: AtomicU64 = AtomicU64::new(0);
static PIPELINE: Mutex<Option<gst::Pipeline>> = Mutex::new(None);

/// The preview and decoding size; the camera's picture is scaled into it with borders, keeping its shape.
const WIDTH: usize = 640;
const HEIGHT: usize = 480;

fn with_shared<R>(f: impl FnOnce(&mut Shared) -> R) -> R {
    let mut guard = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Shared::default))
}

/// Starts the camera (the portal asks for access the first time). Results arrive through `poll()`.
pub fn start() {
    with_shared(|s| *s = Shared { running: true, ..Default::default() });
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    // The portal's dialog and opening the camera can take seconds: off the main thread.
    std::thread::spawn(move || match open() {
        Ok(pipeline) => {
            let current = GENERATION.load(Ordering::SeqCst) == generation && with_shared(|s| s.running);
            if current {
                watch(pipeline.clone(), generation);
                *PIPELINE.lock().unwrap_or_else(|e| e.into_inner()) = Some(pipeline);
            } else {
                let _ = pipeline.set_state(gst::State::Null);
            }
        }
        Err(error) => {
            if GENERATION.load(Ordering::SeqCst) == generation {
                fail(error);
            }
        }
    });
}

fn fail(error: CameraError) {
    with_shared(|s| {
        s.error = Some(error);
        s.running = false;
    });
}

/// Asks the portal for the camera, then plays its PipeWire stream into an app sink.
fn open() -> Result<gst::Pipeline, CameraError> {
    use ashpd::desktop::camera::Camera;
    let fd = futures_executor::block_on(async {
        let camera = Camera::new().await.map_err(|_| CameraError::Unavailable)?;
        if !camera.is_present().await.unwrap_or(false) {
            return Err(CameraError::Unavailable);
        }
        let request = camera.request_access(Default::default()).await.map_err(|_| CameraError::Unavailable)?;
        match request.response() {
            Ok(()) => {}
            Err(ashpd::Error::Response(_)) => return Err(CameraError::Denied),
            Err(_) => return Err(CameraError::Unavailable),
        }
        camera.open_pipe_wire_remote(Default::default()).await.map_err(|_| CameraError::Unavailable)
    })?;
    gst::init().map_err(|_| CameraError::Unavailable)?;
    // PipeWire takes the remote's descriptor over (`pw_context_connect_fd`), so it is handed on, not kept here.
    let description = format!(
        "pipewiresrc fd={} always-copy=true ! videoconvert ! videoscale add-borders=true \
         ! video/x-raw,format=BGRx,width={WIDTH},height={HEIGHT},pixel-aspect-ratio=1/1 \
         ! appsink name=frames max-buffers=1 drop=true sync=false emit-signals=false",
        fd.into_raw_fd()
    );
    let pipeline = gst::parse::launch(&description)
        .map_err(|_| CameraError::Unavailable)?
        .downcast::<gst::Pipeline>()
        .map_err(|_| CameraError::Unavailable)?;
    let sink = pipeline.by_name("frames").and_then(|e| e.downcast::<gstreamer_app::AppSink>().ok()).ok_or(CameraError::Unavailable)?;
    sink.set_callbacks(
        gstreamer_app::AppSinkCallbacks::builder()
            .new_sample(|sink| {
                if let Ok(sample) = sink.pull_sample() {
                    on_sample(&sample);
                }
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );
    if pipeline.set_state(gst::State::Playing).is_err() {
        let _ = pipeline.set_state(gst::State::Null);
        return Err(CameraError::Unavailable);
    }
    Ok(pipeline)
}

/// Reports a stream that fails later (the camera unplugged or taken by another app) as unavailable.
fn watch(pipeline: gst::Pipeline, generation: u64) {
    let Some(bus) = pipeline.bus() else { return };
    std::thread::spawn(move || {
        while GENERATION.load(Ordering::SeqCst) == generation && with_shared(|s| s.running) {
            if bus.timed_pop_filtered(gst::ClockTime::from_mseconds(200), &[gst::MessageType::Error, gst::MessageType::Eos]).is_none() {
                continue;
            }
            if GENERATION.load(Ordering::SeqCst) == generation {
                fail(CameraError::Unavailable);
                stop_pipeline();
            }
            return;
        }
    });
}

/// Stops and releases the camera; the preview disappears.
pub fn stop() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    with_shared(|s| *s = Shared::default());
    stop_pipeline();
}

fn stop_pipeline() {
    if let Some(pipeline) = PIPELINE.lock().unwrap_or_else(|e| e.into_inner()).take() {
        // Stopping waits for the streaming thread: off the main thread.
        std::thread::spawn(move || {
            let _ = pipeline.set_state(gst::State::Null);
        });
    }
}

pub fn poll() -> Poll {
    with_shared(|s| Poll { frame: s.frame.clone(), ready: s.ready, error: s.error, result: s.result.take() })
}

fn on_sample(sample: &gst::Sample) {
    if !with_shared(|s| s.running) {
        return;
    }
    let Some(buffer) = sample.buffer() else { return };
    let Ok(map) = buffer.map_readable() else { return };
    // The caps fix the size and BGRx rows are 4-byte aligned already, so a row is exactly `WIDTH * 4` bytes.
    let row = WIDTH * 4;
    if map.len() < row * HEIGHT {
        return;
    }
    let (luma, preview) = convert(&map[..row * HEIGHT], WIDTH, HEIGHT, row);
    drop(map);
    let n = FRAMES.fetch_add(1, Ordering::Relaxed);
    let due = with_shared(|s| {
        s.ready = true;
        let due = s.last_preview.is_none_or(|t| t.elapsed() >= Duration::from_millis(80));
        if due {
            s.last_preview = Some(Instant::now());
        }
        due
    });
    if due && let Some(uri) = jpeg(&preview, WIDTH / 2, HEIGHT / 2) {
        with_shared(|s| s.frame = Some(uri));
    }
    if n.is_multiple_of(3)
        && let Some(gray) = image::GrayImage::from_raw(WIDTH as u32, HEIGHT as u32, luma.to_vec())
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_is_mirrored_and_luma_is_weighted() {
        // Two pixels a row, two rows: blue then red (BGRx).
        let row = [255, 0, 0, 0, 0, 0, 255, 0];
        let data: Vec<u8> = row.iter().chain(row.iter()).copied().collect();
        let (luma, preview) = convert(&data, 2, 2, 8);
        assert_eq!(&luma[..2], &[(255 * 114 / 1000) as u8, (255 * 299 / 1000) as u8]);
        // One preview pixel, taken from the right edge: red.
        assert_eq!(&preview[..], &[255, 0, 0]);
    }
}
