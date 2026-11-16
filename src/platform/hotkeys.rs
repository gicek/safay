//! The global shortcuts' stored form and editor. The shortcuts themselves are registered through NSH
//! (`register_global_shortcut`, in `desktop.rs`); the preferences store a macOS virtual key
//! code with Carbon modifier bits.

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

/// "⌥⌘Z": modifiers in display order (⌃⌥⇧⌘) and the key's label on a US layout.
pub fn label(h: safay_app::preferences::Hotkey) -> String {
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

/// Records the next key press in this app for the shortcut editor: Escape cancels, Delete clears.
pub fn record(done: impl FnOnce(Option<Option<safay_app::preferences::Hotkey>>) + 'static) {
    use nsh_platform_macos::{
        objc2::rc::Retained,
        objc2::runtime::AnyObject,
        objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags},
    };
    use std::{cell::RefCell, rc::Rc};
    type Done = Box<dyn FnOnce(Option<Option<safay_app::preferences::Hotkey>>)>;
    let slot: Rc<RefCell<Option<Done>>> = Rc::new(RefCell::new(Some(Box::new(done))));
    let monitor: Rc<RefCell<Option<Retained<AnyObject>>>> = Rc::new(RefCell::new(None));
    let (slot2, monitor2) = (slot.clone(), monitor.clone());
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
        if let Some(f) = slot2.borrow_mut().take() {
            f(result);
        }
        std::ptr::null_mut()
    });
    let token = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block) };
    *monitor.borrow_mut() = token;
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
}
