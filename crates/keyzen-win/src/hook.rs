use std::sync::{
    Arc, Condvar, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
static TIMER: OnceLock<TimerHandle> = OnceLock::new();
static OUTPUT_ERROR_LOGGED: AtomicBool = AtomicBool::new(false);

pub struct KeyboardHook {
    handle: HHOOK,
    _timer: TimerWorker,
}

impl KeyboardHook {
    pub fn install(engine: Arc<Mutex<Engine>>, paused: Arc<AtomicBool>) -> Result<Self> {
        let _ = ENGINE.set(engine);
        let _ = PAUSED.set(paused);
        let timer = TimerWorker::start(
            ENGINE.get().expect("engine initialized").clone(),
            PAUSED.get().expect("paused initialized").clone(),
        );
        let _ = TIMER.set(timer.handle());
        let handle = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0) }
            .context("failed to install low-level keyboard hook")?;
        Ok(Self {
            handle,
            _timer: timer,
        })
    }

    pub fn update_deadline(&self, deadline_ms: Option<u64>) {
        if let Some(timer) = TIMER.get() {
            timer.update_deadline(deadline_ms);
        }
    }
}

impl Drop for KeyboardHook {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWindowsHookEx(self.handle);
        }
    }
}

#[derive(Clone)]
struct TimerHandle {
    state: Arc<(Mutex<TimerState>, Condvar)>,
}

impl TimerHandle {
    fn update_deadline(&self, deadline_ms: Option<u64>) {
        let (state, condvar) = &*self.state;
        let mut state = state.lock().expect("timer mutex poisoned");
        state.deadline_ms = deadline_ms;
        condvar.notify_one();
    }
}

struct TimerState {
    deadline_ms: Option<u64>,
    stop: bool,
}

struct TimerWorker {
    handle: TimerHandle,
    thread: Option<JoinHandle<()>>,
}

impl TimerWorker {
    fn start(engine: Arc<Mutex<Engine>>, paused: Arc<AtomicBool>) -> Self {
        let state = Arc::new((
            Mutex::new(TimerState {
                deadline_ms: None,
                stop: false,
            }),
            Condvar::new(),
        ));
        let handle = TimerHandle {
            state: state.clone(),
        };
        let thread = thread::spawn(move || timer_loop(state, engine, paused));
        Self {
            handle,
            thread: Some(thread),
        }
    }

    fn handle(&self) -> TimerHandle {
        self.handle.clone()
    }
}

impl Drop for TimerWorker {
    fn drop(&mut self) {
        let (state, condvar) = &*self.handle.state;
        {
            let mut state = state.lock().expect("timer mutex poisoned");
            state.stop = true;
            condvar.notify_one();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn timer_loop(
    state: Arc<(Mutex<TimerState>, Condvar)>,
    engine: Arc<Mutex<Engine>>,
    paused: Arc<AtomicBool>,
) {
    loop {
        let (lock, condvar) = &*state;
        let mut timer = lock.lock().expect("timer mutex poisoned");
        while !timer.stop {
            let Some(deadline_ms) = timer.deadline_ms else {
                timer = condvar.wait(timer).expect("timer mutex poisoned");
                continue;
            };
            let now_ms = current_time_ms();
            if deadline_ms <= now_ms {
                break;
            }
            let wait_ms = deadline_ms - now_ms;
            let (next_timer, _) = condvar
                .wait_timeout(timer, Duration::from_millis(wait_ms))
                .expect("timer mutex poisoned");
            timer = next_timer;
        }

        if timer.stop {
            break;
        }
        timer.deadline_ms = None;
        drop(timer);

        if paused.load(Ordering::Relaxed) {
            continue;
        }

        let plan = match engine.lock() {
            Ok(mut engine) => engine.handle_time(current_time_ms()),
            Err(_) => continue,
        };
        if let Err(error) = send_output(&plan.events) {
            if !OUTPUT_ERROR_LOGGED.swap(true, Ordering::Relaxed) {
                ::log::error!("KeyZen output error: {error:#}");
            }
        }
        let mut timer = lock.lock().expect("timer mutex poisoned");
        timer.deadline_ms = plan.next_deadline_ms;
        condvar.notify_one();
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
        Ok(mut engine) => engine.handle_event_at(EngineEvent { key, kind }, current_time_ms()),
        Err(_) => return unsafe { CallNextHookEx(None, code, wparam, lparam) },
    };

    if let Err(error) = send_output(&plan.events) {
        if !OUTPUT_ERROR_LOGGED.swap(true, Ordering::Relaxed) {
            ::log::error!("KeyZen output error: {error:#}");
        }
    }
    if let Some(timer) = TIMER.get() {
        timer.update_deadline(plan.next_deadline_ms);
    }

    if plan.consume_input {
        LRESULT(1)
    } else {
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }
}

fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
