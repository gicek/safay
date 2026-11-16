//! A process-local session and lock generation shared by every window's store.
use crate::{
    session::{Clock, Error, Session},
    storage::Repository,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

pub struct Controller {
    session: Mutex<Session>,
    /// Read by the launcher commands without the session's lock (crate::launcher).
    launcher: Arc<crate::launcher::Launcher>,
    gate: Arc<AtomicU64>,
    waiters: Mutex<Vec<std::task::Waker>>,
    boundary: Mutex<()>,
    /// Bumped after a user operation, so every window's store re-projects at once (unlock in the tray panel
    /// opens the main window too). Periodic refreshes never bump it, so stores cannot ping-pong.
    revision: AtomicU64,
    revision_waiters: Mutex<Vec<std::task::Waker>>,
}
impl Controller {
    pub fn new(repo: Box<dyn Repository>, clock: Arc<dyn Clock>) -> Result<Arc<Self>, Error> {
        let gate = Arc::new(AtomicU64::new(0));
        let session = Session::new(repo, clock, gate.clone())?;
        Ok(Arc::new(Self {
            launcher: session.launcher(),
            session: Mutex::new(session),
            gate,
            waiters: Mutex::new(Vec::new()),
            boundary: Mutex::new(()),
            revision: AtomicU64::new(0),
            revision_waiters: Mutex::new(Vec::new()),
        }))
    }
    /// What launchers may search, readable while a slow operation holds the session.
    pub fn launcher(&self) -> &crate::launcher::Launcher {
        &self.launcher
    }
    pub fn epoch(&self) -> u64 {
        self.gate.load(Ordering::SeqCst)
    }
    /// Nonblocking: a slow KDF can never delay a window hiding its sensitive content.
    pub fn request_lock(&self) {
        let _boundary = self.boundary.lock().unwrap_or_else(|e| e.into_inner());
        self.gate.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut waiters) = self.waiters.lock() {
            for waiter in waiters.drain(..) {
                waiter.wake();
            }
        }
        if let Ok(mut session) = self.session.try_lock() {
            session.lock();
        }
    }
    /// Order irreversible clipboard commits against locking, without holding the vault mutex.
    pub fn at_epoch<T>(&self, epoch: u64, operation: impl FnOnce() -> Result<T, Error>) -> Result<T, Error> {
        let _boundary = self.boundary.lock().map_err(|_| Error::Cancelled)?;
        if self.epoch() != epoch {
            return Err(Error::Cancelled);
        }
        operation()
    }
    /// Wakes an engine-owned task without sharing NSH signals across threads.
    pub async fn changed(&self, seen: u64) -> u64 {
        std::future::poll_fn(|cx| {
            let Ok(mut waiters) = self.waiters.lock() else { return std::task::Poll::Ready(self.epoch()) };
            let current = self.epoch();
            if current != seen {
                return std::task::Poll::Ready(current);
            }
            if !waiters.iter().any(|w| w.will_wake(cx.waker())) {
                waiters.push(cx.waker().clone());
            }
            std::task::Poll::Pending
        })
        .await
    }
    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::SeqCst)
    }
    pub fn touch(&self) {
        self.revision.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut waiters) = self.revision_waiters.lock() {
            for waiter in waiters.drain(..) {
                waiter.wake();
            }
        }
    }
    pub async fn revised(&self, seen: u64) -> u64 {
        std::future::poll_fn(|cx| {
            let Ok(mut waiters) = self.revision_waiters.lock() else { return std::task::Poll::Ready(self.revision()) };
            let current = self.revision();
            if current != seen {
                return std::task::Poll::Ready(current);
            }
            if !waiters.iter().any(|w| w.will_wake(cx.waker())) {
                waiters.push(cx.waker().clone());
            }
            std::task::Poll::Pending
        })
        .await
    }
    /// Called only for input received by this application, never global idle time.
    pub fn activity(&self) {
        if let Ok(mut session) = self.session.try_lock() {
            session.activity();
        }
    }
    pub fn run<T>(&self, epoch: u64, operation: impl FnOnce(&mut Session) -> Result<T, Error>) -> Result<T, Error> {
        let mut session = self.session.lock().map_err(|_| Error::Storage)?;
        if self.epoch() != epoch {
            session.tick();
            return Err(Error::Cancelled);
        }
        let result = operation(&mut session);
        if self.epoch() != epoch {
            session.lock();
            return Err(Error::Cancelled);
        }
        result
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        if let Ok(session) = self.session.get_mut() {
            session.lock();
        }
    }
}
