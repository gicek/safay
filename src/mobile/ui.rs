//! The styled controls of the phone and tablet interface: one set of metrics (touch targets of 48 pt and more, 56 pt
//! fields and primary buttons, grouped rows with 24 pt outer and 6 pt inner corners) drawn with the tokens of
//! `theme-mobile.json`. Pages compose these and never style a control themselves.
//!
//! On iOS ([`IOS`]) the same controls take the system's own look, drawn with `theme-ios.json`: 44 pt targets, inset
//! grouped rows divided by hairlines, square colored tiles, the system's switch and segmented control, glass buttons in
//! the bars, menus with their glyphs at the end, and the system's type sizes.
use crate::{
    components::{CODE_FONT, tok},
    icons::{Icon, icon},
    store::Store,
};
use nsh_base::{button, input, theme};
use nsh_core::prelude::*;

/// The interface takes iOS's look (`theme-ios.json` and the metrics below); the `ios-look` feature shows it on a
/// desktop's development build.
pub const IOS: bool = cfg!(any(target_os = "ios", feature = "ios-look"));
/// Outer corner of a group of rows, and of a card that stands alone.
pub const GROUP_RADIUS: f32 = if IOS { 22.0 } else { 24.0 };
/// Corner between two rows of a group (iOS: none, a hairline divides them).
pub const GROUP_INNER: f32 = if IOS { 0.0 } else { 6.0 };
/// Gap between the rows of a group.
pub const GROUP_GAP: f32 = if IOS { 0.0 } else { 2.0 };
/// The page's side margin.
pub const MARGIN: f32 = 16.0;
/// Fields, primary buttons and the search bar.
pub const CONTROL: f32 = if IOS { 52.0 } else { 56.0 };
/// The smallest touch target, and a menu's rows.
pub const TOUCH: f32 = if IOS { 44.0 } else { 48.0 };
/// A page's top bar.
pub const BAR: f32 = if IOS { 52.0 } else { 56.0 };
/// The round button floating at a page's bottom corner.
pub const FAB: f32 = if IOS { 52.0 } else { 56.0 };
/// Room under a list for the floating button over its end: the button, its margins and some air.
pub const FAB_CLEARANCE: f32 = 120.0;
/// A list's rows: an account, a category. The account row's own height (its names, 14 pt above and below), so the list's
/// estimate is its measure: a row measured taller than estimated moves the rows after it as the list scrolls.
pub const ROW: f32 = if IOS { 70.0 } else { 72.0 };
/// Where a hairline between two rows of a group starts on iOS: under the text, past a row's leading tile.
pub const TILE_INSET: f32 = 16.0 + 30.0 + 14.0;
/// From this window width on (a tablet, a phone's window made wide), the list shows its categories in a sidebar beside
/// the accounts, which take columns of cards; pages over it keep a readable width ([`PAGE_MAX`]).
pub const WIDE: f32 = 700.0;
/// The sidebar of a wide window.
pub const SIDEBAR: f32 = 270.0;
/// The widest a page over the list grows in a wide window: its content stays centered.
pub const PAGE_MAX: f32 = 720.0;
/// `content` in a column of a readable width ([`PAGE_MAX`]), centered in a wide window. Pages put it inside their scroll
/// view, which spans the window: its indicator stays at the window's edge, and on iOS the system's header, which takes
/// its background from that scroll view, spans it too.
pub fn readable(content: impl IntoElement) -> View {
    view().w_full().items_center().child(view().w_full().max_w(px(PAGE_MAX)).child(content))
}
/// The window is wide ([`WIDE`]).
pub fn is_wide() -> bool {
    window().width >= px(WIDE)
}
/// What a wide window's account pane is drawn on: iOS's plain background (cards stand out from it in the grouped gray),
/// elsewhere the page's.
pub fn pane() -> AdaptiveColor {
    if IOS { AdaptiveColor::rgb(0xffffff, 0x000000) } else { theme().background }
}
/// An account card in a wide window, on [`pane`].
pub fn cell() -> AdaptiveColor {
    if IOS { AdaptiveColor::rgb(0xf2f2f7, 0x1c1c1e) } else { theme().card }
}
/// A wide window's sidebar.
pub fn sidebar() -> AdaptiveColor {
    if IOS { AdaptiveColor::rgb(0xf2f2f7, 0x1c1c1e) } else { theme().card }
}
/// Color and opacity changes.
pub const EFFECTS_MS: f32 = 160.0;
/// Shape and position changes.
pub const SPATIAL_MS: f32 = 280.0;

/// The device's biometric as the interface names it: Face ID or Touch ID on iOS, "biometrics" elsewhere.
// Each system builds only the variants it has.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Biometric {
    Face,
    Touch,
    Other,
}
impl Biometric {
    pub fn get() -> Self {
        #[cfg(target_os = "ios")]
        {
            static FACE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
            if *FACE.get_or_init(crate::platform::biometric::face) { Biometric::Face } else { Biometric::Touch }
        }
        #[cfg(not(target_os = "ios"))]
        Biometric::Other
    }
    /// The glyph of its button and its settings row.
    pub fn icon(self) -> Icon {
        if self == Biometric::Face { Icon::FaceId } else { Icon::TouchId }
    }
    /// "Unlock with …": the settings row and the lock screen's button.
    pub fn unlock(self) -> &'static str {
        match self {
            Biometric::Face => "Unlock with Face ID",
            Biometric::Touch => "Unlock with Touch ID",
            Biometric::Other => "Unlock with biometrics",
        }
    }
    /// What the lock screen says unlocks it.
    pub fn prompt(self) -> &'static str {
        match self {
            Biometric::Face => "Use your master password or Face ID to unlock",
            Biometric::Touch => "Use your master password or Touch ID to unlock",
            Biometric::Other => "Use your master password or fingerprint to unlock",
        }
    }
}

/// The state screen readers announce for a checked control: a switch, a radio, a checkbox. A button has no checked
/// state, so those are views (`toggle`).
fn checked_state(checked: bool) -> AccessibilityState {
    AccessibilityState { checked: Some(if checked { nsh_core::Checked::True } else { nsh_core::Checked::False }), ..Default::default() }
}
/// A press target that is on or off for screen readers: `role` (a switch, a radio, a checkbox), `name` and whether it
/// is on. The page lays it out and gives it its press.
pub fn toggle(role: AccessibilityRole, name: impl Fn() -> SharedString + 'static, on: impl Fn() -> bool + Copy + 'static) -> View {
    view().accessible(true).role(role).label(name).accessibility_state(move || checked_state(on()))
}

// ---- Type scale ----------------------------------------------------------------------------------------------------

// iOS: the system's sizes (Large Title, Title 1, Headline, Body, Subheadline, Footnote).
pub fn display<E: Styled>(e: E) -> E {
    if IOS {
        return e.text_size(px(34.0)).line_height(px(41.0)).font_weight(FontWeight::BOLD);
    }
    e.text_size(px(34.0)).line_height(px(42.0)).font_weight(FontWeight::BOLD)
}
pub fn headline<E: Styled>(e: E) -> E {
    if IOS {
        return e.text_size(px(28.0)).line_height(px(34.0)).font_weight(FontWeight::BOLD);
    }
    e.text_size(px(26.0)).line_height(px(34.0)).font_weight(FontWeight::SEMIBOLD)
}
pub fn title_large<E: Styled>(e: E) -> E {
    if IOS {
        return e.text_size(px(17.0)).line_height(px(22.0)).font_weight(FontWeight::SEMIBOLD);
    }
    e.text_size(px(22.0)).line_height(px(28.0))
}
pub fn title<E: Styled>(e: E) -> E {
    if IOS {
        return e.text_size(px(17.0)).line_height(px(22.0)).font_weight(FontWeight::SEMIBOLD);
    }
    e.text_size(px(16.0)).line_height(px(24.0)).font_weight(FontWeight::MEDIUM)
}
pub fn body_large<E: Styled>(e: E) -> E {
    if IOS {
        return e.text_size(px(17.0)).line_height(px(22.0));
    }
    e.text_size(px(16.0)).line_height(px(24.0))
}
pub fn body<E: Styled>(e: E) -> E {
    if IOS {
        return e.text_size(px(15.0)).line_height(px(20.0));
    }
    e.text_size(px(14.0)).line_height(px(20.0))
}
pub fn caption<E: Styled>(e: E) -> E {
    if IOS {
        return e.text_size(px(13.0)).line_height(px(18.0));
    }
    e.text_size(px(12.0)).line_height(px(16.0))
}
pub fn label<E: Styled>(e: E) -> E {
    if IOS {
        return e.text_size(px(15.0)).line_height(px(20.0)).font_weight(FontWeight::SEMIBOLD);
    }
    e.text_size(px(14.0)).line_height(px(20.0)).font_weight(FontWeight::MEDIUM)
}
/// The text of an alert under its title: secondary, centered on iOS as the system's.
pub fn message<E: Styled>(e: E) -> E {
    let e = dim(e);
    if IOS { e.text_center() } else { e }
}
/// Secondary text.
pub fn dim<E: Styled>(e: E) -> E {
    e.text_color(move || theme().muted_foreground)
}

// ---- Surfaces ------------------------------------------------------------------------------------------------------

