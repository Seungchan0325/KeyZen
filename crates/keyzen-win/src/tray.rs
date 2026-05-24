use std::{
    cell::RefCell,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::{Context, Result};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Shell::{
                NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
                Shell_NotifyIconW,
            },
            WindowsAndMessaging::*,
        },
    },
    core::{BOOL, Error, HSTRING, PCWSTR, w},
};

use crate::app::{AppCommand, AppStatus};

const WM_TRAYICON: u32 = WM_APP + 1;
const ID_PAUSE: usize = 1001;
const ID_RELOAD: usize = 1002;
const ID_OPEN_CONFIG: usize = 1003;
const ID_STARTUP: usize = 1004;
const ID_EXIT: usize = 1005;

static SHOULD_EXIT: AtomicBool = AtomicBool::new(false);

thread_local! {
    static HANDLER: RefCell<Option<Box<dyn FnMut(AppCommand) -> Result<AppStatus>>>> = const { RefCell::new(None) };
    static STATUS: RefCell<AppStatus> = const { RefCell::new(AppStatus::Running) };
}

pub fn run_message_loop<F>(handler: F, initial_status: AppStatus) -> Result<()>
where
    F: FnMut(AppCommand) -> Result<AppStatus> + 'static,
{
    HANDLER.with(|slot| *slot.borrow_mut() = Some(Box::new(handler)));
    STATUS.with(|slot| *slot.borrow_mut() = initial_status);

    let hwnd = create_message_window()?;
    add_tray_icon(hwnd, initial_status)?;

    let mut msg = MSG::default();
    while !SHOULD_EXIT.load(Ordering::Relaxed) {
        let result = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if result.0 == -1 {
            break;
        }
        if result.0 == 0 {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    delete_tray_icon(hwnd);
    Ok(())
}

pub fn request_exit() {
    SHOULD_EXIT.store(true, Ordering::Relaxed);
    unsafe {
        PostQuitMessage(0);
    }
}

fn create_message_window() -> Result<HWND> {
    let instance = unsafe { GetModuleHandleW(None) }.context("failed to get module handle")?;
    let class_name = w!("KeyZenTrayWindow");
    let window_class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: instance.into(),
        lpszClassName: class_name,
        ..Default::default()
    };
    unsafe { RegisterClassW(&window_class) };
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!("KeyZen"),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance.into()),
            None,
        )
    }
    .context("failed to create message window")?;
    Ok(hwnd)
}

fn add_tray_icon(hwnd: HWND, status: AppStatus) -> Result<()> {
    let nid = notify_data(hwnd, status);
    bool_ok(
        unsafe { Shell_NotifyIconW(NIM_ADD, &nid) },
        "failed to add tray icon",
    )?;
    Ok(())
}

fn delete_tray_icon(hwnd: HWND) {
    let nid = notify_data(hwnd, AppStatus::Running);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

fn notify_data(hwnd: HWND, status: AppStatus) -> NOTIFYICONDATAW {
    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_TRAYICON,
        hIcon: load_default_icon(),
        ..Default::default()
    };
    let tip = match status {
        AppStatus::Running => "KeyZen - Running",
        AppStatus::Paused => "KeyZen - Paused",
        AppStatus::ConfigError => "KeyZen - Config Error",
    };
    write_wide_tip(&mut nid.szTip, tip);
    nid
}

fn load_default_icon() -> HICON {
    unsafe { LoadIconW(None, IDI_APPLICATION).unwrap_or_default() }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let id = wparam.0 & 0xffff;
            let command = match id {
                ID_PAUSE => Some(AppCommand::TogglePause),
                ID_RELOAD => Some(AppCommand::ReloadConfig),
                ID_OPEN_CONFIG => Some(AppCommand::OpenConfigFolder),
                ID_STARTUP => Some(AppCommand::ToggleStartAtLogin),
                ID_EXIT => Some(AppCommand::Exit),
                _ => None,
            };
            if let Some(command) = command {
                HANDLER.with(|slot| {
                    let mut borrowed = slot.borrow_mut();
                    let Some(handler) = borrowed.as_mut() else {
                        return;
                    };
                    match handler(command) {
                        Ok(status) => {
                            STATUS.with(|slot| *slot.borrow_mut() = status);
                            let nid = notify_data(hwnd, status);
                            unsafe {
                                let _ =
                                    Shell_NotifyIconW(windows::Win32::UI::Shell::NIM_MODIFY, &nid);
                            }
                        }
                        Err(error) => eprintln!("KeyZen command failed: {error:#}"),
                    }
                });
            }
            LRESULT(0)
        }
        WM_TRAYICON if lparam.0 as u32 == WM_RBUTTONUP || lparam.0 as u32 == WM_LBUTTONUP => {
            show_menu(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            request_exit();
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn show_menu(hwnd: HWND) {
    unsafe {
        let menu = CreatePopupMenu().unwrap_or_default();
        let status = STATUS.with(|slot| *slot.borrow());
        let pause_text = match status {
            AppStatus::Paused => "Resume",
            _ => "Pause",
        };
        append_menu(menu, ID_PAUSE, pause_text);
        append_menu(menu, ID_RELOAD, "Reload Config");
        append_menu(menu, ID_OPEN_CONFIG, "Open Config Folder");
        append_menu(menu, ID_STARTUP, "Toggle Start at Login");
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        append_menu(menu, ID_EXIT, "Exit");

        let mut point = POINT::default();
        let _ = GetCursorPos(&mut point);
        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenu(
            menu,
            TPM_BOTTOMALIGN | TPM_LEFTALIGN,
            point.x,
            point.y,
            None,
            hwnd,
            None,
        );
        let _ = DestroyMenu(menu);
    }
}

unsafe fn append_menu(menu: HMENU, id: usize, text: &str) {
    let text = HSTRING::from(text);
    let _ = unsafe { AppendMenuW(menu, MF_STRING, id, &text) };
}

fn write_wide_tip(target: &mut [u16], value: &str) {
    let wide = value.encode_utf16().collect::<Vec<_>>();
    for (slot, code) in target.iter_mut().zip(wide.into_iter().chain([0])) {
        *slot = code;
    }
}

fn bool_ok(value: BOOL, message: &'static str) -> Result<()> {
    if value.as_bool() {
        Ok(())
    } else {
        Err(Error::from_thread()).context(message)
    }
}
