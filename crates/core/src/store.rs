use std::collections::HashMap;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use chrono::{SecondsFormat, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::*;
use crate::render::RenderCache;
use crate::{CoreError, Result};

mod history;
mod library;
mod versions;

pub use library::Folder;

const PROJECT_FILE: &str = "project.json";
const DOCUMENT_FILE: &str = "document.json";
const STATE_FILE: &str = "state.json";
const OPS_FILE: &str = "ops.jsonl";
const CHAT_FILE: &str = "chat.jsonl";
const MEMORY_FILE: &str = "memory.md";
const LIBRARY_FILE: &str = "library.json";
const THUMBNAIL_FILE: &str = "thumbnail.png";
const VERSIONS_DIR: &str = "versions";
const VERSION_INDEX_FILE: &str = "index.json";
const ORIGINALS_DIR: &str = "originals";
const ASSETS_DIR: &str = "assets";
const ANALYSIS_DIR: &str = "analysis";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub updated_at: String,
    pub thumbnail: Option<PathBuf>,
    pub folder: Option<String>,
    pub deleted_at: Option<String>,
}

pub struct ProjectStore {
    root: PathBuf,
}

pub struct ProjectHandle {
    pub dir: PathBuf,
    pub project: Project,
    pub document: Document,
    undo: Vec<Vec<OpEntry>>,
    redo: Vec<Vec<OpEntry>>,
    next_seq: u64,
    next_batch: u64,
    pub render_cache: RenderCache,
}

#[derive(Serialize, Deserialize, Default)]
struct HistoryState {
    next_seq: u64,
    next_batch: u64,
    undo: Vec<[u64; 2]>,
    redo: Vec<[u64; 2]>,
}

