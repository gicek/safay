//! The list: a search bar, the categories as chips, and the accounts as one group of rows. A press on a row copies its
//! code; the button at the bottom adds an account.
use super::{Nav, Page, ui};
use crate::{
    components::{CODE_FONT, badge_with_icon, tok},
    icons::{Icon, icon},
    store::Store,
};
use nsh_base::{button, input, theme};
use nsh_core::prelude::*;
use safay_app::session::Row;

/// A row: 14 pt above and below the two lines of its names.
const ROW: f32 = 72.0;
/// What the list leaves free at its end, so its last row scrolls clear of the add button.
const CLEARANCE: f32 = 120.0;

pub fn page(s: Store, nav: Nav) -> impl IntoElement {
    let empty_vault = move || s.projection.with(|p| p.account_count == 0);
    view()
        .flex_1()
        .pt_safe_area()
        .child(search_bar(s, nav))
        .child(when(move || !empty_vault(), move || chips(s, nav)))
        .child(
            when(empty_vault, move || empty(s, nav)).otherwise(move || {
                when(move || s.projection.with(|p| p.rows.is_empty()), move || nothing(s)).otherwise(move || list(s, nav))
            }),
        )
        .child(add_menu(s, nav))
        .child(overflow(s, nav))
        .child(super::menus::account(s, nav))
}

fn search_bar(s: Store, nav: Nav) -> impl IntoElement {
    let field = element_ref();
    view()
        .flex_row()
        .items_center()
        .mx(px(ui::MARGIN))
        .my(px(8.0))
        .h(move || px(ui::CONTROL) * font_scale())
        .pl(px(16.0))
        .pr(px(4.0))
        .rounded_full()
        .bg(move || theme().card)
        .on_press(move |_: &()| field.focus())
        .child(view().text_color(move || theme().muted_foreground).child(icon(Icon::Search).size(px(24.0))))
        .child(
            input()
                .element_ref(&field)
                .test_id("search")
                .label(move || s.label("Search accounts or sites"))
                .placeholder(move || s.label("Search accounts or sites").to_string())
                .value(move || s.query.get())
                .on_change(move |v: &str| {
                    s.controller.get().activity();
                    s.query.set(v.to_owned());
                    s.refresh();
                })
                .return_key_type(ReturnKeyType::Search)
                .auto_correct(false)
                .auto_capitalize(nsh_core::AutoCapitalize::None)
                .auto_fill(nsh_core::AutoFill::Off)
                .flex_1()
                .min_w(px(0.0))
                .mx(px(12.0))
                .text_size(px(16.0))
                .text_color(move || theme().foreground),
        )
        .child(when(
            move || !s.query.get().is_empty(),
            move || {
                ui::icon_button(move || s.label("Clear Search"), || Icon::Close).on_press(move || {
                    s.query.set(String::new());
                    s.refresh();
                })
            },
        ))
        // The glyph is the row of dots, upright here.
        .child(ui::icon_button(move || s.label("More"), || Icon::More).rotate(|| std::f32::consts::FRAC_PI_2).test_id("more").on_press(
            move || {
                dismiss_keyboard();
                nav.menu.set(true)
            },
        ))
}

fn chips(s: Store, nav: Nav) -> impl IntoElement {
    let select = move |id| {
        s.category.set(id);
        s.refresh();
    };
    // Chips scroll sideways with the finger; no indicator under them.
    scroll().horizontal().scrollbar(false).flex_shrink(0.0).child(
        view()
            .flex_row()
            .gap(px(8.0))
            .px(px(ui::MARGIN))
            .pb(px(4.0))
            .child(ui::chip(move || s.label("All"), move || s.category.get().is_none(), None).on_press(move || select(None)))
            .child(each(
                move || s.projection.with(|p| p.categories.clone()),
                |c| c.id,
                move |c, _| {
                    let id = c.with_untracked(|c| c.id);
                    ui::chip(move || c.with(|c| SharedString::from(c.name.clone())), move || s.category.get() == Some(id), None)
                        .on_press(move || select(Some(id)))
                },
            ))
            .child(
                ui::chip(move || s.label("Manage"), || false, Some(Icon::Sliders))
                    .test_id("manage")
                    .on_press(move || nav.push(Page::Categories)),
            ),
    )
}

