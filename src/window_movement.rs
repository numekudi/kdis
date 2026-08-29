use gpui::Window;

#[cfg(not(target_os = "windows"))]
pub fn start(window: &Window) {
    window.start_window_move();
}

#[cfg(target_os = "windows")]
pub fn start(window: &Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::{HWND, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
    use windows::Win32::UI::WindowsAndMessaging::{HTCAPTION, SendMessageW, WM_NCLBUTTONDOWN};

    let raw_handle = HasWindowHandle::window_handle(window)
        .expect("GPUI must expose a window handle while processing mouse input")
        .as_raw();
    let RawWindowHandle::Win32(handle) = raw_handle else {
        unreachable!("a Windows build must expose a Win32 window handle");
    };
    let hwnd = HWND(handle.hwnd.get() as *mut _);

    // GPUI does not implement start_window_move on Windows. Hand the current
    // pointer gesture to the native non-client caption move loop instead.
    unsafe {
        ReleaseCapture().expect("failed to release pointer capture before window move");
        SendMessageW(
            hwnd,
            WM_NCLBUTTONDOWN,
            Some(WPARAM(HTCAPTION as usize)),
            None,
        );
    }
}
