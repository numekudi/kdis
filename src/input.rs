use std::sync::mpsc::Sender;
use std::time::Instant;

use pynput::keyboard::{Key, KeyCode, KeyInput, Listener};

use crate::history::{KeyId, KeySignal};

/// Messages crossing from the platform listener into the GPUI thread.
pub enum InputMessage {
    Signal(KeySignal),
    ListenerFailed(String),
}

/// Starts the OS-level listener on its own thread; callbacks only enqueue work.
pub fn start_global_listener(sender: Sender<InputMessage>) {
    std::thread::Builder::new()
        .name("global-key-listener".into())
        .spawn(move || {
            let press_sender = sender.clone();
            let release_sender = sender.clone();
            let listener = Listener::builder()
                .on_press(move |key, _injected| {
                    if let Some(key) = key {
                        let _ = press_sender.send(InputMessage::Signal(signal_for(key, true)));
                    }
                    true
                })
                .on_release(move |key, _injected| {
                    if let Some(key) = key {
                        let _ = release_sender.send(InputMessage::Signal(signal_for(key, false)));
                    }
                    true
                })
                .start();

            match listener {
                Ok(listener) => {
                    if let Err(error) = listener.join() {
                        let _ = sender.send(InputMessage::ListenerFailed(error.to_string()));
                    }
                }
                Err(error) => {
                    let _ = sender.send(InputMessage::ListenerFailed(error.to_string()));
                }
            }
        })
        .expect("failed to spawn global keyboard listener thread");
}

fn signal_for(key: KeyInput, pressed: bool) -> KeySignal {
    let description = describe_key(&key);
    let at = Instant::now();
    if pressed {
        KeySignal::Pressed {
            key: description.id,
            label: description.label,
            sort_code: description.sort_code,
            at,
        }
    } else {
        KeySignal::Released {
            key: description.id,
            at,
        }
    }
}

struct KeyDescription {
    id: KeyId,
    label: String,
    sort_code: u32,
}

fn describe_key(input: &KeyInput) -> KeyDescription {
    match input {
        KeyInput::Key(key) => KeyDescription {
            id: KeyId::new(format!("key:{}", key.name())),
            label: special_key_label(*key),
            sort_code: special_key_sort_code(*key),
        },
        KeyInput::Code(code) => describe_key_code(code),
    }
}

fn describe_key_code(code: &KeyCode) -> KeyDescription {
    if let Some(vk) = code.vk {
        return KeyDescription {
            id: KeyId::new(format!("vk:{vk}")),
            label: platform_key_label(vk),
            sort_code: vk,
        };
    }

    let character = code
        .char
        .expect("pynput KeyCode must contain either a virtual key or a character");
    KeyDescription {
        id: KeyId::new(format!("char:{character}")),
        label: character.to_uppercase().collect(),
        sort_code: u32::from(character),
    }
}

fn special_key_label(key: Key) -> String {
    match key {
        Key::Up => "↑".into(),
        Key::Left => "←".into(),
        Key::Right => "→".into(),
        Key::Down => "↓".into(),
        _ => key.name().replace('_', " ").to_uppercase(),
    }
}