/// The corners of row `index` among `count` rows of a group.
/// iOS: a hairline over every row but the first, from 16 pt in ([`group_corners_inset`] for another start).
pub fn group_corners<E: Styled + ParentElement>(
    e: E,
    index: impl Fn() -> usize + Copy + 'static,
    count: impl Fn() -> usize + Copy + 'static,
) -> E {
    group_corners_inset(e, index, count, 16.0)
}
/// As [`group_corners`], the hairline starting `inset` points in: under the text of a row with a leading mark.
pub fn group_corners_inset<E: Styled + ParentElement>(
    e: E,
    index: impl Fn() -> usize + Copy + 'static,
    count: impl Fn() -> usize + Copy + 'static,
    inset: f32,
) -> E {
    e.rounded_t(move || px(if index() == 0 { GROUP_RADIUS } else { GROUP_INNER }))
        .rounded_b(move || px(if index() + 1 >= count() { GROUP_RADIUS } else { GROUP_INNER }))
        .child(when(move || IOS && index() > 0, move || separator(inset)))
}
/// The hairline between two rows of a group on iOS, from `inset` points to the row's end. It lies over the top of the
/// row below it and takes no room.
pub fn separator(inset: f32) -> View {
    view()
        .absolute()
        .top(px(0.0))
        .left(px(inset))
        .right(px(0.0))
        .h(px(0.5))
        .bg(move || tok("separator"))
        .pointer_events(nsh_core::PointerEvents::None)
}
/// A card of a form: a field group, a switch row.
pub fn form_card() -> View {
    view().rounded(px(if IOS { GROUP_RADIUS } else { 28.0 })).bg(move || theme().card).p(px(16.0))
}

// ---- Buttons -------------------------------------------------------------------------------------------------------

/// The page's primary action: full width, 56 pt, a pill. `busy` replaces the label with a spinner.
pub fn primary_button(
    label_: impl Fn() -> SharedString + Copy + 'static,
    enabled: impl Fn() -> bool + Copy + 'static,
    busy: impl Fn() -> bool + Copy + 'static,
) -> nsh_base::ButtonElement {
    button()
        .accessibility_label(move || Some(label_()))
        .h(move || px(CONTROL) * font_scale())
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(move || if enabled() { theme().primary } else { theme().foreground.opacity(0.12) })
        .transition()
        .duration(EFFECTS_MS)
        .ease_out()
        .active(|s| s.opacity(0.88))
        .disabled(move || !enabled() || busy())
        // The spinner takes the label's color, so it reads on the filled button.
        .child(
            when(busy, move || {
                crate::components::spinner_in(
                    20.0,
                    move || {
                        if enabled() { theme().primary_foreground } else { theme().foreground.opacity(0.38) }
                    },
                )
            })
            .otherwise(move || {
                label(text(label_))
                    .text_size(px(if IOS { 17.0 } else { 16.0 }))
                    .text_color(move || if enabled() { theme().primary_foreground } else { theme().foreground.opacity(0.38) })
            }),
        )
}
/// A secondary action beside or under a primary one: 48 pt, outlined (iOS: a tinted capsule, as the system's gray
/// button).
pub fn outlined_button(label_: impl Fn() -> SharedString + Copy + 'static) -> nsh_base::ButtonElement {
    if IOS {
        return button()
            .accessibility_label(move || Some(label_()))
            .h(move || px(TOUCH) * font_scale())
            .px(px(20.0))
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(move || theme().primary.opacity(0.12))
            .active(|s| s.opacity(0.7))
            .child(label(text(label_)).text_color(move || theme().primary));
    }
    button()
        .accessibility_label(move || Some(label_()))
        .h(move || px(48.0) * font_scale())
        .px(px(24.0))
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(move || tok("outline").opacity(0.6))
        .active(|s| s.bg(move || tok("pressed")))
        .child(label(text(label_)).text_color(move || theme().primary))
}
/// A filled action of its text's width, with an optional leading icon: 48 pt.
pub fn filled_button(glyph: Option<Icon>, label_: impl Fn() -> SharedString + Copy + 'static) -> nsh_base::ButtonElement {
    button()
        .accessibility_label(move || Some(label_()))
        .h(move || px(if IOS { 50.0 } else { 48.0 }) * font_scale())
        .px(px(24.0))
        .flex_row()
        .items_center()
        .justify_center()
        .gap(px(8.0))
        .rounded_full()
        .bg(move || theme().primary)
        .text_color(move || theme().primary_foreground)
        .active(|s| s.opacity(0.88))
        .child(when(move || glyph.is_some(), move || icon(glyph.unwrap_or(Icon::Plus)).size(px(18.0))))
        .child(label(text(label_)))
}
/// An action in a bar or at a field's end: a 48 pt target around a 24 pt glyph.
pub fn icon_button(name: impl Fn() -> SharedString + 'static, glyph: impl Fn() -> Icon + Copy + 'static) -> nsh_base::ButtonElement {
    glyph_button(name, move || icon(glyph()).size(px(24.0)).into_element())
}
/// An action of a page's top bar: as `icon_button`, its glyph drawn heavier, as the bar's back chevron is.
/// On iOS a glass circle, as the system's bar buttons are.
pub fn bar_button(name: impl Fn() -> SharedString + 'static, glyph: impl Fn() -> Icon + Copy + 'static) -> nsh_base::ButtonElement {
    let b =
        glyph_button(name, move || crate::icons::icon_bold(glyph()).size(px(24.0)).into_element()).text_color(move || theme().foreground);
    if IOS { glass(b.size(px(44.0))) } else { b }
}
/// The look of a control floating over the page on iOS: a translucent fill, a faint rim and a soft shadow.
pub fn glass<E: Styled>(e: E) -> E {
    // A device's build: the system's Liquid Glass (iOS 26; the system's blur before it), its own edge and shadow.
    #[cfg(target_os = "ios")]
    return e.rounded_full().glass_effect(nsh_core::GlassEffect::Regular);
    // A desktop showing iOS's look: a drawing of it.
    #[allow(unreachable_code)]
    e.rounded_full().bg(move || tok("glass")).border_1().border_color(move || tok("glass-border")).shadow_md()
}
/// A sheet's action in words on iOS ("Done", "Edit"): a capsule of Liquid Glass (`glass`), its label in the text's
/// color, as the system's sheets draw theirs.
pub fn glass_text_button(label_: impl Fn() -> SharedString + Copy + 'static) -> nsh_base::ButtonElement {
    glass(
        button()
            .accessibility_label(move || Some(label_()))
            .h(px(TOUCH))
            .min_w(px(72.0))
            .px(px(20.0))
            .items_center()
            .justify_center()
            .active(|st| st.scale(0.94)),
    )
    .child(body_large(text(label_)).font_weight(FontWeight::SEMIBOLD).number_of_lines(1).text_color(move || theme().foreground))
}
/// What floats over a page, the search field and its button: glass on iOS (`glass`); on Android the page's card color,
/// set off by a rim and a shadow (the button is the filled `fab`).
pub fn floating<E: Styled>(e: E) -> E {
    if IOS {
        return glass(e);
    }
    e.rounded_full().bg(move || theme().card).border_1().border_color(move || tok("float-rim")).shadow_xl()
}
fn glyph_button(name: impl Fn() -> SharedString + 'static, glyph: impl Fn() -> AnyElement + 'static) -> nsh_base::ButtonElement {
    button()
        .accessibility_label(move || Some(name()))
        .size(px(48.0))
        .items_center()
        .justify_center()
        .rounded_full()
        .text_color(move || theme().muted_foreground)
        .active(|s| s.bg(move || tok("pressed")))
        .child(view().size(px(24.0)).child(dynamic(glyph)))
}
/// A text input in the theme's colors, in light and dark alike: the placeholder dimmed as other secondary text, the
/// caret and the selection in the primary color (the platform's own are its accent, and its placeholder can all but
/// vanish on a dark background).
pub fn themed_input(i: Input) -> Input {
    i.placeholder_color(move || color(theme().muted_foreground))
        .caret_color(move || color(theme().primary))
        .selection_color(move || color(theme().primary.opacity(0.3)))
}
/// The look of a menu over the page: the popover color and corners, a faint outline that draws its edge on any
/// background (light in dark appearance, where a shadow does not show), and a deep shadow.
pub fn popover<E: Styled>(e: E) -> E {
    if IOS {
        return e.rounded(px(22.0)).py(px(6.0)).bg(move || tok("popover")).border_1().border_color(move || tok("elevated-rim")).shadow_xl();
    }
    e.rounded(px(20.0)).py(px(8.0)).bg(move || tok("popover")).border_1().border_color(move || tok("elevated-rim")).shadow_xl()
}

