//! The sheets: Add Account (620), Account Details (540), QR Code (420), Import / Export (660),
//! Change Master Password (460), plus the master-password prompt that stands in for Touch ID.
use super::unlock::{check_box, drop_zone, field_label, new_password_fields, new_password_ok};
use crate::{
    components::*,
    icons::{Icon, icon},
    store::{AddTab, Auth, Sheet, Status, Store, Tone},
};
use nsh_base::{button, input, theme};
use nsh_core::prelude::*;
use safay_app::{
    Uuid, Zeroizing,
    core::import::FileKind,
    core::{OtpType, otpauth, site_icon, totp},
    session::placeholders,
};

pub fn sheets(s: Store) -> impl IntoElement {
    let is = move |f: fn(Sheet) -> bool| move || s.sheet.get().is_some_and(f);
    view()
        .child(when(is(|x| x == Sheet::Add), move || add_sheet(s)))
        .child(when(is(|x| matches!(x, Sheet::Detail(_))), move || detail_sheet(s)))
        .child(when(is(|x| matches!(x, Sheet::Qr(_))), move || qr_sheet(s)))
        .child(when(is(|x| x == Sheet::ExportImport), move || export_import_sheet(s)))
        .child(when(is(|x| x == Sheet::ChangePassword), move || change_password_sheet(s)))
        .child(when(is(|x| x == Sheet::Restore), move || restore_sheet(s)))
        .child(when(is(|x| x == Sheet::Settings), move || settings_dialog(s)))
        .child(when(is(|x| x == Sheet::Backup), move || super::backup::backup_sheet(s)))
}
/// Windows and Linux: the Settings panes in a dialog over the main window (macOS uses a window).
fn settings_dialog(s: Store) -> impl IntoElement {
    // Escape while a shortcut is recorded stops recording only (`settings::recorder`), as on macOS.
    let cancel = move || {
        if s.recording.get().is_some() {
            s.recording.set(None);
            #[cfg(target_os = "linux")]
            crate::desktop::pause_shortcuts(false);
        } else {
            s.close_sheet()
        }
    };
    on_key_with("escape", KeyOptions { when: nsh_core::keymap::When::Always, ..Default::default() }, move |_| {
        if s.auth.get().is_none() {
            cancel()
        }
    });
    nsh_base::dialog().animation(nsh_core::ModalAnimation::Fade).on_cancel(move |_: &()| cancel()).child(
        modal_backdrop().p(px(24.0)).child(
            fit_width(modal_surface(view().role(nsh_core::AccessibilityRole::Alert)).w(px(760.0)))
                .h(px(520.0))
                .max_h(move || window().height - px(48.0))
                .overflow_hidden()
                .child(super::settings::page(s).flex_1())
                .child(
                    view().absolute().top(px(12.0)).right(px(12.0)).child(
                        icon_button(move || s.label("Close"), Icon::Close)
                            .rounded_full()
                            .bg(move || theme().secondary)
                            .on_press(move || s.close_sheet()),
                    ),
                ),
        ),
    )
}
fn cancel_button(s: Store) -> nsh_base::ButtonElement {
    push_button(move || s.label("Cancel"), Kind::Standard, false).on_press(move || s.close_sheet())
}
fn footer_error(s: Store) -> impl IntoElement {
    status_line(move || {
        s.error.get().map(|e| Status { tone: Tone::Error, text: s.label(crate::i18n::error_key(e)) }).or_else(|| s.status.get())
    })
}

// ---------------------------------------------------------------- add account

