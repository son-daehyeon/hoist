use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::paths::exe_dir;

const RESOURCE_MANIFEST_JSON: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/payload/resources.json"));
const RESOURCES_STAMP: &str = "resources.sha256";
const RESOURCE_DIRS: [&str; 2] = ["fonts", "models"];
const RELEASES_URL: &str = "https://github.com/son-daehyeon/hoist/releases/download";
const MB: u64 = 1_048_576;
pub(crate) const STEP_RESOURCES: u8 = 4;
const RESOURCES_START_PERCENT: u8 = 42;

#[derive(Deserialize, Default)]
#[serde(default)]
struct ResourceManifest {
    url_name: String,
    sha256: String,
    size: u64,
    version: String,
}

fn resource_manifest() -> serde_json::Result<ResourceManifest> {
    serde_json::from_str(RESOURCE_MANIFEST_JSON)
}

pub(crate) fn resources_present(dir: &Path) -> bool {
    RESOURCE_DIRS.iter().all(|d| dir.join(d).is_dir())
}

pub(crate) fn update_resources(dir: &Path, progress: &dyn Fn(u8, u8, &str)) -> anyhow::Result<()> {
    let manifest = resource_manifest().context("설치 파일의 글꼴·모델 정보가 잘못됐어요")?;
    if manifest.sha256.is_empty() {
        return Ok(());
    }
    let stamp = dir.join(RESOURCES_STAMP);
    let current = std::fs::read_to_string(&stamp).unwrap_or_default();
    if current.trim() == manifest.sha256 && resources_present(dir) {
        progress(STEP_RESOURCES, 100, "");
        return Ok(());
    }
    progress(STEP_RESOURCES, RESOURCES_START_PERCENT, "");
    let beside_installer = exe_dir()
        .map(|d| d.join(&manifest.url_name))
        .filter(|p| p.is_file() && sha256_file(p).ok().as_deref() == Some(manifest.sha256.as_str()));
    let (zip_path, downloaded) = match beside_installer {
        Some(p) => (p, false),
        None => (download(dir, &manifest, progress)?, true),
    };
    let unpacked = unpack(&zip_path, dir, progress);
    if downloaded {
        let _ = std::fs::remove_file(&zip_path);
    }
    unpacked?;
    std::fs::write(&stamp, &manifest.sha256).context("글꼴·모델 기록")?;
    Ok(())
}

fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(hasher))
}

fn hex(hasher: Sha256) -> String {
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn download(
    dir: &Path,
    manifest: &ResourceManifest,
    progress: &dyn Fn(u8, u8, &str),
) -> anyhow::Result<PathBuf> {
    let base = std::env::var("HOIST_RESOURCES_BASE")
        .unwrap_or_else(|_| format!("{RELEASES_URL}/v{}", manifest.version));
    let url = format!("{}/{}", base.trim_end_matches('/'), manifest.url_name);
    let offline = "인터넷 연결을 확인하고 다시 시도해 주세요.";
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(60))
        .build()?;
    let mut resp = client.get(&url).send().context(offline)?;
    if !resp.status().is_success() {
        bail!("{offline} (HTTP {})", resp.status().as_u16());
    }
    let total = resp.content_length().unwrap_or(manifest.size).max(1);
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
            let pct = RESOURCES_START_PERCENT + (done.min(total) * 43 / total) as u8;
            progress(STEP_RESOURCES, pct, &format!("{}/{}MB", done / MB, total / MB));
        }
    };
    drop(file);
    let valid = received.is_ok() && hex(hasher) == manifest.sha256;
    if !valid {
        let _ = std::fs::remove_file(&tmp);
        received.context(offline)?;
        bail!("받은 파일이 손상됐어요. 다시 시도해 주세요.");
    }
    Ok(tmp)
}

fn unpack(zip_path: &Path, dir: &Path, progress: &dyn Fn(u8, u8, &str)) -> anyhow::Result<()> {
    let staging = dir.join("resources.new");
    let _ = std::fs::remove_dir_all(&staging);
    let mut archive =
        zip::ZipArchive::new(File::open(zip_path)?).context("글꼴·모델 파일을 읽을 수 없어요")?;
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
        progress(STEP_RESOURCES, 85 + (i * 14 / total) as u8, "글꼴·모델 푸는 중");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_from_pack_resources_parses() {
        let m: ResourceManifest = serde_json::from_str(
            r#"{"url_name":"Hoist-Resources-b4d760d3.zip","sha256":"b4d7","size":289907061,"version":"0.1.0"}"#,
        )
        .expect("manifest");
        assert_eq!((m.size, m.version.as_str()), (289907061, "0.1.0"));
        assert!(
            serde_json::from_str::<ResourceManifest>("{}")
                .expect("placeholder")
                .sha256
                .is_empty()
        );
        assert!(serde_json::from_str::<ResourceManifest>(r#"{"size": 1.0}"#).is_err());
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
