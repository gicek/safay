//! The dialogs every page may raise: the master password asked again, an account's QR code, a category's name.
use super::ui::{self, Entry};
use crate::store::{Done, Sheet, Store};
use nsh_core::prelude::*;
use safay_app::Zeroizing;

pub fn all(s: Store) -> impl IntoElement {
    view()
        .child(when(move || s.editing.with(Option::is_some), move || category_name(s)))
        .child(when(move || matches!(s.sheet.get(), Some(Sheet::Qr(_))) && !s.qr.with(|q| q.is_empty()), move || qr(s)))
        .child(when(move || s.auth.get().is_some() && !s.auth_prompting.get(), move || auth(s)))
}

/// The master password, before a secret shows or the vault is exported or erased.
fn auth(s: Store) -> impl IntoElement {
    let submit = move || {
        if !s.auth_password.with(|p| p.is_empty()) && !s.busy.get() {
            s.authorize()
        }
    };
    ui::dialog(
        move || s.label("Verify your identity"),
        view()
            .gap(px(8.0))
            .child(ui::dim(ui::body(text(move || s.label("Enter your master password to continue.")))))
            .child(ui::field(
                s,
                "auth-password",
                Entry::Password,
                move || s.label("Master Password"),
                move || s.auth_password.with(|p| p.to_string()),
                move |v| {
                    s.auth_error.set(None);
                    s.auth_password.set(Zeroizing::new(v.to_owned()))
                },
                submit,
                |_| {},
                move || s.auth_error.get().is_some(),
                ReturnKeyType::Done,
                None,
            ))
            .child(ui::field_note(move || s.auth_error.get().map(|e| s.label(crate::i18n::error_key(e))), || true)),
        view()
            .flex_row()
            .gap(px(4.0))
            .child(ui::text_button(move || s.label("Cancel"), false).on_press(move || s.cancel_auth()))
            .child(ui::text_button(move || s.label("Continue"), false).test_id("authorize").on_press(submit)),
        move || {
            if !s.busy.get() {
                s.cancel_auth()
            }
        },
    )
}

/// An account as a QR code, for another device to scan.
fn qr(s: Store) -> impl IntoElement {
    ui::dialog(
        move || s.label("Export QR Code"),
        view()
            .items_center()
            .gap(px(12.0))
            .child(view().p(px(8.0)).rounded(px(12.0)).bg(white()).child(image(move || s.qr.with(|v| v.to_string())).size(px(240.0))))
            .child(ui::title(text(move || s.detail.with(|d| d.as_ref().map(|d| SharedString::from(d.issuer.clone())).unwrap_or_default()))))
            .child(
                ui::dim(ui::caption(text(move || {
                    s.label("Anyone who scans this code can generate your codes. Scan it only on devices you trust.")
                })))
                .text_center(),
            ),
        ui::text_button(move || s.label("Done"), false).test_id("qr-done").on_press(move || s.close_sheet()),
        move || s.close_sheet(),
    )
}

/// A new category's name, or another name for one there is.
fn category_name(s: Store) -> impl IntoElement {
    let renaming = move || s.editing.with(|e| e.as_ref().is_some_and(|e| e.id.is_some()));
    let blank = move || s.editing.with(|e| e.as_ref().is_none_or(|e| e.name.trim().is_empty()));
    let submit = move || {
        if !blank() {
            s.commit_category()
        }
    };
    let cancel = move || {
        dismiss_keyboard();
        s.editing.set(None)
    };
    ui::dialog(
        move || s.label(if renaming() { "Rename" } else { "New Category" }),
        view()
            .gap(px(8.0))
            .child(ui::field(
                s,
                "category-name",
                Entry::Text,
                move || s.label("Category name"),
                move || s.editing.with(|e| e.as_ref().map(|e| e.name.clone()).unwrap_or_default()),
                move |v| {
                    s.editing.update(|e| {
                        if let Some(e) = e {
                            e.name = v.to_owned();
                            e.error = None;
                        }
                    })
                },
                submit,
                |_| {},
                move || s.editing.with(|e| e.as_ref().is_some_and(|e| e.error.is_some())),
                ReturnKeyType::Done,
                None,
            ))
            .child(ui::field_note(
                move || s.editing.with(|e| e.as_ref().and_then(|e| e.error)).map(|e| s.label(crate::i18n::error_key(e))),
                || true,
            )),
        view()
            .flex_row()
            .gap(px(4.0))
            .child(ui::text_button(move || s.label("Cancel"), false).on_press(cancel))
            .child(ui::text_button(move || s.label(if renaming() { "Done" } else { "Create" }), false).test_id("commit").on_press(submit)),
        cancel,
    )
}

