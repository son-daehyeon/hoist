use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tauri_plugin_dialog::DialogExt;

const PAYLOAD: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/payload/app.zip"));
const APP_NAME: &str = "Hoist";
const APP_EXE: &str = "hoist.exe";
const UNINSTALL_EXE: &str = "uninstall.exe";
const REG_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Hoist";
const APP_IDENTIFIER: &str = "com.hoist.app";

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

pub fn is_uninstall_mode() -> bool {
    if std::env::args().any(|a| a == "--uninstall") {
        return true;
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().eq_ignore_ascii_case(UNINSTALL_EXE))
        })
        .unwrap_or(false)
}

fn local_programs() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\"))
        .join("Programs")
}

pub fn default_dir() -> PathBuf {
    local_programs().join(APP_NAME)
}

fn payload_size() -> u64 {
    PAYLOAD.len() as u64
}

fn webview2_installed() -> bool {
    use winreg::RegKey;
    use winreg::enums::*;
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

#[derive(Serialize, Clone)]
pub struct Plan {
    default_dir: String,
    size_mb: f64,
    existing_dir: Option<String>,
    webview2: bool,
    payload_ok: bool,
    projects_dir: String,
}

#[tauri::command]
pub fn plan() -> CmdResult<Plan> {
    let existing = read_install_location();
    let docs = dirs_documents().join("Hoist");
    Ok(Plan {
        default_dir: existing
            .clone()
            .unwrap_or_else(|| default_dir().to_string_lossy().into_owned()),
        size_mb: (payload_size() as f64 * 2.2 / 1_048_576.0).round(),
        existing_dir: existing,
        webview2: webview2_installed(),
        payload_ok: payload_size() > 1024,
        projects_dir: docs.to_string_lossy().into_owned(),
    })
}

fn dirs_documents() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\"))
        .join("Documents")
}

fn read_install_location() -> Option<String> {
    use winreg::RegKey;
    use winreg::enums::*;
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(REG_KEY)
        .ok()?
        .get_value::<String, _>("InstallLocation")
        .ok()
        .filter(|p| Path::new(p).join(APP_EXE).exists())
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

fn emit(app: &AppHandle, step: u8, percent: u8, detail: impl Into<String>) {
    let _ = app.emit(
        "install-progress",
        Progress {
            step,
            percent,
            detail: detail.into(),
        },
    );
}

#[tauri::command]
pub async fn install(app: AppHandle, args: InstallArgs) -> CmdResult<String> {
    let dir = PathBuf::from(args.dir.trim());
    if dir.as_os_str().is_empty() || !dir.is_absolute() {
        return Err("설치 위치가 올바르지 않아요".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        do_install(&dir, args.desktop_shortcut, &|step, percent, detail| {
            emit(&app, step, percent, detail)
        })
    })
    .await
    .map_err(err)?
    .map_err(|e| format!("{e:#}"))
}

fn do_install(dir: &Path, desktop_shortcut: bool, progress: &dyn Fn(u8, u8, &str)) -> anyhow::Result<String> {
    progress(0, 2, "WebView2 런타임 확인");
    if !webview2_installed() {
        bail!("Microsoft Edge WebView2 런타임이 없어요. Windows 업데이트를 먼저 해 주세요.");
    }
    if payload_size() <= 1024 {
        bail!("설치 파일이 비어 있어요 (개발용 빌드). 배포용 설치 파일을 쓰세요.");
    }
    progress(1, 5, "파일 복사 준비");
    let exe_path = dir.join(APP_EXE);
    if exe_path.exists() && is_running(APP_EXE) {
        bail!("실행 중인 Hoist를 닫고 다시 시도해 주세요.");
    }
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
        let pct = 5 + (i as f64 / total as f64 * 70.0) as u8;
        progress(1, pct, &format!("파일 복사 · {}", rel.display()));
    }
    let self_exe = std::env::current_exe().context("설치 프로그램 경로")?;
    std::fs::copy(&self_exe, dir.join(UNINSTALL_EXE)).context("제거 프로그램 복사")?;
    progress(2, 80, "바로가기 만들기");
    let start_menu = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(r"Microsoft\Windows\Start Menu\Programs");
    create_shortcut(&start_menu.join(format!("{APP_NAME}.lnk")), &exe_path, dir)?;
    if desktop_shortcut {
        let desktop = desktop_dir();
        create_shortcut(&desktop.join(format!("{APP_NAME}.lnk")), &exe_path, dir)?;
    }
    progress(3, 92, "프로그램 등록");
    register(dir, &exe_path)?;
    progress(3, 100, "완료");
    Ok(dir.to_string_lossy().into_owned())
}

pub struct Silent {
    relaunch: bool,
}