fn list(s: Store, nav: Nav) -> impl IntoElement {
    let count = move || s.projection.with(|p| p.rows.len());
    virtual_list(count, move |at| {
        // The position is read in the bindings, not once here: a cell keeps its content as rows come and go or the
        // list scrolls, and shows whichever row is now at its place (its data, its corners).
        let row = computed(move || s.projection.with(|p| p.rows.get(at.index()).cloned()));
        account_row(s, nav, row, move || at.index(), count)
    })
    .keys(move || s.projection.with(|p| p.rows.iter().map(|r| r.id.to_string()).collect::<Vec<_>>()))
    .estimated_item_size(move || px(ROW) * font_scale())
    .row_gap(px(ui::GROUP_GAP))
    .content_padding_x(px(ui::MARGIN))
    .content_padding_y(px(8.0))
    .footer(view().h(px(CLEARANCE)))
    .on_scroll(move |_: &ScrollEvent| s.controller.get().activity())
    .keyboard_dismiss_mode(KeyboardDismissMode::OnDrag)
    .flex_1()
}

fn account_row(
    s: Store,
    nav: Nav,
    row: Memo<Option<Row>>,
    index: impl Fn() -> usize + Copy + 'static,
    count: impl Fn() -> usize + Copy + 'static,
) -> impl IntoElement {
    let id = move || row.with(|r| r.as_ref().map(|r| r.id));
    let issuer = move || row.with(|r| r.as_ref().map(|r| r.issuer.clone()).unwrap_or_default());
    let urgent = move || row.with(|r| r.as_ref().is_some_and(|r| r.remaining <= 5));
    let copied = move || s.copied.get().is_some() && s.copied.get() == id();
    ui::group_corners(
        // A view, not a button: a row takes a press and a long press.
        view()
            .accessible(true)
            .role(nsh_core::AccessibilityRole::Button)
            .label(move || s.fmt("Copy code for {}", &[&issuer()]))
            .min_h(move || px(ROW) * font_scale())
            .px(px(16.0))
            .py(px(14.0))
            .flex_row()
            .items_center()
            .gap(px(14.0))
            // The flash of a copy: the container color at once, back to the row's own as it fades.
            .bg(move || if copied() { tok("primary-container") } else { theme().card })
            .transition()
            .duration(move || if copied() { 0.0 } else { ui::EFFECTS_MS })
            .ease_out()
            .active(|st| st.bg(move || tok("surface-high")))
            .on_press(move |_: &()| {
                if let Some(id) = id() {
                    haptic(Haptic::Notification(HapticNotification::Success));
                    s.copy(id)
                }
            })
            .on_long_press(move |e: &PressEvent| {
                if let Some(id) = id() {
                    haptic(Haptic::Impact(HapticImpact::Medium));
                    dismiss_keyboard();
                    nav.context.set(Some((id, e.page_position)))
                }
            }),
        index,
        count,
    )
    .child(badge_with_icon(issuer, || false, move || row.with(|r| r.as_ref().and_then(|r| r.icon.clone())), 42.0))
    .child(view().flex_1().min_w(px(0.0)).child(ui::title(text(issuer)).number_of_lines(1)).child(
        ui::dim(ui::body(text(move || row.with(|r| r.as_ref().map(|r| r.username.clone()).unwrap_or_default())))).number_of_lines(1),
    ))
    .child(
        text(move || row.with(|r| r.as_ref().map(|r| r.code.clone()).unwrap_or_default()))
            .font_family(CODE_FONT.family)
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(px(24.0))
            .letter_spacing(px(0.5))
            .number_of_lines(1)
            .flex_shrink(0.0)
            .text_color(move || if urgent() { tok("urgent") } else { theme().foreground }),
    )
    .child(ring(move || row.with(|r| r.as_ref().map(|r| (r.remaining, r.period))), 26.0))
}