// ---- Settings ------------------------------------------------------------------------------------------------------

/// The dialogs settings and the add menu raise.
pub fn of(s: Store, nav: super::Nav) -> impl IntoElement {
    use super::Dialog;
    let is = move |d: Dialog| nav.dialog.get() == Some(d);
    view()
        .child(when(move || is(Dialog::AutoLock), move || auto_lock(s, nav)))
        .child(when(move || is(Dialog::Language), move || language(s, nav)))
        .child(when(move || is(Dialog::ChangePassword), move || change_password(s, nav)))
        .child(when(move || is(Dialog::Export), move || export(s, nav)))
        .child(when(move || is(Dialog::BackupPassword), move || backup_password(s, nav)))
}

/// One of a few values, picked and applied at once.
fn options<T: Copy + PartialEq + 'static>(
    s: Store,
    nav: super::Nav,
    title: &'static str,
    message: Option<&'static str>,
    values: Vec<(T, SharedString)>,
    current: impl Fn() -> T + Copy + 'static,
    pick: impl Fn(T) + Copy + 'static,
) -> impl IntoElement {
    use crate::components::tok;
    let close = move || nav.dialog.set(None);
    ui::dialog(
        move || s.label(title),
        view()
            .mx(px(-24.0))
            .child(when(
                move || message.is_some(),
                move || ui::dim(ui::body(text(move || s.label(message.unwrap_or_default())))).px(px(24.0)).pb(px(8.0)),
            ))
            .children(values.into_iter().map(move |(value, name)| {
                let on = move || current() == value;
                let said = name.clone();
                ui::toggle(AccessibilityRole::Radio, move || said.clone(), on)
                    .min_h(move || px(ui::TOUCH) * font_scale())
                    .px(px(24.0))
                    .flex_row()
                    .items_center()
                    .gap(px(24.0))
                    .active(|st| st.bg(move || tok("pressed")))
                    .on_press(move |_: &()| {
                        pick(value);
                        close()
                    })
                    .child(ui::radio_mark(on))
                    .child(ui::body_large(text(name)))
            })),
        ui::text_button(move || s.label("Cancel"), false).on_press(close),
        close,
    )
}
fn auto_lock(s: Store, nav: super::Nav) -> impl IntoElement {
    options(
        s,
        nav,
        "Auto-Lock",
        Some("Locks after the selected time without activity or in the background, and immediately when the device locks"),
        super::settings::AUTO_LOCK_MINUTES.iter().map(|m| (*m, s.count("minutes", *m as usize))).collect(),
        move || s.preferences.get().auto_lock_minutes,
        move |m| s.update_preferences(move |p| p.auto_lock_minutes = m),
    )
}
fn language(s: Store, nav: super::Nav) -> impl IntoElement {
    use safay_app::preferences::Language;
    options(
        s,
        nav,
        "Language",
        None,
        [Language::System, Language::En, Language::Hans, Language::Hant]
            .into_iter()
            .map(|l| (l, super::settings::language_name(s, l)))
            .collect(),
        move || s.preferences.get().language,
        move |l| s.update_preferences(move |p| p.language = l),
    )
}

