use std::{env, path::Path};

use anyhow::{Context, Result};
use windows::{
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, NO_ERROR, WIN32_ERROR},
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RegCloseKey, RegDeleteValueW,
            RegOpenKeyExW, RegSetValueExW,
        },
    },
    core::HSTRING,
};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE_NAME: &str = "KeyZen";

pub fn set_enabled(enabled: bool) -> Result<()> {
    let key = open_run_key(KEY_SET_VALUE)?;
    let result = if enabled {
        let exe = env::current_exe().context("failed to resolve current executable")?;
        let command = quote_path(&exe);
        let wide = command.encode_utf16().chain([0]).collect::<Vec<_>>();
        let status = unsafe {
            RegSetValueExW(
                key,
                &HSTRING::from(VALUE_NAME),
                None,
                REG_SZ,
                Some(std::slice::from_raw_parts(
                    wide.as_ptr().cast(),
                    wide.len() * std::mem::size_of::<u16>(),
                )),
            )
        };
        win32_ok(status, "failed to write startup registry value")
    } else {
        let status = unsafe { RegDeleteValueW(key, &HSTRING::from(VALUE_NAME)) };
        if status == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            win32_ok(status, "failed to delete startup registry value")
        }
    };
    unsafe {
        let _ = RegCloseKey(key);
    }
    result
}

fn open_run_key(access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Result<HKEY> {
    let mut key = HKEY::default();
    unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(RUN_KEY),
            None,
            access,
            &mut key,
        )
    }
    .pipe(|status| win32_ok(status, "failed to open HKCU Run registry key"))?;
    Ok(key)
}

fn quote_path(path: &Path) -> String {
    format!("\"{}\"", path.display())
}

fn win32_ok(status: WIN32_ERROR, message: &'static str) -> Result<()> {
    if status == NO_ERROR {
        Ok(())
    } else {
        anyhow::bail!("{message}: Win32 error {}", status.0)
    }
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}

impl<T> Pipe for T {}
