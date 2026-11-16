//! The app's alerts. In the app itself on macOS they are AppKit alerts the app presents (`platform::macos::alert`):
//! NSH's `AlertDialog` appears there only once AppKit's appear animation would have ended, about 0.3 s after the click.
//! In NSH's runners and tests, and on the other platforms, they are NSH's `AlertDialog`, so tests still see and answer
//! them. Either way the buttons follow NSH's rules: a cancel action closes it with Escape, and an alert of at most two
//! buttons has a default one (Return, drawn prominent), its last regular action unless [`Alert::default`] names one.
use nsh_core::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Regular,
    Cancel,
    Destructive,
}
struct Action {
    id: &'static str,
    label: SharedString,
    role: Role,
}
pub struct Alert {
    title: SharedString,
    description: Option<SharedString>,
    actions: Vec<Action>,
    default: Option<&'static str>,
}
pub fn alert(title: impl Into<SharedString>) -> Alert {
    Alert { title: title.into(), description: None, actions: Vec::new(), default: None }
}
impl Alert {
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }
    pub fn action(self, id: &'static str, label: impl Into<SharedString>) -> Self {
        self.with(id, label, Role::Regular)
    }
    /// The action that closes it without doing anything (Escape).
    pub fn cancel(self, id: &'static str, label: impl Into<SharedString>) -> Self {
        self.with(id, label, Role::Cancel)
    }
    pub fn destructive(self, id: &'static str, label: impl Into<SharedString>) -> Self {
        self.with(id, label, Role::Destructive)
    }
    /// The default button (Return, drawn prominent). A cancel action named here takes Return and Escape: the safe
    /// choice of a warning.
    pub fn default(mut self, id: &'static str) -> Self {
        self.default = Some(id);
        self
    }
    fn with(mut self, id: &'static str, label: impl Into<SharedString>, role: Role) -> Self {
        self.actions.push(Action { id, label: label.into(), role });
        self
    }
    /// NSH's default button: the last regular action of an alert with at most two buttons.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    fn default_id(&self) -> Option<&'static str> {
        self.default.or_else(|| {
            (self.actions.len() <= 2).then(|| self.actions.iter().rev().find(|a| a.role == Role::Regular).map(|a| a.id)).flatten()
        })
    }
    /// Shows it; the chosen action's id, or `None` when it closed without one.
    pub async fn show(self) -> Option<&'static str> {
        #[cfg(target_os = "macos")]
        if crate::hosted() {
            use crate::platform::macos::{AlertButton, AlertKey};
            let default = self.default_id();
            // AppKit's order: the default first (on top, or rightmost), the cancel action last.
            let mut order: Vec<&Action> = self.actions.iter().filter(|a| Some(a.id) == default).collect();
            order.extend(self.actions.iter().filter(|a| Some(a.id) != default && a.role != Role::Cancel));
            order.extend(self.actions.iter().filter(|a| Some(a.id) != default && a.role == Role::Cancel));
            let ids: Vec<&'static str> = order.iter().map(|a| a.id).collect();
            let buttons = order
                .iter()
                .map(|a| AlertButton {
                    label: a.label.to_string(),
                    key: match (Some(a.id) == default, a.role == Role::Cancel) {
                        (true, true) => AlertKey::ReturnAndEscape,
                        (true, false) => AlertKey::Return,
                        (false, true) => AlertKey::Escape,
                        (false, false) => AlertKey::None,
                    },
                    destructive: a.role == Role::Destructive,
                })
                .collect();
            let chosen = crate::platform::macos::alert(self.title.to_string(), self.description.as_ref().map(|d| d.to_string()), buttons);
            return match blocking(move || chosen.recv()).await {
                Ok(Ok(Ok(index))) => index.map(|i| ids[i]),
                // No window to attach it to: NSH's alert, which finds one of its own.
                _ => self.show_nsh().await,
            };
        }
        self.show_nsh().await
    }
    /// NSH's `AlertDialog`; its default button is its last regular action, so a named default goes last as one.
    async fn show_nsh(self) -> Option<&'static str> {
        let mut dialog = nsh_base::alert_dialog().title(self.title.clone());
        if let Some(description) = &self.description {
            dialog = dialog.description(description.clone());
        }
        let ids: Vec<&'static str> = self
            .actions
            .iter()
            .filter(|a| Some(a.id) != self.default)
            .chain(self.actions.iter().filter(|a| Some(a.id) == self.default))
            .map(|a| a.id)
            .collect();
        for id in &ids {
            let a = self.actions.iter().find(|a| a.id == *id).expect("one of its actions");
            dialog = match (a.role, Some(a.id) == self.default) {
                (_, true) | (Role::Regular, false) => dialog.action(a.id, a.label.clone()),
                (Role::Cancel, false) => dialog.cancel(a.id, a.label.clone()),
                (Role::Destructive, false) => dialog.destructive(a.id, a.label.clone()),
            };
        }
        let answer = dialog.show().await.ok()?.action?;
        ids.into_iter().find(|id| *id == answer.as_ref())
    }
}
