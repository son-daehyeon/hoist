use std::path::Path;

use image::RgbaImage;
use serde::{Deserialize, Serialize};

use crate::model::*;
use crate::text::Fonts;
use crate::Result;

#[derive(Clone, Debug)]
pub struct RenderOptions {
    pub scale_px_per_mm: f64,
    pub background: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LayerRaster {
    pub id: LayerId,
    pub x_px: i32,
    pub y_px: i32,
    pub w_px: u32,
    pub h_px: u32,
    #[serde(skip)]
    pub image: Option<RgbaImage>,
}

#[derive(Clone, Debug)]
pub struct SvgOptions {
    pub scale: f64,
    pub outline_text: bool,
    pub embed_images: bool,
}

pub fn render_document(doc: &Document, assets_dir: &Path, fonts: &Fonts, opts: &RenderOptions) -> Result<RgbaImage> {
    todo!()
}

pub fn render_layer(doc: &Document, layer_id: &str, assets_dir: &Path, fonts: &Fonts, scale_px_per_mm: f64) -> Result<LayerRaster> {
    todo!()
}

pub fn render_layers(doc: &Document, assets_dir: &Path, fonts: &Fonts, scale_px_per_mm: f64) -> Result<Vec<LayerRaster>> {
    todo!()
}

pub fn render_region(doc: &Document, assets_dir: &Path, fonts: &Fonts, scale_px_per_mm: f64, rect_mm: [f64; 4]) -> Result<RgbaImage> {
    todo!()
}

pub fn to_svg(doc: &Document, assets_dir: &Path, fonts: &Fonts, opts: &SvgOptions) -> Result<String> {
    todo!()
}

pub fn layer_bounds_mm(doc: &Document, layer: &Layer) -> [f64; 4] {
    todo!()
}
