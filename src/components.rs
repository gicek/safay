//! Application-owned presentation over nsh_base behavior: the app's style constants, per desktop.
use crate::{
    chrome::{self, Look, look},
    icons::{Icon, icon},
    store::{Status, Store, Tone},
};
use nsh_base::{button, input, theme};
use nsh_core::{element::ImageSrc, prelude::*};

pub const CODE_FONT: nsh_core::element::FontAsset = asset!("fonts/AtkinsonHyperlegibleMono-Variable.ttf");

/// A named theme token, falling back to the border color when a theme lacks it.
pub fn tok(name: &'static str) -> AdaptiveColor {
    theme().color(name).unwrap_or(theme().border)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Standard,
    Primary,
    Danger,
    Link,
}

/// A push button: 30pt (36 large) with platform radii and press feedback.
pub fn push_button(label: impl Fn() -> SharedString + 'static, kind: Kind, large: bool) -> nsh_base::ButtonElement {
    let h = chrome::control_height(large);
    let r = chrome::control_radius(large);
    button()
        .flex_row()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .min_h(move || px(h) * font_scale())
        .min_w(px(if large && kind == Kind::Primary { 132.0 } else { 0.0 }))
        // 13pt from the edge (large 18, link 10): the padding plus the 1pt border.
        .px(px(match kind {
            Kind::Link => 9.0,
            _ if large => 17.0,
            _ => 12.0,
        }))
        .rounded(px(r))
        .border_1()
        .border_color(move || match kind {
            Kind::Standard | Kind::Danger => match look() {
                Look::Windows => tok("glass-border"),
                Look::Mac => tok("separator").opacity(0.55),
                Look::Linux => tok("separator"),
            },
            _ => theme().background.opacity(0.0),
        })
        .bg(move || match kind {
            Kind::Primary => theme().primary,
            Kind::Link => theme().background.opacity(0.0),
            _ => match look() {
                Look::Windows => tok("glass"),
                // `Color.secondary.opacity(0.09)`.
                Look::Mac => theme().muted_foreground.opacity(0.09),
                Look::Linux => theme().secondary,
            },
        })
        .text_color(move || match kind {
            Kind::Primary => theme().primary_foreground,
            Kind::Danger => theme().destructive,
            Kind::Link => tok("code-accent"),
            Kind::Standard => theme().foreground,
        })
        .hover(move |s| match kind {
            Kind::Primary => s.opacity(0.92),
            _ => s.bg(move || tok("hover")),
        })
        .active(move |s| s.scale(if large { 0.98 } else { 0.96 }).opacity(0.85))
        .focus(|s| s.border_color(move || theme().ring))
        .disabled_style(move |s| {
            match kind {
                // Disabled primary: a quiet gray fill, not a faded accent.
                Kind::Primary => s
                    .bg(move || theme().muted_foreground.opacity(if large { 0.28 } else { 0.20 }))
                    .text_color(move || theme().muted_foreground.opacity(0.72)),
                Kind::Standard | Kind::Danger if look() == Look::Mac => s
                    .bg(move || theme().muted_foreground.opacity(0.06))
                    .border_color(move || tok("separator").opacity(0.28))
                    .text_color(move || theme().muted_foreground.opacity(0.72)),
                _ => s.opacity(0.45),
            }
        })
        .child(
            text(label).text_size(px(if large { 13.5 } else { 13.0 })).line_height(px(17.0)).font_weight(if large && kind != Kind::Link {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::MEDIUM
            }),
        )
}

/// A link-style button: accent text with no padding or chrome.
pub fn link_button(label: impl Fn() -> SharedString + 'static, size: f32) -> nsh_base::ButtonElement {
    let label = std::rc::Rc::new(label);
    let name = label.clone();
    button()
        .accessibility_label(move || Some(name()))
        .active(|st| st.opacity(0.7))
        .focus(|st| st.opacity(0.8))
        .child(text(move || label()).text_size(px(size)).text_color(move || tok("code-accent")))
}
/// A 30×30 borderless glyph button with an accessible name.
pub fn icon_button(label: impl Fn() -> SharedString + 'static, glyph: Icon) -> nsh_base::ButtonElement {
    icon_button_with(label, glyph_element(glyph))
}
/// As [`icon_button`], with its own glyph element (one that changes, as [`eye_glyph`]).
pub fn icon_button_with(label: impl Fn() -> SharedString + 'static, glyph: AnyElement) -> nsh_base::ButtonElement {
    button()
        .accessibility_label(move || Some(label()))
        .size(px(30.0))
        .items_center()
        .justify_center()
        .rounded(px(chrome::control_radius(false)))
        .text_color(move || theme().muted_foreground)
        .hover(|s| s.bg(move || tok("hover")).text_color(move || theme().foreground))
        .focus(|s| s.bg(move || theme().accent).text_color(move || theme().accent_foreground))
        .active(|s| s.bg(move || tok("pressed")).scale(0.94))
        .disabled_style(|s| s.opacity(0.4))
        .child(glyph)
}
/// An icon button's glyph: on macOS a 13pt medium symbol in the primary color (icon prominence).
fn glyph_element(glyph: Icon) -> AnyElement {
    if look() == Look::Mac {
        crate::icons::symbol_weight(glyph, 13.0, 0.23, move || theme().foreground).into_element()
    } else {
        icon(glyph).into_element()
    }
}
/// The reveal eye: crossed out while the text is `shown`, as macOS's reveal buttons are.
pub fn eye_glyph(shown: impl Fn() -> bool + Copy + 'static) -> AnyElement {
    when(shown, || glyph_element(Icon::EyeSlash)).otherwise(|| glyph_element(Icon::Eye)).into_element()
}

