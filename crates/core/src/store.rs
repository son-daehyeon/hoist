use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use chrono::{SecondsFormat, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::*;
use crate::ops::{apply_op, diff, patch_paths, sibling_index, subtree};
use crate::render::RenderCache;
use crate::{CoreError, Result};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub updated_at: String,
    pub thumbnail: Option<PathBuf>,
    /// The folder it is shown in; `None` at the top level.
    pub folder: Option<String>,
    /// When it was moved to the trash.
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
struct State {
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
            let Ok(p) = read_json::<Project>(&dir.join("project.json")) else {
                continue;
            };
            let thumbnail = dir.join("thumbnail.png");
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
            return Err(CoreError::InvalidOp("project size must be > 0".into()));
        }
        let id = uuid::Uuid::new_v4().simple().to_string();
        let dir = self.root.join(&id);
        for sub in ["versions", "originals", "assets"] {
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
        let mut project: Project = read_json(&dir.join("project.json"))?;
        project.split_legacy_usage();
        let document: Document = read_json(&dir.join("document.json"))?;
        let entries: Vec<OpEntry> = read_lines(&dir.join("ops.jsonl"))?;
        let state: State = read_json_opt(&dir.join("state.json"))?.unwrap_or_default();
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
        if library.placement.remove(id).is_some() | library.trash.remove(id).is_some() {
            self.save_library(&library)?;
        }
        Ok(())
    }

    pub fn duplicate(&self, id: &str, name: &str) -> Result<ProjectHandle> {
        let src = self.project_dir(id)?;
        let project: Project = read_json(&src.join("project.json"))?;
        let mut handle = self.create(name, project.width_mm, project.height_mm)?;
        for sub in ["assets", "originals", "analysis"] {
            if src.join(sub).is_dir() {
                copy_dir(&src.join(sub), &handle.dir.join(sub))?;
            }
        }
        for file in ["thumbnail.png", "memory.md"] {
            if src.join(file).is_file() {
                fs::copy(src.join(file), handle.dir.join(file))?;
            }
        }
        handle.document = read_json(&src.join("document.json"))?;
        handle.project.originals = project.originals;
        handle.project.fit_mode = project.fit_mode;
        handle.project.target_dpi = project.target_dpi;
        handle.save()?;
        Ok(handle)
    }

    fn project_dir(&self, id: &str) -> Result<PathBuf> {
        let dir = self.root.join(id);
        let safe = !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
        if !safe || !dir.join("project.json").is_file() {
            return Err(CoreError::ProjectNotFound(id.to_string()));
        }
        Ok(dir)
    }
}

/// A folder on the home screen. Folders are virtual: project directories stay flat under the projects
/// root, named by id, and the library file says where each one is shown.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, specta::Type)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub parent: Option<String>,
}

/// Where projects sit on the home screen: folders and the trash. Kept beside the project directories
/// (`library.json`) instead of inside them, so moving or trashing a project touches neither its files nor
/// its modified time, and a project the file does not mention is simply at the top level.
#[derive(Serialize, Deserialize, Default)]
struct Library {
    #[serde(default)]
    folders: Vec<Folder>,
    #[serde(default)]
    placement: BTreeMap<String, String>,
    #[serde(default)]
    trash: BTreeMap<String, String>,
}

impl Library {
    fn has(&self, folder: &str) -> bool {
        self.folders.iter().any(|f| f.id == folder)
    }

    /// The folder and everything nested in it.
    fn subtree(&self, folder: &str) -> Vec<String> {
        let mut all = vec![folder.to_string()];
        let mut i = 0;
        while i < all.len() {
            let parent = all[i].clone();
            let children: Vec<String> = self
                .folders
                .iter()
                .filter(|f| f.parent.as_deref() == Some(parent.as_str()) && !all.contains(&f.id))
                .map(|f| f.id.clone())
                .collect();
            all.extend(children);
            i += 1;
        }
        all
    }
}

impl ProjectStore {
    fn library(&self) -> Result<Library> {
        Ok(read_json_opt(&self.root.join("library.json"))?.unwrap_or_default())
    }

