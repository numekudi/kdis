use gpui::Window;

/// Requests the EWMH ABOVE state in addition to GPUI's notification window type.
#[cfg(target_os = "linux")]
pub fn enable_always_on_top(_window: &Window) {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ClientMessageEvent, ConnectionExt, EventMask};

    let (connection, screen_index) =
        x11rb::connect(None).expect("could not connect to X11 for always-on-top configuration");
    let root = connection.setup().roots[screen_index].root;
    let net_wm_pid = connection
        .intern_atom(false, b"_NET_WM_PID")
        .expect("could not request _NET_WM_PID")
        .reply()
        .expect("could not resolve _NET_WM_PID")
        .atom;
    let window_id = connection
        .query_tree(root)
        .expect("could not query X11 windows")
        .reply()
        .expect("could not read X11 windows")
        .children
        .into_iter()
        .find_map(|window_id| {
            let reply = connection
                .get_property(false, window_id, net_wm_pid, AtomEnum::CARDINAL, 0, 1)
                .ok()?
                .reply()
                .ok()?;
            (reply.value32()?.next()? == std::process::id()).then_some(window_id)
        })
        .expect("could not find the kdis X11 window by process id");
    let wm_state = connection
        .intern_atom(false, b"_NET_WM_STATE")
        .expect("could not request _NET_WM_STATE")
        .reply()
        .expect("could not resolve _NET_WM_STATE")
        .atom;
    let wm_state_above = connection
        .intern_atom(false, b"_NET_WM_STATE_ABOVE")
        .expect("could not request _NET_WM_STATE_ABOVE")
        .reply()
        .expect("could not resolve _NET_WM_STATE_ABOVE")
        .atom;
    let event = ClientMessageEvent::new(32, window_id, wm_state, [1, wm_state_above, 0, 1, 0]);

    connection
        .send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            event,
        )
        .expect("could not send always-on-top request");
    connection
        .flush()
        .expect("could not flush always-on-top request");
}

/// GPUI's popup window level is already elevated on macOS.
#[cfg(target_os = "macos")]
pub fn enable_always_on_top(_window: &Window) {}

/// Promotes the GPUI popup to Windows' persistent topmost z-order band.
#[cfg(target_os = "windows")]
pub fn enable_always_on_top(window: &Window) {
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
