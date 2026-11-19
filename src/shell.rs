//! The main window: an app-drawn title bar that carries the toolbar, a sidebar and the content,
//! with the sheets over it. Platform chrome differs only through `chrome`.
use crate::{
    chrome::{self, Look, look},
    components::*,
    icons::Icon,
    pages,
    store::{Check, Sheet, Store, Surface},
};
use nsh_base::theme;
use nsh_core::{
    desktop::{TitleBarOptions, TitleBarStyle, WindowAttributes},
    element::desktop::{MenuItem, main_window as native_window, set_application_menu, title_bar},
    prelude::*,
};
use safay_app::controller::Controller;
use std::sync::Arc;

/// The main window draws its own title bar, `height` tall (the platform's own height without it); `controls` puts the
/// macOS traffic lights' top-left corner there (AppKit's place without it).
pub(crate) fn hide_title_bar(height: Option<Points>, controls: Option<Point<Points>>) {
    let mut bar = TitleBarOptions::default();
    bar.style = TitleBarStyle::Hidden;
    bar.height = height;
    bar.controls_offset = controls;
    let mut attributes = WindowAttributes::default();
    attributes.title_bar = Some(bar);
    native_window().set(attributes);
}

pub fn main_window(controller: Arc<Controller>, fixture: bool) -> impl IntoElement {
    let s = Store::new(controller, fixture, Surface::Main);
    effect(move || untracked(|| s.start()));
    #[cfg(debug_assertions)]
    if fixture {
        crate::demo::scene(s);
    }
    // The lock screen has no toolbar: the system's own title bar, so the traffic lights sit at the system's place on each
    // macOS release. Elsewhere the toolbar row holds them at their place in the sidebar.
    effect(move || {
        let locked = s.ready.get() && !s.unlocked() && s.projection.with(|p| p.exists);
        let system = locked && look() == Look::Mac;
        let height = (!system).then(chrome::toolbar_height);
        // The traffic lights sit centered in the toolbar row, their left edge at `chrome::traffic_lights_x`.
        #[cfg(target_os = "macos")]
        let controls = height.filter(|_| look() == Look::Mac).map(|h| {
            let y = ((f64::from(h) - crate::platform::macos::traffic_light_height()) / 2.0).round();
            point(px(chrome::traffic_lights_x() as f32), px(y as f32))
        });
        #[cfg(not(target_os = "macos"))]
        let controls = None;
        hide_title_bar(height.map(px), controls);
    });
    // iCloud backup changes arrive as silent pushes while iCloud is the location (subscribed once it is, removed when it
    // stops being it).
    let icloud = signal(None::<bool>);
    effect(move || {
        if !s.ready.get() {
            return;
        }
        let now = s.icloud_linked();
        let before = untracked(|| icloud.get());
        if before == Some(now) {
            return;
        }
        icloud.set(Some(now));
        if now {
            s.subscribe_icloud();
        } else if before == Some(true) {
            s.unsubscribe_icloud();
        }
    });
    effect(move || {
        let unlocked = s.unlocked();
        let language = s.language.get();
        let _ = language;
        untracked(|| app_menu(s, unlocked));
    });
    on_key("mod+n", move |_| {
        if s.unlocked() && s.sheet.get().is_none() {
            s.open_sheet(Sheet::Add)
        }
    });
    on_key("mod+f", move |_| {
        if s.unlocked() && s.sheet.get().is_none() {
            s.focus_search()
        }
    });
    on_key("mod+,", move |_| open_settings(s));
    // Requests from the Settings window, the tray panel or a global shortcut; only a sheet kind crosses stores.
    set_interval(std::time::Duration::from_millis(200), move || {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        if let Some(from_shortcut) = crate::platform::take_capture_request() {
            s.capture_and_import(from_shortcut);
        }
        if crate::platform::take_update_check_request() {
            s.check_updates(Check::Manual);
        }
        // Another device changed the iCloud backup (silent push): sync now; while locked, the sync after
        // unlocking picks it up.
        #[cfg(target_os = "macos")]
        if crate::platform::macos::take_icloud_change() && s.icloud_linked() && s.unlocked() && !s.syncing.get() {
            s.sync_now(safay_app::sync::Source::Auto);
        }
        #[cfg(target_os = "macos")]
        if let Some(id) = crate::platform::take_launcher_copy() {
            s.launcher_copy_request(id);
        }
        s.launcher_copy_after_unlock();
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        s.capture_after_unlock();
        if let Some(sheet) = crate::platform::take_request()
            && (s.unlocked() || sheet == Sheet::Settings)
        {
            s.open_sheet(sheet)
        }
    });
    view()
        .flex_1()
        .font_family("system-ui")
        .font_weight(FontWeight::NORMAL)
        .text_size(px(13.0))
        .bg(move || if look() == Look::Windows { tok("chrome") } else { theme().background })
        .text_color(move || theme().foreground)
        .on_press(move |_: &()| s.controller.get().activity())
        .child(when(move || !s.ready.get(), move || view().flex_1().window_drag(true)).otherwise(move || {
            when(move || s.unlocked(), move || unlocked(s)).otherwise(move || {
                when(move || s.projection.with(|p| p.exists), move || pages::unlock::lock_screen(s))
                    .otherwise(move || pages::unlock::onboarding(s))
            })
        }))
        .child(pages::sheets::sheets(s))
        .child(pages::sheets::auth_dialog(s))
        .child(toast(s))
}

