use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn info(message: impl AsRef<str>) {
    write("INFO", message.as_ref());
}

pub fn error(message: impl AsRef<str>) {
    write("ERROR", message.as_ref());
}

pub fn path() -> PathBuf {
    log_dir().join("keyzen.log")
}

fn write(level: &str, message: &str) {
    let path = path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };

    let _ = writeln!(file, "[{}] {level} {message}", timestamp());
}

fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn log_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("KeyZen")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_path_uses_keyzen_log_file_name() {
        assert_eq!(path().file_name().unwrap(), "keyzen.log");
    }
}
