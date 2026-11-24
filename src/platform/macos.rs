//! System lock observers: sleep, display sleep, session switch, screen lock
//! (including Control-Command-Q) and screen saver start lock at once. Leaving the app (clicking another window) does not
//! lock: the auto-lock timer covers inactivity. No global input/content monitoring.
use nsh_platform_macos::{
    objc2::{rc::Retained, runtime::ProtocolObject},
    objc2_app_kit::{
        NSWorkspace, NSWorkspaceScreensDidSleepNotification, NSWorkspaceSessionDidResignActiveNotification,
        NSWorkspaceWillSleepNotification,
    },
    objc2_foundation::{NSDistributedNotificationCenter, NSNotification, NSNotificationCenter, NSObjectProtocol, NSString},
};
use safay_app::controller::Controller;
use std::sync::Arc;

pub fn observe(controller: Arc<Controller>) -> Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>> {
    let mut tokens = Vec::new();
    let watch = |center: &NSNotificationCenter, name: &NSString, tokens: &mut Vec<_>| {
        let copy = controller.clone();
        // The block captures only a thread-safe controller, and observes all senders on their posting queue.
        let callback = block2::RcBlock::new(move |_: std::ptr::NonNull<NSNotification>| copy.request_lock());
        tokens.push(unsafe { center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &callback) });
    };
    let workspace = NSWorkspace::sharedWorkspace().notificationCenter();
    for name in
        unsafe { [NSWorkspaceWillSleepNotification, NSWorkspaceScreensDidSleepNotification, NSWorkspaceSessionDidResignActiveNotification] }
    {
        watch(&workspace, name, &mut tokens);
    }
    // Screen lock and the screen saver exist only as distributed notifications, which carry no content.
    let distributed = NSDistributedNotificationCenter::defaultCenter();
    for name in ["com.apple.screenIsLocked", "com.apple.screensaver.didstart"] {
        watch(&distributed, &NSString::from_str(name), &mut tokens);
    }
    tokens
}

/// Counts input delivered to this app as activity, for the auto-lock timer.
pub fn configure(controller: Arc<Controller>) {
    use nsh_platform_macos::{
        objc2::runtime::AnyObject,
        objc2_app_kit::{NSEvent, NSEventMask},
    };
    dispatch2::DispatchQueue::main().exec_async(move || {
        let callback = block2::RcBlock::new(move |event: std::ptr::NonNull<NSEvent>| {
            controller.activity();
            event.as_ptr()
        });
        // Returns the original event unchanged. The monitor observes no events outside this process.
        let token: Option<Retained<AnyObject>> = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(
                NSEventMask::KeyDown | NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown | NSEventMask::ScrollWheel,
                &callback,
            )
        };
        // AppKit retains the monitor until process termination; the callback holds no AppKit object.
        let _ = token;
    });
}

