use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tauri_plugin_dialog::DialogExt;

const PAYLOAD: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/payload/app.zip"));
const RESOURCES: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/payload/resources.json"));
const RESOURCES_STAMP: &str = "resources.sha256";
const RESOURCE_DIRS: [&str; 2] = ["fonts", "models"];
const RELEASES: &str = "https://github.com/son-daehyeon/hoist-releases/releases/download";
const MB: u64 = 1_048_576;
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

#[derive(Deserialize, Default)]
#[serde(default)]
struct ResourceManifest {
    url_name: String,
    sha256: String,
    size: u64,
    unpacked_size: u64,
    version: String,
}

fn manifest() -> serde_json::Result<ResourceManifest> {
    serde_json::from_str(RESOURCES)
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
        size_mb: ((payload_size() as f64 * 2.2 + manifest().unwrap_or_default().unpacked_size as f64)
            / MB as f64)
            .round(),
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

#[derive(Serialize, Clone)]
pub struct Installed {
    dir: String,
    resources_error: Option<String>,
    resources_missing: bool,
}

fn install_dir(raw: &str) -> CmdResult<PathBuf> {
    let dir = PathBuf::from(raw.trim());
    if dir.as_os_str().is_empty() || !dir.is_absolute() {
        return Err("설치 위치가 올바르지 않아요".into());
    }
    Ok(dir)
}

#[tauri::command]
pub async fn install_resources(app: AppHandle, dir: String) -> CmdResult<()> {
    let dir = install_dir(&dir)?;
    tauri::async_runtime::spawn_blocking(move || {
        update_resources(&dir, &|step, percent, detail| emit(&app, step, percent, detail))
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
pub async fn install(app: AppHandle, args: InstallArgs) -> CmdResult<Installed> {
    let dir = install_dir(&args.dir)?;
    tauri::async_runtime::spawn_blocking(move || {
        do_install(&dir, args.desktop_shortcut, &|step, percent, detail| {
            emit(&app, step, percent, detail)
        })
    })
    .await
    .map_err(err)?
    .map_err(|e| format!("{e:#}"))
}

fn do_install(
    dir: &Path,
    desktop_shortcut: bool,
    progress: &dyn Fn(u8, u8, &str),
) -> anyhow::Result<Installed> {
    progress(0, 2, "WebView2 런타임 확인");
    if !webview2_installed() {
        bail!("Microsoft Edge WebView2 런타임이 없어요. Windows 업데이트를 먼저 해 주세요.");
    }
    if payload_size() <= 1024 {
        bail!("설치 파일이 비어 있어요 (개발용 빌드). 배포용 설치 파일을 쓰세요.");
    }
    progress(1, 5, "파일 복사 준비");
    let exe_path = dir.join(APP_EXE);
    if locked(&exe_path) {
        progress(1, 5, "Hoist가 닫히기를 기다리는 중");
        if !wait_unlocked(&exe_path, Duration::from_secs(15)) {
            bail!("실행 중인 Hoist를 닫고 다시 시도해 주세요.");
        }
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
        let pct = 5 + (i as f64 / total as f64 * 25.0) as u8;
        progress(1, pct, &format!("파일 복사 · {}", rel.display()));
    }
    let self_exe = std::env::current_exe().context("설치 프로그램 경로")?;
    std::fs::copy(&self_exe, dir.join(UNINSTALL_EXE)).context("제거 프로그램 복사")?;
    progress(2, 32, "바로가기 만들기");
    let start_menu = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(r"Microsoft\Windows\Start Menu\Programs");
    create_shortcut(&start_menu.join(format!("{APP_NAME}.lnk")), &exe_path, dir)?;
    if desktop_shortcut {
        let desktop = desktop_dir();
        create_shortcut(&desktop.join(format!("{APP_NAME}.lnk")), &exe_path, dir)?;
    }
    progress(3, 38, "프로그램 등록");
    register(dir, &exe_path)?;
    let resources_error = update_resources(dir, progress).err().map(err);
    if resources_error.is_none() {
        progress(4, 100, "완료");
    }
    Ok(Installed {
        dir: dir.to_string_lossy().into_owned(),
        resources_missing: resources_error.is_some() && !resources_present(dir),
        resources_error,
    })
}

fn resources_present(dir: &Path) -> bool {
    RESOURCE_DIRS.iter().all(|d| dir.join(d).is_dir())
}

fn update_resources(dir: &Path, progress: &dyn Fn(u8, u8, &str)) -> anyhow::Result<()> {
    let m = manifest().context("설치 파일의 글꼴·모델 정보가 잘못됐어요")?;
    if m.sha256.is_empty() {
        return Ok(());
    }
    let stamp = dir.join(RESOURCES_STAMP);
    let current = std::fs::read_to_string(&stamp).unwrap_or_default();
    if current.trim() == m.sha256 && resources_present(dir) {
        progress(4, 100, "글꼴·모델은 그대로 써요");
        return Ok(());
    }
    progress(4, 42, "글꼴·모델 확인 중");
    let local = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(&m.url_name)))
        .filter(|p| p.is_file() && sha256_file(p).ok().as_deref() == Some(m.sha256.as_str()));
    let (zip, downloaded) = match local {
        Some(p) => (p, false),
        None => (download(dir, &m, progress)?, true),
    };
    let unpacked = unpack(&zip, dir, progress);
    if downloaded {
        let _ = std::fs::remove_file(&zip);
    }
    unpacked?;
    std::fs::write(&stamp, &m.sha256).context("글꼴·모델 기록")?;
    Ok(())
}

fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn download(dir: &Path, m: &ResourceManifest, progress: &dyn Fn(u8, u8, &str)) -> anyhow::Result<PathBuf> {
    let base = std::env::var("HOIST_RESOURCES_BASE").unwrap_or_else(|_| format!("{RELEASES}/v{}", m.version));
    let url = format!("{}/{}", base.trim_end_matches('/'), m.url_name);
    let offline = "인터넷 연결을 확인하고 다시 시도해 주세요.";
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(60))
        .build()?;
    let mut resp = client.get(&url).send().context(offline)?;
    if !resp.status().is_success() {
        bail!("{offline} (HTTP {})", resp.status().as_u16());
    }
    let total = resp.content_length().unwrap_or(m.size).max(1);
    let tmp = dir.join("resources.download");
    let mut file = File::create(&tmp).context("글꼴·모델 임시 파일")?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let (mut done, mut shown) = (0u64, Instant::now());
    let received = loop {
        let n = match resp.read(&mut buf) {
            Ok(0) => break Ok(()),
            Ok(n) => n,
            Err(e) => break Err(e),
        };
        hasher.update(&buf[..n]);
        if let Err(e) = file.write_all(&buf[..n]) {
            break Err(e);
        }
        done += n as u64;
        if shown.elapsed() >= Duration::from_millis(200) {
            shown = Instant::now();
            let pct = 42 + (done.min(total) * 43 / total) as u8;
            progress(
                4,
                pct,
                &format!("글꼴·모델 받는 중 ({}/{}MB)", done / MB, total / MB),
            );
        }
    };
    drop(file);
    let valid = received.is_ok() && format!("{:x}", hasher.finalize()) == m.sha256;
    if !valid {
        let _ = std::fs::remove_file(&tmp);
        received.context(offline)?;
        bail!("받은 파일이 손상됐어요. 다시 시도해 주세요.");
    }
    Ok(tmp)
}

fn unpack(zip: &Path, dir: &Path, progress: &dyn Fn(u8, u8, &str)) -> anyhow::Result<()> {
    let staging = dir.join("resources.new");
    let _ = std::fs::remove_dir_all(&staging);
    let mut archive = zip::ZipArchive::new(File::open(zip)?).context("글꼴·모델 파일을 읽을 수 없어요")?;
    let total = archive.len().max(1);
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let out = staging.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::io::copy(&mut entry, &mut File::create(&out)?)
            .with_context(|| format!("쓸 수 없어요: {}", out.display()))?;
        progress(
            4,
            85 + (i * 14 / total) as u8,
            &format!("글꼴·모델 푸는 중 · {}", rel.display()),
        );
    }
    for entry in std::fs::read_dir(&staging)?.flatten() {
        let target = dir.join(entry.file_name());
        if target.is_dir() {
            std::fs::remove_dir_all(&target)
                .with_context(|| format!("지울 수 없어요: {}", target.display()))?;
        } else if target.exists() {
            std::fs::remove_file(&target)?;
        }
        std::fs::rename(entry.path(), &target)
            .with_context(|| format!("옮길 수 없어요: {}", target.display()))?;
    }
    std::fs::remove_dir_all(&staging)?;
    Ok(())
}

