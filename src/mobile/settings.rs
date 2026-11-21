//! Settings, as groups of rows: security, the vault's data, how the app looks, and what it is.
use super::{Dialog, Nav, Page, ui};
use crate::{
    icons::{Icon, icon},
    store::{Auth, Sheet, Store},
};
use nsh_base::theme;
use nsh_core::prelude::*;
use safay_app::preferences::{Appearance, Language};

pub const AUTO_LOCK_MINUTES: [u64; 4] = [1, 5, 15, 30];

/// A language's own name: never translated.
pub fn language_name(s: Store, language: Language) -> SharedString {
    match language {
        Language::System => s.label("System"),
        Language::En => "English".into(),
        Language::Hans => "简体中文".into(), // i18n-guard: allow
        Language::Hant => "繁體中文".into(), // i18n-guard: allow
    }
}

fn chevron() -> impl IntoElement {
    view().text_color(move || theme().muted_foreground).child(icon(Icon::ChevronRight).size(px(24.0)))
}
fn nothing() -> impl IntoElement {
    view()
}

pub fn page(s: Store, nav: Nav) -> impl IntoElement {
    let preferences = move || s.preferences.get();
    let mut security_rows = Vec::new();
    // Shown where the device has a strong biometric enrolled.
    if s.biometrics.get_untracked() {
        security_rows.push(
            ui::row(
                ui::round_tile(Icon::TouchId, false),
                move || s.label("Unlock with biometrics"),
                Some(Box::new(move || {
                    s.label("The vault key is protected by this device’s hardware keystore. After you change enrolled fingerprints or faces, turn it on again with your master password.")
                })),
                ui::switch(move || preferences().touch_id),
            )
            .test_id("biometrics")
            .on_press(move || s.set_touch_id(!preferences().touch_id))
            .into_element(),
        );
    }
    security_rows.extend(vec![
        ui::row(
            ui::round_tile(Icon::Lock, false),
            move || s.label("Auto-Lock"),
            Some(Box::new(move || s.count("minutes", preferences().auto_lock_minutes as usize))),
            chevron(),
        )
        .test_id("auto-lock")
        .on_press(move || nav.dialog.set(Some(Dialog::AutoLock)))
        .into_element(),
        ui::row(ui::round_tile(Icon::Key, false), move || s.label("Change Master Password"), None, chevron())
            .test_id("change-password")
            .on_press(move || {
                s.clear();
                nav.dialog.set(Some(Dialog::ChangePassword))
            })
            .into_element(),
        ui::row(
            ui::round_tile(Icon::Photo, false),
            move || s.label("Show Site Icons"),
            Some(Box::new(move || {
                s.label("When you add an account or change its website, Safay gets the icon directly from that website. No other service is involved.")
            })),
            ui::switch(move || preferences().site_icons),
        )
        .test_id("site-icons")
        .on_press(move || {
            let on = !preferences().site_icons;
            s.update_preferences(move |p| p.site_icons = on)
        })
        .into_element(),
    ]);
    let security = ui::group(security_rows);
    let data = ui::group(vec![
        ui::row(ui::round_tile(Icon::ExportImport, false), move || s.label("Export Encrypted Backup"), None, chevron())
            .test_id("export")
            .on_press(move || {
                // Every account: the export takes the ones selected.
                s.open_sheet(Sheet::ExportImport);
                nav.dialog.set(Some(Dialog::Export))
            })
            .into_element(),
        ui::row(ui::round_tile(Icon::Download, false), move || s.label("Import Backup File"), None, chevron())
            .test_id("import")
            .on_press(move || s.choose_backup())
            .into_element(),
        ui::row(
            ui::round_tile(Icon::Trash, false),
            move || s.label("Recently Deleted"),
            Some(Box::new(move || SharedString::from(s.projection.with(|p| p.deleted_count).to_string()))),
            chevron(),
        )
        .test_id("deleted")
        .on_press(move || nav.push(Page::Deleted))
        .into_element(),
        ui::row(
            ui::round_tile(Icon::Warning, true),
            move || s.label("Reset App"),
            Some(Box::new(move || s.label("Erases the vault and settings on this device. Your backup isn’t affected."))),
            nothing(),
        )
        .test_id("reset")
        .on_press(move || reset(s))
        .into_element(),
    ]);
    let appearance = ui::group(vec![
        view()
            .flex_row()
            .items_center()
            .gap(px(16.0))
            .px(px(16.0))
            .py(px(14.0))
            .bg(move || theme().card)
            .child(ui::round_tile(Icon::Eye, false))
            .child(view().flex_1().min_w(px(0.0)).child(ui::choice(
                vec![
                    (Appearance::System, Box::new(move || s.label("System")) as Box<dyn Fn() -> SharedString>),
                    (Appearance::Light, Box::new(move || s.label("Light"))),
                    (Appearance::Dark, Box::new(move || s.label("Dark"))),
                ],
                move || preferences().appearance,
                move |a| s.update_preferences(move |p| p.appearance = a),
            )))
            .into_element(),
        ui::row(
            ui::round_tile(Icon::Keyboard, false),
            move || s.label("Language"),
            Some(Box::new(move || language_name(s, preferences().language))),
            chevron(),
        )
        .test_id("language")
        .on_press(move || nav.dialog.set(Some(Dialog::Language)))
        .into_element(),
    ]);
    let about = ui::group(vec![
        ui::row(
            ui::round_tile(Icon::Info, false),
            move || s.label("Version"),
            Some(Box::new(|| SharedString::from(format!("{} ({})", env!("CARGO_PKG_VERSION"), crate::BUILD)))),
            nothing(),
        )
        .into_element(),
    ]);
    view()
        .flex_1()
        .pt_safe_area()
        .child(ui::top_bar(s, Some("Settings"), move || {
            nav.back();
        }))
        .child(
            scroll().flex_1().child(
                view().items_center().pb_safe_area_plus(px(32.0)).child(
                    view()
                        .w_full()
                        .max_w(px(720.0))
                        .px(px(ui::MARGIN))
                        .child(ui::section_title(move || s.label("Security")))
                        .child(security)
                        .child(super::backup::section(s, nav))
                        .child(ui::section_title(move || s.label("Data")))
                        .child(data)
                        .child(ui::section_title(move || s.label("Appearance")))
                        .child(appearance)
                        .child(ui::section_title(move || s.label("About")))
                        .child(about),
                ),
            ),
        )
}

