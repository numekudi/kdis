mod history;
mod input;

use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use gpui::{
    App, Application, Bounds, Context, MouseButton, MouseDownEvent, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div, hsla, point,
    prelude::*, px, size,
};

use history::KeyHistory;
use input::InputMessage;

const HISTORY_CAPACITY: usize = 10;
const WINDOW_WIDTH: f32 = 260.0;
const WINDOW_HEIGHT: f32 = 380.0;

/// Requests the EWMH ABOVE state in addition to GPUI's notification window type.
#[cfg(target_os = "linux")]
fn request_always_on_top(_window: &Window) -> Result<(), String> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ClientMessageEvent, ConnectionExt, EventMask};

    let (connection, screen_index) =
        x11rb::connect(None).map_err(|error| format!("could not connect to X11: {error}"))?;
    let root = connection.setup().roots[screen_index].root;
    let net_wm_pid = connection
        .intern_atom(false, b"_NET_WM_PID")
        .map_err(|error| format!("could not request _NET_WM_PID: {error}"))?
        .reply()
        .map_err(|error| format!("could not resolve _NET_WM_PID: {error}"))?
        .atom;
    let window_id = connection
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
        .ok_or_else(|| "could not find the kdis X11 window by process id".to_string())?;
    let wm_state = connection
        .intern_atom(false, b"_NET_WM_STATE")
        .map_err(|error| format!("could not request _NET_WM_STATE: {error}"))?
        .reply()
        .map_err(|error| format!("could not resolve _NET_WM_STATE: {error}"))?
        .atom;
    let wm_state_above = connection
        .intern_atom(false, b"_NET_WM_STATE_ABOVE")
        .map_err(|error| format!("could not request _NET_WM_STATE_ABOVE: {error}"))?
        .reply()
        .map_err(|error| format!("could not resolve _NET_WM_STATE_ABOVE: {error}"))?
        .atom;
    let event = ClientMessageEvent::new(32, window_id, wm_state, [1, wm_state_above, 0, 1, 0]);
    connection
        .send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            event,
        )
        .map_err(|error| format!("could not send always-on-top request: {error}"))?;
    connection
        .flush()
        .map_err(|error| format!("could not flush always-on-top request: {error}"))?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn request_always_on_top(_window: &Window) -> Result<(), String> {
    Ok(())
}

struct KeyDisplay {
    history: KeyHistory,
    receiver: Receiver<InputMessage>,
    listener_error: Option<String>,
}

impl KeyDisplay {
    fn new(receiver: Receiver<InputMessage>) -> Self {
        Self {
            history: KeyHistory::new(HISTORY_CAPACITY, Instant::now()),
            receiver,
            listener_error: None,
        }
    }

    fn receive_pending_input(&mut self) {
        // Draining is non-blocking, so rendering never waits on the listener thread.
        for message in self.receiver.try_iter() {
            match message {
                InputMessage::Signal(signal) => self.history.apply(signal),
                InputMessage::ListenerFailed(error) => self.listener_error = Some(error),
            }
        }
    }
}

impl Render for KeyDisplay {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.receive_pending_input();
        let now = Instant::now();
        let show_chrome = window.is_window_active();

        // Keep polling global input. Active rows also need frames for their live timer.
        window.request_animation_frame();

        div()
            .size_full()
            .flex()
            .flex_col()
            .items_end()
            .gap(px(2.0))
            .p_2()
            .font_family("monospace")
            .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, window, _| {
                // The entire chrome-less surface acts as a drag handle.
                window.start_window_move();
            })
            .on_mouse_down(MouseButton::Right, |_: &MouseDownEvent, _, cx| {
                // A chrome-less overlay still needs an unobtrusive way to exit.
                cx.quit();
            })
            .when_some(self.listener_error.clone(), |view, error| {
                view.child(
                    div()
                        .w_full()
                        .p_4()
                        .rounded_lg()
                        .border_1()
                        .border_color(hsla(0.02, 0.78, 0.62, 0.9))
                        .bg(hsla(0.02, 0.45, 0.10, 0.94))
                        .text_sm()
                        .text_color(hsla(0.0, 0.0, 1.0, 0.94))
                        .child("KEYBOARD ACCESS REQUIRED")
                        .child(div().mt_2().text_xs().child(error)),
                )
            })
            .children(self.history.rows().enumerate().map(|(index, row)| {
                let duration_ms = row.display_millis_at(now);
                let pressed = row.is_current();
                let opacity = (1.0_f32 - index as f32 * 0.075).max(0.32);

                div()
                    .w_full()
                    .h(px(30.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .rounded_md()
                    .border_1()
                    .border_color(if !show_chrome {
                        hsla(0.0, 0.0, 0.0, 0.0)
                    } else if pressed {
                        hsla(0.48, 0.86, 0.58, 0.9)
                    } else {
                        hsla(0.0, 0.0, 1.0, 0.16)
                    })
                    .bg(if !show_chrome {
                        hsla(0.0, 0.0, 0.0, 0.0)
                    } else if pressed {
                        hsla(0.48, 0.55, 0.12, 0.90)
                    } else {
                        hsla(0.63, 0.20, 0.08, 0.82)
                    })
                    .text_color(hsla(0.0, 0.0, 1.0, opacity))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .children(row.keys().map(|key| {
                                div()
                                    .min_w(px(18.0))
                                    .text_center()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child(key.label.clone())
                            })),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(if pressed {
                                hsla(0.48, 0.82, 0.70, opacity)
                            } else {
                                hsla(0.0, 0.0, 0.78, opacity)
                            })
                            .child(format!("{duration_ms} ms")),
                    )
            }))
    }
}

fn main() {
    let (sender, receiver) = channel();
    input::start_global_listener(sender);

    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::new(
            point(px(32.0), px(80.0)),
            size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)),
        );
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                focus: false,
                kind: WindowKind::PopUp,
                is_movable: true,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                app_id: Some("kdis".into()),
                window_min_size: Some(size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT))),
                window_decorations: None,
                ..Default::default()
            },
            move |window, cx| {
                request_always_on_top(window)
                    .expect("failed to request always-on-top window state");
                cx.new(|_| KeyDisplay::new(receiver))
            },
        )
        .expect("failed to open key display window");
    });
}
