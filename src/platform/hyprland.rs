//! Placing the tray's quick panel and binding the global shortcuts' keys on Hyprland. Wayland lets no app place its windows and neither GTK 4 nor NSH can, so
//! elsewhere the compositor puts the panel where it likes (most center it). Hyprland takes window rules over its IPC
//! socket: before the panel opens, a rule floats it at the top right of the focused monitor, under the bar's reserved
//! area, as a menu bar panel sits. The rule matches only the panel's title and the app's class, and is replaced each
//! time. Nothing is read from Hyprland but the monitors' layout.
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    time::Duration,
};

/// Space between the panel and the monitor's usable edges, in points.
const EDGE: f64 = 8.0;

/// Sets the rule that places the window titled `title` (`width` points wide) at the focused monitor's top right. Does
/// nothing outside Hyprland or when its socket does not answer.
pub fn place_at_top_right(title: &str, width: f64) {
    let Some(socket) = socket() else { return };
    let Some(monitors) = request(&socket, "j/monitors") else { return };
    let Some((x, y)) = top_right(&monitors, width) else { return };
    let class = super::APP_ID.replace('.', "\\\\.");
    let title = title.replace(['\\', '"'], "");
    let rule = format!(
        "eval hl.window_rule({{ name = \"safay-panel\", enabled = true, match = {{ class = \"^({class})$\", title = \"^({title})$\" }}, \
         float = true, move = \"{x} {y}\" }})"
    );
    drop(request(&socket, &rule));
}

/// Hyprland's request socket for this session.
fn socket() -> Option<PathBuf> {
    let signature = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    Some(PathBuf::from(runtime).join("hypr").join(signature).join(".socket.sock"))
}

/// One request, answered once the compositor closes the connection; at most a second and 1 MB.
fn request(socket: &PathBuf, command: &str) -> Option<String> {
    let mut stream = UnixStream::connect(socket).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(1))).ok()?;
    stream.set_write_timeout(Some(Duration::from_secs(1))).ok()?;
    stream.write_all(command.as_bytes()).ok()?;
    let mut answer = Vec::new();
    stream.take(1024 * 1024).read_to_end(&mut answer).ok()?;
    String::from_utf8(answer).ok()
}

/// The panel's top left corner in the focused monitor's own coordinates (a rule's `move` is relative to the monitor),
/// in logical pixels: `EDGE` inside the area the bars leave (`reserved`: left, top, right, bottom).
fn top_right(monitors: &str, width: f64) -> Option<(i64, i64)> {
    let monitors: serde_json::Value = serde_json::from_str(monitors).ok()?;
    let monitor = monitors.as_array()?.iter().find(|m| m["focused"].as_bool() == Some(true))?;
    let scale = monitor["scale"].as_f64().filter(|s| *s > 0.0)?;
    // A monitor turned a quarter shows its height across.
    let turned = monitor["transform"].as_i64().is_some_and(|t| t % 2 == 1);
    let (w, h) = (monitor["width"].as_f64()?, monitor["height"].as_f64()?);
    let logical_width = if turned { h } else { w } / scale;
    let reserved = monitor["reserved"].as_array()?;
    let (top, right) = (reserved.get(1)?.as_f64()?, reserved.get(2)?.as_f64()?);
    let x = (logical_width - right - width - EDGE).max(EDGE);
    Some((x.round() as i64, (top + EDGE).round() as i64))
}

/// Binds each of the app's global shortcuts (NSH keystrokes, `alt+cmd+z`) to its key on Hyprland, whose portal
/// registers shortcuts but binds no key (`bind = …, global, <app id>:<id>` in the user's config otherwise). The keys this
/// app bound before in this Hyprland session (a Lua global, gone with a config reload, as the binds) are unbound first;
/// a key the user's config already binds is left to it. Does nothing outside Hyprland.
pub fn bind_shortcuts(keystrokes: &[String]) {
    let Some(socket) = socket() else { return };
    if request(&socket, "eval for _, k in ipairs(safay_keys or {}) do hl.unbind(k) end safay_keys = {}").is_none() {
        return;
    }
    let taken = request(&socket, "j/binds").map(|binds| bound_keys(&binds)).unwrap_or_default();
    let mut lua = String::new();
    for keystroke in keystrokes {
        let Ok(shortcut) = nsh_core::desktop::Shortcut::parse(keystroke, true) else { continue };
        let Some((keys, modmask, key)) = hyprland_keys(&shortcut) else { continue };
        if taken.iter().any(|(m, k)| *m == modmask && k.eq_ignore_ascii_case(&key)) {
            continue;
        }
        // NSH's id for it on the portal (`global_shortcut::portal_id`): its label, lowercase, Win as Super.
        let id = shortcut.label().to_lowercase().replace("win+", "super+");
        lua.push_str(&format!("hl.bind(\"{keys}\", hl.dsp.global(\"{}:{id}\")) table.insert(safay_keys, \"{keys}\") ", super::APP_ID));
    }
    if !lua.is_empty() {
        drop(request(&socket, &format!("eval {lua}")));
    }
}

