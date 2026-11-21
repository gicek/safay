//! A sign-in whose callback scheme NSH's web sign-in session refuses (longer than its `MAX_SCHEME_LEN`, as Google's
//! reversed client IDs are): the page opens in a Custom Tab (`Bridge.openCustomTab`), and the callback comes back as a
//! link of a scheme the app claims, which NSH hands over through `next_url_scheme_open`. Coming back to Safay without
//! a callback ends the sign-in as closed, as NSH's session does.
use super::Signed;
use nsh_core::{platform::Lifecycle, prelude::*};
use std::{
    cell::Cell,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::Sender,
    },
    time::Duration,
};

/// The latest sign-in: watchers of an older one's return leave a newer one alone.
static TURN: AtomicU64 = AtomicU64::new(0);
/// The sign-in waiting for its callback: its scheme (lower case) and where its end goes.
static PENDING: Mutex<Option<(String, Sender<Signed>)>> = Mutex::new(None);
thread_local! {
    /// The link listener runs once per process: NSH takes one waiting call at a time.
    static LISTENING: Cell<bool> = const { Cell::new(false) };
}

/// The longest callback scheme NSH's session takes (`nsh_core::auth_session::MAX_SCHEME_LEN`).
pub fn needed(callback_scheme: &str) -> bool {
    callback_scheme.len() > nsh_core::auth_session::MAX_SCHEME_LEN
}

fn end(signed: Signed) {
    if let Some((_, sender)) = PENDING.lock().ok().and_then(|mut pending| pending.take()) {
        let _ = sender.send(signed);
    }
}

/// Ends the sign-in in progress, if any (the user left its page).
pub fn cancel() {
    end(Signed::Closed);
}

/// Opens `url` and waits for a link of `callback_scheme`. On the engine thread, under an owner that lasts the sign-in.
pub fn start(url: &str, callback_scheme: &str, sender: Sender<Signed>) {
    // A newer sign-in ends an older one.
    cancel();
    TURN.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut pending) = PENDING.lock() {
        *pending = Some((callback_scheme.to_ascii_lowercase(), sender));
    }
    if !LISTENING.with(|l| l.replace(true)) {
        // Back in Safay without a callback: the user closed the page. The callback's link arrives as the app comes to
        // the front, so it is given a moment first; a sign-in started meanwhile is left alone.
        let away = signal(false);
        effect(move || {
            let state = lifecycle();
            if state == Lifecycle::Background {
                away.set(true);
            } else if state == Lifecycle::Active && untracked(|| away.get()) {
                away.set(false);
                let turn = TURN.load(Ordering::SeqCst);
                set_timeout(Duration::from_millis(1500), move || {
                    if lifecycle() == Lifecycle::Active && TURN.load(Ordering::SeqCst) == turn {
                        end(Signed::Closed);
                    }
                });
            }
        });
        spawn(async {
            loop {
                let link = next_url_scheme_open().await;
                let matches = PENDING.lock().ok().is_some_and(|pending| {
                    pending.as_ref().is_some_and(|(scheme, _)| link.split_once(':').is_some_and(|(s, _)| s.eq_ignore_ascii_case(scheme)))
                });
                if matches {
                    end(Signed::Callback(link));
                }
            }
        });
    }
    if !super::android::open_custom_tab(url) {
        end(Signed::Failed);
    }
}
