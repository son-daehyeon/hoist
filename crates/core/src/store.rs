use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::*;
use crate::Result;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub updated_at: String,
    pub thumbnail: Option<PathBuf>,
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
}

impl ProjectStore {
    pub fn new(root: PathBuf) -> Self {
        ProjectStore { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn list(&self) -> Result<Vec<ProjectSummary>> {
        todo!()
    }

    pub fn create(&self, name: &str, width_mm: f64, height_mm: f64) -> Result<ProjectHandle> {
        todo!()
    }

    pub fn open(&self, id: &str) -> Result<ProjectHandle> {
        todo!()
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        todo!()
    }

    pub fn duplicate(&self, id: &str) -> Result<ProjectHandle> {
        todo!()
    }
}

impl ProjectHandle {
    pub fn save(&mut self) -> Result<()> {
        todo!()
    }

    pub fn rename(&mut self, name: &str) -> Result<()> {
        todo!()
    }

    pub fn add_original(&mut self, src: &Path) -> Result<OriginalInfo> {
        todo!()
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

    pub fn new_asset_name(&self, prefix: &str, ext: &str) -> String {
        todo!()
    }

    pub fn apply_ops(&mut self, ops: Vec<Op>, actor: Actor, turn: Option<String>) -> Result<Vec<OpEntry>> {
        todo!()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> Result<Option<Vec<OpEntry>>> {
        todo!()
    }

    pub fn redo(&mut self) -> Result<Option<Vec<OpEntry>>> {
        todo!()
    }

    pub fn snapshot(&mut self, actor: Actor, summary: &str, turn: Option<String>, thumbnail_png: Option<&[u8]>) -> Result<Version> {
        todo!()
    }

    pub fn versions(&self) -> Result<Vec<Version>> {
        todo!()
    }

    pub fn version_document(&self, seq: u64) -> Result<Document> {
        todo!()
    }

    pub fn version_thumbnail_path(&self, seq: u64) -> PathBuf {
        self.dir.join("versions").join(format!("{seq}.png"))
    }

    pub fn restore_version(&mut self, seq: u64) -> Result<Vec<OpEntry>> {
        todo!()
    }

    pub fn memory(&self) -> Result<String> {
        todo!()
    }

    pub fn set_memory(&self, text: &str) -> Result<()> {
        todo!()
    }

    pub fn chat(&self) -> Result<Vec<serde_json::Value>> {
        todo!()
    }

    pub fn append_chat(&self, item: &serde_json::Value) -> Result<()> {
        todo!()
    }

    pub fn replace_chat(&self, items: &[serde_json::Value]) -> Result<()> {
        todo!()
    }

    pub fn update_chat(&self, id: &str, item: &serde_json::Value) -> Result<()> {
        todo!()
    }

    pub fn entries_since(&self, seq: u64) -> Result<Vec<OpEntry>> {
        todo!()
    }

    pub fn last_seq(&self) -> u64 {
        self.next_seq.saturating_sub(1)
    }

    pub fn write_thumbnail(&self, png: &[u8]) -> Result<()> {
        todo!()
    }

    pub fn thumbnail_path(&self) -> PathBuf {
        self.dir.join("thumbnail.png")
    }
}