/// A menu's row: its icon and its name, in the destructive color for an action that deletes. iOS: the name first and
/// the glyph at the end, both in the text's color.
pub fn menu_item(glyph: Icon, name: impl Fn() -> SharedString + 'static, destructive: bool) -> nsh_base::ButtonElement {
    let name = std::rc::Rc::new(name);
    let said = name.clone();
    if IOS {
        let ink = move || if destructive { theme().destructive } else { theme().foreground };
        return button()
            .accessibility_label(move || Some(said()))
            .min_h(move || px(TOUCH) * font_scale())
            .px(px(16.0))
            .flex_row()
            .items_center()
            .gap(px(12.0))
            .active(|st| st.bg(move || tok("pressed")))
            .child(body_large(text(move || name())).number_of_lines(1).flex_1().text_color(ink))
            .child(view().text_color(ink).child(icon(glyph).size(px(22.0))));
    }
    button()
        .accessibility_label(move || Some(said()))
        .min_h(move || px(TOUCH) * font_scale())
        .px(px(12.0))
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .active(|st| st.bg(move || tok("pressed")))
        .child(
            view()
                .text_color(move || if destructive { theme().destructive } else { theme().muted_foreground })
                .child(icon(glyph).size(px(24.0))),
        )
        .child(
            label(text(move || name()))
                .number_of_lines(1)
                .flex_1()
                .text_color(move || if destructive { theme().destructive } else { theme().foreground }),
        )
}

/// The circle of a choice among several: filled with the primary color when chosen. The row around it is the
/// `toggle` screen readers name.
pub fn radio_mark(on: impl Fn() -> bool + Copy + 'static) -> View {
    // iOS: a checkmark by the chosen one, nothing by the others.
    if IOS {
        return view()
            .size(px(22.0))
            .text_color(move || theme().primary)
            .child(when(on, || crate::icons::icon_bold(Icon::Check).size(px(22.0))));
    }
    view()
        .size(px(20.0))
        .rounded_full()
        .items_center()
        .justify_center()
        .border(px(2.0))
        .border_color(move || if on() { theme().primary } else { theme().muted_foreground })
        .child(when(on, || view().size(px(10.0)).rounded_full().bg(move || theme().primary)))
}

/// The round button floating at a page's bottom corner, in the primary color: the page places it and gives it its icon.
pub fn fab(name: impl Fn() -> SharedString + 'static) -> nsh_base::ButtonElement {
    // iOS: a glass circle in the text's color, as the system's floating search button.
    if IOS {
        return glass(
            button()
                .accessibility_label(move || Some(name()))
                .size(px(FAB))
                .items_center()
                .justify_center()
                .text_color(move || theme().foreground)
                .active(|st| st.scale(0.94)),
        );
    }
    button()
        .accessibility_label(move || Some(name()))
        .size(px(FAB))
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(move || theme().primary)
        .text_color(move || theme().primary_foreground)
        .shadow_xl()
        .active(|st| st.scale(0.96))
}

/// A row picked up to drag, over the others: the card's color, raised by a deep shadow, and a rim that draws its edge in
/// dark appearance, where the shadow does not show.
pub fn lifted<E: Styled>(e: E) -> E {
    // iOS: as round as a lifted account (whose list draws no corner between rows).
    e.rounded(px(if IOS { 14.0 } else { GROUP_INNER * 2.0 }))
        .bg(move || theme().card)
        .border_1()
        .border_color(move || tok("elevated-rim"))
        .shadow_xl()
}

// ---- Fields --------------------------------------------------------------------------------------------------------

/// How a field is typed into.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    Text,
    /// Masked, with a show and hide control; Roman characters only.
    Password,
    /// A setup key, in the code font. Always: an input whose font changes with its first character drops the
    /// characters typed while the system's text field is set up again (the second one, typed quickly).
    Key,
}
/// The outline of a text field, as Android draws one: 16 pt corners, the primary color and 2 pt with the focus, the
/// error color while `error` holds. A press anywhere in it focuses `me`. It keeps room above itself for the half of its
/// floated label that stands out over the outline, so whatever is above stays clear of the label.
fn outlined(focused: Signal<bool>, error: impl Fn() -> bool + Copy + 'static, me: ElementRef) -> View {
    // iOS, in a form: a row of its group, a hairline over every row but the first.
    if let Some(rows) = FORM.get().filter(|_| IOS) {
        FORM.set(Some(rows + 1));
        return view().bg(move || theme().card).on_press(move |_: &()| me.focus()).child(when(move || rows > 0, || separator(16.0)));
    }
    // iOS: a filled box, its label inside (`floating_label`); the focus and a problem draw its edge.
    if IOS {
        return view()
            .rounded(px(12.0))
            .bg(move || tok("field"))
            .border(px(1.0))
            .border_color(move || {
                if error() {
                    theme().destructive
                } else if focused.get() {
                    theme().primary
                } else {
                    tok("field").opacity(0.0)
                }
            })
            .on_press(move |_: &()| me.focus());
    }
    let on_dialog = compact_now();
    view()
        .mt(move || px(8.0) * font_scale())
        .rounded(px(16.0))
        .bg(move || field_surface(on_dialog))
        .border(move || px(outline_width(focused, error)))
        .border_color(move || {
            if error() {
                theme().destructive
            } else if focused.get() {
                theme().primary
            } else {
                tok("outline").opacity(0.55)
            }
        })
        .on_press(move |_: &()| me.focus())
}
/// What an outlined field is filled with: the surface it stands on (a dialog's, or the page's cards'), so its floated
/// label's band, which is the same, breaks the outline without a patch of another color.
fn field_surface(on_dialog: bool) -> AdaptiveColor {
    if on_dialog { tok("popover") } else { theme().card }
}
fn outline_width(focused: Signal<bool>, error: impl Fn() -> bool) -> f32 {
    if focused.get() || error() { 2.0 } else { 1.0 }
}
/// A field's label, over the text where it starts (4 pt of room on each side of it, where it breaks the outline).
/// `rest` is the top of its 24 pt line while it stands in the field, in points from inside the outline at the default
/// text size. Floated (`floated`), it shrinks to a 16 pt line centered on the outline, which a band of the field's own
/// color breaks under it; above the outline it stands on whatever the field is on.
fn floating_label(
    label_: impl Fn() -> SharedString + 'static,
    floated: impl Fn() -> bool + Copy + 'static,
    focused: Signal<bool>,
    error: impl Fn() -> bool + Copy + 'static,
    rest: impl Fn(f32) -> f32 + 'static,
) -> View {
    let outline = move || outline_width(focused, error);
    // iOS: inside the box, shrinking to its top-left corner; the text below it moves down to make room
    // (`FLOATED_TEXT`).
    if IOS {
        return view()
            .absolute()
            .left(px(16.0))
            .right(px(16.0))
            .top(move || px(if floated() { 7.0 * font_scale() } else { rest(1.0) }))
            .pointer_events(nsh_core::PointerEvents::None)
            .transition()
            .duration(EFFECTS_MS)
            .ease_out()
            .child(
                text(label_)
                    .number_of_lines(1)
                    .text_size(move || px(if floated() { 12.0 } else { 17.0 }))
                    .line_height(move || px(if floated() { 16.0 } else { 22.0 }))
                    .text_color(move || if error() { theme().destructive } else { theme().muted_foreground })
                    .transition()
                    .duration(EFFECTS_MS)
                    .ease_out(),
            );
    }
    let on_dialog = compact_now();
    view()
        .absolute()
        .left(px(12.0))
        .px(px(4.0))
        .top(move || px(if floated() { -8.0 * font_scale() - outline() / 2.0 } else { rest(outline()) }))
        .pointer_events(nsh_core::PointerEvents::None)
        .transition()
        .duration(EFFECTS_MS)
        .ease_out()
        .child(
            view()
                .absolute()
                .left(px(0.0))
                .right(px(0.0))
                .top(move || px(8.0 * font_scale() - outline() / 2.0 - 0.5))
                .h(move || px(outline() + 1.0))
                .bg(move || field_surface(on_dialog))
                .opacity(move || if floated() { 1.0_f32 } else { 0.0 }),
        )
        .child(
            text(label_)
                .number_of_lines(1)
                .text_size(move || px(if floated() { 12.0 } else { 16.0 }))
                .line_height(move || px(if floated() { 16.0 } else { 24.0 }))
                .text_color(move || {
                    if error() {
                        theme().destructive
                    } else if focused.get() {
                        theme().primary
                    } else {
                        theme().muted_foreground
                    }
                })
                .transition()
                .duration(EFFECTS_MS)
                .ease_out(),
        )
}

thread_local! {
    /// Fields and notes are being built for an alert (`compact`).
    static COMPACT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Fields are being built as the rows of an iOS form (`form`): how many there are so far.
    static FORM: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}
