//! Per-desktop presentation. One layout, three native accents: macOS (the sidebar its release draws: floating on
//! macOS 26, flush elsewhere; the traffic lights over it), Windows (Fluent navigation pane with caption buttons on the right) and Linux
//! (libadwaita-style split view under a header bar). Behavior never forks here, only geometry.
use nsh_core::{platform::Os, prelude::*};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Look {
    Mac,
    Windows,
    Linux,
}
pub fn look() -> Look {
    match os() {
        Os::Windows => Look::Windows,
        Os::Linux => Look::Linux,
        _ => Look::Mac,
    }
}
/// Height of the app-drawn title bar row that also carries the toolbar.
pub fn toolbar_height() -> f32 {
    match look() {
        Look::Mac => 52.0,
        Look::Windows => 48.0,
        Look::Linux => 46.0,
    }
}
/// The sidebar floats as an inset panel on macOS 26; elsewhere it is flush with the window edge.
/// The left edge of the macOS traffic lights' close button in the main window's toolbar row, from the window's left
/// edge: 19 pt, where a system sidebar window has it on macOS 26 (11 pt inside
/// the floating sidebar) and on macOS 27 (Finder); centered in the 52 pt row. Set explicitly because AppKit's
/// own place for a toolbar-less window's buttons depends on the release: macOS 27 puts them 9 pt from the edge.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn traffic_lights_x() -> f64 {
    19.0
}
/// The sidebar as macOS draws its own (Finder, split views): a floating
/// rounded panel inset in the window on macOS 26, concentric with that release's window corners; a flush column
/// with a hairline separator on macOS 15 and earlier and on macOS 27 (whose window corners are smaller again).
/// An unknown version (0, headless tests) gets the flush one.
pub fn floating_sidebar() -> bool {
    look() == Look::Mac && os_version().0 == 26
}
/// Whether the window the calling view renders in has the keyboard focus (reading subscribes). macOS draws a selection
/// or a selected segment in an inactive window in gray, as AppKit's controls; unknown (headless, before the platform
/// reports, other desktops) counts as active. Call it while rendering: it finds the view's window then.
pub fn window_active() -> impl Fn() -> bool + Copy + 'static {
    let window = stored(nsh_core::element::desktop::current_window());
    move || look() != Look::Mac || window.get().state().is_none_or(|s| s.focused)
}
pub fn sidebar_inset() -> f32 {
    if floating_sidebar() { 8.0 } else { 0.0 }
}
pub fn sidebar_radius() -> f32 {
    if floating_sidebar() { 18.0 } else { 0.0 }
}
pub fn sidebar_width() -> f32 {
    match look() {
        Look::Mac => 210.0,
        Look::Windows => 240.0,
        Look::Linux => 220.0,
    }
}
pub fn control_height(large: bool) -> f32 {
    match (look(), large) {
        (Look::Mac, false) => 30.0,
        (Look::Mac, true) => 36.0,
        (Look::Windows, _) => 32.0,
        (Look::Linux, false) => 34.0,
        (Look::Linux, true) => 38.0,
    }
}
pub fn control_radius(large: bool) -> f32 {
    match look() {
        Look::Mac => {
            if large {
                8.0
            } else {
                6.0
            }
        }
        Look::Windows => 4.0,
        Look::Linux => 6.0,
    }
}
pub fn card_radius() -> f32 {
    match look() {
        Look::Mac => 10.0,
        Look::Windows => 8.0,
        Look::Linux => 12.0,
    }
}
pub fn sheet_radius() -> f32 {
    match look() {
        // macOS 26 sheets: 26pt continuous corners.
        Look::Mac => 26.0,
        Look::Windows => 8.0,
        Look::Linux => 14.0,
    }
}
pub fn sidebar_row_height() -> f32 {
    match look() {
        Look::Mac => 30.0,
        Look::Windows => 36.0,
        Look::Linux => 36.0,
    }
}
/// Space between footer buttons (the standard macOS spacing between controls).
pub fn footer_gap() -> f32 {
    if look() == Look::Mac { 8.0 } else { 10.0 }
}