impl ProjectStore {
    pub fn new(root: PathBuf) -> Self {
        ProjectStore { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn list(&self) -> Result<Vec<ProjectSummary>> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let library = self.library()?;
        let mut out = Vec::new();
        for entry in entries {
            let dir = entry?.path();
            let Ok(p) = read_json::<Project>(&dir.join(PROJECT_FILE)) else {
                continue;
            };
            let thumbnail = dir.join(THUMBNAIL_FILE);
            out.push(ProjectSummary {
                folder: library.placement.get(&p.id).filter(|f| library.has(f)).cloned(),
                deleted_at: library.trash.get(&p.id).cloned(),
                id: p.id,
                name: p.name,
                width_mm: p.width_mm,
                height_mm: p.height_mm,
                updated_at: p.updated_at,
                thumbnail: thumbnail.is_file().then_some(thumbnail),
            });
        }
        out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(out)
    }

    pub fn create(&self, name: &str, width_mm: f64, height_mm: f64) -> Result<ProjectHandle> {
        if !(width_mm > 0.0 && height_mm > 0.0) {
            return Err(CoreError::invalid("project size must be > 0"));
        }
        let id = uuid::Uuid::new_v4().simple().to_string();
        let dir = self.root.join(&id);
        for sub in [VERSIONS_DIR, ORIGINALS_DIR, ASSETS_DIR] {
            fs::create_dir_all(dir.join(sub))?;
        }
        let ts = now();
        let mut handle = ProjectHandle {
            dir,
            project: Project {
                id,
                name: name.to_string(),
                created_at: ts.clone(),
                updated_at: ts,
                width_mm,
                height_mm,
                target_dpi: 100.0,
                originals: Vec::new(),
                fit_mode: FitMode::default(),
                usage_krw: 0.0,
                usage_text_krw: 0.0,
                usage_image_krw: 0.0,
            },
            document: Document::new(width_mm, height_mm),
            undo: Vec::new(),
            redo: Vec::new(),
            next_seq: 1,
            next_batch: 1,
            render_cache: RenderCache::default(),
        };
        handle.save()?;
        Ok(handle)
    }

    pub fn open(&self, id: &str) -> Result<ProjectHandle> {
        let dir = self.project_dir(id)?;
        let mut project: Project = read_json(&dir.join(PROJECT_FILE))?;
        project.split_legacy_usage();
        let document: Document = read_json(&dir.join(DOCUMENT_FILE))?;
        let entries: Vec<OpEntry> = read_lines(&dir.join(OPS_FILE))?;
        let state: HistoryState = read_json_opt(&dir.join(STATE_FILE))?.unwrap_or_default();
        let last = entries.iter().map(|e| e.seq).max().unwrap_or(0);
        let by_seq: HashMap<u64, OpEntry> = entries.into_iter().map(|e| (e.seq, e)).collect();
        let rebuild = |ranges: &[[u64; 2]]| -> Vec<Vec<OpEntry>> {
            ranges
                .iter()
                .map(|[a, b]| {
                    (*a..=*b)
                        .filter_map(|s| by_seq.get(&s).cloned())
                        .collect::<Vec<_>>()
                })
                .filter(|step| !step.is_empty())
                .collect()
        };
        Ok(ProjectHandle {
            undo: rebuild(&state.undo),
            redo: rebuild(&state.redo),
            next_seq: state.next_seq.max(last + 1),
            next_batch: state.next_batch.max(1),
            render_cache: RenderCache::default(),
            dir,
            project,
            document,
        })
    }

    /// Removes the project's files for good. The home screen's delete is `trash`.
    pub fn delete(&self, id: &str) -> Result<()> {
        fs::remove_dir_all(self.project_dir(id)?)?;
        let mut library = self.library()?;
        let placed = library.placement.remove(id).is_some();
        let trashed = library.trash.remove(id).is_some();
        if placed || trashed {
            self.save_library(&library)?;
        }
        Ok(())
    }

    pub fn duplicate(&self, id: &str, name: &str) -> Result<ProjectHandle> {
        let src = self.project_dir(id)?;
        let project: Project = read_json(&src.join(PROJECT_FILE))?;
        let mut handle = self.create(name, project.width_mm, project.height_mm)?;
        for sub in [ASSETS_DIR, ORIGINALS_DIR, ANALYSIS_DIR] {
            if src.join(sub).is_dir() {
                copy_dir(&src.join(sub), &handle.dir.join(sub))?;
            }
        }
        for file in [THUMBNAIL_FILE, MEMORY_FILE] {
            if src.join(file).is_file() {
                fs::copy(src.join(file), handle.dir.join(file))?;
            }
        }
        handle.document = read_json(&src.join(DOCUMENT_FILE))?;
        handle.project.originals = project.originals;
        handle.project.fit_mode = project.fit_mode;
        handle.project.target_dpi = project.target_dpi;
        handle.save()?;
        Ok(handle)
    }

    fn project_dir(&self, id: &str) -> Result<PathBuf> {
        let dir = self.root.join(id);
        let safe = !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
        if !safe || !dir.join(PROJECT_FILE).is_file() {
            return Err(CoreError::ProjectNotFound(id.to_string()));
        }
        Ok(dir)
    }
}

impl ProjectHandle {
    pub fn save(&mut self) -> Result<()> {
        self.project.updated_at = now();
        write_json(&self.dir.join(PROJECT_FILE), &self.project)?;
        write_json(&self.dir.join(DOCUMENT_FILE), &self.document)?;
        let range = |step: &Vec<OpEntry>| {
            [
                step.first().map_or(0, |e| e.seq),
                step.last().map_or(0, |e| e.seq),
            ]
        };
        let state = HistoryState {
            next_seq: self.next_seq,
            next_batch: self.next_batch,
            undo: self.undo.iter().map(range).collect(),
            redo: self.redo.iter().map(range).collect(),
        };
        write_json(&self.dir.join(STATE_FILE), &state)
    }

    pub fn rename(&mut self, name: &str) -> Result<()> {
        self.project.name = name.to_string();
        self.save()
    }

    pub fn add_usage(&mut self, text_krw: f64, image_krw: f64) -> Result<()> {
        self.project.split_legacy_usage();
        self.project.usage_text_krw += text_krw;
        self.project.usage_image_krw += image_krw;
        self.project.usage_krw += text_krw + image_krw;
        write_json(&self.dir.join(PROJECT_FILE), &self.project)
    }

    pub fn set_fit_mode(&mut self, mode: FitMode) -> Result<()> {
        self.project.fit_mode = mode;
        self.save()
    }

    pub fn add_original(&mut self, src: &Path) -> Result<OriginalInfo> {
        let img = crate::image::load(src)?;
        let version = self
            .project
            .originals
            .iter()
            .map(|o| o.version)
            .max()
            .unwrap_or(0)
            + 1;
        fs::create_dir_all(self.dir.join(ORIGINALS_DIR))?;
        crate::image::save_png(&img, &self.original_path(version))?;
        let info = OriginalInfo {
            version,
            file: format!("{ORIGINALS_DIR}/v{version}.png"),
            width_px: img.width(),
            height_px: img.height(),
            uploaded_at: now(),
        };
        self.project.originals.push(info.clone());
        self.save()?;
        Ok(info)
    }

