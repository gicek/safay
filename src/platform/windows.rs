//! A hidden native notification window receives session lock and suspend events.
use safay_app::controller::Controller;
use std::{sync::Arc, time::Duration};
use windows_sys::Win32::{
    Foundation::*,
    System::{LibraryLoader::GetModuleHandleW, RemoteDesktop::*, Threading::GetCurrentProcessId},
    UI::WindowsAndMessaging::*,
};
unsafe extern "system" fn procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if message == WM_NCCREATE {
            let create = &*(lparam as *const CREATESTRUCTW);
            SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut Arc<Controller>;
        if !pointer.is_null() {
            if (message == WM_WTSSESSION_CHANGE
                && matches!(wparam as u32, WTS_SESSION_LOCK | WTS_SESSION_LOGOFF | WTS_REMOTE_DISCONNECT | WTS_CONSOLE_DISCONNECT))
                || (message == WM_POWERBROADCAST && wparam == 4)
            {
                (&*pointer).request_lock();
            }
            if message == WM_NCDESTROY {
                SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            }
        }
        DefWindowProcW(window, message, wparam, lparam)
    }
}
pub fn observe(controller: Arc<Controller>) {
    std::thread::spawn(move || {
        unsafe {
            let class: Vec<u16> = "SafaySessionObserver".encode_utf16().chain(Some(0)).collect();
            let module = GetModuleHandleW(std::ptr::null());
            let specification =
                WNDCLASSW { lpfnWndProc: Some(procedure), hInstance: module, lpszClassName: class.as_ptr(), ..std::mem::zeroed() };
            RegisterClassW(&specification);
            let state = Box::new(controller.clone());
            let pointer = (&*state as *const Arc<Controller>).cast_mut();
            // A hidden top-level window receives power broadcasts too; message-only windows do not.
            let window = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                class.as_ptr(),
                class.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                module,
                pointer.cast(),
            );
            if window.is_null() {
                foreground_fallback(controller);
                return;
            }
            if WTSRegisterSessionNotification(window, NOTIFY_FOR_THIS_SESSION) == 0 {
                DestroyWindow(window);
                foreground_fallback(controller);
                return;
            }
            let mut message: MSG = std::mem::zeroed();
            while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            WTSUnRegisterSessionNotification(window);
            DestroyWindow(window);
        }
    });
}
fn foreground_fallback(controller: Arc<Controller>) {
    let mut active = true;
    loop {
        let current = unsafe {
            let mut process = 0;
            GetWindowThreadProcessId(GetForegroundWindow(), &mut process);
            process == GetCurrentProcessId()
        };
        if active && !current {
            controller.request_lock();
        }
        active = current;
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Hands this launch to the Safay already running (a second launch: the Start menu, a sign-in's callback link) and
/// whether it took it. It speaks the NSH runner's own hand-over (nsh-platform-windows `linking`): the window marked with
/// the property `nsh.app.<app id>` takes the launch's arguments and directory as `SecondInstance` JSON in a `WM_COPYDATA`
/// of kind "NSHI", then shows itself, comes to the front, and passes on the links among the arguments. NSH does this
/// itself only after the vault is opened, which the running Safay holds. Recheck on every NSH upgrade.
pub fn hand_to_running() -> bool {
    use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
    /// `COPYDATASTRUCT::dwData` of a forwarded launch ("NSHI").
    const COPY_INSTANCE: usize = 0x4E53_4849;
    let id: String = super::APP_ID.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' }).collect();
    let property: Vec<u16> = format!("nsh.app.{id}").encode_utf16().chain([0]).collect();
    let data = nsh_core::single_instance::SecondInstance::current().to_json().to_string().into_bytes();
    unsafe extern "system" fn each(window: HWND, search: LPARAM) -> windows_sys::core::BOOL {
        // SAFETY: `search` is the property and result pair below, alive for the enumeration.
        let search = unsafe { &mut *(search as *mut (*const u16, HWND)) };
        // SAFETY: a plain read of a window's property, named by a NUL-terminated string.
        if unsafe { !GetPropW(window, search.0).is_null() } {
            search.1 = window;
            return 0;
        }
        1
    }
    // The running Safay may still be opening its window: look for it for up to 5 s.
    for _ in 0..50 {
        let mut search: (*const u16, HWND) = (property.as_ptr(), std::ptr::null_mut());
        // SAFETY: `each` only uses `search`, which outlives the enumeration.
        unsafe { EnumWindows(Some(each), &mut search as *mut _ as LPARAM) };
        let window = search.1;
        if !window.is_null() {
            let copy = COPYDATASTRUCT { dwData: COPY_INSTANCE, cbData: data.len() as u32, lpData: data.as_ptr() as *mut _ };
            let mut result = 0usize;
            // SAFETY: plain calls on another process's window; `copy` and `data` outlive the synchronous send.
            unsafe {
                let mut pid = 0u32;
                GetWindowThreadProcessId(window, &mut pid);
                AllowSetForegroundWindow(pid);
                SendMessageTimeoutW(window, WM_COPYDATA, 0, &copy as *const _ as LPARAM, SMTO_ABORTIFHUNG, 5000, &mut result);
            }
            return result == 1;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// A restart (after an update): NSH starts the new process at once, naming the one it replaces in `NSH_RESTART_AFTER`
/// for its runner to wait for, but that one holds the vault's instance lock until it exits, and `main` opens the vault
/// before the runner starts. Waits for it here (at most NSH's limit), so this launch neither fails on the lock nor hands
/// itself to the process that is going away. Whether this launch is such a restart.
pub fn wait_for_replaced() -> bool {
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject};
    let Some(pid) = nsh_core::restart::wait_for_pid() else { return false };
    // SAFETY: plain calls; the handle is closed once waited on.
    unsafe {
        let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if !process.is_null() {
            WaitForSingleObject(process, nsh_core::restart::WAIT_LIMIT.as_millis() as u32);
            CloseHandle(process);
        }
    }
    true
}
