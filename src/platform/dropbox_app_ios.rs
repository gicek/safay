//! Signing in to Dropbox through its official iOS app, where it is installed (`cloudset`'s app-to-app protocol, as the
//! official SDK's `DropboxMobileOAuthManager`): the app opens with a `dbapi-2://1/connect` link (`dbapi-8-emm` for its
//! enterprise build; both in `LSApplicationQueriesSchemes`, nsh.json) and comes back with a link of Safay's Dropbox
//! scheme (`db-<app key>`, claimed by the device package: scripts/ios.py), which NSH hands over through
//! `next_url_scheme_open`. A link of another sign-in is left out. Back in Safay without one (the user left the app) ends
//! the sign-in as closed, as the browser's does.
use nsh_core::{platform::Lifecycle, prelude::*};
use objc2::{MainThreadMarker, msg_send, runtime::Bool};
use objc2_foundation::{NSString, NSURL};
use objc2_ui_kit::UIApplication;
use safay_app::{cloud::DropboxAppAnswer, cloudset::Authorization};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, Sender},
    },
    time::Duration,
};

/// The schemes of the Dropbox app's builds, the App Store's first.
const SCHEMES: [&str; 2] = ["dbapi-2", "dbapi-8-emm"];

static TURN: AtomicU64 = AtomicU64::new(0);
/// A sign-in waiting for the app: whose answer it takes, and where its end goes (`None`: closed).
type Waiting = (Arc<Authorization>, Sender<Option<DropboxAppAnswer>>);
static PENDING: Mutex<Option<Waiting>> = Mutex::new(None);

fn end(answer: Option<DropboxAppAnswer>) {
    if let Some((_, sender)) = PENDING.lock().ok().and_then(|mut pending| pending.take()) {
        let _ = sender.send(answer);
    }
}

/// Ends the sign-in in progress, if any.
pub fn cancel() {
    end(None);
}

/// The scheme of the Dropbox build installed here, if any. On the main thread.
fn installed() -> Option<&'static str> {
    let Some(mtm) = MainThreadMarker::new() else {
        let found = Mutex::new(None);
        dispatch2::DispatchQueue::main().exec_sync(|| *found.lock().unwrap_or_else(|e| e.into_inner()) = installed());
        return found.into_inner().unwrap_or_else(|e| e.into_inner());
    };
    let application = UIApplication::sharedApplication(mtm);
    SCHEMES.into_iter().find(|scheme| {
        let Some(url) = NSURL::URLWithString(&NSString::from_str(&format!("{scheme}://1/connect"))) else { return false };
        let can: Bool = unsafe { msg_send![&*application, canOpenURL: &*url] };
        can.as_bool()
    })
}

/// Asks the Dropbox app; `None` where it is not installed (the browser signs in instead). On the engine thread, under an
/// owner that lasts the sign-in.
pub fn start(authorization: &Arc<Authorization>) -> Option<Receiver<Option<DropboxAppAnswer>>> {
    let scheme = installed()?;
    let url = authorization.dropbox_app_url(scheme)?;
    cancel();
    let turn = TURN.fetch_add(1, Ordering::SeqCst) + 1;
    let (sender, receiver) = std::sync::mpsc::channel();
    if let Ok(mut pending) = PENDING.lock() {
        *pending = Some((authorization.clone(), sender));
    }
    // The app's link back; another sign-in's is left out (and waited past).
    spawn(async move {
        while TURN.load(Ordering::SeqCst) == turn {
            let link = next_url_scheme_open().await;
            let ours =
                PENDING.lock().ok().is_some_and(|pending| pending.as_ref().is_some_and(|(a, _)| a.accepts_dropbox_app_callback(&link)));
            if ours && TURN.load(Ordering::SeqCst) == turn {
                end(Some(DropboxAppAnswer::Callback(link)));
                break;
            }
        }
    });
    // Back in Safay without the link: the user left the app. The link comes as the app comes to the front, so it is given
    // a moment first.
    let away = signal(false);
    effect(move || {
        let state = lifecycle();
        if state == Lifecycle::Background {
            away.set(true);
        } else if state == Lifecycle::Active && untracked(|| away.get()) {
            away.set(false);
            set_timeout(Duration::from_millis(1500), move || {
                if lifecycle() == Lifecycle::Active && TURN.load(Ordering::SeqCst) == turn {
                    end(None);
                }
            });
        }
    });
    if NSURL::URLWithString(&NSString::from_str(&url)).is_none() {
        if let Ok(mut pending) = PENDING.lock() {
            pending.take();
        }
        TURN.fetch_add(1, Ordering::SeqCst);
        return None;
    }
    // Through NSH (`UIApplication`'s `openURL`); refused after all (the app went meanwhile): back to the form.
    let opened = open_url(url);
    spawn(async move {
        if !opened.await && TURN.load(Ordering::SeqCst) == turn {
            end(None);
        }
    });
    Some(receiver)
}
