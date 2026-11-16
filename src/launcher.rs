//! A launcher's `copy code` (Alfred, Raycast, Tinycast; `platform::automation`): every copy is proven on the spot,
//! whether or not Safay is unlocked (docs/ARCHITECTURE.md, Launcher integration).
//!
//! With Touch ID set up, the system's prompt names the app that asked and the account; the key it releases unlocks a
//! locked vault or must match an unlocked one. Without Touch ID, or when the user chooses the password, the main
//! window takes over (`Store::launcher_copy_request`): the master password dialog, or the lock screen. The code goes to
//! the sensitive clipboard and a desktop notice says which account's it is; the code itself never goes back to the
//! caller, nor into the notice.
use crate::{
    desktop, i18n,
    platform::{self, biometric::Failure, clipboard},
    store::Tone,
};
use safay_app::{Uuid, controller::Controller};
use std::sync::Arc;

/// `Some(status)` when it is decided at once; `None` while the user is asked, the status then going to
/// `platform::automation::finish` on the main thread.
pub fn copy(controller: Arc<Controller>, id: Uuid, app: Option<String>) -> Option<&'static str> {
    let launcher = controller.launcher();
    if !launcher.enabled() {
        return Some("disabled");
    }
    if !launcher.ready() {
        return Some("not_ready");
    }
    let Some(issuer) = launcher.issuer(id) else { return Some("not_found") };
    let language = desktop::language();
    let app = app.unwrap_or_else(|| i18n::tr(language, "another app").to_string());
    let reason = i18n::tr(language, "copy the code for {} (asked by {})").replacen("{}", &issuer, 1).replacen("{}", &app, 1);
    let cancel = i18n::tr(language, "Cancel").to_string();
    std::thread::spawn(move || {
        let status = prove_and_copy(&controller, id, &reason, &cancel);
        dispatch2::DispatchQueue::main().exec_async(move || match status {
            Ok(issuer) => {
                let text = i18n::tr(language, "Copied the code for {}").replacen("{}", &issuer, 1);
                desktop::notice(Tone::Success, text);
                platform::automation::finish("copied");
            }
            Err(Outcome::Cancelled) => platform::automation::finish("cancelled"),
            Err(Outcome::Failed) => platform::automation::finish("failed"),
            Err(Outcome::Password) => {
                platform::request_launcher_copy(id);
                platform::open_main();
                platform::automation::finish("pending");
            }
        });
    });
    None
}

enum Outcome {
    Cancelled,
    Failed,
    /// Touch ID is not set up or did not prove it: the master password, in the main window.
    Password,
}

/// Off the main thread: Touch ID blocks while its prompt is up.
fn prove_and_copy(controller: &Arc<Controller>, id: Uuid, reason: &str, cancel: &str) -> Result<String, Outcome> {
    let epoch = controller.epoch();
    let file = controller.run(epoch, |s| Ok(if s.preferences().touch_id { s.biometric()? } else { None })).ok().flatten();
    let Some(file) = file.filter(|_| platform::biometric::available()) else { return Err(Outcome::Password) };
    let key = match platform::biometric::open(&file, reason, cancel) {
        Ok(key) => key,
        Err(Failure::Cancelled) => return Err(Outcome::Cancelled),
        // "Use Password…", lockout, a stale key: the master password decides.
        Err(_) => return Err(Outcome::Password),
    };
    let epoch = controller.epoch();
    let (code, ttl, issuer) = controller.run(epoch, |s| s.launcher_code_with_key(id, &key)).map_err(|_| Outcome::Password)?;
    // A Touch ID unlock shows in every window at once.
    controller.touch();
    clipboard::copy(code, ttl, controller.clone(), controller.epoch()).map_err(|_| Outcome::Failed)?;
    Ok(issuer)
}