pub fn open_settings(s: Store) {
    #[cfg(target_os = "macos")]
    if crate::desktop::installed() {
        crate::desktop::open_settings();
        return;
    }
    s.open_sheet(Sheet::Settings);
}

fn app_menu(s: Store, unlocked: bool) {
    let l = move |k: &str| s.label(k).to_string();
    let mut app = vec![MenuItem::role(nsh_core::desktop::MenuRole::About).label(l("About Safay"))];
    // Right below About, in every build; disabled where this build doesn't update itself.
    if crate::platform::supports::UPDATES {
        let configured = crate::platform::updates_configured();
        app.push(MenuItem::new(l("Check for Updates")).disabled(!configured).on_select(move || s.check_updates(Check::Manual)));
    }
    app.push(MenuItem::separator());
    app.push(MenuItem::new(l("Settings")).shortcut("mod+,").on_select(move || open_settings(s)));
    app.push(MenuItem::separator());
    if look() == Look::Mac {
        app.push(MenuItem::role(nsh_core::desktop::MenuRole::Hide));
        app.push(MenuItem::role(nsh_core::desktop::MenuRole::HideOthers));
        app.push(MenuItem::role(nsh_core::desktop::MenuRole::ShowAll));
        app.push(MenuItem::separator());
    }
    app.push(MenuItem::role(nsh_core::desktop::MenuRole::Quit).label(l("Quit")));
    let file = vec![
        MenuItem::new(l("Add Account")).shortcut("mod+n").disabled(!unlocked).on_select(move || s.open_sheet(Sheet::Add)),
        MenuItem::separator(),
        MenuItem::new(l("Import / Export")).disabled(!unlocked).on_select(move || s.open_sheet(Sheet::ExportImport)),
        MenuItem::new(l("Backup & Sync")).disabled(!unlocked).on_select(move || s.open_sheet(Sheet::Backup)),
        MenuItem::new(l("Lock")).shortcut("mod+l").disabled(!unlocked).on_select(move || s.lock()),
        MenuItem::separator(),
        MenuItem::role(nsh_core::desktop::MenuRole::Close),
    ];
    let edit = vec![
        MenuItem::role(nsh_core::desktop::MenuRole::Undo),
        MenuItem::role(nsh_core::desktop::MenuRole::Redo),
        MenuItem::separator(),
        MenuItem::role(nsh_core::desktop::MenuRole::Cut),
        MenuItem::role(nsh_core::desktop::MenuRole::Copy),
        MenuItem::role(nsh_core::desktop::MenuRole::Paste),
        MenuItem::role(nsh_core::desktop::MenuRole::SelectAll),
        MenuItem::separator(),
        MenuItem::new(l("Find")).shortcut("mod+f").disabled(!unlocked).on_select(move || s.focus_search()),
    ];
    let view_menu = vec![
        MenuItem::new(l("Cards"))
            .checked(s.cards.get_untracked())
            .disabled(!unlocked)
            .on_select(move || s.update_preferences(|p| p.cards = true)),
        MenuItem::new(l("List"))
            .checked(!s.cards.get_untracked())
            .disabled(!unlocked)
            .on_select(move || s.update_preferences(|p| p.cards = false)),
        MenuItem::separator(),
        MenuItem::new(l("Toggle Sidebar")).shortcut("mod+alt+s").on_select(move || s.sidebar.update(|v| *v = !*v)),
        MenuItem::role(nsh_core::desktop::MenuRole::ToggleFullScreen),
    ];
    let window = vec![MenuItem::role(nsh_core::desktop::MenuRole::Minimize), MenuItem::role(nsh_core::desktop::MenuRole::Zoom)];
    set_application_menu(vec![
        MenuItem::submenu("Safay", app),
        MenuItem::submenu(l("File"), file),
        MenuItem::submenu(l("Edit"), edit),
        MenuItem::submenu(l("View"), view_menu),
        MenuItem::submenu(l("Window"), window),
    ]);
}

