use crate::app_settings::{AppPaths, AppSettings, AppSettingsError, save_at};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

pub const TASK_FOLDER_PATH: &str = "\\KeyZen";
pub const AUTORUN_TASK_PREFIX: &str = "Autorun for ";
const TASK_TRIGGER_DELAY: &str = "PT03S";
const SDDL_FULL_ACCESS_FOR_EVERYONE: &str = "D:(A;;FA;;;WD)";

pub trait StartupRegistration {
    fn set_enabled(&self, enabled: bool, tray_exe: &Path) -> Result<(), StartupError>;
}

#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    #[error("Task Scheduler integration is only supported on Windows")]
    UnsupportedPlatform,
    #[error("Task Scheduler error: {0}")]
    Scheduler(String),
    #[error(transparent)]
    Settings(#[from] AppSettingsError),
}

pub fn update_start_at_login<S: StartupRegistration>(
    scheduler: &S,
    paths: &AppPaths,
    settings: &mut AppSettings,
    enabled: bool,
    tray_exe: &Path,
) -> Result<(), StartupError> {
    let previous = settings.start_at_login;
    scheduler.set_enabled(enabled, tray_exe)?;
    settings.start_at_login = enabled;

    if let Err(error) = save_at(paths, settings) {
        settings.start_at_login = previous;
        let _ = scheduler.set_enabled(previous, tray_exe);
        return Err(error.into());
    }

    Ok(())
}

#[cfg(windows)]
#[derive(Debug, Default, Clone, Copy)]
pub struct TaskSchedulerStartup;

#[cfg(windows)]
impl StartupRegistration for TaskSchedulerStartup {
    fn set_enabled(&self, enabled: bool, tray_exe: &Path) -> Result<(), StartupError> {
        task_scheduler_set_enabled(enabled, tray_exe)
    }
}

#[cfg(not(windows))]
#[derive(Debug, Default, Clone, Copy)]
pub struct TaskSchedulerStartup;

#[cfg(not(windows))]
impl StartupRegistration for TaskSchedulerStartup {
    fn set_enabled(&self, _enabled: bool, _tray_exe: &Path) -> Result<(), StartupError> {
        Err(StartupError::UnsupportedPlatform)
    }
}

