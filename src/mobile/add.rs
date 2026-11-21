//! "Enter Setup Key": the service, the account and the key a service shows beside its QR code.
use super::{
    Nav,
    ui::{self, Entry},
};
use crate::store::Store;
use nsh_core::prelude::*;
use safay_app::{Zeroizing, core::totp};

pub fn manual(s: Store, nav: Nav) -> impl IntoElement {
    s.clear();
    let had = s.projection.with_untracked(|p| p.account_count);
    // Saved: back to the list, where the new account is.
    effect(move || {
        if s.projection.with(|p| p.account_count) > had {
            untracked(|| nav.home());
        }
    });
    // Pasted links, or the fields typed one by one.
    let paste = signal(false);
    let advanced = signal(false);
    let account = element_ref();
    let secret = element_ref();
    let valid = move || s.form.with(|f| !f.issuer.trim().is_empty() && totp::is_valid_secret(&f.secret));
    let save = move || {
        if s.busy.get() {
            return;
        }
        if !valid() {
            let bad = s.form.with(|f| !totp::is_valid_secret(&f.secret));
            s.marks.update(|m| m.secret_error = bad);
            return;
        }
        dismiss_keyboard();
        s.save_new()
    };
    let problem = move || {
        if let Some(e) = s.error.get() {
            return Some(s.label(if e == safay_app::session::Error::Duplicate {
                "Account already exists"
            } else {
                crate::i18n::error_key(e)
            }));
        }
        s.marks.with(|m| m.secret_error).then(|| s.label("Invalid secret key"))
    };
    view()
        .flex_1()
        .pt_safe_area()
        .child(ui::top_bar(s, Some("Enter Setup Key"), move || {
            nav.back();
        }))
        .child(
            scroll().flex_1().keyboard_dismiss_mode(KeyboardDismissMode::None).keyboard_taps(KeyboardTaps::Handled).child(
                view().items_center().pb_safe_area_plus(px(24.0)).child(
                    view()
                        .w_full()
                        .max_w(px(480.0))
                        .px(px(ui::MARGIN))
                        .pt(px(12.0))
                        .gap(px(16.0))
                        .child(ui::choice(
                            vec![
                                (false, Box::new(move || s.label("Manual Entry")) as Box<dyn Fn() -> SharedString>),
                                (true, Box::new(move || s.label("Paste otpauth"))),
                            ],
                            move || paste.get(),
                            move |v| {
                                s.error.set(None);
                                paste.set(v)
                            },
                        ))
                        .child(when(move || paste.get(), move || paste_mode(s)))
                        .child(when(
                            move || !paste.get(),
                            move || {
                                view()
                                    .gap(px(16.0))
                                    .child(
                                        ui::form_card()
                                            .gap(px(12.0))
                                            .child(ui::field(
                                                s,
                                                "issuer",
                                                Entry::Text,
                                                move || s.label("Service Name"),
                                                move || s.form.with(|f| f.issuer.clone()),
                                                move |v| {
                                                    s.error.set(None);
                                                    s.form.update(|f| f.issuer = v.to_owned())
                                                },
                                                move || account.focus(),
                                                |_| {},
                                                || false,
                                                ReturnKeyType::Next,
                                                None,
                                            ))
                                            .child(ui::field(
                                                s,
                                                "username",
                                                Entry::Text,
                                                move || s.label("Account / Email"),
                                                move || s.form.with(|f| f.username.clone()),
                                                move |v| {
                                                    s.error.set(None);
                                                    s.form.update(|f| f.username = v.to_owned())
                                                },
                                                move || secret.focus(),
                                                |_| {},
                                                || false,
                                                ReturnKeyType::Next,
                                                Some(account),
                                            ))
                                            .child(ui::field(
                                                s,
                                                "secret",
                                                Entry::Key,
                                                move || s.label("Secret"),
                                                move || s.form.with(|f| f.secret.to_string()),
                                                move |v| {
                                                    s.error.set(None);
                                                    s.form.update(|f| f.secret = Zeroizing::new(v.into()));
                                                    if s.marks.with(|m| m.secret_error) {
                                                        s.marks.update(|m| m.secret_error = false)
                                                    }
                                                },
                                                save,
                                                move |focused| {
                                                    if !focused {
                                                        let bad =
                                                            s.form.with(|f| !f.secret.is_empty() && !totp::is_valid_secret(&f.secret));
                                                        s.marks.update(|m| m.secret_error = bad)
                                                    }
                                                },
                                                move || s.marks.with(|m| m.secret_error),
                                                ReturnKeyType::Done,
                                                Some(secret),
                                            ))
                                            .child(ui::field_note(problem, || true)),
                                    )
                                    .child(advanced_options(s, advanced))
                                    .child(
                                        ui::primary_button(move || s.label("Add Account"), valid, move || s.busy.get())
                                            .test_id("save")
                                            .on_press(save),
                                    )
                            },
                        )),
                ),
            ),
        )
}

