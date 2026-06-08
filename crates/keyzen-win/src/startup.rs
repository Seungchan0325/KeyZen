use crate::app_settings::{AppPaths, AppSettings, AppSettingsError, save_at};
use std::path::Path;

pub const TASK_NAME: &str = "KeyZen";

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

        if enabled {
            let task = service
                .NewTask(0)
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            let user = current_user()?;
            let command = tray_exe
                .canonicalize()
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;

            let registration = task
                .RegistrationInfo()
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            registration
                .SetDescription(&BSTR::from("Start KeyZen tray at user logon."))
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            registration
                .SetAuthor(&BSTR::from("KeyZen"))
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;

            let principal = task
                .Principal()
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            principal
                .SetUserId(&BSTR::from(user.as_str()))
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
                .SetExecutionTimeLimit(&BSTR::from("PT0S"))
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            settings
                .SetDisallowStartIfOnBatteries(VARIANT_FALSE)
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            settings
                .SetStopIfGoingOnBatteries(VARIANT_FALSE)
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            settings
                .SetStartWhenAvailable(VARIANT_TRUE)
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
                .SetPriority(7)
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;

            let triggers = task
                .Triggers()
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            let logon_trigger: ILogonTrigger = triggers
                .Create(TASK_TRIGGER_LOGON)
                .and_then(|trigger| trigger.cast())
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            logon_trigger
                .SetUserId(&BSTR::from(user.as_str()))
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;

            let actions = task
                .Actions()
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            let exec_action: IExecAction = actions
                .Create(TASK_ACTION_EXEC)
                .and_then(|action| action.cast())
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            exec_action
                .SetPath(&BSTR::from(command.to_string_lossy().as_ref()))
                .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            if let Some(parent) = command.parent() {
                exec_action
                    .SetWorkingDirectory(&BSTR::from(parent.to_string_lossy().as_ref()))
                    .map_err(|error| StartupError::Scheduler(error.to_string()))?;
            }

            root.RegisterTaskDefinition(
                &BSTR::from(TASK_NAME),
                &task,
                TASK_CREATE_OR_UPDATE.0,
                &VARIANT::default(),
                &VARIANT::default(),
                TASK_LOGON_INTERACTIVE_TOKEN,
                &VARIANT::default(),
            )
            .map_err(|error| StartupError::Scheduler(error.to_string()))?;
        } else if let Err(error) = root.DeleteTask(&BSTR::from(TASK_NAME), 0) {
            let code = error.code().0 as u32;
            if code != 0x8007_0002 && code != 0x8004_130F {
                return Err(StartupError::Scheduler(error.to_string()));
            }
        }
    }

    Ok(())
}

#[cfg(windows)]
fn current_user() -> Result<String, StartupError> {
    match (
        std::env::var("USERDOMAIN").ok(),
        std::env::var("USERNAME").ok(),
    ) {
        (Some(domain), Some(user)) => Ok(format!("{domain}\\{user}")),
        (_, Some(user)) => Ok(user),
        _ => Err(StartupError::Scheduler(
            "current user is unknown".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{StartupError, StartupRegistration, update_start_at_login};
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
}