#[cfg(windows)]
fn task_scheduler_set_enabled(enabled: bool, tray_exe: &Path) -> Result<(), StartupError> {
    use windows::Win32::Foundation::{VARIANT_FALSE, VARIANT_TRUE};
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize,
    };
    use windows::Win32::System::TaskScheduler::{
        IExecAction, ILogonTrigger, ITaskService, TASK_ACTION_EXEC, TASK_CREATE_OR_UPDATE,
        TASK_INSTANCES_IGNORE_NEW, TASK_LOGON_INTERACTIVE_TOKEN, TASK_RUNLEVEL_LUA,
        TASK_TRIGGER_LOGON, TaskScheduler,
    };
    use windows::Win32::System::Variant::VARIANT;
    use windows::core::{BSTR, Interface};

    struct ComGuard;
    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }

    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .map_err(|error| StartupError::Scheduler(error.to_string()))?;
        let _guard = ComGuard;

        let service: ITaskService = CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)
            .map_err(|error| StartupError::Scheduler(error.to_string()))?;
        service
            .Connect(
                &VARIANT::default(),
                &VARIANT::default(),
                &VARIANT::default(),
                &VARIANT::default(),
            )
            .map_err(|error| StartupError::Scheduler(error.to_string()))?;
        let root = service
            .GetFolder(&BSTR::from("\\"))
            .map_err(|error| StartupError::Scheduler(error.to_string()))?;
        let user = current_user()?;
        let task_name = autorun_task_name(&user.name);

        if enabled {
            let folder = get_or_create_task_folder(&service, &root)?;
            delete_task_if_present(&folder, &task_name)?;

            let task = service
                .NewTask(0)
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            let command = task_scheduler_path(tray_exe)?;

            let registration = task
                .RegistrationInfo()
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            registration
                .SetDescription(&BSTR::from("Start KeyZen tray at user logon."))
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            registration
                .SetAuthor(&BSTR::from(user.domain_name.as_str()))
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;

            let principal = task
                .Principal()
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            principal
                .SetId(&BSTR::from("Principal1"))
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            principal
                .SetUserId(&BSTR::from(user.domain_name.as_str()))
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            principal
                .SetLogonType(TASK_LOGON_INTERACTIVE_TOKEN)
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            principal
                .SetRunLevel(TASK_RUNLEVEL_LUA)
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;

            let settings = task
                .Settings()
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            settings
                .SetMultipleInstances(TASK_INSTANCES_IGNORE_NEW)
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            settings
                .SetStartWhenAvailable(VARIANT_FALSE)
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            settings
                .SetExecutionTimeLimit(&BSTR::from("PT0S"))
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            settings
                .SetDisallowStartIfOnBatteries(VARIANT_FALSE)
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            settings
                .SetStopIfGoingOnBatteries(VARIANT_FALSE)
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            settings
                .SetAllowDemandStart(VARIANT_TRUE)
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            settings
                .SetRunOnlyIfNetworkAvailable(VARIANT_FALSE)
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            settings
                .SetRunOnlyIfIdle(VARIANT_FALSE)
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            settings
                .SetWakeToRun(VARIANT_FALSE)
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            settings
                .SetHidden(VARIANT_FALSE)
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            settings
                .SetEnabled(VARIANT_TRUE)
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            settings
                .SetPriority(4)
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;

            let triggers = task
                .Triggers()
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            let logon_trigger: ILogonTrigger = triggers
                .Create(TASK_TRIGGER_LOGON)
                .and_then(|trigger| trigger.cast())
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            logon_trigger
                .SetId(&BSTR::from("Trigger1"))
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            logon_trigger
                .SetDelay(&BSTR::from(TASK_TRIGGER_DELAY))
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            logon_trigger
                .SetUserId(&BSTR::from(user.domain_name.as_str()))
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;

            let actions = task
                .Actions()
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            let exec_action: IExecAction = actions
                .Create(TASK_ACTION_EXEC)
                .and_then(|action| action.cast())
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            exec_action
                .SetPath(&BSTR::from(command.as_str()))
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            if let Some(parent) = command_parent(&command) {
                exec_action
                    .SetWorkingDirectory(&BSTR::from(parent.as_str()))
                    .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            }

            let user_variant = VARIANT::from(BSTR::from(user.domain_name.as_str()));
            let sddl_variant = VARIANT::from(BSTR::from(SDDL_FULL_ACCESS_FOR_EVERYONE));
            folder
                .RegisterTaskDefinition(
                    &BSTR::from(task_name.as_str()),
                    &task,
                    TASK_CREATE_OR_UPDATE.0,
                    &user_variant,
                    &VARIANT::default(),
                    TASK_LOGON_INTERACTIVE_TOKEN,
                    &sddl_variant,
                )
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
        } else {
            if let Ok(folder) = service.GetFolder(&BSTR::from(TASK_FOLDER_PATH)) {
                delete_task_if_present(&folder, &task_name)?;
            }
        }
    }

    Ok(())
}

#[cfg(windows)]
fn get_or_create_task_folder(
    service: &windows::Win32::System::TaskScheduler::ITaskService,
    root: &windows::Win32::System::TaskScheduler::ITaskFolder,
) -> Result<windows::Win32::System::TaskScheduler::ITaskFolder, StartupError> {
    use windows::Win32::System::Variant::VARIANT;
    use windows::core::BSTR;

    unsafe {
        match service.GetFolder(&BSTR::from(TASK_FOLDER_PATH)) {
            Ok(folder) => Ok(folder),
            Err(_) => root
                .CreateFolder(&BSTR::from(TASK_FOLDER_PATH), &VARIANT::default())
                .map_err(|error| StartupError::Scheduler(error.to_string())),
        }
    }
}

#[cfg(windows)]
fn delete_task_if_present(
    folder: &windows::Win32::System::TaskScheduler::ITaskFolder,
    task_name: &str,
) -> Result<(), StartupError> {
    use windows::core::BSTR;

    unsafe {
        match folder.DeleteTask(&BSTR::from(task_name), 0) {
            Ok(()) => Ok(()),
            Err(error) if is_missing_task_error(error.code().0 as u32) => Ok(()),
            Err(error) => Err(StartupError::Scheduler(error.to_string())),
        }
    }
}

fn autorun_task_name(username: &str) -> String {
    format!("{AUTORUN_TASK_PREFIX}{username}")
}

#[cfg(windows)]
fn is_missing_task_error(code: u32) -> bool {
    matches!(code, 0x8007_0002 | 0x8007_0003 | 0x8004_130F)
}

#[cfg(windows)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct CurrentUser {
    name: String,
    domain_name: String,
}

