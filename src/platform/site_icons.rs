//! Site icons (SECURITY.md, Network): the core's `SiteIconFetch` decides every URL; this module only sends its requests with
//! no credentials, decodes the image it picks (PNG, ICO, JPEG, GIF, WebP, size checked from the header first), scales it
//! to fit 96 × 96 and re-encodes it as PNG for the core to check. Every failure is silent: the monogram stays.
use super::http::Executor;
use safay_app::{
    cloudset::{HttpRequest, Method},
    core::site_icon::{self, SiteIconFetch, Step},
};
use std::{
    io::Cursor,
    time::{Duration, Instant},
};

/// One website's icon as a checked PNG, or `None`. Blocking: run off the UI thread.
pub fn fetch(website: &str) -> Option<Vec<u8>> {
    let executor = Executor::new(
        Duration::from_secs(u64::from(site_icon::REQUEST_TIMEOUT_SECONDS)),
        Duration::from_secs(u64::from(site_icon::REQUEST_TIMEOUT_SECONDS)),
        site_icon::MAX_IMAGE_BYTES as usize,
    );
    let started = Instant::now();
    let (mut state, mut request) = SiteIconFetch::start(website)?;
    loop {
        if started.elapsed() > Duration::from_secs(u64::from(site_icon::HOST_TIMEOUT_SECONDS)) {
            return None;
        }
        let http = HttpRequest::new(Method::Get, request.url.clone());
        let mut body = Vec::new();
        let mut step = match executor.execute_with_limit(http, None, request.max_bytes as usize) {
            Ok(response) => {
                body = response.body;
                let location = response.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case("location")).map(|(_, v)| v.clone());
                state.handle_response(response.status, location.as_deref(), &body)
            }
            Err(_) => state.transport_failed(),
        };
        loop {
            match step {
                Step::Fetch(next) => {
                    request = next;
                    break;
                }
                Step::Decode => match decode(&body) {
                    Some(png) => return Some(png),
                    None => step = state.decode_failed(),
                },
                Step::Finished => return None,
            }
        }
    }
}

/// The platform's half of `Step::Decode`: header size check, bounded decode, fit to 96 × 96, PNG, core check.
pub fn decode(data: &[u8]) -> Option<Vec<u8>> {
    let reader = image::ImageReader::new(Cursor::new(data)).with_guessed_format().ok()?;
    let (w, h) = reader.into_dimensions().ok()?;
    let (min, max) = (site_icon::MIN_SOURCE_PIXELS, site_icon::MAX_SOURCE_PIXELS);
    if w < min || h < min || w > max || h > max {
        return None;
    }
    let mut reader = image::ImageReader::new(Cursor::new(data)).with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(max);
    limits.max_image_height = Some(max);
    limits.max_alloc = Some(16 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().ok()?;
    let fitted = if w > site_icon::ICON_PIXELS || h > site_icon::ICON_PIXELS {
        image.resize(site_icon::ICON_PIXELS, site_icon::ICON_PIXELS, image::imageops::FilterType::Lanczos3)
    } else {
        image
    };
    let mut out = Cursor::new(Vec::new());
    fitted.to_rgba8().write_to(&mut out, image::ImageFormat::Png).ok()?;
    let png = out.into_inner();
    site_icon::is_valid_icon(&png).then_some(png)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(w, h, image::Rgba([10, 120, 250, 255])).write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }
    #[test]
    fn large_icons_are_fitted_and_tiny_or_huge_ones_refused() {
        let fitted = decode(&png(512, 256)).expect("a 512×256 icon");
        let (w, h) = image::ImageReader::new(Cursor::new(&fitted)).with_guessed_format().unwrap().into_dimensions().unwrap();
        assert_eq!((w, h), (96, 48));
        assert!(decode(&png(16, 16)).is_none(), "below the core's minimum");
        assert!(decode(&png(2048, 64)).is_none(), "above the core's maximum");
        assert!(decode(b"<html>not an image</html>").is_none());
    }
}