use std::sync::atomic::{AtomicU8, Ordering};
/// 0 always, 1 only while the main window is open, 2 never (the Dock icon setting).
static DOCK: AtomicU8 = AtomicU8::new(1);
/// Whether the main window is on screen, as the app last showed or hid it ([`apply_dock`]). AppKit's window list does
/// not tell: its first window is NSH's app host, which never shows.
static MAIN_SHOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Applies the Dock policy. Only presentation state crosses into AppKit; the tray and the shortcuts are NSH's
/// (`desktop.rs`).
pub fn apply_preferences(p: safay_app::preferences::Preferences) {
    use safay_app::preferences::DockIcon;
    DOCK.store(
        match p.dock_icon {
            DockIcon::Always => 0,
            DockIcon::MainWindow => 1,
            DockIcon::Never => 2,
        },
        Ordering::SeqCst,
    );
    set_policy(None);
}
/// Regular (Dock icon) or accessory (menu bar only) activation policy for the main window's state. Any thread.
pub fn apply_dock(main_open: bool) {
    MAIN_SHOWN.store(main_open, Ordering::SeqCst);
    set_policy(Some(main_open));
}
/// Whether the main window is on screen, as the app last showed or hid it.
pub fn main_shown() -> bool {
    MAIN_SHOWN.load(Ordering::SeqCst)
}
/// `main_open`: `None` takes what the main window was last said to be.
fn set_policy(main_open: Option<bool>) {
    use nsh_platform_macos::{
        objc2::MainThreadMarker,
        objc2_app_kit::{NSApplication, NSApplicationActivationPolicy},
    };
    dispatch2::DispatchQueue::main().exec_async(move || {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let app = NSApplication::sharedApplication(mtm);
        let regular = match DOCK.load(Ordering::SeqCst) {
            0 => true,
            2 => false,
            _ => main_open.unwrap_or_else(|| MAIN_SHOWN.load(Ordering::SeqCst)),
        };
        app.setActivationPolicy(if regular { NSApplicationActivationPolicy::Regular } else { NSApplicationActivationPolicy::Accessory });
    });
}
thread_local! {
    /// Main thread: the monitor of clicks in other apps while the menu bar panel is open.
    static OUTSIDE: std::cell::RefCell<Option<Retained<nsh_platform_macos::objc2::runtime::AnyObject>>> = const { std::cell::RefCell::new(None) };
}
/// Reports the next clicks in other apps (the desktop and the menu bar included) through `clicked`, until
/// [`unwatch_outside_clicks`]. The menu bar panel takes the keyboard without activating Safay, so such a click does not
/// take the keyboard from it and NSH's close-on-blur does not see it. The monitor
/// gets no event of this app and reads nothing from the event.
pub fn watch_outside_clicks(clicked: std::sync::mpsc::Sender<()>) {
    use nsh_platform_macos::objc2_app_kit::{NSEvent, NSEventMask};
    dispatch2::DispatchQueue::main().exec_async(move || {
        let block = block2::RcBlock::new(move |_: std::ptr::NonNull<NSEvent>| {
            let _ = clicked.send(());
        });
        let monitor =
            NSEvent::addGlobalMonitorForEventsMatchingMask_handler(NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown, &block);
        if let Some(old) = OUTSIDE.with(|slot| slot.replace(monitor)) {
            // SAFETY: the monitor `addGlobalMonitorForEventsMatchingMask_handler` returned.
            unsafe { NSEvent::removeMonitor(&old) };
        }
    });
}
/// Stops [`watch_outside_clicks`]; its sender goes with the monitor.
pub fn unwatch_outside_clicks() {
    dispatch2::DispatchQueue::main().exec_async(|| {
        if let Some(old) = OUTSIDE.with(|slot| slot.borrow_mut().take()) {
            // SAFETY: the monitor `addGlobalMonitorForEventsMatchingMask_handler` returned.
            unsafe { nsh_platform_macos::objc2_app_kit::NSEvent::removeMonitor(&old) };
        }
    });
}
static LIGHT_HEIGHT: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
/// The height of a window's traffic lights, as AppKit sizes them on this release. NSH places them by their top-left
/// corner (`TitleBarOptions::controls_offset`), so the app centers them in its bar with this. Read on the main thread
/// first (`main` calls it before the runner starts); any thread after that. 14 pt when it can't be read.
pub fn traffic_light_height() -> f64 {
    *LIGHT_HEIGHT.get_or_init(|| {
        use nsh_platform_macos::{
            objc2::MainThreadMarker,
            objc2_app_kit::{NSWindow, NSWindowButton, NSWindowStyleMask},
        };
        let Some(mtm) = MainThreadMarker::new() else { return 14.0 };
        let mask = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable;
        NSWindow::standardWindowButton_forStyleMask(NSWindowButton::CloseButton, mask, mtm).map_or(14.0, |b| b.frame().size.height)
    })
}
static ICLOUD_CHANGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static PUSH_INSTALLED: std::sync::Once = std::sync::Once::new();
/// Another device changed the iCloud backup (a silent push of this app's subscription) since the last call.
pub fn take_icloud_change() -> bool {
    ICLOUD_CHANGED.swap(false, Ordering::SeqCst)
}
/// `application:didReceiveRemoteNotification:` on NSH's application delegate: only this app's backup subscription counts
/// (`platform::icloud::is_backup_change`); it raises a flag the main window's store polls, and nothing else is read.
unsafe extern "C-unwind" fn did_receive_remote_notification(
    _this: *mut nsh_platform_macos::objc2::runtime::AnyObject,
    _cmd: nsh_platform_macos::objc2::runtime::Sel,
    _app: *mut nsh_platform_macos::objc2::runtime::AnyObject,
    user_info: *mut nsh_platform_macos::objc2_foundation::NSDictionary,
) {
    // SAFETY: AppKit passes the notification's `userInfo` dictionary (or nil).
    if let Some(info) = unsafe { user_info.as_ref() }
        && super::icloud::is_backup_change(info)
    {
        ICLOUD_CHANGED.store(true, Ordering::SeqCst);
    }
}
/// Silent pushes for iCloud backups: registers for remote notifications and gives
/// NSH's application delegate the method AppKit delivers them to. NSH has no remote notification API and owns the
/// delegate, so the method is added to the delegate's class at run time, only if the class has none (it does not
/// replace anything of NSH's). Without the push entitlement (`aps-environment`, from a provisioning profile with the
/// capability) registration fails quietly and backups sync on unlock, after saves and by hand, as before. Any thread.
pub fn enable_remote_push() {
    use nsh_platform_macos::{
        objc2::{MainThreadMarker, ffi, msg_send, runtime::AnyClass, sel},
        objc2_app_kit::NSApplication,
    };
    dispatch2::DispatchQueue::main().exec_async(|| {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let app = NSApplication::sharedApplication(mtm);
        PUSH_INSTALLED.call_once(|| {
            let Some(delegate) = app.delegate() else { return };
            let class: &AnyClass = AsRef::<nsh_platform_macos::objc2::runtime::AnyObject>::as_ref(&*delegate).class();
            // SAFETY: the implementation matches the selector's signature (`v@:@@`: self, _cmd, the app, `userInfo`); a
            // method the class already has is left alone (class_addMethod returns NO).
            unsafe {
                let imp: nsh_platform_macos::objc2::runtime::Imp = std::mem::transmute(
                    did_receive_remote_notification
                        as unsafe extern "C-unwind" fn(
                            *mut nsh_platform_macos::objc2::runtime::AnyObject,
                            nsh_platform_macos::objc2::runtime::Sel,
                            *mut nsh_platform_macos::objc2::runtime::AnyObject,
                            *mut nsh_platform_macos::objc2_foundation::NSDictionary,
                        ),
                );
                ffi::class_addMethod(
                    class as *const AnyClass as *mut AnyClass,
                    sel!(application:didReceiveRemoteNotification:),
                    imp,
                    c"v@:@@".as_ptr(),
                );
            }
        });
        // SAFETY: NSApplication's own method, on the main thread.
        let _: () = unsafe { msg_send![&*app, registerForRemoteNotifications] };
    });
}
/// Where the pointer is, in AppKit's screen coordinates (from the primary display's bottom left). NSH reports the
/// displays, not the pointer.
pub fn pointer() -> Option<(f64, f64)> {
    let at = nsh_platform_macos::objc2_app_kit::NSEvent::mouseLocation();
    Some((at.x, at.y))
}

