use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tauri_plugin_dialog::DialogExt;

use crate::paths::{
    APP_EXE, APP_NAME, UNINSTALL_EXE, default_dir, desktop_shortcut_path, hidden_command,
    start_menu_shortcut_path,
};
use crate::registry::{read_install_location, register, webview2_installed};
use crate::resources::{STEP_RESOURCES, resources_present, update_resources};
use crate::{CmdResult, err};

const PAYLOAD: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/payload/app.zip"));
const STEP_WEBVIEW2: u8 = 0;
const STEP_COPY: u8 = 1;
const STEP_SHORTCUTS: u8 = 2;
const STEP_REGISTER: u8 = 3;

fn has_payload() -> bool {
    PAYLOAD.len() > 1024
}

#[derive(Serialize, Clone)]
pub struct Plan {
    default_dir: String,
    existing_dir: Option<String>,
    payload_ok: bool,
}

#[tauri::command]
pub fn plan() -> CmdResult<Plan> {
    let existing = read_install_location();
    Ok(Plan {
        default_dir: existing
            .clone()
            .unwrap_or_else(|| default_dir().to_string_lossy().into_owned()),
        existing_dir: existing,
        payload_ok: has_payload(),
    })
}

#[derive(Deserialize)]
pub struct InstallArgs {
    dir: String,
    desktop_shortcut: bool,
}

#[derive(Serialize, Clone)]
struct Progress {
    step: u8,
    percent: u8,
    detail: String,
}

fn emit_progress(app: &AppHandle, step: u8, percent: u8, detail: impl Into<String>) {
    let _ = app.emit(
        "install-progress",
        Progress {
            step,
            percent,
            detail: detail.into(),
        },
    );
}

#[derive(Serialize, Clone)]
pub struct Installed {
    dir: String,
    pub(crate) resources_error: Option<String>,
    pub(crate) resources_missing: bool,
}

fn parse_install_dir(raw: &str) -> CmdResult<PathBuf> {
    let dir = PathBuf::from(raw.trim());
    if dir.as_os_str().is_empty() || !dir.is_absolute() {
        return Err("설치 위치가 올바르지 않아요".into());
    }
    Ok(dir)
}

#[tauri::command]
pub async fn install_resources(app: AppHandle, dir: String) -> CmdResult<()> {
    let dir = parse_install_dir(&dir)?;
    tauri::async_runtime::spawn_blocking(move || {
        update_resources(&dir, &|step, percent, detail| {
            emit_progress(&app, step, percent, detail)
        })
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
pub async fn install(app: AppHandle, args: InstallArgs) -> CmdResult<Installed> {
    let dir = parse_install_dir(&args.dir)?;
    tauri::async_runtime::spawn_blocking(move || {
        do_install(&dir, args.desktop_shortcut, &|step, percent, detail| {
            emit_progress(&app, step, percent, detail)
        })
    })
    .await
    .map_err(err)?
    .map_err(|e| format!("{e:#}"))
}

pub(crate) fn do_install(
    dir: &Path,
    desktop_shortcut: bool,
    progress: &dyn Fn(u8, u8, &str),
) -> anyhow::Result<Installed> {
    progress(STEP_WEBVIEW2, 2, "");
    if !webview2_installed() {
        bail!("Microsoft Edge WebView2 런타임이 없어요. Windows 업데이트를 먼저 해 주세요.");
    }
    if !has_payload() {
        bail!("설치 파일이 비어 있어요 (개발용 빌드). 배포용 설치 파일을 쓰세요.");
    }
    progress(STEP_COPY, 5, "");
    let exe_path = dir.join(APP_EXE);
    if locked(&exe_path) {
        progress(STEP_COPY, 5, "Hoist가 닫히기를 기다리는 중");
        if !wait_unlocked(&exe_path, Duration::from_secs(15)) {
            bail!("실행 중인 Hoist를 닫고 다시 시도해 주세요.");
        }
    }
    extract_payload(dir, progress)?;
    let self_exe = std::env::current_exe().context("설치 프로그램 경로")?;
    std::fs::copy(&self_exe, dir.join(UNINSTALL_EXE)).context("제거 프로그램 복사")?;
    progress(STEP_SHORTCUTS, 32, "");
    create_shortcut(&start_menu_shortcut_path(), &exe_path, dir)?;
    if desktop_shortcut {
        create_shortcut(&desktop_shortcut_path(), &exe_path, dir)?;
    }
    progress(STEP_REGISTER, 38, "");
    register(dir, &exe_path)?;
    let resources_error = update_resources(dir, progress).err().map(err);
    if resources_error.is_none() {
        progress(STEP_RESOURCES, 100, "");
    }
    Ok(Installed {
        dir: dir.to_string_lossy().into_owned(),
        resources_missing: resources_error.is_some() && !resources_present(dir),
        resources_error,
    })
}

fn extract_payload(dir: &Path, progress: &dyn Fn(u8, u8, &str)) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("폴더를 만들 수 없어요: {}", dir.display()))?;
    let mut archive = zip::ZipArchive::new(Cursor::new(PAYLOAD)).context("설치 파일을 읽을 수 없어요")?;
    let total = archive.len().max(1);
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let out = dir.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buf)?;
        std::fs::write(&out, &buf).with_context(|| format!("쓸 수 없어요: {}", out.display()))?;
        let pct = 5 + (i as f64 / total as f64 * 25.0) as u8;
        progress(STEP_COPY, pct, "");
    }
    Ok(())
}

fn locked(exe: &Path) -> bool {
    exe.exists() && std::fs::OpenOptions::new().write(true).open(exe).is_err()
}

fn wait_unlocked(exe: &Path, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while locked(exe) {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    true
}

fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn create_shortcut(lnk: &Path, target: &Path, workdir: &Path) -> anyhow::Result<()> {
    if let Some(p) = lnk.parent() {
        std::fs::create_dir_all(p)?;
    }
    let script = format!(
        "$s=(New-Object -ComObject WScript.Shell).CreateShortcut({}); $s.TargetPath={}; $s.WorkingDirectory={}; $s.IconLocation={}; $s.Description={}; $s.Save()",
        ps_quote(&lnk.to_string_lossy()),
        ps_quote(&target.to_string_lossy()),
        ps_quote(&workdir.to_string_lossy()),
        ps_quote(&format!("{},0", target.to_string_lossy())),
        ps_quote(APP_NAME)
    );
    let out = hidden_command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .context("바로가기 만들기")?;
    if !out.status.success() {
        bail!(
            "바로가기를 만들지 못했어요: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

pub(crate) fn spawn_app(dir: &Path) -> std::io::Result<Child> {
    Command::new(dir.join(APP_EXE)).current_dir(dir).spawn()
}

#[tauri::command]
pub fn launch(app: AppHandle, dir: String) -> CmdResult<()> {
    spawn_app(Path::new(&dir)).map_err(|e| format!("실행하지 못했어요: {e}"))?;
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub async fn pick_dir(app: AppHandle) -> CmdResult<Option<String>> {
    let picked = app.dialog().file().set_title("설치 위치").blocking_pick_folder();
    Ok(picked
        .and_then(|p| p.into_path().ok())
        .map(|p| p.join(APP_NAME).to_string_lossy().into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launching_a_missing_app_is_an_error() {
        let dir = std::env::temp_dir().join(format!("hoist-setup-launch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        assert!(spawn_app(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