/// The countdown: a track, an arc that shortens clockwise from the top, and the seconds left inside. Orange in the last
/// five seconds.
fn ring(row: impl Fn() -> Option<(u32, u32)> + Copy + 'static, size: f32) -> impl IntoElement {
    let full = size + 2.5;
    let r = size / 2.0;
    let circle = move |extra: String| {
        nsh_core::element::ImageSrc::svg_markup(&format!(
            "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 {full} {full}' fill='none' stroke-width='2.5'><circle cx='{c}' cy='{c}' r='{r}' stroke='currentColor' {extra}/></svg>",
            c = full / 2.0
        ))
    };
    let track = circle(String::new());
    let urgent = move || row().is_some_and(|(left, _)| left <= 5);
    let ink = move || if urgent() { tok("urgent") } else { theme().primary };
    view()
        .size(px(size))
        .flex_shrink(0.0)
        .items_center()
        .justify_center()
        .child(svg(move || track.clone()).absolute().left(px(-1.25)).top(px(-1.25)).size(px(full)).text_color(move || theme().border))
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
            .text_color(ink),
        )
        .child(
            text(move || SharedString::from(row().map(|(left, _)| left.to_string()).unwrap_or_default()))
                .text_size(px(size * 0.4))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(move || if urgent() { tok("urgent") } else { theme().muted_foreground }),
        )
}

/// No account yet: what to do first.
fn empty(s: Store, nav: Nav) -> impl IntoElement {
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .p(px(32.0))
        .pb_safe_area_plus(px(32.0))
        .child(view().text_color(move || theme().primary).child(icon(Icon::QrScan).size(px(56.0))))
        .child(ui::title_large(text(move || s.label("No 2FA accounts yet"))).text_center().mt(px(16.0)))
        .child(
            ui::dim(ui::body(text(move || s.label("Enter the setup key from a service. Codes are encrypted on this device."))))
                .text_center()
                .mt(px(8.0)),
        )
        .child(
            view().mt(px(24.0)).child(
                ui::filled_button(Some(Icon::Keyboard), move || s.label("Enter Setup Key"))
                    .test_id("enter-key")
                    .on_press(move || nav.push(Page::Manual)),
            ),
        )
}

/// Accounts there are, none to show: nothing matches, or the category is empty.
fn nothing(s: Store) -> impl IntoElement {
    let searching = move || !s.query.get().trim().is_empty();
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .p(px(32.0))
        // Centered in what the keyboard leaves: a search with no match is typed with the keyboard up.
        .pb_keyboard_plus(px(32.0))
        .child(
            view()
                .size(px(48.0))
                .text_color(move || theme().muted_foreground)
                .child(dynamic(move || icon(if searching() { Icon::Search } else { Icon::Folder }).size(px(48.0)).into_element())),
        )
        .child(
            ui::title(text(move || {
                if searching() {
                    return s.label("No matching accounts");
                }
                let name = s.projection.with(|p| p.categories.iter().find(|c| Some(c.id) == s.category.get()).map(|c| c.name.clone()));
                s.fmt("No accounts in “{}” yet", &[&name.unwrap_or_default()])
            }))
            .text_center()
            .mt(px(12.0)),
        )
}

