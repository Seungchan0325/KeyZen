use keyzen_core::{KeyCode, OutputCommand};

#[derive(Debug, thiserror::Error)]
pub enum KeyzenWinError {
    #[error("KeyZen keyboard remapping is only supported on Windows")]
    UnsupportedPlatform,
    #[error("Windows API call `{api}` failed with code {code}")]
    WindowsApi { api: &'static str, code: u32 },
    #[error("unsupported output key `{0}`")]
    UnsupportedKey(String),
    #[error("input emitter failed: {0}")]
    Emit(String),
}

pub trait Emitter {
    fn emit(&self, commands: &[OutputCommand]) -> Result<(), KeyzenWinError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NullEmitter;

impl Emitter for NullEmitter {
    fn emit(&self, _commands: &[OutputCommand]) -> Result<(), KeyzenWinError> {
        Ok(())
    }
}

#[cfg(windows)]
mod platform;

#[cfg(windows)]
pub use platform::{SendInputEmitter, run, run_until_stop};

#[cfg(not(windows))]
pub fn run(_config: keyzen_core::Config) -> Result<(), KeyzenWinError> {
    Err(KeyzenWinError::UnsupportedPlatform)
}

#[cfg(not(windows))]
pub fn run_until_stop(
    _config: keyzen_core::Config,
    _stop: std::sync::mpsc::Receiver<()>,
) -> Result<(), KeyzenWinError> {
    Err(KeyzenWinError::UnsupportedPlatform)
}

pub fn emit_with<E: Emitter>(
    emitter: &E,
    commands: &[OutputCommand],
) -> Result<(), KeyzenWinError> {
    emitter.emit(commands)
}

pub fn key_label(key: &KeyCode) -> &str {
    key.as_str()
}

#[cfg(test)]
mod tests {
    use super::{Emitter, emit_with};
    use keyzen_core::OutputCommand;
    use std::cell::RefCell;

    #[derive(Default)]
    struct RecordingEmitter {
        commands: RefCell<Vec<OutputCommand>>,
    }

    impl Emitter for RecordingEmitter {
        fn emit(&self, commands: &[OutputCommand]) -> Result<(), super::KeyzenWinError> {
            self.commands.borrow_mut().extend_from_slice(commands);
            Ok(())
        }
    }

    #[test]
    fn emitter_can_be_mocked_without_sending_input() {
        let emitter = RecordingEmitter::default();
        let commands = vec![OutputCommand::Tap("A".parse().unwrap())];

        emit_with(&emitter, &commands).unwrap();

        assert_eq!(*emitter.commands.borrow(), commands);
    }
}
