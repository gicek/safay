//! An account's details, in a sheet over the list: its code, large; its names and website; its categories; how it was
//! added; and its secret and QR code, behind the master password. It is deleted from its menu in the list.
use super::ui::{self, Entry};
use crate::{
    components::{CODE_FONT, badge_with_icon, tok},
    icons::{Icon, icon},
    store::{Auth, CategoryEdit, Sheet, Store},
};
use nsh_base::{button, theme};
use nsh_core::prelude::*;
use safay_app::Uuid;
use safay_app::Zeroizing;

/// The sheet's side margin.
const SIDE: f32 = 24.0;

/// The account whose details show, the QR code over them included.
fn shown(s: Store) -> Option<Uuid> {
    match s.sheet.get() {
        Some(Sheet::Detail(id) | Sheet::Qr(id)) => Some(id),
        _ => None,
    }
}

pub fn sheet(s: Store) -> impl IntoElement {
    ui::bottom_sheet(
        move || shown(s).is_some() && s.detail.with(Option::is_some),
        move || {
            s.revealed.set(Zeroizing::new(String::new()));
            s.sheet.set(None);
            s.detail.set(None);
        },
        dynamic(move || shown(s).map(|id| content(s, id).into_element())),
    )
}

fn content(s: Store, id: Uuid) -> impl IntoElement {
    let row = computed(move || s.projection.with(|p| p.rows.iter().find(|r| r.id == id).cloned()));
    let detail = move || s.detail.get();
    let editing = signal(false);
    let issuer = move || detail().map(|d| d.issuer).unwrap_or_default();
    let urgent = move || row.with(|r| r.as_ref().is_some_and(|r| r.remaining <= 5));
    let copied = move || s.copied.get() == Some(id);
    let revealed = move || !s.revealed.with(|r| r.is_empty());
    let save = move || {
        dismiss_keyboard();
        s.save_detail(id);
        editing.set(false);
    };
    scroll().max_h(move || ui::sheet_room(0.0)).keyboard_dismiss_mode(KeyboardDismissMode::None).keyboard_taps(KeyboardTaps::Handled).child(
        view()
            .px(px(SIDE))
            .pb(move || ui::sheet_bottom(16.0))
            .gap(px(16.0))
            // Names, and the control that edits them.
            .child(
                view()
                    .flex_row()
                    .items_center()
                    .gap(px(14.0))
                    .child(badge_with_icon(issuer, || false, move || row.with(|r| r.as_ref().and_then(|r| r.icon.clone())), 48.0))
                    .child(
                        view()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(ui::title_large(text(issuer)).number_of_lines(1))
                            .child(ui::dim(ui::body(text(move || detail().map(|d| d.username).unwrap_or_default()))).number_of_lines(1)),
                    )
                    .child(
                        when(move || editing.get(), move || ui::text_button(move || s.label("Done"), false).test_id("done").on_press(save))
                            .otherwise(move || {
                                ui::icon_button(move || s.label("Edit"), || Icon::Pencil)
                                    .test_id("edit")
                                    .on_press(move || editing.set(true))
                            }),
                    ),
            )
            // The code: a press copies it.
            .child(
                button()
                    .test_id("code")
                    .items_center()
                    .py(px(20.0))
                    .rounded(px(32.0))
                    .bg(move || if copied() { tok("primary-container") } else { tok("surface-high") })
                    .transition()
                    .duration(ui::EFFECTS_MS)
                    .ease_out()
                    .on_press(move || {
                        haptic(Haptic::Notification(HapticNotification::Success));
                        s.copy(id)
                    })
                    .child(
                        text(move || row.with(|r| r.as_ref().map(|r| r.code.clone()).unwrap_or_default()))
                            .font_family(CODE_FONT.family)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(42.0))
                            .number_of_lines(1)
                            .text_color(move || if urgent() { tok("urgent") } else { theme().foreground }),
                    )
                    .child(
                        view()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .mt(px(10.0))
                            .child(ui::dim(ui::caption(text(move || {
                                s.fmt("Refreshes in {}s", &[&row.with(|r| r.as_ref().map(|r| r.remaining).unwrap_or_default()).to_string()])
                            }))))
                            .child(ui::caption(text("·")).text_color(move || tok("outline")))
                            .child(view().size(px(16.0)).text_color(move || theme().primary).child(dynamic(move || {
                                icon(if copied() { Icon::Check } else { Icon::Copy }).size(px(16.0)).into_element()
                            })))
                            .child(
                                ui::caption(text(move || s.label(if copied() { "Copied" } else { "Copy" })))
                                    .text_color(move || theme().primary),
                            ),
                    ),
            )
            .child(when(move || editing.get(), move || edit_fields(s, save)))
            .child(categories(s, id, row))
            // How it was made.
            .child(
                view()
                    .child(info(s, "Algorithm", move || {
                        detail()
                            .map(|d| {
                                // Steam Guard and Battle.net have fixed parameters: their name says it all.
                                match d.otp_type {
                                    safay_app::core::OtpType::Steam => return s.label("Steam"),
                                    safay_app::core::OtpType::BattleNet => return s.label("Battle.net"),
                                    safay_app::core::OtpType::Totp => {}
                                }
                                let algorithm = match d.algorithm {
                                    safay_app::core::TotpAlgorithm::Sha256 => "SHA256",
                                    safay_app::core::TotpAlgorithm::Sha512 => "SHA512",
                                    _ => "SHA1",
                                };
                                s.fmt("{} · {} digits · {} sec", &[algorithm, &d.digits.to_string(), &d.period.to_string()])
                            })
                            .unwrap_or_default()
                    }))
                    .child(rule())
                    .child(info(s, "Added via", move || {
                        s.label(if detail().is_some_and(|d| d.imported) { "Imported" } else { "Added manually" })
                    }))
                    .child(rule())
                    .child(info(s, "Date Added", move || {
                        detail().map(|d| SharedString::from(crate::platform::local_time::date_time(d.created_at))).unwrap_or_default()
                    })),
            )
            // The secret and the QR code.
            .child(ui::label(text(move || s.label("Secret"))))
            .child(when(revealed, move || {
                view()
                    .flex_row()
                    .items_center()
                    .pl(px(16.0))
                    .rounded(px(16.0))
                    .bg(move || tok("surface-high"))
                    .child(
                        text(move || SharedString::from(s.revealed.get().to_string()))
                            .font_family(CODE_FONT.family)
                            .text_size(px(15.0))
                            .line_height(px(22.0))
                            .flex_1()
                            .py(px(12.0)),
                    )
                    .child(ui::icon_button(move || s.label("Copy"), || Icon::Copy).on_press(move || {
                        set_clipboard(s.revealed.get().to_string());
                        s.show_toast(crate::store::Tone::Success, s.label("Copied"));
                    }))
            }))
            .child(
                view()
                    .flex_row()
                    .gap(px(ui::GROUP_GAP))
                    .child(
                        ui::tonal_button(
                            move || if revealed() { Icon::EyeSlash } else { Icon::Key },
                            move || s.label(if revealed() { "Hide" } else { "Show Secret" }),
                            false,
                        )
                        .test_id("secret")
                        .flex_1()
                        .rounded_l(px(28.0))
                        .rounded_r(px(8.0))
                        .on_press(move || {
                            if revealed() {
                                s.revealed.set(Zeroizing::new(String::new()));
                            } else {
                                s.reveal(Auth::Secret(id))
                            }
                        }),
                    )
                    .child(
                        ui::tonal_button(|| Icon::Qr, move || s.label("Export QR Code"), false)
                            .test_id("qr")
                            .flex_1()
                            .rounded_l(px(8.0))
                            .rounded_r(px(28.0))
                            .on_press(move || s.reveal(Auth::Qr(id))),
                    ),
            )
            // No delete button here, where a press meant for the secret could land on it: an account is deleted from
            // its menu in the list.
            .child(ui::dim(ui::caption(text(move || s.label("Showing the secret or the QR code asks for your master password."))))),
    )
}