    pub fn original_path(&self, version: u32) -> PathBuf {
        self.dir.join(ORIGINALS_DIR).join(format!("v{version}.png"))
    }

    pub fn assets_dir(&self) -> PathBuf {
        self.dir.join(ASSETS_DIR)
    }

    pub fn asset_path(&self, name: &str) -> PathBuf {
        self.assets_dir().join(name)
    }

    pub fn import_image(&self, src: &Path) -> Result<(String, u32, u32)> {
        use ::image::metadata::Orientation;
        use ::image::{DynamicImage, ImageDecoder, ImageFormat};

        let reader = ::image::ImageReader::open(src)?.with_guessed_format()?;
        let format = reader.format();
        fs::create_dir_all(self.assets_dir())?;
        let copy = |ext: &str| -> Result<String> {
            let name = self.new_asset_name("import", ext);
            fs::copy(src, self.asset_path(&name))?;
            Ok(name)
        };
        if format == Some(ImageFormat::Png) {
            let (w, h) = reader.into_dimensions()?;
            return Ok((copy("png")?, w, h));
        }
        let mut decoder = reader.into_decoder()?;
        let orientation = decoder.orientation()?;
        let jpeg = format == Some(ImageFormat::Jpeg);
        if jpeg && orientation == Orientation::NoTransforms {
            let (w, h) = decoder.dimensions();
            return Ok((copy("jpg")?, w, h));
        }
        let mut img = DynamicImage::from_decoder(decoder)?;
        img.apply_orientation(orientation);
        let name = self.new_asset_name("import", if jpeg { "jpg" } else { "png" });
        if jpeg {
            let file = std::io::BufWriter::new(fs::File::create(self.asset_path(&name))?);
            ::image::codecs::jpeg::JpegEncoder::new_with_quality(file, crate::image::JPEG_QUALITY)
                .encode_image(&img.to_rgb8())?;
        } else {
            crate::image::save_png(&img.to_rgba8(), &self.asset_path(&name))?;
        }
        Ok((name, img.width(), img.height()))
    }

    pub fn new_asset_name(&self, prefix: &str, ext: &str) -> String {
        (1u32..)
            .map(|n| format!("{prefix}_{n:03}.{ext}"))
            .find(|name| !self.asset_path(name).exists())
            .unwrap_or_default()
    }

