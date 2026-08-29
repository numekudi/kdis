use std::sync::OnceLock;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, Window};
use x11rb::rust_connection::RustConnection;

const MOVERESIZE_MOVE: u32 = 8;
const LEFT_MOUSE_BUTTON: u32 = 1;
const SOURCE_APPLICATION: u32 = 1;

// Mutter reparents normal X11 windows after creation. Preserve the client id
// found during initial overlay configuration instead of searching the root
// tree again after that reparenting has happened.
static APPLICATION_WINDOW_ID: OnceLock<Window> = OnceLock::new();

/// Owns the X11 connection and identifiers needed for native window requests.
pub(crate) struct X11Window {
    connection: RustConnection,
    root: Window,
    id: Window,
}

impl X11Window {
    /// Locates the application's sole top-level window by its EWMH process id.
    pub(crate) fn for_current_process() -> Result<Self, String> {
        let (connection, screen_index) =
            x11rb::connect(None).map_err(|error| format!("could not connect to X11: {error}"))?;
        let root = connection.setup().roots[screen_index].root;
        let id = match APPLICATION_WINDOW_ID.get() {
            Some(id) => *id,
            None => {
                let net_wm_pid = intern_atom(&connection, b"_NET_WM_PID")?;
                let id = connection
                    .query_tree(root)
                    .map_err(|error| format!("could not query X11 windows: {error}"))?
                    .reply()
                    .map_err(|error| format!("could not read X11 windows: {error}"))?
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
                    .ok_or_else(|| {
                        "could not find the kdis X11 window by process id".to_string()
                    })?;
                APPLICATION_WINDOW_ID
                    .set(id)
                    .map_err(|_| "kdis X11 window id was initialized twice".to_string())?;
                id
            }
        };

        Ok(Self {
            connection,
            root,
            id,
        })
    }

    /// Keeps the movable normal window above applications and out of shell UI.
    pub(crate) fn configure_overlay(&self) -> Result<(), String> {
        let wm_state = intern_atom(&self.connection, b"_NET_WM_STATE")?;
        let wm_state_above = intern_atom(&self.connection, b"_NET_WM_STATE_ABOVE")?;
        let wm_state_skip_taskbar = intern_atom(&self.connection, b"_NET_WM_STATE_SKIP_TASKBAR")?;
        let wm_state_skip_pager = intern_atom(&self.connection, b"_NET_WM_STATE_SKIP_PAGER")?;

        // EWMH state messages carry at most two state atoms, so pager hiding
        // is sent separately from the above/taskbar pair.
        self.add_window_states(wm_state, wm_state_above, Some(wm_state_skip_taskbar))?;
        self.add_window_states(wm_state, wm_state_skip_pager, None)
    }

    fn add_window_states(
        &self,
        wm_state: u32,
        first: u32,
        second: Option<u32>,
    ) -> Result<(), String> {
        let event = ClientMessageEvent::new(
            32,
            self.id,
            wm_state,
            [1, first, second.unwrap_or_default(), SOURCE_APPLICATION, 0],
        );
        self.send_to_window_manager(event, "overlay state")
    }

    /// Transfers the active left-button gesture to the X11 window manager.
    pub(crate) fn start_move(&self) -> Result<(), String> {
        let moveresize = intern_atom(&self.connection, b"_NET_WM_MOVERESIZE")?;

        // The window manager must own the pointer grab during an interactive move.
        self.connection
            .ungrab_pointer(x11rb::CURRENT_TIME)
            .map_err(|error| format!("could not release the X11 pointer grab: {error}"))?
            .check()
            .map_err(|error| format!("could not release the X11 pointer grab: {error}"))?;
        let pointer = self
            .connection
            .query_pointer(self.id)
            .map_err(|error| format!("could not query the X11 pointer: {error}"))?
            .reply()
            .map_err(|error| format!("could not read the X11 pointer: {error}"))?;
        let event = ClientMessageEvent::new(
            32,
            self.id,
            moveresize,
            move_request_data(pointer.root_x, pointer.root_y),
        );

        self.send_to_window_manager(event, "window move")
    }

    fn send_to_window_manager(
        &self,
        event: ClientMessageEvent,
        request_name: &str,
    ) -> Result<(), String> {
        self.connection
            .send_event(
                false,
                self.root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                event,
            )
            .map_err(|error| format!("could not send {request_name} request: {error}"))?
            .check()
            .map_err(|error| format!("window manager rejected {request_name} request: {error}"))?;
        self.connection
            .flush()
            .map_err(|error| format!("could not flush {request_name} request: {error}"))
    }
}

fn intern_atom(connection: &RustConnection, name: &[u8]) -> Result<u32, String> {
    connection
        .intern_atom(false, name)
        .map_err(|error| format!("could not request X11 atom: {error}"))?
        .reply()
        .map(|reply| reply.atom)
        .map_err(|error| format!("could not resolve X11 atom: {error}"))
}

fn move_request_data(root_x: i16, root_y: i16) -> [u32; 5] {
    [
        root_x as u32,
        root_y as u32,
        MOVERESIZE_MOVE,
        LEFT_MOUSE_BUTTON,
        SOURCE_APPLICATION,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_request_identifies_the_pressed_left_button_and_application_source() {
        assert_eq!(move_request_data(120, 240), [120, 240, 8, 1, 1]);
    }
}
