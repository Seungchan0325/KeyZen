use windows::{
    Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
    core::HSTRING,
};

pub fn show_error(title: &str, message: &str) {
    let title = HSTRING::from(title);
    let message = HSTRING::from(message);
    unsafe {
        let _ = MessageBoxW(None, &message, &title, MB_OK | MB_ICONERROR);
    }
}

pub fn show_fatal_error(error: &anyhow::Error) {
    let message = format!(
        "KeyZen failed to start.\n\n{error:#}\n\nDetails were written to:\n{}",
        crate::log::path().display()
    );
    show_error("KeyZen Startup Error", &message);
}

pub fn show_config_error(context: &str, error: &anyhow::Error) {
    let message = format!(
        "{context}\n\n{error:#}\n\nDetails were written to:\n{}",
        crate::log::path().display()
    );
    show_error("KeyZen Config Error", &message);
}