/// Which keys an [`alert`] button answers.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AlertKey {
    None,
    Return,
    Escape,
    /// The safe choice of a warning: Return and Escape both pick it.
    ReturnAndEscape,
}
/// One button of an [`alert`], in AppKit's order: the first is the prominent one (on top, or rightmost).
pub struct AlertButton {
    pub label: String,
    pub key: AlertKey,
    pub destructive: bool,
}
/// Shows an `NSAlert` as a sheet on the key window (else the main, else the first visible one) and reports the index of
/// the chosen button, `None` when it closed without one; `Err` when the app has no window to attach it to.
///
/// NSH presents its `AlertDialog` while applying a UI batch inside a Core Animation transaction, so AppKit's appear
/// animation never reaches the screen and the alert pops up only when it is over (about 0.3 s after the click).
/// Presenting from the main queue, after that batch has returned, shows it at once with the system animation.
pub fn alert(title: String, message: Option<String>, buttons: Vec<AlertButton>) -> std::sync::mpsc::Receiver<Result<Option<usize>, ()>> {
    use nsh_platform_macos::{
        objc2::{MainThreadMarker, runtime::AnyObject},
        objc2_app_kit::{
            NSAlert, NSAlertFirstButtonReturn, NSAlertStyle, NSAppearanceCustomization, NSEvent, NSEventMask, NSModalResponse,
        },
    };
    let (sender, receiver) = std::sync::mpsc::channel();
    dispatch2::DispatchQueue::main().exec_async(move || {
        let Some(mtm) = MainThreadMarker::new() else {
            let _ = sender.send(Err(()));
            return;
        };
        let Some(window) = sheet_parent(mtm) else {
            let _ = sender.send(Err(()));
            return;
        };
        let alert = NSAlert::new(mtm);
        alert.setAlertStyle(NSAlertStyle::Informational);
        alert.setMessageText(&NSString::from_str(&title));
        if let Some(message) = &message {
            alert.setInformativeText(&NSString::from_str(message));
        }
        let mut escape_too = None;
        for b in &buttons {
            let button = alert.addButtonWithTitle(&NSString::from_str(&b.label));
            button.setKeyEquivalent(&NSString::from_str(match b.key {
                AlertKey::Return | AlertKey::ReturnAndEscape => "\r",
                AlertKey::Escape => "\u{1b}",
                AlertKey::None => "",
            }));
            if b.key == AlertKey::ReturnAndEscape {
                escape_too = Some(button.clone());
            }
            if b.destructive {
                button.setHasDestructiveAction(true);
            }
        }
        // The sheet follows the window's appearance, which the app may force.
        alert.window().setAppearance(Some(&window.effectiveAppearance()));
        // A button has one key equivalent: Escape reaches the one that also has Return through a monitor of this app's
        // key presses, only while the sheet is up.
        let monitor: Option<Retained<AnyObject>> = escape_too.and_then(|button| {
            let block = block2::RcBlock::new(move |event: std::ptr::NonNull<NSEvent>| {
                // SAFETY: AppKit hands the monitor a live event.
                if unsafe { event.as_ref() }.keyCode() == 53 {
                    // SAFETY: a button of the alert on screen, clicked on the main thread.
                    unsafe { button.performClick(None) };
                    return std::ptr::null_mut();
                }
                event.as_ptr()
            });
            // SAFETY: the handler returns the event or null, as the API requires.
            unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block) }
        });
        let monitor = std::cell::RefCell::new(monitor);
        let count = buttons.len();
        let handler = block2::RcBlock::new(move |response: NSModalResponse| {
            if let Some(monitor) = monitor.borrow_mut().take() {
                // SAFETY: the monitor `addLocalMonitorForEventsMatchingMask_handler` returned.
                unsafe { NSEvent::removeMonitor(&monitor) };
            }
            let index = usize::try_from(response - NSAlertFirstButtonReturn).ok().filter(|i| *i < count);
            let _ = sender.send(Ok(index));
        });
        alert.beginSheetModalForWindow_completionHandler(&window, Some(&handler));
    });
    receiver
}

