use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};

use windows::Win32::{Foundation::SYSTEMTIME, System::SystemInformation::GetLocalTime};

use crate::app_config::LoggingConfig;

static LOGGER: LazyLock<KeyZenLogger> = LazyLock::new(KeyZenLogger::default);

pub fn init_default() {
    init_with(LoggingConfig::default());
}

pub fn configure(config: &LoggingConfig) {
    init_with(config.clone());
}

pub fn path() -> PathBuf {
    configured_path()
}

pub fn configured_path() -> PathBuf {
    LOGGER.state.lock().unwrap().config.resolved_path()
}

pub fn info(message: impl AsRef<str>) {
    ::log::info!("{}", message.as_ref());
}

pub fn error(message: impl AsRef<str>) {
    ::log::error!("{}", message.as_ref());
}

fn init_with(config: LoggingConfig) {
    let max_level = config.level.as_level_filter();
    LOGGER.configure(config);
    if ::log::set_logger(&*LOGGER).is_ok() {
        ::log::set_max_level(max_level);
    } else {
        ::log::set_max_level(max_level);
    }
}

#[derive(Default)]
struct KeyZenLogger {
    state: Mutex<LoggerState>,
}

impl KeyZenLogger {
    fn configure(&self, config: LoggingConfig) {
        let mut state = self.state.lock().unwrap();
        state.config = config;
        state.file = None;
    }
}

impl ::log::Log for KeyZenLogger {
    fn enabled(&self, metadata: &::log::Metadata<'_>) -> bool {
        metadata.level() <= self.state.lock().unwrap().config.level.as_level_filter()
    }

    fn log(&self, record: &::log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let line = format_log_line(record);

        let mut state = self.state.lock().unwrap();
        let _ = state.write_line(line.as_bytes());
    }

    fn flush(&self) {
        let mut state = self.state.lock().unwrap();
        if let Some(file) = state.file.as_mut() {
            let _ = file.flush();
        }
    }
}

struct LoggerState {
    config: LoggingConfig,
    file: Option<File>,
}

impl Default for LoggerState {
    fn default() -> Self {
        Self {
            config: LoggingConfig::default(),
            file: None,
        }
    }
}

impl LoggerState {
    fn write_line(&mut self, line: &[u8]) -> std::io::Result<()> {
        let path = self.config.resolved_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        if self.should_rotate(&path, line.len() as u64) {
            self.file = None;
            rotate_files(&path, self.config.max_files)?;
        }

        if self.file.is_none() {
            self.file = Some(OpenOptions::new().create(true).append(true).open(path)?);
        }

        if let Some(file) = self.file.as_mut() {
            file.write_all(line)?;
        }
        Ok(())
    }

    fn should_rotate(&self, path: &Path, next_len: u64) -> bool {
        let Ok(metadata) = fs::metadata(path) else {
            return false;
        };
        metadata.len().saturating_add(next_len) > self.config.max_bytes
    }
}

fn rotate_files(path: &Path, max_files: u8) -> std::io::Result<()> {
    let last_path = rotated_path(path, max_files);
    match fs::remove_file(&last_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    for index in (1..max_files).rev() {
        let source = rotated_path(path, index);
        if source.exists() {
            fs::rename(source, rotated_path(path, index + 1))?;
        }
    }

    if path.exists() {
        fs::rename(path, rotated_path(path, 1))?;
    }

    Ok(())
}

fn rotated_path(path: &Path, index: u8) -> PathBuf {
    let mut name = path
        .file_name()
        .map(OsString::from)
        .unwrap_or_else(|| OsString::from("keyzen.log"));
    name.push(format!(".{index}"));
    path.with_file_name(name)
}

fn format_log_line(record: &::log::Record<'_>) -> String {
    format!(
        "{} {} {} - {}\n",
        local_timestamp(),
        format_level(record.level()),
        record.target(),
        record.args()
    )
}

fn local_timestamp() -> String {
    let time = unsafe { GetLocalTime() };
    format_system_time(&time)
}

fn format_system_time(time: &SYSTEMTIME) -> String {
    if time.wYear == 0
        || time.wMonth == 0
        || time.wMonth > 12
        || time.wDay == 0
        || time.wDay > 31
        || time.wHour > 23
        || time.wMinute > 59
        || time.wSecond > 59
    {
        return "0000-00-00 00:00:00".to_string();
    }

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        time.wYear, time.wMonth, time.wDay, time.wHour, time.wMinute, time.wSecond
    )
}