pub fn silent_mode() -> Option<Silent> {
    if is_uninstall_mode() {
        return None;
    }
    let args: Vec<String> = std::env::args()
        .skip(1)
        .take_while(|a| !a.eq_ignore_ascii_case("/ARGS"))
        .collect();
    let has = |flag: &str| args.iter().any(|a| a.eq_ignore_ascii_case(flag));
    (has("/S") || has("/P")).then(|| Silent { relaunch: has("/R") })
}

pub fn run_silent(mode: Silent) -> i32 {
    let dir = read_install_location()
        .map(PathBuf::from)
        .unwrap_or_else(default_dir);
    let exe = dir.join(APP_EXE);
    wait_unlocked(&exe, Duration::from_secs(15));
    let desktop_shortcut = desktop_dir().join(format!("{APP_NAME}.lnk")).exists();
    let code = match do_install(&dir, desktop_shortcut, &|_, _, _| {}) {
        Ok(_) => 0,
        Err(_) => 1,
    };
    if mode.relaunch && exe.exists() {
        let _ = Command::new(&exe).current_dir(&dir).spawn();
    }
    code
}

fn wait_unlocked(exe: &Path, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while exe.exists()
        && std::fs::OpenOptions::new().write(true).open(exe).is_err()
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn desktop_dir() -> PathBuf {
    let out = Command::new("powershell")
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
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join("Desktop")
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
    let out = Command::new("powershell")
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

fn register(dir: &Path, exe: &Path) -> anyhow::Result<()> {
    use winreg::RegKey;
    use winreg::enums::*;
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
    key.set_value("InstallDate", &chrono_date())?;
    Ok(())
}

fn chrono_date() -> String {
    let out = Command::new("powershell")
        .args(["-NoProfile", "-Command", "Get-Date -Format yyyyMMdd"])
        .output();
    out.ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
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

fn is_running(exe: &str) -> bool {
    let out = Command::new("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {exe}"), "/NH"])
        .output();
    out.map(|o| {
        String::from_utf8_lossy(&o.stdout)
            .to_lowercase()
            .contains(&exe.to_lowercase())
    })
    .unwrap_or(false)
}

#[tauri::command]
pub fn launch(app: AppHandle, dir: String) -> CmdResult<()> {
    let exe = PathBuf::from(&dir).join(APP_EXE);
    Command::new(&exe)
        .current_dir(&dir)
        .spawn()
        .map_err(|e| format!("실행하지 못했어요: {e}"))?;
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub fn close_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub async fn pick_dir(app: AppHandle) -> CmdResult<Option<String>> {
    let picked = app.dialog().file().set_title("설치 위치").blocking_pick_folder();
    Ok(picked
        .and_then(|p| p.into_path().ok())
        .map(|p| p.join(APP_NAME).to_string_lossy().into_owned()))
}

#[derive(Deserialize)]
pub struct UninstallArgs {
    keep_projects: bool,
    remove_settings: bool,
}

#[tauri::command]
pub async fn uninstall(app: AppHandle, args: UninstallArgs) -> CmdResult<()> {
    let dir = read_install_location()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        })
        .ok_or("설치 위치를 찾을 수 없어요")?;
    if is_running(APP_EXE) {
        return Err("실행 중인 Hoist를 닫고 다시 시도해 주세요".into());
    }
    let start_menu = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(r"Microsoft\Windows\Start Menu\Programs")
        .join(format!("{APP_NAME}.lnk"));
    let _ = std::fs::remove_file(start_menu);
    let _ = std::fs::remove_file(desktop_dir().join(format!("{APP_NAME}.lnk")));
    {
        use winreg::RegKey;
        use winreg::enums::*;
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(REG_KEY);
    }
    if args.remove_settings {
        if let Some(appdata) = std::env::var_os("APPDATA").map(PathBuf::from) {
            let _ = std::fs::remove_dir_all(appdata.join(APP_IDENTIFIER));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            let _ = std::fs::remove_dir_all(local.join(APP_IDENTIFIER));
        }
        let _ = Command::new("cmdkey").args(["/delete:openai.hoist"]).output();
    }
    if !args.keep_projects {
        let _ = std::fs::remove_dir_all(dirs_documents().join("Hoist"));
    }
    for entry in std::fs::read_dir(&dir).map_err(err)?.flatten() {
        let p = entry.path();
        if p.file_name()
            .map(|n| n.to_string_lossy().eq_ignore_ascii_case(UNINSTALL_EXE))
            .unwrap_or(false)
        {
            continue;
        }
        let _ = if p.is_dir() {
            std::fs::remove_dir_all(&p)
        } else {
            std::fs::remove_file(&p)
        };
    }
    let script = format!(
        "ping -n 3 127.0.0.1 > nul & rmdir /s /q \"{}\"",
        dir.to_string_lossy()
    );
    Command::new("cmd").args(["/C", &script]).spawn().map_err(err)?;
    app.exit(0);
    Ok(())
}
