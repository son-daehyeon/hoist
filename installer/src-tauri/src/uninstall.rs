use std::path::{Path, PathBuf};
use std::process::Child;

use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::paths::{
    APP_EXE, APP_NAME, UNINSTALL_EXE, desktop_shortcut_path, env_dir, exe_dir, hidden_command,
    start_menu_shortcut_path,
};
use crate::registry::{read_install_location, unregister};
use crate::{CmdResult, err};

const APP_IDENTIFIER: &str = "com.hoist.app";

#[derive(Deserialize)]
pub struct UninstallArgs {
    keep_projects: bool,
    remove_settings: bool,
}

pub(crate) fn is_uninstall_mode() -> bool {
    if std::env::args().any(|a| a == "--uninstall") {
        return true;
    }
    std::env::current_exe().ok().is_some_and(|p| is_uninstaller(&p))
}

fn is_uninstaller(path: &Path) -> bool {
    path.file_name()
        .map(|n| n.to_string_lossy().eq_ignore_ascii_case(UNINSTALL_EXE))
        .unwrap_or(false)
}

fn read_json(file: &Path) -> Option<serde_json::Value> {
    serde_json::from_slice(&std::fs::read(file).ok()?).ok()
}

fn stored_projects_dir(settings: &Path) -> Option<PathBuf> {
    read_json(settings)?
        .get("projects_dir")?
        .as_str()
        .map(PathBuf::from)
}

fn resolve_projects_dir(settings: &Path, default: Option<PathBuf>) -> Option<PathBuf> {
    stored_projects_dir(settings)
        .or(default)
        .filter(|dir| dir.is_absolute())
}

fn default_projects_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().document_dir().ok().map(|d| d.join(APP_NAME))
}

pub(crate) fn projects_dir(app: &AppHandle) -> Option<PathBuf> {
    let settings = env_dir("APPDATA")?.join(APP_IDENTIFIER).join("settings.json");
    resolve_projects_dir(&settings, default_projects_dir(app))
}

fn is_project_dir(dir: &Path) -> bool {
    let Some(id) = dir.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    id.len() == 32
        && id.chars().all(|c| c.is_ascii_hexdigit())
        && read_json(&dir.join("project.json")).is_some_and(|project| project["id"] == id)
        && dir.join("document.json").is_file()
}

fn is_library_file(file: &Path) -> bool {
    read_json(file).is_some_and(|library| {
        ["folders", "placement", "trash"]
            .iter()
            .all(|key| library.get(key).is_some())
    })
}

fn remove_projects(root: &Path, root_made_by_app: bool) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for dir in entries.flatten().map(|e| e.path()).filter(|p| is_project_dir(p)) {
        let _ = std::fs::remove_dir_all(&dir);
    }
    let library = root.join("library.json");
    if is_library_file(&library) {
        let _ = std::fs::remove_file(&library);
    }
    if root_made_by_app {
        let _ = std::fs::remove_dir(root);
    }
}

fn is_running(exe: &str) -> bool {
    let out = hidden_command("tasklist")
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
pub async fn uninstall(app: AppHandle, args: UninstallArgs) -> CmdResult<()> {
    let dir = read_install_location()
        .map(PathBuf::from)
        .or_else(exe_dir)
        .ok_or("설치 위치를 찾을 수 없어요")?;
    if is_running(APP_EXE) {
        return Err("실행 중인 Hoist를 닫고 다시 시도해 주세요".into());
    }
    let _ = std::fs::remove_file(start_menu_shortcut_path());
    let _ = std::fs::remove_file(desktop_shortcut_path());
    unregister();
    let projects = projects_dir(&app);
    if args.remove_settings {
        remove_settings();
    }
    if !args.keep_projects
        && let Some(root) = projects
    {
        remove_projects(&root, Some(&root) == default_projects_dir(&app).as_ref());
    }
    remove_files_except_uninstaller(&dir).map_err(err)?;
    remove_dir_later(&dir).map_err(err)?;
    app.exit(0);
    Ok(())
}

fn remove_settings() {
    if let Some(appdata) = env_dir("APPDATA") {
        let _ = std::fs::remove_dir_all(appdata.join(APP_IDENTIFIER));
    }
    if let Some(local) = env_dir("LOCALAPPDATA") {
        let _ = std::fs::remove_dir_all(local.join(APP_IDENTIFIER));
    }
    // keyring names the credential `{user}.{service}`; the app saves user "openai", service "hoist".
    let _ = hidden_command("cmdkey").args(["/delete:openai.hoist"]).output();
}

fn remove_files_except_uninstaller(dir: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)?.flatten() {
        let p = entry.path();
        if is_uninstaller(&p) {
            continue;
        }
        let _ = if p.is_dir() {
            std::fs::remove_dir_all(&p)
        } else {
            std::fs::remove_file(&p)
        };
    }
    Ok(())
}

