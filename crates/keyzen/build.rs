#[cfg(windows)]
fn main() {
    use std::{
        env, fs,
        path::{Path, PathBuf},
        process::Command,
    };

    const APP_ICON_RESOURCE_ID: u16 = 1;

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let repo_root = manifest_dir.parent().unwrap().parent().unwrap();
    let icon_path = repo_root.join("assets").join("keyzen.ico");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let rc_path = out_dir.join("keyzen.rc");
    let res_path = out_dir.join("keyzen.res");

    println!("cargo:rerun-if-changed={}", icon_path.display());

    fs::write(
        &rc_path,
        format!(
            "{} ICON \"{}\"\n",
            APP_ICON_RESOURCE_ID,
            icon_path.display().to_string().replace('\\', "\\\\")
        ),
    )
    .expect("failed to write generated resource script");

    let rc = find_rc_exe().unwrap_or_else(|| PathBuf::from("rc.exe"));
    let status = Command::new(&rc)
        .arg("/nologo")
        .arg(format!("/fo{}", res_path.display()))
        .arg(&rc_path)
        .status()
        .expect("failed to run Windows resource compiler");

    if !status.success() {
        panic!("Windows resource compiler failed with status {status}");
    }

    println!("cargo:rustc-link-arg-bin=keyzen={}", res_path.display());

    fn find_rc_exe() -> Option<PathBuf> {
        if let Some(path) = env::var_os("WindowsSdkDir") {
            let sdk_dir = PathBuf::from(path);
            if let Some(candidate) = find_rc_in_sdk(&sdk_dir) {
                return Some(candidate);
            }
        }

        for root in [
            r"C:\Program Files (x86)\Windows Kits\10",
            r"C:\Program Files\Windows Kits\10",
        ] {
            if let Some(candidate) = find_rc_in_sdk(Path::new(root)) {
                return Some(candidate);
            }
        }

        None
    }

    fn find_rc_in_sdk(sdk_dir: &Path) -> Option<PathBuf> {
        let bin_dir = sdk_dir.join("bin");
        let entries = fs::read_dir(bin_dir).ok()?;
        let mut candidates = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path().join("x64").join("rc.exe"))
            .filter(|path| path.is_file())
            .collect::<Vec<_>>();
        candidates.sort();
        candidates.pop()
    }
}

#[cfg(not(windows))]
fn main() {}
