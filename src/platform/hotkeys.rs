//! The global shortcuts' stored form and editor. The shortcuts themselves are registered through NSH
//! (`register_global_shortcut`, in `desktop.rs`); the preferences store a macOS virtual key
//! code with Carbon modifier bits. Linux stores the same form: its editor maps NSH's key events onto it
//! ([`recorded`]), and Command is the Super key there.

/// The keystroke NSH registers for `h` (`"alt+cmd+z"`): the key by its US-layout name, as [`label`] shows it. `None`
/// for a key outside the names below, which is then not registered.
pub fn keystroke(h: safay_app::preferences::Hotkey) -> Option<String> {
    let key = match key_name(h.key) {
        "?" => return None,
        "Space" => "space".to_owned(),
        "↩" => "enter".to_owned(),
        "⇥" => "tab".to_owned(),
        "⌫" => "backspace".to_owned(),
        "←" => "left".to_owned(),
        "→" => "right".to_owned(),
        "↓" => "down".to_owned(),
        "↑" => "up".to_owned(),
        name => name.to_ascii_lowercase(),
    };
    let mut out = String::new();
    for (bit, name) in [(0x1000, "ctrl"), (0x800, "alt"), (0x200, "shift"), (0x100, "cmd")] {
        if h.modifiers & bit != 0 {
            out.push_str(name);
            out.push('+');
        }
    }
    out.push_str(&key);
    Some(out)
}

/// "⌥⌘Z": modifiers in display order (⌃⌥⇧⌘) and the key's label on a US layout. Linux: "Super+Alt+Z", as its desktops
/// write shortcuts.
pub fn label(h: safay_app::preferences::Hotkey) -> String {
    if cfg!(target_os = "linux") {
        let mut out = String::new();
        for (bit, name) in [(0x100, "Super"), (0x1000, "Ctrl"), (0x800, "Alt"), (0x200, "Shift")] {
            if h.modifiers & bit != 0 {
                out.push_str(name);
                out.push('+');
            }
        }
        out.push_str(match key_name(h.key) {
            "↩" => "Enter",
            "⇥" => "Tab",
            "⌫" => "Backspace",
            "←" => "Left",
            "→" => "Right",
            "↓" => "Down",
            "↑" => "Up",
            name => name,
        });
        return out;
    }
    let mut out = String::new();
    for (bit, sign) in [(0x1000, "⌃"), (0x800, "⌥"), (0x200, "⇧"), (0x100, "⌘")] {
        if h.modifiers & bit != 0 {
            out.push_str(sign);
        }
    }
    out.push_str(key_name(h.key));
    out
}
fn key_name(key: u16) -> &'static str {
    const LETTERS: [(u16, &str); 50] = [
        (0, "A"),
        (11, "B"),
        (8, "C"),
        (2, "D"),
        (14, "E"),
        (3, "F"),
        (5, "G"),
        (4, "H"),
        (34, "I"),
        (38, "J"),
        (40, "K"),
        (37, "L"),
        (46, "M"),
        (45, "N"),
        (31, "O"),
        (35, "P"),
        (12, "Q"),
        (15, "R"),
        (1, "S"),
        (17, "T"),
        (32, "U"),
        (9, "V"),
        (13, "W"),
        (7, "X"),
        (16, "Y"),
        (6, "Z"),
        (29, "0"),
        (18, "1"),
        (19, "2"),
        (20, "3"),
        (21, "4"),
        (23, "5"),
        (22, "6"),
        (26, "7"),
        (28, "8"),
        (25, "9"),
        (49, "Space"),
        (36, "↩"),
        (48, "⇥"),
        (51, "⌫"),
        (123, "←"),
        (124, "→"),
        (125, "↓"),
        (126, "↑"),
        (122, "F1"),
        (120, "F2"),
        (99, "F3"),
        (118, "F4"),
        (96, "F5"),
        (97, "F6"),
    ];
    LETTERS.iter().find(|(k, _)| *k == key).map(|(_, n)| *n).unwrap_or("?")
}

/// What the shortcut editor recorded: `None` cancelled (Escape), `Some(None)` cleared (Delete), or the new shortcut.
pub type Recorded = Option<Option<safay_app::preferences::Hotkey>>;

/// The shortcut editor's key press on Linux, from NSH's key event (`code` is the DOM's physical key, `KeyZ`): Escape
/// cancels, Backspace or Delete clears, a lone modifier or a key without Super, Ctrl or Alt waits for the next press
/// (`None`), and a key outside the stored names is not taken either.
#[cfg(target_os = "linux")]
pub fn recorded(code: &str, modifiers: nsh_core::keymap::Modifiers) -> Option<Recorded> {
    match code {
        "Escape" => return Some(None),
        "Backspace" | "Delete" => return Some(Some(None)),
        _ => {}
    }
    let name = if let Some(letter) = code.strip_prefix("Key") {
        letter
    } else if let Some(digit) = code.strip_prefix("Digit") {
        digit
    } else {
        match code {
            "Space" => "Space",
            "Enter" => "↩",
            "Tab" => "⇥",
            "ArrowLeft" => "←",
            "ArrowRight" => "→",
            "ArrowDown" => "↓",
            "ArrowUp" => "↑",
            "F1" | "F2" | "F3" | "F4" | "F5" | "F6" => code,
            _ => return None,
        }
    };
    let key = (0..=u16::from(u8::MAX)).find(|k| key_name(*k) == name)?;
    let mut bits = 0;
    for (held, bit) in [(modifiers.cmd, 0x100), (modifiers.shift, 0x200), (modifiers.alt, 0x800), (modifiers.ctrl, 0x1000)] {
        if held {
            bits |= bit;
        }
    }
    if bits & (0x100 | 0x800 | 0x1000) == 0 {
        return None;
    }
    Some(Some(Some(safay_app::preferences::Hotkey { key, modifiers: bits })))
}

