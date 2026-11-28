//! Settings, as groups of rows: security, the vault's data, how the app looks, and what it is.
use super::{Dialog, Nav, Page, ui};
use crate::{
    icons::{Icon, icon},
    store::{Auth, Store},
};
use nsh_base::theme;
use nsh_core::prelude::*;
use safay_app::preferences::{Appearance, Language};
use std::rc::Rc;

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

fn nothing() -> impl IntoElement {
    view()
}

pub fn page(s: Store, nav: Nav) -> impl IntoElement {
    let preferences = move || s.preferences.get();
    // Shown where the device has a strong biometric enrolled.
    let biometric_note = move || s.label("After you change fingerprints or faces, turn it on again.");
    let biometric = s.biometrics.get_untracked().then(|| {
        ui::switch_row(
            ui::round_tile(ui::Biometric::get().icon(), false),
            move || s.label(ui::Biometric::get().unlock()),
            Rc::new(biometric_note),
            move || preferences().touch_id,
        )
        .test_id("biometrics")
        .on_press(move |_: &()| s.set_touch_id(!preferences().touch_id))
        .into_element()
    });
    let auto_lock = ui::picker_row(
        "auto-lock",
        ui::round_tile(Icon::Lock, false),
        move || s.label("Auto-Lock"),
        move || s.count("minutes", preferences().auto_lock_minutes as usize),
        move || AUTO_LOCK_MINUTES.iter().map(|m| (*m, s.count("minutes", *m as usize))).collect(),
        move || preferences().auto_lock_minutes,
        move |m| s.update_preferences(move |p| p.auto_lock_minutes = m),
        move || nav.dialog.set(Some(Dialog::AutoLock)),
    );
    let change_password = ui::row(ui::round_tile(Icon::Key, false), move || s.label("Change Master Password"), None, ui::chevron())
        .test_id("change-password")
        .on_press(move || {
            s.clear();
            nav.push(Page::ChangePassword)
        })
        .into_element();
    let icons_note = move || s.label("Each icon comes straight from the account’s website.");
    let site_icons = ui::switch_row(
        ui::round_tile(Icon::Photo, false),
        move || s.label("Show Site Icons"),
        Rc::new(icons_note),
        move || preferences().site_icons,
    )
    .test_id("site-icons")
    .on_press(move |_: &()| {
        let on = !preferences().site_icons;
        s.update_preferences(move |p| p.site_icons = on)
    })
    .into_element();
    let export = ui::row(ui::round_tile(Icon::ExportImport, false), move || s.label("Export Encrypted Backup"), None, ui::chevron())
        .test_id("export")
        .on_press(move || nav.push(Page::Export))
        .into_element();
    let import = ui::row(ui::round_tile(Icon::Download, false), move || s.label("Import Backup File"), None, ui::chevron())
        .test_id("import")
        .on_press(move || s.choose_backup())
        .into_element();
    let deleted = ui::value_row(
        ui::round_tile(Icon::Trash, false),
        move || s.label("Recently Deleted"),
        move || s.count("accounts_n", s.projection.with(|p| p.deleted_count)),
        ui::chevron(),
    )
    .test_id("deleted")
    .on_press(move || nav.push(Page::Deleted))
    .into_element();
    let reset_note = move || s.label("Erases the vault and settings on this device. Your backup isn’t affected.");
    let reset_row = if ui::IOS {
        // iOS: an action in red, its consequence under the group.
        ui::row(ui::round_tile(Icon::Warning, true), move || s.label("Reset App"), None, nothing())
    } else {
        ui::row(ui::round_tile(Icon::Warning, true), move || s.label("Reset App"), Some(Rc::new(reset_note)), nothing())
    }
    .test_id("reset")
    .on_press(move || reset(s))
    .into_element();
    let appearance_names = move |a: Appearance| match a {
        Appearance::System => s.label("System"),
        Appearance::Light => s.label("Light"),
        Appearance::Dark => s.label("Dark"),
    };
    let appearance = if ui::IOS {
        ui::picker_row(
            "appearance",
            ui::round_tile(Icon::Eye, false),
            move || s.label("Appearance"),
            move || appearance_names(preferences().appearance),
            move || [Appearance::System, Appearance::Light, Appearance::Dark].map(|a| (a, appearance_names(a))).to_vec(),
            move || preferences().appearance,
            move |a| s.update_preferences(move |p| p.appearance = a),
            || {},
        )
    } else {
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
            .into_element()
    };
    let language = ui::picker_row(
        "language",
        ui::round_tile(Icon::Keyboard, false),
        move || s.label("Language"),
        move || language_name(s, preferences().language),
        move || [Language::System, Language::En, Language::Hans, Language::Hant].map(|l| (l, language_name(s, l))).to_vec(),
        move || preferences().language,
        move |l| s.update_preferences(move |p| p.language = l),
        move || nav.dialog.set(Some(Dialog::Language)),
    );
    let version = ui::value_row(
        ui::round_tile(Icon::Info, false),
        move || s.label("Version"),
        || SharedString::from(format!("{} ({})", env!("CARGO_PKG_VERSION"), crate::BUILD)),
        nothing(),
    )
    .into_element();
    // iOS: a row whose setting needs a word of explanation is a group of its own, the explanation under it in small
    // text. Elsewhere each row carries its own detail.
    let column = view().w_full().max_w(px(720.0)).px(px(ui::MARGIN));
    let column = if ui::IOS {
        let gap = || view().h(px(20.0));
        let mut column = column.child(ui::section_title(move || s.label("Security")));
        if let Some(biometric) = biometric {
            column = column.child(ui::group(vec![biometric])).child(ui::footer(biometric_note)).child(gap());
        }
        column
            .child(ui::group(vec![auto_lock, change_password]))
            .child(gap())
            .child(ui::group(vec![site_icons]))
            .child(ui::footer(icons_note))
            .child(super::backup::section(s, nav))
            .child(ui::section_title(move || s.label("Data")))
            .child(ui::group(vec![export, import, deleted]))
            .child(gap())
            .child(ui::group(vec![reset_row]))
            .child(ui::footer(reset_note))
            .child(ui::section_title(move || s.label("Appearance")))
            .child(ui::group(vec![appearance, language]))
            .child(ui::section_title(move || s.label("About")))
            .child(ui::group(vec![version]))
    } else {
        let mut security = Vec::new();
        security.extend(biometric);
        security.extend([auto_lock, change_password, site_icons]);
        column
            .child(ui::section_title(move || s.label("Security")))
            .child(ui::group(security))
            .child(super::backup::section(s, nav))
            .child(ui::section_title(move || s.label("Data")))
            .child(ui::group(vec![export, import, deleted, reset_row]))
            .child(ui::section_title(move || s.label("Appearance")))
            .child(ui::group(vec![appearance, language]))
            .child(ui::section_title(move || s.label("About")))
            .child(ui::group(vec![version]))
    };
    // iOS: under the system's header (`mobile::native_stack`); its scroll view is the page's first, which the header's
    // large title follows.
    view()
        .flex_1()
        .pt(ui::page_top)
        .child(ui::top_bar(s, Some("Settings"), move || nav.pop()))
        .child(scroll().flex_1().child(view().items_center().pb_safe_area_plus(px(32.0)).child(column)))
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
        .pt(ui::page_top)
        .child(ui::top_bar(s, Some("Recently Deleted"), move || {
            nav.pop();
        }))
        // Nothing in it: centered on the page, as the list's own empty state.
        .child(when(
            move || count() == 0,
            move || {
                view()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .px(px(32.0))
                    .pb_safe_area_plus(px(48.0))
                    .gap(px(8.0))
                    .child(view().text_color(move || theme().muted_foreground).child(icon(Icon::Trash).size(px(48.0))))
                    .child(ui::title(text(move || s.label("No recently deleted accounts"))).text_center().mt(px(8.0)))
                    .child(ui::dim(ui::body(text(move || s.label("Accounts you delete wait here for 30 days.")))).text_center())
            },
        ))
        .child(when(
            move || count() > 0,
            move || {
                scroll().flex_1().child(ui::readable(
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
                                    view()
                                        .flex_row()
                                        .items_center()
                                        .bg(move || theme().card)
                                        .min_h(move || px(ui::ROW) * font_scale())
                                        .pl(px(20.0))
                                        .pr(px(4.0)),
                                    move || index.get(),
                                    count,
                                )
                                .child(
                                    view()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .child(
                                            ui::body_large(text(move || d.with(|d| SharedString::from(d.issuer.clone()))))
                                                .number_of_lines(1),
                                        )
                                        .child(
                                            ui::dim(ui::body(text(move || d.with(|d| SharedString::from(d.username.clone())))))
                                                .number_of_lines(1),
                                        ),
                                )
                                .child(ui::text_button(move || s.label("Recover"), false).on_press(move || s.recover_account(id)))
                                .child(
                                    ui::icon_button(move || s.label("Delete"), || Icon::Trash).on_press(move || s.remove_deleted(Some(id))),
                                )
                            },
                        )))
                        .child(ui::dim(ui::caption(text(move || s.label("Accounts are deleted for good after 30 days.")))).p(px(16.0)))
                        // Empties it, after asking.
                        .child(when(
                            move || count() > 1,
                            move || {
                                view().items_center().child(
                                    ui::text_button(move || s.label("Delete All"), true)
                                        .test_id("delete-all")
                                        .on_press(move || s.remove_deleted(None)),
                                )
                            },
                        )),
                ))
            },
        ))
}