fn unlocked(s: Store) -> impl IntoElement {
    view()
        .flex_1()
        .flex_row()
        .child(when(move || s.sidebar.get() && window().width.0 >= 640.0, move || sidebar_column(s)))
        .child(view().flex_1().min_w(px(0.0)).child(toolbar(s)).child(content_layer(s, pages::accounts::page(s))))
}

/// Windows lays content on a rounded layer over Mica; macOS and Linux use the window background.
fn content_layer(s: Store, content: impl IntoElement) -> impl IntoElement {
    let _ = s;
    let win = look() == Look::Windows;
    view()
        .flex_1()
        .min_h(px(0.0))
        // macOS: transparent, so the floating sidebar's shadow falls over the content.
        .bg(move || if look() == Look::Mac { theme().background.opacity(0.0) } else { theme().background })
        .rounded_tl(px(if win { 8.0 } else { 0.0 }))
        .border_t(px(if win { 1.0 } else { 0.0 }))
        .border_l(px(if win { 1.0 } else { 0.0 }))
        .border_color(move || tok("card-border"))
        .child(content)
}

fn sidebar_column(s: Store) -> impl IntoElement {
    let inset = chrome::sidebar_inset();
    // macOS 26: a floating panel; macOS 15 and earlier, 27 and later: a flush column, as the system's sidebar.
    let floating = chrome::floating_sidebar();
    let flush_mac = look() == Look::Mac && !floating;
    view()
        // macOS: the detail column starts at the sidebar's edge (its content pads itself), as a macOS split view does.
        .w(px(chrome::sidebar_width() + inset))
        .pl(px(inset))
        .py(px(inset))
        .bg(move || match look() {
            Look::Mac if floating => theme().background,
            Look::Mac => tok("sidebar-plain"),
            Look::Windows => tok("chrome"),
            Look::Linux => theme().sidebar,
        })
        // Finder's on macOS 27: a hairline (one device pixel at 2x).
        .border_r(px(match look() {
            Look::Linux => 1.0,
            Look::Mac if flush_mac => 0.5,
            _ => 0.0,
        }))
        .border_color(move || if flush_mac { tok("sidebar-separator") } else { tok("sidebar-border") })
        .child(
            view()
                .test_id("sidebar-panel")
                .flex_1()
                .rounded(px(chrome::sidebar_radius()))
                .bg(move || if floating { theme().sidebar } else { theme().sidebar.opacity(0.0) })
                .shadow(if floating { vec![soft_shadow(0.0, 1.0, 0.05), soft_shadow(2.0, 24.0, 0.09)] } else { vec![] })
                .child(sidebar_header(s))
                .child(pages::accounts::sidebar(s))
                // The floating panel's 1pt rim (white in light, a lighter gray in dark), an overlay that takes no space.
                .child(when(
                    move || floating,
                    || {
                        view()
                            .absolute()
                            .inset(px(0.0))
                            .rounded(px(chrome::sidebar_radius()))
                            .border_1()
                            .border_color(move || tok("sidebar-rim"))
                            .pointer_events(nsh_core::PointerEvents::None)
                    },
                )),
        )
}