/// Records the next key press in this app for the shortcut editor: Escape cancels, Delete clears. AppKit hands the key
/// to its monitor on the main thread, where the UI's signals may not be touched: the result comes through the channel,
/// for the engine thread to apply.
#[cfg(target_os = "macos")]
pub fn record() -> std::sync::mpsc::Receiver<Recorded> {
    use nsh_platform_macos::{
        objc2::rc::Retained,
        objc2::runtime::AnyObject,
        objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags},
    };
    use std::{cell::RefCell, rc::Rc};
    let (sender, receiver) = std::sync::mpsc::channel::<Recorded>();
    let sender = RefCell::new(Some(sender));
    let monitor: Rc<RefCell<Option<Retained<AnyObject>>>> = Rc::new(RefCell::new(None));
    let monitor2 = monitor.clone();
    let block = block2::RcBlock::new(move |event: std::ptr::NonNull<NSEvent>| -> *mut NSEvent {
        let event_ref = unsafe { event.as_ref() };
        let code = event_ref.keyCode();
        let flags = event_ref.modifierFlags();
        let result = match code {
            53 => Some(None),
            51 | 117 => Some(Some(None)),
            _ => {
                let mut modifiers = 0;
                for (flag, bit) in [
                    (NSEventModifierFlags::Command, 0x100),
                    (NSEventModifierFlags::Shift, 0x200),
                    (NSEventModifierFlags::Option, 0x800),
                    (NSEventModifierFlags::Control, 0x1000),
                ] {
                    if flags.contains(flag) {
                        modifiers |= bit;
                    }
                }
                if modifiers & (0x100 | 0x800 | 0x1000) == 0 {
                    None
                } else {
                    Some(Some(Some(safay_app::preferences::Hotkey { key: code, modifiers })))
                }
            }
        };
        let Some(result) = result else { return std::ptr::null_mut() };
        if let Some(m) = monitor2.borrow_mut().take() {
            unsafe { NSEvent::removeMonitor(&m) };
        }
        if let Some(sender) = sender.borrow_mut().take() {
            let _ = sender.send(result);
        }
        std::ptr::null_mut()
    });
    let token = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block) };
    *monitor.borrow_mut() = token;
    receiver
}

#[cfg(test)]
mod tests {
    use super::*;
    use safay_app::preferences::Hotkey;

    #[test]
    fn stored_shortcuts_become_keystrokes_nsh_parses() {
        let parse = |k: &str| nsh_core::desktop::Shortcut::parse(k, true).is_ok();
        // ⌥⌘Z, ⌃⇧Space, ⌘F5, and one of every named key.
        assert_eq!(keystroke(Hotkey { key: 6, modifiers: 0x800 | 0x100 }).as_deref(), Some("alt+cmd+z"));
        assert_eq!(keystroke(Hotkey { key: 49, modifiers: 0x1000 | 0x200 }).as_deref(), Some("ctrl+shift+space"));
        assert_eq!(keystroke(Hotkey { key: 96, modifiers: 0x100 }).as_deref(), Some("cmd+f5"));
        for key in [0, 29, 49, 36, 48, 51, 123, 124, 125, 126, 122] {
            let k = keystroke(Hotkey { key, modifiers: 0x100 }).unwrap();
            assert!(parse(&k), "{k}");
        }
        // A key without a name is not registered.
        assert_eq!(keystroke(Hotkey { key: 200, modifiers: 0x100 }), None);
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_key_presses_become_the_stored_form() {
        let held = |cmd, alt, ctrl, shift| nsh_core::keymap::Modifiers { cmd, alt, ctrl, shift };
        // Super+Alt+Z is ⌥⌘Z's stored form, and reads back as Linux writes it.
        let z = recorded("KeyZ", held(true, true, false, false));
        assert_eq!(z, Some(Some(Some(Hotkey { key: 6, modifiers: 0x100 | 0x800 }))));
        assert_eq!(label(Hotkey { key: 6, modifiers: 0x100 | 0x800 }), "Super+Alt+Z");
        assert_eq!(label(Hotkey { key: 123, modifiers: 0x1000 | 0x200 }), "Ctrl+Shift+Left");
        assert_eq!(recorded("Digit7", held(false, false, true, false)), Some(Some(Some(Hotkey { key: 26, modifiers: 0x1000 }))));
        // Escape cancels, Backspace clears; Shift alone, a lone modifier and an unnamed key wait for another press.
        assert_eq!(recorded("Escape", held(false, false, false, false)), Some(None));
        assert_eq!(recorded("Backspace", held(false, false, false, false)), Some(Some(None)));
        assert_eq!(recorded("KeyA", held(false, false, false, true)), None);
        assert_eq!(recorded("AltLeft", held(false, true, false, false)), None);
        assert_eq!(recorded("F12", held(true, false, false, false)), None);
    }
}
