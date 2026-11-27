//! Sign-in for the OAuth backup services (Dropbox, OneDrive, Google Drive) on macOS, and on iOS where NSH's session
//! refuses the callback scheme (Google's, longer than its `MAX_SCHEME_LEN`): an `ASWebAuthenticationSession` opens
//! the provider's page and hands back the callback URL for its registered scheme itself, so no URL scheme is
//! registered with the system. NSH's web sign-in session is the same on iOS, short schemes only.
use objc2::{AllocAnyThread, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained, runtime::ProtocolObject};
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSApplication, NSWindow};
use objc2_authentication_services::{ASWebAuthenticationPresentationContextProviding, ASWebAuthenticationSession};
use objc2_foundation::{NSError, NSObject, NSObjectProtocol, NSString, NSURL};
#[cfg(target_os = "ios")]
use objc2_ui_kit::{UIApplication, UISceneActivationState, UIWindow, UIWindowScene};
use std::{cell::RefCell, sync::mpsc};

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SafayWebAuthAnchor"]
    struct Anchor;
    unsafe impl NSObjectProtocol for Anchor {}
    unsafe impl ASWebAuthenticationPresentationContextProviding for Anchor {
        #[cfg(target_os = "ios")]
        #[unsafe(method_id(presentationAnchorForWebAuthenticationSession:))]
        fn anchor(&self, _session: &ASWebAuthenticationSession) -> Retained<UIWindow> {
            // The key window of the scene in front.
            let app = UIApplication::sharedApplication(self.mtm());
            let scenes: Vec<_> = app.connectedScenes().iter().filter_map(|s| s.downcast::<UIWindowScene>().ok()).collect();
            let active = scenes.iter().find(|s| s.activationState() == UISceneActivationState::ForegroundActive).or(scenes.first());
            active
                .and_then(|s| s.windows().iter().find(|w| w.isKeyWindow()).or_else(|| s.windows().firstObject()))
                .expect("a window to present sign-in from")
        }
        #[cfg(target_os = "macos")]
        #[unsafe(method_id(presentationAnchorForWebAuthenticationSession:))]
        fn anchor(&self, _session: &ASWebAuthenticationSession) -> Retained<NSWindow> {
            let app = NSApplication::sharedApplication(self.mtm());
            // A window on screen: the list's first is NSH's app host, which never shows.
            app.keyWindow()
                .or_else(|| app.mainWindow())
                .or_else(|| app.windows().iter().find(|w| w.isVisible()))
                .or_else(|| app.windows().firstObject())
                .expect("a window to present sign-in from")
        }
    }
);

type Running = (Retained<ASWebAuthenticationSession>, Retained<Anchor>, block2::RcBlock<dyn Fn(*mut NSURL, *mut NSError)>);
thread_local! {
    static RUNNING: RefCell<Option<Running>> = const { RefCell::new(None) };
}

pub use super::Signed;
/// The outcome of a sign-in; the receiver yields once.
pub type Outcome = mpsc::Receiver<Signed>;

/// `ASWebAuthenticationSessionErrorCodeCanceledLogin`.
const CANCELED_LOGIN: isize = 1;

/// Starts a sign-in from any thread: the session is created and started on the main thread (store actions run on
/// the engine thread, where AppKit, UIKit and AuthenticationServices must not be used).
pub fn start(url: &str, callback_scheme: &str) -> Outcome {
    let (tx, rx) = mpsc::channel();
    let (url, scheme) = (url.to_owned(), callback_scheme.to_owned());
    dispatch2::DispatchQueue::main().exec_async(move || {
        let Some(mtm) = MainThreadMarker::new() else { return drop(tx.send(Signed::Failed)) };
        // A sign-in still running (a previous sheet) is replaced.
        cancel_here();
        let Some(page) = NSURL::URLWithString(&NSString::from_str(&url)) else { return drop(tx.send(Signed::Failed)) };
        let reply = tx.clone();
        let reply = RefCell::new(Some(reply));
        let handler = block2::RcBlock::new(move |callback: *mut NSURL, error: *mut NSError| {
            // SAFETY: AuthenticationServices passes a valid NSURL / NSError or null.
            let value = unsafe { callback.as_ref() }.and_then(|u| u.absoluteString()).map(|s| s.to_string());
            let canceled = unsafe { error.as_ref() }.is_some_and(|e| e.code() == CANCELED_LOGIN);
            let outcome = match value {
                Some(v) => Signed::Callback(v),
                None if canceled => Signed::Closed,
                None => Signed::Failed,
            };
            if let Some(tx) = reply.borrow_mut().take() {
                drop(tx.send(outcome));
            }
            RUNNING.with(|r| r.borrow_mut().take());
        });
        let anchor: Retained<Anchor> = unsafe { msg_send![Anchor::alloc(mtm), init] };
        // SAFETY: the session is configured and started on the main thread; the handler and anchor outlive it in RUNNING.
        // The scheme-based initializer: its replacement (`initWithURL:callback:completionHandler:`) needs macOS 14.4 and
        // iOS 17.4, above the app's minimums.
        #[allow(deprecated)]
        let session = unsafe {
            ASWebAuthenticationSession::initWithURL_callbackURLScheme_completionHandler(
                ASWebAuthenticationSession::alloc(),
                &page,
                Some(&NSString::from_str(&scheme)),
                &*handler as *const _ as *mut _,
            )
        };
        let started = unsafe {
            session.setPresentationContextProvider(Some(ProtocolObject::from_ref(&*anchor)));
            session.start()
        };
        if started {
            RUNNING.with(|r| *r.borrow_mut() = Some((session, anchor, handler)));
        } else {
            drop(tx.send(Signed::Failed));
        }
    });
    rx
}

/// Closes a sign-in in progress (leaving the page), from any thread.
pub fn cancel() {
    dispatch2::DispatchQueue::main().exec_async(cancel_here);
}
fn cancel_here() {
    if let Some((session, _, _)) = RUNNING.with(|r| r.borrow_mut().take()) {
        unsafe { session.cancel() };
    }
}
