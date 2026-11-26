//! First run: a welcome page with two choices, then the page that sets the master password. Restoring a backup joins
//! once the phone has backup locations.
use super::ui::{self, Entry};
use crate::{
    components::{app_icon, tok},
    icons::{Icon, icon},
    pages::unlock::{mismatch, new_password_ok},
    store::{Stage, Store},
};
use nsh_base::{button, theme};
use nsh_core::prelude::*;
use safay_app::{Zeroizing, core::password};

/// Back to the welcome page, with nothing typed left behind.
pub fn leave(s: Store) {
    dismiss_keyboard();
    s.clear();
    s.discard_restore_location();
    s.stage.set(Stage::Welcome);
}

/// One step back, from the top bar or the back key; nothing while the vault is being made or restored. False on the
/// welcome page.
pub fn back(s: Store) -> bool {
    if s.stage.get_untracked() == Stage::Welcome {
        return false;
    }
    if !s.busy.get_untracked() {
        if super::backup::restoring(s) { super::backup::restore_back(s) } else { leave(s) }
    }
    true
}

pub fn page(s: Store) -> AnyElement {
    if ui::IOS {
        return native(s).into_element();
    }
    step(s).into_element()
}
/// The page of the stage shown: setting the master password, restoring, or the welcome page.
fn step(s: Store) -> impl IntoElement {
    when(move || s.stage.get() == Stage::Create, move || create(s)).otherwise(move || {
        when(
            move || super::backup::restoring(s),
            move || super::backup::restore_page(s, |top, content| scaffold(top, content).into_element()),
        )
        .otherwise(move || welcome(s))
    })
}

/// iOS: the welcome page in the system's navigation stack, and the step a choice opens pushed over it, with the
/// system's back button and swipe. Back from a found backup chooses the location again instead of leaving, and nothing
/// goes back while the vault is being made or restored.
fn native(s: Store) -> impl IntoElement {
    let steps = computed(move || if s.stage.get() == Stage::Welcome { Vec::new() } else { vec![s.stage.get()] });
    nsh_base::nav_stack()
        .flex_1()
        .child(nsh_base::nav_page().key("welcome").header_visible(false).bg(move || theme().background).child(welcome(s)))
        .child(each(
            move || steps.get(),
            |stage: &Stage| *stage as u8,
            move |_, _| {
                nsh_base::nav_page()
                    .title("")
                    .header_shadow_visible(false)
                    .bg(move || theme().background)
                    // The system's back pops only where it leaves onboarding's step; elsewhere `back` takes it.
                    .pop_on_back(move || !s.busy.get() && !crate::pages::unlock::restore_found(s))
                    .on_back_press(move || {
                        if crate::pages::unlock::restore_found(s) {
                            back(s);
                        }
                    })
                    .on_dismiss(move || leave(s))
                    .child(step(s))
            },
        ))
}

/// A page of onboarding: scrolls, clear of the system bars and the keyboard, its column at most 480 pt wide. iOS: under
/// the system's header (`native`).
fn scaffold(top: impl IntoElement, content: impl IntoElement) -> impl IntoElement {
    // The status bar's room is outside the scroll view: what scrolls up ends below the status bar, not behind it.
    view().flex_1().pt(ui::page_top).child(
        scroll().flex_1().keyboard_dismiss_mode(KeyboardDismissMode::None).keyboard_taps(KeyboardTaps::Handled).child(
            view()
                .pb_safe_area_plus(px(24.0))
                .items_center()
                .child(view().w_full().child(top))
                .child(view().w_full().max_w(px(480.0)).px(px(24.0)).gap(px(16.0)).items_center().child(content)),
        ),
    )
}

fn welcome(s: Store) -> impl IntoElement {
    scaffold(
        view().h(px(48.0)),
        view()
            .w_full()
            .items_center()
            .gap(px(16.0))
            .child(view().mt(px(24.0)).mb(px(8.0)).child(app_icon(128.0)))
            .child(ui::display(text(move || s.label("Protect your 2FA with Safay"))).text_center())
            .child(
                ui::dim(ui::body_large(text(move || {
                    s.label("End-to-end encrypted, effortless to use, and ready on every device you own.")
                })))
                .text_center()
                .mb(px(16.0)),
            )
            .child(
                view()
                    .w_full()
                    // iOS: two cards of their own, apart.
                    .gap(px(if ui::IOS { 12.0 } else { ui::GROUP_GAP }))
                    .child(choice(
                        s,
                        Icon::PlusCircleFill,
                        "primary-container",
                        "primary-container-foreground",
                        "Start fresh",
                        "Set a master password to get started.",
                        move || {
                            s.clear();
                            s.stage.set(Stage::Create)
                        },
                    ))
                    .child(choice(
                        s,
                        Icon::CloudDown,
                        "secondary",
                        "foreground",
                        "Restore a backup",
                        "From a cloud service or an exported file.",
                        move || {
                            s.clear();
                            s.stage.set(Stage::Restore)
                        },
                    )),
            ),
    )
}