pub struct Silent {
    relaunch: bool,
    dir: Option<PathBuf>,
}

pub fn silent_mode() -> Option<Silent> {
    if is_uninstall_mode() {
        return None;
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    parse_silent(&args)
}

fn parse_silent(all: &[String]) -> Option<Silent> {
    let args: Vec<&String> = all
        .iter()
        .take_while(|a| !a.eq_ignore_ascii_case("/ARGS"))
        .collect();
    // Git Bash rewrites a lone `/P` into `P:/` (MSYS path conversion), so accept that form too.
    let has = |flag: char| {
        args.iter().any(|a| {
            let a = a.to_ascii_uppercase();
            a == format!("/{flag}") || a == format!("{flag}:/")
        })
    };
    let dir = args
        .iter()
        .position(|a| a.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("/D=")))
        .map(|i| {
            let rest: Vec<&str> = args[i..].iter().map(|a| a.as_str()).collect();
            PathBuf::from(&rest.join(" ")[3..])
        });
    (has('S') || has('P')).then(|| Silent {
        relaunch: has('R'),
        dir,
    })
}

pub fn run_silent(mode: Silent) -> i32 {
    let dir = mode
        .dir
        .or_else(|| read_install_location().map(PathBuf::from))
        .unwrap_or_else(default_dir);
    let exe = dir.join(APP_EXE);
    let desktop_shortcut = desktop_dir().join(format!("{APP_NAME}.lnk")).exists();
    let (code, problem) = match do_install(&dir, desktop_shortcut, &|_, _, _| {}) {
        Ok(done) => (i32::from(done.resources_missing), done.resources_error),
        Err(e) => (1, Some(format!("{e:#}"))),
    };
    if let Some(problem) = problem {
        log_line(&format!("{} exit={code}: {problem}", dir.display()));
    }
    if mode.relaunch && exe.exists() {
        let _ = Command::new(&exe).current_dir(&dir).spawn();
    }
    code
}