// ---- Pages of a task -----------------------------------------------------------------------------------------------

/// A page that does one thing with a few fields: its bar, then a column of a readable width that scrolls, the action
/// at its end. iOS: under the system's header (small title), as the pages a task opens.
fn task_page(s: Store, nav: Nav, title: &'static str, column: impl IntoElement) -> impl IntoElement {
    view().flex_1().pt(ui::page_top).child(ui::top_bar(s, Some(title), move || nav.pop())).child(
        scroll().flex_1().keyboard_dismiss_mode(KeyboardDismissMode::None).keyboard_taps(KeyboardTaps::Handled).child(
            view().items_center().pb_safe_area_plus(px(24.0)).child(
                view().w_full().max_w(px(480.0)).px(px(ui::MARGIN)).pt(px(if ui::IOS { 8.0 } else { 12.0 })).gap(px(16.0)).child(column),
            ),
        ),
    )
}
/// What a page says about itself, under its bar.
fn intro(text_: impl Fn() -> SharedString + 'static) -> impl IntoElement {
    ui::dim(ui::body(text(text_))).px(px(if ui::IOS { 16.0 } else { 4.0 }))
}

/// "Change Master Password": the current one, then the new one twice. Its fields are a form of each platform's
/// (`ui::form`), not crowded into a dialog.
pub fn change_password(s: Store, nav: Nav) -> impl IntoElement {
    use crate::pages::unlock::new_password_ok;
    use ui::Entry;
    let ready = move || s.form.with(|f| !f.password.is_empty()) && new_password_ok(s) && !s.busy.get();
    let new_field = element_ref();
    let submit = move || {
        if ready() {
            dismiss_keyboard();
            s.change_password()
        }
    };
    s.clear();
    // What was typed goes with the page.
    cleanup(move || s.clear());
    super::shows_errors(s);
    super::on_done(s, crate::store::Done::PasswordChanged, move || {
        nav.pop();
        s.show_toast(crate::store::Tone::Success, s.label("Master password changed"));
    });
    task_page(
        s,
        nav,
        "Change Master Password",
        view()
            .gap(px(16.0))
            .child(intro(move || {
                s.label("The vault on this device is re-encrypted with the new password. If sync is on, your backup is updated too.")
            }))
            .child(ui::form(move || {
                view()
                    .child(ui::field(
                        s,
                        "current-password",
                        Entry::Password,
                        move || s.label("Current Master Password"),
                        move || s.form.with(|f| f.password.to_string()),
                        move |v| {
                            s.error.set(None);
                            s.form.update(|f| f.password = safay_app::Zeroizing::new(v.to_owned()))
                        },
                        move || new_field.focus(),
                        |_| {},
                        move || s.error.get().is_some(),
                        ReturnKeyType::Next,
                        None,
                    ))
                    // A wrong password, said under its field.
                    .child(ui::field_note(move || s.error.get().map(|e| s.label(crate::i18n::error_key(e))), || true))
            }))
            .child(super::onboarding::new_password_form(s, "New Password", "Enter again", submit, new_field))
            .child(
                ui::primary_button(move || s.label("Change Password"), move || ready() || s.busy.get(), move || s.busy.get())
                    .test_id("change")
                    .on_press(submit),
            ),
    )
}