/// Fields on a page, as each platform lays out a form. iOS: the rows of one inset group (a card, hairlines between
/// them), each field a row of it. Android: Material's outlined fields straight on the page, no card around them.
pub fn form<E: IntoElement>(content: impl FnOnce() -> E) -> View {
    if !IOS {
        return view().gap(px(12.0)).child(content());
    }
    let was = FORM.replace(Some(0));
    let built = content();
    FORM.set(was);
    view().rounded(px(GROUP_RADIUS)).overflow_hidden().bg(move || theme().card).child(built)
}
/// Whether fields are being built as iOS form rows (`form`).
pub fn in_form() -> bool {
    IOS && FORM.get().is_some()
}
/// Builds `content` for an alert: a field's note takes room only while it says something, and on iOS its fields are the
/// system alert's (44 pt, their label as the placeholder, no control of their own).
pub fn compact<R>(content: impl FnOnce() -> R) -> R {
    let was = COMPACT.replace(true);
    let built = content();
    COMPACT.set(was);
    built
}
fn compact_now() -> bool {
    COMPACT.get()
}
/// The one text field: 56 pt and outlined. Empty and without the focus, its label stands in the middle as a
/// placeholder; with the focus or a text, the label shrinks onto the outline (`floating_label`).
#[allow(clippy::too_many_arguments)]
pub fn field(
    s: Store,
    id: &'static str,
    entry: Entry,
    label_: impl Fn() -> SharedString + Copy + 'static,
    get: impl Fn() -> String + Copy + 'static,
    set: impl Fn(&str) + 'static,
    on_submit: impl Fn() + 'static,
    on_focus_change: impl Fn(bool) + 'static,
    error: impl Fn() -> bool + Copy + 'static,
    return_key: ReturnKeyType,
    reference: Option<ElementRef>,
) -> impl IntoElement + Styled {
    if IOS && compact_now() {
        return alert_field(s, id, entry, label_, get, set, on_submit, on_focus_change, error, return_key, reference);
    }
    let focused = signal(false);
    let shown = signal(entry != Entry::Password);
    let floated = move || focused.get() || !get().is_empty();
    let on_focus_change = std::rc::Rc::new(on_focus_change);
    let on_blur = on_focus_change.clone();
    let me = reference.unwrap_or_else(element_ref);
    outlined(focused, error, me)
        .flex_row()
        .items_center()
        .min_h(move || px(field_height()) * font_scale())
        .pl(px(16.0))
        .pr(px(if entry == Entry::Password { 4.0 } else { 16.0 }))
        .child(
            view().flex_1().min_w(px(0.0)).justify_center().pt(move || px(if IOS && floated() { FLOATED_TEXT } else { 0.0 })).child(
                themed_input(input())
                    .element_ref(&me)
                    .test_id(id)
                    .label(label_)
                    .masked(move || !shown.get())
                    .font_family(move || {
                        let code = match entry {
                            Entry::Text => false,
                            Entry::Password => shown.get(),
                            Entry::Key => true,
                        };
                        SharedString::from(if code { CODE_FONT.family } else { "system-ui" })
                    })
                    .value(get)
                    .on_change(move |v: &str| {
                        s.controller.get().activity();
                        set(v);
                    })
                    .on_submit(move |_: &str| on_submit())
                    .on_focus(move |_: &()| {
                        focused.set(true);
                        on_focus_change(true)
                    })
                    .on_blur(move |_: &()| {
                        focused.set(false);
                        on_blur(false)
                    })
                    .return_key_type(return_key)
                    .auto_correct(false)
                    .auto_capitalize(nsh_core::AutoCapitalize::None)
                    .auto_fill(nsh_core::AutoFill::Off)
                    .text_size(px(if IOS { 17.0 } else { 16.0 }))
                    .text_color(move || theme().foreground)
                    .disabled(move || s.busy.get()),
            ),
        )
        .child(when(
            move || entry == Entry::Password,
            move || {
                icon_button(
                    move || s.label(if shown.get() { "Hide password" } else { "Show password" }),
                    move || if shown.get() { Icon::EyeSlash } else { Icon::Eye },
                )
                .on_press(move || shown.update(|v| *v = !*v))
            },
        ))
        // Down, its line is centered in the field.
        .child(floating_label(label_, floated, focused, error, |outline| {
            if IOS {
                return (field_height() * font_scale() - 2.0 * outline - 22.0 * font_scale()) / 2.0;
            }
            (CONTROL * font_scale() - 2.0 * outline - 24.0 * font_scale()) / 2.0
        }))
}
/// A field of an iOS alert, as the system's: 44 pt on a gray fill, its label as the placeholder, masked when it takes a
/// password, its edge red while `error` holds.
#[allow(clippy::too_many_arguments)]
fn alert_field(
    s: Store,
    id: &'static str,
    entry: Entry,
    label_: impl Fn() -> SharedString + Copy + 'static,
    get: impl Fn() -> String + Copy + 'static,
    set: impl Fn(&str) + 'static,
    on_submit: impl Fn() + 'static,
    on_focus_change: impl Fn(bool) + 'static,
    error: impl Fn() -> bool + Copy + 'static,
    return_key: ReturnKeyType,
    reference: Option<ElementRef>,
) -> View {
    let me = reference.unwrap_or_else(element_ref);
    let on_focus_change = std::rc::Rc::new(on_focus_change);
    let on_blur = on_focus_change.clone();
    view()
        .min_h(move || px(44.0) * font_scale())
        .px(px(12.0))
        .justify_center()
        .rounded_full()
        .bg(move || tok("alert-field"))
        .border(px(1.0))
        .border_color(move || if error() { theme().destructive } else { tok("alert-field").opacity(0.0) })
        .on_press(move |_: &()| me.focus())
        .child(
            themed_input(input())
                .element_ref(&me)
                .test_id(id)
                .label(label_)
                .placeholder(move || label_().to_string())
                .masked(entry == Entry::Password)
                .font_family(if entry == Entry::Key { CODE_FONT.family } else { "system-ui" })
                .value(get)
                .on_change(move |v: &str| {
                    s.controller.get().activity();
                    set(v);
                })
                .on_submit(move |_: &str| on_submit())
                .on_focus(move |_: &()| on_focus_change(true))
                .on_blur(move |_: &()| on_blur(false))
                .return_key_type(return_key)
                .auto_correct(false)
                .auto_capitalize(nsh_core::AutoCapitalize::None)
                .auto_fill(nsh_core::AutoFill::Off)
                .text_size(px(17.0))
                .text_color(move || theme().foreground)
                .disabled(move || s.busy.get()),
        )
}
/// How tall a field is at the default text size: iOS's holds its shrunken label over the text.
fn field_height() -> f32 {
    if IOS { 58.0 } else { CONTROL }
}
/// How far the text of an iOS field moves down while its label sits over it.
const FLOATED_TEXT: f32 = 14.0;

/// A password field in a capsule, with no label of its own on screen (the text above it says what it is; `label_` is for
/// screen readers): 56 pt, outlined as `field` is, the show and hide control at its end.
#[allow(clippy::too_many_arguments)]
pub fn pill_password(
    s: Store,
    id: &'static str,
    label_: impl Fn() -> SharedString + Copy + 'static,
    get: impl Fn() -> String + Copy + 'static,
    set: impl Fn(&str) + 'static,
    on_submit: impl Fn() + 'static,
    error: impl Fn() -> bool + Copy + 'static,
) -> impl IntoElement + Styled {
    let focused = signal(false);
    let shown = signal(false);
    let me = element_ref();
    outlined(focused, error, me)
        .mt(px(0.0))
        .rounded_full()
        .flex_row()
        .items_center()
        .min_h(move || px(CONTROL) * font_scale())
        .pl(px(24.0))
        .pr(px(8.0))
        .child(
            view().flex_1().min_w(px(0.0)).justify_center().child(
                themed_input(input())
                    .element_ref(&me)
                    .test_id(id)
                    .label(label_)
                    .masked(move || !shown.get())
                    .font_family(move || SharedString::from(if shown.get() { CODE_FONT.family } else { "system-ui" }))
                    .value(get)
                    .on_change(move |v: &str| {
                        s.controller.get().activity();
                        set(v);
                    })
                    .on_submit(move |_: &str| on_submit())
                    .on_focus(move |_: &()| focused.set(true))
                    .on_blur(move |_: &()| focused.set(false))
                    .return_key_type(ReturnKeyType::Go)
                    .auto_correct(false)
                    .auto_capitalize(nsh_core::AutoCapitalize::None)
                    .auto_fill(nsh_core::AutoFill::Off)
                    .text_size(px(16.0))
                    .text_color(move || theme().foreground)
                    .disabled(move || s.busy.get()),
            ),
        )
        .child(
            icon_button(
                move || s.label(if shown.get() { "Hide password" } else { "Show password" }),
                move || if shown.get() { Icon::EyeSlash } else { Icon::Eye },
            )
            .on_press(move || shown.update(|v| *v = !*v)),
        )
}

/// A text field of several lines, `height` points tall at least, outlined as `field` is. Down, its label stands on
/// the first line; `placeholder` shows only once the label has floated, while the field is empty.
#[allow(clippy::too_many_arguments)]
pub fn text_area(
    s: Store,
    id: &'static str,
    label_: impl Fn() -> SharedString + Copy + 'static,
    placeholder: impl Fn() -> SharedString + Copy + 'static,
    get: impl Fn() -> String + Copy + 'static,
    set: impl Fn(&str) + 'static,
    error: impl Fn() -> bool + Copy + 'static,
    height: f32,
) -> impl IntoElement + Styled {
    let focused = signal(false);
    let floated = move || focused.get() || !get().is_empty();
    let me = element_ref();
    outlined(focused, error, me)
        .min_h(px(height))
        // At most twice that: a long paste scrolls inside the field, and what is under it (the page's action) stays.
        .max_h(px(height * 2.0))
        .px(px(16.0))
        .pt(px(if IOS { 28.0 } else { 14.0 }))
        .pb(px(14.0))
        .child(
            themed_input(input())
                .element_ref(&me)
                .test_id(id)
                .label(label_)
                .placeholder(move || if focused.get() || IOS { placeholder().to_string() } else { String::new() })
                .multiline(true)
                .min_h(px(0.0))
                .flex_shrink(1.0)
                .value(get)
                .on_change(move |v: &str| {
                    s.controller.get().activity();
                    set(v);
                })
                .on_focus(move |_: &()| focused.set(true))
                .on_blur(move |_: &()| focused.set(false))
                .auto_correct(false)
                .auto_capitalize(nsh_core::AutoCapitalize::None)
                .auto_fill(nsh_core::AutoFill::Off)
                .flex_1()
                .text_size(px(16.0))
                .text_color(move || theme().foreground)
                .disabled(move || s.busy.get()),
        )
        // Down, its line is the first line's, under the field's top padding. iOS: always at the top, the text under it.
        .child(floating_label(label_, move || IOS || floated(), focused, error, |_| 14.0))
}