/// A choice of the welcome page: a tile, a title over a line of detail, a chevron.
#[allow(clippy::too_many_arguments)]
fn choice(
    s: Store,
    glyph: Icon,
    fill: &'static str,
    ink: &'static str,
    title: &'static str,
    detail: &'static str,
    press: impl Fn() + 'static,
) -> impl IntoElement {
    button()
        .test_id(title)
        .flex_row()
        .items_center()
        .gap(px(16.0))
        .p(px(18.0))
        .rounded(px(if ui::IOS { ui::GROUP_RADIUS } else { 28.0 }))
        .bg(move || theme().card)
        .active(|st| st.bg(move || if ui::IOS { tok("pressed") } else { tok("surface-high") }))
        .on_press(press)
        .child(ui::icon_tile(glyph, 48.0, if ui::IOS { 12.0 } else { 16.0 }, fill, ink))
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .gap(px(2.0))
                .child(ui::title(text(move || s.label(title))).font_weight(FontWeight::SEMIBOLD))
                .child(ui::dim(ui::body(text(move || s.label(detail))))),
        )
        .child(ui::chevron())
}

fn create(s: Store) -> impl IntoElement {
    let submit = move || {
        if new_password_ok(s) && !s.busy.get() {
            dismiss_keyboard();
            let p = s.form.with(|f| f.new_password.clone());
            s.confirm_fair(p, move |c| s.create(c));
        }
    };
    scaffold(
        ui::top_bar(s, None, move || {
            back(s);
        }),
        view()
            .w_full()
            .gap(px(16.0))
            .child(
                view()
                    .items_center()
                    .gap(px(12.0))
                    .child(ui::icon_tile(Icon::Key, 88.0, 30.0, "secondary", "foreground"))
                    .child(ui::headline(text(move || s.label("Set master password"))).text_center())
                    .child(
                        ui::dim(ui::body_large(text(move || {
                            s.label("Your master password encrypts your 2FA on this device and in every backup. Pick one you won’t forget.")
                        })))
                        .text_center(),
                    ),
            )
            .child(
                view()
                    .gap(px(12.0))
                    .child(
                        ui::form_card().child(new_password_fields(s, "New Password", "Enter again", submit)),
                    )
                    .child(
                        view()
                            .flex_row()
                            .gap(px(12.0))
                            .px(px(8.0))
                            .child(view().pt(px(2.0)).text_color(move || theme().muted_foreground).child(icon(Icon::Info).size(px(18.0))))
                            .child(
                                ui::dim(ui::caption(text(move || {
                                    s.label("Your master password is never uploaded and can’t be recovered. If you forget it, this device and your backups can’t be decrypted.")
                                })))
                                .flex_1(),
                            ),
                    ),
            )
            .child(when(
                move || s.error.get().is_some(),
                move || ui::body(text(move || s.error.get().map(|e| s.label(crate::i18n::error_key(e))).unwrap_or_default())).text_color(move || theme().destructive),
            ))
            .child(
                ui::primary_button(move || s.label("Create Vault"), move || new_password_ok(s), move || s.busy.get()).test_id("create").on_press(submit),
            ),
    )
}

/// A new password and its confirmation, with the strength bar between them: shared by every page that sets a password.
/// The requirement under the bar shows only while there is one; the mismatch line always keeps its height.
pub fn new_password_fields(
    s: Store,
    new_label: &'static str,
    confirm_label: &'static str,
    submit: impl Fn() + Copy + 'static,
) -> impl IntoElement {
    let confirm = element_ref();
    let level = move || s.strength.with(|x| x.0);
    let short = move || s.form.with(|f| f.new_password.chars().count()) < 8;
    let weak = move || !short() && !s.strength.with(|x| x.1);
    view()
        .gap(px(8.0))
        .child(ui::field(
            s,
            "new-password",
            Entry::Password,
            move || s.label(new_label),
            move || s.form.with(|f| f.new_password.to_string()),
            move |v| {
                s.error.set(None);
                let next = password::filter_password_input(v);
                s.form.update(|f| f.new_password = Zeroizing::new(next));
            },
            move || confirm.focus(),
            move |focused| {
                s.marks.update(|m| {
                    m.new_focused = focused;
                    m.new_left |= !focused;
                })
            },
            weak,
            ReturnKeyType::Next,
            None,
        ))
        // The strength meter, with a little more room under it than the gap: the next field's label stands above it.
        .child(view().flex_row().gap(px(4.0)).px(px(4.0)).mb(px(4.0)).children((1..=4u8).map(move |i| {
            view()
                .flex_1()
                .h(px(4.0))
                .rounded_full()
                .bg(move || {
                    if level() < i {
                        tok("surface-highest")
                    } else {
                        match level() {
                            1 => theme().destructive,
                            2 => tok("urgent"),
                            _ => tok("success"),
                        }
                    }
                })
                .transition()
                .duration(200.0)
                .ease_out()
        })))
        .child(when(
            move || short() || weak(),
            move || {
                ui::caption(text(move || {
                    s.label(if short() {
                        "At least 8 characters"
                    } else {
                        "Too easy to crack. Use a longer password, or a few unrelated words."
                    })
                }))
                .px(px(4.0))
                .text_color(move || if weak() { theme().destructive } else { theme().muted_foreground })
            },
        ))
        .child(ui::field(
            s,
            "confirm-password",
            Entry::Password,
            move || s.label(confirm_label),
            move || s.form.with(|f| f.confirmation.to_string()),
            move |v| {
                s.error.set(None);
                let next = password::filter_password_input(v);
                s.form.update(|f| f.confirmation = Zeroizing::new(next));
            },
            submit,
            move |focused| s.marks.update(|m| m.confirm_focused = focused),
            move || mismatch(s).is_some(),
            ReturnKeyType::Done,
            Some(confirm),
        ))
        .child(ui::field_note(move || mismatch(s), || true))
}