/// "Export Encrypted Backup": the password the file is encrypted with, then the master password, then where to save.
pub fn export(s: Store, nav: Nav) -> impl IntoElement {
    use crate::{pages::unlock::new_password_ok, store::Sheet};
    // Every account: the export takes the ones the store selects as it lists them (which needs its export sheet, let
    // go with the page). Nothing is exported before they are listed.
    s.selected.set(Default::default());
    s.open_sheet(Sheet::ExportImport);
    // A password from before is not offered again. (Not as the page goes: the export, after it, still needs it.)
    s.form.update(|f| {
        f.new_password = safay_app::Zeroizing::new(String::new());
        f.confirmation = safay_app::Zeroizing::new(String::new());
    });
    cleanup(move || {
        if s.sheet.get_untracked() == Some(Sheet::ExportImport) {
            s.sheet.set(None);
        }
    });
    let ready = move || new_password_ok(s) && !s.busy.get() && !s.selected.with(|x| x.is_empty());
    let submit = move || {
        if ready() {
            dismiss_keyboard();
            let p = s.form.with(|f| f.new_password.clone());
            s.confirm_fair(p, move |_| {
                nav.pop();
                s.reveal(Auth::Export)
            })
        }
    };
    task_page(
        s,
        nav,
        "Export Encrypted Backup",
        view()
            .gap(px(16.0))
            .child(intro(move || {
                s.label("The backup is encrypted with this separate password. You’ll need it to import on another device.")
            }))
            .child(super::onboarding::new_password_form(s, "Export Password", "Enter again", submit, element_ref()))
            .child(ui::primary_button(move || s.label("Export"), ready, move || s.busy.get()).test_id("export-go").on_press(submit)),
    )
}