/// macOS: the traffic lights live here. Windows: the app's icon and name. Linux: a header bar title.
fn sidebar_header(s: Store) -> impl IntoElement {
    // The toggle centers on the traffic lights: the title-bar row's center, inside the inset panel.
    let h = chrome::toolbar_height() - 2.0 * chrome::sidebar_inset();
    view()
        .h(px(h))
        .flex_row()
        .items_center()
        .window_drag(true)
        .pl(move || px(if look() == Look::Mac { (title_bar().left.0 - chrome::sidebar_inset()).max(12.0) } else { 14.0 }))
        .pr(px(8.0))
        .child(when(
            || look() == Look::Windows,
            || {
                view()
                    .flex_row()
                    .items_center()
                    .gap(px(10.0))
                    .child(crate::components::app_icon(16.0))
                    .child(text("Safay").text_size(px(12.0)))
            },
        ))
        .child(when(|| look() == Look::Linux, || text("Safay").text_size(px(13.0)).font_weight(FontWeight::BOLD)))
        .child(spacer())
        .gap(px(2.0))
        .child(sidebar_button(s, "New Category", Icon::FolderPlus, move || pages::accounts::new_category(s, None)))
        .child(tool_toggle(s))
}
fn tool_toggle(s: Store) -> impl IntoElement {
    sidebar_button(s, "Toggle Sidebar", Icon::Sidebar, move || s.sidebar.update(|v| *v = !*v))
}
/// A borderless button in the sidebar's title bar, with a round hover, as Notes' are.
fn sidebar_button(s: Store, label: &'static str, glyph: Icon, press: impl Fn() + 'static) -> impl IntoElement {
    nsh_base::button()
        .accessibility_label(move || Some(s.label(label)))
        .size(px(30.0))
        .items_center()
        .justify_center()
        .rounded_full()
        .text_color(move || theme().foreground.opacity(0.75))
        .hover(|st| st.bg(move || tok("hover")))
        .focus(focus_ring)
        .child(crate::icons::symbol(glyph, 17.0, move || theme().foreground.opacity(0.75)))
        .on_press(press)
}

fn toolbar(s: Store) -> impl IntoElement {
    let collapsed = move || !s.sidebar.get() || window().width.0 < 640.0;
    view()
        .h(px(chrome::toolbar_height()))
        .flex_row()
        .items_center()
        .gap(px(if look() == Look::Mac { 7.0 } else { 10.0 }))
        .window_drag(true)
        .pl(move || {
            px(if collapsed() {
                title_bar().left.0 + 8.0
            } else if look() == Look::Mac {
                20.0
            } else {
                16.0
            })
        })
        .pr(move || px(title_bar().right.0 + if look() == Look::Mac { 7.5 } else { 12.0 }))
        .child(when(collapsed, move || tool_toggle(s)))
        .child(
            heading(
                move || {
                    if s.trash.get() {
                        return s.label("Recently Deleted");
                    }
                    s.category
                        .get()
                        .and_then(|id| {
                            s.projection.with(|p| p.categories.iter().find(|c| c.id == id).map(|c| SharedString::from(c.name.clone())))
                        })
                        // Linux: the sidebar's header already names the app, so the toolbar names the scope.
                        .unwrap_or_else(|| if look() == Look::Linux && !collapsed() { s.label("All") } else { "Safay".into() })
                },
                15.0,
            )
            .number_of_lines(1)
            .flex_shrink(1.0),
        )
        .child(spacer())
        .child(tool_button(move || s.label("Lock"), Icon::Lock).on_press(move || s.lock()))
        // Recently Deleted is a plain list.
        .child(when(move || !s.trash.get(), move || view_picker(s)))
        .child(search_field(s))
        .child(add_button(s))
}

