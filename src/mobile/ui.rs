//! The styled controls of the phone and tablet interface: one set of metrics (touch targets of 48 pt and more, 56 pt
//! fields and primary buttons, grouped rows with 24 pt outer and 6 pt inner corners) drawn with the tokens of
//! `theme-mobile.json`. Pages compose these and never style a control themselves.
use crate::{
    components::{CODE_FONT, tok},
    icons::{Icon, icon},
    store::Store,
};
use nsh_base::{button, input, theme};
use nsh_core::prelude::*;

/// Outer corner of a group of rows, and of a card that stands alone.
pub const GROUP_RADIUS: f32 = 24.0;
/// Corner between two rows of a group.
pub const GROUP_INNER: f32 = 6.0;
/// Gap between the rows of a group.
pub const GROUP_GAP: f32 = 2.0;
/// The page's side margin.
pub const MARGIN: f32 = 16.0;
/// Fields, primary buttons and the search bar.
pub const CONTROL: f32 = 56.0;
/// Color and opacity changes.
pub const EFFECTS_MS: f32 = 160.0;
/// Shape and position changes.
pub const SPATIAL_MS: f32 = 280.0;

// ---- Type scale ----------------------------------------------------------------------------------------------------

pub fn display<E: Styled>(e: E) -> E {
    e.text_size(px(34.0)).line_height(px(42.0)).font_weight(FontWeight::BOLD)
}
pub fn headline<E: Styled>(e: E) -> E {
    e.text_size(px(26.0)).line_height(px(34.0)).font_weight(FontWeight::SEMIBOLD)
}
pub fn title_large<E: Styled>(e: E) -> E {
    e.text_size(px(22.0)).line_height(px(28.0))
}
pub fn title<E: Styled>(e: E) -> E {
    e.text_size(px(16.0)).line_height(px(24.0)).font_weight(FontWeight::MEDIUM)
}
pub fn body_large<E: Styled>(e: E) -> E {
    e.text_size(px(16.0)).line_height(px(24.0))
}
pub fn body<E: Styled>(e: E) -> E {
    e.text_size(px(14.0)).line_height(px(20.0))
}
pub fn caption<E: Styled>(e: E) -> E {
    e.text_size(px(12.0)).line_height(px(16.0))
}
pub fn label<E: Styled>(e: E) -> E {
    e.text_size(px(14.0)).line_height(px(20.0)).font_weight(FontWeight::MEDIUM)
}
/// Secondary text.
pub fn dim<E: Styled>(e: E) -> E {
    e.text_color(move || theme().muted_foreground)
}

// ---- Surfaces ------------------------------------------------------------------------------------------------------

/// The corners of row `index` among `count` rows of a group.
pub fn group_corners<E: Styled>(e: E, index: impl Fn() -> usize + Copy + 'static, count: impl Fn() -> usize + Copy + 'static) -> E {
    e.rounded_t(move || px(if index() == 0 { GROUP_RADIUS } else { GROUP_INNER }))
        .rounded_b(move || px(if index() + 1 >= count() { GROUP_RADIUS } else { GROUP_INNER }))
}
/// A card of a form: a field group, a switch row.
pub fn form_card() -> View {
    view().rounded(px(28.0)).bg(move || theme().card).p(px(16.0))
}

// ---- Buttons -------------------------------------------------------------------------------------------------------

