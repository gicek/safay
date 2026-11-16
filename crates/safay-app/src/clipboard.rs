//! Clipboard boundary injected independently of NSH's ordinary clipboard service.
use crate::{
    controller::Controller,
    session::{Clock, Error},
};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use zeroize::Zeroizing;
pub trait SecretClipboard: Send + Sync {
    fn copy(&self, text: Zeroizing<String>, ttl: Duration, controller: Arc<Controller>, epoch: u64) -> Result<(), Error>;
}
pub struct MemoryClipboard {
    clock: Arc<dyn Clock>,
    value: Mutex<Option<(Zeroizing<String>, Duration)>>,
}
impl MemoryClipboard {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self { clock, value: Mutex::new(None) }
    }
    pub fn text(&self) -> Option<Zeroizing<String>> {
        let mut value = self.value.lock().ok()?;
        if value.as_ref().is_some_and(|(_, expiry)| *expiry <= self.clock.elapsed()) {
            *value = None;
        }
        value.as_ref().map(|(text, _)| text.clone())
    }
}
impl SecretClipboard for MemoryClipboard {
    fn copy(&self, text: Zeroizing<String>, ttl: Duration, controller: Arc<Controller>, epoch: u64) -> Result<(), Error> {
        controller.at_epoch(epoch, || {
            let mut value = self.value.lock().map_err(|_| Error::Storage)?;
            *value = Some((text, self.clock.elapsed() + ttl));
            Ok(())
        })
    }
}
