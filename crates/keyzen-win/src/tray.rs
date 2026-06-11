pub const MENU_LABELS: [&str; 4] = ["Pause", "Start at login", "Choose key config...", "Quit"];

const ID_PAUSE: usize = 1001;
const ID_START_AT_LOGIN: usize = 1002;
const ID_CHOOSE_KEY_CONFIG: usize = 1003;
const ID_QUIT: usize = 1004;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayMenuCommand {
    Pause,
    StartAtLogin,
    ChooseKeyConfig,
    Quit,
}

pub fn command_from_menu_id(id: usize) -> Option<TrayMenuCommand> {
    match id {
        ID_PAUSE => Some(TrayMenuCommand::Pause),
        ID_START_AT_LOGIN => Some(TrayMenuCommand::StartAtLogin),
        ID_CHOOSE_KEY_CONFIG => Some(TrayMenuCommand::ChooseKeyConfig),
        ID_QUIT => Some(TrayMenuCommand::Quit),
        _ => None,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TrayError {
    #[error("KeyZen tray is only supported on Windows")]
    UnsupportedPlatform,
    #[error("another KeyZen remapper instance is already running")]
    AlreadyRunning,
    #[error("{0}")]
    Operation(String),
}

#[cfg(not(windows))]
pub fn run_tray_app() -> Result<(), TrayError> {
    Err(TrayError::UnsupportedPlatform)
}

#[cfg(windows)]
mod platform {
    use super::{
        ID_CHOOSE_KEY_CONFIG, ID_PAUSE, ID_QUIT, ID_START_AT_LOGIN, TrayError, TrayMenuCommand,
        command_from_menu_id,
    };
    use crate::RuntimeCommand;
    use crate::app_settings::{self, AppPaths, AppSettings};
    use crate::single_instance::{SingleInstance, SingleInstanceError};
    use crate::startup::{StartupRegistration, TaskSchedulerStartup, update_start_at_login};
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, OnceLock, mpsc};
    use std::thread;
    use windows::Win32::Foundation::{ERROR_CANCELLED, HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoTaskMemFree, CoUninitialize,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{
        FileOpenDialog, IFileOpenDialog, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP,
        NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NOTIFYICON_VERSION_4,
        NOTIFYICONDATAW, SIGDN_FILESYSPATH, Shell_NotifyIconW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
        DispatchMessageW, GetCursorPos, GetMessageW, IDI_APPLICATION, LoadIconW, MF_CHECKED,
        MF_SEPARATOR, MF_STRING, MF_UNCHECKED, MSG, PostQuitMessage, RegisterClassW,
        RegisterWindowMessageW, SetForegroundWindow, TPM_RIGHTBUTTON, TrackPopupMenu,
        TranslateMessage, WINDOW_EX_STYLE, WM_APP, WM_COMMAND, WM_CONTEXTMENU, WM_DESTROY,
        WM_RBUTTONUP, WM_WINDOWPOSCHANGING, WNDCLASSW, WS_OVERLAPPEDWINDOW, WS_POPUP,
    };
    use windows::core::{Error as WindowsError, HRESULT, PCWSTR, w};

    const WM_TRAY_ICON: u32 = WM_APP + 1;
    const TRAY_ICON_ID: u32 = 1;

    static STATE: OnceLock<Mutex<TrayState>> = OnceLock::new();
    static TASKBAR_CREATED: OnceLock<u32> = OnceLock::new();
    static TRAY_ICON_CREATED: AtomicBool = AtomicBool::new(false);

    struct TrayState {
        paths: AppPaths,
        settings: AppSettings,
        runtime_tx: mpsc::Sender<RuntimeCommand>,
        paused: bool,
        has_config: bool,
        tray_exe: PathBuf,
    }

    struct ComGuard;

    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }

    pub fn run_tray_app() -> Result<(), TrayError> {
        let _instance = match SingleInstance::acquire() {
            Ok(instance) => instance,
            Err(SingleInstanceError::AlreadyRunning) => return Ok(()),
            Err(error) => return Err(TrayError::Operation(error.to_string())),
        };

        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                .ok()
                .map_err(|error| operation_error(format!("CoInitializeEx failed: {error}")))?;
        }
        let _com = ComGuard;

        let (paths, settings) = app_settings::load_or_bootstrap()
            .map_err(|error| operation_error(error.to_string()))?;
        let (config, paused, startup_notice) =
            match app_settings::load_key_config(&paths.settings, &settings) {
                Ok(config) => (Some(config), false, None),
                Err(error) => (
                    None,
                    true,
                    Some(format!(
                        "Key config is invalid or missing. Choose a key config file.\n{error}"
                    )),
                ),
            };

        let tray_exe = std::env::current_exe()
            .map_err(|error| operation_error(format!("current_exe failed: {error}")))?;
        let scheduler_notice = TaskSchedulerStartup
            .set_enabled(settings.start_at_login, &tray_exe)
            .err()
            .map(|error| format!("Could not reconcile Start at login.\n{error}"));

        let (runtime_tx, runtime_rx) = mpsc::channel();
        STATE
            .set(Mutex::new(TrayState {
                paths,
                settings,
                runtime_tx,
                paused,
                has_config: !paused,
                tray_exe,
            }))
            .map_err(|_| operation_error("tray state was already initialized"))?;

        let hwnd = create_hidden_window()?;
        let _ = try_add_tray_icon(hwnd);
        let runtime = thread::spawn(move || crate::run_controlled(config, paused, runtime_rx));

        if let Some(message) = startup_notice.or(scheduler_notice) {
            show_notification(hwnd, &message);
        }

        let mut message = MSG::default();
        let mut message_error = None;
        loop {
            let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
            if result.0 == -1 {
                message_error = Some(operation_error(WindowsError::from_win32()));
                break;
            }
            if result.0 == 0 {
                break;
            }
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }

        delete_tray_icon(hwnd);
        if let Some(state) = STATE.get()
            && let Ok(state) = state.lock()
        {
            let _ = state.runtime_tx.send(RuntimeCommand::Stop);
        }
        if let Some(error) = message_error {
            let _ = runtime.join();
            return Err(error);
        }
        match runtime.join() {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(operation_error(error)),
            Err(_) => Err(operation_error("KeyZen runtime thread panicked")),
        }
    }

    fn create_hidden_window() -> Result<HWND, TrayError> {
        let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
        TASKBAR_CREATED
            .set(taskbar_created)
            .map_err(|_| operation_error("taskbar message was already registered"))?;

        let module = unsafe { GetModuleHandleW(None) }
            .map_err(|error| operation_error(format!("GetModuleHandleW failed: {error}")))?;
        let class_name = w!("KeyZenTrayWindow");
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: module.into(),
            lpszClassName: class_name,
            ..Default::default()
        };
        if unsafe { RegisterClassW(&class) } == 0 {
            return Err(operation_error(format!(
                "RegisterClassW failed: {}",
                WindowsError::from_win32()
            )));
        }

        unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class_name,
                w!("KeyZen"),
                WS_OVERLAPPEDWINDOW | WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(module.into()),
                None,
            )
            .map_err(|error| operation_error(format!("CreateWindowExW failed: {error}")))
        }
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if TASKBAR_CREATED.get().copied() == Some(message) {
            let _ = try_add_tray_icon(hwnd);
            return LRESULT(0);
        }

        match message {
            WM_WINDOWPOSCHANGING => {
                if !TRAY_ICON_CREATED.load(Ordering::SeqCst) {
                    let _ = try_add_tray_icon(hwnd);
                }
                LRESULT(0)
            }
            WM_TRAY_ICON
                if lparam.0 as u32 & 0xffff == WM_RBUTTONUP
                    || lparam.0 as u32 & 0xffff == WM_CONTEXTMENU =>
            {
                let _ = show_context_menu(hwnd);
                LRESULT(0)
            }
            WM_COMMAND => {
                if let Some(command) = command_from_menu_id(wparam.0 & 0xffff) {
                    handle_command(hwnd, command);
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                unsafe { PostQuitMessage(0) };
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        }
    }

    fn show_context_menu(hwnd: HWND) -> Result<(), TrayError> {
        let (paused, start_at_login) = STATE
            .get()
            .and_then(|state| state.lock().ok())
            .map(|state| (state.paused, state.settings.start_at_login))
            .unwrap_or((true, false));
        let menu = unsafe { CreatePopupMenu() }
            .map_err(|error| operation_error(format!("CreatePopupMenu failed: {error}")))?;

        unsafe {
            AppendMenuW(
                menu,
                MF_STRING | checked_flag(paused),
                ID_PAUSE,
                w!("Pause"),
            )
            .map_err(operation_error)?;
            AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null()).map_err(operation_error)?;
            AppendMenuW(
                menu,
                MF_STRING | checked_flag(start_at_login),
                ID_START_AT_LOGIN,
                w!("Start at login"),
            )
            .map_err(operation_error)?;
            AppendMenuW(
                menu,
                MF_STRING,
                ID_CHOOSE_KEY_CONFIG,
                w!("Choose key config..."),
            )
            .map_err(operation_error)?;
            AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null()).map_err(operation_error)?;
            AppendMenuW(menu, MF_STRING, ID_QUIT, w!("Quit")).map_err(operation_error)?;

            let mut cursor = POINT::default();
            let _ = GetCursorPos(&mut cursor);
            let _ = SetForegroundWindow(hwnd);
            let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON, cursor.x, cursor.y, None, hwnd, None);
            DestroyMenu(menu).map_err(operation_error)?;
        }

        Ok(())
    }

    fn checked_flag(checked: bool) -> windows::Win32::UI::WindowsAndMessaging::MENU_ITEM_FLAGS {
        if checked { MF_CHECKED } else { MF_UNCHECKED }
    }

    fn handle_command(hwnd: HWND, command: TrayMenuCommand) {
        match command {
            TrayMenuCommand::Pause => toggle_pause(hwnd),
            TrayMenuCommand::StartAtLogin => toggle_start_at_login(hwnd),
            TrayMenuCommand::ChooseKeyConfig => choose_key_config(hwnd),
            TrayMenuCommand::Quit => unsafe {
                let _ = DestroyWindow(hwnd);
            },
        }
    }

    fn toggle_pause(hwnd: HWND) {
        let Some(state) = STATE.get() else {
            return;
        };
        let Ok(mut state) = state.lock() else {
            return;
        };
        if state.paused && !state.has_config {
            show_notification(
                hwnd,
                "No valid key config is loaded. Choose a key config file first.",
            );
            return;
        }

        let paused = !state.paused;
        if state
            .runtime_tx
            .send(RuntimeCommand::Pause(paused))
            .is_err()
        {
            show_notification(hwnd, "The KeyZen remapping runtime is unavailable.");
            return;
        }
        state.paused = paused;
        update_tooltip(hwnd, paused);
    }

    fn toggle_start_at_login(hwnd: HWND) {
        let Some(state) = STATE.get() else {
            return;
        };
        let Ok(mut state) = state.lock() else {
            return;
        };
        let enabled = !state.settings.start_at_login;
        let paths = state.paths.clone();
        let tray_exe = state.tray_exe.clone();
        if let Err(error) = update_start_at_login(
            &TaskSchedulerStartup,
            &paths,
            &mut state.settings,
            enabled,
            &tray_exe,
        ) {
            show_notification(hwnd, &format!("Could not update Start at login.\n{error}"));
        }
    }

    fn choose_key_config(hwnd: HWND) {
        let path = match pick_key_config(hwnd) {
            Ok(Some(path)) => path,
            Ok(None) => return,
            Err(error) => {
                show_notification(hwnd, &format!("Could not open the file picker.\n{error}"));
                return;
            }
        };
        let config = match app_settings::validate_key_config(&path) {
            Ok(config) => config,
            Err(error) => {
                show_notification(
                    hwnd,
                    &format!("The selected key config is invalid.\n{error}"),
                );
                return;
            }
        };

        let Some(state) = STATE.get() else {
            return;
        };
        let Ok(mut state) = state.lock() else {
            return;
        };
        if state
            .runtime_tx
            .send(RuntimeCommand::ReplaceConfig(config))
            .is_err()
        {
            show_notification(hwnd, "The KeyZen remapping runtime is unavailable.");
            return;
        }
        state.has_config = true;
        state.settings.key_config_path = path;
        if let Err(error) = app_settings::save_at(&state.paths, &state.settings) {
            show_notification(
                hwnd,
                &format!("The key config was loaded but the app setting was not saved.\n{error}"),
            );
        }
    }

    fn pick_key_config(hwnd: HWND) -> windows::core::Result<Option<PathBuf>> {
        unsafe {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            let filters = [
                COMDLG_FILTERSPEC {
                    pszName: w!("YAML files"),
                    pszSpec: w!("*.yaml;*.yml"),
                },
                COMDLG_FILTERSPEC {
                    pszName: w!("All files"),
                    pszSpec: w!("*.*"),
                },
            ];
            dialog.SetFileTypes(&filters)?;
            dialog.SetTitle(w!("Choose KeyZen key config"))?;
            if let Err(error) = dialog.Show(Some(hwnd)) {
                if error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) {
                    return Ok(None);
                }
                return Err(error);
            }

            let item = dialog.GetResult()?;
            let raw_path = item.GetDisplayName(SIGDN_FILESYSPATH)?;
            let path = raw_path.to_string();
            CoTaskMemFree(Some(raw_path.0.cast::<c_void>()));
            Ok(Some(PathBuf::from(path?)))
        }
    }

    fn try_add_tray_icon(hwnd: HWND) -> bool {
        if TRAY_ICON_CREATED.load(Ordering::SeqCst) {
            return true;
        }

        let paused = STATE
            .get()
            .and_then(|state| state.lock().ok())
            .map(|state| state.paused)
            .unwrap_or(true);
        let Ok(icon) = (unsafe { LoadIconW(None, IDI_APPLICATION) }) else {
            return false;
        };
        let mut data = base_notify_data(hwnd);
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
        data.uCallbackMessage = WM_TRAY_ICON;
        data.hIcon = icon;
        fill_wide(&mut data.szTip, tooltip(paused));

        if !unsafe { Shell_NotifyIconW(NIM_ADD, &data) }.as_bool() {
            TRAY_ICON_CREATED.store(false, Ordering::SeqCst);
            return false;
        }
        TRAY_ICON_CREATED.store(true, Ordering::SeqCst);
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        let _ = unsafe { Shell_NotifyIconW(NIM_SETVERSION, &data) };
        true
    }

    fn delete_tray_icon(hwnd: HWND) {
        if !TRAY_ICON_CREATED.swap(false, Ordering::SeqCst) {
            return;
        }
        let data = base_notify_data(hwnd);
        let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
    }

    fn update_tooltip(hwnd: HWND, paused: bool) {
        let mut data = base_notify_data(hwnd);
        data.uFlags = NIF_TIP | NIF_SHOWTIP;
        fill_wide(&mut data.szTip, tooltip(paused));
        let _ = unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
    }

    fn show_notification(hwnd: HWND, message: &str) {
        let mut data = base_notify_data(hwnd);
        data.uFlags = NIF_INFO;
        data.dwInfoFlags = NIIF_INFO;
        fill_wide(&mut data.szInfoTitle, "KeyZen");
        fill_wide(&mut data.szInfo, message);
        let _ = unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
    }

    fn base_notify_data(hwnd: HWND) -> NOTIFYICONDATAW {
        NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: TRAY_ICON_ID,
            ..Default::default()
        }
    }

    fn tooltip(paused: bool) -> &'static str {
        if paused { "KeyZen (Paused)" } else { "KeyZen" }
    }

    fn fill_wide<const N: usize>(target: &mut [u16; N], value: &str) {
        target.fill(0);
        for (slot, code_unit) in target
            .iter_mut()
            .take(N.saturating_sub(1))
            .zip(value.encode_utf16())
        {
            *slot = code_unit;
        }
    }

    fn operation_error(error: impl ToString) -> TrayError {
        TrayError::Operation(error.to_string())
    }
}

#[cfg(windows)]
pub use platform::run_tray_app;

#[cfg(test)]
mod tests {
    use super::{MENU_LABELS, TrayMenuCommand, command_from_menu_id};

    #[test]
    fn maps_command_ids_to_english_menu_events() {
        assert_eq!(
            MENU_LABELS,
            ["Pause", "Start at login", "Choose key config...", "Quit"]
        );
        assert_eq!(command_from_menu_id(1001), Some(TrayMenuCommand::Pause));
        assert_eq!(
            command_from_menu_id(1002),
            Some(TrayMenuCommand::StartAtLogin)
        );
        assert_eq!(
            command_from_menu_id(1003),
            Some(TrayMenuCommand::ChooseKeyConfig)
        );
        assert_eq!(command_from_menu_id(1004), Some(TrayMenuCommand::Quit));
        assert_eq!(command_from_menu_id(9999), None);
    }
}
