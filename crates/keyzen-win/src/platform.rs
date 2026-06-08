use crate::{Emitter, KeyzenWinError};
use keyzen_core::{Config, Diagnostic, Engine, Event, EventKind, KeyCode, OutputCommand};
use std::mem::size_of;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};
use tracing::warn;
use windows_sys::Win32::Foundation::{GetLastError, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, MSG,
    PostThreadMessageW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL,
    WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

const LLKHF_INJECTED: u32 = 0x0000_0010;
const LLKHF_EXTENDED: u32 = 0x0000_0001;
const TIMER_MAX_SLEEP_MS: u64 = 50;

static RUNTIME: OnceLock<Arc<Runtime>> = OnceLock::new();

pub fn run(config: Config) -> Result<(), KeyzenWinError> {
    let (_tx, rx) = mpsc::channel();
    run_until_stop(config, rx)
}

pub fn run_until_stop(config: Config, stop: mpsc::Receiver<()>) -> Result<(), KeyzenWinError> {
    let runtime = Arc::new(Runtime::new(config));
    let _ = RUNTIME.set(runtime.clone());

    let thread_id = unsafe { GetCurrentThreadId() };
    let stop_runtime = runtime.clone();
    let stop_thread = thread::spawn(move || {
        while stop_runtime.running.load(Ordering::SeqCst) {
            match stop.recv_timeout(Duration::from_millis(TIMER_MAX_SLEEP_MS)) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    unsafe {
                        PostThreadMessageW(thread_id, WM_QUIT, 0, 0);
                    }
                    break;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    });

    let timer_runtime = runtime.clone();
    let timer_thread = thread::spawn(move || poll_timer(timer_runtime));

    let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), null_mut(), 0) };
    if hook.is_null() {
        runtime.running.store(false, Ordering::SeqCst);
        let _ = stop_thread.join();
        let _ = timer_thread.join();
        return Err(last_error("SetWindowsHookExW"));
    }

    let result = message_loop();

    unsafe {
        UnhookWindowsHookEx(hook);
    }
    runtime.running.store(false, Ordering::SeqCst);
    let _ = stop_thread.join();
    let _ = timer_thread.join();

    result
}