    fn save_library(&self, library: &Library) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        write_json(&self.root.join("library.json"), library)
    }

    pub fn folders(&self) -> Result<Vec<Folder>> {
        Ok(self.library()?.folders)
    }

    pub fn create_folder(&self, name: &str, parent: Option<&str>) -> Result<Folder> {
        let mut library = self.library()?;
        if let Some(parent) = parent.filter(|p| !library.has(p)) {
            return Err(CoreError::InvalidOp(format!("folder {parent} not found")));
        }
        let folder = Folder {
            id: uuid::Uuid::new_v4().simple().to_string(),
            name: folder_name(name)?,
            parent: parent.map(str::to_string),
        };
        library.folders.push(folder.clone());
        self.save_library(&library)?;
        Ok(folder)
    }

    pub fn rename_folder(&self, id: &str, name: &str) -> Result<()> {
        let mut library = self.library()?;
        let name = folder_name(name)?;
        let folder = library
            .folders
            .iter_mut()
            .find(|f| f.id == id)
            .ok_or_else(|| CoreError::InvalidOp(format!("folder {id} not found")))?;
        folder.name = name;
        self.save_library(&library)
    }

    /// Removes the folder and everything nested in it; the projects that were inside go to the trash and
    /// come back to the top level if restored. Returns the ids of those projects.
    pub fn delete_folder(&self, id: &str) -> Result<Vec<String>> {
        let mut library = self.library()?;
        if !library.has(id) {
            return Err(CoreError::InvalidOp(format!("folder {id} not found")));
        }
        let gone = library.subtree(id);
        let inside: Vec<String> = library
            .placement
            .iter()
            .filter(|(_, folder)| gone.contains(folder))
            .map(|(project, _)| project.clone())
            .collect();
        let ts = now();
        for project in &inside {
            library.placement.remove(project);
            library.trash.entry(project.clone()).or_insert_with(|| ts.clone());
        }
        library.folders.retain(|f| !gone.contains(&f.id));
        self.save_library(&library)?;
        Ok(inside)
    }

    pub fn move_project(&self, id: &str, folder: Option<&str>) -> Result<()> {
        self.project_dir(id)?;
        let mut library = self.library()?;
        match folder {
            Some(folder) if library.has(folder) => {
                library.placement.insert(id.to_string(), folder.to_string());
            }
            Some(folder) => return Err(CoreError::InvalidOp(format!("folder {folder} not found"))),
            None => {
                library.placement.remove(id);
            }
        }
        self.save_library(&library)
    }

    /// Deleting on the home screen only moves a project here; its files stay until the trash is emptied.
    pub fn trash(&self, id: &str) -> Result<()> {
        self.project_dir(id)?;
        let mut library = self.library()?;
        library.trash.entry(id.to_string()).or_insert_with(now);
        self.save_library(&library)
    }

    pub fn restore(&self, id: &str) -> Result<()> {
        let mut library = self.library()?;
        library.trash.remove(id);
        self.save_library(&library)
    }

    /// Removes every trashed project for good. Returns their ids.
    pub fn empty_trash(&self) -> Result<Vec<String>> {
        let trashed: Vec<String> = self.library()?.trash.into_keys().collect();
        for id in &trashed {
            match self.delete(id) {
                Ok(()) | Err(CoreError::ProjectNotFound(_)) => {}
                Err(e) => return Err(e),
            }
        }
        let mut library = self.library()?;
        library.trash.clear();
        self.save_library(&library)?;
        Ok(trashed)
    }
}

fn folder_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CoreError::InvalidOp("a folder needs a name".into()));
    }
    Ok(name.to_string())
}

impl ProjectHandle {
    pub fn save(&mut self) -> Result<()> {
        self.project.updated_at = now();
        write_json(&self.dir.join("project.json"), &self.project)?;
        write_json(&self.dir.join("document.json"), &self.document)?;
        let range = |step: &Vec<OpEntry>| {
            [
                step.first().map_or(0, |e| e.seq),
                step.last().map_or(0, |e| e.seq),
            ]
        };
        let state = State {
            next_seq: self.next_seq,
            next_batch: self.next_batch,
            undo: self.undo.iter().map(range).collect(),
            redo: self.redo.iter().map(range).collect(),
        };
        write_json(&self.dir.join("state.json"), &state)
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
        write_json(&self.dir.join("project.json"), &self.project)
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
        fs::create_dir_all(self.dir.join("originals"))?;
        crate::image::save_png(&img, &self.original_path(version))?;
        let info = OriginalInfo {
            version,
            file: format!("originals/v{version}.png"),
            width_px: img.width(),
            height_px: img.height(),
            uploaded_at: now(),
        };
        self.project.originals.push(info.clone());
        self.save()?;
        Ok(info)
    }