/// The add button at the bottom, and what it opens over a scrim.
fn add_menu(s: Store, nav: Nav) -> impl IntoElement {
    let open = move || nav.adding.get();
    view()
        .absolute()
        .inset(px(0.0))
        .pointer_events(nsh_core::PointerEvents::BoxNone)
        .child(
            view()
                .absolute()
                .inset(px(0.0))
                .bg(move || tok("scrim"))
                .opacity(move || if open() { 1.0_f32 } else { 0.0 })
                .pointer_events(move || if open() { nsh_core::PointerEvents::Auto } else { nsh_core::PointerEvents::None })
                .transition()
                .duration(ui::EFFECTS_MS)
                .ease_out()
                .on_press(move |_: &()| nav.adding.set(false)),
        )
        .child(
            view()
                .absolute()
                .right(px(ui::MARGIN))
                .bottom(px(0.0))
                .pb_safe_area_plus(px(ui::MARGIN))
                .items_end()
                .gap(px(8.0))
                .pointer_events(nsh_core::PointerEvents::BoxNone)
                .child(when(open, move || {
                    view()
                        .items_end()
                        .gap(px(8.0))
                        .child(add_item(s, Icon::Photo, "Choose from Photos", move || {
                            nav.adding.set(false);
                            s.scanned.set(Default::default());
                            s.choose_images()
                        }))
                        .child(add_item(s, Icon::Keyboard, "Enter Setup Key", move || nav.push(Page::Manual)))
                        .child(add_item(s, Icon::Doc, "Import Backup File", move || {
                            nav.adding.set(false);
                            s.choose_backup()
                        }))
                }))
                .child(
                    button()
                        .test_id("add")
                        .accessibility_label(move || Some(s.label(if open() { "Close" } else { "Add Account" })))
                        .size(px(56.0))
                        .items_center()
                        .justify_center()
                        .rounded(move || px(if open() { 28.0 } else { 16.0 }))
                        .bg(move || if open() { theme().primary } else { tok("primary-container") })
                        .text_color(move || if open() { theme().primary_foreground } else { tok("primary-container-foreground") })
                        .shadow_lg()
                        .transition()
                        .duration(ui::SPATIAL_MS)
                        .ease_out()
                        .active(|st| st.scale(0.96))
                        .on_press(move || {
                            dismiss_keyboard();
                            nav.adding.update(|v| *v = !*v)
                        })
                        .child(
                            view()
                                .rotate(move || if open() { std::f32::consts::FRAC_PI_4 } else { 0.0 })
                                .transition()
                                .duration(ui::SPATIAL_MS)
                                .ease_out()
                                .child(icon(Icon::Plus).size(px(24.0))),
                        ),
                ),
        )
}
fn add_item(s: Store, glyph: Icon, label_: &'static str, press: impl Fn() + 'static) -> impl IntoElement {
    button()
        .test_id(label_)
        .h(px(56.0))
        .pl(px(20.0))
        .pr(px(24.0))
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .rounded_full()
        .bg(move || tok("primary-container"))
        .text_color(move || tok("primary-container-foreground"))
        .shadow_md()
        .active(|st| st.opacity(0.9))
        .on_press(press)
        .child(icon(glyph).size(px(24.0)))
        .child(ui::title(text(move || s.label(label_))))
}

/// The search bar's menu, under its button.
fn overflow(s: Store, nav: Nav) -> impl IntoElement {
    when(
        move || nav.menu.get(),
        move || {
            view().absolute().inset(px(0.0)).on_press(move |_: &()| nav.menu.set(false)).child(
                view()
                    .absolute()
                    .right(px(ui::MARGIN))
                    .top(px(0.0))
                    .mt_safe_area_plus(px(8.0 + ui::CONTROL - 4.0))
                    .min_w(px(200.0))
                    .py(px(8.0))
                    .rounded(px(16.0))
                    .bg(move || tok("surface-high"))
                    .shadow_lg()
                    .child(menu_item(s, Icon::Settings, "Settings", move || nav.push(Page::Settings)))
                    .child(menu_item(s, Icon::Sliders, "Manage Categories", move || nav.push(Page::Categories)))
                    .child(menu_item(s, Icon::Lock, "Lock Now", move || {
                        nav.menu.set(false);
                        s.lock()
                    })),
            )
        },
    )
}
fn menu_item(s: Store, glyph: Icon, label_: &'static str, press: impl Fn() + 'static) -> impl IntoElement {
    button()
        .test_id(label_)
        .h(px(48.0))
        .px(px(12.0))
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .active(|st| st.bg(move || tok("pressed")))
        .on_press(press)
        .child(view().text_color(move || theme().muted_foreground).child(icon(glyph).size(px(24.0))))
        .child(ui::label(text(move || s.label(label_))))
}