fn view_picker(s: Store) -> impl IntoElement {
    let mac = look() == Look::Mac;
    let seg = move |cards: bool, glyph: Icon| {
        nsh_base::button()
            .accessibility_label(move || Some(s.label(if cards { "Cards" } else { "List" })))
            .selected(move || s.cards.get() == cards)
            .w(px(if mac { 34.5 } else { 34.0 }))
            .h(px(28.0))
            .items_center()
            .justify_center()
            .rounded(px(if mac { 14.0 } else { chrome::control_radius(false) }))
            .bg(move || if s.cards.get() == cards { tok("pressed") } else { theme().background.opacity(0.0) })
            .text_color(move || if s.cards.get() == cards { theme().foreground } else { theme().muted_foreground })
            .hover(|st| st.text_color(move || theme().foreground))
            .focus(focus_ring)
            .on_press(move || s.update_preferences(move |p| p.cards = cards))
            .child(crate::icons::symbol_weight(
                glyph,
                if glyph == Icon::List { 16.5 } else { 16.0 },
                if mac { 0.23 } else { 0.0 },
                move || {
                    if mac {
                        tok("glass-foreground")
                    } else if s.cards.get() == cards {
                        theme().foreground
                    } else {
                        theme().muted_foreground
                    }
                },
            ))
    };
    view()
        .window_drag(false)
        .flex_row()
        .px(px(3.0))
        .py(px(if mac { (GLASS - 28.0) / 2.0 } else { 3.0 }))
        .rounded(px(if mac { GLASS / 2.0 } else { chrome::control_radius(false) + 2.0 }))
        .bg(move || if mac { tok("glass") } else { theme().background.opacity(0.0) })
        .border(px(if mac { 0.0 } else { 1.0 }))
        .border_color(move || tok("separator"))
        .shadow(if mac { glass_shadow() } else { vec![] })
        .child(seg(false, Icon::List))
        .child(seg(true, Icon::Grid))
}

