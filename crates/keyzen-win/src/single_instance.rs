#[derive(Debug, thiserror::Error)]
pub enum SingleInstanceError {
    #[error("another KeyZen remapper instance is already running")]
    AlreadyRunning,
    #[error("failed to create KeyZen single-instance mutex: {0}")]
    CreateFailed(std::io::Error),
    #[error("single-instance mutex is only supported on Windows")]
    UnsupportedPlatform,
}

#[cfg(windows)]
#[derive(Debug)]
pub struct SingleInstance {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl SingleInstance {
    pub fn acquire() -> Result<Self, SingleInstanceError> {
        use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
        use windows_sys::Win32::System::Threading::CreateMutexW;

        let name = "Local\\KeyZen.Runtime"
            .encode_utf16()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if handle.is_null() {
            return Err(SingleInstanceError::CreateFailed(
                std::io::Error::last_os_error(),
            ));
        }
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(handle);
            }
            return Err(SingleInstanceError::AlreadyRunning);
        }

        Ok(Self { handle })
    }
}

#[cfg(windows)]
impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

#[cfg(not(windows))]
#[derive(Debug)]
pub struct SingleInstance;

#[cfg(not(windows))]
impl SingleInstance {
    pub fn acquire() -> Result<Self, SingleInstanceError> {
        Err(SingleInstanceError::UnsupportedPlatform)
    }
}
