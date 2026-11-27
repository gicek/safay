//! iOS services NSH has no API for, through UIKit: the lock when the device locks, a cover over the app while it is in
//! the background (the app switcher's picture shows no codes), a clipboard entry that stays on this device and
//! expires, and the silent push of an iCloud backup change (NSH delivers the delegate's callback; registering is the
//! app's). Everything here runs on the main thread; [`start`] hands itself there.
use objc2::{MainThreadMarker, rc::Retained, runtime::ProtocolObject};
use objc2_foundation::{NSDate, NSDictionary, NSNotification, NSNotificationCenter, NSObjectProtocol, NSOperationQueue, NSString};
use objc2_ui_kit::{
    UIApplication, UIApplicationDidEnterBackgroundNotification, UIApplicationProtectedDataWillBecomeUnavailable,
    UIApplicationWillEnterForegroundNotification, UIBlurEffect, UIBlurEffectStyle, UIDevice, UIPasteboard,
    UIPasteboardOptionExpirationDate, UIPasteboardOptionLocalOnly, UISceneActivationState, UIUserInterfaceIdiom, UIView,
    UIViewAutoresizing, UIVisualEffectView, UIWindow, UIWindowLevelAlert, UIWindowScene,
};
use safay_app::controller::Controller;
use std::{
    cell::RefCell,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

static CONTROLLER: OnceLock<Arc<Controller>> = OnceLock::new();
/// The vault is open: the app is covered while in the background.
static PRIVATE: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// The observers [`start`] added, kept for the life of the process.
    static OBSERVERS: RefCell<Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>> = const { RefCell::new(Vec::new()) };
    /// The cover's window while it shows.
    static COVER: RefCell<Option<Retained<UIWindow>>> = const { RefCell::new(None) };
}

/// "iPhone" or "iPad": the name of this device in backup metadata and history. The device's own name needs an
/// entitlement and is never asked for.
pub fn device_model() -> String {
    let Some(mtm) = MainThreadMarker::new() else {
        let model = std::sync::Mutex::new(String::new());
        dispatch2::DispatchQueue::main().exec_sync(|| *model.lock().unwrap_or_else(|e| e.into_inner()) = device_model());
        return model.into_inner().unwrap_or_else(|e| e.into_inner());
    };
    let idiom = UIDevice::currentDevice(mtm).userInterfaceIdiom();
    if idiom == UIUserInterfaceIdiom::Pad { "iPad" } else { "iPhone" }.to_owned()
}

/// Locks `controller`'s vault when the device locks (with a passcode set), and covers the app in the background while
/// the vault is open ([`set_private`]).
pub fn start(controller: Arc<Controller>) {
    if CONTROLLER.set(controller).is_err() {
        return;
    }
    dispatch2::DispatchQueue::main().exec_async(|| {
        let center = NSNotificationCenter::defaultCenter();
        let observe = |name: &NSString, then: fn()| {
            let block = block2::RcBlock::new(move |_: std::ptr::NonNull<NSNotification>| then());
            let observer = unsafe {
                center.addObserverForName_object_queue_usingBlock(Some(name), None, Some(&NSOperationQueue::mainQueue()), &block)
            };
            OBSERVERS.with_borrow_mut(|o| o.push(observer));
        };
        unsafe {
            observe(UIApplicationProtectedDataWillBecomeUnavailable, || {
                if let Some(controller) = CONTROLLER.get() {
                    controller.request_lock();
                }
            });
            observe(UIApplicationDidEnterBackgroundNotification, || {
                if PRIVATE.load(Ordering::SeqCst) {
                    cover(true);
                }
            });
            observe(UIApplicationWillEnterForegroundNotification, || cover(false));
        }
    });
}

/// Whether the vault is open, so the app is covered when it goes to the background.
pub fn set_private(on: bool) {
    PRIVATE.store(on, Ordering::SeqCst);
}

/// Shows or removes a blurred window over every other, the system's alerts included.
fn cover(on: bool) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    if !on {
        COVER.with_borrow_mut(|cover| {
            if let Some(window) = cover.take() {
                window.setHidden(true);
            }
        });
        return;
    }
    if COVER.with_borrow(|cover| cover.is_some()) {
        return;
    }
    let application = UIApplication::sharedApplication(mtm);
    let scene = application.connectedScenes().iter().find_map(|scene| {
        let scene = scene.downcast::<UIWindowScene>().ok()?;
        (scene.activationState() != UISceneActivationState::Unattached).then_some(scene)
    });
    let Some(scene) = scene else { return };
    let window = UIWindow::initWithWindowScene(mtm.alloc(), &scene);
    window.setWindowLevel(unsafe { UIWindowLevelAlert } + 1.0);
    let blur =
        UIVisualEffectView::initWithEffect(mtm.alloc(), Some(&UIBlurEffect::effectWithStyle(UIBlurEffectStyle::SystemMaterial, mtm)));
    let view: &UIView = &blur;
    view.setFrame(window.bounds());
    view.setAutoresizingMask(UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight);
    window.addSubview(view);
    window.setHidden(false);
    COVER.with_borrow_mut(|cover| *cover = Some(window));
}