/// "Add Account": what a scan, photos or pasted links hold. Every valid entry is added with one press.
pub fn review(s: Store, nav: Nav) -> impl IntoElement {
    use crate::{
        components::badge_with_icon,
        icons::{Icon, icon},
    };
    use nsh_base::theme;
    let source = move || s.scanned.get();
    let items = move || safay_app::core::otpauth::parse_lines(&source(), &safay_app::session::placeholders());
    let valid = move || items().iter().filter(|p| p.is_valid()).count();
    let leave = move || {
        s.scanned.set(Default::default());
        nav.back();
    };
    // Added: the scanned list is spent, and the list shows what came in.
    let had = s.projection.with_untracked(|p| p.account_count);
    effect(move || {
        if s.projection.with(|p| p.account_count) > had {
            untracked(|| {
                s.scanned.set(Default::default());
                nav.home()
            });
        }
    });
    view()
        .flex_1()
        .pt_safe_area()
        .child(ui::top_bar(s, Some("Add Account"), leave))
        .child(scroll().flex_1().child(view().px(px(ui::MARGIN)).pt(px(8.0)).pb(px(16.0)).gap(px(ui::GROUP_GAP)).child(each(
            move || {
                items()
                    .into_iter()
                    .enumerate()
                    .map(|(i, p)| {
                        (i, p.is_valid(), p.issuer.clone(), p.account.clone(), p.failure.map(|f| f.l10n_key()), p.digits, p.period)
                    })
                    .collect::<Vec<_>>()
            },
            |x| format!("{}:{}:{}", x.0, x.2, x.3),
            move |item, index| {
                let ok = move || item.with(|x| x.1);
                let count = move || items().len();
                ui::group_corners(
                    view().flex_row().items_center().gap(px(16.0)).px(px(16.0)).py(px(12.0)).min_h(px(72.0)).bg(move || theme().card),
                    move || index.get(),
                    count,
                )
                .child(when(ok, move || badge_with_icon(move || item.with(|x| x.2.clone()), || false, || None, 40.0)).otherwise(|| {
                    view()
                        .size(px(40.0))
                        .items_center()
                        .justify_center()
                        .text_color(move || theme().destructive)
                        .child(icon(Icon::Warning).size(px(24.0)))
                }))
                .child(
                    view()
                        .flex_1()
                        .min_w(px(0.0))
                        .child(
                            ui::body_large(text(move || {
                                item.with(|x| SharedString::from(if x.2.is_empty() { "—".to_owned() } else { x.2.clone() }))
                            }))
                            .number_of_lines(1),
                        )
                        .child(
                            ui::body(text(move || {
                                item.with(|x| match x.4 {
                                    Some(key) => s.label(key),
                                    None => SharedString::from(x.3.clone()),
                                })
                            }))
                            .number_of_lines(1)
                            .text_color(move || if ok() { theme().muted_foreground } else { theme().destructive }),
                        ),
                )
                .child(when(ok, move || {
                    ui::dim(ui::caption(text(move || item.with(|x| s.fmt("{} digits · {} sec", &[&x.5.to_string(), &x.6.to_string()])))))
                }))
            },
        ))))
        .child(view().px(px(ui::MARGIN)).pt(px(8.0)).pb_safe_area_plus(px(16.0)).child(
            ui::primary_button(move || s.count("add_n", valid()), move || valid() > 0, move || s.busy.get()).test_id("add-all").on_press(
                move || {
                    if valid() > 0 && !s.busy.get() {
                        s.import_text(source())
                    }
                },
            ),
        ))
}