/// The line under a field: a problem in the error color, or nothing, in a height that is always there so the form does
/// not move.
pub fn field_note(message: impl Fn() -> Option<SharedString> + Copy + 'static, problem: impl Fn() -> bool + Copy + 'static) -> AnyElement {
    // An alert's, or a form row's on iOS: only while it says something (iOS's alert: centered).
    if compact_now() || (IOS && FORM.get().is_some()) {
        let in_form = !compact_now();
        return when(
            move || message().is_some(),
            move || {
                let note = caption(text(move || message().unwrap_or_default()));
                if IOS && !in_form { note.text_center() } else { note }
                    .px(px(if IOS && !in_form { 0.0 } else { 16.0 }))
                    .pb(px(if in_form { 10.0 } else { 0.0 }))
                    .text_color(move || if problem() { theme().destructive } else { theme().muted_foreground })
            },
        )
        .into_element();
    }
    caption(text(move || message().unwrap_or_else(|| " ".into())))
        .px(px(16.0))
        .text_color(move || if problem() { theme().destructive } else { theme().muted_foreground })
        .into_element()
}

// ---- Small parts ---------------------------------------------------------------------------------------------------

/// A filter chip: 32 pt, filled with the primary color when selected, outlined otherwise.
pub fn chip(
    label_: impl Fn() -> SharedString + Copy + 'static,
    selected: impl Fn() -> bool + Copy + 'static,
    glyph: Option<Icon>,
) -> nsh_base::ButtonElement {
    // iOS: a capsule, filled with the accent when selected and with the system's gray otherwise.
    if IOS {
        return button()
            .accessibility_label(move || Some(label_()))
            .selected(selected)
            .h(move || px(34.0) * font_scale())
            .pl(px(if glyph.is_some() { 10.0 } else { 14.0 }))
            .pr(px(14.0))
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .flex_shrink(0.0)
            .rounded_full()
            .bg(move || if selected() { theme().primary } else { tok("input") })
            .text_color(move || if selected() { theme().primary_foreground } else { theme().foreground })
            .active(|s| s.opacity(0.7))
            .child(when(move || glyph.is_some(), move || icon(glyph.unwrap_or(Icon::Sliders)).size(px(18.0))))
            .child(label(text(label_)).number_of_lines(1));
    }
    button()
        .accessibility_label(move || Some(label_()))
        .selected(selected)
        .h(move || px(32.0) * font_scale())
        .pl(px(if glyph.is_some() { 8.0 } else { 16.0 }))
        .pr(px(16.0))
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .flex_shrink(0.0)
        .rounded(px(12.0))
        .border_1()
        .border_color(move || if selected() { theme().primary } else { theme().border })
        .bg(move || if selected() { theme().primary } else { theme().background.opacity(0.0) })
        .text_color(move || {
            if selected() {
                theme().primary_foreground
            } else if glyph.is_some() {
                theme().foreground
            } else {
                theme().muted_foreground
            }
        })
        .active(|s| s.bg(move || tok("pressed")))
        .child(when(move || glyph.is_some(), move || icon(glyph.unwrap_or(Icon::Sliders)).size(px(18.0))))
        .child(label(text(label_)).number_of_lines(1))
}

/// A square tile around a glyph: the leading mark of a choice or a settings row.
pub fn icon_tile(glyph: Icon, size: f32, radius: f32, fill: &'static str, ink: &'static str) -> impl IntoElement {
    view()
        .size(px(size))
        .flex_shrink(0.0)
        .items_center()
        .justify_center()
        .rounded(px(radius))
        .bg(move || tok(fill))
        .text_color(move || tok(ink))
        .child(icon(glyph).size(px(size * 0.54)))
}

/// The bar at a page's top: a back button, an optional centered title.
/// The room a page keeps at its top: the status bar's, or none on iOS, where the system's header is over it
/// (`mobile::native_stack`).
pub fn page_top() -> Points {
    if IOS { px(0.0) } else { safe_area().top }
}
/// iOS: nothing, the system's header shows the title and goes back (`mobile::native_stack`).
pub fn top_bar(s: Store, title_: Option<&'static str>, back: impl Fn() + 'static) -> AnyElement {
    if IOS {
        // No view at all: the system's header tracks the page's first subview as its scroll view, for the large title
        // to collapse as the content scrolls.
        let _ = (s, title_, back);
        return when(|| false, view).into_element();
    }
    own_top_bar(s, title_, back)
}
/// The bar of a page, as Material's top app bar: the back arrow, then the title at the start.
fn own_top_bar(s: Store, title_: Option<&'static str>, back: impl Fn() + 'static) -> AnyElement {
    let back_button = icon_button(move || s.label("Back"), || Icon::Back).text_color(move || theme().foreground).on_press(back);
    view()
        .flex_row()
        .items_center()
        .h(move || px(BAR) * font_scale())
        .px(px(4.0))
        .child(back_button)
        .child(when(
            move || title_.is_some(),
            move || title_large(text(move || s.label(title_.unwrap_or_default()))).number_of_lines(1).flex_1().pl(px(4.0)).pr(px(12.0)),
        ))
        .into_element()
}

// ---- Rows, groups and choices --------------------------------------------------------------------------------------

