//! Signing in to Dropbox through its official Android app, where it is installed (`cloudset`'s app-to-app protocol, as
//! the official SDK's `AuthActivity`): `DropboxAuth.start` (android/java) checks that `com.dropbox.android` is
//! Dropbox's, signed with its production certificate, and sends it the request; the app answers by starting
//! `DropboxAuthActivity` with the result in its extras, which comes here through `DropboxAuth.answer`. An answer of
//! another sign-in (another state) is left out. Back in Safay without an answer (the user left the app) ends the sign-in
//! as closed, as the browser's does.
use nsh_core::{platform::Lifecycle, prelude::*};
use safay_app::{Zeroizing, cloud::DropboxAppAnswer, cloudset::Authorization};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, Sender},
    },
    time::Duration,
};

/// The latest sign-in: the watcher of an older one's return leaves a newer one alone.
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

/// Asks the Dropbox app; `None` where it is not installed, not Dropbox's, or cannot be started (the browser signs in
/// instead). On the engine thread, under an owner that lasts the sign-in.
pub fn start(authorization: &Arc<Authorization>) -> Option<Receiver<Option<DropboxAppAnswer>>> {
    let request = authorization.dropbox_app_request()?;
    cancel();
    let turn = TURN.fetch_add(1, Ordering::SeqCst) + 1;
    let (sender, receiver) = std::sync::mpsc::channel();
    // Waiting before the app opens: its answer may come before this thread runs again.
    if let Ok(mut pending) = PENDING.lock() {
        *pending = Some((authorization.clone(), sender));
    }
    let started = super::android::with_env(|env| {
        let class = super::android::class(env, "com.gicek.safay.DropboxAuth")?;
        let key = env.new_string(&request.app_key)?;
        let state = env.new_string(&request.auth_state)?;
        let params = env.new_string(&request.query_params)?;
        env.call_static_method(
            class,
            "start",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
            &[(&key).into(), (&state).into(), (&params).into()],
        )?
        .z()
    })
    .unwrap_or(false);
    if !started {
        if let Ok(mut pending) = PENDING.lock() {
            pending.take();
        }
        return None;
    }
    // Back in Safay without an answer: the user left the app. The answer's activity comes first, so a moment is given.
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
    Some(receiver)
}

/// `DropboxAuth.answer`: what the app gave `DropboxAuthActivity`. Only the waiting sign-in's answer (its state) ends it.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_gicek_safay_DropboxAuth_answer(
    mut env: jni::JNIEnv<'_>,
    _class: jni::objects::JClass<'_>,
    token_type: jni::objects::JString<'_>,
    code: jni::objects::JString<'_>,
    state: jni::objects::JString<'_>,
) {
    let mut text = |s: &jni::objects::JString<'_>| -> String {
        if s.is_null() { String::new() } else { env.get_string(s).map(String::from).unwrap_or_default() }
    };
    let (token_type, code, state) = (text(&token_type), Zeroizing::new(text(&code)), text(&state));
    let ours = PENDING.lock().ok().is_some_and(|pending| pending.as_ref().is_some_and(|(a, _)| a.accepts_dropbox_app(&state)));
    if ours {
        end(Some(DropboxAppAnswer::Extras { token_type, code, state }));
    }
}