fn parsed(text: &str) -> Vec<otpauth::ParsedOtpAuth> {
    otpauth::parse_lines(text, &placeholders())
}
fn add_sheet(s: Store) -> impl IntoElement {
    let valid_key = move || {
        s.form.with(|f| {
            totp::is_valid_secret(&f.secret) && (f.website.trim().is_empty() || site_icon::normalized_website(&f.website).is_some())
        })
    };
    let lines = move || match s.add_tab.get() {
        AddTab::Text => s.form.with(|f| f.text.clone()),
        AddTab::Scan => s.scanned.get(),
        AddTab::Key => Zeroizing::new(String::new()),
    };
    let valid_count = move || {
        if s.add_tab.get() == AddTab::Key { usize::from(valid_key()) } else { parsed(&lines()).iter().filter(|p| p.is_valid()).count() }
    };
    let save = move || {
        if valid_count() == 0 || s.busy.get() {
            if s.add_tab.get() == AddTab::Key {
                let (secret, website) = s.form.with(|f| {
                    (!totp::is_valid_secret(&f.secret), !f.website.trim().is_empty() && site_icon::normalized_website(&f.website).is_none())
                });
                s.marks.update(|m| {
                    m.secret_error = secret;
                    m.website_error = website;
                });
            }
            return;
        }
        match s.add_tab.get() {
            AddTab::Key => s.save_new(),
            _ => s.import_text(lines()),
        }
    };
    let tabs = segmented(
        vec![
            (AddTab::Scan, Box::new(move || s.label("Scan QR Code")) as Box<dyn Fn() -> SharedString>),
            (AddTab::Key, Box::new(move || s.label("Enter Key"))),
            (AddTab::Text, Box::new(move || s.label("Paste Links"))),
        ],
        move || s.add_tab.get(),
        move |t| {
            s.error.set(None);
            s.add_tab.set(t)
        },
    )
    // On macOS the segmented picker is 324pt inside a 340pt frame.
    .w(px(if crate::chrome::look() == crate::chrome::Look::Mac { 324.0 } else { 340.0 }));
    sheet(
        s,
        620.0,
        236.0,
        move || s.label("Add Account"),
        tabs,
        view()
            .child(when(move || s.add_tab.get() == AddTab::Scan, move || scan_panel(s)))
            .child(when(move || s.add_tab.get() == AddTab::Key, move || key_panel(s, save)))
            .child(when(move || s.add_tab.get() == AddTab::Text, move || text_panel(s))),
        view()
            .flex_1()
            .flex_row()
            .items_center()
            .gap(px(crate::chrome::footer_gap()))
            .child(view().flex_1().min_w(px(0.0)).child(status_line(move || {
                if let Some(e) = s.error.get() {
                    return Some(Status {
                        tone: Tone::Error,
                        text: s.label(if e == safay_app::session::Error::Duplicate {
                            "Account already exists"
                        } else {
                            crate::i18n::error_key(e)
                        }),
                    });
                }
                if s.marks.with(|m| m.secret_error) && s.add_tab.get() == AddTab::Key {
                    return Some(Status { tone: Tone::Error, text: s.label("Invalid secret key") });
                }
                // The footer is the only error slot, so the form keeps its height.
                if s.marks.with(|m| m.website_error) && s.add_tab.get() == AddTab::Key {
                    return Some(Status { tone: Tone::Error, text: s.label("Enter a website like example.com") });
                }
                s.status.get()
            })))
            .child(cancel_button(s))
            .child(
                push_button(
                    move || {
                        let n = valid_count();
                        if n > 1 { s.count("add_n", n) } else { s.label("Save Account") }
                    },
                    Kind::Primary,
                    false,
                )
                .disabled(move || valid_count() == 0 || s.busy.get())
                .on_press(save),
            ),
        move || s.close_sheet(),
    )
}
fn key_panel(s: Store, _save: impl Fn() + Copy + 'static) -> impl IntoElement {
    view()
        .gap(px(14.0))
        .child(
            view()
                .flex_row()
                .gap(px(12.0))
                .child(
                    labeled(
                        move || s.label("Service Name"),
                        text_field(
                            s,
                            "service",
                            move || s.label("Service Name"),
                            move || s.label("Service Name"),
                            move || s.form.with(|f| f.issuer.clone()),
                            move |v| {
                                s.form.update(|f| {
                                    f.issuer = v.into();
                                    if !f.website_touched {
                                        f.website = site_icon::suggested_website(v).unwrap_or_default();
                                    }
                                })
                            },
                            true,
                            || {},
                        ),
                    )
                    .flex_1(),
                )
                .child(
                    labeled(
                        move || s.label("Account / Email"),
                        text_field(
                            s,
                            "username",
                            move || s.label("Account / Email"),
                            move || s.label("Account name or email"),
                            move || s.form.with(|f| f.username.clone()),
                            move |v| s.form.update(|f| f.username = v.into()),
                            false,
                            || {},
                        ),
                    )
                    .flex_1(),
                ),
        )
        .child(view().gap(px(4.0)).child(labeled(
            move || s.label("Website"),
            text_field(
                s,
                "website",
                move || s.label("Website"),
                move || s.label("Optional"),
                move || s.form.with(|f| f.website.clone()),
                move |v| {
                    s.form.update(|f| {
                        f.website = v.into();
                        f.website_touched = true;
                    });
                    s.marks.update(|m| m.website_error = false)
                },
                false,
                move || {
                    let bad = s.form.with(|f| !f.website.trim().is_empty() && site_icon::normalized_website(&f.website).is_none());
                    s.marks.update(|m| m.website_error = bad)
                },
            ),
        )))
        .child(
            view()
                .gap(px(6.0))
                .child(
                    text(move || s.label("Secret"))
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(move || theme().muted_foreground),
                )
                .child(secret_input(s))
                .child(
                    text(move || s.label("The secret is encrypted on this device and never leaves in plaintext."))
                        .text_size(px(11.0))
                        .line_height(px(13.0))
                        .text_color(move || theme().muted_foreground),
                ),
        )
        .child(advanced(s))
}
fn secret_input(s: Store) -> impl IntoElement {
    let shown = signal(false);
    input_chrome(
        view()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .min_h(move || px(36.0) * font_scale())
            .pl(px(if crate::chrome::look() == crate::chrome::Look::Mac { 11.0 } else { 12.0 }))
            .pr(px(if crate::chrome::look() == crate::chrome::Look::Mac { 5.0 } else { 6.0 }))
            .child(
                input()
                    .test_id("secret")
                    .label(move || s.label("Secret"))
                    .placeholder(move || s.label("Enter or paste the key from the service").to_string())
                    .masked(move || !shown.get())
                    .value(move || s.form.with(|f| f.secret.to_string()))
                    .on_change(move |v: &str| {
                        s.form.update(|f| f.secret = Zeroizing::new(v.into()));
                        if s.marks.with(|m| m.secret_error) {
                            s.marks.update(|m| m.secret_error = false)
                        }
                    })
                    .on_focus(|_: &()| roman_input_only(true))
                    .on_blur(move |_: &()| {
                        roman_input_only(false);
                        let bad = s.form.with(|f| !f.secret.is_empty() && !totp::is_valid_secret(&f.secret));
                        s.marks.update(|m| m.secret_error = bad)
                    })
                    .auto_correct(false)
                    .auto_fill(nsh_core::AutoFill::Off)
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(move || {
                        SharedString::from(if s.form.with(|f| f.secret.is_empty()) { "system-ui" } else { CODE_FONT.family })
                    })
                    .text_size(px(13.0))
                    .text_color(move || theme().foreground),
            )
            .child(reveal_toggle(
                move || shown.get(),
                move || s.label(if shown.get() { "Hide" } else { "Show Secret" }),
                move || shown.update(|v| *v = !*v),
            )),
        7.0,
    )
}
fn advanced(s: Store) -> impl IntoElement {
    let open = move || s.form.with(|f| f.advanced);
    let picker = |s: Store,
                  title: &'static str,
                  items: &'static [(u32, &'static str)],
                  get: fn(&crate::store::Form) -> u32,
                  set: fn(&mut crate::store::Form, u32)| {
        let items2 = items;
        view()
            .flex_1()
            .gap(px(6.0))
            .child(
                text(move || s.label(title))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(move || theme().muted_foreground),
            )
            .child(super::settings::choice(
                s,
                title,
                move || items2.iter().map(|(v, l)| nsh_base::SelectItem::new(v.to_string(), s.label(l))).collect(),
                move || s.form.with(get).to_string().into(),
                move |v| {
                    if let Ok(v) = v.parse::<u32>() {
                        s.form.update(|f| set(f, v))
                    }
                },
                None,
            ))
    };
    view()
        .gap(px(12.0))
        .child(
            button()
                .self_start()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .on_press(move || s.form.update(|f| f.advanced = !f.advanced))
                .focus(|st| st.opacity(0.7))
                // An 11pt semibold chevron.right, turned 90° when open.
                .child(view().rotate(move || if open() { std::f32::consts::FRAC_PI_2 } else { 0.0 }).child(crate::icons::symbol_weight(
                    Icon::ChevronRight,
                    11.0,
                    0.3,
                    move || theme().muted_foreground,
                )))
                .child(text(move || s.label("Advanced Options")).text_size(px(12.5)).font_weight(FontWeight::SEMIBOLD)),
        )
        .child(when(open, move || {
            // Steam Guard and Battle.net have fixed parameters: only Time Based shows them.
            let time_based = move || s.form.with(|f| f.otp_type == 0);
            view()
                .gap(px(12.0))
                .child(
                    view()
                        .flex_row()
                        .gap(px(18.0))
                        .child(picker(
                            s,
                            "Type",
                            &[(0, "Time Based"), (2, "Battle.net"), (1, "Steam")],
                            |f| f.otp_type,
                            |f, v| f.otp_type = v,
                        ))
                        // Keeps Type as wide as each picker below.
                        .child(view().flex_1())
                        .child(view().flex_1()),
                )
                .child(when(time_based, move || {
                    view()
                        .flex_row()
                        .gap(px(18.0))
                        .child(picker(
                            s,
                            "Algorithm",
                            &[(0, "SHA1"), (1, "SHA256"), (2, "SHA512")],
                            |f| f.algorithm,
                            |f, v| f.algorithm = v,
                        ))
                        .child(picker(s, "Digits", &[(6, "6"), (8, "8")], |f| f.digits, |f, v| f.digits = v))
                        .child(picker(s, "Refresh Interval", &[(30, "30 seconds"), (60, "60 seconds")], |f| f.period, |f, v| f.period = v))
                }))
        }))
}
fn text_panel(s: Store) -> impl IntoElement {
    let over = signal(false);
    view()
        .gap(px(10.0))
        .child(
            input_chrome(
                input()
                    .test_id("import-text")
                    .multiline(true)
                    .label(move || s.label("Paste Links"))
                    .placeholder("otpauth://totp/GitHub:you@example.com?secret=…&issuer=GitHub\notpauth-migration://offline?data=…")
                    .value(move || s.form.with(|f| f.text.to_string()))
                    .on_change(move |v: &str| s.form.update(|f| f.text = Zeroizing::new(v.into())))
                    .on_file_drag_enter(move |_: &()| over.set(true))
                    .on_file_drag_leave(move |_: &()| over.set(false))
                    .on_file_drop(move |e: &nsh_core::element::FileDropEvent| {
                        over.set(false);
                        for path in e.uris.iter().filter_map(|u| super::unlock::file_path(u)).take(10) {
                            spawn(async move {
                                if let Ok((_, bytes)) = crate::platform::files::read_named(path).await
                                    && let Ok(t) = String::from_utf8(bytes)
                                {
                                    let t = Zeroizing::new(t);
                                    s.form.update(|f| {
                                        let mut next = Zeroizing::new(f.text.trim_end().to_owned());
                                        if !next.is_empty() {
                                            next.push('\n');
                                        }
                                        next.push_str(&t);
                                        f.text = next;
                                    });
                                }
                            });
                        }
                    })
                    .auto_correct(false)
                    .auto_fill(nsh_core::AutoFill::Off)
                    .h(px(150.0))
                    // The field scrolls its own text. Its native measure is the whole text's height, which as overflow
                    // made the sheet's scroll view as tall as the text (macOS): clipped, it stays the field's 150pt.
                    .overflow_hidden()
                    .p(px(8.0))
                    .font_family(CODE_FONT.family)
                    .text_size(px(12.0))
                    .text_color(move || theme().foreground),
                7.0,
            )
            .border_color(move || if over.get() { tok("code-accent") } else { tok("input") }),
        )
        .child(note(move || {
            s.label("Paste one otpauth:// URL or Google Authenticator migration URL per line, or drop in a plain text file.")
        }))
        .child(parsed_list(s, move || s.form.with(|f| f.text.clone())))
}
/// A row of the parsed lines, and how many show before the rest scroll.
const PARSED_ROW: f32 = 36.0;
const PARSED_SHOWN: usize = 6;
/// Each recognized line with "Parsed", "Already in Safay" or the reason it failed. Issuer and account only, never
/// secrets.
fn parsed_list(s: Store, source: impl Fn() -> Zeroizing<String> + Copy + 'static) -> impl IntoElement {
    // Which of the valid accounts the vault already has, asked again whenever the lines change.
    let known = signal(Vec::<bool>::new());
    effect(move || s.check_known(source(), known));
    let items = move || {
        let known = known.get();
        let mut valid = 0;
        parsed(&source())
            .into_iter()
            .map(|p| {
                let ok = p.is_valid();
                let title = if ok {
                    format!("{} — {}", p.issuer, p.account)
                } else {
                    s.label(p.failure.map(|e| e.l10n_key()).unwrap_or("Unable to parse")).to_string()
                };
                let exists = ok && known.get(valid).copied().unwrap_or(false);
                valid += usize::from(ok);
                (title, (ok, exists))
            })
            .collect::<Vec<_>>()
    };
    // Up to six rows high (with the hairlines between them), the rest scroll inside.
    let height = move || {
        let n = items().len().min(PARSED_SHOWN) as f32;
        px(n * PARSED_ROW + (n - 1.0).max(0.0))
    };
    when(
        move || !items().is_empty(),
        move || {
            scroll().h(height).rounded(px(8.0)).bg(move || tok("hover").opacity(0.5)).child(each(
                move || items().into_iter().enumerate().collect::<Vec<_>>(),
                |(i, (t, (_, exists)))| format!("{i}:{t}:{exists}"),
                move |item, _| {
                    let i = move || item.with(|x| x.0);
                    let ok = move || item.with(|x| x.1.1.0);
                    let exists = move || item.with(|x| x.1.1.1);
                    view().child(when(move || i() > 0, divider)).child(
                        view()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .h(px(PARSED_ROW))
                            .px(px(8.0))
                            .child(text(move || item.with(|x| x.1.0.clone())).flex_1().number_of_lines(1).text_size(px(12.5)))
                            .child(
                                text(move || {
                                    s.label(if exists() {
                                        "Already in Safay"
                                    } else if ok() {
                                        "Parsed"
                                    } else {
                                        "Unable to parse"
                                    })
                                })
                                .text_size(px(11.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(move || {
                                    if exists() {
                                        theme().muted_foreground
                                    } else if ok() {
                                        tok("success")
                                    } else {
                                        theme().destructive
                                    }
                                }),
                            ),
                    )
                },
            ))
        },
    )
}
/// Which scan source the user picked last (drives the selected card and the hint).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Screen,
    Images,
    Camera,
}
fn scan_panel(s: Store) -> impl IntoElement {
    let over = signal(false);
    let source = signal(None::<Source>);
    let camera_on = signal(false);
    let camera_failed = signal(false);
    let migration = move || {
        let scans: Vec<_> = s.scanned.get().lines().map(|l| otpauth::parse_scan_result(l, &placeholders())).collect();
        scans.iter().find(|x| x.is_google_migration && x.batch_size.unwrap_or(1) > 1).map(|x| {
            let size = x.batch_size.unwrap_or(1);
            let seen = scans.iter().filter(|y| y.is_google_migration && y.batch_id == x.batch_id).count() as i64;
            (seen, size)
        })
    };
    view()
        .gap(px(12.0))
        .on_file_drag_enter(move |_: &()| over.set(true))
        .on_file_drag_leave(move |_: &()| over.set(false))
        .on_file_drop(move |e: &nsh_core::element::FileDropEvent| {
            over.set(false);
            let paths: Vec<_> = e.uris.iter().filter_map(|u| super::unlock::file_path(u)).take(20).collect();
            spawn(async move {
                let mut images = Vec::new();
                for p in paths {
                    if let Ok((_, b)) = crate::platform::files::read_named(p).await {
                        images.push(b);
                    }
                }
                s.add_images(images).await;
            });
        })
        // The row: screen, images, camera, a third of the row each. A source this desktop lacks leaves its
        // third empty rather than stretching the others (docs/PLATFORMS.md).
        .child(
            view()
                .flex_row()
                .gap(px(10.0))
                .child(if crate::platform::supports::SCREEN_CAPTURE {
                    source_card(s, Icon::Screen, "Select on Screen", "A QR code in a web page or another app", move || {
                        source.get() == Some(Source::Screen)
                    })
                    .on_press(move || {
                        source.set(Some(Source::Screen));
                        #[cfg(any(target_os = "macos", target_os = "linux"))]
                        s.capture_screen();
                    })
                    .into_element()
                } else {
                    view().flex_1().into_element()
                })
                .child(
                    source_card(s, Icon::Photo, "Choose Images", "Screenshots or photos; pick several at once", move || {
                        over.get() || source.get() == Some(Source::Images)
                    })
                    .on_press(move || {
                        source.set(Some(Source::Images));
                        s.choose_images()
                    }),
                )
                .child(if crate::platform::supports::CAMERA {
                    source_card(s, Icon::Camera, "Use Camera", "Point at a QR code on a phone or paper", move || {
                        source.get() == Some(Source::Camera)
                    })
                    .on_press(move || {
                        source.set(Some(Source::Camera));
                        camera_failed.set(false);
                        s.status.set(None);
                        camera_on.set(true);
                    })
                    .into_element()
                } else {
                    view().flex_1().into_element()
                }),
        )
        .child(when(move || camera_on.get(), move || camera_view(s, camera_on, camera_failed, migration)))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(6.0))
                .text_color(move || theme().muted_foreground)
                .child(
                    when(
                        move || parsed(&s.scanned.get()).iter().any(|p| p.is_valid()) && migration().is_none_or(|(a, b)| a >= b),
                        move || crate::icons::symbol(Icon::CheckCircle, 12.0, move || tok("success")),
                    )
                    .otherwise(move || {
                        when(
                            move || source.get() == Some(Source::Screen),
                            move || {
                                crate::icons::symbol(if screen_ready() { Icon::Screen } else { Icon::LockShield }, 12.0, move || {
                                    theme().muted_foreground
                                })
                            },
                        )
                        .otherwise(move || {
                            when(
                                move || source.get() == Some(Source::Images),
                                move || crate::icons::symbol(Icon::Photo, 12.0, move || theme().muted_foreground),
                            )
                            .otherwise(move || {
                                when(
                                    move || source.get() == Some(Source::Camera),
                                    move || {
                                        crate::icons::symbol(
                                            if camera_failed.get() { Icon::Refresh } else { Icon::CameraViewfinder },
                                            12.0,
                                            move || theme().muted_foreground,
                                        )
                                    },
                                )
                            })
                            .otherwise(move || crate::icons::symbol(Icon::Hand, 12.0, move || theme().muted_foreground))
                        })
                    }),
                )
                .child(
                    text(move || {
                        if s.busy.get() {
                            return s.label("Recognizing");
                        }
                        if let Some((seen, size)) = migration()
                            && seen < size
                        {
                            return s.fmt("Scanned Google migration QR {} / {}", &[&seen.to_string(), &size.to_string()]);
                        }
                        let n = parsed(&s.scanned.get()).iter().filter(|p| p.is_valid()).count();
                        if n > 0 {
                            return s.count("recognized", n);
                        }
                        // The hint for the source picked last.
                        match source.get() {
                            // Linux: the desktop's own screenshot tool picks the area, or takes the whole screen.
                            Some(Source::Screen) if cfg!(target_os = "linux") => s.label("Take a screenshot that shows the QR code"),
                            Some(Source::Screen) if screen_ready() => s.label("Drag to select the QR code on screen"),
                            Some(Source::Screen) => s.label("Needs Screen Recording permission the first time"),
                            Some(Source::Images) => s.label("You can choose several images at once"),
                            // On error the footer explains why; here only how to try again.
                            Some(Source::Camera) if camera_failed.get() => s.label("Click the card to try again"),
                            Some(Source::Camera) => s.label("Point the camera at the QR code"),
                            None if crate::platform::supports::SCREEN_CAPTURE => s.label("Choose how to scan"),
                            None => s.label("Choose images or drop them here"),
                        }
                    })
                    .text_size(px(12.0))
                    .text_color(move || {
                        if parsed(&s.scanned.get()).iter().any(|p| p.is_valid()) { tok("success") } else { theme().muted_foreground }
                    }),
                ),
        )
        .child(parsed_list(s, move || s.scanned.get()))
}
/// The camera preview: 180pt, rounded, "Starting camera" until frames arrive, a close button in the corner.
/// Polls the camera module; stops it when hidden. Keeps scanning only while a Google migration batch is incomplete.
fn camera_view(
    s: Store,
    camera_on: Signal<bool>,
    camera_failed: Signal<bool>,
    migration: impl Fn() -> Option<(i64, i64)> + Copy + 'static,
) -> impl IntoElement {
    let frame = signal(None::<String>);
    let ready = signal(false);
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let _ = (camera_failed, migration);
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        crate::platform::camera::start();
        cleanup(crate::platform::camera::stop);
        set_interval(std::time::Duration::from_millis(100), move || {
            let poll = crate::platform::camera::poll();
            if let Some(error) = poll.error {
                camera_failed.set(true);
                camera_on.set(false);
                let key = match error {
                    crate::platform::camera::CameraError::Denied => {
                        // Linux has no common settings page to send the user to: the portal asks again next time.
                        #[cfg(target_os = "macos")]
                        drop(open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_Camera"));
                        "Camera access not allowed"
                    }
                    crate::platform::camera::CameraError::Unavailable => "Can’t open the camera",
                };
                s.status.set(Some(Status { tone: Tone::Error, text: s.label(key) }));
                return;
            }
            if poll.ready != ready.get() {
                ready.set(poll.ready);
            }
            if poll.frame.is_some() {
                frame.set(poll.frame);
            }
            if let Some(text) = poll.result
                && text.lines().any(|l| otpauth::parse_scan_result(l, &placeholders()).accounts.iter().any(|a| a.is_valid()))
            {
                s.append_scanned(&text);
                if migration().is_none_or(|(seen, size)| seen >= size) {
                    camera_on.set(false);
                }
            }
        });
    }
    view()
        .h(px(180.0))
        .rounded(px(10.0))
        .overflow_hidden()
        .bg(move || theme().foreground.opacity(0.05))
        .items_center()
        .justify_center()
        .child(when(
            move || frame.with(|f| f.is_some()),
            move || image(move || frame.get().unwrap_or_default()).absolute().inset(px(0.0)).overflow_hidden(),
        ))
        .child(when(
            move || !ready.get(),
            move || {
                view()
                    .items_center()
                    .gap(px(8.0))
                    .child(spinner(16.0))
                    .child(text(move || s.label("Starting camera")).text_size(px(12.0)).text_color(move || theme().muted_foreground))
            },
        ))
        .child(
            view().absolute().top(px(8.0)).right(px(8.0)).child(
                button()
                    .accessibility_label(move || Some(s.label("Turn Off Camera")))
                    .size(px(24.0))
                    .rounded_full()
                    .items_center()
                    .justify_center()
                    .bg(move || tok("glass").opacity(0.85))
                    .active(|st| st.opacity(0.7))
                    .on_press(move || camera_on.set(false))
                    .child(crate::icons::symbol_weight(Icon::Close, 11.0, 0.3, move || theme().foreground)),
            ),
        )
}
fn screen_ready() -> bool {
    #[cfg(target_os = "macos")]
    return crate::platform::screen_capture::has_permission();
    // Linux: the portal asks for any permission itself, while the screenshot is taken.
    #[cfg(target_os = "linux")]
    return true;
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    false
}
fn source_card(
    s: Store,
    glyph: Icon,
    title: &'static str,
    detail: &'static str,
    selected: impl Fn() -> bool + Copy + 'static,
) -> nsh_base::ButtonElement {
    let over = selected;
    button()
        .accessibility_label(move || Some(s.label(title)))
        .disabled(move || s.busy.get())
        .flex_1()
        .gap(px(6.0))
        // 12pt from the edge: the padding plus the border.
        .p(move || px(if over() { 10.5 } else { 11.0 }))
        .rounded(px(10.0))
        .bg(move || if over() { tok("code-accent").opacity(0.08) } else { theme().foreground.opacity(0.04) })
        .border(move || px(if over() { 1.5 } else { 1.0 }))
        .border_color(move || if over() { tok("code-accent").opacity(0.8) } else { tok("separator").opacity(0.6) })
        .hover(|st| st.bg(move || theme().foreground.opacity(0.07)))
        .focus(|st| st.border_color(move || theme().ring))
        .active(|st| st.scale(0.99))
        .disabled_style(|st| st.opacity(0.6))
        .child(
            view()
                .h(px(24.0))
                .justify_center()
                .child(crate::icons::symbol(glyph, 20.0, move || if over() { tok("code-accent") } else { theme().foreground })),
        )
        .child(text(move || s.label(title)).text_size(px(13.0)).font_weight(FontWeight::SEMIBOLD))
        .child(text(move || s.label(detail)).number_of_lines(2).text_size(px(11.0)).text_color(move || theme().muted_foreground))
}

