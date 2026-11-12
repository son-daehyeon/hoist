use std::io::Write;
use std::path::PathBuf;

use crate::install::{do_install, spawn_app};
use crate::paths::{APP_EXE, default_dir, desktop_shortcut_path, env_dir};
use crate::registry::read_install_location;
use crate::uninstall::is_uninstall_mode;

pub(crate) struct Silent {
    relaunch: bool,
    dir: Option<PathBuf>,
}

pub(crate) fn silent_mode() -> Option<Silent> {
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

pub(crate) fn run_silent(flags: Silent) -> i32 {
    let dir = flags
        .dir
        .or_else(|| read_install_location().map(PathBuf::from))
        .unwrap_or_else(default_dir);
    let exe = dir.join(APP_EXE);
    let desktop_shortcut = desktop_shortcut_path().exists();
    let (code, problem) = match do_install(&dir, desktop_shortcut, &|_, _, _| {}) {
        Ok(done) => (i32::from(done.resources_missing), done.resources_error),
        Err(e) => (1, Some(format!("{e:#}"))),
    };
    if let Some(problem) = problem {
        log_line(&format!("{} exit={code}: {problem}", dir.display()));
    }
    if flags.relaunch && exe.exists() {
        let _ = spawn_app(&dir);
    }
    code
}

fn log_line(message: &str) {
    let Some(dir) = env_dir("LOCALAPPDATA").map(|d| d.join("com.hoist.installer")) else {
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
}
