use anyhow::Result;
use keyzen_core::{EventKind, OutputEvent};
use windows::Win32::UI::Input::KeyboardAndMouse::*;

use crate::keycode::key_to_vk;

pub const KEYZEN_EXTRA_INFO: usize = 0x4b5a_454e;

pub fn send_output(events: &[OutputEvent]) -> Result<()> {
    if events.is_empty() {
        return Ok(());
    }

    let mut inputs = Vec::with_capacity(events.len());
    for event in events {
        let flags = match event.kind {
            EventKind::Down => KEYBD_EVENT_FLAGS(0),
            EventKind::Up => KEYEVENTF_KEYUP,
        };
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: key_to_vk(event.key),
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: KEYZEN_EXTRA_INFO,
                },
            },
        });
    }

    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent != inputs.len() as u32 {
        anyhow::bail!("SendInput sent {sent} of {} events", inputs.len());
    }
    Ok(())
}