#[cfg(target_os = "linux")]
fn platform_key_label(vk: u32) -> String {
    use evdev::Key as EvdevKey;

    let code = u16::try_from(vk).expect("Linux evdev key codes always fit in u16");
    let key = EvdevKey::new(code);
    let label = match key {
        EvdevKey::KEY_1 => "1",
        EvdevKey::KEY_2 => "2",
        EvdevKey::KEY_3 => "3",
        EvdevKey::KEY_4 => "4",
        EvdevKey::KEY_5 => "5",
        EvdevKey::KEY_6 => "6",
        EvdevKey::KEY_7 => "7",
        EvdevKey::KEY_8 => "8",
        EvdevKey::KEY_9 => "9",
        EvdevKey::KEY_0 => "0",
        EvdevKey::KEY_Q => "Q",
        EvdevKey::KEY_W => "W",
        EvdevKey::KEY_E => "E",
        EvdevKey::KEY_R => "R",
        EvdevKey::KEY_T => "T",
        EvdevKey::KEY_Y => "Y",
        EvdevKey::KEY_U => "U",
        EvdevKey::KEY_I => "I",
        EvdevKey::KEY_O => "O",
        EvdevKey::KEY_P => "P",
        EvdevKey::KEY_A => "A",
        EvdevKey::KEY_S => "S",
        EvdevKey::KEY_D => "D",
        EvdevKey::KEY_F => "F",
        EvdevKey::KEY_G => "G",
        EvdevKey::KEY_H => "H",
        EvdevKey::KEY_J => "J",
        EvdevKey::KEY_K => "K",
        EvdevKey::KEY_L => "L",
        EvdevKey::KEY_Z => "Z",
        EvdevKey::KEY_X => "X",
        EvdevKey::KEY_C => "C",
        EvdevKey::KEY_V => "V",
        EvdevKey::KEY_B => "B",
        EvdevKey::KEY_N => "N",
        EvdevKey::KEY_M => "M",
        EvdevKey::KEY_MINUS => "-",
        EvdevKey::KEY_EQUAL => "=",
        EvdevKey::KEY_LEFTBRACE => "[",
        EvdevKey::KEY_RIGHTBRACE => "]",
        EvdevKey::KEY_SEMICOLON => ";",
        EvdevKey::KEY_APOSTROPHE => "'",
        EvdevKey::KEY_GRAVE => "`",
        EvdevKey::KEY_BACKSLASH => "\\",
        EvdevKey::KEY_COMMA => ",",
        EvdevKey::KEY_DOT => ".",
        EvdevKey::KEY_SLASH => "/",
        _ => {
            // evdev supplies symbolic names for layout-specific and extended keys.
            let symbolic_name = format!("{key:?}");
            return symbolic_name
                .strip_prefix("KEY_")
                .unwrap_or(&symbolic_name)
                .replace('_', " ");
        }
    };
    label.into()
}

#[cfg(not(target_os = "linux"))]
fn platform_key_label(vk: u32) -> String {
    if let Some(character) = char::from_u32(vk).filter(|value| value.is_ascii_alphanumeric()) {
        character.to_ascii_uppercase().to_string()
    } else {
        format!("VK {vk}")
    }
}

#[cfg(target_os = "linux")]
fn special_key_sort_code(key: Key) -> u32 {
    match key {
        Key::Esc => 1,
        Key::Backspace => 14,
        Key::Tab => 15,
        Key::Enter => 28,
        Key::Ctrl | Key::CtrlL => 29,
        Key::Shift | Key::ShiftL => 42,
        Key::ShiftR => 54,
        Key::Alt | Key::AltL => 56,
        Key::Space => 57,
        Key::CapsLock => 58,
        Key::F1 => 59,
        Key::F2 => 60,
        Key::F3 => 61,
        Key::F4 => 62,
        Key::F5 => 63,
        Key::F6 => 64,
        Key::F7 => 65,
        Key::F8 => 66,
        Key::F9 => 67,
        Key::F10 => 68,
        Key::NumLock => 69,
        Key::ScrollLock => 70,
        Key::F11 => 87,
        Key::F12 => 88,
        Key::CtrlR => 97,
        Key::PrintScreen => 99,
        Key::AltR | Key::AltGr => 100,
        Key::Home => 102,
        Key::Up => 103,
        Key::PageUp => 104,
        Key::Left => 105,
        Key::Right => 106,
        Key::End => 107,
        Key::Down => 108,
        Key::PageDown => 109,
        Key::Insert => 110,
        Key::Delete => 111,
        Key::MediaVolumeMute => 113,
        Key::MediaVolumeDown => 114,
        Key::MediaVolumeUp => 115,
        Key::Pause => 119,
        Key::Cmd | Key::CmdL => 125,
        Key::CmdR => 126,
        Key::Menu => 139,
        Key::MediaNext => 163,
        Key::MediaPlayPause => 164,
        Key::MediaPrevious => 165,
        Key::F13 => 183,
        Key::F14 => 184,
        Key::F15 => 185,
        Key::F16 => 186,
        Key::F17 => 187,
        Key::F18 => 188,
        Key::F19 => 189,
        Key::F20 => 190,
    }
}

#[cfg(not(target_os = "linux"))]
fn special_key_sort_code(key: Key) -> u32 {
    0x1_0000 + key as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_direction_keys_as_arrows() {
        assert_eq!(special_key_label(Key::Up), "↑");
        assert_eq!(special_key_label(Key::Left), "←");
        assert_eq!(special_key_label(Key::Right), "→");
        assert_eq!(special_key_label(Key::Down), "↓");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn maps_linux_evdev_codes_to_key_labels() {
        assert_eq!(platform_key_label(30), "A");
        assert_eq!(platform_key_label(48), "B");
        assert_eq!(platform_key_label(2), "1");
    }
}
