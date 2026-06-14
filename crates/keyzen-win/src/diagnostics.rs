use crate::app_settings::AppPaths;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::error;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;

const LOG_FILE: &str = "keyzen.log";
const PREVIOUS_LOG_FILE: &str = "keyzen.log.1";
const RUNNING_MARKER_FILE: &str = "tray.running";
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

pub struct TrayDiagnostics {
    log: Arc<Mutex<File>>,
    marker_path: PathBuf,
    session_started: bool,
}

impl TrayDiagnostics {
    pub fn initialize(paths: &AppPaths) -> io::Result<Self> {
        fs::create_dir_all(&paths.dir)?;

        let log_path = paths.dir.join(LOG_FILE);
        rotate_log_if_needed(&log_path, &paths.dir.join(PREVIOUS_LOG_FILE))?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;
        let log = Arc::new(Mutex::new(file));

        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info"))
            .add_directive(
                "keyzen_win::lifecycle=info"
                    .parse()
                    .expect("tray lifecycle log directive should be valid"),
            );
        tracing_subscriber::fmt()
            .with_ansi(false)
            .with_env_filter(filter)
            .with_writer(SharedMakeWriter { log: log.clone() })
            .try_init()
            .map_err(io::Error::other)?;

        install_panic_hook();

        tracing::info!(
            target: "keyzen_win::lifecycle",
            pid = std::process::id(),
            log_path = %log_path.display(),
            "tray diagnostics initialized"
        );

        Ok(Self {
            log,
            marker_path: paths.dir.join(RUNNING_MARKER_FILE),
            session_started: false,
        })
    }

    pub fn begin_session(&mut self) -> io::Result<()> {
        if let Ok(previous_session) = fs::read_to_string(&self.marker_path) {
            tracing::warn!(
                target: "keyzen_win::lifecycle",
                previous_session = %previous_session.trim(),
                "previous tray session did not shut down cleanly"
            );
        }
        fs::write(&self.marker_path, session_marker())?;
        self.session_started = true;

        tracing::info!(
            target: "keyzen_win::lifecycle",
            pid = std::process::id(),
            "tray session marker created"
        );

        Ok(())
    }

    pub fn complete(self, clean_shutdown: bool) {
        if clean_shutdown
            && self.session_started
            && let Err(error) = fs::remove_file(&self.marker_path)
        {
            tracing::warn!(
                target: "keyzen_win::lifecycle",
                path = %self.marker_path.display(),
                %error,
                "failed to remove tray running marker"
            );
        }
        self.flush();
    }

    fn flush(&self) {
        if let Ok(mut log) = self.log.lock() {
            let _ = log.flush();
        }
    }
}

impl Drop for TrayDiagnostics {
    fn drop(&mut self) {
        self.flush();
    }
}

#[derive(Clone)]
struct SharedMakeWriter {
    log: Arc<Mutex<File>>,
}

impl<'a> MakeWriter<'a> for SharedMakeWriter {
    type Writer = SharedWriter;

    fn make_writer(&'a self) -> Self::Writer {
        SharedWriter {
            log: self.log.clone(),
        }
    }
}

struct SharedWriter {
    log: Arc<Mutex<File>>,
}

impl Write for SharedWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let mut log = self
            .log
            .lock()
            .map_err(|_| io::Error::other("tray log lock was poisoned"))?;
        log.write_all(buffer)?;
        log.flush()?;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut log = self
            .log
            .lock()
            .map_err(|_| io::Error::other("tray log lock was poisoned"))?;
        log.flush()
    }
}

fn rotate_log_if_needed(log_path: &Path, previous_log_path: &Path) -> io::Result<()> {
    let Ok(metadata) = fs::metadata(log_path) else {
        return Ok(());
    };
    if metadata.len() < MAX_LOG_BYTES {
        return Ok(());
    }

    if previous_log_path.exists() {
        fs::remove_file(previous_log_path)?;
    }
    fs::rename(log_path, previous_log_path)
}

fn session_marker() -> String {
    let started_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("pid={} started_at_unix={started_at}", std::process::id())
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        let message = panic
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| panic.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("non-string panic payload");
        let location = panic
            .location()
            .map(ToString::to_string)
            .unwrap_or_else(|| "unknown".to_string());
        error!(
            target: "keyzen_win::lifecycle",
            panic_message = message,
            panic_location = %location,
            "tray process panicked"
        );
        previous(panic);
    }));
}

#[cfg(test)]
mod tests {
    use super::{MAX_LOG_BYTES, TrayDiagnostics, rotate_log_if_needed, session_marker};
    use crate::app_settings::AppPaths;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("keyzen-diagnostics-{name}-{nonce}"))
    }

    #[test]
    fn rotates_full_log_once() {
        let dir = temp_dir("rotate");
        fs::create_dir_all(&dir).unwrap();
        let log = dir.join("keyzen.log");
        let previous = dir.join("keyzen.log.1");
        let file = fs::File::create(&log).unwrap();
        file.set_len(MAX_LOG_BYTES).unwrap();

        rotate_log_if_needed(&log, &previous).unwrap();

        assert!(!log.exists());
        assert_eq!(fs::metadata(previous).unwrap().len(), MAX_LOG_BYTES);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn session_marker_identifies_process() {
        assert!(session_marker().contains(&format!("pid={}", std::process::id())));
    }

    #[test]
    fn writes_events_immediately_and_removes_clean_session_marker() {
        let dir = temp_dir("lifecycle");
        let paths = AppPaths::under(dir.clone());
        let mut diagnostics = TrayDiagnostics::initialize(&paths).unwrap();
        diagnostics.begin_session().unwrap();

        tracing::info!(
            target: "keyzen_win::lifecycle",
            "diagnostics integration test event"
        );
        diagnostics.complete(true);

        let log = fs::read_to_string(dir.join("keyzen.log")).unwrap();
        assert!(log.contains("diagnostics integration test event"));
        assert!(!dir.join("tray.running").exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
