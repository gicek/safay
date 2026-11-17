//! The update window (`desktop::open_update`), laid out as macOS update windows are: the app icon beside what is new
//! and which version the user has, the version's notes (the manifest's Markdown, from CHANGELOG.md) in a box that
//! scrolls, the choice to download and install future updates on their own, and the buttons. Installing stays in the
//! window: the download's progress, then "Install and Relaunch".
use crate::{components::*, pages::unlock::check_box, platform::updates::Phase, store::Store};
use nsh_base::{TextViewLinks, TextViewPart, TextViewParts, text_view, theme};
use nsh_core::{element::StyleRefinement, element::desktop::current_window, prelude::*};

/// `major.minor.patch` of a version string; `None` for anything else.
pub fn version_numbers(version: &str) -> Option<(u32, u32, u32)> {
    let mut parts = version.trim().trim_start_matches('v').split('.').map(|p| p.parse::<u32>().ok());
    let numbers = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(numbers)
}
/// Whether `version` is the one the user chose to skip.
pub fn is_skipped(version: &str, skipped: Option<(u32, u32, u32)>) -> bool {
    skipped.is_some() && version_numbers(version) == skipped
}

/// The notes' typography: the window's 13 pt text, the section headings ("Changed", "Fixed") as small bold labels.
fn parts() -> TextViewParts {
    use TextViewPart as P;
    TextViewParts::new()
        .part(P::Heading, |s: StyleRefinement| s.text_size(px(13.0)).line_height(px(18.0)).font_weight(FontWeight::SEMIBOLD))
        .part(P::Paragraph, |s: StyleRefinement| s.line_height(px(19.0)))
        .part(P::ListItem, |s: StyleRefinement| s.line_height(px(19.0)))
}

