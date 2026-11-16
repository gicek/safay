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
