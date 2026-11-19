//! Placing the tray's quick panel on Hyprland. Wayland lets no app place its windows and neither GTK 4 nor NSH can, so
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
}
