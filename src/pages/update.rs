//! The update window (`desktop::open_update`), laid out as macOS update windows are: the app icon beside what is new
//! and which version the user has, the version's notes (the manifest's Markdown, from CHANGELOG.md) in a box that
//! scrolls, the choice to download and install future updates on their own, and the buttons. Installing stays in the
//! window: the download's progress, then "Install and Relaunch".
use crate::{
    chrome::{Look, look},
    components::*,
    i18n::Language,
    pages::unlock::check_box,
    platform::updates::Phase,
    store::Store,
};
use nsh_base::{TextViewLinks, TextViewPart, TextViewParts, text_view, theme};
use nsh_core::{element::StyleRefinement, element::desktop::current_window, prelude::*};

/// `major.minor.patch` of a version string; `None` for anything else.
pub fn version_numbers(version: &str) -> Option<(u32, u32, u32)> {
    let mut parts = version.trim().trim_start_matches('v').split('.').map(|p| p.parse::<u32>().ok());
    let numbers = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(numbers)
}
/// The part of a version's notes in `language`. The manifest carries the English notes, then each translation after a
/// `<!-- lang: <code> -->` line (`scripts/release.py`); English stands in for a language the notes lack.
pub fn notes_in(notes: &str, language: Language) -> String {
    let wanted = crate::i18n::code(language);
    let mut parts: Vec<(&str, Vec<&str>)> = vec![("en", Vec::new())];
    for line in notes.lines() {
        let marker = line.trim().strip_prefix("<!-- lang:").and_then(|rest| rest.strip_suffix("-->")).map(str::trim);
        match marker {
            Some(code) => parts.push((code, Vec::new())),
            None => parts.last_mut().expect("the English part").1.push(line),
        }
    }
    let text = |lines: &[&str]| lines.join("\n").trim().to_string();
    parts
        .iter()
        .find(|(code, lines)| *code == wanted && !text(lines).is_empty())
        .or_else(|| parts.first())
        .map(|(_, lines)| text(lines))
        .unwrap_or_default()
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
    // The notes in the window's language, following it when it changes.
    let localized = signal(None::<SharedString>);
    effect(move || {
        let next: Option<SharedString> = notes.as_deref().map(|n| notes_in(n, s.language.get())).filter(|n| !n.is_empty()).map(Into::into);
        if untracked(|| localized.get()) != next {
            localized.set(next);
        }
    });
    let skippable = version_numbers(&version);
    // Installed where the user cannot write: the update is a download, and nothing installs on its own.
    let read_only = crate::platform::updates_read_only();
    let subtitle = s.fmt("Safay {} is now available—you have {}. Would you like to download it now?", &[&version, current]);
    provide_context(parts());
    // On macOS the window has no title bar: everything but the notes and the controls drags it, below the traffic lights.
    let mac = look() == Look::Mac;
    view()
        .flex_1()
        .window_drag(mac)
        .bg(move || tok("chrome"))
        .pt(px(if mac { 36.0 } else { 20.0 }))
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
                .child(
                    when(
                        move || localized.with(Option::is_some),
                        move || {
                            scroll().flex_1().child(
                                text_view()
                                    .markdown(move || localized.get())
                                    .links(TextViewLinks::Open)
                                    .gap(px(10.0))
                                    .p(px(16.0))
                                    .text_size(px(13.0))
                                    .text_color(move || theme().foreground),
                            )
                        },
                    )
                    .otherwise(move || {
                        view().flex_1().items_center().justify_center().child(note(move || s.label("This update has no release notes.")))
                    }),
                ),
        )
        .child(when(
            move || read_only,
            move || text(move || s.label(crate::store::READ_ONLY_UPDATE)).text_size(px(12.0)).text_color(move || theme().muted_foreground),
        ))
        .child(when(
            move || !read_only,
            move || {
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
                )
            },
        ))
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
                                            _ if read_only => "Download",
                                            Phase::Ready { .. } => "Install and Relaunch",
                                            Phase::Failed(_) => "Try Again",
                                            _ => "Install Update",
                                        })
                                    },
                                    Kind::Primary,
                                    false,
                                )
                                .on_press(move || match phase() {
                                    _ if read_only => {
                                        s.open_update_download();
                                        close();
                                    }
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
    #[test]
    fn notes_show_in_the_apps_language_or_in_english() {
        let notes = "Intro\n\n- **A**: one\n\n<!-- lang: zh-Hans -->\n\nHans\n\n<!-- lang: zh-Hant -->\n\nHant\n";
        assert_eq!(notes_in(notes, Language::En), "Intro\n\n- **A**: one");
        assert_eq!(notes_in(notes, Language::Hans), "Hans");
        assert_eq!(notes_in(notes, Language::Hant), "Hant");
        // Notes without translations, or with an empty one, fall back to English.
        assert_eq!(notes_in("Only English", Language::Hans), "Only English");
        assert_eq!(notes_in("English\n<!-- lang: zh-Hans -->\n\n", Language::Hans), "English");
    }
}