fn format_level(level: ::log::Level) -> &'static str {
    match level {
        ::log::Level::Error => "[ERROR]",
        ::log::Level::Warn => "[WARN ]",
        ::log::Level::Info => "[INFO ]",
        ::log::Level::Debug => "[DEBUG]",
        ::log::Level::Trace => "[TRACE]",
    }
}

#[cfg(test)]
fn unique_timestamp_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_path_uses_keyzen_log_file_name() {
        assert_eq!(path().file_name().unwrap(), "keyzen.log");
    }

    #[test]
    fn formats_system_time_as_local_date_time() {
        let time = SYSTEMTIME {
            wYear: 2026,
            wMonth: 5,
            wDayOfWeek: 1,
            wDay: 25,
            wHour: 14,
            wMinute: 32,
            wSecond: 10,
            wMilliseconds: 42,
        };

        assert_eq!(format_system_time(&time), "2026-05-25 14:32:10");
    }

    #[test]
    fn formats_invalid_system_time_as_placeholder() {
        let time = SYSTEMTIME::default();

        assert_eq!(format_system_time(&time), "0000-00-00 00:00:00");
    }

    #[test]
    fn formats_levels_with_fixed_width() {
        assert_eq!(format_level(::log::Level::Error), "[ERROR]");
        assert_eq!(format_level(::log::Level::Warn), "[WARN ]");
        assert_eq!(format_level(::log::Level::Info), "[INFO ]");
        assert_eq!(format_level(::log::Level::Debug), "[DEBUG]");
        assert_eq!(format_level(::log::Level::Trace), "[TRACE]");
    }

    #[test]
    fn formats_log_line_with_readable_structure() {
        let args = format_args!("KeyZen keyboard hook installed");
        let record = ::log::Record::builder()
            .args(args)
            .level(::log::Level::Info)
            .target("keyzen_win::app")
            .build();

        let line = format_log_line(&record);

        assert!(line.contains(" [INFO ] keyzen_win::app - KeyZen keyboard hook installed\n"));
    }

    #[test]
    fn rotates_log_file_when_max_bytes_is_exceeded() {
        let dir = unique_temp_path();
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("keyzen.log");
        fs::write(&path, "existing").unwrap();

        let mut state = LoggerState {
            config: LoggingConfig {
                path: Some(path.clone()),
                max_bytes: 8,
                max_files: 3,
                ..Default::default()
            },
            file: None,
        };

        state.write_line(b"next line\n").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "next line\n");
        assert_eq!(
            fs::read_to_string(rotated_path(&path, 1)).unwrap(),
            "existing"
        );
    }

    #[test]
    fn removes_rotated_files_beyond_retention() {
        let dir = unique_temp_path();
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("keyzen.log");
        fs::write(&path, "current").unwrap();
        fs::write(rotated_path(&path, 1), "old1").unwrap();
        fs::write(rotated_path(&path, 2), "old2").unwrap();

        rotate_files(&path, 2).unwrap();

        assert_eq!(
            fs::read_to_string(rotated_path(&path, 1)).unwrap(),
            "current"
        );
        assert_eq!(fs::read_to_string(rotated_path(&path, 2)).unwrap(), "old1");
        assert!(!rotated_path(&path, 3).exists());
    }

    fn unique_temp_path() -> PathBuf {
        std::env::temp_dir().join(format!("keyzen-log-test-{}", unique_timestamp_nanos()))
    }
}
