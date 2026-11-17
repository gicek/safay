//! Settings: a 760×520 window with a 196pt sidebar of panes and grouped rows.
//! macOS shows it in its own window (⌘,); Windows and Linux in a dialog over the main window.
use crate::{
    chrome::{Look, look},
    components::*,
    icons::{Icon, icon},
    store::{Auth, SettingsPage, Sheet, Store},
};
use nsh_base::{SelectItem, button, select, theme};
use nsh_core::prelude::*;
use safay_app::preferences::{Appearance, DockIcon, Language};

pub fn choice(
    s: Store,
    key: &'static str,
    items: impl Fn() -> Vec<SelectItem> + Copy + 'static,
    value: impl Fn() -> SharedString + Copy + 'static,
    change: impl Fn(&str) + 'static,
    width: Option<f32>,
) -> impl IntoElement {
    let mac = look() == Look::Mac;
    select()
        .items(items)
        .value(value)
        .disabled(move || s.busy.get())
        .accessibility_label(move || Some(s.label(key)))
        .trigger(
            // macOS 26 pop-up button: a gray fill without outline or shadow, sized to its title (at least 92pt).
            view()
                .min_w(px(if mac { 92.0 } else { width.unwrap_or(150.0) }))
                .max_w(px(if mac { 260.0 } else { 240.0 }))
                .min_h(move || px(if mac { 24.0 } else { 32.0 }) * font_scale())
                .pl(px(if mac { 12.0 } else { 10.0 }))
                .pr(px(if mac { 10.0 } else { 6.0 }))
                .flex_row()
                .items_center()
                .gap(px(if mac { 11.0 } else { 6.0 }))
                .rounded(px(if mac { 6.0 } else { crate::chrome::control_radius(false) }))
                .border(px(if mac { 0.0 } else { 1.0 }))
                .border_color(move || tok("separator"))
                .bg(move || if mac { tok("popup-fill") } else { tok("glass") })
                .shadow(if mac { vec![] } else { vec![soft_shadow(1.0, 2.0, 0.05)] })
                .hover(|st| st.border_color(move || tok("input")))
                .focus(|st| st.border_color(move || theme().ring))
                .child(
                    text(move || items().into_iter().find(|i| i.value == value()).map(|i| i.title).unwrap_or_default())
                        .number_of_lines(1)
                        .text_size(px(13.0))
                        .flex_1(),
                )
                .child(if mac {
                    crate::icons::symbol_weight(Icon::ChevronUpDown, 11.0, 0.4, move || theme().foreground).into_element()
                } else {
                    view().text_color(move || theme().muted_foreground).child(icon(Icon::ChevronUpDown).size(px(12.0))).into_element()
                }),
        )
        // The list is as wide as its widest choice, each on one line, as a macOS pop-up menu is. NSH makes it as wide as
        // the button unless the style sets a width, and its rows (a virtual list) have no width of their own to size
        // it by: the titles are measured, in the selected rows' medium 14px, plus the rows' and the list's padding.
        .popup_style(move |style| {
            let widest = items().iter().map(|i| measure_text("500 14px system-ui", &i.title).0).fold(0.0_f32, f32::max);
            style
                .w(px(widest.ceil() + 24.0 + 8.0 + 2.0 + 4.0))
                .p_1()
                .rounded(px(8.0))
                .border_1()
                .border_color(move || tok("separator"))
                .bg(move || theme().popover)
                .shadow_lg()
        })
        .item_class("px-3 py-1.5 rounded-md text-sm focus:bg-primary focus:text-primary-foreground selected:font-medium")
        .render_item(|item: &nsh_base::SelectEntry| text(item.title.clone()).number_of_lines(1))
        .on_change({
            // `Select` reports the choice before it closes its list: apply it on the next turn, so the list closes at once
            // and the work it causes (theme, language, preferences) never holds it open.
            let change = std::rc::Rc::new(change);
            move |next: &Option<SharedString>| {
                if let Some(next) = next.clone() {
                    let change = change.clone();
                    set_timeout(std::time::Duration::ZERO, move || change(&next));
                }
            }
        })
}
/// SettingsRow: title, optional description, the control on the right; 50pt (60 with a description).
fn row(s: Store, title: &'static str, help: Option<Box<dyn Fn() -> SharedString>>, control: impl IntoElement) -> impl IntoElement {
    let has_help = help.is_some();
    view()
        .min_h(move || px(if has_help { 60.0 } else { 50.0 }) * font_scale())
        .px(px(18.0))
        .py(px(8.0))
        .flex_row()
        .items_center()
        .gap(px(20.0))
        .child(
            view().flex_1().gap(px(2.0)).child(text(move || s.label(title)).text_size(px(13.0))).child(match help {
                Some(h) => text(h)
                    .number_of_lines(2)
                    .text_size(px(11.5))
                    .line_height(px(15.0))
                    .text_color(move || theme().muted_foreground)
                    .into_element(),
                None => view().into_element(),
            }),
        )
        .child(control)
}
fn help(s: Store, key: &'static str) -> Option<Box<dyn Fn() -> SharedString>> {
    Some(Box::new(move || s.label(key)))
}
fn section(rows: Vec<AnyElement>) -> impl IntoElement {
    let n = rows.len();
    view()
        .rounded(px(14.0))
        .bg(move || tok("group"))
        .border(px(if look() == Look::Mac { 0.0 } else { 1.0 }))
        .border_color(move || tok("card-border"))
        .children(rows.into_iter().enumerate().flat_map(move |(i, r)| {
            let mut v = vec![r];
            if i + 1 < n {
                v.push(view().px(px(18.0)).child(divider()).into_element());
            }
            v
        }))
}
fn danger_section(rows: Vec<AnyElement>) -> impl IntoElement {
    view().rounded(px(14.0)).bg(move || theme().destructive.opacity(0.07)).children(rows)
}
fn locked_section(s: Store) -> impl IntoElement {
    section(vec![
        view()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .min_h(px(50.0))
            .px(px(18.0))
            .text_color(move || theme().muted_foreground)
            .child(icon(Icon::Lock).size(px(14.0)))
            .child(text(move || s.label("Unlock Safay to change these settings")).text_size(px(13.0)))
            .into_element(),
    ])
}

