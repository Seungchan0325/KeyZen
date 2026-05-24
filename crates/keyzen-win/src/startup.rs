use std::{env, mem::ManuallyDrop, path::Path};

use anyhow::{Context, Result};
use windows::{
    Win32::{
        Foundation::{RPC_E_CHANGED_MODE, S_FALSE, S_OK, VARIANT_FALSE},
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize,
            },
            TaskScheduler::{
                IExecAction, ILogonTrigger, ITaskFolder, ITaskService, TASK_ACTION_EXEC,
                TASK_CREATE_OR_UPDATE, TASK_LOGON_INTERACTIVE_TOKEN, TASK_RUNLEVEL_LUA,
                TASK_TRIGGER_LOGON, TaskScheduler,
            },
            Variant::{VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_BSTR, VariantClear},
        },
    },
    core::{BSTR, Interface},
};

const TASK_FOLDER: &str = "\\KeyZen";
const TASK_FOLDER_NAME: &str = "KeyZen";
const TASK_DELAY: &str = "PT03S";
const TASK_EXECUTION_TIME_LIMIT: &str = "PT0S";
const TASK_PRIORITY: i32 = 4;
const SDDL_FULL_ACCESS_FOR_EVERYONE: &str = "D:(A;;FA;;;WD)";

pub fn set_enabled(enabled: bool) -> Result<()> {
    let _com = ComApartment::initialize()?;
    let service = connect_task_service()?;
    let username = current_username()?;
    let task_name = task_name(&username);

    if enabled {
        let folder = get_or_create_keyzen_folder(&service)?;
        let user_id = current_user_id(&username)?;
        let exe = env::current_exe().context("failed to resolve current executable")?;
        create_startup_task(&service, &folder, &task_name, &user_id, &exe)
    } else {
        delete_startup_task(&service, &task_name)
    }
}

fn connect_task_service() -> Result<ITaskService> {
    let service = unsafe {
        CoCreateInstance::<_, ITaskService>(&TaskScheduler, None, CLSCTX_INPROC_SERVER)
            .context("failed to create Task Scheduler service")?
    };
    unsafe {
        service
            .Connect(
                &VARIANT::default(),
                &VARIANT::default(),
                &VARIANT::default(),
                &VARIANT::default(),
            )
            .context("failed to connect to Task Scheduler service")?;
    }
    Ok(service)
}

fn get_or_create_keyzen_folder(service: &ITaskService) -> Result<ITaskFolder> {
    let folder = unsafe { service.GetFolder(&BSTR::from(TASK_FOLDER)) };
    if let Ok(folder) = folder {
        return Ok(folder);
    }

    let root = unsafe { service.GetFolder(&BSTR::from("\\")) }
        .context("failed to get Task Scheduler root folder")?;
    unsafe {
        root.CreateFolder(&BSTR::from(TASK_FOLDER_NAME), &VARIANT::default())
            .context("failed to create KeyZen Task Scheduler folder")
    }
}