pub fn page(s: Store, version: String, notes: Option<String>) -> impl IntoElement {
    let this = stored(current_window());
    let close = move || this.get().close();
    let current = env!("CARGO_PKG_VERSION");
    let phase = move || s.updates.get().unwrap_or_default();
    // While the window shows, the download's progress is read more often than the store's once a second.
    set_interval(std::time::Duration::from_millis(250), move || {
        if matches!(phase(), Phase::Downloading { .. } | Phase::Installing) {
            s.refresh_updates();
        }
    });
    let notes: Option<SharedString> = notes.filter(|n| !n.trim().is_empty()).map(Into::into);
    let skippable = version_numbers(&version);
    let subtitle = s.fmt("Safay {} is now available—you have {}. Would you like to download it now?", &[&version, current]);
    provide_context(parts());
    // The window has no title bar: everything but the notes and the controls drags it.
    view()
        .flex_1()
        .window_drag(true)
        .bg(move || tok("chrome"))
        .pt(px(36.0))
        .px(px(24.0))
        .pb(px(20.0))
        .gap(px(16.0))
        .child(
            view().flex_row().gap(px(16.0)).child(app_icon(64.0).rounded(px(14.0))).child(
                view()
                    .flex_1()
                    .min_w(px(0.0))
                    .justify_center()
                    .gap(px(6.0))
                    .child(
                        text(move || s.label("A new version of Safay is available!")).text_size(px(15.0)).font_weight(FontWeight::SEMIBOLD),
                    )
                    .child(text(subtitle).text_size(px(13.0)).line_height(px(18.0))),
            ),
        )
        .child(
            view()
                .flex_1()
                .min_h(px(0.0))
                .window_drag(false)
                .rounded(px(8.0))
                .border(px(1.0))
                .border_color(move || tok("card-border"))
                .bg(move || tok("card"))
                .overflow_hidden()
                .child(match notes {
                    Some(notes) => scroll()
                        .flex_1()
                        .child(
                            text_view()
                                .markdown(move || Some(notes.clone()))
                                .links(TextViewLinks::Open)
                                .gap(px(10.0))
                                .p(px(16.0))
                                .text_size(px(13.0))
                                .text_color(move || theme().foreground),
                        )
                        .into_element(),
                    None => view()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .child(note(move || s.label("This update has no release notes.")))
                        .into_element(),
                }),
        )
        .child(
            view().flex_row().self_start().window_drag(false).child(
                nsh_base::checkbox()
                    .checked(move || s.preferences.with(|p| p.update_checks == Some(true) && p.update_downloads))
                    .on_change(move |v: &bool| {
                        let v = *v;
                        s.update_preferences(move |p| {
                            if v {
                                p.update_checks = Some(true);
                            }
                            p.update_downloads = v;
                        })
                    })
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .indicator(check_box(move || s.preferences.with(|p| p.update_checks == Some(true) && p.update_downloads)))
                    .child(text(move || s.label("Automatically download and install updates in the future")).text_size(px(13.0))),
            ),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(10.0))
                .h(px(32.0))
                .window_drag(false)
                .child(when(move || matches!(phase(), Phase::Downloading { .. } | Phase::Installing), move || progress(s, phase)))
                .child(when(
                    move || !matches!(phase(), Phase::Downloading { .. } | Phase::Installing),
                    move || {
                        view()
                            .flex_1()
                            .flex_row()
                            .items_center()
                            .gap(px(10.0))
                            .child(when(
                                move || skippable.is_some() && !matches!(phase(), Phase::Ready { .. }),
                                move || {
                                    push_button(move || s.label("Skip This Version"), Kind::Standard, false).on_press(move || {
                                        s.update_preferences(move |p| p.skipped_update = skippable);
                                        close();
                                    })
                                },
                            ))
                            .child(
                                text(move || match phase() {
                                    Phase::Failed(f) => s.label(crate::store::update_failure_key(f)),
                                    Phase::Ready { .. } => s.label("The update is ready. Relaunch Safay to finish installing it."),
                                    _ => SharedString::default(),
                                })
                                .flex_1()
                                .min_w(px(0.0))
                                .number_of_lines(2)
                                .text_size(px(12.0))
                                .text_color(move || {
                                    if matches!(phase(), Phase::Failed(_)) { theme().destructive } else { theme().muted_foreground }
                                }),
                            )
                            // Once downloaded, the one choice left is relaunching; closing the window installs on quit.
                            .child(when(
                                move || !matches!(phase(), Phase::Ready { .. }),
                                move || push_button(move || s.label("Remind Me Later"), Kind::Standard, false).on_press(close),
                            ))
                            .child(
                                push_button(
                                    move || {
                                        s.label(match phase() {
                                            Phase::Ready { .. } => "Install and Relaunch",
                                            Phase::Failed(_) => "Try Again",
                                            _ => "Install Update",
                                        })
                                    },
                                    Kind::Primary,
                                    false,
                                )
                                .on_press(move || match phase() {
                                    Phase::Ready { .. } => s.restart_to_update(),
                                    _ => s.download_update(false),
                                }),
                            )
                    },
                )),
        )
}

/// The download's bar and how far it is.
fn progress(s: Store, phase: impl Fn() -> Phase + Copy + 'static) -> impl IntoElement {
    let fraction = move || match phase() {
        Phase::Downloading { received, total: Some(total) } if total > 0 => (received as f32 / total as f32).clamp(0.0, 1.0),
        Phase::Installing => 1.0,
        _ => 0.0,
    };
    let label = move || -> SharedString {
        match phase() {
            Phase::Downloading { received, total: Some(total) } if total > 0 => {
                s.fmt("Downloading… {}%", &[&(received * 100 / total).min(100).to_string()])
            }
            Phase::Installing => s.label("Installing…"),
            _ => s.label("Downloading…"),
        }
    };
    view()
        .flex_1()
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .child(
            view()
                .flex_1()
                .h(px(6.0))
                .rounded_full()
                .bg(move || theme().foreground.opacity(0.1))
                .overflow_hidden()
                .child(view().h_full().rounded_full().w(move || relative(fraction())).bg(move || theme().primary)),
        )
        .child(text(label).text_size(px(12.0)).text_color(move || theme().muted_foreground))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn versions_compare_by_their_numbers() {
        assert_eq!(version_numbers("1.10.2"), Some((1, 10, 2)));
        assert_eq!(version_numbers("v2.0.0"), Some((2, 0, 0)));
        assert_eq!(version_numbers("1.0"), None);
        assert_eq!(version_numbers("1.0.0-beta"), None);
        assert!(is_skipped("1.1.0", Some((1, 1, 0))));
        assert!(!is_skipped("1.1.1", Some((1, 1, 0))));
        assert!(!is_skipped("1.1.0", None));
    }
}
