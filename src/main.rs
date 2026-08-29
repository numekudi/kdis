#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod history;
#[cfg(not(target_os = "windows"))]
mod input;
#[cfg(target_os = "windows")]
#[path = "input_windows.rs"]
mod input;
mod window_layer;
mod window_movement;

use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use gpui::{
    App, Application, Bounds, Context, Div, Hsla, MouseButton, MouseDownEvent, MouseUpEvent,
    SharedString, Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div,
    hsla, point, prelude::*, px, size,
};

use history::KeyHistory;
use input::InputMessage;

const HISTORY_CAPACITY: usize = 10;
const WINDOW_WIDTH: f32 = 260.0;
const WINDOW_HEIGHT: f32 = 380.0;

/// Layers a restrained four-direction outline behind the foreground glyphs.
fn outlined_text(text: impl Into<SharedString>, foreground: Hsla, outline: Hsla) -> Div {
    let text = text.into();
    const OUTLINE_OFFSETS: [(f32, f32); 4] = [(0.0, -1.0), (-1.0, 0.0), (1.0, 0.0), (0.0, 1.0)];
    let transparent = Hsla {
        a: 0.0,
        ..foreground
    };

    div()
        .relative()
        // This invisible copy alone participates in layout, keeping every
        // painted layer on the exact same origin and baseline.
        .child(div().text_color(transparent).child(text.clone()))
        .children(OUTLINE_OFFSETS.map(|(x, y)| {
            div()
                .absolute()
                .left(px(x))
                .top(px(y))
                .text_color(outline)
                .child(text.clone())
        }))
        // Positioned layers paint in insertion order; foreground must be last.
        .child(
            div()
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .text_color(foreground)
                .child(text),
        )
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
                window_movement::start(window);
            })
            .on_mouse_up(MouseButton::Right, |_: &MouseUpEvent, _, cx| {
                // A chrome-less overlay still needs an unobtrusive way to exit.
                // Wait for release so the popup captures both halves of the gesture;
                // destroying it on button-down would leak mouse-up to the window below.
                cx.stop_propagation();
                cx.defer(|cx| cx.quit());
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
                let foreground = hsla(0.0, 0.0, 1.0, opacity);
                let outline = hsla(0.0, 0.0, 0.0, opacity);

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
                                    .child(outlined_text(key.label.clone(), foreground, outline))
                            })),
                    )
                    .child(div().text_sm().child(outlined_text(
                        format!("{duration_ms} ms"),
                        if pressed {
                            hsla(0.48, 0.82, 0.70, opacity)
                        } else {
                            hsla(0.0, 0.0, 0.78, opacity)
                        },
                        outline,
                    )))
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
                window_layer::enable_always_on_top(window);
                cx.new(|_| KeyDisplay::new(receiver))
            },
        )
        .expect("failed to open key display window");
    });
}