/// Puts `text` on the general pasteboard for this device only (never Universal Clipboard), taken off by the system
/// after `ttl`. On the main thread.
pub fn copy_sensitive(text: &str, ttl: std::time::Duration) -> bool {
    let Some(mtm) = MainThreadMarker::new() else { return false };
    let _ = mtm;
    let pasteboard = UIPasteboard::generalPasteboard();
    let item = NSDictionary::<NSString, objc2::runtime::AnyObject>::from_slices(
        &[&*NSString::from_str("public.utf8-plain-text")],
        &[&**NSString::from_str(text)],
    );
    let items = objc2_foundation::NSArray::from_retained_slice(&[item]);
    let expires = NSDate::dateWithTimeIntervalSinceNow(ttl.as_secs_f64());
    let options = unsafe {
        NSDictionary::<NSString, objc2::runtime::AnyObject>::from_slices(
            &[UIPasteboardOptionLocalOnly, UIPasteboardOptionExpirationDate],
            &[&**objc2_foundation::NSNumber::new_bool(true), &**expires],
        )
    };
    unsafe { pasteboard.setItems_options(&items, &options) };
    true
}

/// Makes the action titled `action` the preferred one of the system alert titled `title` once it shows: drawn prominent
/// and chosen by Return, as the system marks a safe default. NSH marks a preferred action only in alerts with a text
/// field. Looks for the alert for about a second.
pub fn prefer_alert_action(title: String, action: String) {
    fn attempt(title: String, action: String, left: u32) {
        let found = MainThreadMarker::new().is_some_and(|mtm| {
            let application = UIApplication::sharedApplication(mtm);
            let windows = application
                .connectedScenes()
                .iter()
                .filter_map(|scene| scene.downcast::<UIWindowScene>().ok())
                .flat_map(|scene| scene.windows().to_vec())
                .collect::<Vec<_>>();
            windows.iter().any(|window| {
                let mut controller = window.rootViewController();
                while let Some(c) = controller {
                    if let Ok(alert) = c.clone().downcast::<objc2_ui_kit::UIAlertController>()
                        && alert.title().is_some_and(|t| t.to_string() == title)
                    {
                        if let Some(chosen) = alert.actions().iter().find(|a| a.title().is_some_and(|t| t.to_string() == action)) {
                            alert.setPreferredAction(Some(&chosen));
                            // Shown already: its buttons take the preferred look only once redrawn, their tint set anew.
                            if let Some(view) = alert.view() {
                                let tint = view.tintColor();
                                // SAFETY: a color, or none, on the alert's own view, on the main thread.
                                unsafe {
                                    view.setTintColor(None);
                                    view.setTintColor(tint.as_deref());
                                }
                                view.setNeedsLayout();
                                view.layoutIfNeeded();
                            }
                        }
                        return true;
                    }
                    controller = c.presentedViewController();
                }
                false
            })
        });
        if !found && left > 0 {
            dispatch2::DispatchQueue::main()
                .after(dispatch2::DispatchTime::NOW.time(50_000_000), move || attempt(title, action, left - 1))
                .ok();
        }
    }
    dispatch2::DispatchQueue::main().exec_async(move || attempt(title, action, 20));
}

static ICLOUD_CHANGED: AtomicBool = AtomicBool::new(false);
/// Another device changed the iCloud backup (a silent push of this app's subscription) since the last call.
pub fn take_icloud_change() -> bool {
    ICLOUD_CHANGED.swap(false, Ordering::SeqCst)
}
/// Hears the application delegate's remote notifications through NSH (`on_application_event`), from the native
/// module's entry, before the app runs: only this app's backup subscription counts (`platform::icloud::is_backup_change`);
/// it raises a flag the store polls, and nothing else is read. Once, however often the entry runs.
pub fn observe_remote_notifications() {
    static OBSERVED: std::sync::Once = std::sync::Once::new();
    OBSERVED.call_once(|| {
        nsh_platform_ios::on_application_event(|event| {
            if let nsh_platform_ios::ApplicationEvent::RemoteNotification { user_info, .. } = event {
                // SAFETY: the dictionary UIKit delivered, valid while the event is delivered.
                let info = unsafe { user_info.as_ptr().cast::<NSDictionary>().as_ref() };
                if super::icloud::is_backup_change(info) {
                    ICLOUD_CHANGED.store(true, Ordering::SeqCst);
                }
            }
        });
    });
}
/// Registers for remote notifications: iCloud backup changes arrive as silent pushes. Without the push entitlement
/// (`aps-environment`, from the provisioning profile) registration fails quietly and backups sync on unlock, after
/// saves and by hand. Any thread.
pub fn enable_remote_push() {
    dispatch2::DispatchQueue::main().exec_async(|| {
        let Some(mtm) = MainThreadMarker::new() else { return };
        UIApplication::sharedApplication(mtm).registerForRemoteNotifications();
    });
}