fn change_password(s: Store, nav: super::Nav) -> impl IntoElement {
    use crate::pages::unlock::new_password_ok;
    let ready = move || s.form.with(|f| !f.password.is_empty()) && new_password_ok(s) && !s.busy.get();
    let submit = move || {
        if ready() {
            dismiss_keyboard();
            s.change_password()
        }
    };
    let close = move || {
        if !s.busy.get() {
            s.clear();
            nav.dialog.set(None)
        }
    };
    super::shows_errors(s);
    super::on_done(s, Done::PasswordChanged, move || {
        nav.dialog.set(None);
        s.show_toast(crate::store::Tone::Success, s.label("Master password changed"));
    });
    ui::dialog(
        move || s.label("Change Master Password"),
        view()
            .gap(px(8.0))
            .child(ui::field(
                s,
                "current-password",
                Entry::Password,
                move || s.label("Current Master Password"),
                move || s.form.with(|f| f.password.to_string()),
                move |v| {
                    s.error.set(None);
                    s.form.update(|f| f.password = Zeroizing::new(v.to_owned()))
                },
                || {},
                |_| {},
                move || s.error.get().is_some(),
                ReturnKeyType::Next,
                None,
            ))
            .child(super::onboarding::new_password_fields(s, "New Password", "Enter again", submit))
            .child(ui::dim(ui::caption(text(move || {
                s.label("The vault on this device is re-encrypted with the new password. If sync is on, your backup is updated too.")
            }))))
            .child(ui::field_note(move || s.error.get().map(|e| s.label(crate::i18n::error_key(e))), || true)),
        view()
            .flex_row()
            .gap(px(4.0))
            .child(ui::text_button(move || s.label("Cancel"), false).disabled(move || s.busy.get()).on_press(close))
            .child(
                ui::text_button(move || s.label("Change Password"), false).test_id("change").disabled(move || !ready()).on_press(submit),
            ),
        close,
    )
}

/// "Export Encrypted Backup": the password the file is encrypted with, then the master password, then where to save.
fn export(s: Store, nav: super::Nav) -> impl IntoElement {
    use crate::{pages::unlock::new_password_ok, store::Auth};
    // Every account: the export takes the ones the store selects as it lists them (which needs its export sheet, let
    // go with the dialog). Nothing is exported before they are listed.
    s.selected.set(Default::default());
    s.open_sheet(Sheet::ExportImport);
    cleanup(move || {
        if s.sheet.get_untracked() == Some(Sheet::ExportImport) {
            s.sheet.set(None);
        }
    });
    let ready = move || new_password_ok(s) && !s.busy.get() && !s.selected.with(|x| x.is_empty());
    let close = move || {
        s.form.update(|f| {
            f.new_password = Zeroizing::new(String::new());
            f.confirmation = Zeroizing::new(String::new());
        });
        nav.dialog.set(None)
    };
    let submit = move || {
        if ready() {
            dismiss_keyboard();
            let p = s.form.with(|f| f.new_password.clone());
            s.confirm_fair(p, move |_| {
                nav.dialog.set(None);
                s.reveal(Auth::Export)
            })
        }
    };
    ui::dialog(
        move || s.label("Export Encrypted Backup"),
        view()
            .gap(px(8.0))
            .child(ui::dim(ui::body(text(move || {
                s.label("The backup is encrypted with this separate password. You’ll need it to import on another device.")
            }))))
            .child(super::onboarding::new_password_fields(s, "Export Password", "Enter again", submit)),
        view()
            .flex_row()
            .gap(px(4.0))
            .child(ui::text_button(move || s.label("Cancel"), false).on_press(close))
            .child(ui::text_button(move || s.label("Export"), false).test_id("export-go").disabled(move || !ready()).on_press(submit)),
        close,
    )
}

/// The password of a backup file chosen for import.
fn backup_password(s: Store, nav: super::Nav) -> impl IntoElement {
    let close = move || {
        s.remove_file();
        nav.dialog.set(None)
    };
    let submit = move || {
        if s.form.with(|f| !f.password.is_empty()) && !s.busy.get() {
            dismiss_keyboard();
            s.import_file()
        }
    };
    super::shows_errors(s);
    // Its one field takes the keyboard at once, from the first frame it is up.
    let field = element_ref();
    request_animation_frame(move |_| field.focus());
    ui::dialog(
        move || s.label("Backup Password"),
        view()
            .gap(px(12.0))
            .child(ui::dim(ui::body(text(move || s.label("Enter the password that was used to export this backup.")))))
            .child(ui::field(
                s,
                "file-password",
                Entry::Password,
                move || s.label("Enter password"),
                move || s.form.with(|f| f.password.to_string()),
                move |v| {
                    s.error.set(None);
                    s.form.update(|f| f.password = Zeroizing::new(v.to_owned()))
                },
                submit,
                |_| {},
                move || s.error.get().is_some(),
                ReturnKeyType::Done,
                Some(field),
            ))
            .child(ui::field_note(move || s.error.get().map(|e| s.label(crate::i18n::error_key(e))), || true)),
        view()
            .flex_row()
            .gap(px(4.0))
            .child(ui::text_button(move || s.label("Cancel"), false).on_press(close))
            .child(ui::text_button(move || s.label("Import"), false).test_id("import-go").on_press(submit)),
        close,
    )
}