/// The title over a group of rows.
pub fn section_title(text_: impl Fn() -> SharedString + 'static) -> impl IntoElement + Styled {
    // iOS: the system's group header, in capitals over the group's text.
    if IOS {
        return caption(text(move || SharedString::from(text_().to_uppercase())))
            .text_color(move || theme().muted_foreground)
            .pl(px(16.0))
            .pt(px(24.0))
            .pb(px(7.0));
    }
    label(text(text_)).text_color(move || theme().primary).pl(px(16.0)).pt(px(24.0)).pb(px(8.0))
}
/// The name over a part of a sheet ("Categories", "Secret"): iOS's group header there, elsewhere a label.
pub fn heading(text_: impl Fn() -> SharedString + 'static) -> impl IntoElement + Styled {
    if IOS {
        return caption(text(move || SharedString::from(text_().to_uppercase()))).text_color(move || theme().muted_foreground).pl(px(16.0));
    }
    label(text(text_))
}
/// The line of detail under a row's title.
pub type Detail = std::rc::Rc<dyn Fn() -> SharedString>;
/// A row of a group: a leading mark, a title over an optional line of detail, a trailing control. A row with an action
/// is pressed as a whole; screen readers say its title and its detail ("Auto-Lock, 5 minutes").
pub fn row(
    leading: impl IntoElement,
    title_: impl Fn() -> SharedString + Copy + 'static,
    detail: Option<Detail>,
    trailing: impl IntoElement,
) -> nsh_base::ButtonElement {
    let said = row_label(title_, detail.clone());
    button()
        .accessibility_label(move || Some(said()))
        .bg(move || theme().card)
        .active(|s| s.bg(move || if IOS { tok("pressed") } else { tok("surface-high") }))
        .child(row_body(leading, title_, detail, trailing))
}
/// A row that turns something on or off, with a switch at its end; screen readers announce it as a switch, on or off.
pub fn switch_row(
    leading: impl IntoElement,
    title_: impl Fn() -> SharedString + Copy + 'static,
    detail: Detail,
    on: impl Fn() -> bool + Copy + 'static,
) -> View {
    // iOS: the detail is the group's footer (`footer`), not in the row.
    let detail = if IOS { None } else { Some(detail) };
    toggle(AccessibilityRole::Switch, row_label(title_, detail.clone()), on)
        .bg(move || theme().card)
        .active(|s| s.bg(move || if IOS { tok("pressed") } else { tok("surface-high") }))
        .child(row_body(leading, title_, detail, switch(on)))
}
fn row_label(title_: impl Fn() -> SharedString + Copy + 'static, detail: Option<Detail>) -> impl Fn() -> SharedString + 'static {
    move || match &detail {
        Some(d) => SharedString::from(format!("{}, {}", title_(), d())),
        None => title_(),
    }
}
fn row_body(
    leading: impl IntoElement,
    title_: impl Fn() -> SharedString + Copy + 'static,
    detail: Option<Detail>,
    trailing: impl IntoElement,
) -> View {
    let tall = detail.is_some();
    let (high, low) = if IOS { (60.0, 48.0) } else { (72.0, 56.0) };
    view()
        .min_h(move || px(if tall { high } else { low }) * font_scale())
        .px(px(16.0))
        .py(px(if IOS { 8.0 } else { 10.0 }))
        .flex_row()
        .items_center()
        .gap(px(if IOS { 14.0 } else { 16.0 }))
        .child(leading)
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .child(body_large(text(title_)))
                .child(detail.map(|d| dim(body(text(move || d()))).into_element())),
        )
        .child(trailing)
}
/// A round tile around a glyph: the leading mark of a settings row.
pub fn round_tile(glyph: Icon, destructive: bool) -> impl IntoElement {
    // iOS: a square tile in a color of its own, the glyph white, as the system's settings draw theirs.
    if IOS {
        return view()
            .size(px(30.0))
            .flex_shrink(0.0)
            .items_center()
            .justify_center()
            .rounded(px(7.0))
            .bg(move || if destructive { theme().destructive } else { tile_color(glyph) })
            .text_color(|| AdaptiveColor::rgb(0xffffff, 0xffffff))
            .child(crate::icons::icon_bold(glyph).size(px(19.0)));
    }
    view()
        .size(px(40.0))
        .flex_shrink(0.0)
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(move || if destructive { tok("destructive-container") } else { theme().secondary })
        .text_color(move || if destructive { tok("destructive-container-foreground") } else { theme().foreground })
        .child(icon(glyph).size(px(22.0)))
}
/// The tile color of a settings row's glyph on iOS, by what the row is about.
fn tile_color(glyph: Icon) -> AdaptiveColor {
    match glyph {
        Icon::TouchId | Icon::FaceId => AdaptiveColor::rgb(0x34c759, 0x30d158),
        Icon::Lock | Icon::Eye | Icon::ExportImport | Icon::Download | Icon::Cloud => AdaptiveColor::rgb(0x007aff, 0x0a84ff),
        Icon::Photo => AdaptiveColor::rgb(0xff9500, 0xff9f0a),
        Icon::Key => AdaptiveColor::rgb(0xff9500, 0xff9f0a),
        _ => AdaptiveColor::rgb(0x8e8e93, 0x8e8e93),
    }
}
/// Rows as one group: 24 pt outer corners, 6 pt between rows. iOS: one rounded group, hairlines between its rows from
/// under their text.
pub fn group(rows: Vec<AnyElement>) -> impl IntoElement + Styled {
    let count = rows.len();
    view().gap(px(GROUP_GAP)).children(rows.into_iter().enumerate().map(move |(index, row)| {
        view()
            .overflow_hidden()
            .rounded_t(px(if index == 0 { GROUP_RADIUS } else { GROUP_INNER }))
            .rounded_b(px(if index + 1 == count { GROUP_RADIUS } else { GROUP_INNER }))
            .child(row)
            .child(when(move || IOS && index > 0, || separator(TILE_INSET)))
    }))
}
/// The small text under a group of rows on iOS (its footer): what a row's setting means. Elsewhere rows carry it
/// themselves.
pub fn footer(text_: impl Fn() -> SharedString + 'static) -> impl IntoElement + Styled {
    dim(caption(text(text_))).px(px(16.0)).pt(px(7.0))
}
/// A row that shows a value at its end, in secondary text (iOS), or under its title elsewhere; `trailing` after it.
pub fn value_row(
    leading: impl IntoElement,
    title_: impl Fn() -> SharedString + Copy + 'static,
    value: impl Fn() -> SharedString + 'static,
    trailing: impl IntoElement,
) -> nsh_base::ButtonElement {
    if !IOS {
        return row(leading, title_, Some(std::rc::Rc::new(value)), trailing);
    }
    let value = std::rc::Rc::new(value);
    let said = value.clone();
    button()
        .accessibility_label(move || Some(SharedString::from(format!("{}, {}", title_(), said()))))
        .bg(move || theme().card)
        .active(|s| s.bg(move || tok("pressed")))
        .child(value_body(leading, title_, move || value(), trailing))
}
fn value_body(
    leading: impl IntoElement,
    title_: impl Fn() -> SharedString + Copy + 'static,
    value: impl Fn() -> SharedString + 'static,
    trailing: impl IntoElement,
) -> View {
    view()
        .min_h(move || px(48.0) * font_scale())
        .px(px(16.0))
        .py(px(8.0))
        .flex_row()
        .items_center()
        .gap(px(14.0))
        .child(leading)
        .child(body_large(text(title_)).flex_1().min_w(px(0.0)))
        .child(dim(body_large(text(value))).number_of_lines(1).flex_shrink(0.0))
        .child(trailing)
}
/// A setting chosen among a few: its value at its end. iOS: a press opens the system's pull-down menu
/// (`UIMenu`) of the choices, the current one checked; elsewhere the row's `on_press` (a dialog) chooses. `id` is the
/// row's test ID.
#[allow(clippy::too_many_arguments)]
pub fn picker_row<T: Copy + PartialEq + 'static>(
    id: &'static str,
    leading: impl IntoElement,
    title_: impl Fn() -> SharedString + Copy + 'static,
    value: impl Fn() -> SharedString + 'static,
    options: impl Fn() -> Vec<(T, SharedString)> + 'static,
    get: impl Fn() -> T + Copy + 'static,
    set: impl Fn(T) + 'static,
    on_press: impl Fn() + 'static,
) -> AnyElement {
    #[cfg(target_os = "ios")]
    {
        let _ = on_press;
        let options = std::rc::Rc::new(options);
        let chosen = options.clone();
        // The system's pop-up button is the value at the row's end, as the system's settings have it: its menu opens
        // from there, at the trailing edge, and the row stays as it is. (A button as wide as the row would be the
        // menu's source: the menu would open from the row's middle and the row would go while it is open.)
        let button = nsh_base::menu_button()
            .test_id(id)
            .accessibility_label(move || Some(title_()))
            .items(move || {
                let current = get();
                let items = options()
                    .into_iter()
                    .enumerate()
                    .map(|(i, (v, label))| {
                        let mut item = nsh_base::MenuButtonItem::new(SharedString::from(i.to_string()), label);
                        item.checked = Some(v == current);
                        item
                    })
                    .collect::<Vec<_>>();
                vec![nsh_base::MenuButtonItem::section("", items).single_selection(true)]
            })
            .on_select(move |id: &SharedString| {
                if let Some((v, _)) = id.parse::<usize>().ok().and_then(|i| chosen().get(i).cloned()) {
                    set(v)
                }
            })
            .min_h(px(TOUCH))
            .pl(px(12.0))
            .mr(px(-16.0))
            .pr(px(16.0))
            .flex_row()
            .items_center()
            .justify_end()
            .gap(px(6.0))
            .flex_shrink(1.0)
            .min_w(px(0.0))
            .child(dim(body_large(text(value))).number_of_lines(1).flex_shrink(1.0))
            .child(view().text_color(move || tok("tertiary")).child(crate::icons::icon_bold(Icon::ChevronUpDown).size(px(16.0))));
        return view()
            .bg(move || theme().card)
            .min_h(move || px(48.0) * font_scale())
            .pl(px(16.0))
            .pr(px(16.0))
            .flex_row()
            .items_center()
            .gap(px(14.0))
            .child(leading)
            .child(body_large(text(title_)).flex_1().min_w(px(0.0)).accessibility_elements_hidden(true))
            .child(button)
            .into_element();
    }
    #[allow(unreachable_code)]
    {
        let _ = (options, get, set);
        row(leading, title_, Some(std::rc::Rc::new(value)), chevron()).test_id(id).on_press(on_press).into_element()
    }
}
/// The mark at a row's end that says it opens a page.
pub fn chevron() -> impl IntoElement {
    if IOS {
        return view().text_color(move || tok("tertiary")).child(crate::icons::icon_bold(Icon::ChevronRight).size(px(18.0)));
    }
    view().text_color(move || theme().muted_foreground).child(icon(Icon::ChevronRight).size(px(24.0)))
}
/// A switch: 52 by 32 pt. iOS: the system's `UISwitch` (Liquid Glass from iOS 26), which the row around it flips: it
/// takes no touches of its own.
// iOS returns the system's at the top; the rest is for the other systems.
#[cfg_attr(target_os = "ios", allow(unreachable_code))]
pub fn switch(on: impl Fn() -> bool + Copy + 'static) -> impl IntoElement {
    #[cfg(target_os = "ios")]
    return view()
        .flex_shrink(0.0)
        .pointer_events(nsh_core::PointerEvents::None)
        // The row is the switch screen readers hear (`switch_row`).
        .accessibility_elements_hidden(true)
        .child(nsh_core::mobile::switch().checked(on))
        .into_element();
    if IOS {
        return view()
            .w(px(51.0))
            .h(px(31.0))
            .flex_shrink(0.0)
            .rounded_full()
            .justify_center()
            .bg(move || if on() { tok("switch-on") } else { tok("switch-off") })
            .transition()
            .duration(EFFECTS_MS)
            .ease_out()
            .child(
                view()
                    .size(px(27.0))
                    .ml(move || px(if on() { 22.0 } else { 2.0 }))
                    .rounded_full()
                    .bg(|| AdaptiveColor::rgb(0xffffff, 0xffffff))
                    .shadow_sm()
                    .transition()
                    .duration(EFFECTS_MS)
                    .ease_out(),
            )
            .into_element();
    }
    view()
        .w(px(52.0))
        .h(px(32.0))
        .flex_shrink(0.0)
        .rounded_full()
        .justify_center()
        .border(px(2.0))
        .border_color(move || if on() { theme().primary } else { tok("outline") })
        .bg(move || if on() { theme().primary } else { tok("surface-highest") })
        .transition()
        .duration(EFFECTS_MS)
        .ease_out()
        .child(
            view()
                .size(move || px(if on() { 24.0 } else { 16.0 }))
                .ml(move || px(if on() { 22.0 } else { 6.0 }))
                .rounded_full()
                .bg(move || if on() { theme().primary_foreground } else { tok("outline") })
                .transition()
                .duration(EFFECTS_MS)
                .ease_out(),
        )
        .into_element()
}
/// One of a few choices, side by side: 48 pt segments, 20 pt corners at the ends and 6 pt between.
#[cfg_attr(target_os = "ios", allow(unreachable_code))]
pub fn choice<T: Copy + PartialEq + 'static>(
    options: Vec<(T, Box<dyn Fn() -> SharedString>)>,
    get: impl Fn() -> T + Copy + 'static,
    set: impl Fn(T) + Copy + 'static,
) -> impl IntoElement {
    // iOS: the system's `UISegmentedControl`.
    #[cfg(target_os = "ios")]
    {
        let values: Vec<T> = options.iter().map(|(v, _)| *v).collect();
        let names: Vec<Box<dyn Fn() -> SharedString>> = options.into_iter().map(|(_, n)| n).collect();
        let chosen = values.clone();
        return nsh_core::mobile::segmented_control()
            .items(move || names.iter().map(|n| nsh_core::mobile::SegmentedControlItem::new(n())).collect::<Vec<_>>())
            .selected_index(move || values.iter().position(|v| *v == get()).unwrap_or(0) as u32)
            .on_change(move |index: &u32| {
                if let Some(v) = chosen.get(*index as usize) {
                    set(*v)
                }
            })
            .w_full()
            .test_id("segments")
            .into_element();
    }
    let count = options.len();
    // A desktop showing iOS's look: a drawn one, the chosen segment raised on a gray track.
    if IOS {
        return view()
            .flex_row()
            .p(px(2.0))
            .rounded(px(9.0))
            .bg(move || tok("segment-track"))
            .children(options.into_iter().map(move |(value, name)| {
                let on = move || get() == value;
                let name: std::rc::Rc<dyn Fn() -> SharedString> = name.into();
                let said = name.clone();
                button()
                    .accessibility_label(move || Some(said()))
                    .selected(on)
                    .flex_1()
                    .min_w(px(0.0))
                    .h(move || px(32.0) * font_scale())
                    .px(px(8.0))
                    .items_center()
                    .justify_center()
                    .rounded(px(7.0))
                    .bg(move || if on() { tok("segment-selected") } else { tok("segment-selected").opacity(0.0) })
                    .transition()
                    .duration(EFFECTS_MS)
                    .ease_out()
                    .on_press(move || set(value))
                    .child(
                        text(move || name())
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .number_of_lines(1)
                            .text_color(move || theme().foreground),
                    )
            }))
            .into_element();
    }
    view()
        .flex_row()
        .gap(px(GROUP_GAP))
        .children(options.into_iter().enumerate().map(move |(index, (value, name))| {
            let on = move || get() == value;
            let name: std::rc::Rc<dyn Fn() -> SharedString> = name.into();
            let said = name.clone();
            button()
                .accessibility_label(move || Some(said()))
                .selected(on)
                .flex_1()
                .min_w(px(0.0))
                .h(move || px(48.0) * font_scale())
                .px(px(12.0))
                .items_center()
                .justify_center()
                .rounded_l(px(if index == 0 { 20.0 } else { GROUP_INNER }))
                .rounded_r(px(if index + 1 == count { 20.0 } else { GROUP_INNER }))
                .bg(move || if on() { theme().primary } else { theme().secondary })
                .transition()
                .duration(EFFECTS_MS)
                .ease_out()
                .on_press(move || set(value))
                .child(
                    label(text(move || name()))
                        .number_of_lines(1)
                        .text_color(move || if on() { theme().primary_foreground } else { theme().foreground }),
                )
        }))
        .into_element()
}
/// A tonal action: 56 pt, with a leading glyph.
pub fn tonal_button(
    glyph: impl Fn() -> Icon + Copy + 'static,
    label_: impl Fn() -> SharedString + Copy + 'static,
    destructive: bool,
) -> nsh_base::ButtonElement {
    button()
        .accessibility_label(move || Some(label_()))
        .h(move || px(CONTROL) * font_scale())
        .px(px(16.0))
        .flex_row()
        .items_center()
        .justify_center()
        .gap(px(8.0))
        // iOS: a grouped button on a sheet, white (raised in dark) with the accent's (or the destructive) text.
        .bg(move || {
            if IOS {
                tok("sheet-card")
            } else if destructive {
                tok("destructive-container")
            } else {
                theme().secondary
            }
        })
        .text_color(move || {
            if IOS {
                if destructive { theme().destructive } else { theme().primary }
            } else if destructive {
                tok("destructive-container-foreground")
            } else {
                theme().foreground
            }
        })
        .active(|s| s.opacity(if IOS { 0.6 } else { 0.85 }))
        .child(view().size(px(18.0)).child(dynamic(move || icon(glyph()).size(px(18.0)).into_element())))
        .child(label(text(label_)).number_of_lines(1))
}
/// What an action of a dialog does: closes it, does what it is for, or destroys something.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Act {
    Cancel,
    Default,
    Destructive,
}
/// An action of a dialog (`dialog`): text only elsewhere; on iOS a capsule as iOS 26's alerts draw theirs, the default
/// one filled with the accent, the others gray, sharing the row (`dialog_actions`).
pub fn dialog_action(label_: impl Fn() -> SharedString + Copy + 'static, act: Act) -> nsh_base::ButtonElement {
    if !IOS {
        return text_button(label_, act == Act::Destructive);
    }
    button()
        .accessibility_label(move || Some(label_()))
        .flex_1()
        .min_w(px(0.0))
        .min_h(move || px(48.0) * font_scale())
        .px(px(14.0))
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(move || if act == Act::Default { theme().primary } else { tok("input") })
        .text_color(move || match act {
            Act::Default => theme().primary_foreground,
            Act::Cancel => theme().foreground,
            Act::Destructive => theme().destructive,
        })
        .active(|s| s.opacity(0.7))
        // As the system's alerts: an action that cannot be taken yet is a gray capsule, its label dimmed.
        .disabled_style(|s| s.bg(move || tok("input")).text_color(move || theme().foreground.opacity(0.3)))
        .child(
            body_large(text(label_))
                .number_of_lines(1)
                .font_weight(if act == Act::Default { FontWeight::SEMIBOLD } else { FontWeight::MEDIUM })
                .text_center(),
        )
}
/// The row of a dialog's actions: side by side, or on iOS one over another (`stacked`) where a label is long.
pub fn dialog_actions(stacked: bool) -> View {
    // Stacked: the default action on top, as the system's alerts order a column (the cancel one last).
    if IOS && stacked {
        return view().flex_col_reverse().gap(px(8.0));
    }
    view().flex_row().justify_end().gap(px(if IOS { 8.0 } else { 4.0 }))
}
/// An action of a dialog or a bar: text only, 40 pt.
pub fn text_button(label_: impl Fn() -> SharedString + Copy + 'static, destructive: bool) -> nsh_base::ButtonElement {
    button()
        .accessibility_label(move || Some(label_()))
        .h(move || px(40.0) * font_scale())
        .px(px(12.0))
        .items_center()
        .justify_center()
        .rounded_full()
        .active(|s| s.bg(move || tok("pressed")))
        .child(
            label(text(label_))
                .text_size(px(if IOS { 17.0 } else { 14.0 }))
                .text_color(move || if destructive { theme().destructive } else { theme().primary }),
        )
}

