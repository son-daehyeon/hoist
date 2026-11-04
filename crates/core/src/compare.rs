use image::RgbaImage;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CompareResult {
    pub similarity: f64,
    pub overlap: f64,
    pub edge_similarity: f64,
    pub worst_regions: Vec<DiffRegion>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DiffRegion {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub score: f64,
}

pub fn compare(original: &RgbaImage, work: &RgbaImage) -> CompareResult {
    todo!()
}

pub fn diff_heatmap(original: &RgbaImage, work: &RgbaImage) -> RgbaImage {
    todo!()
}

pub fn mask_overlap(a: &image::GrayImage, b: &image::GrayImage) -> f64 {
    todo!()
}
