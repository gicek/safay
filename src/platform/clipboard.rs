//! Sensitive clipboard IO is deliberately separate from NSH's ordinary clipboard command.
use safay_app::{Zeroizing, session::Error};
use std::time::Duration;

#[cfg(target_os = "macos")]
pub fn copy(
    text: Zeroizing<String>,
    ttl: Duration,
    controller: std::sync::Arc<safay_app::controller::Controller>,
    epoch: u64,
) -> Result<(), Error> {
    use nsh_platform_macos::{
        objc2_app_kit::{NSPasteboard, NSPasteboardTypeString},
        objc2_foundation::{NSArray, NSData, NSString},
    };
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    dispatch2::DispatchQueue::main().exec_async(move || {
        let result = controller.at_epoch(epoch, || {
            let board = NSPasteboard::generalPasteboard();
            let concealed = NSString::from_str("org.nspasteboard.ConcealedType");
            let transient = NSString::from_str("org.nspasteboard.TransientType");
            let string_type = unsafe { NSPasteboardTypeString };
            board.clearContents();
            // No owner callback; AppKit copies the immutable type list and payloads.
            unsafe {
                board.declareTypes_owner(&NSArray::from_slice(&[string_type, &concealed, &transient]), None);
            }
            let ok = board.setData_forType(Some(&NSData::new()), &concealed)
                && board.setData_forType(Some(&NSData::new()), &transient)
                && board.setString_forType(&NSString::from_str(text.as_str()), string_type);
            if !ok {
                board.clearContents();
                return Err(Error::Storage);
            }
            let count = board.changeCount();
            let when = dispatch2::DispatchTime::try_from(ttl).map_err(|_| {
                board.clearContents();
                Error::Storage
            })?;
            dispatch2::DispatchQueue::main()
                .after(when, move || {
                    let board = NSPasteboard::generalPasteboard();
                    if board.changeCount() == count {
                        board.clearContents();
                    }
                })
                .map_err(|_| {
                    board.clearContents();
                    Error::Storage
                })?;
            Ok(())
        });
        let _ = tx.send(result);
    });
    rx.recv().map_err(|_| Error::Storage)?
}

#[cfg(target_os = "windows")]
pub fn copy(
    text: Zeroizing<String>,
    ttl: Duration,
    controller: std::sync::Arc<safay_app::controller::Controller>,
    epoch: u64,
) -> Result<(), Error> {
    use windows_sys::Win32::{
        Foundation::GlobalFree,
        System::{DataExchange::*, Memory::*, Ole::CF_UNICODETEXT, Threading::GetCurrentProcessId},
        UI::WindowsAndMessaging::*,
    };
    // Win32 owns each successful HGLOBAL; failed transfers are freed here.
    unsafe fn put(format: u32, bytes: &[u8]) -> bool {
        unsafe {
            let memory = GlobalAlloc(GMEM_MOVEABLE, bytes.len());
            if memory.is_null() {
                return false;
            }
            let destination = GlobalLock(memory);
            if destination.is_null() {
                GlobalFree(memory);
                return false;
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), destination.cast::<u8>(), bytes.len());
            GlobalUnlock(memory);
            if SetClipboardData(format, memory).is_null() {
                GlobalFree(memory);
                false
            } else {
                true
            }
        }
    }
    let mut count = 0;
    controller.at_epoch(epoch, || unsafe {
        let window = GetForegroundWindow();
        let mut process = 0;
        GetWindowThreadProcessId(window, &mut process);
        if window.is_null() || process != GetCurrentProcessId() || OpenClipboard(window) == 0 {
            return Err(Error::Storage);
        }
        let result = (|| {
            if EmptyClipboard() == 0 {
                return Err(Error::Storage);
            }
            for name in ["CanIncludeInClipboardHistory", "CanUploadToCloudClipboard"] {
                let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
                let format = RegisterClipboardFormatW(name.as_ptr());
                if format == 0 || !put(format, &0u32.to_ne_bytes()) {
                    EmptyClipboard();
                    return Err(Error::Storage);
                }
            }
            let units: Zeroizing<Vec<u16>> = Zeroizing::new(text.encode_utf16().chain(Some(0)).collect());
            let bytes = std::slice::from_raw_parts(units.as_ptr().cast::<u8>(), units.len() * 2);
            if !put(u32::from(CF_UNICODETEXT), bytes) {
                EmptyClipboard();
                return Err(Error::Storage);
            }
            count = GetClipboardSequenceNumber();
            Ok(())
        })();
        CloseClipboard();
        result
    })?;
    windows_expiry(count, ttl);
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn copy(
    text: Zeroizing<String>,
    ttl: Duration,
    controller: std::sync::Arc<safay_app::controller::Controller>,
    epoch: u64,
) -> Result<(), Error> {
    use nsh_platform_linux::gtk4::{gdk, glib, prelude::*};
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    glib::MainContext::default().invoke(move || {
        let result = controller.at_epoch(epoch, || {
            let display = gdk::Display::default().ok_or(Error::Storage)?;
            let clipboard = display.clipboard();
            let content = gdk::ContentProvider::for_value(&text.as_str().to_value());
            let hint = gdk::ContentProvider::for_bytes("x-kde-passwordManagerHint", &glib::Bytes::from_static(b"secret"));
            let owner = gdk::ContentProvider::new_union(&[content, hint]);
            clipboard.set_content(Some(&owner)).map_err(|_| Error::Storage)?;
            glib::timeout_add_local_once(ttl, move || {
                if clipboard.content().as_ref() == Some(&owner) {
                    let _ = clipboard.set_content(None::<&gdk::ContentProvider>);
                }
            });
            Ok(())
        });
        let _ = tx.send(result);
    });
    rx.recv().map_err(|_| Error::Storage)?
}

pub struct NativeClipboard;
impl safay_app::clipboard::SecretClipboard for NativeClipboard {
    fn copy(
        &self,
        text: Zeroizing<String>,
        ttl: Duration,
        controller: std::sync::Arc<safay_app::controller::Controller>,
        epoch: u64,
    ) -> Result<(), Error> {
        copy(text, ttl, controller, epoch)
    }
}

#[cfg(target_os = "windows")]
fn windows_expiry(sequence: u32, ttl: Duration) {
    use windows_sys::Win32::System::DataExchange::*;
    static SENDER: std::sync::OnceLock<std::sync::mpsc::Sender<(u32, Duration)>> = std::sync::OnceLock::new();
    let sender = SENDER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<(u32, Duration)>();
        std::thread::spawn(move || {
            let mut pending: Option<(u32, std::time::Instant)> = None;
            loop {
                let result = match pending {
                    Some((_, deadline)) => rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())),
                    None => rx.recv().map_err(|_| std::sync::mpsc::RecvTimeoutError::Disconnected),
                };
                match result {
                    Ok((count, duration)) => {
                        let now = std::time::Instant::now();
                        pending = Some((count, now.checked_add(duration).unwrap_or(now)));
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        if let Some((count, _)) = pending.take() {
                            unsafe {
                                if GetClipboardSequenceNumber() == count && OpenClipboard(std::ptr::null_mut()) != 0 {
                                    if GetClipboardSequenceNumber() == count {
                                        EmptyClipboard();
                                    }
                                    CloseClipboard();
                                }
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        tx
    });
    let _ = sender.send((sequence, ttl));
}