fn create_startup_task(
    service: &ITaskService,
    folder: &ITaskFolder,
    task_name: &str,
    user_id: &str,
    exe: &Path,
) -> Result<()> {
    let task = unsafe {
        service
            .NewTask(0)
            .context("failed to create KeyZen startup task definition")?
    };

    let registration_info = unsafe {
        task.RegistrationInfo()
            .context("failed to get KeyZen startup task registration info")?
    };
    unsafe {
        registration_info
            .SetAuthor(&BSTR::from(user_id))
            .context("failed to set KeyZen startup task author")?;
    }

    let settings = unsafe {
        task.Settings()
            .context("failed to get KeyZen startup task settings")?
    };
    unsafe {
        settings
            .SetStartWhenAvailable(VARIANT_FALSE)
            .context("failed to configure KeyZen startup task availability")?;
        settings
            .SetStopIfGoingOnBatteries(VARIANT_FALSE)
            .context("failed to configure KeyZen startup task battery behavior")?;
        settings
            .SetExecutionTimeLimit(&BSTR::from(TASK_EXECUTION_TIME_LIMIT))
            .context("failed to configure KeyZen startup task execution time limit")?;
        settings
            .SetDisallowStartIfOnBatteries(VARIANT_FALSE)
            .context("failed to configure KeyZen startup task battery start behavior")?;
        settings
            .SetPriority(TASK_PRIORITY)
            .context("failed to configure KeyZen startup task priority")?;
    }

    let triggers = unsafe {
        task.Triggers()
            .context("failed to get KeyZen startup task triggers")?
    };
    let trigger = unsafe {
        triggers
            .Create(TASK_TRIGGER_LOGON)
            .context("failed to create KeyZen startup task logon trigger")?
    };
    let logon_trigger = trigger
        .cast::<ILogonTrigger>()
        .context("failed to configure KeyZen startup task logon trigger")?;
    unsafe {
        logon_trigger
            .SetId(&BSTR::from("Trigger1"))
            .context("failed to set KeyZen startup task trigger id")?;
        logon_trigger
            .SetDelay(&BSTR::from(TASK_DELAY))
            .context("failed to set KeyZen startup task trigger delay")?;
        logon_trigger
            .SetUserId(&BSTR::from(user_id))
            .context("failed to set KeyZen startup task trigger user")?;
    }

    let actions = unsafe {
        task.Actions()
            .context("failed to get KeyZen startup task actions")?
    };
    let action = unsafe {
        actions
            .Create(TASK_ACTION_EXEC)
            .context("failed to create KeyZen startup task action")?
    };
    let exec_action = action
        .cast::<IExecAction>()
        .context("failed to configure KeyZen startup task action")?;
    unsafe {
        exec_action
            .SetPath(&BSTR::from(exe.display().to_string()))
            .context("failed to set KeyZen startup task executable path")?;
        if let Some(parent) = exe.parent() {
            exec_action
                .SetWorkingDirectory(&BSTR::from(parent.display().to_string()))
                .context("failed to set KeyZen startup task working directory")?;
        }
    }

    let principal = unsafe {
        task.Principal()
            .context("failed to get KeyZen startup task principal")?
    };
    unsafe {
        principal
            .SetId(&BSTR::from("Principal1"))
            .context("failed to set KeyZen startup task principal id")?;
        principal
            .SetUserId(&BSTR::from(user_id))
            .context("failed to set KeyZen startup task principal user")?;
        principal
            .SetLogonType(TASK_LOGON_INTERACTIVE_TOKEN)
            .context("failed to set KeyZen startup task logon type")?;
        principal
            .SetRunLevel(TASK_RUNLEVEL_LUA)
            .context("failed to set KeyZen startup task run level")?;
    }

    let user_id_variant = BstrVariant::new(user_id);
    let password_variant = VARIANT::default();
    let sddl_variant = BstrVariant::new(SDDL_FULL_ACCESS_FOR_EVERYONE);
    unsafe {
        folder
            .RegisterTaskDefinition(
                &BSTR::from(task_name),
                &task,
                TASK_CREATE_OR_UPDATE.0,
                user_id_variant.as_variant(),
                &password_variant,
                TASK_LOGON_INTERACTIVE_TOKEN,
                sddl_variant.as_variant(),
            )
            .context("failed to register KeyZen startup task")?;
    }

    Ok(())
}

fn delete_startup_task(service: &ITaskService, task_name: &str) -> Result<()> {
    let folder = unsafe { service.GetFolder(&BSTR::from(TASK_FOLDER)) };
    let Ok(folder) = folder else {
        return Ok(());
    };

    if unsafe { folder.GetTask(&BSTR::from(task_name)) }.is_ok() {
        unsafe {
            folder
                .DeleteTask(&BSTR::from(task_name), 0)
                .context("failed to delete KeyZen startup task")?;
        }
    }

    Ok(())
}

fn current_username() -> Result<String> {
    env::var("USERNAME").context("failed to read USERNAME environment variable")
}

fn current_user_id(username: &str) -> Result<String> {
    let domain =
        env::var("USERDOMAIN").context("failed to read USERDOMAIN environment variable")?;
    Ok(user_id(&domain, username))
}

fn user_id(domain: &str, username: &str) -> String {
    format!("{domain}\\{username}")
}

fn task_name(username: &str) -> String {
    format!("Autorun for {username}")
}

struct ComApartment {
    should_uninitialize: bool,
}

impl ComApartment {
    fn initialize() -> Result<Self> {
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if hr == S_OK || hr == S_FALSE {
            Ok(Self {
                should_uninitialize: true,
            })
        } else if hr == RPC_E_CHANGED_MODE {
            Ok(Self {
                should_uninitialize: false,
            })
        } else {
            hr.ok().context("failed to initialize COM apartment")?;
            Ok(Self {
                should_uninitialize: false,
            })
        }
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.should_uninitialize {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

struct BstrVariant(VARIANT);

impl BstrVariant {
    fn new(value: &str) -> Self {
        let bstr = BSTR::from(value);
        Self(VARIANT {
            Anonymous: VARIANT_0 {
                Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                    vt: VT_BSTR,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,
                    Anonymous: VARIANT_0_0_0 {
                        bstrVal: ManuallyDrop::new(bstr),
                    },
                }),
            },
        })
    }

    fn as_variant(&self) -> &VARIANT {
        &self.0
    }
}

impl Drop for BstrVariant {
    fn drop(&mut self) {
        unsafe {
            let _ = VariantClear(&mut self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_name_is_scoped_to_the_user() {
        assert_eq!(task_name("imcha"), "Autorun for imcha");
    }

    #[test]
    fn current_user_id_uses_domain_and_username() {
        assert_eq!(user_id("DESKTOP", "imcha"), "DESKTOP\\imcha");
    }
}
