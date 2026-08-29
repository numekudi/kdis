use std::sync::OnceLock;
use std::sync::mpsc::Sender;
use std::time::Instant;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT,
    VK_SHIFT, VK_SPACE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, HHOOK, KBDLLHOOKSTRUCT, MSG, SetWindowsHookExW,
    UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::history::{KeyId, KeySignal};

/// Messages crossing from the platform listener into the GPUI thread.
pub enum InputMessage {
    Signal(KeySignal),
    ListenerFailed(String),
}

static INPUT_SENDER: OnceLock<Sender<InputMessage>> = OnceLock::new();

/// Starts a low-level keyboard hook on a dedicated Windows message-loop thread.
pub fn start_global_listener(sender: Sender<InputMessage>) {
    let error_sender = sender.clone();
    if INPUT_SENDER.set(sender).is_err() {
        let _ = error_sender.send(InputMessage::ListenerFailed(
            "global keyboard listener was started more than once".into(),
        ));
        return;
    }

    std::thread::Builder::new()
        .name("global-key-listener".into())
        .spawn(move || {
            if let Err(error) = run_message_loop() {
                let _ = error_sender.send(InputMessage::ListenerFailed(error));
            }
        })
        .expect("failed to spawn global keyboard listener thread");
}

fn run_message_loop() -> Result<(), String> {
    // A WH_KEYBOARD_LL callback runs on the installing thread, which must pump messages.
    let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), None, 0) }
        .map_err(|error| format!("failed to install keyboard hook: {error}"))?;
    let _hook_guard = HookGuard(hook);
    let mut message = MSG::default();

    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) }.0;
        match result {
            -1 => return Err("keyboard listener message loop failed".into()),
            0 => return Ok(()),
            _ => {}
        }
    }
}

struct HookGuard(HHOOK);

impl Drop for HookGuard {
    fn drop(&mut self) {
        // The hook belongs to this thread and is removed before its message loop exits.
        let _ = unsafe { UnhookWindowsHookEx(self.0) };
    }
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let message = wparam.0 as u32;
        let pressed = matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN);
        let released = matches!(message, WM_KEYUP | WM_SYSKEYUP);
        if pressed || released {
            // Windows guarantees KBDLLHOOKSTRUCT for non-negative low-level hook callbacks.
            let event = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            if let Some(sender) = INPUT_SENDER.get() {
                let _ = sender.send(InputMessage::Signal(signal_for(event, pressed)));
            }
        }
    }

    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn signal_for(event: &KBDLLHOOKSTRUCT, pressed: bool) -> KeySignal {
    // Scan code and flags distinguish physical left/right and extended keys sharing a VK code.
    // Only the extended-key bit is stable across the matching key-down/key-up pair.
    let extended = event.flags.0 & 1;
    let key = KeyId::new(format!("vk:{}:{}:{extended}", event.vkCode, event.scanCode));
    let at = Instant::now();
    if pressed {
        KeySignal::Pressed {
            key,
            label: key_label(event.vkCode),
            sort_code: display_sort_code(event.vkCode, event.scanCode),
            at,
        }
    } else {
        KeySignal::Released { key, at }
    }
}

fn key_label(vk: u32) -> String {
    match vk {
        0x08 => "BACKSPACE".into(),
        0x09 => "TAB".into(),
        0x0D => "ENTER".into(),
        0x1B => "ESC".into(),
        value if value == VK_SPACE.0 as u32 => "SPACE".into(),
        value if value == VK_SHIFT.0 as u32 => "SHIFT".into(),
        value if value == VK_LSHIFT.0 as u32 => "L-SHIFT".into(),
        value if value == VK_RSHIFT.0 as u32 => "R-SHIFT".into(),
        value if value == VK_CONTROL.0 as u32 => "CTRL".into(),
        value if value == VK_LCONTROL.0 as u32 => "L-CTRL".into(),
        value if value == VK_RCONTROL.0 as u32 => "R-CTRL".into(),
        value if value == VK_MENU.0 as u32 => "ALT".into(),
        value if value == VK_LMENU.0 as u32 => "L-ALT".into(),
        value if value == VK_RMENU.0 as u32 => "R-ALT".into(),
        0x25 => "←".into(),
        0x26 => "↑".into(),
        0x27 => "→".into(),
        0x28 => "↓".into(),
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(vk)
            .expect("ASCII virtual-key codes are valid Unicode")
            .to_string(),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        _ => format!("VK {vk}"),
    }
}

/// Places modifiers on the left, with left-hand variants before right-hand ones.
fn display_sort_code(vk: u32, scan_code: u32) -> u32 {
    let group = match vk {
        value
            if value == VK_CONTROL.0 as u32
                || value == VK_LCONTROL.0 as u32
                || value == VK_RCONTROL.0 as u32 =>
        {
            0
        }
        value
            if value == VK_SHIFT.0 as u32
                || value == VK_LSHIFT.0 as u32
                || value == VK_RSHIFT.0 as u32 =>
        {
            1
        }
        value
            if value == VK_MENU.0 as u32
                || value == VK_LMENU.0 as u32
                || value == VK_RMENU.0 as u32 =>
        {
            2
        }
        0x5B | 0x5C => 3, // Windows keys
        _ => 4,
    };
    (group << 16) | scan_code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_common_windows_virtual_keys() {
        assert_eq!(key_label(0x20), "SPACE");
        assert_eq!(key_label(0xA0), "L-SHIFT");
        assert_eq!(key_label(0xA1), "R-SHIFT");
        assert_eq!(key_label(0xA2), "L-CTRL");
        assert_eq!(key_label(0xA3), "R-CTRL");
        assert_eq!(key_label(0x0D), "ENTER");
        assert_eq!(key_label(0x41), "A");
        assert_eq!(key_label(0x31), "1");
        assert_eq!(key_label(0x25), "←");
        assert_eq!(key_label(0x7B), "F12");
    }

    #[test]
    fn sorts_modifiers_before_regular_keys() {
        assert!(display_sort_code(0xA2, 29) < display_sort_code(0xA0, 42));
        assert!(display_sort_code(0xA0, 42) < display_sort_code(0x41, 30));
    }
}