/// The page's primary action: full width, 56 pt, a pill. `busy` replaces the label with a spinner.
pub fn primary_button(
    label_: impl Fn() -> SharedString + Copy + 'static,
    enabled: impl Fn() -> bool + Copy + 'static,
    busy: impl Fn() -> bool + Copy + 'static,
) -> nsh_base::ButtonElement {
    button()
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
        .child(when(busy, || crate::components::spinner(20.0)).otherwise(move || {
            label(text(label_))
                .text_size(px(16.0))
                .text_color(move || if enabled() { theme().primary_foreground } else { theme().foreground.opacity(0.38) })
        }))
}
/// A secondary action beside or under a primary one: 48 pt, outlined.
pub fn outlined_button(label_: impl Fn() -> SharedString + 'static) -> nsh_base::ButtonElement {
    button()
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
pub fn filled_button(glyph: Option<Icon>, label_: impl Fn() -> SharedString + 'static) -> nsh_base::ButtonElement {
    button()
        .h(move || px(48.0) * font_scale())
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
    button()
        .accessibility_label(move || Some(name()))
        .size(px(48.0))
        .items_center()
        .justify_center()
        .rounded_full()
        .text_color(move || theme().muted_foreground)
        .active(|s| s.bg(move || tok("pressed")))
        .child(view().size(px(24.0)).child(dynamic(move || icon(glyph()).size(px(24.0)).into_element())))
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
/// The one text field: 56 pt, 16 pt corners, its label inside, floating above the text once there is any or the field
/// has the focus. The outline is the primary color with the focus and the error color while `error` holds.
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
    let focused = signal(false);
    let shown = signal(entry != Entry::Password);
    let floated = move || focused.get() || !get().is_empty();
    let on_focus_change = std::rc::Rc::new(on_focus_change);
    let on_blur = on_focus_change.clone();
    let me = reference.unwrap_or_else(element_ref);
    view()
        .flex_row()
        .items_center()
        .min_h(move || px(CONTROL) * font_scale())
        .pl(px(16.0))
        .pr(px(if entry == Entry::Password { 4.0 } else { 16.0 }))
        .rounded(px(16.0))
        .bg(move || theme().card)
        .border(move || px(if focused.get() || error() { 2.0 } else { 1.0 }))
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
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .justify_center()
                .child(
                    text(label_)
                        .number_of_lines(1)
                        .absolute()
                        .left(px(0.0))
                        .top(move || px(if floated() { -9.0 } else { 7.0 }))
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
                        .pointer_events(nsh_core::PointerEvents::None)
                        .transition()
                        .duration(EFFECTS_MS)
                        .ease_out(),
                )
                .child(
                    input()
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
                        .mt(px(14.0))
                        .text_size(px(16.0))
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
}

/// The line under a field: a problem in the error color, or nothing, in a height that is always there so the form does
/// not move.
pub fn field_note(
    message: impl Fn() -> Option<SharedString> + Copy + 'static,
    problem: impl Fn() -> bool + Copy + 'static,
) -> impl IntoElement {
    caption(text(move || message().unwrap_or_else(|| " ".into())))
        .px(px(16.0))
        .text_color(move || if problem() { theme().destructive } else { theme().muted_foreground })
}

// ---- Small parts ---------------------------------------------------------------------------------------------------

/// A filter chip: 32 pt, filled when selected, outlined otherwise.
pub fn chip(
    label_: impl Fn() -> SharedString + 'static,
    selected: impl Fn() -> bool + Copy + 'static,
    glyph: Option<Icon>,
) -> nsh_base::ButtonElement {
    button()
        .h(move || px(32.0) * font_scale())
        .pl(px(if glyph.is_some() { 8.0 } else { 16.0 }))
        .pr(px(16.0))
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .flex_shrink(0.0)
        .rounded(px(12.0))
        .border_1()
        .border_color(move || if selected() { theme().secondary } else { theme().border })
        .bg(move || if selected() { theme().secondary } else { theme().background.opacity(0.0) })
        .text_color(move || if selected() || glyph.is_some() { theme().foreground } else { theme().muted_foreground })
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
pub fn top_bar(s: Store, title_: Option<&'static str>, back: impl Fn() + 'static) -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .h(move || px(56.0) * font_scale())
        .px(px(4.0))
        .child(icon_button(move || s.label("Back"), || Icon::Back).text_color(move || theme().foreground).on_press(back))
        .child(when(
            move || title_.is_some(),
            move || title_large(text(move || s.label(title_.unwrap_or_default()))).number_of_lines(1).flex_1().text_center(),
        ))
        // The title stays centered: as wide as the back button.
        .child(when(move || title_.is_some(), || view().w(px(48.0))))
}

// ---- Rows, groups and choices --------------------------------------------------------------------------------------

/// The title over a group of rows.
pub fn section_title(text_: impl Fn() -> SharedString + 'static) -> impl IntoElement + Styled {
    label(text(text_)).text_color(move || theme().primary).pl(px(16.0)).pt(px(24.0)).pb(px(8.0))
}
/// A row of a group: a leading mark, a title over an optional line of detail, a trailing control. A row with an action
/// is pressed as a whole.
pub fn row(
    leading: impl IntoElement,
    title_: impl Fn() -> SharedString + 'static,
    detail: Option<Box<dyn Fn() -> SharedString>>,
    trailing: impl IntoElement,
) -> nsh_base::ButtonElement {
    let has_detail = detail.is_some();
    let detail = std::rc::Rc::new(detail);
    button()
        .min_h(move || px(if has_detail { 72.0 } else { 56.0 }) * font_scale())
        .px(px(16.0))
        .py(px(10.0))
        .flex_row()
        .items_center()
        .gap(px(16.0))
        .bg(move || theme().card)
        .active(|s| s.bg(move || tok("surface-high")))
        .child(leading)
        .child(view().flex_1().min_w(px(0.0)).child(body_large(text(title_))).child(when(
            move || has_detail,
            move || {
                let detail = detail.clone();
                dim(body(text(move || detail.as_ref().as_ref().map(|d| d()).unwrap_or_default())))
            },
        )))
        .child(trailing)
}
/// A round tile around a glyph: the leading mark of a settings row.
pub fn round_tile(glyph: Icon, destructive: bool) -> impl IntoElement {
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
/// Rows as one group: 24 pt outer corners, 6 pt between rows.
pub fn group(rows: Vec<AnyElement>) -> impl IntoElement + Styled {
    let count = rows.len();
    view().gap(px(GROUP_GAP)).children(rows.into_iter().enumerate().map(move |(index, row)| {
        view()
            .overflow_hidden()
            .rounded_t(px(if index == 0 { GROUP_RADIUS } else { GROUP_INNER }))
            .rounded_b(px(if index + 1 == count { GROUP_RADIUS } else { GROUP_INNER }))
            .child(row)
    }))
}
/// A switch: 52 by 32 pt.
pub fn switch(on: impl Fn() -> bool + Copy + 'static) -> impl IntoElement {
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
}
/// One of a few choices, side by side: 48 pt segments, 20 pt corners at the ends and 6 pt between.
pub fn choice<T: Copy + PartialEq + 'static>(
    options: Vec<(T, Box<dyn Fn() -> SharedString>)>,
    get: impl Fn() -> T + Copy + 'static,
    set: impl Fn(T) + Copy + 'static,
) -> impl IntoElement {
    let count = options.len();
    view().flex_row().gap(px(GROUP_GAP)).children(options.into_iter().enumerate().map(move |(index, (value, name))| {
        let on = move || get() == value;
        button()
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
                label(text(name)).number_of_lines(1).text_color(move || if on() { theme().primary_foreground } else { theme().foreground }),
            )
    }))
}
/// A tonal action: 56 pt, with a leading glyph.
pub fn tonal_button(
    glyph: impl Fn() -> Icon + Copy + 'static,
    label_: impl Fn() -> SharedString + 'static,
    destructive: bool,
) -> nsh_base::ButtonElement {
    button()
        .h(move || px(CONTROL) * font_scale())
        .px(px(16.0))
        .flex_row()
        .items_center()
        .justify_center()
        .gap(px(8.0))
        .bg(move || if destructive { tok("destructive-container") } else { theme().secondary })
        .text_color(move || if destructive { tok("destructive-container-foreground") } else { theme().foreground })
        .active(|s| s.opacity(0.85))
        .child(view().size(px(18.0)).child(dynamic(move || icon(glyph()).size(px(18.0)).into_element())))
        .child(label(text(label_)).number_of_lines(1))
}
/// An action of a dialog or a bar: text only, 40 pt.
pub fn text_button(label_: impl Fn() -> SharedString + 'static, destructive: bool) -> nsh_base::ButtonElement {
    button()
        .h(move || px(40.0) * font_scale())
        .px(px(12.0))
        .items_center()
        .justify_center()
        .rounded_full()
        .active(|s| s.bg(move || tok("pressed")))
        .child(label(text(label_)).text_color(move || if destructive { theme().destructive } else { theme().primary }))
}

// ---- Dialogs and sheets --------------------------------------------------------------------------------------------

/// A dialog over the page: a scrim and a panel with a title, its content and its actions, end-aligned. A press on the
/// scrim asks to close it.
pub fn dialog(
    title_: impl Fn() -> SharedString + 'static,
    content: impl IntoElement,
    actions: impl IntoElement,
    dismiss: impl Fn() + Copy + 'static,
) -> impl IntoElement {
    let scrim = view().flex_1().items_center().justify_center().px(px(27.0)).pt_safe_area();
    // Android resizes the dialog's window to end at the keyboard's top (as it does a sheet's, below), so the panel
    // centers above the keyboard already; making room for the keyboard as well lifted it that far again, its title and
    // focused field under the status bar.
    #[cfg(not(target_os = "android"))]
    let scrim = scrim.pb_keyboard();
    // A modal of its own: over the page and over a sheet that is open on it. The back key asks to close it.
    nsh_base::dialog().animation(nsh_core::ModalAnimation::Fade).on_cancel(move |_: &()| dismiss()).child(
        scrim.bg(move || tok("scrim")).on_press(move |_: &()| dismiss()).child(
            view()
                .w_full()
                .max_w(px(560.0))
                .rounded(px(28.0))
                .bg(move || tok("surface-high"))
                .shadow_lg()
                // A press inside stays inside.
                .on_press(|_: &()| {})
                .child(text(title_).text_size(px(22.0)).line_height(px(28.0)).px(px(24.0)).pt(px(22.0)).pb(px(12.0)))
                .child(view().px(px(24.0)).gap(px(8.0)).child(content))
                .child(view().flex_row().justify_end().gap(px(4.0)).px(px(12.0)).pt(px(12.0)).pb(px(12.0)).child(actions)),
        ),
    )
}
/// How tall a bottom sheet's scroll view may be, besides `chrome` (what the sheet shows around it, in points at the
/// default text size): the room between the status bar and the keyboard (the screen's bottom when it is down), less
/// the handle. A scroll view needs a height of its own: without one it is as tall as its content, nothing scrolls, and a
/// drag in it moves the sheet instead, which takes the keyboard down.
pub fn sheet_room(chrome: f32) -> Points {
    let keyboard = (keyboard_height() - safe_area().bottom).max(px(0.0));
    let bottom = if keyboard > px(0.0) { px(0.0) } else { safe_area().bottom };
    (window().height - safe_area().top - px(12.0) - keyboard - bottom - px(28.0) - px(chrome) * font_scale()).max(px(120.0))
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
    content: impl IntoElement,
) -> impl IntoElement {
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
        // Android: the sheet's window is resized to end at the keyboard's top (NSH's modal host asks the system for
        // that), so the panel is above the keyboard already; NSH lifting it by the keyboard's height as well put it
        // that far above, off the top of the screen.
        .popup_style(|s| if cfg!(target_os = "android") { s.bottom(px(0.0)) } else { s })
        .child(
            view()
                .rounded_t(px(28.0))
                .bg(move || theme().background)
                // Below the status bar, and above the keyboard while it is up: NSH lifts the panel to the keyboard's
                // top, so a panel taller than the room left would run off the top of the screen.
                .max_h(move || {
                    let keyboard = (keyboard_height() - safe_area().bottom).max(px(0.0));
                    window().height - safe_area().top - px(12.0) - keyboard
                })
                .overflow_hidden()
                .child(
                    view()
                        .items_center()
                        .py(px(12.0))
                        .child(view().w(px(32.0)).h(px(4.0)).rounded_full().bg(move || tok("outline").opacity(0.5))),
                )
                // The content gives way first: its scroll view takes what is left.
                .child(view().flex_shrink(1.0).min_h(px(0.0)).child(content)),
        )
}
