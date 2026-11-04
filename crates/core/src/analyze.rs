use image::RgbaImage;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Region {
    pub bbox_px: [u32; 4],
    pub color: String,
    pub area_px: u32,
    pub kind_hint: RegionKind,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RegionKind {
    TextLike,
    Shape,
    Photo,
    Background,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Palette {
    pub colors: Vec<(String, f64)>,
}

pub fn palette(img: &RgbaImage, max_colors: usize) -> Palette {
    todo!()
}

pub fn segment_regions(img: &RgbaImage) -> Vec<Region> {
    todo!()
}

pub fn quantize(img: &RgbaImage, colors: usize) -> (RgbaImage, Vec<String>) {
    todo!()
}
