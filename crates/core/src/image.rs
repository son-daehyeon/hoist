use std::path::Path;

use image::RgbaImage;

use crate::Result;

pub fn load(path: &Path) -> Result<RgbaImage> {
    todo!()
}

pub fn save_png(img: &RgbaImage, path: &Path) -> Result<()> {
    todo!()
}

pub fn to_png_bytes(img: &RgbaImage) -> Result<Vec<u8>> {
    todo!()
}

pub fn from_png_bytes(bytes: &[u8]) -> Result<RgbaImage> {
    todo!()
}

pub fn to_base64_png(img: &RgbaImage) -> Result<String> {
    todo!()
}

pub fn to_jpeg_base64(img: &RgbaImage, quality: u8) -> Result<String> {
    todo!()
}

pub fn info(path: &Path) -> Result<(u32, u32)> {
    todo!()
}

pub fn resize_max(img: &RgbaImage, max_px: u32) -> RgbaImage {
    todo!()
}

pub fn resize_to(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    todo!()
}

pub fn crop(img: &RgbaImage, rect: [u32; 4]) -> RgbaImage {
    todo!()
}

pub fn denoise_light(img: &RgbaImage) -> RgbaImage {
    todo!()
}

pub fn alpha_bbox(img: &RgbaImage) -> Option<[u32; 4]> {
    todo!()
}

pub fn side_by_side(a: &RgbaImage, b: &RgbaImage, gap: u32) -> RgbaImage {
    todo!()
}

pub fn dominant_color(img: &RgbaImage, rect: Option<[u32; 4]>) -> String {
    todo!()
}