/// The eye inside a password or secret field. Not a Tab stop: Tab goes from field to field, as with a reveal
/// button under macOS's default keyboard navigation (nsh-base's own in-field clear button is built the same way). It
/// stays pressable and is a labeled button for screen readers. Looks like [`icon_button`]. The eye is crossed out while
/// the text is `shown`, as macOS's reveal buttons are.
pub fn reveal_toggle(
    shown: impl Fn() -> bool + Copy + 'static,
    label: impl Fn() -> SharedString + 'static,
    press: impl Fn() + 'static,
) -> impl IntoElement + Styled {
    view()
        .role(nsh_core::AccessibilityRole::Button)
        .label(label)
        .focusable(false)
        .on_press(move |_: &()| press())
        .size(px(30.0))
        .items_center()
        .justify_center()
        .rounded(px(chrome::control_radius(false)))
        .text_color(move || theme().muted_foreground)
        .hover(|s| s.bg(move || tok("hover")).text_color(move || theme().foreground))
        .active(|s| s.bg(move || tok("pressed")).scale(0.94))
        .child(eye_glyph(shown))
}

/// macOS 26 toolbar glass: no outline, a hairline halo and a soft drop shadow, as the system draws toolbar items.
pub fn glass_shadow() -> Vec<BoxShadow> {
    vec![soft_shadow(0.0, 1.0, 0.10), soft_shadow(2.0, 10.0, 0.07)]
}
pub const GLASS: f32 = 38.0;
/// Toolbar capsule (macOS 26 glass button; a quiet ghost button elsewhere).
pub fn tool_button(label: impl Fn() -> SharedString + 'static, glyph: Icon) -> nsh_base::ButtonElement {
    let mac = look() == Look::Mac;
    button()
        .accessibility_label(move || Some(label()))
        .size(px(if mac { GLASS } else { 32.0 }))
        .items_center()
        .justify_center()
        .rounded(px(if mac { GLASS / 2.0 } else { chrome::control_radius(false) }))
        .bg(move || if mac { tok("glass") } else { theme().background.opacity(0.0) })
        .shadow(if mac { glass_shadow() } else { vec![] })
        .text_color(move || theme().foreground)
        .hover(|s| s.bg(move || tok("hover")))
        .focus(focus_ring)
        .active(|s| s.scale(0.95))
        .child(crate::icons::symbol_weight(glyph, if mac { 17.0 } else { 16.0 }, if mac { 0.23 } else { 0.0 }, move || {
            tok("glass-foreground")
        }))
}

