//! macOS: SF Symbols, which NSH cannot show. AppKit renders the named symbol at a point size and weight into a
//! 3× template PNG; the UI shows it as an NSH image tinted with its text color. Cached per (name, size, weight).
//! Other desktops keep the bundled SVG glyphs (`icons.rs`).
use base64::Engine;
use nsh_platform_macos::{
    objc2::{AnyThread, rc::Retained},
    objc2_app_kit::{
        NSBitmapImageFileType, NSBitmapImageRep, NSDeviceRGBColorSpace, NSGraphicsContext, NSImage, NSImageSymbolConfiguration,
    },
    objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString},
};
use std::{cell::RefCell, collections::HashMap};

/// A rendered symbol: a `data:` PNG URI and its size in points.
#[derive(Clone)]
pub struct Rendered {
    pub uri: String,
    pub width: f32,
    pub height: f32,
}

/// (symbol name, point size × 100, weight × 100 + 100).
type Key = (&'static str, u32, u32);
thread_local! {
    static CACHE: RefCell<HashMap<Key, Option<Rendered>>> = RefCell::new(HashMap::new());
}

/// `weight` is AppKit's NSFontWeight (regular 0, medium 0.23, semibold 0.3, bold 0.4).
pub fn render(name: &'static str, point_size: f32, weight: f32) -> Option<Rendered> {
    let key = (name, (point_size * 100.0) as u32, (weight * 100.0 + 100.0) as u32);
    CACHE.with(|c| c.borrow_mut().entry(key).or_insert_with(|| draw(name, point_size, weight)).clone())
}

fn draw(name: &str, point_size: f32, weight: f32) -> Option<Rendered> {
    const SCALE: f64 = 3.0;
    let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(name), None)?;
    let config = NSImageSymbolConfiguration::configurationWithPointSize_weight(point_size as f64, weight as f64);
    let image = image.imageWithSymbolConfiguration(&config)?;
    let size = image.size();
    let (w, h) = ((size.width * SCALE).ceil() as isize, (size.height * SCALE).ceil() as isize);
    if w <= 0 || h <= 0 {
        return None;
    }
    // SAFETY: a null planes pointer makes AppKit allocate the buffer; the arguments describe a valid RGBA 8-bit layout.
    let rep: Retained<NSBitmapImageRep> = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            w,
            h,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            0,
            0,
        )
    }?;
    rep.setSize(size);
    let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep)?;
    NSGraphicsContext::saveGraphicsState_class();
    NSGraphicsContext::setCurrentContext(Some(&context));
    // A template symbol draws in black: only its alpha matters, as the UI tints it.
    image.drawInRect(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(size.width, size.height)));
    context.flushGraphics();
    NSGraphicsContext::restoreGraphicsState_class();
    // SAFETY: an empty properties dictionary is valid for PNG.
    let data = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) }?;
    let bytes = data.to_vec();
    Some(Rendered {
        uri: format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)),
        width: size.width as f32,
        height: size.height as f32,
    })
}
