use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};

use anyhow::{Context, Result};
use keyzen_core::{Engine, EngineEvent, EventKind};
use windows::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    UI::WindowsAndMessaging::{
        CallNextHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, LLKHF_INJECTED, SetWindowsHookExW,
        UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    },
};

use crate::{keycode::vk_to_key, output::send_output};

static ENGINE: OnceLock<Arc<Mutex<Engine>>> = OnceLock::new();
static PAUSED: OnceLock<Arc<AtomicBool>> = OnceLock::new();

pub struct KeyboardHook {
    handle: HHOOK,
}

impl KeyboardHook {
    pub fn install(engine: Arc<Mutex<Engine>>, paused: Arc<AtomicBool>) -> Result<Self> {
        let _ = ENGINE.set(engine);
        let _ = PAUSED.set(paused);
        let handle = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0) }
            .context("failed to install low-level keyboard hook")?;
        Ok(Self { handle })
    }
}

impl Drop for KeyboardHook {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWindowsHookEx(self.handle);
        }
    }
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code != HC_ACTION as i32 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    if PAUSED
        .get()
        .is_some_and(|paused| paused.load(Ordering::Relaxed))
    {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let keyboard = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };
    if keyboard.flags.contains(LLKHF_INJECTED) {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let Some(key) = vk_to_key(keyboard.vkCode) else {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    };

    let kind = match wparam.0 as u32 {
        WM_KEYDOWN | WM_SYSKEYDOWN => EventKind::Down,
        WM_KEYUP | WM_SYSKEYUP => EventKind::Up,
        _ => return unsafe { CallNextHookEx(None, code, wparam, lparam) },
    };

    let Some(engine) = ENGINE.get() else {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    };

    let plan = match engine.lock() {
        Ok(mut engine) => engine.handle_event(EngineEvent { key, kind }),
        Err(_) => return unsafe { CallNextHookEx(None, code, wparam, lparam) },
    };

    if let Err(error) = send_output(&plan.events) {
        eprintln!("KeyZen output error: {error:#}");
    }

    if plan.consume_input {
        LRESULT(1)
    } else {
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }
}