/// The window a sheet attaches to: the key window, else the main one, else the first visible one without a sheet.
fn sheet_parent(mtm: nsh_platform_macos::objc2::MainThreadMarker) -> Option<Retained<nsh_platform_macos::objc2_app_kit::NSWindow>> {
    let app = nsh_platform_macos::objc2_app_kit::NSApplication::sharedApplication(mtm);
    app.keyWindow()
        .or_else(|| app.mainWindow())
        .or_else(|| app.windows().iter().find(|w| w.isVisible() && w.canBecomeMainWindow() && w.attachedSheet().is_none()))
}

/// What a [`file_panel`] asks for.
pub enum FilePanel {
    /// Files (several when `multiple`) or folders.
    Open { folders: bool, multiple: bool },
    /// Where to save, suggesting `name`.
    Save { name: Option<String> },
}
/// Shows an open or save panel as a sheet on the key window, from the main queue for the reason given at [`alert`]
/// (NSH's `pick_paths` sheet appears about 0.6 s late, without its animation). `message` is the line the sheet shows,
/// `button` its confirm button, `extensions` the file types offered (none: any). Reports the chosen paths, `None` when
/// cancelled, `Err` when the app has no window to attach it to.
pub fn file_panel(
    kind: FilePanel,
    message: Option<String>,
    button: Option<String>,
    extensions: Vec<String>,
) -> std::sync::mpsc::Receiver<Result<Option<Vec<std::path::PathBuf>>, ()>> {
    use nsh_platform_macos::{
        objc2::MainThreadMarker,
        objc2_app_kit::{NSModalResponse, NSModalResponseOK, NSOpenPanel, NSSavePanel},
        objc2_foundation::NSArray,
    };
    let (sender, receiver) = std::sync::mpsc::channel();
    dispatch2::DispatchQueue::main().exec_async(move || {
        let Some(window) = MainThreadMarker::new().and_then(sheet_parent) else {
            let _ = sender.send(Err(()));
            return;
        };
        let mtm = MainThreadMarker::new().expect("on the main queue");
        let (panel, open): (Retained<NSSavePanel>, Option<Retained<NSOpenPanel>>) = match &kind {
            FilePanel::Open { folders, multiple } => {
                let open = NSOpenPanel::openPanel(mtm);
                open.setCanChooseFiles(!folders);
                open.setCanChooseDirectories(*folders);
                open.setAllowsMultipleSelection(*multiple);
                open.setCanCreateDirectories(*folders);
                (Retained::into_super(open.clone()), Some(open))
            }
            FilePanel::Save { name } => {
                let save = NSSavePanel::savePanel(mtm);
                save.setCanCreateDirectories(true);
                if let Some(name) = name {
                    save.setNameFieldStringValue(&NSString::from_str(name));
                }
                (save, None)
            }
        };
        // Sheets show no title bar: the message is what the user reads.
        if let Some(message) = &message {
            panel.setMessage(Some(&NSString::from_str(message)));
        }
        if let Some(button) = &button {
            panel.setPrompt(Some(&NSString::from_str(button)));
        }
        if !extensions.is_empty() {
            let types: Vec<Retained<NSString>> = extensions.iter().map(|e| NSString::from_str(e)).collect();
            // `allowedContentTypes` needs UniformTypeIdentifiers; the extensions say the same (as NSH does).
            #[allow(deprecated)]
            panel.setAllowedFileTypes(Some(&NSArray::from_retained_slice(&types)));
        }
        let chosen = panel.clone();
        let handler = block2::RcBlock::new(move |response: NSModalResponse| {
            if response != NSModalResponseOK {
                let _ = sender.send(Ok(None));
                return;
            }
            let path = |url: &nsh_platform_macos::objc2_foundation::NSURL| url.path().map(|p| std::path::PathBuf::from(p.to_string()));
            let paths: Vec<std::path::PathBuf> = match &open {
                Some(open) => open.URLs().iter().filter_map(|u| path(&u)).collect(),
                None => chosen.URL().and_then(|u| path(&u)).into_iter().collect(),
            };
            let _ = sender.send(Ok((!paths.is_empty()).then_some(paths)));
        });
        panel.beginSheetModalForWindow_completionHandler(&window, &handler);
    });
    receiver
}

