use std::path::Path;

use anyhow::Context;
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

use crate::paths::{APP_EXE, APP_NAME, UNINSTALL_EXE};

const REG_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Hoist";

pub(crate) fn webview2_installed() -> bool {
    let keys = [
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
        ),
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
        ),
        (
            HKEY_CURRENT_USER,
            r"Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
        ),
    ];
    keys.iter().any(|(root, path)| {
        RegKey::predef(*root)
            .open_subkey(path)
            .and_then(|k| k.get_value::<String, _>("pv"))
            .map(|v| !v.is_empty() && v != "0.0.0.0")
            .unwrap_or(false)
    })
}

pub(crate) fn read_install_location() -> Option<String> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(REG_KEY)
        .ok()?
        .get_value::<String, _>("InstallLocation")
        .ok()
        .filter(|p| Path::new(p).join(APP_EXE).exists())
}

pub(crate) fn register(dir: &Path, exe: &Path) -> anyhow::Result<()> {
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(REG_KEY)
        .context("레지스트리 등록")?;
    key.set_value("DisplayName", &APP_NAME)?;
    key.set_value("DisplayVersion", &env!("CARGO_PKG_VERSION"))?;
    key.set_value("Publisher", &"Hoist")?;
    key.set_value("InstallLocation", &dir.to_string_lossy().into_owned())?;
    key.set_value("DisplayIcon", &exe.to_string_lossy().into_owned())?;
    key.set_value(
        "UninstallString",
        &format!("\"{}\" --uninstall", dir.join(UNINSTALL_EXE).to_string_lossy()),
    )?;
    key.set_value("NoModify", &1u32)?;
    key.set_value("NoRepair", &1u32)?;
    key.set_value("EstimatedSize", &((dir_size(dir) / 1024) as u32))?;
    key.set_value("InstallDate", &install_date())?;
    Ok(())
}

pub(crate) fn unregister() {
    let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(REG_KEY);
}

fn install_date() -> String {
    chrono::Local::now().format("%Y%m%d").to_string()
}

fn dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            total += if p.is_dir() {
                dir_size(&p)
            } else {
                e.metadata().map(|m| m.len()).unwrap_or(0)
            };
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_date_is_eight_digits() {
        let date = install_date();
        assert_eq!(date.len(), 8);
        assert!(date.chars().all(|c| c.is_ascii_digit()));
    }
}