/// Links pasted one per line: `otpauth://` links and Google Authenticator's migration links. "Parse" reads them, and
/// the review page shows what they hold.
fn paste_mode(s: Store) -> impl IntoElement {
    let parse = move || {
        let text = s.form.with(|f| f.text.clone());
        if safay_app::core::otpauth::parse_lines(&text, &safay_app::session::placeholders()).iter().any(|p| p.is_valid()) {
            dismiss_keyboard();
            s.scanned.set(text);
        } else {
            s.error.set(Some(safay_app::session::Error::Import));
        }
    };
    view()
        .gap(px(16.0))
        .child(
            ui::form_card()
                .gap(px(8.0))
                .child(
                    nsh_base::input()
                        .test_id("otpauth")
                        .label(move || s.label("Paste otpauth"))
                        .placeholder(move || s.label("Paste one otpauth:// or migration link per line").to_string())
                        .multiline(true)
                        .value(move || s.form.with(|f| f.text.to_string()))
                        .on_change(move |v: &str| {
                            s.error.set(None);
                            s.form.update(|f| f.text = Zeroizing::new(v.to_owned()))
                        })
                        .auto_correct(false)
                        .auto_capitalize(nsh_core::AutoCapitalize::None)
                        .auto_fill(nsh_core::AutoFill::Off)
                        .min_h(px(120.0))
                        .text_size(px(15.0))
                        .text_color(move || nsh_base::theme().foreground),
                )
                .child(ui::field_note(
                    move || s.error.get().map(|_| s.label("The text doesn’t contain an otpauth or migration link.")),
                    || true,
                )),
        )
        .child(
            ui::primary_button(move || s.label("Parse"), move || s.form.with(|f| !f.text.trim().is_empty()), move || s.busy.get())
                .test_id("parse")
                .on_press(parse),
        )
}

/// "Advanced Options": the algorithm, the digits and the interval, for the few services that do not use the defaults.
fn advanced_options(s: Store, open: Signal<bool>) -> impl IntoElement {
    let group = move |name: &'static str, control: AnyElement| {
        view().gap(px(6.0)).child(ui::dim(ui::body(text(move || s.label(name))))).child(control)
    };
    view()
        .rounded(px(24.0))
        .overflow_hidden()
        .bg(move || nsh_base::theme().card)
        .child(
            nsh_base::button()
                .test_id("advanced")
                .flex_row()
                .items_center()
                .px(px(16.0))
                .py(px(12.0))
                .on_press(move || open.update(|v| *v = !*v))
                .child(view().flex_1().child(ui::label(text(move || s.label("Advanced Options")))).child(ui::dim(ui::caption(text(
                    move || {
                        let (algorithm, digits, period) = s.form.with(|f| (f.algorithm, f.digits, f.period));
                        let algorithm = ["SHA1", "SHA256", "SHA512"].get(algorithm as usize).copied().unwrap_or("SHA1");
                        s.fmt("{} · {} digits · {} sec", &[algorithm, &digits.to_string(), &period.to_string()])
                    },
                )))))
                .child(
                    view()
                        .text_color(move || nsh_base::theme().muted_foreground)
                        .rotate(move || if open.get() { std::f32::consts::PI } else { 0.0 })
                        .transition()
                        .duration(ui::SPATIAL_MS)
                        .ease_out()
                        .child(crate::icons::icon(crate::icons::Icon::ChevronDown).size(px(24.0))),
                ),
        )
        .child(when(
            move || open.get(),
            move || {
                view()
                    .px(px(16.0))
                    .pb(px(16.0))
                    .gap(px(16.0))
                    .child(group(
                        "Algorithm",
                        ui::choice(
                            vec![
                                (0u32, Box::new(|| SharedString::from("SHA1")) as Box<dyn Fn() -> SharedString>),
                                (1, Box::new(|| SharedString::from("SHA256"))),
                                (2, Box::new(|| SharedString::from("SHA512"))),
                            ],
                            move || s.form.with(|f| f.algorithm),
                            move |v| s.form.update(|f| f.algorithm = v),
                        )
                        .into_element(),
                    ))
                    .child(group(
                        "Digits",
                        ui::choice(
                            vec![
                                (6u32, Box::new(|| SharedString::from("6")) as Box<dyn Fn() -> SharedString>),
                                (8, Box::new(|| SharedString::from("8"))),
                            ],
                            move || s.form.with(|f| f.digits),
                            move |v| s.form.update(|f| f.digits = v),
                        )
                        .into_element(),
                    ))
                    .child(group(
                        "Refresh Interval",
                        ui::choice(
                            vec![
                                (30u32, Box::new(move || s.fmt("{} seconds", &["30"])) as Box<dyn Fn() -> SharedString>),
                                (60, Box::new(move || s.fmt("{} seconds", &["60"]))),
                            ],
                            move || s.form.with(|f| f.period),
                            move |v| s.form.update(|f| f.period = v),
                        )
                        .into_element(),
                    ))
            },
        ))
}