thread_local! {
    /// Main thread: the input context [`roman_input_only`] restricted, and the field it edits for (its delegate).
    static ROMAN: std::cell::RefCell<Option<(Retained<nsh_platform_macos::objc2_app_kit::NSTextInputContext>, usize)>> =
        const { std::cell::RefCell::new(None) };
}
/// Keeps password and secret fields to Roman input sources while they are edited: they take only ASCII, an input method
/// such as Pinyin shows candidates in a plain field, and in a masked one it keeps letters that form no syllable as marked
/// text the field never shows or commits, so they are lost. Restricts the input context of the field editor that edits
/// now (a masked field has its own; the others share the window's), and restores it when editing stops, unless focus has
/// already moved to another restricted field (the two calls arrive in no fixed order). Call it as the field takes and
/// loses the focus; it runs on the next turn of the main queue, once the field editor is attached.
pub fn roman_input_only(enabled: bool) {
    use nsh_platform_macos::{
        objc2::MainThreadMarker,
        objc2_app_kit::{NSAllRomanInputSourcesLocaleIdentifier, NSApplication, NSTextView},
        objc2_foundation::NSArray,
    };
    dispatch2::DispatchQueue::main().exec_async(move || {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let editor = NSApplication::sharedApplication(mtm)
            .keyWindow()
            .and_then(|w| w.firstResponder())
            .and_then(|r| r.downcast::<NSTextView>().ok());
        // The field an editor edits for, by address: its delegate.
        let field_of = |e: &NSTextView| e.delegate().map_or(0, |d| Retained::as_ptr(&d).cast::<()>() as usize);
        ROMAN.with(|slot| {
            let mut slot = slot.borrow_mut();
            if enabled {
                let Some(editor) = editor else { return };
                let Some(context) = editor.inputContext() else { return };
                // SAFETY: an AppKit constant string.
                let roman = unsafe { NSAllRomanInputSourcesLocaleIdentifier };
                context.setAllowedInputSourceLocales(Some(&NSArray::from_slice(&[roman])));
                *slot = Some((context, field_of(&editor)));
            } else {
                if let (Some((_, field)), Some(editor)) = (slot.as_ref(), editor.as_ref())
                    && *field == field_of(editor)
                {
                    return;
                }
                if let Some((context, _)) = slot.take() {
                    context.setAllowedInputSourceLocales(None);
                }
            }
        });
    });
}

/// Scrolls the field being edited so its caret shows: NSH puts the caret at the end when a field takes the focus but
/// leaves the field scrolled to its start, so a long text shows its beginning with the caret out of sight. Runs a moment
/// later, once NSH has finished focusing the field.
pub fn scroll_to_caret() {
    use nsh_platform_macos::{
        objc2::MainThreadMarker,
        objc2_app_kit::{NSApplication, NSTextView},
    };
    let when = dispatch2::DispatchTime::NOW.time(30_000_000);
    let _ = dispatch2::DispatchQueue::main().after(when, || {
        let Some(mtm) = MainThreadMarker::new() else { return };
        if let Some(editor) =
            NSApplication::sharedApplication(mtm).keyWindow().and_then(|w| w.firstResponder()).and_then(|r| r.downcast::<NSTextView>().ok())
        {
            editor.scrollRangeToVisible(editor.selectedRange());
        }
    });
}
