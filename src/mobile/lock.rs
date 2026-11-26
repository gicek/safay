//! The lock screen: the mark, "Safay is locked", what unlocks it and the password field in one centered group. Under
//! the field, a round button: the fingerprint where biometrics unlock, else an arrow that does what the keyboard's Go
//! key does.
use super::ui;
use crate::{components::app_icon, store::Store};
use nsh_base::theme;
use nsh_core::prelude::*;
use safay_app::Zeroizing;

pub fn page(s: Store) -> impl IntoElement {
    let waiting = move || s.projection.with(|p| p.waiting);
    let message = move || {
        if waiting() > 0 {
            Some(s.count("wait", waiting() as usize))
        } else {
            // A prompt that could not be shown or did not verify says so here.
            s.error.get().map(|e| s.label(crate::i18n::error_key(e))).or_else(|| s.status.get().map(|status| status.text))
        }
    };
    let submit = move || {
        if waiting() == 0 && !s.busy.get() && s.form.with(|f| !f.password.is_empty()) {
            s.unlock()
        }
    };
    // The prompt opens by itself once, on the lock screen the app starts with; after that it waits for its button.
    effect(move || {
        if s.touch_id_ready() && !s.biometric_offered.replace(true) {
            untracked(|| s.unlock_touch_id());
        }
    });
    scroll().flex_1().keyboard_dismiss_mode(KeyboardDismissMode::OnDrag).keyboard_taps(KeyboardTaps::Handled).child(
        view()
            .flex_grow(1.0)
            .items_center()
            .justify_center()
            .pt_safe_area_plus(px(32.0))
            // Centered in what the keyboard leaves: the window does not shrink for it, and the field is typed into.
            .pb(move || px(72.0) + keyboard_height().max(safe_area().bottom))
            .px(px(24.0))
            .child(
                view()
                    .w_full()
                    .max_w(px(400.0))
                    .items_center()
                    .child(app_icon(120.0))
                    .child(ui::headline(text(move || s.label("Safay is locked"))).mt(px(16.0)).mb(px(40.0)))
                    // What unlocks it, over a field of its own shape: the field needs no label then.
                    .child(
                        ui::dim(ui::body(text(move || {
                            s.label(if s.touch_id_ready() { ui::Biometric::get().prompt() } else { "Use your master password to unlock" })
                        })))
                        .text_center()
                        .mb(px(12.0)),
                    )
                    .child(
                        ui::pill_password(
                            s,
                            "password",
                            move || s.label("Master Password"),
                            move || s.form.with(|f| f.password.to_string()),
                            move |v| {
                                s.error.set(None);
                                s.form.update(|f| f.password = Zeroizing::new(v.to_owned()));
                            },
                            submit,
                            move || s.error.get().is_some(),
                        )
                        .w_full(),
                    )
                    .child(
                        ui::caption(text(move || message().unwrap_or_else(|| " ".into())))
                            .test_id("error")
                            .text_center()
                            .mt(px(12.0))
                            .text_color(move || theme().destructive),
                    )
                    // A round button under it: the fingerprint where it unlocks, else Enter's arrow.
                    .child(
                        when(
                            move || s.touch_id_ready(),
                            move || {
                                round_action(s)
                                    .test_id("biometric")
                                    .accessibility_label(move || Some(s.label(ui::Biometric::get().unlock())))
                                    .disabled(move || s.busy.get())
                                    .on_press(move || {
                                        dismiss_keyboard();
                                        s.unlock_touch_id()
                                    })
                                    .child(crate::icons::icon(ui::Biometric::get().icon()).size(px(36.0)))
                            },
                        )
                        .otherwise(move || {
                            let ready = move || waiting() == 0 && s.form.with(|f| !f.password.is_empty());
                            round_action(s)
                                .test_id("unlock")
                                .accessibility_label(move || Some(s.label("Unlock")))
                                .disabled(move || !ready() || s.busy.get())
                                .opacity(move || if ready() || s.busy.get() { 1.0_f32 } else { 0.38 })
                                .on_press(submit)
                                .child(
                                    when(move || s.busy.get(), move || crate::components::spinner_in(24.0, move || theme().primary))
                                        .otherwise(|| crate::icons::icon(crate::icons::Icon::ArrowRight).size(px(28.0))),
                                )
                        }),
                    ),
            ),
    )
}

/// The round button under the password field: 64 pt on a tint of the accent, the glyph in the accent.
fn round_action(_s: Store) -> nsh_base::ButtonElement {
    nsh_base::button()
        .size(px(64.0))
        .mt(px(8.0))
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(move || theme().primary.opacity(0.12))
        .text_color(move || theme().primary)
        .active(|st| st.bg(move || theme().primary.opacity(0.2)))
}
