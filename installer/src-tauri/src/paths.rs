use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) const APP_NAME: &str = "Hoist";
pub(crate) const APP_EXE: &str = "hoist.exe";
pub(crate) const UNINSTALL_EXE: &str = "uninstall.exe";

pub(crate) fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from)
}

pub(crate) fn default_dir() -> PathBuf {
    env_dir("LOCALAPPDATA")
        .unwrap_or_else(|| PathBuf::from(r"C:\"))
        .join("Programs")
        .join(APP_NAME)
}

pub(crate) fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
}

pub(crate) fn start_menu_shortcut_path() -> PathBuf {
    env_dir("APPDATA")
        .unwrap_or_default()
        .join(r"Microsoft\Windows\Start Menu\Programs")
        .join(format!("{APP_NAME}.lnk"))
}

pub(crate) fn desktop_shortcut_path() -> PathBuf {
    desktop_dir().join(format!("{APP_NAME}.lnk"))
}

pub(crate) fn hidden_command(program: &str) -> Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

fn desktop_dir() -> PathBuf {
    let out = hidden_command("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "[Environment]::GetFolderPath('Desktop')",
        ])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !s.is_empty() {
                return PathBuf::from(s);
            }
        }
        _ => {}
    }
    env_dir("USERPROFILE").unwrap_or_default().join("Desktop")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_command_still_captures_output() {
        let out = hidden_command("cmd")
            .args(["/C", "echo ok"])
            .output()
            .expect("cmd");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "ok");
    }
}
