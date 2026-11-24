//! "Scan QR Code": the camera's picture over the whole screen, a frame to aim with, and what was read. A 2FA QR code
//! read goes to "Add Account" for review; a Google Authenticator export in several QR codes is read to its last one
//! first. Without the camera (refused, or none), the page says so and offers the photo library instead; refused, it also
//! offers the app's page in the system's settings, where the camera is allowed again.
use super::{Nav, ui};
use crate::{icons::Icon, store::Store};
use nsh_base::button;
use nsh_core::prelude::*;
use safay_app::{Zeroizing, core::otpauth, session::placeholders};

/// The frame aimed with, in points.
const FRAME: f32 = 248.0;
/// The page is the camera's picture in any appearance: black under it, white over it.
const BLACK: AdaptiveColor = AdaptiveColor::rgb(0x000000, 0x000000);
const WHITE: AdaptiveColor = AdaptiveColor::rgb(0xffffff, 0xffffff);
/// Under the bar and the line at the bottom: white stays readable over a bright picture.
const SHADE: AdaptiveColor = AdaptiveColor::rgb(0x000000, 0x000000);

pub fn page(s: Store, nav: Nav) -> impl IntoElement {
    // The preview in two layers that take turns: a frame loads into the hidden one and shows once loaded, so the shown
    // image never blanks while the next decodes (an image clears as its source changes).
    let layers = [signal(None::<String>), signal(None::<String>)];
    let shown = signal(0_usize);
    let failed = signal(None::<&'static str>);
    // The camera was refused and the system can show the app's settings page.
    let settings = signal(false);
    // What was read, until a Google Authenticator export has all its parts.
    let collected = signal(Zeroizing::new(String::new()));
    let note = signal(None::<SharedString>);
    let progress = move || migration(&collected.get());
    let take = move |text: Zeroizing<String>| {
        let valid = text.lines().any(|l| otpauth::parse_scan_result(l, &placeholders()).accounts.iter().any(|a| a.is_valid()));
        if !valid {
            note.set(Some(s.label("This isn’t a 2FA QR code")));
            return;
        }
        note.set(None);
        collected.update(|c| {
            for line in text.lines() {
                if !line.trim().is_empty() && !c.lines().any(|l| l == line) {
                    c.push_str(line);
                    c.push('\n');
                }
            }
        });
        haptic(Haptic::Notification(HapticNotification::Success));
        if progress().is_none_or(|(seen, size)| seen >= size) {
            // Off this page, then to the review: going back from there leads to the list, not to the camera.
            let text = collected.get();
            nav.back();
            s.append_scanned(&text);
        }
    };
    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "android"))]
    {
        crate::platform::camera::start();
        cleanup(crate::platform::camera::stop);
        set_interval(std::time::Duration::from_millis(100), move || {
            let poll = crate::platform::camera::poll();
            if let Some(error) = poll.error {
                let denied = error == crate::platform::camera::CameraError::Denied;
                if denied && failed.get_untracked().is_none() {
                    s.task(async move { settings.set(app_settings_available().await) });
                }
                failed.set(Some(if denied { "Camera access not allowed" } else { "Can’t open the camera" }));
                return;
            }
            if poll.frame.is_some() {
                layers[1 - shown.get_untracked()].set(poll.frame);
            }
            if let Some(text) = poll.result {
                take(text);
            }
        });
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "android")))]
    {
        let _ = take;
        failed.set(Some("Can’t open the camera"));
    }
    view()
        .absolute()
        .inset(px(0.0))
        .bg(|| BLACK)
        // Android: the camera draws its preview itself, into a native view.
        .child(if cfg!(target_os = "android") {
            native("safay.camera").absolute().inset(px(0.0)).into_element()
        } else {
            view().into_element()
        })
        // Elsewhere (the phone interface on a desktop): the frames as images.
        .children([0, 1].map(|i| {
            let layer = layers[i];
            when(
                move || layer.with(|f| f.is_some()),
                move || {
                    image(move || layer.get().unwrap_or_default())
                        .absolute()
                        .inset(px(0.0))
                        .object_fit(ResizeMode::Cover)
                        // At once: Android fades a new image in (300 ms by default), and a layer shown as it loads
                        // would still be nearly clear, many times a second.
                        .fade_duration(0.0)
                        .opacity(move || if shown.get() == i { 1.0_f32 } else { 0.0 })
                        .on_load(move |_: &Size<DevicePixels>| {
                            if shown.get_untracked() != i {
                                shown.set(i);
                            }
                        })
                },
            )
        }))
        // The frame to aim with, centered.
        .child(
            view().absolute().inset(px(0.0)).items_center().justify_center().pointer_events(nsh_core::PointerEvents::None).child(
                view()
                    .size(px(FRAME))
                    .rounded(px(28.0))
                    .border(px(3.0))
                    .border_color(|| WHITE)
                    .opacity(move || if failed.get().is_some() { 0.0_f32 } else { 0.9 }),
            ),
        )
        // The bar: closing, and the page's name.
        .child(
            view().absolute().left(px(0.0)).right(px(0.0)).top(px(0.0)).pt_safe_area().bg(|| SHADE.opacity(0.45)).child(
                view()
                    .flex_row()
                    .items_center()
                    .h(move || px(ui::BAR) * font_scale())
                    .px(px(4.0))
                    .child(ui::bar_button(move || s.label("Close"), || Icon::Close).text_color(|| WHITE).test_id("scan-close").on_press(
                        move || {
                            nav.back();
                        },
                    ))
                    .child(ui::title_large(text(move || s.label("Scan QR Code"))).text_color(|| WHITE).ml(px(4.0))),
            ),
        )
        // What is going on, under the frame: aim, how far an export is read, what went wrong.
        .child(
            view()
                .absolute()
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .pt(px(24.0))
                .pb_safe_area_plus(px(48.0))
                .px(px(32.0))
                .bg(|| SHADE.opacity(0.45))
                .items_center()
                .gap(px(16.0))
                .child(
                    text(move || {
                        if let Some(key) = failed.get() {
                            return s.label(key);
                        }
                        if let Some((seen, size)) = progress().filter(|(seen, size)| seen < size) {
                            return s.fmt("Scanned Google migration QR {} / {}", &[&seen.to_string(), &size.to_string()]);
                        }
                        note.get().unwrap_or_else(|| s.label("Point the camera at the QR code"))
                    })
                    .test_id("scan-hint")
                    .text_center()
                    .text_size(px(16.0))
                    .line_height(px(22.0))
                    .text_color(|| WHITE),
                )
                .child(when(
                    move || failed.get().is_some(),
                    move || {
                        ui::filled_button(Some(Icon::Photo), move || s.label("Choose from Photos")).on_press(move || {
                            nav.back();
                            s.scanned.set(Default::default());
                            s.choose_images()
                        })
                    },
                ))
                // Off this page first: back from the settings, "Scan QR Code" asks the camera again.
                .child(when(
                    move || settings.get(),
                    move || {
                        button()
                            .accessibility_label(move || Some(s.label("Open System Settings")))
                            .test_id("scan-settings")
                            .h(move || px(48.0) * font_scale())
                            .px(px(24.0))
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .border_1()
                            .border_color(|| WHITE.opacity(0.6))
                            .active(|st| st.bg(|| WHITE.opacity(0.12)))
                            .child(ui::label(text(move || s.label("Open System Settings"))).text_color(|| WHITE))
                            .on_press(move || {
                                nav.back();
                                drop(open_app_settings());
                            })
                    },
                )),
        )
}

/// How far a Google Authenticator export in several QR codes has been read: the parts seen and their number.
fn migration(text: &str) -> Option<(i64, i64)> {
    let scans: Vec<_> = text.lines().map(|l| otpauth::parse_scan_result(l, &placeholders())).collect();
    scans.iter().find(|x| x.is_google_migration && x.batch_size.unwrap_or(1) > 1).map(|x| {
        let size = x.batch_size.unwrap_or(1);
        let seen = scans.iter().filter(|y| y.is_google_migration && y.batch_id == x.batch_id).count() as i64;
        (seen, size)
    })
}