fn log_line(message: &str) {
    let Some(dir) = std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("com.hoist.installer"))
    else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("install.log"))
    {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let when = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let _ = writeln!(file, "{when} [{}] {message}", args.join(" "));
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn silent(args: &[&str]) -> Option<(bool, Option<PathBuf>)> {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        parse_silent(&args).map(|s| (s.relaunch, s.dir))
    }

    #[test]
    fn silent_flags_and_install_dir() {
        assert_eq!(silent(&["/P", "/UPDATE"]), Some((false, None)));
        assert_eq!(
            silent(&["/p", "/update", "/R", "/ARGS", "/S"]),
            Some((true, None))
        );
        assert_eq!(silent(&["/UPDATE"]), None);
        assert_eq!(silent(&["/UPDATE", "/ARGS", "/P"]), None);
        assert_eq!(
            silent(&["P:/", "C:/Program Files/Git/UPDATE", "R:/"]),
            Some((true, None))
        );
        assert_eq!(
            silent(&["/S", "/D=C:\\Temp\\my", "dir"]),
            Some((false, Some(PathBuf::from("C:\\Temp\\my dir"))))
        );
    }

    #[test]
    fn manifest_from_pack_resources_parses() {
        let m: ResourceManifest = serde_json::from_str(
            r#"{"url_name":"Hoist-Resources-b4d760d3.zip","sha256":"b4d7","size":289907061,"unpacked_size":495098813,"version":"0.1.0"}"#,
        )
        .expect("manifest");
        assert_eq!(
            (m.size, m.unpacked_size, m.version.as_str()),
            (289907061, 495098813, "0.1.0")
        );
        assert!(
            serde_json::from_str::<ResourceManifest>("{}")
                .expect("placeholder")
                .sha256
                .is_empty()
        );
        assert!(serde_json::from_str::<ResourceManifest>(r#"{"unpacked_size": 1.0}"#).is_err());
    }

    #[test]
    fn unpack_replaces_resource_folders() {
        let dir = std::env::temp_dir().join(format!("hoist-setup-unpack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("fonts")).expect("dir");
        std::fs::write(dir.join("fonts/old.otf"), b"old").expect("old");
        std::fs::write(dir.join("hoist.exe"), b"exe").expect("exe");
        let zip_path = dir.join("res.zip");
        let mut w = zip::ZipWriter::new(File::create(&zip_path).expect("zip"));
        let opts = zip::write::SimpleFileOptions::default();
        for (name, body) in [
            ("fonts/new.otf", b"font".as_slice()),
            ("models/m.onnx", b"model".as_slice()),
        ] {
            w.start_file(name, opts).expect("entry");
            w.write_all(body).expect("write");
        }
        w.finish().expect("finish");
        unpack(&zip_path, &dir, &|_, _, _| {}).expect("unpack");
        assert!(!dir.join("fonts/old.otf").exists());
        assert_eq!(std::fs::read(dir.join("fonts/new.otf")).expect("font"), b"font");
        assert_eq!(std::fs::read(dir.join("models/m.onnx")).expect("model"), b"model");
        assert_eq!(std::fs::read(dir.join("hoist.exe")).expect("exe"), b"exe");
        assert!(!dir.join("resources.new").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