pub fn page(s: Store) -> impl IntoElement + Styled {
    view()
        .flex_row()
        .flex_1()
        .child(settings_sidebar(s))
        .child(view().w(px(1.0)).bg(move || tok("separator")).opacity(if look() == Look::Mac { 1.0 } else { 0.6 }))
        .child(
            scroll()
                .flex_1()
                // The Settings window's pane is the window background over the sidebar material.
                .bg(move || theme().background)
                .child(
                    view()
                        .px(px(22.0))
                        .pt(px(if s.surface == crate::store::Surface::Settings { 40.0 } else { 52.0 }))
                        .pb(px(22.0))
                        .gap(px(22.0))
                        .child(when(move || s.settings_page.get() == SettingsPage::General, move || general(s)))
                        .child(when(move || s.settings_page.get() == SettingsPage::Security, move || security(s)))
                        .child(when(move || s.settings_page.get() == SettingsPage::Shortcuts, move || shortcuts(s)))
                        .child(when(move || s.settings_page.get() == SettingsPage::Advanced, move || advanced(s)))
                        .child(when(move || s.settings_page.get() == SettingsPage::About, move || about(s))),
                ),
        )
}
fn settings_sidebar(s: Store) -> impl IntoElement {
    let window = s.surface == crate::store::Surface::Settings;
    let mac = look() == Look::Mac;
    let item = move |page: SettingsPage, glyph: Icon, key: &'static str| {
        let active = move || s.settings_page.get() == page;
        button()
            .selected(active)
            .accessibility_label(move || Some(s.label(key)))
            .h(move || px(34.0) * font_scale())
            .px(px(10.0))
            .flex_row()
            .items_center()
            .gap(px(9.0))
            .rounded(px(8.0))
            .bg(move || {
                if active() {
                    if mac { theme().foreground.opacity(0.09) } else { tok("row-selected") }
                } else {
                    theme().sidebar.opacity(0.0)
                }
            })
            .hover(|st| st.bg(move || tok("hover")))
            .focus(focus_ring)
            .on_press(move || s.settings_page.set(page))
            .child(view().w(px(22.0)).items_center().child(crate::icons::symbol(glyph, 15.0, move || theme().foreground)))
            .child(text(move || s.label(key)).text_size(px(13.0)))
    };
    view()
        .w(px(196.0))
        .px(px(10.0))
        .pt(px(if window { 52.0 } else { 14.0 }))
        .gap(px(2.0))
        // macOS: the behind-window sidebar material: the window's backdrop shows through (`desktop.rs`).
        .bg(move || {
            if window && mac {
                theme().sidebar.opacity(0.0)
            } else if window {
                theme().sidebar
            } else {
                tok("chrome")
            }
        })
        .window_drag(window)
        .child(item(SettingsPage::General, Icon::Settings, "General"))
        .child(item(SettingsPage::Security, Icon::Lock, "Security"))
        .child(when(|| crate::platform::supports::GLOBAL_HOTKEYS, move || item(SettingsPage::Shortcuts, Icon::Keyboard, "Shortcuts")))
        .child(item(SettingsPage::Advanced, Icon::Sliders, "Advanced"))
        .child(item(SettingsPage::About, Icon::Info, "About"))
}