// uninstall.exe cannot delete itself while it runs, and the WebView holds the folder briefly after exit.
fn remove_dir_later(dir: &Path) -> std::io::Result<Child> {
    use std::os::windows::process::CommandExt;
    let d = dir.to_string_lossy();
    let script = format!(
        "for /l %i in (1,1,20) do (ping -n 2 127.0.0.1 > nul & rmdir /s /q \"{d}\" 2> nul & if not exist \"{d}\" exit)"
    );
    // raw_arg: the default quoting turns the inner quotes into \" which cmd does not understand.
    hidden_command("cmd")
        .arg("/C")
        .raw_arg(&script)
        .current_dir(std::env::temp_dir())
        .spawn()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_install_folder_is_removed_after_exit() {
        let dir = std::env::temp_dir().join(format!("hoist uninstall {}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).expect("dir");
        std::fs::write(dir.join("uninstall.exe"), b"x").expect("file");
        let mut child = remove_dir_later(&dir).expect("spawn");
        child.wait().expect("wait");
        assert!(!dir.exists(), "a folder with a space in its path is gone");
    }

    const PROJECT_ID: &str = "0123456789abcdef0123456789abcdef";

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("hoist-setup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("root");
        root
    }

    fn write_project(root: &Path, folder: &str, id: &str) -> PathBuf {
        let dir = root.join(folder);
        std::fs::create_dir_all(dir.join("assets")).expect("project dir");
        let project = serde_json::json!({ "id": id, "name": "banner" }).to_string();
        std::fs::write(dir.join("project.json"), project).expect("project.json");
        std::fs::write(dir.join("document.json"), b"{}").expect("document.json");
        std::fs::write(dir.join("assets/a.png"), b"png").expect("asset");
        dir
    }

    #[test]
    fn only_a_folder_hoist_wrote_counts_as_a_project() {
        let root = temp_root("is-project");
        assert!(is_project_dir(&write_project(&root, PROJECT_ID, PROJECT_ID)));

        assert!(!is_project_dir(&write_project(&root, "my-app", "my-app")));
        let renamed = "f".repeat(32);
        assert!(
            !is_project_dir(&write_project(&root, &renamed, PROJECT_ID)),
            "project.json names another project"
        );
        let no_document = "1".repeat(32);
        let dir = write_project(&root, &no_document, &no_document);
        std::fs::remove_file(dir.join("document.json")).expect("remove");
        assert!(!is_project_dir(&dir));
        let not_json = "2".repeat(32);
        let dir = write_project(&root, &not_json, &not_json);
        std::fs::write(dir.join("project.json"), b"id").expect("write");
        assert!(!is_project_dir(&dir));
        let empty = root.join("3".repeat(32));
        std::fs::create_dir_all(&empty).expect("dir");
        assert!(!is_project_dir(&empty));
        let file = root.join("4".repeat(32));
        std::fs::write(&file, b"x").expect("file");
        assert!(!is_project_dir(&file));
        assert!(!is_project_dir(&root));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn only_hoist_projects_are_removed() {
        let root = temp_root("mixed");
        let project = write_project(&root, PROJECT_ID, PROJECT_ID);
        let lookalike = write_project(&root, "my-app", "my-app");
        let library = serde_json::json!({ "folders": [], "placement": {}, "trash": {} });
        std::fs::write(root.join("library.json"), library.to_string()).expect("library");
        std::fs::create_dir_all(root.join("notes")).expect("notes");
        std::fs::write(root.join("notes/keep.txt"), b"mine").expect("keep");
        std::fs::write(root.join("report.json"), b"{}").expect("report");

        remove_projects(&root, true);

        assert!(!project.exists());
        assert!(!root.join("library.json").exists());
        assert!(lookalike.join("project.json").is_file());
        assert!(lookalike.join("assets/a.png").is_file());
        assert!(root.join("notes/keep.txt").is_file());
        assert!(root.join("report.json").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_library_file_from_another_program_stays() {
        let root = temp_root("foreign-library");
        for foreign in [
            serde_json::json!({ "books": [] }),
            serde_json::json!({ "folders": ["D:\\music"] }),
            serde_json::json!([]),
        ] {
            std::fs::write(root.join("library.json"), foreign.to_string()).expect("library");
            remove_projects(&root, true);
            assert!(root.join("library.json").is_file(), "{foreign}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn only_the_emptied_default_folder_is_removed() {
        let default = temp_root("default-root");
        write_project(&default, PROJECT_ID, PROJECT_ID);
        remove_projects(&default, true);
        assert!(!default.exists());

        let chosen = temp_root("chosen-root");
        let project = write_project(&chosen, PROJECT_ID, PROJECT_ID);
        remove_projects(&chosen, false);
        assert!(!project.exists());
        assert!(chosen.is_dir(), "a folder the user picked stays");
        let _ = std::fs::remove_dir_all(&chosen);
    }

    #[test]
    fn the_projects_folder_comes_from_saved_settings() {
        let dir = temp_root("settings");
        let settings = dir.join("settings.json");
        let default = dir.join("Documents").join("Hoist");
        let resolve = || resolve_projects_dir(&settings, Some(default.clone()));
        assert_eq!(resolve(), Some(default.clone()), "no settings file");
        assert_eq!(resolve_projects_dir(&settings, None), None);

        let write = |json: serde_json::Value| {
            std::fs::write(&settings, json.to_string()).expect("settings");
        };
        write(serde_json::json!({ "agent_model": "m" }));
        assert_eq!(resolve(), Some(default.clone()), "no projects_dir key");

        let chosen = dir.join("banners");
        write(serde_json::json!({ "projects_dir": chosen }));
        assert_eq!(resolve(), Some(chosen));

        write(serde_json::json!({ "projects_dir": "banners" }));
        assert_eq!(resolve(), None, "a relative folder is never guessed at");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
