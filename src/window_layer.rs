use gpui::Window;

/// Applies overlay states without using the immovable X11 notification type.
#[cfg(target_os = "linux")]
pub fn configure(_window: &Window) {
    crate::x11_window::X11Window::for_current_process()
        .and_then(|window| window.configure_overlay())
        .expect("failed to configure the key display overlay");
}

/// GPUI's popup window level is already elevated on macOS.
#[cfg(target_os = "macos")]
pub fn configure(_window: &Window) {}

/// Promotes the GPUI popup to Windows' persistent topmost z-order band.
#[cfg(target_os = "windows")]
pub fn configure(window: &Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        HWND_TOPMOST, SET_WINDOW_POS_FLAGS, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
    };

    let raw_handle = HasWindowHandle::window_handle(window)
        .expect("GPUI must expose a window handle while configuring its z-order")
        .as_raw();
    let RawWindowHandle::Win32(handle) = raw_handle else {
        unreachable!("a Windows build must expose a Win32 window handle");
    };
    let hwnd = HWND(handle.hwnd.get() as *mut _);

    // Changing only the z-order preserves GPUI's chosen position and dimensions,
    // while NOACTIVATE prevents the keyboard overlay from stealing focus.
    let flags: SET_WINDOW_POS_FLAGS = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;
    unsafe {
        SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, flags)
            .expect("failed to keep the key display window always on top");
    }
}