fn message_loop() -> Result<(), KeyzenWinError> {
    let mut message = MSG::default();
    loop {
        let result = unsafe { GetMessageW(&mut message, null_mut(), 0, 0) };
        if result == -1 {
            return Err(last_error("GetMessageW"));
        }
        if result == 0 || message.message == WM_QUIT {
            return Ok(());
        }
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

fn poll_timer(runtime: Arc<Runtime>) {
    while runtime.running.load(Ordering::SeqCst) {
        let sleep_ms = runtime
            .with_engine(|engine| {
                let now = runtime.now_ms();
                engine
                    .next_deadline_ms()
                    .map(|deadline| deadline.saturating_sub(now).clamp(1, TIMER_MAX_SLEEP_MS))
                    .unwrap_or(TIMER_MAX_SLEEP_MS)
            })
            .unwrap_or(TIMER_MAX_SLEEP_MS);

        thread::sleep(Duration::from_millis(sleep_ms));

        let now = runtime.now_ms();
        let outcome = runtime
            .with_engine(|engine| engine.poll(now))
            .unwrap_or_default();
        runtime.emit_outcome(outcome);
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let keyboard = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        if keyboard.flags & LLKHF_INJECTED == 0 {
            if let Some(event) = keyboard_event(wparam, keyboard) {
                if let Some(runtime) = RUNTIME.get() {
                    let outcome = runtime
                        .with_engine(|engine| engine.handle_event(event))
                        .unwrap_or_default();
                    let suppress = outcome.suppress;
                    runtime.emit_outcome(outcome);
                    if suppress {
                        return 1;
                    }
                }
            }
        }
    }

    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

fn keyboard_event(wparam: WPARAM, keyboard: &KBDLLHOOKSTRUCT) -> Option<Event> {
    let kind = match wparam as u32 {
        WM_KEYDOWN | WM_SYSKEYDOWN => EventKind::KeyDown,
        WM_KEYUP | WM_SYSKEYUP => EventKind::KeyUp,
        _ => return None,
    };

    Some(Event {
        key: key_from_keyboard(keyboard)?,
        kind,
        time_ms: RUNTIME.get().map(|runtime| runtime.now_ms()).unwrap_or(0),
    })
}

#[derive(Debug)]
struct Runtime {
    engine: Mutex<Engine>,
    emitter: SendInputEmitter,
    started_at: Instant,
    running: AtomicBool,
}

impl Runtime {
    fn new(config: Config) -> Self {
        Self {
            engine: Mutex::new(Engine::new(config)),
            emitter: SendInputEmitter,
            started_at: Instant::now(),
            running: AtomicBool::new(true),
        }
    }

    fn now_ms(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }

    fn with_engine<T>(&self, f: impl FnOnce(&mut Engine) -> T) -> Option<T> {
        match self.engine.lock() {
            Ok(mut engine) => Some(f(&mut engine)),
            Err(error) => {
                warn!("engine lock poisoned: {error}");
                None
            }
        }
    }

    fn emit_outcome(&self, outcome: keyzen_core::Outcome) {
        for diagnostic in outcome.diagnostics {
            match diagnostic {
                Diagnostic::Warning(message) => warn!("{message}"),
            }
        }
        if outcome.commands.is_empty() {
            return;
        }
        if let Err(error) = self.emitter.emit(&outcome.commands) {
            warn!("failed to emit input: {error}");
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SendInputEmitter;

impl Emitter for SendInputEmitter {
    fn emit(&self, commands: &[OutputCommand]) -> Result<(), KeyzenWinError> {
        for command in expand_commands(commands)? {
            send_keyboard_input(command)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KeyboardInput {
    vk: u16,
    key_up: bool,
}

fn expand_commands(commands: &[OutputCommand]) -> Result<Vec<KeyboardInput>, KeyzenWinError> {
    let mut inputs = Vec::new();
    for command in commands {
        match command {
            OutputCommand::KeyDown(key) => inputs.push(KeyboardInput {
                vk: vk_for_key(key)?,
                key_up: false,
            }),
            OutputCommand::KeyUp(key) => inputs.push(KeyboardInput {
                vk: vk_for_key(key)?,
                key_up: true,
            }),
            OutputCommand::Tap(key) => {
                if let Some((modifier, shifted_key)) = shifted_key(key) {
                    inputs.push(KeyboardInput {
                        vk: modifier,
                        key_up: false,
                    });
                    inputs.push(KeyboardInput {
                        vk: shifted_key,
                        key_up: false,
                    });
                    inputs.push(KeyboardInput {
                        vk: shifted_key,
                        key_up: true,
                    });
                    inputs.push(KeyboardInput {
                        vk: modifier,
                        key_up: true,
                    });
                } else {
                    let vk = vk_for_key(key)?;
                    inputs.push(KeyboardInput { vk, key_up: false });
                    inputs.push(KeyboardInput { vk, key_up: true });
                }
            }
        }
    }
    Ok(inputs)
}

fn send_keyboard_input(input: KeyboardInput) -> Result<(), KeyzenWinError> {
    let mut flags = 0;
    if input.key_up {
        flags |= KEYEVENTF_KEYUP;
    }

    let mut raw = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: input.vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

    let sent = unsafe { SendInput(1, &mut raw, size_of::<INPUT>() as i32) };
    if sent == 0 {
        return Err(last_error("SendInput"));
    }
    Ok(())
}

fn key_from_vk(vk: u16) -> Option<KeyCode> {
    let name = match vk {
        0x08 => "Backspace",
        0x09 => "Tab",
        0x0D => "Enter",
        0x10 => "Shift",
        0x11 => "Ctrl",
        0x12 => "Alt",
        0x13 => "Pause",
        0x14 => "CapsLock",
        0x1B => "Escape",
        0x20 => "Space",
        0x21 => "PageUp",
        0x22 => "PageDown",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "Left",
        0x26 => "Up",
        0x27 => "Right",
        0x28 => "Down",
        0x2D => "Insert",
        0x2E => "Delete",
        0x5B => "LeftWin",
        0x5C => "RightWin",
        0x5D => "Menu",
        0xA0 => "LeftShift",
        0xA1 => "RightShift",
        0xA2 => "LeftCtrl",
        0xA3 => "RightCtrl",
        0xA4 => "LeftAlt",
        0xA5 => "RightAlt",
        0x90 => "NumLock",
        0x91 => "ScrollLock",
        0xBA => "Semicolon",
        0xBB => "Equal",
        0xBC => "Comma",
        0xBD => "Minus",
        0xBE => "Period",
        0xBF => "Slash",
        0xC0 => "Grave",
        0xDB => "LeftBracket",
        0xDC => "Backslash",
        0xDD => "RightBracket",
        0xDE => "Quote",
        _ if (0x30..=0x39).contains(&vk) => {
            return KeyCode::new(char::from_u32(vk as u32)?.to_string()).ok();
        }
        _ if (0x41..=0x5A).contains(&vk) => {
            return KeyCode::new(char::from_u32(vk as u32)?.to_string()).ok();
        }
        _ if (0x70..=0x87).contains(&vk) => return KeyCode::new(format!("F{}", vk - 0x6F)).ok(),
        _ => return None,
    };
    KeyCode::new(name).ok()
}

fn key_from_keyboard(keyboard: &KBDLLHOOKSTRUCT) -> Option<KeyCode> {
    match keyboard.vkCode as u16 {
        0x10 => {
            if keyboard.scanCode == 0x36 {
                KeyCode::new("RightShift").ok()
            } else {
                KeyCode::new("LeftShift").ok()
            }
        }
        0x11 => {
            if keyboard.flags & LLKHF_EXTENDED != 0 {
                KeyCode::new("RightCtrl").ok()
            } else {
                KeyCode::new("LeftCtrl").ok()
            }
        }
        0x12 => {
            if keyboard.flags & LLKHF_EXTENDED != 0 {
                KeyCode::new("RightAlt").ok()
            } else {
                KeyCode::new("LeftAlt").ok()
            }
        }
        vk => key_from_vk(vk),
    }
}

fn shifted_key(key: &KeyCode) -> Option<(u16, u16)> {
    let shift = 0x10;
    let key = key.as_str();
    let shifted = match key {
        "DoubleQuote" => 0xDE,
        "Colon" => 0xBA,
        "LessThan" => 0xBC,
        "GreaterThan" => 0xBE,
        "Question" => 0xBF,
        "Pipe" => 0xDC,
        "Underscore" => 0xBD,
        "Plus" => 0xBB,
        "Tilde" => 0xC0,
        "LeftBrace" => 0xDB,
        "RightBrace" => 0xDD,
        "Exclamation" => 0x31,
        "At" => 0x32,
        "Hash" => 0x33,
        "Dollar" => 0x34,
        "Percent" => 0x35,
        "Caret" => 0x36,
        "Ampersand" => 0x37,
        "Asterisk" => 0x38,
        "LeftParen" => 0x39,
        "RightParen" => 0x30,
        _ => return None,
    };
    Some((shift, shifted))
}

fn vk_for_key(key: &KeyCode) -> Result<u16, KeyzenWinError> {
    let vk = match key.as_str() {
        "Backspace" => 0x08,
        "Tab" => 0x09,
        "Enter" => 0x0D,
        "Shift" => 0x10,
        "Ctrl" => 0x11,
        "Alt" => 0x12,
        "Pause" => 0x13,
        "CapsLock" => 0x14,
        "Escape" => 0x1B,
        "Space" => 0x20,
        "PageUp" => 0x21,
        "PageDown" => 0x22,
        "End" => 0x23,
        "Home" => 0x24,
        "Left" => 0x25,
        "Up" => 0x26,
        "Right" => 0x27,
        "Down" => 0x28,
        "Insert" => 0x2D,
        "Delete" => 0x2E,
        "0" => 0x30,
        "1" => 0x31,
        "2" => 0x32,
        "3" => 0x33,
        "4" => 0x34,
        "5" => 0x35,
        "6" => 0x36,
        "7" => 0x37,
        "8" => 0x38,
        "9" => 0x39,
        "A" => 0x41,
        "B" => 0x42,
        "C" => 0x43,
        "D" => 0x44,
        "E" => 0x45,
        "F" => 0x46,
        "G" => 0x47,
        "H" => 0x48,
        "I" => 0x49,
        "J" => 0x4A,
        "K" => 0x4B,
        "L" => 0x4C,
        "M" => 0x4D,
        "N" => 0x4E,
        "O" => 0x4F,
        "P" => 0x50,
        "Q" => 0x51,
        "R" => 0x52,
        "S" => 0x53,
        "T" => 0x54,
        "U" => 0x55,
        "V" => 0x56,
        "W" => 0x57,
        "X" => 0x58,
        "Y" => 0x59,
        "Z" => 0x5A,
        "LeftWin" | "Win" => 0x5B,
        "RightWin" => 0x5C,
        "Menu" => 0x5D,
        "LeftShift" => 0xA0,
        "RightShift" => 0xA1,
        "LeftCtrl" => 0xA2,
        "RightCtrl" => 0xA3,
        "LeftAlt" => 0xA4,
        "RightAlt" => 0xA5,
        "NumLock" => 0x90,
        "ScrollLock" => 0x91,
        "Semicolon" => 0xBA,
        "Equal" => 0xBB,
        "Comma" => 0xBC,
        "Minus" => 0xBD,
        "Period" => 0xBE,
        "Slash" => 0xBF,
        "Grave" => 0xC0,
        "LeftBracket" => 0xDB,
        "Backslash" => 0xDC,
        "RightBracket" => 0xDD,
        "Quote" => 0xDE,
        function if function.starts_with('F') => {
            let number = function[1..]
                .parse::<u16>()
                .map_err(|_| KeyzenWinError::UnsupportedKey(key.to_string()))?;
            if !(1..=24).contains(&number) {
                return Err(KeyzenWinError::UnsupportedKey(key.to_string()));
            }
            0x6F + number
        }
        _ => return Err(KeyzenWinError::UnsupportedKey(key.to_string())),
    };
    Ok(vk)
}

fn last_error(api: &'static str) -> KeyzenWinError {
    KeyzenWinError::WindowsApi {
        api,
        code: unsafe { GetLastError() },
    }
}

#[cfg(test)]
mod tests {
    use super::{KeyboardInput, expand_commands, key_from_keyboard};
    use keyzen_core::OutputCommand;
    use windows_sys::Win32::UI::WindowsAndMessaging::KBDLLHOOKSTRUCT;

    #[test]
    fn expands_shifted_tap() {
        let commands = vec![OutputCommand::Tap("DoubleQuote".parse().unwrap())];
        let expanded = expand_commands(&commands).unwrap();

        assert_eq!(
            expanded,
            vec![
                KeyboardInput {
                    vk: 0x10,
                    key_up: false
                },
                KeyboardInput {
                    vk: 0xDE,
                    key_up: false
                },
                KeyboardInput {
                    vk: 0xDE,
                    key_up: true
                },
                KeyboardInput {
                    vk: 0x10,
                    key_up: true
                },
            ]
        );
    }

    #[test]
    fn normalizes_modifier_sides_from_low_level_keyboard_data() {
        let right_shift = KBDLLHOOKSTRUCT {
            vkCode: 0x10,
            scanCode: 0x36,
            flags: 0,
            time: 0,
            dwExtraInfo: 0,
        };
        let right_ctrl = KBDLLHOOKSTRUCT {
            vkCode: 0x11,
            scanCode: 0,
            flags: 0x01,
            time: 0,
            dwExtraInfo: 0,
        };

        assert_eq!(
            key_from_keyboard(&right_shift).unwrap().as_str(),
            "RightShift"
        );
        assert_eq!(
            key_from_keyboard(&right_ctrl).unwrap().as_str(),
            "RightCtrl"
        );
    }
}