    pub fn original_path(&self, version: u32) -> PathBuf {
        self.dir.join("originals").join(format!("v{version}.png"))
    }

    pub fn assets_dir(&self) -> PathBuf {
        self.dir.join("assets")
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
            ::image::codecs::jpeg::JpegEncoder::new_with_quality(file, 92).encode_image(&img.to_rgb8())?;
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

    pub fn apply_ops(&mut self, ops: Vec<Op>, actor: Actor, turn: Option<String>) -> Result<Vec<OpEntry>> {
        if ops.is_empty() {
            return Ok(Vec::new());
        }
        let new_batch = turn.is_none();
        let turn = turn.unwrap_or_else(|| format!("u{}", self.next_batch));
        let mut doc = self.document.clone();
        let mut applied = Vec::new();
        for op in ops {
            let expanded = match op {
                Op::DeleteLayer { id } => subtree(&doc.layers, &id)
                    .into_iter()
                    .rev()
                    .map(|id| Op::DeleteLayer { id })
                    .collect(),
                other => vec![other],
            };
            for op in expanded {
                let op = guard_pins(&doc, op, actor)?;
                let inverse = apply_op(&mut doc, &op)?;
                applied.push((op, inverse));
            }
        }
        let entries = self.record(doc, applied, actor, &turn)?;
        if new_batch {
            self.next_batch += 1;
        }
        let same_turn = !new_batch
            && self
                .undo
                .last()
                .and_then(|s| s.first())
                .and_then(|e| e.turn.as_deref())
                == Some(turn.as_str());
        match (same_turn, self.undo.last_mut()) {
            (true, Some(step)) => step.extend(entries.iter().cloned()),
            _ => self.undo.push(entries.clone()),
        }
        self.redo.clear();
        self.save()?;
        Ok(entries)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> Result<Option<Vec<OpEntry>>> {
        let Some(step) = self.undo.last().cloned() else {
            return Ok(None);
        };
        let entries = self.revert(&step, "undo")?;
        self.undo.pop();
        self.redo.push(entries.clone());
        self.save()?;
        Ok(Some(entries))
    }

    pub fn redo(&mut self) -> Result<Option<Vec<OpEntry>>> {
        let Some(step) = self.redo.last().cloned() else {
            return Ok(None);
        };
        let entries = self.revert(&step, "redo")?;
        self.redo.pop();
        self.undo.push(entries.clone());
        self.save()?;
        Ok(Some(entries))
    }

    pub fn snapshot(
        &mut self,
        actor: Actor,
        summary: &str,
        turn: Option<String>,
        thumbnail_png: Option<&[u8]>,
    ) -> Result<Version> {
        let seq = self.last_seq();
        let dir = self.dir.join("versions");
        fs::create_dir_all(&dir)?;
        write_json(&dir.join(format!("{seq}.json")), &self.document)?;
        if let Some(png) = thumbnail_png {
            write_atomic(&self.version_thumbnail_path(seq), png)?;
        }
        let version = Version {
            seq,
            ts: now(),
            actor,
            summary: summary.to_string(),
            turn,
        };
        let mut index: Vec<Version> = read_json_opt(&dir.join("index.json"))?.unwrap_or_default();
        index.retain(|v| v.seq != seq);
        index.push(version.clone());
        write_json(&dir.join("index.json"), &index)?;
        Ok(version)
    }

    pub fn versions(&self) -> Result<Vec<Version>> {
        let mut index: Vec<Version> =
            read_json_opt(&self.dir.join("versions").join("index.json"))?.unwrap_or_default();
        index.sort_by_key(|v| std::cmp::Reverse(v.seq));
        Ok(index)
    }

    pub fn version_document(&self, seq: u64) -> Result<Document> {
        read_json(&self.dir.join("versions").join(format!("{seq}.json")))
    }

    pub fn version_thumbnail_path(&self, seq: u64) -> PathBuf {
        self.dir.join("versions").join(format!("{seq}.png"))
    }

    pub fn restore_version(&mut self, seq: u64) -> Result<Vec<OpEntry>> {
        let target = self.version_document(seq)?;
        let ops = restore_ops(&self.document, &target)?;
        self.apply_ops(ops, Actor::User, Some(format!("restore:{seq}")))
    }

    pub fn memory(&self) -> Result<String> {
        match fs::read_to_string(self.dir.join("memory.md")) {
            Ok(text) => Ok(text),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn set_memory(&self, text: &str) -> Result<()> {
        write_atomic(&self.dir.join("memory.md"), text.as_bytes())
    }

    pub fn chat(&self) -> Result<Vec<serde_json::Value>> {
        read_lines(&self.dir.join("chat.jsonl"))
    }

    pub fn append_chat(&self, item: &serde_json::Value) -> Result<()> {
        append_lines(&self.dir.join("chat.jsonl"), std::slice::from_ref(item))
    }

    pub fn replace_chat(&self, items: &[serde_json::Value]) -> Result<()> {
        write_atomic(&self.dir.join("chat.jsonl"), to_lines(items)?.as_bytes())
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

    pub fn entries_since(&self, seq: u64) -> Result<Vec<OpEntry>> {
        let entries: Vec<OpEntry> = read_lines(&self.dir.join("ops.jsonl"))?;
        Ok(entries.into_iter().filter(|e| e.seq > seq).collect())
    }

    pub fn last_seq(&self) -> u64 {
        self.next_seq.saturating_sub(1)
    }

    pub fn write_thumbnail(&self, png: &[u8]) -> Result<()> {
        write_atomic(&self.thumbnail_path(), png)
    }

    pub fn thumbnail_path(&self) -> PathBuf {
        self.dir.join("thumbnail.png")
    }

    fn record(
        &mut self,
        doc: Document,
        applied: Vec<(Op, Op)>,
        actor: Actor,
        turn: &str,
    ) -> Result<Vec<OpEntry>> {
        let ts = now();
        let entries: Vec<OpEntry> = applied
            .into_iter()
            .zip(self.next_seq..)
            .map(|((op, inverse), seq)| OpEntry {
                seq,
                actor,
                ts: ts.clone(),
                turn: Some(turn.to_string()),
                op,
                inverse,
            })
            .collect();
        append_lines(&self.dir.join("ops.jsonl"), &entries)?;
        self.next_seq += entries.len() as u64;
        self.document = doc;
        self.document.rev += 1;
        Ok(entries)
    }

    fn revert(&mut self, step: &[OpEntry], prefix: &str) -> Result<Vec<OpEntry>> {
        let turn = step.first().and_then(|e| e.turn.as_deref()).unwrap_or_default();
        let base = turn
            .strip_prefix("undo:")
            .or_else(|| turn.strip_prefix("redo:"))
            .unwrap_or(turn);
        let mut doc = self.document.clone();
        let mut applied = Vec::new();
        for entry in step.iter().rev() {
            let inverse = apply_op(&mut doc, &entry.inverse)?;
            applied.push((entry.inverse.clone(), inverse));
        }
        self.record(doc, applied, Actor::User, &format!("{prefix}:{base}"))
    }
}

fn guard_pins(doc: &Document, op: Op, actor: Actor) -> Result<Op> {
    let Op::UpdateLayer { id, mut patch } = op else {
        return Ok(op);
    };
    let Some(layer) = doc.layer(&id) else {
        return Ok(Op::UpdateLayer { id, patch });
    };
    let paths = patch_paths(&patch);
    let Some(map) = patch.as_object_mut().filter(|m| !m.contains_key("pinned")) else {
        return Ok(Op::UpdateLayer { id, patch });
    };
    match actor {
        Actor::Agent => {
            let overlaps = |a: &str, b: &str| {
                a == b || a.starts_with(&format!("{b}.")) || b.starts_with(&format!("{a}."))
            };
            if let Some(path) = paths.iter().find(|p| layer.pinned.iter().any(|q| overlaps(p, q))) {
                return Err(CoreError::InvalidOp(format!(
                    "{} ({id}): {path} was set by the user and is pinned; include \"pinned\" in the patch only if the user asked to change it",
                    layer.name
                )));
            }
        }
        Actor::User => {
            let mut pinned = layer.pinned.clone();
            for path in paths {
                if !pinned.contains(&path) {
                    pinned.push(path);
                }
            }
            if pinned != layer.pinned {
                map.insert("pinned".into(), serde_json::json!(pinned));
            }
        }
    }
    Ok(Op::UpdateLayer { id, patch })
}

fn restore_ops(current: &Document, target: &Document) -> Result<Vec<Op>> {
    let mut wanted = Vec::new();
    preorder(target, None, &mut wanted);
    let in_target = |id: &str| wanted.iter().any(|l| l.id == id);
    let mut doc = current.clone();
    let mut ops = Vec::new();
    let mut push = |doc: &mut Document, op: Op| -> Result<()> {
        apply_op(doc, &op)?;
        ops.push(op);
        Ok(())
    };

    let detach: Vec<LayerId> = doc
        .layers
        .iter()
        .filter(|l| in_target(&l.id) && l.parent.as_deref().is_some_and(|p| !in_target(p)))
        .map(|l| l.id.clone())
        .collect();
    for id in detach {
        let index = doc.layers.len();
        push(
            &mut doc,
            Op::MoveLayer {
                id,
                parent: None,
                index,
            },
        )?;
    }

    let doomed: Vec<LayerId> = doc
        .layers
        .iter()
        .filter(|l| !in_target(&l.id) && l.parent.as_deref().is_none_or(in_target))
        .map(|l| l.id.clone())
        .collect();
    for id in doomed {
        push(&mut doc, Op::DeleteLayer { id })?;
    }

    let mut ungroup = Vec::new();
    for t in &wanted {
        let Some(cur) = doc.layer(&t.id) else { continue };
        let Some(patch) = layer_patch(cur, t)? else {
            continue;
        };
        let op = Op::UpdateLayer {
            id: t.id.clone(),
            patch,
        };
        if matches!(cur.kind, LayerKind::Group) && !matches!(t.kind, LayerKind::Group) {
            ungroup.push(op);
        } else {
            push(&mut doc, op)?;
        }
    }

    for t in &wanted {
        if doc.layer(&t.id).is_none() {
            let index = Some(sibling_index(target, t));
            push(
                &mut doc,
                Op::AddLayer {
                    layer: Box::new((*t).clone()),
                    index,
                },
            )?;
        }
    }

    for t in &wanted {
        let index = sibling_index(target, t);
        let Some(cur) = doc.layer(&t.id) else { continue };
        if cur.parent != t.parent || sibling_index(&doc, cur) != index {
            push(
                &mut doc,
                Op::MoveLayer {
                    id: t.id.clone(),
                    parent: t.parent.clone(),
                    index,
                },
            )?;
        }
    }

    for op in ungroup {
        push(&mut doc, op)?;
    }

    let artboard = diff(
        &serde_json::to_value(&doc.artboard)?,
        &serde_json::to_value(&target.artboard)?,
    );
    if artboard.as_object().is_some_and(|m| !m.is_empty()) {
        push(&mut doc, Op::SetArtboard { patch: artboard })?;
    }
    Ok(ops)
}

fn layer_patch(cur: &Layer, target: &Layer) -> Result<Option<Value>> {
    let mut patch = diff(&serde_json::to_value(cur)?, &serde_json::to_value(target)?);
    let Some(map) = patch.as_object_mut() else {
        return Ok(None);
    };
    map.remove("parent");
    if map.is_empty() {
        return Ok(None);
    }
    map.insert("pinned".into(), serde_json::to_value(&target.pinned)?);
    Ok(Some(patch))
}

fn preorder<'a>(doc: &'a Document, parent: Option<&str>, out: &mut Vec<&'a Layer>) {
    for layer in doc.children(parent) {
        out.push(layer);
        preorder(doc, Some(&layer.id), out);
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

    #[test]
    fn folders_and_trash_place_projects_without_touching_them() {
        let root =
            std::env::temp_dir().join(format!("banner-store-library-{}", uuid::Uuid::new_v4().simple()));
        let store = ProjectStore::new(root.clone());
        let a = store.create("가", 100.0, 50.0).expect("create").project;
        let b = store.create("나", 100.0, 50.0).expect("create").project;
        let find = |id: &str| {
            store
                .list()
                .expect("list")
                .into_iter()
                .find(|p| p.id == id)
                .expect("listed")
        };
        assert_eq!(
            (find(&a.id).folder, find(&a.id).deleted_at),
            (None, None),
            "top level by default"
        );

        let year = store.create_folder(" 2026 ", None).expect("folder");
        let autumn = store
            .create_folder("하반기", Some(&year.id))
            .expect("nested folder");
        assert_eq!(year.name, "2026", "names are trimmed");
        assert!(store.create_folder("  ", None).is_err(), "a folder needs a name");
        assert!(store.create_folder("고아", Some("nope")).is_err());
        store.move_project(&a.id, Some(&autumn.id)).expect("move");
        assert!(store.move_project(&a.id, Some("nope")).is_err());
        assert_eq!(find(&a.id).folder.as_deref(), Some(autumn.id.as_str()));
        assert_eq!(
            find(&a.id).updated_at,
            a.updated_at,
            "moving leaves the project's own files alone"
        );
        store.rename_folder(&autumn.id, "가을").expect("rename");
        assert_eq!(store.folders().expect("folders")[1].name, "가을");

        store.trash(&b.id).expect("trash");
        assert!(find(&b.id).deleted_at.is_some());
        assert!(
            root.join(&b.id).join("project.json").is_file(),
            "the files stay until the trash is emptied"
        );
        store.restore(&b.id).expect("restore");
        assert_eq!(find(&b.id).deleted_at, None);

        let inside = store.delete_folder(&year.id).expect("delete folder");
        assert_eq!(
            inside,
            std::slice::from_ref(&a.id),
            "projects in nested folders go to the trash"
        );
        assert!(
            store.folders().expect("folders").is_empty(),
            "nested folders go with it"
        );
        assert_eq!(
            (find(&a.id).folder, find(&a.id).deleted_at.is_some()),
            (None, true)
        );

        assert_eq!(store.empty_trash().expect("empty"), std::slice::from_ref(&a.id));
        assert!(!root.join(&a.id).exists());
        assert_eq!(
            store.list().expect("list").len(),
            1,
            "only the project outside the trash is left"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn same_turn_ops_form_one_undo_step() {
        let root = std::env::temp_dir().join(format!("banner-store-turn-{}", uuid::Uuid::new_v4().simple()));
        let store = ProjectStore::new(root.clone());
        let mut h = store.create("t", 100.0, 50.0).expect("create");
        let mk = |id: &str| Layer {
            id: id.into(),
            name: id.into(),
            parent: None,
            kind: LayerKind::Group,
            x_mm: 0.0,
            y_mm: 0.0,
            w_mm: 10.0,
            h_mm: 10.0,
            rotation_deg: 0.0,
            opacity: 1.0,
            visible: true,
            locked: false,
            fills: vec![],
            strokes: vec![],
            effects: vec![],
            strategy: Strategy::NativeVector,
            strategy_reason: String::new(),
            pinned: vec![],
            source: None,
        };
        h.apply_ops(
            vec![Op::AddLayer {
                layer: Box::new(mk("A")),
                index: None,
            }],
            Actor::Agent,
            Some("turn1".into()),
        )
        .expect("a");
        h.apply_ops(
            vec![Op::AddLayer {
                layer: Box::new(mk("B")),
                index: None,
            }],
            Actor::Agent,
            Some("turn1".into()),
        )
        .expect("b");
        h.apply_ops(
            vec![Op::AddLayer {
                layer: Box::new(mk("C")),
                index: None,
            }],
            Actor::User,
            None,
        )
        .expect("c");
        assert_eq!(h.document.layers.len(), 3);
        h.undo().expect("undo user");
        assert_eq!(h.document.layers.len(), 2);
        h.undo().expect("undo turn");
        assert_eq!(
            h.document.layers.len(),
            0,
            "both agent ops of turn1 undo together"
        );
        h.redo().expect("redo turn");
        assert_eq!(h.document.layers.len(), 2);
        let _ = std::fs::remove_dir_all(root);
    }

    use crate::ops::tests::{layer, sample, text};
    use serde_json::json;

    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("hoist-core-{}", uuid::Uuid::new_v4().simple()));
            fs::create_dir_all(&dir).expect("temp dir");
            TempRoot(dir)
        }
        fn store(&self) -> ProjectStore {
            ProjectStore::new(self.0.clone())
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

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

    fn seeded(store: &ProjectStore) -> ProjectHandle {
        let mut h = store.create("청전동 현수막", 900.0, 300.0).expect("create");
        let ops = sample()
            .layers
            .into_iter()
            .map(|layer| Op::AddLayer {
                layer: Box::new(layer),
                index: None,
            })
            .collect();
        h.apply_ops(ops, Actor::Agent, Some("t1".into())).expect("seed");
        h
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
        h.append_chat(&serde_json::json!({"id": "c1", "role": "user", "text": "안녕"}))
            .expect("chat");
        fs::write(h.asset_path("a.png"), [9]).expect("asset");
        h.set_memory("글꼴은 Pretendard").expect("memory");
        fs::create_dir_all(h.dir.join("analysis")).expect("analysis dir");
        fs::write(h.dir.join("analysis/v1.json"), "{}").expect("analysis");
        h.document
            .layers
            .push(crate::ops::tests::layer("L1", "배경", None, LayerKind::Group));
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
    fn apply_ops_groups_one_undo_step_and_persists() {
        let root = TempRoot::new();
        let store = root.store();
        let mut h = seeded(&store);
        assert_eq!(h.last_seq(), 6);
        let before = h.document.clone();

        let entries = h
            .apply_ops(
                vec![
                    Op::UpdateLayer {
                        id: "L3".into(),
                        patch: json!({"x_mm": 130}),
                    },
                    Op::DeleteLayer { id: "L2".into() },
                ],
                Actor::User,
                None,
            )
            .expect("apply");
        assert_eq!(entries.len(), 5);
        assert!(entries.iter().all(|e| e.turn.as_deref() == Some("u1")));
        assert_eq!(
            entries.iter().map(|e| e.seq).collect::<Vec<_>>(),
            [7, 8, 9, 10, 11]
        );
        assert_eq!(h.document.rev, before.rev + 1);
        assert_eq!(h.document.layers.len(), 2);

        let reopened = store.open(&h.project.id).expect("open");
        assert_eq!(reopened.document, h.document);
        assert!(reopened.can_undo());
        drop(h);

        let mut h = reopened;
        let undone = h.undo().expect("undo").expect("step");
        assert_eq!(undone.len(), 5);
        assert!(undone.iter().all(|e| e.turn.as_deref() == Some("undo:u1")));
        assert_eq!(h.document.layers, before.layers);
        assert!(h.can_redo());

        let mut h = store.open(&h.project.id).expect("reopen");
        let redone = h.redo().expect("redo").expect("step");
        assert!(redone.iter().all(|e| e.turn.as_deref() == Some("redo:u1")));
        assert_eq!(h.document.layers.len(), 2);
        h.undo().expect("undo");
        h.undo().expect("undo seed");
        assert!(h.document.layers.is_empty());
        assert_eq!(h.undo().expect("nothing"), None);

        let all = h.entries_since(0).expect("entries");
        assert_eq!(all.len() as u64, h.last_seq());
        assert_eq!(
            h.entries_since(10).expect("since").len() as u64,
            h.last_seq() - 10
        );
    }

    #[test]
    fn new_ops_clear_redo_and_failed_batch_changes_nothing() {
        let root = TempRoot::new();
        let mut h = seeded(&root.store());
        h.undo().expect("undo");
        let snapshot = h.document.clone();
        let seq = h.last_seq();
        let r = h.apply_ops(
            vec![
                Op::AddLayer {
                    layer: Box::new(layer("L1", "a", None, text("a"))),
                    index: None,
                },
                Op::DeleteLayer { id: "missing".into() },
            ],
            Actor::User,
            None,
        );
        assert!(matches!(r, Err(CoreError::LayerNotFound(_))));
        assert_eq!(h.document, snapshot);
        assert_eq!(h.last_seq(), seq);
        assert!(h.can_redo());

        h.apply_ops(
            vec![Op::AddLayer {
                layer: Box::new(layer("L1", "a", None, text("a"))),
                index: None,
            }],
            Actor::User,
            None,
        )
        .expect("apply");
        assert!(!h.can_redo());
    }

    #[test]
    fn user_edits_pin_and_agent_respects_pins() {
        let root = TempRoot::new();
        let mut h = seeded(&root.store());
        let entries = h
            .apply_ops(
                vec![Op::UpdateLayer {
                    id: "L3".into(),
                    patch: json!({"kind": {"text": "새 문구"}, "x_mm": 120}),
                }],
                Actor::User,
                None,
            )
            .expect("user edit");
        let mut pinned = h.document.layer("L3").expect("L3").pinned.clone();
        pinned.sort();
        assert_eq!(pinned, ["kind.text", "x_mm"]);

        for patch in [
            json!({"x_mm": 1}),
            json!({"kind": {"type": "text", "text": "x"}}),
            json!({"kind": null}),
        ] {
            let r = h.apply_ops(
                vec![Op::UpdateLayer {
                    id: "L3".into(),
                    patch,
                }],
                Actor::Agent,
                Some("t2".into()),
            );
            assert!(matches!(r, Err(CoreError::InvalidOp(_))));
        }
        h.apply_ops(
            vec![Op::UpdateLayer {
                id: "L3".into(),
                patch: json!({"y_mm": 1}),
            }],
            Actor::Agent,
            Some("t2".into()),
        )
        .expect("unpinned path");
        h.apply_ops(
            vec![Op::UpdateLayer {
                id: "L3".into(),
                patch: json!({"x_mm": 1, "pinned": ["kind.text"]}),
            }],
            Actor::Agent,
            Some("t3".into()),
        )
        .expect("explicit unpin");
        assert_eq!(h.document.layer("L3").expect("L3").pinned, ["kind.text"]);

        h.undo().expect("undo t3");
        h.undo().expect("undo t2");
        h.undo().expect("undo user edit");
        assert!(h.document.layer("L3").expect("L3").pinned.is_empty());
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn snapshot_and_restore_keep_history() {
        let root = TempRoot::new();
        let mut h = seeded(&root.store());
        let v = h
            .snapshot(Actor::Agent, "초안", Some("t1".into()), Some(&[9, 9]))
            .expect("snapshot");
        assert_eq!(v.seq, 6);
        assert!(h.version_thumbnail_path(6).is_file());
        let saved = h.document.clone();

        h.apply_ops(
            vec![
                Op::MoveLayer { id: "L3".into(), parent: None, index: 0 },
                Op::DeleteLayer { id: "L2".into() },
                Op::AddLayer { layer: Box::new(layer("L7", "새 그룹", None, LayerKind::Group)), index: Some(0) },
                Op::MoveLayer { id: "L6".into(), parent: Some("L7".into()), index: 0 },
                Op::UpdateLayer { id: "L1".into(), patch: json!({"kind": {"type": "text", "text": "x", "font_family": "A", "font_size_mm": 5}}) },
                Op::SetArtboard { patch: json!({"width_mm": 1000, "bleed_mm": 3}) },
            ],
            Actor::User,
            None,
        )
        .expect("mutate");
        h.apply_ops(
            vec![Op::UpdateLayer {
                id: "L7".into(),
                patch: json!({"kind": {"type": "shape", "shape": "ellipse"}}),
            }],
            Actor::User,
            None,
        )
        .expect_err("group with children");
        h.apply_ops(
            vec![Op::MoveLayer {
                id: "L6".into(),
                parent: None,
                index: 0,
            }],
            Actor::User,
            None,
        )
        .expect("move out");
        h.apply_ops(
            vec![Op::UpdateLayer {
                id: "L7".into(),
                patch: json!({"kind": {"type": "shape", "shape": "ellipse"}}),
            }],
            Actor::User,
            None,
        )
        .expect("degroup");
        h.apply_ops(
            vec![Op::AddLayer {
                layer: Box::new(layer("L8", "그룹", None, LayerKind::Group)),
                index: None,
            }],
            Actor::User,
            None,
        )
        .expect("group");
        h.apply_ops(
            vec![Op::MoveLayer {
                id: "L1".into(),
                parent: Some("L8".into()),
                index: 0,
            }],
            Actor::User,
            None,
        )
        .expect("nest");
        h.apply_ops(
            vec![Op::UpdateLayer {
                id: "L1".into(),
                patch: json!({"kind": {"type": "group"}}),
            }],
            Actor::User,
            None,
        )
        .expect("L1 becomes group");
        h.apply_ops(
            vec![Op::MoveLayer {
                id: "L1".into(),
                parent: None,
                index: 0,
            }],
            Actor::User,
            None,
        )
        .expect("unnest");
        h.apply_ops(
            vec![Op::MoveLayer {
                id: "L3".into(),
                parent: Some("L1".into()),
                index: 0,
            }],
            Actor::User,
            None,
        )
        .expect("L3 under L1");
        let mutated = h.document.clone();

        let entries = h.restore_version(6).expect("restore");
        assert!(entries.iter().all(|e| e.turn.as_deref() == Some("restore:6")));
        assert_eq!(h.document.layers, saved.layers);
        assert_eq!(h.document.artboard, saved.artboard);

        h.undo().expect("undo restore");
        assert_eq!(h.document.layers, mutated.layers);
        assert_eq!(h.document.artboard, mutated.artboard);

        let versions = h.versions().expect("versions");
        assert_eq!(versions.len(), 1);
        h.snapshot(Actor::User, "수정", None, None).expect("snapshot 2");
        assert_eq!(h.versions().expect("versions")[0].summary, "수정");
        assert_eq!(h.version_document(h.last_seq()).expect("doc"), h.document);
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
