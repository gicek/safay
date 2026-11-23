//! Lock screen (UnlockForm, window style) and onboarding (welcome, create, restore), both immersive:
//! no toolbar, the window drags from anywhere empty and the traffic lights float over the content.
use crate::{
    chrome::{Look, look},
    components::*,
    icons::{Icon, icon, icon_bold},
    store::{Stage, Store},
};
use nsh_base::{button, input, theme};
use nsh_core::prelude::*;
use safay_app::{Zeroizing, core::password};

pub fn lock_screen(s: Store) -> impl IntoElement {
    view().flex_1().window_drag(true).items_center().justify_center().p(px(24.0)).child(unlock_form(s, false))
}

/// The same form in the window (112pt icon) and in the tray panel (70pt icon).
pub fn unlock_form(s: Store, panel: bool) -> impl IntoElement {
    let corrupt = move || s.error.get() == Some(safay_app::session::Error::Corrupt);
    view()
        .items_center()
        .gap(px(if panel { 12.0 } else { 16.0 }))
        .window_drag(false)
        .child(
            view()
                .size(px(if panel { 70.0 } else { 112.0 }))
                .child(app_icon(if panel { 70.0 } else { 112.0 }))
                .child(when(move || s.touch_id_ready(), move || touch_id_button(s, panel))),
        )
        .child(
            view().items_center().gap(px(6.0)).child(heading(move || s.label("Safay is locked"), if panel { 14.0 } else { 20.0 })).child(
                text(move || {
                    s.label(if s.touch_id_ready() {
                        "Use Touch ID or enter your master password to unlock"
                    } else {
                        "Enter your master password to unlock"
                    })
                })
                .max_w(px(if panel { 230.0 } else { 280.0 }))
                .text_center()
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
            ),
        )
        .child(capsule_password(s, if panel { 210.0 } else { 220.0 }))
        .child(
            text(move || {
                let waiting = s.projection.with(|p| p.waiting);
                if waiting > 0 {
                    s.count("wait", waiting as usize)
                } else {
                    s.error
                        .get()
                        .map(|e| s.label(crate::i18n::error_key(e)))
                        .or_else(|| s.status.get().map(|st| st.text))
                        .unwrap_or_else(|| " ".into())
                }
            })
            .test_id("error")
            .max_w(px(280.0))
            .text_center()
            .text_size(px(11.5))
            // Errors and the wait in red; a hint (the panel's "Unlock Safay first") in the secondary color.
            .text_color(move || {
                let hint = s.projection.with(|p| p.waiting) == 0
                    && s.error.get().is_none()
                    && s.status.get().is_some_and(|st| st.tone == crate::store::Tone::Info);
                if hint { theme().muted_foreground } else { theme().destructive }
            }),
        )
        .child(when(
            move || !panel && corrupt() && s.projection.with(|p| p.has_snapshot),
            move || {
                push_button(move || s.label("Restore Last Good Copy"), Kind::Link, false)
                    .disabled(move || s.form.with(|f| f.password.is_empty()) || s.busy.get())
                    .on_press(move || s.restore_snapshot())
            },
        ))
}

static PROMPTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The pink fingerprint at the icon's corner. The main window prompts once on its own, on the first lock screen after
/// launch only (never after a manual, automatic or sleep lock).
fn touch_id_button(s: Store, panel: bool) -> impl IntoElement {
    if !panel && s.surface == crate::store::Surface::Main {
        effect(move || {
            if s.touch_id_ready() && !PROMPTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
                untracked(|| s.unlock_touch_id());
            }
        });
    }
    let size = if panel { 34.0 } else { 56.0 };
    let offset = if panel { -8.0 } else { -12.0 };
    view().absolute().right(px(offset)).bottom(px(offset)).child(
        button()
            .accessibility_label(move || Some(s.label("Unlock with Touch ID")))
            .disabled(move || s.busy.get())
            .size(px(size))
            .rounded_full()
            .items_center()
            .justify_center()
            .bg(move || tok("glass"))
            .border_1()
            .border_color(move || tok("separator").opacity(0.5))
            .shadow([soft_shadow(4.0, 8.0, 0.10)])
            .text_color(move || tok("touchid"))
            .hover(|st| st.scale(1.08))
            .active(|st| st.scale(0.96))
            .focus(|st| st.border_color(move || theme().ring))
            .on_press(move || s.unlock_touch_id())
            // Fills the circle as the system's Touch ID prompt does.
            .child(crate::icons::touch_id_glyph(size * 0.7)),
    )
}
/// safayCapsuleInputChrome: centered text with the reveal button balanced by left padding.
///
/// The masked and the plain input are both always there, stacked in the same place; only the one showing has a text
/// color and takes input, and showing or hiding swaps them in one update, so the text turns over in place. NSH's macOS
/// backend has two faults with centered text (docs/ARCHITECTURE.md), worked around here:
/// - Text the app set into a field shows left-aligned until the field is configured again. The masked input, centered,
///   gets the text that way while hidden; the color it gets when shown configures it again in the same update.
/// - Centered text that overflows has its last glyph clipped. The plain input is therefore left-aligned and as wide as
///   its text (the code font measures exactly), centered in the space left of the eye; a longer password fills that
///   space and scrolls as usual. The masked input keeps its end in view by scrolling to the caret when it is shown.
fn capsule_password(s: Store, width: f32) -> impl IntoElement {
    let shown = signal(false);
    let (masked_ref, plain_ref) = (element_ref(), element_ref());
    // The space the inputs have (left of the eye), and the plain input's width: its text in the code font, with room
    // for the 3pt padding and AppKit's 2pt insets on both sides, and the caret.
    let space = signal(0.0_f32);
    let size = move || 12.5 * font_scale();
    let plain_width = move || {
        let font = format!("{}px \"{}\"", size(), CODE_FONT.family);
        (s.form.with(|f| measure_text(&font, &f.password).0) + 12.0).min(space.get())
    };
    let password_input = move |plain: bool| {
        let active = move || shown.get() == plain;
        let input = input()
            .element_ref(if plain { &plain_ref } else { &masked_ref })
            .test_id(if plain { "password-shown" } else { "password" })
            .auto_focus(!plain)
            .label(move || s.label("Master Password"))
            .masked(!plain)
            // Shown in the monospaced font, so the text keeps the masked bullets' even rhythm.
            .font_family(SharedString::from(if plain { CODE_FONT.family } else { "system-ui" }))
            .value(move || s.form.with(|f| f.password.to_string()))
            .on_change(move |v: &str| {
                s.form.update(|f| f.password = Zeroizing::new(v.into()));
                s.error.set(None);
            })
            .on_submit(move |_: &str| s.unlock())
            // In the menu bar panel, Escape closes it at once (the field takes Escape before the window sees it).
            .on_key_press(move |key: &str| {
                #[cfg(any(target_os = "macos", target_os = "linux"))]
                if key == "Escape" && s.surface == crate::store::Surface::Panel {
                    crate::desktop::close_panel();
                }
                let _ = key;
            })
            .on_focus(|_: &()| roman_input_only(true))
            .on_blur(|_: &()| roman_input_only(false))
            .auto_correct(false)
            .auto_fill(nsh_core::AutoFill::Off)
            .disabled(move || s.busy.get() || !active())
            .focusable(active)
            .pointer_events(move || if active() { PointerEvents::Auto } else { PointerEvents::None })
            .accessibility_elements_hidden(move || !active())
            .absolute()
            .top(px(0.0))
            .bottom(px(0.0))
            // Room for the last glyph of a long password: NSH clips the native field a little inside its frame.
            .px(px(3.0))
            .text_size(px(12.5))
            .text_color(move || if active() { theme().foreground } else { theme().foreground.opacity(0.0) });
        if plain {
            input.text_left().w(move || px(plain_width())).left(move || px(((space.get() - plain_width()) / 2.0).max(0.0)))
        } else {
            input.text_center().left(px(0.0)).right(px(0.0))
        }
    };
    let field = view()
        .w(px(width))
        .h(move || px(34.0) * font_scale())
        .flex_row()
        .items_center()
        // The left padding balances the eye and the gap before it, so the text is centered in the capsule.
        .pl(px(36.0))
        .gap(px(6.0))
        // The eye's round hover sits 2pt inside the capsule's border at its end, as above and below it (34pt capsule
        // with a 1pt border, 28pt button), so the two curves run parallel.
        .pr(px(2.0))
        // The inputs' positioning box: the space left of the eye.
        .child(
            view()
                .relative()
                .flex_1()
                .h_full()
                .min_w(px(0.0))
                .on_layout(move |bounds: &Bounds<Points>| space.set(bounds.size.width.0))
                .child(password_input(false))
                .child(password_input(true)),
        )
        .child(
            reveal_toggle(
                move || shown.get(),
                move || s.label(if shown.get() { "Hide" } else { "Show" }),
                move || {
                    shown.update(|v| *v = !*v);
                    if shown.get_untracked() {
                        plain_ref.focus()
                    } else {
                        masked_ref.focus()
                    }
                    // The caret goes to the end; a long password shows its end.
                    scroll_to_caret();
                },
            )
            .size(px(28.0))
            .rounded_full(),
        );
    input_chrome(field, 17.0).rounded_full()
}

// ---------------------------------------------------------------- onboarding