pub fn heading(text_: impl Fn() -> SharedString + 'static, size: f32) -> impl IntoElement + Styled {
    text(text_).text_size(px(size)).line_height(px(size * 1.3)).font_weight(FontWeight::SEMIBOLD)
}
pub fn note(text_: impl Fn() -> SharedString + 'static) -> impl IntoElement + Styled {
    text(text_).text_size(px(11.5)).line_height(px(16.0)).text_color(move || theme().muted_foreground)
}
pub fn section_label(text_: impl Fn() -> SharedString + 'static) -> impl IntoElement + Styled {
    text(move || SharedString::from(text_().to_uppercase()))
        .text_size(px(11.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(move || theme().muted_foreground)
}
/// Shows a native menu (at a point, or at the pointer) and keeps its answer alive until it closes.
pub fn show_menu(s: Store, items: Vec<nsh_core::element::desktop::MenuItem>, at: Option<Point<Points>>) {
    let answer = match at {
        Some(at) => nsh_core::element::desktop::popup_menu_at(items, at),
        None => nsh_core::element::desktop::popup_menu(items),
    };
    s.task(async move {
        let _ = answer.await;
    });
}
/// A neutral drop shadow; box shadows take packed colors, so it is the same in both appearances.
pub fn soft_shadow(y: f32, blur: f32, alpha: f32) -> BoxShadow {
    BoxShadow {
        offset_y: px(y),
        blur: px(blur),
        color: nsh_core::Rgba { r: 0.0, g: 0.0, b: 0.0, a: alpha }.to_packed(),
        ..Default::default()
    }
}
/// The keyboard focus state: `.focus(focus_ring)` draws a 1pt ring in the theme's ring color inside the element's edge.
/// A ring is a solid shadow that takes no space, so focusing never moves or shrinks anything (a border added on focus
/// took 2pt from the width and truncated tight labels, as a segment's "Scan QR Code").
pub fn focus_ring(s: nsh_core::element::StyleRefinement) -> nsh_core::element::StyleRefinement {
    s.ring(px(1.0)).ring_inset().ring_color(move || theme().ring)
}
trait AlignIf: Sized {
    fn text_align_center_if(self, center: bool) -> Self;
}
impl<T: Styled> AlignIf for T {
    fn text_align_center_if(self, center: bool) -> Self {
        if center { self.text_center() } else { self.text_left() }
    }
}
pub fn divider() -> impl IntoElement + Styled {
    view().h(px(1.0)).bg(move || tok("separator")).opacity(0.7)
}
pub fn spacer() -> impl IntoElement + Styled {
    view().flex_1()
}

/// Input chrome: text background, hairline separator and a 1pt soft shadow.
pub fn input_chrome<E: Styled>(e: E, radius: f32) -> E {
    e.rounded(px(radius)).border_1().border_color(move || tok("input")).bg(move || tok("input-bg")).shadow([soft_shadow(1.0, 3.0, 0.035)])
}

/// `.labeled`: a 12pt medium label over a 36pt field.
pub fn labeled(label: impl Fn() -> SharedString + 'static, field: impl IntoElement) -> impl IntoElement + Styled {
    view()
        .gap(px(6.0))
        .child(text(label).text_size(px(12.0)).font_weight(FontWeight::MEDIUM).text_color(move || theme().muted_foreground))
        .child(field)
}
/// A single-line text input in the input chrome.
#[allow(clippy::too_many_arguments)]
pub fn text_field(
    s: Store,
    id: &'static str,
    label: impl Fn() -> SharedString + Copy + 'static,
    placeholder: impl Fn() -> SharedString + 'static,
    get: impl Fn() -> String + 'static,
    set: impl Fn(&str) + 'static,
    autofocus: bool,
    on_blur: impl Fn() + 'static,
) -> impl IntoElement + Styled {
    input_chrome(
        input()
            .test_id(id)
            .auto_focus(autofocus)
            .on_blur(move |_: &()| on_blur())
            .label(label)
            .placeholder(move || placeholder().to_string())
            .value(get)
            .on_change(move |v: &str| {
                s.controller.get().activity();
                set(v);
            })
            .auto_correct(false)
            .auto_fill(nsh_core::AutoFill::Off)
            .min_h(move || px(36.0) * font_scale())
            // 12pt from the edge: the padding plus the border and the field's own inset.
            .px(px(if look() == Look::Mac { 11.0 } else { 12.0 }))
            .text_size(px(13.0))
            .text_color(move || theme().foreground)
            .disabled(move || s.busy.get()),
        7.0,
    )
}
/// A password field with a show/hide button (LabeledSecureField / RevealSecretButton).
#[allow(clippy::too_many_arguments)]
pub fn secure_field(
    s: Store,
    id: &'static str,
    label: impl Fn() -> SharedString + Copy + 'static,
    get: impl Fn() -> String + 'static,
    set: impl Fn(&str) + 'static,
    on_submit: impl Fn() + 'static,
    on_blur: impl Fn() + 'static,
    autofocus: bool,
) -> impl IntoElement + Styled {
    secure_field_with(
        s,
        id,
        label,
        get,
        set,
        on_submit,
        move |focused| {
            if !focused {
                on_blur()
            }
        },
        autofocus,
        true,
    )
}
/// As [`secure_field`]; without `reveal` it is a plain secure field in labeled chrome (36pt, no eye).
#[allow(clippy::too_many_arguments)]
pub fn secure_field_with(
    s: Store,
    id: &'static str,
    label: impl Fn() -> SharedString + Copy + 'static,
    get: impl Fn() -> String + 'static,
    set: impl Fn(&str) + 'static,
    on_submit: impl Fn() + 'static,
    // Called with true when the field takes the keyboard focus, false when it loses it.
    on_focus_change: impl Fn(bool) + 'static,
    autofocus: bool,
    reveal: bool,
) -> impl IntoElement + Styled {
    let on_focus_change = std::rc::Rc::new(on_focus_change);
    let on_blur = on_focus_change.clone();
    let shown = signal(false);
    let mac = look() == Look::Mac;
    // `auto_focus` does not take while another app is in front (the system's Touch ID prompt, after "Use Password…"):
    // until the field has had the focus once, it takes it when its window gets the keyboard.
    let field = element_ref();
    let had_focus = std::rc::Rc::new(std::cell::Cell::new(false));
    if autofocus {
        let window = nsh_core::element::desktop::current_window();
        let had_focus = had_focus.clone();
        effect(move || {
            if window.state().is_some_and(|w| w.focused) && !had_focus.get() {
                untracked(|| field.focus());
            }
        });
    }
    input_chrome(
        view()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .min_h(move || {
                px(if !reveal && mac {
                    36.0
                } else if mac {
                    42.0
                } else {
                    38.0
                }) * font_scale()
            })
            .pl(px(if !reveal && mac { 11.0 } else { 12.0 }))
            .pr(px(6.0))
            .child(
                input()
                    .element_ref(&field)
                    .test_id(id)
                    .label(label)
                    .auto_focus(autofocus)
                    .masked(move || !shown.get())
                    // Shown in the monospaced font, so the text keeps the masked bullets' even rhythm.
                    .font_family(move || SharedString::from(if shown.get() { CODE_FONT.family } else { "system-ui" }))
                    .value(get)
                    .on_change(move |v: &str| {
                        s.controller.get().activity();
                        set(v);
                    })
                    .on_submit(move |_: &str| on_submit())
                    .on_focus(move |_: &()| {
                        had_focus.set(true);
                        roman_input_only(true);
                        on_focus_change(true)
                    })
                    .on_blur(move |_: &()| {
                        roman_input_only(false);
                        on_blur(false)
                    })
                    .auto_correct(false)
                    .auto_fill(nsh_core::AutoFill::Off)
                    .flex_1()
                    .min_w(px(0.0))
                    .text_size(px(if reveal { 14.0 } else { 13.0 }))
                    .text_color(move || theme().foreground)
                    .disabled(move || s.busy.get()),
            )
            .child(when(
                move || reveal,
                move || {
                    reveal_toggle(
                        move || shown.get(),
                        move || s.label(if shown.get() { "Hide" } else { "Show" }),
                        move || shown.update(|v| *v = !*v),
                    )
                },
            )),
        if reveal { 8.0 } else { 7.0 },
    )
}

/// Password and secret fields take Roman input sources only while edited (`platform::macos::roman_input_only`).
pub fn roman_input_only(enabled: bool) {
    #[cfg(target_os = "macos")]
    if crate::hosted() {
        crate::platform::macos::roman_input_only(enabled);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = enabled;
}
/// Scrolls the field being edited to its caret (`platform::macos::scroll_to_caret`).
pub fn scroll_to_caret() {
    #[cfg(target_os = "macos")]
    if crate::hosted() {
        crate::platform::macos::scroll_to_caret();
    }
}
/// Four capsules and a label: weak red, fair/good orange, strong green. Reads the store's memoized score.
pub fn strength_meter(s: Store, left: impl Fn() -> bool + Copy + 'static) -> impl IntoElement {
    let level = move || s.strength.with(|x| x.0);
    let color = move || match level() {
        1 => theme().destructive,
        2 | 3 => tok("warning"),
        4 => tok("success"),
        _ => theme().muted_foreground,
    };
    let problem = move || {
        if !left() || level() == 0 {
            None
        } else if s.form.with(|f| f.new_password.chars().count()) < 8 {
            Some("Password needs at least 8 characters")
        } else if !s.strength.with(|x| x.1) {
            Some("Too easy to crack. Use a longer password, or a few unrelated words.")
        } else {
            None
        }
    };
    view()
        .gap(px(5.0))
        .child(view().flex_row().gap(px(3.0)).children((1..=4).map(move |i| {
            view()
                .flex_1()
                .h(px(4.0))
                .rounded_full()
                .bg(move || if level() >= i { color() } else { theme().muted_foreground.opacity(0.18) })
        })))
        .child(
            text(move || {
                if let Some(p) = problem() {
                    return s.label(p);
                }
                s.label(match level() {
                    0 => "At least 8 characters",
                    1 => "Weak",
                    2 => "Fair",
                    3 => "Strong",
                    _ => "Very Strong",
                })
            })
            .text_size(px(11.0))
            .text_color(move || if problem().is_some() { theme().destructive } else { color() }),
        )
}
/// One reserved red line: present even when empty so forms never jump.
pub fn field_message(message: impl Fn() -> Option<SharedString> + 'static) -> impl IntoElement {
    text(move || message().unwrap_or_else(|| " ".into())).text_size(px(11.0)).line_height(px(14.0)).text_color(move || theme().destructive)
}
pub fn inline_notice(
    glyph: Icon,
    tint: impl Fn() -> AdaptiveColor + Copy + 'static,
    message: impl Fn() -> SharedString + 'static,
) -> impl IntoElement + Styled {
    view()
        .flex_row()
        .gap(px(8.0))
        .p(px(12.0))
        .rounded(px(10.0))
        .bg(move || tint().opacity(0.085))
        .child(view().pt(px(1.0)).text_color(tint).child(icon(glyph).size(px(16.0))))
        .child(text(message).flex_1().text_size(px(12.0)).line_height(px(17.0)).text_color(move || theme().muted_foreground))
}
pub fn status_line(status: impl Fn() -> Option<Status> + Copy + 'static) -> impl IntoElement {
    when(
        move || status().is_some(),
        move || {
            let tone = move || status().map(|s| s.tone).unwrap_or(Tone::Info);
            let tint = move || match tone() {
                Tone::Success => tok("success"),
                Tone::Error => theme().destructive,
                Tone::Info => theme().muted_foreground,
            };
            view()
                .flex_row()
                .items_center()
                .gap(px(6.0))
                .min_w(px(0.0))
                .child(view().text_color(tint).child(
                    when(move || tone() == Tone::Success, || icon(Icon::CheckCircle).size(px(14.0))).otherwise(move || {
                        when(move || tone() == Tone::Error, || icon(Icon::Warning).size(px(14.0)))
                            .otherwise(|| icon(Icon::Info).size(px(14.0)))
                    }),
                ))
                .child(
                    text(move || status().map(|s| s.text).unwrap_or_default())
                        .number_of_lines(1)
                        .text_size(px(12.0))
                        .text_color(move || if tone() == Tone::Info { theme().muted_foreground } else { tint() }),
                )
        },
    )
}

/// LogoBadge: the account's site icon when it has one, otherwise the core's monogram on its palette color, with the
/// recently-added dot. A tile icon fills the badge, cropped to its bounds; a glyph sits inset on a white plate. Both get
/// a 0.5pt outline.
pub fn badge_with_icon(
    issuer: impl Fn() -> String + Copy + 'static,
    recent: impl Fn() -> bool + Copy + 'static,
    icon: impl Fn() -> Option<std::sync::Arc<[u8]>> + Copy + 'static,
    size: f32,
) -> impl IntoElement + Styled {
    let rendered = computed(move || icon().and_then(|png| render_icon(&png)));
    view()
        .size(px(size))
        .child(when(
            move || rendered.with(|r| r.is_some()),
            move || {
                let tile = move || rendered.with(|r| r.as_ref().is_some_and(|r| r.1));
                view()
                    .size(px(size))
                    .items_center()
                    .justify_center()
                    .rounded(px(size * 0.26))
                    .overflow_hidden()
                    .bg(
                        move || {
                            if tile() { theme().background.opacity(0.0) } else { nsh_core::Rgba { r: 1.0, g: 1.0, b: 1.0, a: 1.0 }.into() }
                        },
                    )
                    .child(
                        image(move || rendered.with(|r| r.as_ref().map(|r| r.0.clone()).unwrap_or_default()))
                            .size(move || px(if tile() { size } else { size * 0.68 })),
                    )
                    .child(
                        view()
                            .absolute()
                            .inset(px(0.0))
                            .rounded(px(size * 0.26))
                            .border(px(0.5))
                            .border_color(move || theme().foreground.opacity(0.1))
                            .pointer_events(nsh_core::PointerEvents::None),
                    )
            },
        ))
        .child(when(
            move || rendered.with(|r| r.is_none()),
            move || {
                view()
                    .size(px(size))
                    .items_center()
                    .justify_center()
                    .rounded(px(size * 0.26))
                    .bg(move || {
                        let index = safay_app::core::search::palette_index(&issuer());
                        theme().color(&format!("badge-{index}")).unwrap_or(theme().primary)
                    })
                    .child(
                        text(move || SharedString::from(safay_app::core::search::monogram(&issuer())))
                            .number_of_lines(1)
                            // A box wider than the badge, centered: a fitted monogram never truncates to an ellipsis.
                            .w(px(size * 1.6))
                            .flex_shrink(0.0)
                            .text_center()
                            .text_size(move || {
                                px(if size > 38.0 { 17.0 } else { badge_font(&safay_app::core::search::monogram(&issuer()), size) })
                            })
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(move || tok("badge-foreground")),
                    )
            },
        ))
        .child(when(recent, move || {
            let d = (size * 0.22).clamp(5.5, 8.0);
            view()
                .absolute()
                .top(px(-1.0))
                .left(px(-1.0))
                // The border is drawn inside the frame: d across in all, the white ring included.
                .size(px(d))
                .rounded_full()
                .border(px(1.5))
                .border_color(move || nsh_core::Rgba { r: 1.0, g: 1.0, b: 1.0, a: 1.0 })
                .bg(move || tok("code-accent"))
        }))
}

/// CodeRing: a 2.5pt track and a stepped progress arc, red at five seconds or less.
pub fn code_ring(row: impl Fn() -> Option<(u32, u32)> + Copy + 'static, size: f32) -> impl IntoElement + Styled {
    // A 2.5pt stroke centered on the frame's circle, so it reaches 1.25pt beyond the frame. The track is `separator` at 70%; the arc is the accent (red when five seconds or fewer are left).
    let full = size + 2.5;
    let r = size / 2.0;
    let circle = move |extra: String| {
        ImageSrc::svg_markup(&format!(
            "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 {full} {full}' fill='none' stroke-width='2.5'><circle cx='{c}' cy='{c}' r='{r}' stroke='currentColor' {extra}/></svg>",
            c = full / 2.0
        ))
    };
    let track = circle(String::new());
    view()
        .size(px(size))
        .child(
            svg(move || track.clone())
                .absolute()
                .left(px(-1.25))
                .top(px(-1.25))
                .size(px(full))
                .text_color(move || tok("separator").opacity(0.7)),
        )
        .child(
            svg(move || {
                let (remaining, period) = row().unwrap_or((0, 30));
                let circumference = 2.0 * std::f32::consts::PI * r;
                let visible = circumference * (remaining as f32 / period.max(1) as f32).clamp(0.02, 1.0);
                let c = full / 2.0;
                circle(format!("stroke-linecap='round' stroke-dasharray='{visible:.3} {circumference:.3}' transform='rotate(-90 {c} {c})'"))
            })
            .absolute()
            .left(px(-1.25))
            .top(px(-1.25))
            .size(px(full))
            .text_color(move || if row().is_some_and(|(left, _)| left <= 5) { theme().destructive } else { tok("code-accent") }),
        )
}

/// A toggle switch, sized per desktop.
pub fn switch_control(
    label: impl Fn() -> SharedString + 'static,
    checked: impl Fn() -> bool + Copy + 'static,
    disabled: impl Fn() -> bool + 'static,
    change: impl Fn(bool) + 'static,
) -> impl IntoElement {
    // macOS 26: a 54×24 track and a 32×22 capsule knob.
    let mac = look() == Look::Mac;
    let (w, h) = match look() {
        Look::Mac => (54.0, 24.0),
        Look::Windows => (40.0, 20.0),
        Look::Linux => (42.0, 24.0),
    };
    nsh_base::switch()
        .accessibility_label(move || Some(label()))
        .checked(checked)
        .disabled(disabled)
        .on_change(move |next: &bool| change(*next))
        .track_class(SharedString::from(format!(
            "w-[{w}px] h-[{h}px] p-[{pad}px] rounded-full justify-center items-start bg-separator checked:items-end checked:bg-primary",
            pad = if mac { 1.0 } else { 2.0 }
        )))
        .thumb_class(SharedString::from(if mac {
            format!("w-[32px] h-[{t}px] rounded-full bg-white shadow-sm", t = h - 2.0)
        } else {
            format!("w-[{t}px] h-[{t}px] rounded-full bg-white shadow-sm", t = h - 4.0)
        }))
        .disabled_style(|st| st.opacity(0.5))
}

/// A segmented picker with keyboard-focusable segments.
pub fn segmented<T: Copy + PartialEq + 'static>(
    items: Vec<(T, Box<dyn Fn() -> SharedString>)>,
    value: impl Fn() -> T + Copy + 'static,
    change: impl Fn(T) + Copy + 'static,
) -> impl IntoElement + Styled {
    let mac = look() == Look::Mac;
    // An inactive window's selected segment is gray with normal text, as AppKit's.
    let focused = chrome::window_active();
    let values: Vec<T> = items.iter().map(|(v, _)| *v).collect();
    let values = std::rc::Rc::new(values);
    // macOS 26: a 24pt capsule, gray track, the selected segment filling its full height, hairlines between unselected ones.
    view()
        .flex_row()
        .p(px(if mac { 0.0 } else { 2.0 }))
        .gap(px(if mac { 0.0 } else { 2.0 }))
        .rounded(px(if mac { 12.0 } else { chrome::control_radius(false) + 1.0 }))
        .bg(move || if mac { tok("segment-track") } else { theme().secondary })
        .children(items.into_iter().enumerate().map(move |(i, (v, label))| {
            let label = std::rc::Rc::new(label);
            let l2 = label.clone();
            let values = values.clone();
            let divider_before = move || mac && i > 0 && value() != v && values.get(i - 1).is_some_and(|prev| value() != *prev);
            button()
                .selected(move || value() == v)
                .accessibility_label(move || Some(l2()))
                .flex_1()
                .min_h(move || px(24.0) * font_scale())
                .px(px(10.0))
                .items_center()
                .justify_center()
                .rounded(px(if mac { 12.0 } else { chrome::control_radius(false) - 1.0 }))
                .bg(move || {
                    // An active window's segmented control fills the selected segment with the accent.
                    match (value() == v, mac) {
                        (false, _) => theme().secondary.opacity(0.0),
                        (true, true) if focused() => theme().primary,
                        (true, true) => tok("segment-selected"),
                        (true, false) => tok("glass"),
                    }
                })
                .shadow(if mac { vec![] } else { vec![soft_shadow(1.0, 2.0, 0.10)] })
                .focus(focus_ring)
                .on_press(move || change(v))
                .child(when(divider_before, || {
                    view().absolute().left(px(0.0)).top(px(6.0)).bottom(px(6.0)).w(px(1.0)).bg(move || tok("segment-divider"))
                }))
                .child(
                    text(move || label())
                        .number_of_lines(1)
                        .text_size(px(if mac { 13.0 } else { 12.0 }))
                        .font_weight(if mac { FontWeight::NORMAL } else { FontWeight::MEDIUM })
                        .text_color(move || {
                            if mac && value() == v && focused() {
                                theme().primary_foreground
                            } else if mac || value() == v {
                                theme().foreground
                            } else {
                                theme().muted_foreground
                            }
                        }),
                )
        }))
}

/// A modal sheet: a header bar, padded content and a footer bar, centered over a dimmed window. The content scrolls
/// only when the window is too short for it, without bouncing (see [`overflow_scroll`]). macOS sheets and Windows
/// content dialogs share the structure; Linux centers its title like libadwaita.
#[allow(clippy::too_many_arguments)]
pub fn sheet(
    s: Store,
    width: f32,
    // The content's minimum height, its 22×18 padding included.
    min_body: f32,
    title: impl Fn() -> SharedString + 'static,
    trailing: impl IntoElement,
    body: impl IntoElement,
    footer: impl IntoElement,
    on_cancel: impl Fn() + 'static,
) -> impl IntoElement {
    sheet_frame(s, width, title, trailing, overflow_scroll().child(sheet_body(min_body, body)), footer, on_cancel)
}
/// A sheet's whole content as a scroll view that only rescues a window too short for it. It does not bounce: content
/// that fits stays still under a trackpad, as a sheet does that does not scroll, and lists inside keep their own bounce.
pub fn overflow_scroll() -> nsh_core::element::Scroll {
    scroll().flex_shrink(1.0).bounces(false)
}
/// A sheet's content padding (22×18) with its minimum height, padding included.
pub fn sheet_body(min_body: f32, body: impl IntoElement) -> impl IntoElement + Styled {
    view().min_h(px(min_body)).px(px(22.0)).py(px(18.0)).child(body)
}
/// [`sheet`] around `content` as given: a page that keeps its content still and scrolls only a part of it (the backup
/// history) brings its own padding ([`sheet_body`]) and scroll views.
pub fn sheet_frame(
    s: Store,
    width: f32,
    title: impl Fn() -> SharedString + 'static,
    trailing: impl IntoElement,
    content: impl IntoElement,
    footer: impl IntoElement,
    on_cancel: impl Fn() + 'static,
) -> impl IntoElement {
    let centered = look() == Look::Linux;
    let mac = look() == Look::Mac;
    let on_cancel = std::rc::Rc::new(on_cancel);
    let escape = on_cancel.clone();
    on_key_with("escape", KeyOptions { when: nsh_core::keymap::When::Always, ..Default::default() }, move |_| {
        if s.auth.get().is_none() {
            escape()
        }
    });
    nsh_base::dialog()
        .animation(nsh_core::ModalAnimation::Fade)
        .on_cancel(move |_: &()| {
            if s.auth.get().is_none() {
                on_cancel()
            }
        })
        .child(
            modal_backdrop().p(px(24.0)).child(
                modal_surface(view().role(nsh_core::AccessibilityRole::Alert))
                    .w(px(width))
                    .max_w(percentage(100.0))
                    .max_h(move || window().height - px(48.0))
                    .overflow_hidden()
                    .on_press(move |_: &()| s.controller.get().activity())
                    .child(
                        view()
                            .flex_row()
                            .items_center()
                            .h(px(56.0))
                            .pl(px(24.0))
                            .pr(px(16.0))
                            // The header bar reads as the sheet's own white on macOS.
                            .bg(move || if mac { tok("sheet") } else { tok("chrome") })
                            .child(
                                heading(title, if look() == Look::Windows { 17.0 } else { 15.0 })
                                    .number_of_lines(1)
                                    .flex_1()
                                    .text_align_center_if(centered),
                            )
                            .child(trailing),
                    )
                    .child(when(move || !mac, divider))
                    .child(content)
                    .child(divider())
                    .child(
                        view()
                            .flex_row()
                            .items_center()
                            .gap(px(if mac { 8.0 } else { 10.0 }))
                            .px(px(22.0))
                            .py(px(14.0))
                            .bg(move || if mac { tok("sheet") } else { tok("chrome") })
                            .child(footer),
                    ),
            ),
        )
}

/// What a modal dims the window with: black, stronger in dark mode, where a light wash would brighten it instead.
pub fn scrim() -> AdaptiveColor {
    tok("scrim")
}
/// The dimmed layer a modal centers on. It covers the title bar's drag region, so it drags the window itself: a modal
/// does not pin the window in place, as a sheet on macOS does not.
pub fn modal_backdrop() -> nsh_core::element::View {
    view().flex_1().items_center().justify_center().bg(scrim).window_drag(true)
}
/// A modal's own surface (sheets, the verify dialog, the QR code): a step lighter than the window in dark mode, with a
/// hairline rim, as macOS draws sheets, so it stands apart from the window behind it.
///
/// Presses on the surface stay with it; the backdrop around it drags the window (`modal_backdrop`).
pub fn modal_surface<E: Styled + Interactive>(e: E) -> E {
    e.window_drag(false)
        .rounded(px(chrome::sheet_radius()))
        .bg(move || tok("sheet"))
        .border_1()
        .border_color(move || tok("sheet-rim"))
        .shadow_2xl()
}

/// The window's toast: a capsule in the bottom right corner, as wide as its text, that never takes input.
pub fn toast(s: Store) -> impl IntoElement {
    when(
        move || s.toast.get().is_some(),
        move || {
            view().absolute().bottom(px(16.0)).right(px(16.0)).pointer_events(nsh_core::PointerEvents::None).child(
                view()
                    .flex_row()
                    .items_center()
                    .gap(px(10.0))
                    .max_w(px(430.0))
                    .min_h(px(44.0))
                    .px(px(15.0))
                    .rounded_full()
                    .bg(move || tok("glass"))
                    .border_1()
                    .border_color(move || tok("glass-border"))
                    .shadow_lg()
                    .child(status_line(move || s.toast.get()).into_element()),
            )
        },
    )
}

/// The app icon (`assets/icon.png`, 1024 px) at `size` points. Clipped to its frame: NSH counts an image's natural
/// size in a scroll view's extent, so an unclipped 1024 pt icon made pages scroll into empty space.
pub fn app_icon(size: f32) -> impl IntoElement + Styled {
    image(asset!("icon.png")).size(px(size)).overflow_hidden()
}

/// Safay's glass card on the desktop: the selection's instructions and the desktop notice. The app icon (with the
/// result's symbol on it, when there is one), a title and an optional line under it; callers add trailing content. 56 pt
/// tall: 10 pt around the 36 pt icon, whose 8 pt corners are concentric with the card's 18 pt ones.
pub const CARD_HEIGHT: f32 = 56.0;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn desktop_card(
    tone: Option<Tone>,
    title: impl Fn() -> SharedString + 'static,
    detail: impl Fn() -> Option<SharedString> + Copy + 'static,
) -> View {
    const PADDING: f32 = 10.0;
    const ICON: f32 = 36.0;
    const ICON_RADIUS: f32 = 8.0;
    let mark = tone.map(|tone| {
        let tint = move || match tone {
            Tone::Success => tok("success"),
            Tone::Error => theme().destructive,
            Tone::Info => tok("code-accent"),
        };
        let glyph = match tone {
            Tone::Success => Icon::CheckCircleFill,
            Tone::Error => Icon::WarningFill,
            Tone::Info => Icon::InfoFill,
        };
        view()
            .absolute()
            .right(px(-4.0))
            .bottom(px(-4.0))
            .size(px(18.0))
            .rounded_full()
            .items_center()
            .justify_center()
            .bg(move || tok("glass"))
            .child(crate::icons::symbol_weight(glyph, 14.0, 0.3, tint))
    });
    view()
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .h(px(CARD_HEIGHT))
        .pl(px(PADDING))
        .pr(px(18.0))
        .rounded(px(ICON_RADIUS + PADDING))
        .font_family("system-ui")
        .bg(move || tok("glass"))
        .border(px(0.5))
        .border_color(move || tok("glass-border"))
        .child(view().size(px(ICON)).child(app_icon(ICON).rounded(px(ICON_RADIUS))).children(mark))
        .child(
            view()
                .flex_shrink(1.0)
                .min_w(px(0.0))
                .gap(px(2.0))
                .child(
                    text(title)
                        .number_of_lines(1)
                        .text_size(px(14.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(move || theme().foreground),
                )
                .child(when(
                    move || detail().is_some(),
                    move || {
                        text(move || detail().unwrap_or_default())
                            .number_of_lines(1)
                            .text_size(px(12.0))
                            .text_color(move || theme().muted_foreground)
                    },
                )),
        )
}

/// The monogram: 12pt semibold, scaled down (to at most 0.58×) to fit the badge. Width is estimated from the semibold system font's average capital advance (0.68 em).
fn badge_font(monogram: &str, size: f32) -> f32 {
    let n = monogram.chars().count().max(1) as f32;
    let fit = (size - 2.0) / (n * 0.68);
    fit.clamp(12.0 * 0.58, 12.0)
}

/// A stored icon as a `data:` PNG for the badge and whether it is a tile (the core's rule on its alpha channel); tiles
/// are cropped to their bounds. Cached per icon so a badge decodes it once.
fn render_icon(png: &[u8]) -> Option<(String, bool)> {
    use base64::Engine;
    use std::hash::{Hash, Hasher};
    thread_local! {
        static CACHE: std::cell::RefCell<std::collections::HashMap<u64, Option<(String, bool)>>> = Default::default();
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    png.hash(&mut hasher);
    let key = hasher.finish();
    if let Some(hit) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return hit;
    }
    let result = (|| {
        let image = image::load_from_memory_with_format(png, image::ImageFormat::Png).ok()?.to_rgba8();
        let (w, h) = image.dimensions();
        let alpha: Vec<u8> = image.pixels().map(|p| p.0[3]).collect();
        let (out, tile) = match safay_app::core::site_icon::tile_bounds(&alpha, w, h) {
            Some(b) => {
                let cropped = image::imageops::crop_imm(&image, b.x, b.y, b.width, b.height).to_image();
                let mut buffer = std::io::Cursor::new(Vec::new());
                cropped.write_to(&mut buffer, image::ImageFormat::Png).ok()?;
                (buffer.into_inner(), true)
            }
            None => (png.to_vec(), false),
        };
        Some((format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(out)), tile))
    })();
    CACHE.with(|c| c.borrow_mut().insert(key, result.clone()));
    result
}
/// A small indeterminate spinner; its interval belongs to the
/// view and stops with it.
pub fn spinner(size: f32) -> impl IntoElement {
    let angle = signal(0.0_f32);
    set_interval(std::time::Duration::from_millis(80), move || {
        angle.update(|a| *a = (*a + std::f32::consts::PI / 6.0) % std::f32::consts::TAU)
    });
    view().size(px(size)).rotate(move || angle.get()).child(
        svg(nsh_core::element::ImageSrc::svg_markup(
            "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='3' stroke-linecap='round'><path d='M12 3a9 9 0 1 1-9 9' /></svg>",
        ))
        .size(px(size))
        .text_color(move || theme().muted_foreground),
    )
}