// ---------------------------------------------------------------- details & QR

fn detail_sheet(s: Store) -> impl IntoElement {
    let id = move || match s.sheet.get() {
        Some(Sheet::Detail(id)) => Some(id),
        _ => None,
    };
    let row = move || id().and_then(|id| s.projection.with(|p| p.rows.iter().find(|r| r.id == id).cloned()));
    let website_ok = move || s.form.with(|f| f.website.trim().is_empty() || site_icon::normalized_website(&f.website).is_some());
    let done = move || {
        if let Some(id) = id()
            && website_ok()
        {
            s.save_detail(id)
        }
    };
    sheet(
        s,
        540.0,
        0.0,
        move || s.label("Account Details"),
        view(),
        when(
            move || s.detail.get().is_some(),
            move || {
                view()
                    .gap(px(18.0))
                    .child(
                        view()
                            .flex_row()
                            .items_center()
                            .gap(px(14.0))
                            .child(badge_with_icon(
                                move || s.detail.with(|d| d.as_ref().map(|d| d.issuer.clone()).unwrap_or_default()),
                                || false,
                                move || row().and_then(|r| r.icon),
                                46.0,
                            ))
                            .child(
                                view()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .gap(px(2.0))
                                    .child(
                                        heading(
                                            move || s.detail.with(|d| d.as_ref().map(|d| d.issuer.clone()).unwrap_or_default()).into(),
                                            15.0,
                                        )
                                        .number_of_lines(1),
                                    )
                                    .child(
                                        note(move || s.detail.with(|d| d.as_ref().map(|d| d.username.clone()).unwrap_or_default()).into())
                                            .number_of_lines(1),
                                    ),
                            )
                            .child(
                                button()
                                    .accessibility_label(move || Some(s.label("Copy code")))
                                    .rounded(px(8.0))
                                    .px(px(6.0))
                                    .hover(|st| st.bg(move || tok("hover")))
                                    .focus(focus_ring)
                                    .on_press(move || {
                                        if let Some(id) = id() {
                                            s.copy(id)
                                        }
                                    })
                                    .items_end()
                                    .gap(px(3.0))
                                    .child(
                                        view()
                                            .flex_row()
                                            .items_center()
                                            .gap(px(8.0))
                                            .child(when(
                                                move || s.copied.get().is_some() && s.copied.get() == id(),
                                                move || {
                                                    text(move || s.label("Copied"))
                                                        .text_size(px(13.0))
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .text_color(move || tok("success"))
                                                },
                                            ))
                                            .child(
                                                text(move || row().map(|r| r.code).unwrap_or_default())
                                                    .font_family(CODE_FONT.family)
                                                    .text_size(px(26.0))
                                                    .font_weight(FontWeight::SEMIBOLD),
                                            ),
                                    )
                                    .child(
                                        view()
                                            .flex_row()
                                            .items_center()
                                            .gap(px(5.0))
                                            .child(code_ring(move || row().map(|r| (r.remaining, r.period)), 12.0))
                                            .child(
                                                // Same-width digits, and a figure space before one digit: the line keeps
                                                // its width as the seconds count down, so the ring beside it stays put.
                                                note(move || {
                                                    s.fmt(
                                                        "Refreshes in {}s",
                                                        &[&format!("{:\u{2007}>2}", row().map(|r| r.remaining).unwrap_or(0))],
                                                    )
                                                })
                                                .text_size(px(11.0))
                                                .tabular_nums(),
                                            ),
                                    ),
                            ),
                    )
                    .child(
                        view()
                            .flex_row()
                            .gap(px(12.0))
                            .child(
                                labeled(
                                    move || s.label("Service Name"),
                                    text_field(
                                        s,
                                        "service",
                                        move || s.label("Service Name"),
                                        move || s.label("Service Name"),
                                        move || s.form.with(|f| f.issuer.clone()),
                                        move |v| s.form.update(|f| f.issuer = v.into()),
                                        false,
                                        || {},
                                    ),
                                )
                                .flex_1(),
                            )
                            .child(
                                labeled(
                                    move || s.label("Account / Email"),
                                    text_field(
                                        s,
                                        "username",
                                        move || s.label("Account / Email"),
                                        move || s.label("Account / Email"),
                                        move || s.form.with(|f| f.username.clone()),
                                        move |v| s.form.update(|f| f.username = v.into()),
                                        false,
                                        || {},
                                    ),
                                )
                                .flex_1(),
                            ),
                    )
                    .child(
                        view()
                            .gap(px(4.0))
                            .child(labeled(
                                move || s.label("Website"),
                                text_field(
                                    s,
                                    "website",
                                    move || s.label("Website"),
                                    move || s.label("Optional"),
                                    move || s.form.with(|f| f.website.clone()),
                                    move |v| {
                                        s.form.update(|f| f.website = v.into());
                                        s.marks.update(|m| m.website_error = false)
                                    },
                                    false,
                                    move || {
                                        let bad = s
                                            .form
                                            .with(|f| !f.website.trim().is_empty() && site_icon::normalized_website(&f.website).is_none());
                                        s.marks.update(|m| m.website_error = bad)
                                    },
                                ),
                            ))
                            .child(field_message(move || {
                                s.marks.with(|m| m.website_error).then(|| s.label("Enter a website like example.com"))
                            })),
                    )
                    .child(info_section(s, row))
                    .child(secret_section(s, id))
            },
        ),
        view()
            .flex_1()
            .flex_row()
            .items_center()
            .gap(px(crate::chrome::footer_gap()))
            .child(push_button(move || s.label("Delete Account"), Kind::Danger, false).on_press(move || {
                if let Some(id) = id() {
                    s.delete_account(id)
                }
            }))
            .child(view().flex_1().min_w(px(0.0)).child(footer_error(s)))
            .child(cancel_button(s))
            .child(
                push_button(move || s.label("Done"), Kind::Primary, false).disabled(move || !website_ok() || s.busy.get()).on_press(done),
            ),
        move || s.close_sheet(),
    )
}
fn info_section(s: Store, row: impl Fn() -> Option<safay_app::session::Row> + Copy + 'static) -> impl IntoElement {
    let info = |s: Store, title: &'static str, value: Box<dyn Fn() -> SharedString>, control: Option<AnyElement>| {
        view()
            .flex_row()
            .items_center()
            .min_h(px(36.0))
            .px(px(10.0))
            .child(text(move || s.label(title)).flex_1().text_size(px(12.5)).text_color(move || theme().muted_foreground))
            .child(match control {
                Some(c) => c,
                None => text(value).text_size(px(12.5)).into_element(),
            })
    };
    let detail = move |f: fn(&safay_app::session::Detail) -> String| move || s.detail.with(|d| d.as_ref().map(f).unwrap_or_default());
    let categories = move || {
        let r = row();
        s.projection.with(|p| {
            let names: Vec<_> =
                p.categories.iter().filter(|c| r.as_ref().is_some_and(|r| r.categories.contains(&c.id))).map(|c| c.name.clone()).collect();
            if names.is_empty() { s.label("None").to_string() } else { names.join(", ") }
        })
    };
    view()
        .rounded(px(9.0))
        .bg(move || tok("hover").opacity(0.6))
        .child(info(
            s,
            "Categories",
            Box::new(|| "".into()),
            Some(
                button()
                    .flex_row()
                    .items_center()
                    .gap(px(4.0))
                    .rounded(px(5.0))
                    .px(px(6.0))
                    .py(px(2.0))
                    .hover(|st| st.bg(move || tok("hover")))
                    .focus(focus_ring)
                    .on_press(move || {
                        if let Some(r) = row() {
                            category_menu(s, &r)
                        }
                    })
                    .child(text(categories).number_of_lines(1).max_w(px(240.0)).text_size(px(12.5)))
                    .child(view().text_color(move || theme().muted_foreground).child(icon(Icon::ChevronUpDown).size(px(11.0))))
                    .into_element(),
            ),
        ))
        .child(divider())
        .child(info(
            s,
            "Algorithm",
            Box::new(move || {
                s.detail.with(|d| {
                    d.as_ref()
                        .map(|d| {
                            match d.otp_type {
                                OtpType::Steam => return s.label("Steam"),
                                OtpType::BattleNet => return s.label("Battle.net"),
                                OtpType::Totp => {}
                            }
                            let alg = match d.algorithm {
                                safay_app::core::TotpAlgorithm::Sha1 => "SHA1",
                                safay_app::core::TotpAlgorithm::Sha256 => "SHA256",
                                safay_app::core::TotpAlgorithm::Sha512 => "SHA512",
                            };
                            s.fmt("{} · {} digits · {} sec", &[alg, &d.digits.to_string(), &d.period.to_string()])
                        })
                        .unwrap_or_default()
                })
            }),
            None,
        ))
        .child(divider())
        .child(info(
            s,
            "Added via",
            Box::new(move || {
                s.label(if s.detail.with(|d| d.as_ref().is_some_and(|d| d.imported)) { "Imported" } else { "Added manually" })
            }),
            None,
        ))
        .child(divider())
        .child(info(s, "Date Added", Box::new(move || detail(|d| format_date(d.created_at))().into()), None))
}
fn category_menu(s: Store, row: &safay_app::session::Row) {
    use nsh_core::element::desktop::MenuItem;
    let id = row.id;
    let items: Vec<MenuItem> = s.projection.with(|p| {
        p.categories
            .iter()
            .map(|c| {
                let member = row.categories.contains(&c.id);
                let cid = c.id;
                MenuItem::new(c.name.clone()).checked(member).on_select(move || s.membership(cid, id, !member))
            })
            .collect()
    });
    let items = if items.is_empty() { vec![MenuItem::new(s.label("None").to_string()).disabled(true)] } else { items };
    show_menu(s, items, None);
}
/// A short, locale-neutral date and time (UTC) without a time-zone database.
fn format_date(t: std::time::SystemTime) -> String {
    let name = crate::platform::files::export_file_name(t);
    let d = &name["Safay-export-".len()..name.len() - ".json".len()];
    format!("{}-{}-{} {}:{} UTC", &d[0..4], &d[4..6], &d[6..8], &d[9..11], &d[11..13])
}
fn secret_section(s: Store, id: impl Fn() -> Option<Uuid> + Copy + 'static) -> impl IntoElement {
    let shown = move || !s.revealed.with(|r| r.is_empty());
    view()
        .gap(px(6.0))
        .child(section_label(move || s.label("Secret")))
        .child(input_chrome(
            view()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .min_h(px(36.0))
                .pl(px(12.0))
                .pr(px(6.0))
                .child(
                    text(move || {
                        if shown() { s.revealed.get().to_string() } else { "••••••••••••••••".to_string() }
                    })
                    .test_id("secret-value")
                    .selectable(true)
                    .flex_1()
                    .number_of_lines(1)
                    .font_family(move || SharedString::from(if shown() { CODE_FONT.family } else { "system-ui" }))
                    .text_size(px(13.0))
                    .text_color(move || if shown() { theme().foreground } else { theme().muted_foreground }),
                )
                .child(
                    icon_button_with(move || s.label(if shown() { "Hide" } else { "Show Secret" }), eye_glyph(shown))
                        .disabled(move || s.busy.get())
                        .on_press(move || {
                            if shown() {
                                s.revealed.set(Zeroizing::new(String::new()));
                            } else if let Some(id) = id() {
                                s.reveal(Auth::Secret(id))
                            }
                        }),
                ),
            7.0,
        ))
        .child(
            view()
                .flex_row()
                .items_center()
                .child(note(move || s.label("Showing the secret or QR code requires your master password.")).flex_1())
                .child(push_button(move || s.label("Show QR Code"), Kind::Link, false).on_press(move || {
                    if let Some(id) = id() {
                        s.reveal(Auth::Qr(id))
                    }
                })),
        )
}
fn qr_sheet(s: Store) -> impl IntoElement {
    let id = move || match s.sheet.get() {
        Some(Sheet::Qr(id)) => Some(id),
        _ => None,
    };
    let row = move || id().and_then(|id| s.projection.with(|p| p.rows.iter().find(|r| r.id == id).cloned()));
    let copied = signal(false);
    on_key_with("escape", KeyOptions { when: nsh_core::keymap::When::Always, ..Default::default() }, move |_| {
        if s.auth.get().is_none() {
            s.close_sheet()
        }
    });
    nsh_base::dialog().animation(nsh_core::ModalAnimation::Fade).on_cancel(move |_: &()| s.close_sheet()).child(
        modal_backdrop().child(
            modal_surface(view().role(nsh_core::AccessibilityRole::Alert))
                .w(px(420.0))
                .child(
                    view()
                        .items_center()
                        .gap(px(16.0))
                        .px(px(22.0))
                        .pt(px(28.0))
                        .pb(px(22.0))
                        .child(
                            view()
                                .items_center()
                                .gap(px(4.0))
                                .child(heading(move || row().map(|r| r.issuer).unwrap_or_default().into(), 18.0))
                                .child(note(move || row().map(|r| r.username).unwrap_or_default().into()).text_size(px(12.5))),
                        )
                        .child(
                            view()
                                .size(px(250.0))
                                .rounded(px(8.0))
                                .bg(nsh_core::Rgba { r: 1.0, g: 1.0, b: 1.0, a: 1.0 })
                                .items_center()
                                .justify_center()
                                .overflow_hidden()
                                .child(when(
                                    move || !s.qr.with(|q| q.is_empty()),
                                    move || {
                                        image(move || s.qr.with(|v| v.to_string()))
                                            .size(px(250.0))
                                            .overflow_hidden()
                                            .label(move || s.label("QR code"))
                                    },
                                )),
                        )
                        .child(input_chrome(
                            text(move || s.qr_uri.get().to_string())
                                .selectable(true)
                                .number_of_lines(3)
                                .p(px(10.0))
                                .w_full()
                                .font_family(CODE_FONT.family)
                                .text_size(px(11.0))
                                .text_color(move || theme().muted_foreground),
                            7.0,
                        ))
                        .child(
                            push_button(move || s.label(if copied.get() { "Copied" } else { "Copy URI" }), Kind::Primary, true)
                                .rounded_full()
                                .min_w(px(154.0))
                                .on_press(move || {
                                    let text = s.qr_uri.get();
                                    let controller = s.controller.get();
                                    let epoch = controller.epoch();
                                    let clipboard: std::sync::Arc<dyn safay_app::clipboard::SecretClipboard> = if s.fixture {
                                        std::sync::Arc::new(safay_app::clipboard::MemoryClipboard::new(std::sync::Arc::new(
                                            crate::FixtureClock,
                                        )))
                                    } else {
                                        std::sync::Arc::new(crate::platform::clipboard::NativeClipboard)
                                    };
                                    if clipboard.copy(text, std::time::Duration::from_secs(60), controller, epoch).is_ok() {
                                        copied.set(true);
                                        set_timeout(std::time::Duration::from_millis(1100), move || copied.set(false));
                                    }
                                }),
                        ),
                )
                .child(
                    view().absolute().top(px(14.0)).right(px(14.0)).child(
                        icon_button(move || s.label("Close"), Icon::Close)
                            .size(px(30.0))
                            .rounded_full()
                            .bg(move || theme().secondary)
                            .on_press(move || s.close_sheet()),
                    ),
                ),
        ),
    )
}