pub fn onboarding(s: Store) -> impl IntoElement {
    // The welcome page is centered in the content area; the create and restore steps align their top with the sidebar
    // graphic (50pt). Page changes are not animated.
    let column = move |content: AnyElement| view().w(px(520.0)).max_w(percentage(100.0)).window_drag(false).child(content);
    view().flex_1().flex_row().child(when(move || window().width.0 >= 760.0, move || onboarding_sidebar(s))).child(
        scroll().flex_1().window_drag(true).child(
            when(
                move || s.stage.get() == Stage::Welcome,
                move || {
                    view()
                        .min_h(move || window().height)
                        .px(px(24.0))
                        .py(px(24.0))
                        .items_center()
                        .justify_center()
                        .child(column(welcome(s).into_element()))
                },
            )
            .otherwise(move || {
                view().min_h(move || window().height).px(px(24.0)).pt(px(50.0)).pb(px(22.0)).items_center().child(column(
                    view()
                        .child(when(move || s.stage.get() == Stage::Create, move || create(s)))
                        .child(when(move || s.stage.get() == Stage::Restore, move || restore(s)))
                        .into_element(),
                ))
            }),
        ),
    )
}
fn onboarding_sidebar(s: Store) -> impl IntoElement {
    view()
        .w(px(292.0))
        .px(px(28.0))
        .pt(px(50.0))
        .pb(px(28.0))
        .window_drag(true)
        .bg(move || tok("material"))
        .border_r_1()
        .border_color(move || tok("sidebar-border"))
        .child(
            view().gap(px(18.0)).child(super::graphic::vault_graphic()).child(
                view().gap(px(6.0)).child(heading(|| "Safay".into(), 25.0)).child(
                    text(move || s.label("Secure, modern 2FA that stays in sync across all your devices."))
                        .text_size(px(12.5))
                        .line_height(px(18.0))
                        .text_color(move || theme().muted_foreground),
                ),
            ),
        )
        .child(spacer())
        .child(
            view().gap(px(11.0)).children(
                [
                    (Icon::Hand, "Built for privacy and security"),
                    (Icon::Key, "Your password is never saved"),
                    (Icon::LockShield, "End-to-end encrypted storage"),
                ]
                .into_iter()
                .map(move |(glyph, key)| {
                    view()
                        .flex_row()
                        .items_center()
                        .gap(px(9.0))
                        .child(view().w(px(18.0)).items_center().text_color(move || tok("code-accent")).child(icon(glyph).size(px(14.0))))
                        .child(text(move || s.label(key)).text_size(px(12.0)).text_color(move || theme().muted_foreground))
                }),
            ),
        )
}
fn welcome(s: Store) -> impl IntoElement {
    // The highlighted card: the recommended one at first, then the one last hovered (it stays when the pointer leaves,
    // so the highlight never jumps back). Keyboard focus shows its own ring.
    let selected = signal(0_u8);
    view()
        .gap(px(24.0))
        .child(
            view()
                .gap(px(10.0))
                .child(when(move || window().width.0 < 760.0, || app_icon(64.0)))
                .child(heading(move || s.label("Protect your 2FA with Safay"), 30.0))
                .child(
                    text(move || s.label("End-to-end encrypted, effortless to use, and ready on every device you own."))
                        .text_size(px(14.5))
                        .line_height(px(21.0))
                        .text_color(move || theme().muted_foreground),
                ),
        )
        .child(
            view()
                .gap(px(12.0))
                .child(choice_card(
                    s,
                    "welcome-create",
                    Icon::PlusCircleFill,
                    move || tok("code-accent"),
                    "Start fresh",
                    "Set a master password to get started.",
                    0,
                    selected,
                    Stage::Create,
                ))
                .child(choice_card(
                    s,
                    "welcome-restore",
                    Icon::CloudDownFill,
                    move || tok("success"),
                    "Restore a backup",
                    "From a backup folder or an exported file.",
                    1,
                    selected,
                    Stage::Restore,
                )),
        )
}
/// Selection colors cross-fade over 180 ms (ease-out). Hovering lifts the card 2 pt and 1.1 % larger on a curve that
/// overshoots a little and settles (a soft spring, 420 ms), with a deeper shadow; pressing scales down slightly. NSH
/// skips transitions under reduced motion.
const CHOICE_FADE_MS: f32 = 180.0;
const CHOICE_LIFT_MS: f32 = 420.0;
const CHOICE_SPRING: [f32; 4] = [0.3, 1.4, 0.5, 1.0];
#[allow(clippy::too_many_arguments)]
fn choice_card(
    s: Store,
    id: &'static str,
    glyph: Icon,
    tint: impl Fn() -> AdaptiveColor + Copy + 'static,
    title: &'static str,
    detail: &'static str,
    index: u8,
    selected: Signal<u8>,
    stage: Stage,
) -> impl IntoElement {
    let _ = id;
    let on = move || selected.get() == index;
    let still = reduce_motion();
    let [x1, y1, x2, y2] = CHOICE_SPRING;
    view()
        .on_mouse_enter(move |_: &MouseEvent| selected.set(index))
        .transition_transform()
        .duration(CHOICE_LIFT_MS)
        .ease_bezier(x1, y1, x2, y2)
        .hover(|st| st.translate_y(px(-2.0)).scale(1.011))
        .child(
            button()
                .accessibility_label(move || Some(s.label(title)))
                .on_press(move || {
                    s.error.set(None);
                    s.stage.set(stage)
                })
                .flex_row()
                .items_center()
                .gap(px(14.0))
                .p(px(16.0))
                .min_h(px(90.0))
                .rounded(px(13.0))
                .bg(move || theme().card)
                .border(px(1.4))
                .border_color(move || if on() { tint().opacity(0.55) } else { tok("separator").opacity(0.6) })
                .shadow([soft_shadow(4.0, 8.0, 0.06)])
                .transition()
                .duration(CHOICE_FADE_MS)
                .ease_out()
                .hover(|st| st.shadow([soft_shadow(8.0, 18.0, 0.10)]))
                .focus(|st| st.border_color(move || theme().ring))
                .active(move |st| if still { st } else { st.scale(0.985) })
                .child(
                    view()
                        .size(px(44.0))
                        .rounded(px(10.0))
                        .items_center()
                        .justify_center()
                        .bg(move || tint().opacity(if on() { 0.16 } else { 0.11 }))
                        .transition_colors()
                        .duration(CHOICE_FADE_MS)
                        .ease_out()
                        .text_color(tint)
                        .child(icon(glyph).size(px(25.0))),
                )
                .child(
                    view()
                        .flex_1()
                        .gap(px(3.0))
                        .child(heading(move || s.label(title), 15.5))
                        .child(text(move || s.label(detail)).text_size(px(12.5)).text_color(move || theme().muted_foreground)),
                )
                .child(
                    view()
                        .text_color(move || if on() { tint().opacity(0.9) } else { tok("tertiary") })
                        .transition_colors()
                        .duration(CHOICE_FADE_MS)
                        .ease_out()
                        .child(icon(Icon::ChevronRight).size(px(14.0))),
                ),
        )
}
/// OnboardingTaskPanel: a back button, title and description, the form, and an action row.
fn task_panel(
    s: Store,
    title: &'static str,
    description: &'static str,
    body: impl IntoElement,
    footer: impl IntoElement,
) -> impl IntoElement {
    view()
        .min_h(move || window().height - px(50.0 + 28.0))
        .child(
            view()
                .gap(px(9.0))
                .pb(px(24.0))
                .child(
                    button()
                        .accessibility_label(move || Some(s.label("Back")))
                        .size(px(36.0))
                        .ml(px(-8.0))
                        .rounded_full()
                        .items_center()
                        .justify_center()
                        .hover(|st| st.bg(move || tok("hover")))
                        .active(|st| st.bg(move || tok("pressed")).scale(0.94))
                        .focus(focus_ring)
                        .disabled(move || s.busy.get())
                        .on_press(move || {
                            s.error.set(None);
                            s.form.set(crate::store::Form::fresh());
                            s.file.replace(None);
                            s.file_name.set(String::new());
                            s.discard_restore_location();
                            s.status.set(None);
                            s.stage.set(Stage::Welcome)
                        })
                        .child(icon_bold(Icon::Back).size(px(20.0))),
                )
                .child(heading(move || s.label(title), 27.0))
                .child(
                    text(move || s.label(description))
                        .text_size(px(14.0))
                        .line_height(px(20.0))
                        .text_color(move || theme().muted_foreground),
                ),
        )
        .child(body)
        // The action row stays at the bottom, aligned with the sidebar's security points.
        .child(spacer())
        .child(view().pt(px(18.0)).flex_row().items_center().gap(px(20.0)).min_h(px(42.0)).child(footer))
}
fn forget_note(s: Store) -> impl IntoElement {
    view().flex_1().flex_row().gap(px(6.0)).child(view().text_color(move || tok("warning")).child(icon(Icon::Info).size(px(14.0)))).child(
        text(move || s.label("If you forget your master password, this device and your backups can’t be decrypted."))
            .flex_1()
            .text_size(px(11.0))
            .line_height(px(15.0))
            .text_color(move || theme().muted_foreground),
    )
}
/// Whether to say the confirmation does not match, on every new-password form (new vault, restore with a separate master
/// password, change password, export). Nothing while the new password is being edited or either field is empty, nor while
/// the confirmation being typed is still a prefix of the new password; one wrong character says so at once, and so does an
/// incomplete confirmation once its field is left.
pub fn confirmation_mismatch(password: &str, confirm: &str, editing_password: bool, editing_confirm: bool) -> bool {
    if editing_password || password.is_empty() || confirm.is_empty() || password == confirm {
        return false;
    }
    !(editing_confirm && password.starts_with(confirm))
}
/// The mismatch message, checked only once the new password itself is acceptable (its own problem shows first).
pub fn mismatch(s: Store) -> Option<SharedString> {
    if !s.strength.with(|x| x.1) {
        return None;
    }
    let m = s.marks.get();
    s.form
        .with(|f| confirmation_mismatch(&f.new_password, &f.confirmation, m.new_focused, m.confirm_focused))
        .then(|| s.label("Passwords do not match"))
}
pub fn new_password_ok(s: Store) -> bool {
    s.strength.with(|x| x.1) && s.form.with(|f| f.new_password == f.confirmation)
}
/// New + confirm fields with the strength meter, shared by onboarding, export and change password.
pub fn new_password_fields(
    s: Store,
    new_label: &'static str,
    confirm_label: &'static str,
    submit: impl Fn() + Copy + 'static,
    side_by_side: bool,
    // The export sheet's form: labeled fields without a reveal button, the mismatch in the footer.
    plain: bool,
) -> impl IntoElement {
    let label = move |key: &'static str| {
        if plain {
            text(move || s.label(key))
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(move || theme().muted_foreground)
                .into_element()
        } else {
            field_label(s, key).into_element()
        }
    };
    let new_field = view()
        .gap(px(if plain { 6.0 } else { 7.0 }))
        .child(label(new_label))
        .child(secure_field_with(
            s,
            "new-password",
            move || s.label(new_label),
            move || s.form.with(|f| f.new_password.to_string()),
            move |v| {
                let next = password::filter_password_input(v);
                s.form.update(|f| f.new_password = Zeroizing::new(next));
            },
            submit,
            move |focused| {
                s.marks.update(|m| {
                    m.new_focused = focused;
                    m.new_left |= !focused;
                })
            },
            true,
            !plain,
        ))
        // Length and strength problems show once the field has been left, and not while it is edited again.
        .child(strength_meter(s, move || s.marks.with(|m| m.new_left && !m.new_focused)));
    let confirm_field = view()
        .gap(px(if plain { 6.0 } else { 7.0 }))
        .child(label(confirm_label))
        .child(secure_field_with(
            s,
            "confirm-password",
            move || s.label(confirm_label),
            move || s.form.with(|f| f.confirmation.to_string()),
            move |v| {
                let next = password::filter_password_input(v);
                s.form.update(|f| f.confirmation = Zeroizing::new(next));
            },
            submit,
            move |focused| s.marks.update(|m| m.confirm_focused = focused),
            false,
            !plain,
        ))
        .child(when(move || !plain, move || field_message(move || mismatch(s))));
    if side_by_side {
        view().flex_row().gap(px(12.0)).child(new_field.flex_1()).child(confirm_field.flex_1()).into_element()
    } else {
        view().gap(px(14.0)).child(new_field).child(confirm_field).into_element()
    }
}
pub fn field_label(s: Store, key: &'static str) -> impl IntoElement {
    field_label_with(s, move || key)
}
fn field_label_with(s: Store, key: impl Fn() -> &'static str + 'static) -> impl IntoElement {
    text(move || s.label(key())).text_size(px(12.0)).font_weight(FontWeight::SEMIBOLD).text_color(move || theme().muted_foreground)
}
fn error_notice(s: Store) -> impl IntoElement {
    when(
        move || s.error.get().is_some(),
        move || {
            inline_notice(
                Icon::Warning,
                move || theme().destructive,
                move || s.error.get().map(|e| s.label(crate::i18n::error_key(e))).unwrap_or_default(),
            )
        },
    )
}
fn create(s: Store) -> impl IntoElement {
    let submit = move || {
        if new_password_ok(s) && !s.busy.get() {
            let p = s.form.with(|f| f.new_password.clone());
            s.confirm_fair(p, move |c| s.create(c));
        }
    };
    task_panel(
        s,
        "Set master password",
        if look() == Look::Mac {
            "Your master password encrypts your 2FA on this Mac and in every backup. Pick one you won’t forget."
        } else {
            "Your master password encrypts your 2FA on this device and in every backup. Pick one you won’t forget."
        },
        view()
            .gap(px(14.0))
            .child(new_password_fields(s, "New Password", "Confirm Password", submit, false, false))
            .child(error_notice(s))
            .child(when(|| crate::platform::supports::BIOMETRIC, move || touch_id_toggle(s))),
        view().flex_1().flex_row().items_center().gap(px(20.0)).child(forget_note(s)).child(
            push_button(move || s.label("Create Vault"), Kind::Primary, true)
                .disabled(move || !new_password_ok(s) || s.busy.get())
                .on_press(submit),
        ),
    )
}
/// OnboardingTouchIDToggle: sets the preference; the key is wrapped right after the vault opens.
pub fn touch_id_toggle(s: Store) -> impl IntoElement {
    let available = move || s.biometrics.get();
    let pink = move || tok("touchid");
    view()
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .p(px(16.0))
        .rounded(px(12.0))
        .bg(move || theme().card)
        .border_1()
        .border_color(move || tok("separator").opacity(0.45))
        .opacity(move || if available() { 1.0_f32 } else { 0.55 })
        .child(
            view()
                .size(px(38.0))
                .rounded(px(9.0))
                .items_center()
                .justify_center()
                .bg(move || pink().opacity(0.10))
                .text_color(pink)
                .child(crate::icons::touch_id_glyph(26.0)),
        )
        .child(
            view().flex_1().gap(px(2.0)).child(heading(move || s.label("Unlock with Touch ID"), 13.5)).child(
                text(move || {
                    s.label(if available() {
                        "You can always unlock with your master password."
                    } else {
                        "Touch ID is unavailable or not enrolled on this Mac."
                    })
                })
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
            ),
        )
        .child(switch_control(
            move || s.label("Unlock with Touch ID"),
            move || available() && s.preferences.get().touch_id,
            move || !available() || s.busy.get(),
            move |v| s.set_touch_id(v),
        ))
}
// First run, restoring a backup: shared by the desktop's page and the phone's.