    pub fn memory(&self) -> Result<String> {
        match fs::read_to_string(self.dir.join(MEMORY_FILE)) {
            Ok(text) => Ok(text),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn set_memory(&self, text: &str) -> Result<()> {
        write_atomic(&self.dir.join(MEMORY_FILE), text.as_bytes())
    }

    pub fn chat(&self) -> Result<Vec<serde_json::Value>> {
        read_lines(&self.dir.join(CHAT_FILE))
    }

    pub fn append_chat(&self, item: &serde_json::Value) -> Result<()> {
        append_lines(&self.dir.join(CHAT_FILE), std::slice::from_ref(item))
    }

    pub fn replace_chat(&self, items: &[serde_json::Value]) -> Result<()> {
        write_atomic(&self.dir.join(CHAT_FILE), to_lines(items)?.as_bytes())
    }

    pub fn update_chat(&self, id: &str, item: &serde_json::Value) -> Result<()> {
        let mut items = self.chat()?;
        for existing in items
            .iter_mut()
            .filter(|v| v.get("id").and_then(Value::as_str) == Some(id))
        {
            *existing = item.clone();
        }
        self.replace_chat(&items)
    }

    pub fn write_thumbnail(&self, png: &[u8]) -> Result<()> {
        write_atomic(&self.thumbnail_path(), png)
    }

    pub fn thumbnail_path(&self) -> PathBuf {
        self.dir.join(THUMBNAIL_FILE)
    }
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn read_json_opt<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    write_atomic(path, &serde_json::to_vec_pretty(value)?)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn read_lines<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>> {
    match fs::read_to_string(path) {
        Ok(text) => text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| Ok(serde_json::from_str(l)?))
            .collect(),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}

fn to_lines<T: Serialize>(items: &[T]) -> Result<String> {
    let mut out = String::new();
    for item in items {
        out.push_str(&serde_json::to_string(item)?);
        out.push('\n');
    }
    Ok(out)
}

fn append_lines<T: Serialize>(path: &Path, items: &[T]) -> Result<()> {
    let text = to_lines(items)?;
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(text.as_bytes())?;
    Ok(())
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{TempRoot, layer, seeded};
    use serde_json::json;

    #[test]
    fn import_image_copies_png_jpg_and_converts_the_rest() {
        let root = TempRoot::new();
        let h = root.store().create("p", 100.0, 100.0).expect("create");
        let img = ::image::RgbaImage::from_pixel(5, 3, ::image::Rgba([10, 20, 30, 255]));
        let png = root.0.join("logo.PNG");
        img.save(&png).expect("png");
        let gif = root.0.join("anim.gif");
        img.save(&gif).expect("gif");
        let jpg_as_bin = root.0.join("photo.bin");
        ::image::DynamicImage::ImageRgba8(img.clone())
            .to_rgb8()
            .save_with_format(&jpg_as_bin, ::image::ImageFormat::Jpeg)
            .expect("jpg");

        let (name, w, hgt) = h.import_image(&png).expect("import png");
        assert_eq!((name.as_str(), w, hgt), ("import_001.png", 5, 3));
        assert_eq!(
            fs::read(h.asset_path(&name)).expect("read"),
            fs::read(&png).expect("read")
        );
        let (name, ..) = h.import_image(&jpg_as_bin).expect("import jpg");
        assert_eq!(name, "import_001.jpg");
        assert_eq!(
            fs::read(h.asset_path(&name)).expect("read"),
            fs::read(&jpg_as_bin).expect("read")
        );
        let (name, w, _) = h.import_image(&gif).expect("import gif");
        assert_eq!((name.as_str(), w), ("import_002.png", 5));
        let back = crate::image::load(&h.asset_path(&name)).expect("load");
        assert_eq!(back.get_pixel(0, 0).0, [10, 20, 30, 255]);
        assert!(h.import_image(&root.0.join("missing.png")).is_err());
    }

    #[test]
    fn import_jpg_applies_exif_orientation() {
        use ::image::ImageEncoder;

        let root = TempRoot::new();
        let h = root.store().create("p", 100.0, 100.0).expect("create");
        let rgb = ::image::RgbImage::from_fn(32, 16, |x, _| {
            ::image::Rgb(if x < 16 { [255, 0, 0] } else { [0, 0, 255] })
        });
        let rotate90 = vec![
            0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00, 0x01, 0x00, 0x12, 0x01, 0x03, 0x00, 0x01, 0x00,
            0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let jpg = root.0.join("phone.jpg");
        let mut bytes = Vec::new();
        let mut enc = ::image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 95);
        enc.set_exif_metadata(rotate90).expect("exif");
        enc.encode_image(&rgb).expect("encode");
        fs::write(&jpg, &bytes).expect("write");

        let (name, w, hgt) = h.import_image(&jpg).expect("import");
        assert_eq!((name.as_str(), w, hgt), ("import_001.jpg", 16, 32));
        let back = crate::image::load(&h.asset_path(&name)).expect("load");
        assert_eq!(back.dimensions(), (16, 32));
        let near = |p: [u8; 4], q: [u8; 3]| p.iter().zip(q).all(|(a, b)| a.abs_diff(b) < 40);
        assert!(
            near(back.get_pixel(8, 4).0, [255, 0, 0]),
            "{:?}",
            back.get_pixel(8, 4)
        );
        assert!(
            near(back.get_pixel(8, 28).0, [0, 0, 255]),
            "{:?}",
            back.get_pixel(8, 28)
        );
    }

    #[test]
    fn usage_before_the_split_counts_as_text() {
        let root = TempRoot::new();
        let store = root.store();
        let mut h = store.create("u", 100.0, 100.0).expect("create");
        h.project.usage_krw = 50.0;
        h.save().expect("save");
        let mut h = store.open(&h.project.id.clone()).expect("open");
        assert_eq!((h.project.usage_text_krw, h.project.usage_image_krw), (50.0, 0.0));
        h.add_usage(1.0, 2.0).expect("add");
        assert_eq!(
            (
                h.project.usage_krw,
                h.project.usage_text_krw,
                h.project.usage_image_krw
            ),
            (53.0, 51.0, 2.0)
        );
    }

    #[test]
    fn create_list_open_delete_duplicate() {
        let root = TempRoot::new();
        let store = root.store();
        let h = store.create("첫 프로젝트", 900.0, 300.0).expect("create");
        assert_eq!(h.project.id.len(), 32);
        assert_eq!(h.document, Document::new(900.0, 300.0));
        assert!(chrono::DateTime::parse_from_rfc3339(&h.project.created_at).is_ok());
        h.write_thumbnail(&[1, 2, 3]).expect("thumb");
        let mut h = h;
        h.add_usage(100.0, 20.0).expect("usage");
        assert_eq!(
            (
                h.project.usage_krw,
                h.project.usage_text_krw,
                h.project.usage_image_krw
            ),
            (120.0, 100.0, 20.0)
        );
        h.append_chat(&json!({"id": "c1", "role": "user", "text": "안녕"}))
            .expect("chat");
        fs::write(h.asset_path("a.png"), [9]).expect("asset");
        h.set_memory("글꼴은 Pretendard").expect("memory");
        fs::create_dir_all(h.dir.join("analysis")).expect("analysis dir");
        fs::write(h.dir.join("analysis/v1.json"), "{}").expect("analysis");
        h.document
            .layers
            .push(layer("L1", "배경", None, LayerKind::Group));
        h.save().expect("save");
        h.snapshot(Actor::User, "v1", None, None).expect("snapshot");

        let copy = store.duplicate(&h.project.id, "복제본").expect("duplicate");
        assert_eq!(copy.project.name, "복제본");
        assert_ne!(copy.project.id, h.project.id);
        assert_eq!(copy.project.usage_krw, 0.0);
        assert_eq!(copy.document, h.document);
        assert_eq!(fs::read(copy.asset_path("a.png")).expect("asset"), [9]);
        assert_eq!(fs::read(copy.thumbnail_path()).expect("thumb"), [1, 2, 3]);
        assert_eq!(copy.memory().expect("memory"), "글꼴은 Pretendard");
        assert_eq!(
            fs::read_to_string(copy.dir.join("analysis/v1.json")).expect("analysis"),
            "{}"
        );
        assert!(copy.chat().expect("chat").is_empty());
        assert!(copy.versions().expect("versions").is_empty());
        assert!(!copy.can_undo());

        let list = store.list().expect("list");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, copy.project.id);
        assert!(list.iter().all(|p| p.thumbnail.is_some()));

        store.delete(&h.project.id).expect("delete");
        assert!(matches!(
            store.open(&h.project.id),
            Err(CoreError::ProjectNotFound(_))
        ));
        assert!(matches!(store.delete(".."), Err(CoreError::ProjectNotFound(_))));
        assert_eq!(store.list().expect("list").len(), 1);
    }

    #[test]
    fn originals_assets_chat_memory() {
        let root = TempRoot::new();
        let mut h = seeded(&root.store());
        let src = root.0.join("in.jpg");
        image::RgbImage::from_pixel(40, 20, image::Rgb([200, 10, 10]))
            .save(&src)
            .expect("jpg");
        let info = h.add_original(&src).expect("original");
        assert_eq!((info.version, info.width_px, info.height_px), (1, 40, 20));
        assert!(h.original_path(1).is_file());
        assert_eq!(h.add_original(&src).expect("v2").version, 2);
        assert_eq!(h.project.originals.len(), 2);

        assert_eq!(h.new_asset_name("crop", "png"), "crop_001.png");
        fs::write(h.asset_path("crop_001.png"), b"x").expect("asset");
        assert_eq!(h.new_asset_name("crop", "png"), "crop_002.png");

        assert!(h.chat().expect("chat").is_empty());
        h.append_chat(&json!({"id": "a", "text": "안녕"}))
            .expect("append");
        h.append_chat(&json!({"id": "b", "text": "둘"})).expect("append");
        h.update_chat("a", &json!({"id": "a", "text": "수정"}))
            .expect("update");
        assert_eq!(
            h.chat().expect("chat"),
            [
                json!({"id": "a", "text": "수정"}),
                json!({"id": "b", "text": "둘"})
            ]
        );
        h.replace_chat(&[json!({"id": "c"})]).expect("replace");
        assert_eq!(h.chat().expect("chat").len(), 1);

        assert_eq!(h.memory().expect("memory"), "");
        h.set_memory("글꼴: 본고딕").expect("set");
        assert_eq!(h.memory().expect("memory"), "글꼴: 본고딕");
    }
}
