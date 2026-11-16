//! The AppleScript commands launchers call (`resources/Safay.sdef`): Cocoa Scripting, which NSH does not provide
//! (docs/ARCHITECTURE.md). macOS Automation (TCC) decides which apps may send them; Safay decides what they get
//! (SECURITY.md, Launcher integration):
//!
//! - `find accounts <query>`: the masked matches as JSON, never a secret or a code;
//! - `copy code <id>`: a status only. The code goes to the clipboard after the user proves it is them now
//!   (`crate::launcher::copy`), never back to the caller.
//!
//! Commands run on the main thread. `copy code` suspends its Apple event while Touch ID is up and resumes it with the
//! status; one copy at a time.
use nsh_platform_macos::objc2::{
    ClassType, Message, define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, NSObject},
};
use objc2_app_kit::NSRunningApplication;
use objc2_foundation::{NSAppleEventDescriptor, NSScriptCommand, NSString};
use safay_app::{Uuid, controller::Controller};
use std::{
    cell::RefCell,
    sync::{Arc, OnceLock},
};

static CONTROLLER: OnceLock<Arc<Controller>> = OnceLock::new();
thread_local! {
    /// The `copy code` event waiting for the user (main thread).
    static WAITING: RefCell<Option<Retained<NSScriptCommand>>> = const { RefCell::new(None) };
}

/// Registers the command classes before an Apple event can name them, with the session they read.
pub fn install(controller: Arc<Controller>) {
    let _ = CONTROLLER.set(controller);
    let _ = FindAccountsCommand::class();
    let _ = CopyCodeCommand::class();
}

fn text(s: &str) -> Retained<AnyObject> {
    Retained::into_super(Retained::into_super(NSString::from_str(s)))
}

fn parameter(command: &NSScriptCommand) -> String {
    command.directParameter().and_then(|p| p.downcast::<NSString>().ok()).map(|s| s.to_string()).unwrap_or_default()
}

/// The app that asked: the event comes from `osascript`, so this walks up its parents to the first process that is an
/// app with a bundle ID (Alfred, Raycast, Tinycast, a terminal). Shown to the user only; never a reason to allow.
fn asking_app(command: &NSScriptCommand) -> Option<String> {
    let event = command.appleEvent()?;
    // keySenderPIDAttr
    let pid: Option<Retained<NSAppleEventDescriptor>> =
        unsafe { msg_send![&*event, attributeDescriptorForKeyword: u32::from_be_bytes(*b"spid")] };
    let mut pid = pid?.int32Value();
    for _ in 0..16 {
        let app: Option<Retained<NSRunningApplication>> =
            unsafe { msg_send![NSRunningApplication::class(), runningApplicationWithProcessIdentifier: pid] };
        if let Some(app) = app
            && app.bundleIdentifier().is_some()
            && let Some(name) = app.localizedName()
        {
            return Some(name.to_string());
        }
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
        let read = unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, (&raw mut info).cast(), size) };
        if read != size || info.pbi_ppid <= 1 {
            return None;
        }
        pid = info.pbi_ppid as i32;
    }
    None
}

fn copy_code(command: &NSScriptCommand) -> Option<Retained<AnyObject>> {
    let Some(controller) = CONTROLLER.get().cloned().filter(|c| c.launcher().enabled()) else { return Some(text("disabled")) };
    if WAITING.with(|w| w.borrow().is_some()) {
        return Some(text("busy"));
    }
    let Ok(id) = Uuid::parse_str(parameter(command).trim()) else { return Some(text("not_found")) };
    let app = asking_app(command);
    match crate::launcher::copy(controller, id, app) {
        Some(status) => Some(text(status)),
        None => {
            command.suspendExecution();
            WAITING.with(|w| *w.borrow_mut() = Some(command.retain()));
            None
        }
    }
}

/// Ends the waiting `copy code` with its status (main thread).
pub fn finish(status: &'static str) {
    if let Some(command) = WAITING.with(|w| w.borrow_mut().take()) {
        unsafe { command.resumeExecutionWithResult(Some(&text(status))) };
    }
}

define_class!(
    #[unsafe(super(NSScriptCommand, NSObject))]
    #[name = "SafayFindAccountsCommand"]
    struct FindAccountsCommand;

    impl FindAccountsCommand {
        #[unsafe(method_id(performDefaultImplementation))]
        fn perform(&self) -> Option<Retained<AnyObject>> {
            let mut found = match CONTROLLER.get() {
                Some(c) => c.launcher().find(&parameter(self)),
                None => safay_app::launcher::Found { enabled: false, ready: false, accounts: vec![], language: None },
            };
            found.language = Some(crate::i18n::code(crate::desktop::language()));
            Some(text(&found.to_json()))
        }
    }
);

define_class!(
    #[unsafe(super(NSScriptCommand, NSObject))]
    #[name = "SafayCopyCodeCommand"]
    struct CopyCodeCommand;

    impl CopyCodeCommand {
        #[unsafe(method_id(performDefaultImplementation))]
        fn perform(&self) -> Option<Retained<AnyObject>> {
            copy_code(self)
        }
    }
);