// ---------------------------------------------------------------- export / import

fn export_import_sheet(s: Store) -> impl IntoElement {
    let export_ok = move || !s.selected.with(|x| x.is_empty()) && new_password_ok(s);
    let export = move || {
        if export_ok() && !s.busy.get() {
            let p = s.form.with(|f| f.new_password.clone());
            s.confirm_fair(p, move |_| s.reveal(Auth::Export));
        }
    };
    sheet(
        s,
        660.0,
        300.0,
        move || s.label("Import / Export"),
        segmented(
            vec![
                (true, Box::new(move || s.label("Batch Import")) as Box<dyn Fn() -> SharedString>),
                (false, Box::new(move || s.label("Encrypted Export"))),
            ],
            move || s.import_tab.get(),
            move |v| {
                s.status.set(None);
                s.error.set(None);
                s.import_tab.set(v)
            },
        )
        .w(px(260.0)),
        view().child(when(move || !s.import_tab.get(), move || export_tab(s, export)).otherwise(move || import_tab(s))),
        view()
            .flex_1()
            .flex_row()
            .items_center()
            .gap(px(crate::chrome::footer_gap()))
            .child(view().flex_1().min_w(px(0.0)).child(status_line(move || {
                s.error
                    .get()
                    .map(|e| Status { tone: Tone::Error, text: s.label(crate::i18n::error_key(e)) })
                    .or_else(|| s.status.get())
                    // The export form shows its confirmation mismatch here, in the footer.
                    .or_else(|| {
                        (!s.import_tab.get()).then(|| super::unlock::mismatch(s)).flatten().map(|text| Status { tone: Tone::Error, text })
                    })
            })))
            .child(cancel_button(s))
            .child(
                when(
                    move || !s.import_tab.get(),
                    move || {
                        push_button(move || s.label("Export Encrypted File"), Kind::Primary, false)
                            .disabled(move || !export_ok() || s.busy.get())
                            .on_press(export)
                    },
                )
                .otherwise(move || {
                    push_button(move || s.label("Import Accounts"), Kind::Primary, false)
                        .disabled(move || {
                            !s.has_file()
                                || s.busy.get()
                                || match s.file_kind.get() {
                                    FileKind::Unsupported => true,
                                    FileKind::Encrypted => s.form.with(|f| f.password.trim().is_empty()),
                                    FileKind::Plain => false,
                                }
                        })
                        .on_press(move || s.import_file())
                }),
            ),
        move || s.close_sheet(),
    )
}
fn export_tab(s: Store, submit: impl Fn() + Copy + 'static) -> impl IntoElement {
    let visible = move || {
        let q = s.choice_query.get();
        s.choices
            .with(|c| c.iter().filter(|c| safay_app::core::search::matches_search(&c.issuer, &c.username, &q)).cloned().collect::<Vec<_>>())
    };
    let all_selected = move || s.choices.with(|c| c.len()) == s.selected.with(|x| x.len());
    view()
        .gap(px(10.0))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(10.0))
                .child(input_chrome(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(px(6.0))
                        .w(px(200.0))
                        .h(px(26.0))
                        .px(px(8.0))
                        .text_color(move || theme().muted_foreground)
                        .child(icon(Icon::Search).size(px(13.0)))
                        .child(
                            input()
                                .label(move || s.label("Search accounts"))
                                .placeholder(move || s.label("Search accounts").to_string())
                                .placeholder_color(move || {
                                    let c = theme().muted_foreground;
                                    if dark() { c.dark } else { c.light }
                                })
                                .value(move || s.choice_query.get())
                                .on_change(move |v: &str| s.choice_query.set(v.into()))
                                .flex_1()
                                .min_w(px(0.0))
                                .text_size(px(12.5))
                                .text_color(move || theme().foreground),
                        ),
                    6.0,
                ))
                .child(
                    note(move || {
                        s.fmt("{} of {} selected", &[&s.selected.with(|x| x.len()).to_string(), &s.choices.with(|c| c.len()).to_string()])
                    })
                    .text_size(px(12.0)),
                )
                .child(spacer())
                .child(when(
                    move || s.projection.with(|p| !p.categories.is_empty()),
                    move || {
                        // A borderless menu: primary text and a small chevron.
                        button()
                            .accessibility_label(move || Some(s.label("Select by Category")))
                            .flex_row()
                            .items_center()
                            .gap(px(4.0))
                            .active(|st| st.opacity(0.7))
                            .child(text(move || s.label("Select by Category")).text_size(px(12.5)))
                            .child(crate::icons::symbol_weight(Icon::ChevronDown, 9.0, 0.3, move || theme().foreground))
                            .on_press(move || {
                                use nsh_core::element::desktop::MenuItem;
                                let items = s.projection.with(|p| {
                                    p.categories
                                        .iter()
                                        .map(|c| {
                                            let cid = c.id;
                                            MenuItem::new(s.fmt("{} ({})", &[&c.name, &c.count.to_string()]).to_string()).on_select(
                                                move || {
                                                    let members: std::collections::BTreeSet<Uuid> = s.projection.with(|p| {
                                                        p.rows.iter().filter(|r| r.categories.contains(&cid)).map(|r| r.id).collect()
                                                    });
                                                    s.selected.set(members);
                                                },
                                            )
                                        })
                                        .collect::<Vec<_>>()
                                });
                                show_menu(s, items, None);
                            })
                    },
                ))
                .child(link_button(move || s.label(if all_selected() { "Deselect All" } else { "Select All" }), 12.5).on_press(
                    move || {
                        if all_selected() {
                            s.selected.set(Default::default())
                        } else {
                            s.selected.set(s.choices.with(|c| c.iter().map(|c| c.id).collect()))
                        }
                    },
                )),
        )
        .child(
            scroll()
                .h(px(216.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(move || tok("separator").opacity(0.6))
                .bg(move || tok("input-bg").opacity(0.5))
                .child(each(
                    visible,
                    |c| c.id,
                    move |c, i| {
                        let id = move || c.with(|c| c.id);
                        let on = move || s.selected.with(|x| x.contains(&id()));
                        view().child(when(move || i.get() > 0, || view().ml(px(44.0)).child(divider()))).child(
                            nsh_base::checkbox()
                                .checked(on)
                                .on_change(move |v: &bool| {
                                    let v = *v;
                                    s.selected.update(|x| {
                                        if v {
                                            x.insert(id());
                                        } else {
                                            x.remove(&id());
                                        }
                                    })
                                })
                                .flex_row()
                                .items_center()
                                .gap(px(10.0))
                                .px(px(12.0))
                                .h(px(34.0))
                                .hover(|st| st.bg(move || tok("hover")))
                                .focus(|st| st.bg(move || theme().accent))
                                // The box sits in the checkbox's indicator slot; an empty slot would take a gap of its own.
                                // A checkbox sits 6pt from its label; the label's own parts are 10pt apart.
                                .indicator(view().mr(px(-4.0)).child(check_box(on)))
                                .child(badge_with_icon(
                                    move || c.with(|c| c.issuer.clone()),
                                    || false,
                                    move || c.with(|c| c.icon.clone()),
                                    22.0,
                                ))
                                .child(
                                    text(move || c.with(|c| c.issuer.clone()))
                                        .number_of_lines(1)
                                        .text_size(px(12.5))
                                        .font_weight(FontWeight::MEDIUM),
                                )
                                .child(note(move || c.with(|c| c.username.clone()).into()).number_of_lines(1).flex_1().text_size(px(12.0))),
                        )
                    },
                )),
        )
        .child(view().pt(px(6.0)).child(new_password_fields(s, "Set Export Password", "Confirm Password", submit, true, true)))
        .child(note(move || s.label("The export is encrypted with this password and you’ll need it to import. Don’t lose it.")))
}
fn import_tab(s: Store) -> impl IntoElement {
    view()
        .gap(px(12.0))
        .child(
            when(move || !s.has_file(), move || drop_zone(s, "Drop a backup or export file here, or click to choose")).otherwise(
                move || {
                    view()
                        .gap(px(12.0))
                        .child(
                            view()
                                .flex_row()
                                .items_center()
                                .gap(px(10.0))
                                .h(px(40.0))
                                .px(px(12.0))
                                .rounded(px(8.0))
                                .bg(move || tok("hover").opacity(0.6))
                                .child(view().text_color(move || tok("code-accent")).child(icon(Icon::Doc)))
                                .child(text(move || s.file_name.get()).flex_1().number_of_lines(1).text_size(px(13.0)))
                                .child(icon_button(move || s.label("Remove File"), Icon::Close).on_press(move || s.remove_file())),
                        )
                        // Only an encrypted file asks for its password.
                        .child(when(
                            move || s.file_kind.get() == FileKind::Encrypted,
                            move || {
                                labeled(
                                    move || s.label("File Password"),
                                    secure_field(
                                        s,
                                        "import-password",
                                        move || s.label("Password for encrypted backups"),
                                        move || s.form.with(|f| f.password.to_string()),
                                        move |v| s.form.update(|f| f.password = Zeroizing::new(v.into())),
                                        move || s.import_file(),
                                        || {},
                                        true,
                                    ),
                                )
                            },
                        ))
                },
            ),
        )
        .child(note(move || s.label("Supports Safay, Aegis, 2FAS, Ente Auth, Proton Authenticator, Authenticator, and Bitwarden, plus CSV exports from Apple Passwords and other password managers. Existing accounts are skipped.")))
}

// ---------------------------------------------------------------- password, restore, auth

fn change_password_sheet(s: Store) -> impl IntoElement {
    let ok = move || s.form.with(|f| !f.password.is_empty()) && new_password_ok(s);
    let submit = move || {
        if ok() && !s.busy.get() {
            s.change_password()
        }
    };
    sheet(
        s,
        460.0,
        0.0,
        move || s.label("Change Master Password"),
        view(),
        view()
            .gap(px(14.0))
            .child(view().gap(px(7.0)).child(field_label(s, "Current Master Password")).child(secure_field(
                s,
                "password",
                move || s.label("Current Master Password"),
                move || s.form.with(|f| f.password.to_string()),
                move |v| {
                    s.error.set(None);
                    s.form.update(|f| f.password = Zeroizing::new(v.into()))
                },
                submit,
                || {},
                true,
            )))
            .child(when(
                move || s.error.get().is_some(),
                move || {
                    inline_notice(
                        Icon::Warning,
                        move || theme().destructive,
                        move || s.error.get().map(|e| s.label(crate::i18n::error_key(e))).unwrap_or_default(),
                    )
                },
            ))
            .child(new_password_fields(s, "New Password", "Confirm New Password", submit, false, false))
            .child(inline_notice(
                Icon::Info,
                move || tok("warning"),
                move || s.label("After changing, your vault is re-encrypted with the new password."),
            )),
        view().flex_1().flex_row().gap(px(crate::chrome::footer_gap())).child(spacer()).child(cancel_button(s)).child(
            push_button(move || s.label("Change Password"), Kind::Primary, false).disabled(move || !ok() || s.busy.get()).on_press(submit),
        ),
        move || s.close_sheet(),
    )
}
fn restore_sheet(s: Store) -> impl IntoElement {
    sheet(
        s,
        460.0,
        0.0,
        move || s.label("Restore Last Good Copy"),
        view(),
        view()
            .gap(px(14.0))
            .child(
                text(move || {
                    s.label("This replaces the current vault with its last saved recovery copy. Enter that copy’s master password.")
                })
                .text_size(px(13.0))
                .line_height(px(19.0)),
            )
            .child(secure_field(
                s,
                "password",
                move || s.label("Master Password"),
                move || s.form.with(|f| f.password.to_string()),
                move |v| s.form.update(|f| f.password = Zeroizing::new(v.into())),
                move || s.restore_snapshot(),
                || {},
                true,
            ))
            .child(field_message(move || s.error.get().map(|e| s.label(crate::i18n::error_key(e))))),
        view().flex_1().flex_row().gap(px(crate::chrome::footer_gap())).child(spacer()).child(cancel_button(s)).child(
            push_button(move || s.label("Restore"), Kind::Primary, false)
                .disabled(move || s.form.with(|f| f.password.is_empty()) || s.busy.get())
                .on_press(move || s.restore_snapshot()),
        ),
        move || s.close_sheet(),
    )
}

/// The master password prompt before a sensitive action, when Touch ID is not set up or did not verify (the system's
/// Touch ID prompt comes first when it is: `Store::reveal`). A compact alert: the app icon (with the lock screen's
/// fingerprint button on its corner when Touch ID is set up, to try it again), the title and a short prompt, the password field
/// (Return continues), a slim error line, then Cancel and Continue.
pub fn auth_dialog(s: Store) -> impl IntoElement {
    when(
        move || s.auth.get().is_some() && !s.auth_prompting.get(),
        move || {
            let submit = move || {
                if !s.auth_password.with(|p| p.is_empty()) && !s.busy.get() {
                    s.authorize()
                }
            };
            let message = move || {
                let waiting = s.projection.with(|p| p.waiting);
                if waiting > 0 {
                    Some(s.count("wait", waiting as usize))
                } else {
                    s.auth_error.get().map(|e| s.label(crate::i18n::error_key(e)))
                }
            };
            on_key_with("escape", KeyOptions { when: nsh_core::keymap::When::Always, ..Default::default() }, move |_| s.cancel_auth());
            nsh_base::dialog().animation(nsh_core::ModalAnimation::Fade).on_cancel(move |_: &()| s.cancel_auth()).child(
                modal_backdrop().child(
                    // Narrow, as a system alert: the field and buttons keep close to the icon above them.
                    modal_surface(view().role(nsh_core::AccessibilityRole::Alert))
                        .w(px(272.0))
                        .px(px(24.0))
                        .pt(px(24.0))
                        .pb(px(20.0))
                        .items_center()
                        .child(view().size(px(56.0)).child(app_icon(56.0)).child(when(
                            move || s.touch_id_ready(),
                            // The lock screen's Touch ID button at the menu bar panel's proportions.
                            move || {
                                view().absolute().right(px(-7.0)).bottom(px(-7.0)).child(
                                    button()
                                        .accessibility_label(move || Some(s.label("Use Touch ID")))
                                        .disabled(move || s.busy.get())
                                        .size(px(28.0))
                                        .rounded_full()
                                        .items_center()
                                        .justify_center()
                                        .bg(move || tok("glass"))
                                        .border_1()
                                        .border_color(move || tok("separator").opacity(0.5))
                                        .shadow([soft_shadow(3.0, 6.0, 0.10)])
                                        .text_color(move || tok("touchid"))
                                        .hover(|st| st.scale(1.08))
                                        .active(|st| st.scale(0.96))
                                        .focus(|st| st.border_color(move || theme().ring))
                                        .on_press(move || s.authorize_touch_id())
                                        .child(crate::icons::touch_id_glyph(19.5)),
                                )
                            },
                        )))
                        .child(view().h(px(18.0)))
                        .child(heading(move || s.label("Verify It’s You"), 15.0))
                        .child(
                            text(move || s.label("Enter your master password."))
                                .mt(px(5.0))
                                // Two even lines in English ("Enter your master password" / "to show the QR code."),
                                // not a last word alone.
                                .max_w(px(176.0) * font_scale())
                                .text_center()
                                .text_size(px(12.0))
                                .line_height(px(17.0))
                                .text_color(move || theme().muted_foreground),
                        )
                        .child(
                            secure_field(
                                s,
                                "auth-password",
                                move || s.label("Master Password"),
                                move || s.auth_password.get().to_string(),
                                move |v| {
                                    s.auth_error.set(None);
                                    s.auth_password.set(Zeroizing::new(v.into()))
                                },
                                submit,
                                || {},
                                true,
                            )
                            .mt(px(16.0))
                            .w_full(),
                        )
                        // The error line takes its height only while it shows.
                        .child(when(
                            move || message().is_some(),
                            move || {
                                // A long message wraps inside the dialog, the glyph beside its first line.
                                view()
                                    .mt(px(8.0))
                                    .w_full()
                                    .flex_row()
                                    .justify_center()
                                    .items_start()
                                    .gap(px(5.0))
                                    .text_color(move || theme().destructive)
                                    .child(view().pt(px(2.0)).child(icon(Icon::WarningFill).size(px(12.0))))
                                    .child(
                                        text(move || message().unwrap_or_default())
                                            .flex_shrink(1.0)
                                            .min_w(px(0.0))
                                            .text_size(px(11.5))
                                            .line_height(px(16.0)),
                                    )
                            },
                        ))
                        .child(
                            view()
                                .mt(px(18.0))
                                .flex_row()
                                .gap(px(10.0))
                                .w_full()
                                .child(
                                    push_button(move || s.label("Cancel"), Kind::Standard, false)
                                        .flex_1()
                                        .on_press(move || s.cancel_auth()),
                                )
                                .child(
                                    push_button(move || s.label("Continue"), Kind::Primary, false)
                                        .flex_1()
                                        .disabled(move || s.auth_password.with(|p| p.is_empty()) || s.busy.get())
                                        .on_press(submit),
                                ),
                        ),
                ),
            )
        },
    )
}