fn general(s: Store) -> impl IntoElement {
    let appearance = choice(
        s,
        "Appearance",
        move || {
            vec![
                SelectItem::new("light", s.label("Light")),
                SelectItem::new("dark", s.label("Dark")),
                SelectItem::new("system", s.label("System")),
            ]
        },
        move || match s.preferences.get().appearance {
            Appearance::System => "system".into(),
            Appearance::Light => "light".into(),
            Appearance::Dark => "dark".into(),
        },
        move |v| {
            let a = match v {
                "light" => Appearance::Light,
                "dark" => Appearance::Dark,
                _ => Appearance::System,
            };
            s.update_preferences(move |p| p.appearance = a)
        },
        None,
    );
    let language = choice(
        s,
        "Language",
        move || {
            vec![
                SelectItem::new("system", s.label("System")),
                SelectItem::new("en", "English"),
                SelectItem::new("hans", "简体中文"), // i18n-guard: allow
                SelectItem::new("hant", "繁體中文"), // i18n-guard: allow
            ]
        },
        move || match s.preferences.get().language {
            Language::System => "system".into(),
            Language::En => "en".into(),
            Language::Hans => "hans".into(),
            Language::Hant => "hant".into(),
        },
        move |v| {
            let l = match v {
                "en" => Language::En,
                "hans" => Language::Hans,
                "hant" => Language::Hant,
                _ => Language::System,
            };
            s.update_preferences(move |p| p.language = l)
        },
        None,
    );
    let tray_title = if look() == Look::Mac { "Menu Bar Icon" } else { "Tray Icon" };
    let tray_help = if look() == Look::Mac {
        "Show a menu bar icon for quick code access"
    } else {
        "Show an icon in the notification area for quick code access"
    };
    let mut integration =
        vec![
            row(
                s,
                tray_title,
                help(s, tray_help),
                switch_control(
                    move || s.label(tray_title),
                    move || s.preferences.get().tray_icon,
                    move || s.busy.get(),
                    move |v| s.update_preferences(move |p| p.tray_icon = v),
                ),
            )
            .into_element(),
            // The panel's account scope; no effect while the icon is hidden, so it is dimmed then.
            view()
                .opacity(move || if s.preferences.with(|p| p.tray_icon) { 1.0_f32 } else { 0.5 })
                .pointer_events(move || {
                    if s.preferences.with(|p| p.tray_icon) { nsh_core::PointerEvents::Auto } else { nsh_core::PointerEvents::None }
                })
                .child(row(
                    s,
                    "Menu Bar Accounts",
                    help(
                        s,
                        if look() == Look::Mac { "Which accounts the menu bar panel shows" } else { "Which accounts the tray panel shows" },
                    ),
                    choice(
                        s,
                        "Menu Bar Accounts",
                        move || {
                            let mut items = vec![SelectItem::new("all", s.label("All"))];
                            items.extend(s.projection.with(|p| {
                                p.categories.iter().map(|c| SelectItem::new(c.id.to_string(), c.name.clone())).collect::<Vec<_>>()
                            }));
                            items
                        },
                        move || {
                            let id = s.preferences.with(|p| p.menu_bar_category);
                            match id.filter(|id| s.projection.with(|p| p.categories.iter().any(|c| c.id == *id))) {
                                Some(id) => id.to_string().into(),
                                None => "all".into(),
                            }
                        },
                        move |v| {
                            let next = safay_app::Uuid::parse_str(v).ok();
                            s.update_preferences(move |p| p.menu_bar_category = next)
                        },
                        None,
                    ),
                ))
                .into_element(),
        ];
    // Starting without the main window needs the icon to get back to Safay: dimmed, and saying so, while it is hidden.
    if look() == Look::Mac && crate::platform::supports::LOGIN_ITEM {
        integration.push(
            row(
                s,
                "Start in Menu Bar",
                Some(Box::new(move || {
                    s.label(if s.preferences.with(|p| p.tray_icon) {
                        "When Safay starts, only the menu bar icon appears"
                    } else {
                        "Needs the menu bar icon"
                    })
                })),
                switch_control(
                    move || s.label("Start in Menu Bar"),
                    move || s.preferences.get().background_launch,
                    move || s.busy.get() || !s.preferences.get().tray_icon,
                    move |v| s.update_preferences(move |p| p.background_launch = v),
                ),
            )
            .into_element(),
        );
    }
    if look() == Look::Mac {
        integration.push(
            row(
                s,
                "Dock Icon",
                None,
                choice(
                    s,
                    "Dock Icon",
                    move || {
                        vec![
                            SelectItem::new("always", s.label("Always Show")),
                            SelectItem::new("main", s.label("Only When Main Window Is Open")),
                            SelectItem::new("never", s.label("Never Show")),
                        ]
                    },
                    move || match s.preferences.get().dock_icon {
                        DockIcon::Always => "always".into(),
                        DockIcon::MainWindow => "main".into(),
                        DockIcon::Never => "never".into(),
                    },
                    move |v| {
                        let d = match v {
                            "always" => DockIcon::Always,
                            "never" => DockIcon::Never,
                            _ => DockIcon::MainWindow,
                        };
                        s.update_preferences(move |p| {
                            p.dock_icon = d;
                            if d == DockIcon::Never {
                                p.tray_icon = true
                            }
                        })
                    },
                    Some(236.0),
                ),
            )
            .into_element(),
        );
    }
    view()
        .gap(px(22.0))
        .child(section(vec![row(s, "Appearance", None, appearance).into_element(), row(s, "Language", None, language).into_element()]))
        .child(when(|| crate::platform::supports::LOGIN_ITEM, move || login_section(s)))
        .child(if crate::platform::supports::TRAY { section(integration).into_element() } else { view().into_element() })
        .child(section(vec![
            row(
                s,
                "Show Site Icons",
                help(s, "Gets each account’s icon from its own website"),
                switch_control(
                    move || s.label("Show Site Icons"),
                    move || s.preferences.get().site_icons,
                    move || s.busy.get(),
                    move |v| s.update_preferences(move |p| p.site_icons = v),
                ),
            )
            .into_element(),
        ]))
}
/// Launch at Login is the system's login item state, not a stored preference.
fn login_section(s: Store) -> impl IntoElement {
    let set_login = move |v: bool| {
        use nsh_core::element::desktop::{launch_at_login, set_launch_at_login};
        if s.fixture {
            return;
        }
        s.task(async move {
            match set_launch_at_login(v).await {
                Ok(()) => s.login_error.set(None),
                Err(e) => s.login_error.set(Some(e.to_string())),
            }
            // macOS may wait for the user's approval in Login Items: the switch shows what the system has now.
            s.login_item.set(Some(launch_at_login().await));
        });
    };
    section(vec![
        row(
            s,
            "Launch at Login",
            Some(Box::new(move || {
                s.login_error
                    .get()
                    .map(|e| s.fmt("Couldn’t update login item: {}", &[&e]))
                    .unwrap_or_else(|| s.label("Open Safay automatically when you log in"))
            })),
            switch_control(
                move || s.label("Launch at Login"),
                move || s.login_item.get().unwrap_or(false),
                move || s.fixture || s.login_item.get().is_none(),
                set_login,
            ),
        )
        .into_element(),
    ])
}
fn shortcuts(s: Store) -> impl IntoElement {
    section(vec![
        row(s, "Show Menu Bar Panel", help(s, "Your codes, from anywhere"), recorder(s, 0)).into_element(),
        row(s, "Show Main Window", help(s, "Brings Safay’s main window forward"), recorder(s, 1)).into_element(),
        row(s, "Select QR Code on Screen", help(s, "Adds the account from a 2FA QR code on your screen"), recorder(s, 2)).into_element(),
    ])
}
/// The shortcut recorder: shows the combination, records the next one on press, ✕ clears it.
fn recorder(s: Store, which: u8) -> impl IntoElement {
    let current = move || {
        let p = s.preferences.get();
        match which {
            0 => p.hotkey_panel,
            1 => p.hotkey_main,
            _ => p.hotkey_capture,
        }
    };
    let recording = move || s.recording.get() == Some(which);
    let store = move |next: Option<safay_app::preferences::Hotkey>| {
        s.update_preferences(move |p| match which {
            0 => p.hotkey_panel = next,
            1 => p.hotkey_main = next,
            _ => p.hotkey_capture = next,
        })
    };
    let mac = look() == Look::Mac;
    // macOS: the recorder, a 130×24 capsule with its clear button inside.
    view()
        .w(px(if mac { 130.0 } else { 150.0 }))
        .flex_row()
        .items_center()
        .gap(px(4.0))
        .child(
            button()
                .accessibility_label(move || Some(s.label(if recording() { "Type shortcut" } else { "Record Shortcut" })))
                .flex_1()
                .h(px(if mac { 24.0 } else { 26.0 }))
                .items_center()
                .justify_center()
                .rounded(px(if mac { 12.0 } else { 6.0 }))
                .border_1()
                .border_color(move || {
                    if recording() {
                        theme().ring
                    } else if mac {
                        tok("input")
                    } else {
                        tok("separator")
                    }
                })
                .bg(move || if mac { tok("input-bg") } else { tok("glass") })
                .focus(|st| st.border_color(move || theme().ring))
                .on_press(move || {
                    if s.fixture {
                        return;
                    }
                    s.recording.set(Some(which));
                    // The key arrives on AppKit's main thread; it is applied here, on the engine thread.
                    #[cfg(target_os = "macos")]
                    {
                        let keys = crate::platform::hotkeys::record();
                        s.task(async move {
                            let result = blocking(move || keys.recv().ok().flatten()).await.ok().flatten();
                            s.recording.set(None);
                            if let Some(next) = result {
                                store(next)
                            }
                        });
                    }
                })
                .child(
                    text(move || {
                        if recording() {
                            return s.label("Type shortcut");
                        }
                        match current() {
                            #[cfg(target_os = "macos")]
                            Some(h) => crate::platform::hotkeys::label(h).into(),
                            #[cfg(not(target_os = "macos"))]
                            Some(_) => "".into(),
                            None => s.label("Record Shortcut"),
                        }
                    })
                    .text_size(px(12.5))
                    .text_color(move || if current().is_some() || recording() { theme().foreground } else { theme().muted_foreground }),
                ),
        )
        .child(when(
            move || current().is_some() && !recording(),
            move || {
                if mac {
                    view()
                        .absolute()
                        .right(px(5.0))
                        .top(px(0.0))
                        .bottom(px(0.0))
                        .justify_center()
                        .child(
                            button()
                                .accessibility_label(move || Some(s.label("Clear")))
                                .active(|st| st.opacity(0.6))
                                .on_press(move || store(None))
                                .child(crate::icons::symbol(Icon::ClearFill, 13.0, move || theme().muted_foreground)),
                        )
                        .into_element()
                } else {
                    icon_button(move || s.label("Clear"), Icon::Close).size(px(22.0)).on_press(move || store(None)).into_element()
                }
            },
        ))
}
fn security(s: Store) -> impl IntoElement {
    when(move || !s.unlocked(), move || locked_section(s)).otherwise(move || {
        let auto_lock = choice(
            s,
            "Auto-Lock",
            move || [1, 5, 15, 30].into_iter().map(|m| SelectItem::new(m.to_string(), s.count("minutes", m))).collect(),
            move || s.preferences.get().auto_lock_minutes.to_string().into(),
            move |v| {
                if let Ok(m) = v.parse::<u64>() {
                    s.update_preferences(move |p| p.auto_lock_minutes = m)
                }
            },
            None,
        );
        // Touch ID and the failure delay share one group.
        let mut unlocking = Vec::new();
        if crate::platform::supports::BIOMETRIC {
            unlocking.push(
                row(
                    s,
                    "Unlock with Touch ID",
                    Some(Box::new(move || {
                        s.label(if s.biometrics.get() {
                            "You can always use your master password instead"
                        } else {
                            "Touch ID is unavailable or not enrolled on this Mac"
                        })
                    })),
                    switch_control(
                        move || s.label("Unlock with Touch ID"),
                        move || s.biometrics.get() && s.preferences.get().touch_id,
                        move || !s.biometrics.get() || s.busy.get(),
                        move |v| s.set_touch_id(v),
                    ),
                )
                .into_element(),
            );
        }
        unlocking.push(
            row(
                s,
                "Delay After Failed Attempts",
                help(s, "After 5 wrong attempts, each try waits longer, up to 5 minutes"),
                switch_control(
                    move || s.label("Delay After Failed Attempts"),
                    move || s.preferences.get().failure_delay,
                    move || s.busy.get(),
                    move |v| {
                        if v {
                            s.dispatch(true, |session| {
                                session.set_failure_delay(true, "")?;
                                Ok(crate::store::Payload::Silent)
                            })
                        } else {
                            s.reveal(Auth::DisableDelay)
                        }
                    },
                ),
            )
            .into_element(),
        );
        // Unlocking, locking, then the master password itself.
        view()
            .gap(px(22.0))
            .child(section(unlocking))
            .child(section(vec![
                row(
                    s,
                    "Auto-Lock",
                    help(s, "Locks after the selected time without activity, and immediately when the device locks or sleeps"),
                    auto_lock,
                )
                .into_element(),
            ]))
            .child(section(vec![
                row(
                    s,
                    "Change Master Password",
                    help(s, "Unlocks Safay and encrypts your vault and backups"),
                    push_button(move || s.label("Change"), Kind::Standard, false)
                        .on_press(move || open_main_sheet(s, Sheet::ChangePassword)),
                )
                .into_element(),
            ]))
    })
}
/// Sheets that need the vault open in the main window (the Settings window has a store of its own).
fn open_main_sheet(s: Store, sheet: Sheet) {
    #[cfg(target_os = "macos")]
    if s.surface == crate::store::Surface::Settings {
        crate::platform::open_in_main(sheet);
        return;
    }
    s.open_sheet(sheet);
}
fn advanced(s: Store) -> impl IntoElement {
    let location = crate::platform::data_location();
    let shown = location.clone().unwrap_or_default();
    view()
        .gap(px(22.0))
        .child(section(vec![
            row(
                s,
                "Local Data Location",
                Some(Box::new(move || shown.clone().into())),
                push_button(move || s.label("Open Location"), Kind::Standard, false)
                    .disabled(move || location.is_none() || s.fixture)
                    .on_press({
                        let location = crate::platform::data_location();
                        move || {
                            if let Some(l) = location.clone() {
                                drop(open_path(l));
                            }
                        }
                    }),
            )
            .into_element(),
        ]))
        // Locked, the protected groups below are hidden: say why, as Security does.
        .child(when(move || !s.unlocked(), move || locked_section(s)))
        // Launchers (Alfred, Raycast, Tinycast): changed only while unlocked, like the rest of the protected settings.
        .child(when(
            move || crate::platform::supports::AUTOMATION && s.unlocked(),
            move || {
                section(vec![
                    row(
                        s,
                        "Launcher Access",
                        help(s, "Alfred, Raycast and Tinycast can search your accounts. Every code they copy needs Touch ID or your master password."),
                        switch_control(
                            move || s.label("Launcher Access"),
                            move || s.preferences.get().launcher,
                            move || s.busy.get(),
                            move |v| s.update_preferences(move |p| p.launcher = v),
                        ),
                    )
                    .into_element(),
                    row(
                        s,
                        "Install Alfred Workflow",
                        Some(Box::new(move || {
                            s.label(if s.preferences.with(|p| p.launcher) {
                                "Import the Alfred workflow to search accounts and copy codes with a keyword"
                            } else {
                                "Needs launcher access"
                            })
                        })),
                        // The workflow needs launcher access to do anything.
                        push_button(move || s.label("Install to Alfred"), Kind::Standard, false)
                            .disabled(move || {
                                !s.preferences.with(|p| p.launcher) || crate::platform::alfred_workflow().is_none() || s.fixture
                            })
                            .on_press(move || {
                                if let Some(path) = crate::platform::alfred_workflow() {
                                    drop(open_path(path));
                                }
                            }),
                    )
                    .into_element(),
                ])
            },
        ))
        .child(when(
            move || s.unlocked() && s.projection.with(|p| p.has_snapshot),
            move || {
                section(vec![
                    row(
                        s,
                        "Restore Last Good Copy",
                        help(s, "Replace the vault with the copy saved before the last change"),
                        push_button(move || s.label("Restore"), Kind::Standard, false)
                            .on_press(move || open_main_sheet(s, Sheet::Restore)),
                    )
                    .into_element(),
                ])
            },
        ))
        .child(when(
            move || s.unlocked(),
            move || {
                danger_section(vec![
                    row(
                        s,
                        "Reset App",
                        help(s, "Erases Safay’s data on this computer. Exported files and backups aren’t affected."),
                        push_button(move || s.label("Reset"), Kind::Danger, false).on_press(move || {
                            let chosen = crate::alert::alert(s.label("Reset Safay?"))
                                .description(s.label("Erases Safay’s data on this computer. Exported files and backups aren’t affected."))
                                .cancel("cancel", s.label("Cancel"))
                                .destructive("erase", s.label("Erase Local Data"))
                                .show();
                            spawn(async move {
                                if chosen.await == Some("erase") {
                                    s.reveal(Auth::Reset)
                                }
                            });
                        }),
                    )
                    .into_element(),
                ])
            },
        ))
}
fn about(s: Store) -> impl IntoElement {
    // An About link: title, the address shown below (monospaced for the repository), an accent link glyph.
    let link = move |title: &'static str, url: fn(crate::i18n::Language) -> &'static str, shown: Option<&'static str>, mono: bool| {
        let open = move || drop(open_url(url(s.language.get())));
        view()
            .min_h(move || px(if shown.is_some() { 60.0 } else { 50.0 }) * font_scale())
            .px(px(18.0))
            .py(px(8.0))
            .flex_row()
            .items_center()
            .gap(px(20.0))
            .child(
                view().flex_1().gap(px(2.0)).child(text(move || s.label(title)).text_size(px(13.0))).child(match shown {
                    Some(v) => text(v)
                        .text_size(px(11.5))
                        .font_family(if mono { CODE_FONT.family } else { "system-ui" })
                        .text_color(move || theme().muted_foreground)
                        .into_element(),
                    None => view().into_element(),
                }),
            )
            .child(
                button()
                    .accessibility_label(move || Some(s.label(title)))
                    .active(|st| st.opacity(0.6))
                    .on_press(open)
                    .child(crate::icons::symbol(Icon::External, 15.0, move || tok("code-accent"))),
            )
            .into_element()
    };
    view()
        .gap(px(22.0))
        .child(
            view().flex_row().items_center().gap(px(14.0)).px(px(18.0)).py(px(6.0)).child(app_icon(72.0)).child(
                view()
                    .gap(px(3.0))
                    .child(heading(|| "Safay".into(), 20.0))
                    .child(note(move || s.fmt("Version {} ({})", &[env!("CARGO_PKG_VERSION"), crate::BUILD])).text_size(px(12.0)))
                    .child(
                        text(move || s.fmt("NSH {} · {}", &[env!("SAFAY_NSH_REVISION"), platform_name()]))
                            .text_size(px(11.5))
                            .text_color(move || tok("tertiary")),
                    ),
            ),
        )
        .child(when(|| crate::platform::supports::UPDATES, move || updates(s)))
        .child(section(vec![
            link("Website", |_| "https://safay.gicek.com/", Some("safay.gicek.com"), false),
            link(
                "Privacy Policy",
                |l| match l {
                    crate::i18n::Language::Hans => "https://safay.gicek.com/zh-hans/privacy/",
                    crate::i18n::Language::Hant => "https://safay.gicek.com/zh-hant/privacy/",
                    crate::i18n::Language::En => "https://safay.gicek.com/privacy/",
                },
                None,
                false,
            ),
            link("Source Code", |_| "https://github.com/gicek/safay", Some("github.com/gicek/safay"), true),
        ]))
}
/// The updates group: always shown, so every build makes the options visible; builds without an update source
/// (development and test) show them disabled with the reason. The updater is NSH's `nsh-updater` (`platform::updates`).
fn updates(s: Store) -> impl IntoElement {
    use crate::platform::updates::Phase;
    // The update source is fixed per build, so whether this one updates itself is decided once.
    let configured = crate::platform::updates_configured();
    let checks = move || s.preferences.with(|p| p.update_checks == Some(true));
    let phase = move || s.updates.get().unwrap_or_default();
    let busy = move || matches!(phase(), Phase::Checking | Phase::Downloading { .. } | Phase::Installing);
    let status = move || -> SharedString {
        if !configured {
            return s.label("Only release builds check for updates");
        }
        match phase() {
            Phase::Idle => s.fmt("Version {}", &[env!("CARGO_PKG_VERSION")]),
            Phase::Checking => s.label("Checking for updates…"),
            Phase::UpToDate => s.label("Safay is up to date"),
            Phase::Available { version, .. } => s.fmt("Safay {} is available", &[&version]),
            Phase::Downloading { received, total } => match total.filter(|t| *t > 0) {
                Some(total) => s.fmt("Downloading… {}%", &[&(received * 100 / total).min(100).to_string()]),
                None => s.label("Downloading…"),
            },
            Phase::Ready { version } => s.fmt("Safay {} is ready to install", &[&version]),
            Phase::Installing => s.label("Installing…"),
            Phase::Failed(f) => s.label(crate::store::update_failure_key(f)),
        }
    };
    let action = move || match phase() {
        Phase::Available { .. } => "Download and Install",
        Phase::Ready { .. } => "Restart Now",
        _ => "Check Now",
    };
    section(vec![
        row(
            s,
            "Check for Updates Automatically",
            help(s, "Checks for a new version once a day"),
            switch_control(
                move || s.label("Check for Updates Automatically"),
                checks,
                move || !configured,
                move |v| s.update_preferences(move |p| p.update_checks = Some(v)),
            ),
        )
        .into_element(),
        // Downloading on its own only follows automatic checks.
        row(
            s,
            "Download and Install Automatically",
            help(s, "New versions install when you quit Safay"),
            switch_control(
                move || s.label("Download and Install Automatically"),
                move || s.preferences.with(|p| p.update_downloads),
                move || !configured || !checks(),
                move |v| s.update_preferences(move |p| p.update_downloads = v),
            ),
        )
        .into_element(),
        row(
            s,
            "Software Update",
            Some(Box::new(status)),
            push_button(move || s.label(action()), Kind::Standard, false).disabled(move || !configured || busy()).on_press(move || {
                match phase() {
                    Phase::Available { version, notes } => s.offer_update(version, notes),
                    Phase::Ready { .. } => s.restart_to_update(),
                    _ => s.check_updates(crate::store::Check::Manual),
                }
            }),
        )
        .into_element(),
    ])
}
fn platform_name() -> &'static str {
    match look() {
        Look::Mac => "macOS",
        Look::Windows => "Windows",
        Look::Linux => "Linux",
    }
}