/// "Reset App": a confirmation, then the master password.
fn reset(s: Store) {
    let chosen = crate::alert::alert(s.label("Reset Safay?"))
        .description(s.label("Erases the vault and settings on this device. Your backup isn’t affected."))
        .cancel("cancel", s.label("Cancel"))
        .destructive("erase", s.label("Erase Local Data"))
        .show();
    let epoch = s.controller.get().epoch();
    s.task(async move {
        if chosen.await == Some("erase") && s.controller.get().epoch() == epoch {
            s.reveal(Auth::Reset)
        }
    });
}

/// "Recently Deleted": accounts wait here thirty days; each can come back or go for good.
pub fn deleted(s: Store, nav: Nav) -> impl IntoElement {
    let count = move || s.projection.with(|p| p.deleted.len());
    view()
        .flex_1()
        .pt_safe_area()
        .child(ui::top_bar(s, Some("Recently Deleted"), move || {
            nav.back();
        }))
        .child(
            scroll().flex_1().child(
                view()
                    .px(px(ui::MARGIN))
                    .pt(px(8.0))
                    .pb_safe_area_plus(px(32.0))
                    .child(view().gap(px(ui::GROUP_GAP)).child(each(
                        move || s.projection.with(|p| p.deleted.clone()),
                        |d| d.id,
                        move |d, index| {
                            let id = d.with_untracked(|d| d.id);
                            ui::group_corners(
                                view().flex_row().items_center().bg(move || theme().card).min_h(px(72.0)).pl(px(20.0)).pr(px(4.0)),
                                move || index.get(),
                                count,
                            )
                            .child(
                                view()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .child(
                                        ui::body_large(text(move || d.with(|d| SharedString::from(d.issuer.clone())))).number_of_lines(1),
                                    )
                                    .child(
                                        ui::dim(ui::body(text(move || d.with(|d| SharedString::from(d.username.clone())))))
                                            .number_of_lines(1),
                                    ),
                            )
                            .child(ui::text_button(move || s.label("Recover"), false).on_press(move || s.recover_account(id)))
                            .child(ui::icon_button(move || s.label("Delete"), || Icon::Trash).on_press(move || s.remove_deleted(Some(id))))
                        },
                    )))
                    .child(
                        ui::dim(ui::caption(text(move || {
                            s.label(if count() == 0 {
                                "No recently deleted accounts"
                            } else {
                                "Accounts are deleted for good after 30 days."
                            })
                        })))
                        .p(px(16.0)),
                    ),
            ),
        )
}