/// The backup found is an export file chosen, not a backup in a location.
pub(crate) fn restore_from_export(s: Store) -> bool {
    !s.restore_location.with(|l| l.is_some()) && s.has_file()
}
/// A backup was found, in a location or as an export file.
pub(crate) fn restore_found(s: Store) -> bool {
    s.restore_location.with(|l| l.is_some()) || s.has_file()
}
/// Its password is typed and, for an export whose password won't be the master password too, a new one is good enough.
pub(crate) fn restore_ready(s: Store) -> bool {
    if !restore_found(s) || s.form.with(|f| f.password.trim().is_empty()) {
        return false;
    }
    !restore_from_export(s) || s.use_export_password.get() || new_password_ok(s)
}
pub(crate) fn submit_restore(s: Store) {
    if restore_ready(s) && !s.busy.get() {
        if restore_from_export(s) { s.restore_export(false) } else { s.restore_from_location() }
    }
}
/// Back from the backup found to where it is: what was found and typed goes, and its sign-in with it.
pub(crate) fn choose_restore_again(s: Store) {
    s.form.set(crate::store::Form::fresh());
    s.use_export_password.set(true);
    s.error.set(None);
    s.status.set(None);
    s.file.replace(None);
    s.file_name.set(String::new());
    s.discard_restore_location();
}

