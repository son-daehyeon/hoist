use image::{GrayImage, RgbaImage};

use crate::model::ShapeProps;
use crate::Result;

pub struct TraceOptions {
    pub mm_per_px: f64,
    pub smooth: bool,
    pub corner_threshold_deg: f64,
    pub min_area_px: u32,
}

impl Default for TraceOptions {
    fn default() -> Self {
        TraceOptions {
            mm_per_px: 1.0,
            smooth: true,
            corner_threshold_deg: 60.0,
            min_area_px: 4,
        }
    }
}

pub fn trace_mask(mask: &GrayImage, opts: &TraceOptions) -> Result<String> {
    todo!()
}

pub fn color_mask(img: &RgbaImage, color: &str, tolerance: u8) -> GrayImage {
    todo!()
}

pub fn alpha_mask(img: &RgbaImage, threshold: u8) -> GrayImage {
    todo!()
}

pub fn upsample_mask(mask: &GrayImage, factor: u32) -> GrayImage {
    todo!()
}

pub fn detect_shape(mask: &GrayImage) -> Option<(ShapeProps, f64)> {
    todo!()
}

pub fn rasterize_path(d: &str, w: u32, h: u32, px_per_unit: f64) -> Result<GrayImage> {
    todo!()
}