fn rule() -> impl IntoElement {
    view().h(px(1.0)).bg(move || theme().border.opacity(0.7))
}
fn info(s: Store, name: &'static str, value: impl Fn() -> SharedString + 'static) -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .gap(px(16.0))
        .py(px(12.0))
        .child(ui::dim(ui::body(text(move || s.label(name)))))
        .child(ui::body(text(value)).flex_1().text_right())
}

fn edit_fields(s: Store, save: impl Fn() + Copy + 'static) -> impl IntoElement {
    let account = element_ref();
    let website = element_ref();
    view()
        .gap(px(12.0))
        .child(ui::field(
            s,
            "issuer",
            Entry::Text,
            move || s.label("Service Name"),
            move || s.form.with(|f| f.issuer.clone()),
            move |v| s.form.update(|f| f.issuer = v.to_owned()),
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
            move |v| s.form.update(|f| f.username = v.to_owned()),
            move || website.focus(),
            |_| {},
            || false,
            ReturnKeyType::Next,
            Some(account),
        ))
        .child(ui::field(
            s,
            "website",
            Entry::Text,
            move || s.label("Website"),
            move || s.form.with(|f| f.website.clone()),
            move |v| {
                s.marks.update(|m| m.website_error = false);
                s.form.update(|f| f.website = v.to_owned())
            },
            save,
            |_| {},
            move || s.marks.with(|m| m.website_error),
            ReturnKeyType::Done,
            Some(website),
        ))
        .child(ui::field_note(move || s.marks.with(|m| m.website_error).then(|| s.label("Enter a website like example.com")), || true))
}

/// The account's categories as chips in a row: a press adds it to one or takes it out, at once.
fn categories(s: Store, id: Uuid, row: Memo<Option<safay_app::session::Row>>) -> impl IntoElement {
    view().gap(px(8.0)).child(ui::label(text(move || s.label("Categories")))).child(
        // One row that scrolls sideways, without an indicator, out to the sheet's edges.
        scroll().horizontal().scrollbar(false).mx(px(-SIDE)).child(
            view()
                .flex_row()
                .gap(px(8.0))
                .px(px(SIDE))
                .child(each(
                    move || s.projection.with(|p| p.categories.clone()),
                    |c| c.id,
                    move |c, _| {
                        let category = c.with_untracked(|c| c.id);
                        let member = move || row.with(|r| r.as_ref().is_some_and(|r| r.categories.contains(&category)));
                        ui::chip(move || c.with(|c| SharedString::from(c.name.clone())), member, None)
                            .on_press(move || s.membership(category, id, !member()))
                    },
                ))
                .child(
                    ui::chip(move || s.label("New Category"), || false, Some(Icon::FolderPlus))
                        .test_id("new-category")
                        .on_press(move || s.editing.set(Some(CategoryEdit { account: Some(id), ..Default::default() }))),
                ),
        ),
    )
}