fn search_field(s: Store) -> impl IntoElement {
    let mac = look() == Look::Mac;
    let r = s.search.get();
    // NSH keeps one keyboard focus for the app: a field in the menu bar panel or Settings takes it from this one while
    // AppKit keeps this field editing in the main window, so its focus ring would not come back (focusing it again
    // does nothing while it edits). When the window gets the keyboard back with this field still editing, the field
    // takes the focus again.
    let editing = std::rc::Rc::new(std::cell::Cell::new(false));
    let own_window = nsh_core::element::desktop::current_window();
    let was_key = std::cell::Cell::new(true);
    effect({
        let editing = editing.clone();
        move || {
            let key = own_window.state().is_some_and(|w| w.focused);
            // Read first: the window losing the keyboard is noted too.
            let was = was_key.replace(key);
            if key && !was && editing.get() {
                let editing = editing.clone();
                set_timeout(std::time::Duration::from_millis(50), move || {
                    // Unless another field of the window took the focus meanwhile (a dialog's password field).
                    if editing.get() {
                        r.blur();
                        r.focus();
                    }
                });
            }
        }
    });
    let (focused, blurred) = (editing.clone(), editing);
    let field = view().window_drag(false);
    // Linux: tiling layouts give the window any width, so the field takes what the title leaves (120 to 240 pt) rather
    // than truncating the title.
    let field = if look() == Look::Linux {
        field.flex_1().min_w(px(120.0)).max_w(px(240.0))
    } else {
        field.w(move || px(if window().width.0 >= 960.0 { 240.0 } else { 180.0 }))
    };
    field
        .h(px(if mac { GLASS } else { 32.0 }))
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .pl(px(if mac { 7.0 } else { 12.0 }))
        .pr(px(12.0))
        .rounded(px(if mac { GLASS / 2.0 } else { chrome::control_radius(false) }))
        .bg(move || if mac { tok("glass") } else { tok("input-bg") })
        .border(px(if mac { 0.0 } else { 1.0 }))
        .border_color(move || tok("input"))
        .shadow(if mac { glass_shadow() } else { vec![] })
        .focus_within(move |st| {
            if mac { st.border_2().border_color(move || tok("code-accent").opacity(0.55)) } else { st.border_color(move || theme().ring) }
        })
        .text_color(move || theme().muted_foreground)
        .child(crate::icons::symbol_weight(Icon::Search, if mac { 13.5 } else { 13.0 }, if mac { 0.23 } else { 0.0 }, move || {
            theme().muted_foreground
        }))
        .child(
            nsh_base::input()
                .element_ref(&r)
                .test_id("search")
                .auto_focus(true)
                .on_focus(move |_: &()| focused.set(true))
                .on_blur(move |_: &()| blurred.set(false))
                .label(move || s.label("Search accounts or sites"))
                .placeholder(move || s.label("Search accounts or sites").to_string())
                .placeholder_color(move || {
                    let c = theme().muted_foreground;
                    if dark() { c.dark } else { c.light }
                })
                .value(move || s.query.get())
                .on_change(move |v: &str| {
                    s.controller.get().activity();
                    s.query.set(v.into());
                    s.refresh();
                })
                .on_submit(move |_: &str| {
                    if let Some(id) = s.projection.with(|p| p.rows.first().map(|r| r.id)) {
                        s.copy(id)
                    }
                })
                .flex_1()
                .min_w(px(0.0))
                .text_size(px(13.0))
                .text_color(move || theme().foreground),
        )
        .child(when(
            move || !s.query.with(|q| q.is_empty()),
            move || {
                icon_button(move || s.label("Clear"), Icon::Close).size(px(18.0)).on_press(move || {
                    s.query.set(String::new());
                    s.refresh();
                })
            },
        ))
}

fn add_button(s: Store) -> impl IntoElement {
    match look() {
        Look::Mac => nsh_base::button()
            .accessibility_label(move || Some(s.label("Add")))
            .size(px(GLASS))
            .rounded_full()
            .items_center()
            .justify_center()
            .bg(move || theme().primary)
            .text_color(move || theme().primary_foreground)
            .shadow([soft_shadow(2.0, 6.0, 0.18)])
            .hover(|st| st.opacity(0.92))
            .focus(|st| st.border_2().border_color(move || theme().ring))
            .active(|st| st.scale(0.95))
            .on_press(move || s.open_sheet(Sheet::Add))
            .child(crate::icons::icon_bold(Icon::Plus).size(px(22.0)))
            .into_element(),
        // Linux: GTK's window buttons sit right after it, so a square icon button the size of the toolbar's other
        // controls, filled with the accent as the primary action, rather than a text button beside a round close button.
        Look::Linux => nsh_base::button()
            .accessibility_label(move || Some(s.label("Add")))
            .size(px(32.0))
            .rounded(px(chrome::control_radius(false)))
            .items_center()
            .justify_center()
            .bg(move || theme().primary)
            .text_color(move || theme().primary_foreground)
            .hover(|st| st.opacity(0.9))
            .focus(focus_ring)
            .active(|st| st.scale(0.95))
            .on_press(move || s.open_sheet(Sheet::Add))
            .child(crate::icons::icon_bold(Icon::Plus).size(px(18.0)))
            .into_element(),
        Look::Windows => {
            push_button(move || s.label("Add"), Kind::Primary, false).on_press(move || s.open_sheet(Sheet::Add)).into_element()
        }
    }
}