#[cfg(windows)]
fn current_user() -> Result<CurrentUser, StartupError> {
    let name = std::env::var("USERNAME").map_err(|_| {
        StartupError::Scheduler("USERNAME environment variable is not set".to_string())
    })?;
    let domain = std::env::var("USERDOMAIN").map_err(|_| {
        StartupError::Scheduler("USERDOMAIN environment variable is not set".to_string())
    })?;

    Ok(CurrentUser {
        domain_name: format!("{domain}\\{name}"),
        name,
    })
}

#[cfg(windows)]
fn task_scheduler_path(path: &Path) -> Result<String, StartupError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| StartupError::Scheduler(error.to_string()))?
            .join(path)
    };

    if !absolute.exists() {
        return Err(StartupError::Scheduler(format!(
            "tray executable was not found at {}",
            absolute.display()
        )));
    }

    Ok(strip_windows_verbatim_prefix(&absolute.to_string_lossy()).into_owned())
}

#[cfg(windows)]
fn command_parent(command: &str) -> Option<String> {
    PathBuf::from(command)
        .parent()
        .map(|parent| parent.to_string_lossy().into_owned())
}

#[cfg(windows)]
fn strip_windows_verbatim_prefix(path: &str) -> Cow<'_, str> {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        Cow::Owned(format!(r"\\{rest}"))
    } else if let Some(rest) = path.strip_prefix(r"\\?\") {
        Cow::Borrowed(rest)
    } else {
        Cow::Borrowed(path)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AUTORUN_TASK_PREFIX, StartupError, StartupRegistration, TASK_FOLDER_PATH,
        autorun_task_name, update_start_at_login,
    };
    use crate::app_settings::{AppPaths, AppSettings, load_at, save_at};
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Default)]
    struct RecordingScheduler {
        calls: RefCell<Vec<bool>>,
        fail: bool,
    }

    impl StartupRegistration for RecordingScheduler {
        fn set_enabled(&self, enabled: bool, _tray_exe: &Path) -> Result<(), StartupError> {
            self.calls.borrow_mut().push(enabled);
            if self.fail {
                Err(StartupError::Scheduler("failed".to_string()))
            } else {
                Ok(())
            }
        }
    }

    fn temp_paths(name: &str) -> AppPaths {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        AppPaths::under(std::env::temp_dir().join(format!("keyzen-startup-{name}-{nonce}")))
    }

    #[test]
    fn saves_setting_after_scheduler_succeeds() {
        let paths = temp_paths("success");
        let mut settings = AppSettings::with_default_key_config(PathBuf::from("keyzen.yaml"));
        save_at(&paths, &settings).unwrap();
        let scheduler = RecordingScheduler::default();

        update_start_at_login(
            &scheduler,
            &paths,
            &mut settings,
            true,
            Path::new("keyzen-tray.exe"),
        )
        .unwrap();

        assert!(settings.start_at_login);
        assert!(load_at(&paths).unwrap().start_at_login);
        assert_eq!(*scheduler.calls.borrow(), vec![true]);
    }

    #[test]
    fn does_not_save_setting_when_scheduler_fails() {
        let paths = temp_paths("failure");
        let mut settings = AppSettings::with_default_key_config(PathBuf::from("keyzen.yaml"));
        save_at(&paths, &settings).unwrap();
        let scheduler = RecordingScheduler {
            calls: RefCell::new(Vec::new()),
            fail: true,
        };

        assert!(
            update_start_at_login(
                &scheduler,
                &paths,
                &mut settings,
                true,
                Path::new("keyzen-tray.exe"),
            )
            .is_err()
        );

        assert!(!settings.start_at_login);
        assert!(!load_at(&paths).unwrap().start_at_login);
    }

    #[test]
    fn uses_dedicated_startup_folder_and_user_task_name() {
        assert_eq!(TASK_FOLDER_PATH, "\\KeyZen");
        assert_eq!(AUTORUN_TASK_PREFIX, "Autorun for ");
        assert_eq!(autorun_task_name("imcha"), "Autorun for imcha");
    }

    #[cfg(windows)]
    #[test]
    fn strips_verbatim_paths_before_task_scheduler_registration() {
        assert_eq!(
            super::strip_windows_verbatim_prefix(r"\\?\C:\KeyZen\keyzen-tray.exe"),
            r"C:\KeyZen\keyzen-tray.exe"
        );
        assert_eq!(
            super::strip_windows_verbatim_prefix(r"\\?\UNC\server\share\keyzen-tray.exe"),
            r"\\server\share\keyzen-tray.exe"
        );
    }
}
