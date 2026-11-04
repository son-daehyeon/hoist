use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::*;
use crate::text::Fonts;
use crate::Result;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ExportInfo {
    pub path: String,
    pub width_px: Option<u32>,
    pub height_px: Option<u32>,
    pub bytes: u64,
}

pub fn export_png(doc: &Document, assets_dir: &Path, fonts: &Fonts, dpi: f64, scale_div: u32, out: &Path) -> Result<ExportInfo> {
    todo!()
}

pub fn export_svg(doc: &Document, assets_dir: &Path, fonts: &Fonts, scale_div: u32, outline_text: bool, out: &Path) -> Result<ExportInfo> {
    todo!()
}

pub fn pixel_size(doc: &Document, dpi: f64) -> (u32, u32) {
    todo!()
}