/// OnboardingRestoreStep: first the location list (two columns, export
/// files accepted), then, once a backup is found, the found card, its password and the Touch ID toggle. The
/// action row appears only with a backup: there is nothing to unlock while choosing.
fn restore(s: Store) -> impl IntoElement {
    let use_as_master = s.use_export_password;
    use_as_master.set(true);
    let export = move || restore_from_export(s);
    let found = move || restore_found(s);
    let ready = move || restore_ready(s);
    let submit = move || submit_restore(s);
    let change_location = move || choose_restore_again(s);
    task_panel(
        s,
        "Restore a backup",
        "Choose where your backup is, then unlock it with your master password.",
        view()
            .gap(px(14.0))
            .child(
                when(found, move || {
                    view()
                        .gap(px(14.0))
                        .child(backup_found(s, export, change_location))
                        .child(
                            view()
                                .gap(px(7.0))
                                .child(field_label_with(s, move || if export() { "Export Password" } else { "Existing Master Password" }))
                                .child(secure_field(
                                    s,
                                    "password",
                                    move || s.label(if export() { "Export Password" } else { "Existing Master Password" }),
                                    move || s.form.with(|f| f.password.to_string()),
                                    move |v| s.form.update(|f| f.password = Zeroizing::new(v.into())),
                                    submit,
                                    || {},
                                    true,
                                )),
                        )
                        .child(when(export, move || {
                            view()
                                .gap(px(12.0))
                                .child(
                                    nsh_base::checkbox()
                                        .checked(move || use_as_master.get())
                                        .on_change(move |v: &bool| use_as_master.set(*v))
                                        .flex_row()
                                        .items_center()
                                        .gap(px(8.0))
                                        .indicator(check_box(move || use_as_master.get()))
                                        .child(text(move || s.label("Use as Master Password")).text_size(px(13.0))),
                                )
                                .child(when(
                                    move || !use_as_master.get(),
                                    move || new_password_fields(s, "Master Password", "Confirm Password", submit, true, false),
                                ))
                        }))
                        .child(when(|| crate::platform::supports::BIOMETRIC, move || touch_id_toggle(s)))
                })
                .otherwise(move || restore_locations(s)),
            )
            .child(restore_notice(s)),
        when(found, move || {
            view()
                .flex_1()
                .flex_row()
                .items_center()
                .gap(px(20.0))
                // A separate master password for an export warns about forgetting it, as creating a vault does.
                .child(when(move || export() && !use_as_master.get(), move || forget_note(s)).otherwise(spacer))
                .child(
                    push_button(move || s.label("Unlock & Continue"), Kind::Primary, true)
                        .disabled(move || !ready() || s.busy.get())
                        .on_press(submit)
                        .child(when(move || s.busy.get(), || spinner(12.0))),
                )
        }),
    )
}
/// The location list in two columns: iCloud, cloud storage services, Other Folder, Safay Exported File. An export file can also be dropped
/// anywhere on the list (the exported-file cell lights up while one is over it).
fn restore_locations(s: Store) -> impl IntoElement {
    use super::backup::{LocationIcon, location_row};
    let over = signal(false);
    let icloud = safay_app::icloud::available();
    let idle = move || s.busy.get();
    let clear = move || {
        s.error.set(None);
        s.status.set(None);
    };
    // iCloud, the cloud storage services this build can sign in to, Other Folder, Safay Exported File.
    let mut cells = Vec::new();
    if crate::platform::supports::ICLOUD {
        cells.push(
            location_cell(
                location_row(
                    s,
                    LocationIcon::Icloud,
                    "iCloud",
                    (!icloud).then_some("Not available in this build of Safay"),
                    move || idle() || !icloud,
                    move || {
                        clear();
                        s.choose_restore_icloud()
                    },
                ),
                || false,
            )
            .into_element(),
        );
    }
    if crate::platform::supports::CLOUD {
        use safay_app::cloud::Provider;
        for provider in [Provider::Dropbox, Provider::Onedrive, Provider::GoogleDrive, Provider::Nextcloud, Provider::Webdav] {
            if super::backup::oauth_configured(provider) {
                cells.push(
                    location_cell(
                        location_row(s, LocationIcon::Cloud(provider), provider.name(), None, idle, move || {
                            clear();
                            s.choose_restore_cloud(provider)
                        }),
                        || false,
                    )
                    .into_element(),
                );
            }
        }
    }
    cells.push(
        location_cell(
            location_row(s, LocationIcon::Folder, "Other Folder", None, idle, move || {
                clear();
                s.choose_restore_folder()
            }),
            || false,
        )
        .into_element(),
    );
    cells.push(
        location_cell(
            location_row(s, LocationIcon::ExportFile, "Safay Exported File", None, idle, move || {
                clear();
                s.choose_backup()
            }),
            move || over.get(),
        )
        .into_element(),
    );
    view()
        .gap(px(14.0))
        .on_file_drag_enter(move |_: &()| over.set(true))
        .on_file_drag_leave(move |_: &()| over.set(false))
        .on_file_drop(move |e: &nsh_core::element::FileDropEvent| {
            over.set(false);
            if s.busy.get() {
                return;
            }
            if let Some(path) = e.uris.first().and_then(|u| file_path(u)) {
                s.status.set(None);
                s.task(async move {
                    match crate::platform::files::read_named(path).await {
                        Ok((name, bytes)) => s.take_file(name, bytes),
                        Err(e) => s.error.set(Some(e)),
                    }
                });
            }
        })
        .child(super::backup::location_grid(cells))
        .child(when(idle, move || {
            view()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .child(spinner(12.0))
                .child(text(move || s.label("Looking for a Safay backup")).text_size(px(12.0)).text_color(move || theme().muted_foreground))
        }))
}
/// One grid cell. The cell is an unpadded flex item, so a line with an empty slot keeps the same column widths (a
/// padded item's flex basis is at least its padding). Every row keeps a clear 1pt border, so the drop highlight moves
/// nothing.
fn location_cell(row: nsh_base::ButtonElement, lit: impl Fn() -> bool + Copy + 'static) -> impl IntoElement {
    view().flex_1().min_w(px(0.0)).child(
        row.flex_1()
            .border_1()
            .border_color(move || if lit() { tok("code-accent") } else { theme().foreground.opacity(0.0) })
            .bg(move || if lit() { tok("code-accent").opacity(0.08) } else { theme().foreground.opacity(0.035) }),
    )
}
/// The found card: a green seal, "Backup found" and only the source (never the account or path).
fn backup_found(s: Store, export: impl Fn() -> bool + Copy + 'static, change: impl Fn() + 'static) -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .gap(px(13.0))
        .p(px(16.0))
        .rounded(px(12.0))
        .bg(move || theme().card)
        .border_1()
        .border_color(move || tok("separator").opacity(0.45))
        .child(
            view()
                .size(px(38.0))
                .rounded(px(9.0))
                .items_center()
                .justify_center()
                .bg(move || tok("success").opacity(0.12))
                .child(crate::icons::symbol_weight(Icon::SealFill, 22.0, 0.3, move || tok("success"))),
        )
        .child(
            view().flex_1().min_w(px(0.0)).gap(px(5.0)).child(heading(move || s.label("Backup found"), 14.0)).child(
                text(move || {
                    if export() {
                        return s.label("From an exported file");
                    }
                    match s.restore_location.get() {
                        Some(safay_app::sync::Location::Folder { .. }) | None => s.label("From a backup folder"),
                        Some(other) => s.fmt("From {}", &[&other.title()]),
                    }
                })
                .number_of_lines(1)
                .text_size(px(12.5))
                .text_color(move || theme().muted_foreground),
            ),
        )
        .child(push_button(move || s.label("Change Location"), Kind::Standard, false).disabled(move || s.busy.get()).on_press(change))
}
/// Errors on this page as a red inline notice: a failed read or unlock, or a location without a backup.
fn restore_notice(s: Store) -> impl IntoElement {
    let message = move || {
        s.error
            .get()
            .map(|e| s.label(crate::i18n::error_key(e)))
            .or_else(|| s.status.get().filter(|st| st.tone == crate::store::Tone::Error).map(|st| st.text))
    };
    when(
        move || message().is_some(),
        move || inline_notice(Icon::Warning, move || theme().destructive, move || message().unwrap_or_default()),
    )
}
pub fn check_box(checked: impl Fn() -> bool + Copy + 'static) -> impl IntoElement {
    view()
        .size(px(16.0))
        .rounded(px(if look() == Look::Windows { 3.0 } else { 4.0 }))
        .border_1()
        .border_color(move || if checked() { theme().primary } else { tok("input") })
        .bg(move || if checked() { theme().primary } else { tok("input-bg") })
        .items_center()
        .justify_center()
        .text_color(move || theme().primary_foreground)
        .child(when(checked, || crate::icons::symbol_weight(Icon::Check, 10.0, 0.4, move || theme().primary_foreground)))
}
/// A dashed drop target that also opens the file picker.
pub fn drop_zone(s: Store, label: &'static str) -> impl IntoElement {
    let over = signal(false);
    view()
        .on_file_drag_enter(move |_: &()| over.set(true))
        .on_file_drag_leave(move |_: &()| over.set(false))
        .on_file_drop(move |e: &nsh_core::element::FileDropEvent| {
            over.set(false);
            if let Some(path) = e.uris.first().and_then(|u| file_path(u)) {
                spawn(async move {
                    match crate::platform::files::read_named(path).await {
                        Ok((name, bytes)) => s.take_file(name, bytes),
                        Err(e) => s.error.set(Some(e)),
                    }
                });
            }
        })
        .child(
            button()
                .accessibility_label(move || Some(s.label("Choose File")))
                .on_press(move || s.choose_backup())
                .min_h(px(180.0))
                .gap(px(12.0))
                .items_center()
                .justify_center()
                .rounded(px(10.0))
                .border(px(1.5))
                .border_dashed()
                .border_color(move || if over.get() { tok("code-accent") } else { theme().muted_foreground.opacity(0.3) })
                .bg(move || if over.get() { tok("code-accent").opacity(0.08) } else { theme().background.opacity(0.0) })
                .hover(|st| st.bg(move || tok("hover")))
                .focus(|st| st.border_color(move || theme().ring))
                .text_color(move || theme().muted_foreground)
                .child(crate::icons::symbol_weight(Icon::Download, 30.0, -0.4, move || theme().muted_foreground))
                .child(text(move || s.label(label)).text_size(px(12.5)).text_center())
                .child(
                    view()
                        .px(px(13.0))
                        .min_h(px(28.0))
                        .justify_center()
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(move || tok("separator"))
                        .bg(move || theme().secondary)
                        .child(
                            text(move || s.label("Choose File"))
                                .text_size(px(13.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(move || theme().foreground),
                        ),
                ),
        )
}
/// `file:` URL → local path, with percent-decoding; other schemes are ignored.
pub fn file_path(uri: &str) -> Option<std::path::PathBuf> {
    // Finder drags carry file reference URLs (`file:///.file/id=…`), which name a file by its id, not its path: resolve
    // them to the path they refer to first.
    #[cfg(target_os = "macos")]
    if uri.starts_with("file:///.file/") {
        return crate::platform::files::reference_path(uri);
    }
    let rest = uri.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let mut bytes = Vec::with_capacity(rest.len());
    let raw = rest.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'%'
            && i + 2 < raw.len()
            && let Ok(b) = u8::from_str_radix(std::str::from_utf8(&raw[i + 1..i + 3]).ok()?, 16)
        {
            bytes.push(b);
            i += 3;
            continue;
        }
        bytes.push(raw[i]);
        i += 1;
    }
    let path = String::from_utf8(bytes).ok()?;
    #[cfg(windows)]
    let path = path.trim_start_matches('/').replace('/', "\\");
    Some(std::path::PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::confirmation_mismatch as mismatch;

    #[test]
    fn the_confirmation_mismatch_follows_typing_and_focus() {
        let p = "Correct-Horse-9";
        // Nothing while the new password is edited, either field is empty, or they match.
        assert!(!mismatch(p, "x", true, false));
        assert!(!mismatch("", "x", false, true));
        assert!(!mismatch(p, "", false, false));
        assert!(!mismatch(p, p, false, false));
        // Typing a prefix: nothing yet; the first wrong character says so at once.
        assert!(!mismatch(p, "Correct", false, true));
        assert!(mismatch(p, "Correcx", false, true));
        // An incomplete confirmation says so once its field is left; a longer one at once.
        assert!(mismatch(p, "Correct", false, false));
        assert!(mismatch(p, "Correct-Horse-90", false, true));
    }
}