/// While `on`, a click outside the window titled `title` (the tray panel) closes it, as a click anywhere else closes a
/// menu bar panel on macOS. Wayland tells no app of clicks outside its windows, and a click on the desktop moves no
/// focus, so no blur closes the panel there. On Hyprland a left-click bind that consumes nothing (the click still
/// reaches the desktop or the window under it) checks the pointer against the panel and asks the panel to close. The
/// bind is made once a Hyprland session (a Lua global) and only switched on and off. Does nothing outside Hyprland.
pub fn close_on_outside_click(title: &str, on: bool) {
    let Some(socket) = socket() else { return };
    let title = title.replace(['\\', '"'], "");
    let lua = if on {
        format!(
            "safay_click_title = \"{title}\" \
             if not safay_click then safay_click = hl.bind(\"mouse:272\", function() \
               local w = hl.get_windows({{ title = safay_click_title }})[1] \
               if not w then safay_click:set_enabled(false) return end \
               local c = hl.get_cursor_pos() \
               if c.x < w.at.x or c.y < w.at.y or c.x >= w.at.x + w.size.x or c.y >= w.at.y + w.size.y then \
                 hl.dispatch(hl.dsp.window.close({{ window = \"address:\" .. w.address }})) \
               end \
             end, {{ non_consuming = true }}) end \
             safay_click:set_enabled(true)"
        )
    } else {
        "if safay_click then safay_click:set_enabled(false) end".to_owned()
    };
    drop(request(&socket, &format!("eval {lua}")));
}

/// Unbinds the keys this app bound (`bind_shortcuts`), so the next keystroke reaches the app: the shortcut editor records
/// while they are off. Does nothing outside Hyprland.
pub fn unbind_shortcuts() {
    let Some(socket) = socket() else { return };
    drop(request(&socket, "eval for _, k in ipairs(safay_keys or {}) do hl.unbind(k) end safay_keys = {}"));
}

/// The modifier bits and key of every bind Hyprland has.
fn bound_keys(binds: &str) -> Vec<(i64, String)> {
    let Ok(serde_json::Value::Array(binds)) = serde_json::from_str(binds) else { return Vec::new() };
    binds.iter().filter_map(|b| Some((b["modmask"].as_i64()?, b["key"].as_str()?.to_owned()))).collect()
}

/// How Hyprland writes `shortcut` (`SUPER + ALT + Z`), its modifier bits (Super 64, Alt 8, Control 4, Shift 1) and its
/// key's name (xkb's).
fn hyprland_keys(shortcut: &nsh_core::desktop::Shortcut) -> Option<(String, i64, String)> {
    use nsh_core::desktop::Key;
    let key = match shortcut.key {
        Key::Char(c) if c.is_ascii_alphanumeric() => c.to_ascii_uppercase().to_string(),
        Key::Space => "space".into(),
        Key::Enter => "Return".into(),
        Key::Tab => "Tab".into(),
        Key::Backspace => "BackSpace".into(),
        Key::Left => "Left".into(),
        Key::Right => "Right".into(),
        Key::Up => "Up".into(),
        Key::Down => "Down".into(),
        Key::F(n) => format!("F{n}"),
        _ => return None,
    };
    let mut parts = Vec::new();
    let mut modmask = 0;
    for (held, name, bit) in
        [(shortcut.meta, "SUPER", 64), (shortcut.ctrl, "CTRL", 4), (shortcut.alt, "ALT", 8), (shortcut.shift, "SHIFT", 1)]
    {
        if held {
            parts.push(name);
            modmask |= bit;
        }
    }
    parts.push(&key);
    Some((parts.join(" + "), modmask, key.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_panel_sits_under_the_bar_at_the_focused_monitors_right() {
        let monitors = r#"[
            {"name":"DP-1","focused":false,"width":2560,"height":1440,"scale":1.0,"transform":0,"reserved":[0,0,0,0]},
            {"name":"eDP-1","focused":true,"width":1920,"height":1080,"scale":1.25,"transform":0,"reserved":[0,26,0,0]}
        ]"#;
        // 1536 logical pixels wide: 1536 - 310 - 8.
        assert_eq!(top_right(monitors, 310.0), Some((1218, 34)));
        let turned = r#"[{"focused":true,"width":1920,"height":1080,"scale":1.0,"transform":1,"reserved":[0,0,40,0]}]"#;
        assert_eq!(top_right(turned, 310.0), Some((1080 - 40 - 310 - 8, 8)));
        assert_eq!(top_right("[]", 310.0), None);
        assert_eq!(top_right("not json", 310.0), None);
    }
    #[test]
    fn shortcuts_are_written_as_hyprland_binds_them() {
        let keys = |k: &str| hyprland_keys(&nsh_core::desktop::Shortcut::parse(k, true).unwrap());
        assert_eq!(keys("alt+cmd+z"), Some(("SUPER + ALT + Z".into(), 72, "Z".into())));
        assert_eq!(keys("ctrl+shift+space"), Some(("CTRL + SHIFT + space".into(), 5, "space".into())));
        assert_eq!(keys("cmd+f5"), Some(("SUPER + F5".into(), 64, "F5".into())));
        let binds = r#"[{"modmask":72,"key":"F","dispatcher":"__lua"},{"modmask":64,"key":"return"}]"#;
        assert_eq!(bound_keys(binds), vec![(72, "F".to_owned()), (64, "return".to_owned())]);
        assert!(bound_keys("not json").is_empty());
    }
}