// ---- Dialogs and sheets --------------------------------------------------------------------------------------------

/// A dialog over the page: a scrim and a panel with a title, its content (built for an alert: `compact`) and its
/// actions, end-aligned. A press on the
/// scrim asks to close it.
pub fn dialog<E: IntoElement>(
    title_: impl Fn() -> SharedString + 'static,
    content: impl FnOnce() -> E,
    actions: impl IntoElement,
    dismiss: impl Fn() + Copy + 'static,
) -> impl IntoElement {
    // Its fields and notes are an alert's (`compact`).
    let content = compact(content);
    // Centered in the room above the keyboard while it is up.
    let scrim = view().flex_1().items_center().justify_center().px(px(27.0)).pt_safe_area().pb_keyboard();
    // A modal of its own: over the page and over a sheet that is open on it. The back key asks to close it. Android: its
    // scrim reaches under the status bar, which dims with the page, as the bottom sheet's does.
    nsh_base::dialog().animation(nsh_core::ModalAnimation::Fade).status_bar_translucent(true).on_cancel(move |_: &()| dismiss()).child(
        scrim.bg(move || tok("scrim")).on_press(move |_: &()| dismiss()).child(
            view()
                .w_full()
                .max_w(px(if IOS { 320.0 } else { 560.0 }))
                .rounded(px(if IOS { 34.0 } else { 28.0 }))
                // Android: the system's own alerts' surface, so a drawn dialog reads as one of them. iOS: Liquid Glass,
                // as the system's alerts (`AlertDialog` shows those where it can: a dialog is drawn only for a password,
                // which the system's alert cannot mask through NSH).
                .bg(move || if cfg!(target_os = "ios") { tok("popover").opacity(0.0) } else { tok("popover") })
                .glass_effect(if cfg!(target_os = "ios") { nsh_core::GlassEffect::Regular } else { nsh_core::GlassEffect::None })
                // Tinted with the alert's surface: the system's alerts are frosted, what is under them does not read.
                .glass_tint_color(move || if cfg!(target_os = "ios") { Some(tok("popover").opacity(0.82)) } else { None })
                // A light rim in dark appearance, where the shadow does not show.
                .border_1()
                .border_color(move || tok("sheet-rim"))
                .shadow_lg()
                // A press inside stays inside.
                .on_press(|_: &()| {})
                .child(if IOS {
                    // iOS: an alert's title, centered.
                    title(text(title_)).text_center().px(px(20.0)).pt(px(22.0)).pb(px(8.0)).into_element()
                } else {
                    text(title_).text_size(px(22.0)).line_height(px(28.0)).px(px(24.0)).pt(px(22.0)).pb(px(12.0)).into_element()
                })
                .child(view().px(px(if IOS { 20.0 } else { 24.0 })).gap(px(8.0)).child(content))
                .child(if IOS {
                    // iOS 26's: the actions as capsules across the alert (`dialog_action`).
                    view().px(px(16.0)).pt(px(12.0)).pb(px(16.0)).child(actions).into_element()
                } else {
                    view().flex_row().justify_end().gap(px(4.0)).px(px(12.0)).pt(px(16.0)).pb(px(12.0)).child(actions).into_element()
                }),
        ),
    )
}
/// The widest a bottom sheet grows.
pub const SHEET_MAX: f32 = 640.0;
/// The room a bottom sheet leaves above itself, under the status bar, however tall its content and wherever the
/// keyboard is: the scrim shows there, so a sheet still reads as one over the page, and its content scrolls instead.
pub const SHEET_TOP: f32 = 56.0;
/// How tall a bottom sheet's scroll view may be, besides `chrome` (what the sheet shows around it, in points at the
/// default text size): the room between `SHEET_TOP` under the status bar and the keyboard (the screen's bottom when it
/// is down), less the handle. A scroll view needs a height of its own: without one it is as tall as its content,
/// nothing scrolls, and a drag in it moves the sheet instead, which takes the keyboard down.
/// iOS: the system's sheet sizes itself (`bottom_sheet`); its scroll views fill it instead (`flex_1`).
pub fn sheet_room(chrome: f32) -> Points {
    if cfg!(target_os = "ios") {
        return px(100_000.0);
    }
    let keyboard = (keyboard_height() - safe_area().bottom).max(px(0.0));
    let bottom = if keyboard > px(0.0) { px(0.0) } else { safe_area().bottom };
    (window().height - safe_area().top - px(SHEET_TOP) - keyboard - bottom - px(28.0) - px(chrome) * font_scale()).max(px(120.0))
}
/// The space under a bottom sheet's content: the home indicator's while the keyboard is down, nothing above it.
pub fn sheet_bottom(extra: f32) -> Points {
    px(extra) + if keyboard_height() > px(0.0) { px(0.0) } else { safe_area().bottom }
}
/// A panel from the bottom over a scrim, as tall as its content, with a handle; a press on the scrim, a swipe down and
/// the back key close it.
pub fn bottom_sheet(
    open: impl Fn() -> bool + Copy + 'static,
    close: impl Fn() + Copy + 'static,
    content: impl Fn() -> AnyElement + 'static,
) -> AnyElement {
    // iOS: the system's sheet (`UISheetPresentationController`): on a phone (and a narrow iPad window) half the screen
    // or all of it, dragged between them, over the page it leaves visible, closed by a swipe down; in a wide iPad window
    // a card in the middle of the screen over the dimmed page, as iPadOS presents forms.
    #[cfg(target_os = "ios")]
    {
        let content = std::rc::Rc::new(content);
        return when(open, move || {
            let content = content.clone();
            let dialog = if untracked(is_wide) {
                nsh_base::dialog().presentation(nsh_core::ModalPresentation::FormSheet)
            } else {
                nsh_base::dialog()
                    .presentation(nsh_core::ModalPresentation::PageSheet)
                    .detents(vec![nsh_core::SheetDetent::Medium, nsh_core::SheetDetent::Large])
            };
            dialog.on_cancel(move |_: &()| close()).child(
                view()
                    .flex_1()
                    .bg(move || tok("sheet-surface"))
                    .pt(px(12.0))
                    // A column the content's scroll view fills.
                    .child(view().flex_1().min_h(px(0.0)).child(content())),
            )
        })
        .into_element();
    }
    // Android, a wide window (a tablet): a dialog in the middle of the screen, as Material has large screens show forms
    // rather than a sheet from the bottom edge; a narrow one keeps the bottom sheet.
    #[allow(unreachable_code)]
    let content = std::rc::Rc::new(content);
    let centered = content.clone();
    view()
        .child(when(move || open() && is_wide(), move || centered_sheet(close, centered.clone())))
        .child(bottom_panel(move || open() && !is_wide(), close, move || content()))
        .into_element()
}
/// A wide Android window's sheet: a panel of a readable width in the middle of the screen over the scrim, as tall as its
/// content up to most of the room above the keyboard (its scroll view takes the rest). A press on the scrim and the back
/// key close it, as its own close button does.
fn centered_sheet(close: impl Fn() + Copy + 'static, content: std::rc::Rc<impl Fn() -> AnyElement + 'static>) -> impl IntoElement {
    let scrim = view().flex_1().items_center().justify_center().px(px(24.0)).pt_safe_area().pb_keyboard();
    // Its scrim reaches under the status bar, which dims with the page.
    nsh_base::dialog().animation(nsh_core::ModalAnimation::Fade).status_bar_translucent(true).on_cancel(move |_: &()| close()).child(
        scrim.bg(move || tok("scrim")).on_press(move |_: &()| close()).child(
            view()
                .w_full()
                .max_w(px(CENTERED_SHEET_MAX))
                // Most of the room, its edges showing; with the keyboard up all of it but a margin above and below, so
                // a landscape tablet keeps the field typed into in view.
                .max_h(move || {
                    let keyboard = (keyboard_height() - safe_area().bottom).max(px(0.0));
                    let room = window().height - safe_area().top - safe_area().bottom - keyboard;
                    if keyboard > px(0.0) { room - px(16.0) } else { room * 0.9 }
                })
                .rounded(px(28.0))
                .bg(move || tok("sheet-surface"))
                .border_1()
                .border_color(move || tok("sheet-rim"))
                .shadow_lg()
                .overflow_hidden()
                .pt(px(12.0))
                .pb(px(8.0))
                // A press inside stays inside.
                .on_press(|_: &()| {})
                // The content gives way first: its scroll view takes what is left.
                .child(view().flex_shrink(1.0).min_h(px(0.0)).child(content())),
        ),
    )
}
/// The widest a wide Android window's centered sheet grows (Material's dialogs: 560 dp).
const CENTERED_SHEET_MAX: f32 = 560.0;
/// The bottom sheet of a phone (and a narrow window).
fn bottom_panel(
    open: impl Fn() -> bool + Copy + 'static,
    close: impl Fn() + Copy + 'static,
    content: impl Fn() -> AnyElement + 'static,
) -> AnyElement {
    nsh_base::sheet()
        .open(move || Some(open()))
        .on_open_change(move |e: &nsh_base::SheetEvent| {
            if let nsh_base::SheetEvent::OpenChange(false, _) = e {
                close()
            }
        })
        // Not while the keyboard is up: the sheet's drag would take the drag from the scroll view inside (and NSH takes
        // the keyboard down as it starts), so the fields under the keyboard could not be reached.
        .close_on_swipe(move || keyboard_height() <= px(0.0))
        .backdrop_style(|s| s.bg(move || tok("scrim")))
        .child(
            view()
                .w_full()
                .max_w(px(SHEET_MAX))
                .self_center()
                .rounded_t(px(if IOS { 34.0 } else { 28.0 }))
                // Lighter than the page in dark appearance, with a light rim: there a shadow does not show, and the
                // page's own color under the scrim would make the sheet one with it.
                .bg(move || tok("sheet-surface"))
                .border_1()
                .border_color(move || tok("sheet-rim"))
                // `SHEET_TOP` under the status bar at most, and above the keyboard while it is up: NSH lifts the panel
                // to the keyboard's top, so a panel taller than the room left would run off the top of the screen.
                .max_h(move || {
                    let keyboard = (keyboard_height() - safe_area().bottom).max(px(0.0));
                    window().height - safe_area().top - px(SHEET_TOP) - keyboard
                })
                .overflow_hidden()
                .child(
                    view().items_center().py(px(12.0)).child(
                        view()
                            .w(px(if IOS { 36.0 } else { 32.0 }))
                            .h(px(if IOS { 5.0 } else { 4.0 }))
                            .rounded_full()
                            .bg(move || tok("outline").opacity(if IOS { 0.35 } else { 0.5 })),
                    ),
                )
                // The content gives way first: its scroll view takes what is left.
                .child(view().flex_shrink(1.0).min_h(px(0.0)).child(content())),
        )
        .into_element()
}
