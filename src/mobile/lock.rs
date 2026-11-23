//! The lock screen: the mark, "Safay is locked" and the password field in one centered group. The keyboard's Go key
//! unlocks; there is no button.
use super::ui::{self, Entry};
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
    thread_local! {
        static PROMPTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    effect(move || {
        if s.touch_id_ready() && !PROMPTED.with(|p| p.replace(true)) {
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
                    .child(ui::headline(text(move || s.label("Safay is locked"))).mt(px(16.0)).mb(px(56.0)))
                    .child(
                        ui::field(
                            s,
                            "password",
                            Entry::Password,
                            move || s.label("Master Password"),
                            move || s.form.with(|f| f.password.to_string()),
                            move |v| {
                                s.error.set(None);
                                s.form.update(|f| f.password = Zeroizing::new(v.to_owned()));
                            },
                            submit,
                            |_| {},
                            move || s.error.get().is_some(),
                            ReturnKeyType::Go,
                            None,
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
                    .child(when(
                        move || s.touch_id_ready(),
                        move || {
                            nsh_base::button()
                                .test_id("biometric")
                                .accessibility_label(move || Some(s.label("Unlock with biometrics")))
                                .size(px(68.0))
                                .mt(px(8.0))
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .text_color(move || theme().primary)
                                .active(|st| st.bg(move || crate::components::tok("pressed")))
                                .disabled(move || s.busy.get())
                                .on_press(move || {
                                    dismiss_keyboard();
                                    s.unlock_touch_id()
                                })
                                .child(crate::icons::icon(crate::icons::Icon::TouchId).size(px(44.0)))
                        },
                    )),
            ),
    )
}
