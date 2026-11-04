use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::*;
use crate::render::{RenderOptions, SvgOptions, render_document, to_svg};
use crate::text::Fonts;
use crate::{CoreError, Result};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ExportInfo {
    pub path: String,
    pub width_px: Option<u32>,
    pub height_px: Option<u32>,
    pub bytes: u64,
}

pub fn export_png(
    doc: &Document,
    assets_dir: &Path,
    fonts: &Fonts,
    dpi: f64,
    scale_div: u32,
    out: &Path,
) -> Result<ExportInfo> {
    let opts = RenderOptions {
        scale_px_per_mm: dpi / 25.4,
        background: true,
    };
    let img = render_document(doc, assets_dir, fonts, &opts)?;
    let png = crate::image::to_png_bytes(&img)?;
    let ppm = (dpi * f64::from(scale_div.max(1)) / 0.0254).round() as u32;
    let png = with_phys(&png, ppm)?;
    std::fs::write(out, &png)?;
    Ok(ExportInfo {
        path: out.to_string_lossy().into_owned(),
        width_px: Some(img.width()),
        height_px: Some(img.height()),
        bytes: png.len() as u64,
    })
}

pub fn export_svg(
    doc: &Document,
    assets_dir: &Path,
    fonts: &Fonts,
    scale_div: u32,
    outline_text: bool,
    out: &Path,
) -> Result<ExportInfo> {
    let opts = SvgOptions {
        scale: f64::from(scale_div.max(1)),
        outline_text,
        embed_images: true,
    };
    let svg = to_svg(doc, assets_dir, fonts, &opts)?;
    std::fs::write(out, svg.as_bytes())?;
    Ok(ExportInfo {
        path: out.to_string_lossy().into_owned(),
        width_px: None,
        height_px: None,
        bytes: svg.len() as u64,
    })
}

pub fn pixel_size(doc: &Document, dpi: f64) -> (u32, u32) {
    let px = |mm: f64| (mm * (dpi / 25.4)).round().max(1.0) as u32;
    (px(doc.artboard.width_mm), px(doc.artboard.height_mm))
}

const IHDR_END: usize = 8 + 4 + 4 + 13 + 4;

fn with_phys(png: &[u8], ppm: u32) -> Result<Vec<u8>> {
    if png.len() < IHDR_END || png.get(12..16) != Some(b"IHDR".as_slice()) {
        return Err(CoreError::Render("unexpected PNG layout".into()));
    }
    let mut out = Vec::with_capacity(png.len() + 21);
    out.extend_from_slice(&png[..IHDR_END]);
    out.extend_from_slice(&9u32.to_be_bytes());
    let start = out.len();
    out.extend_from_slice(b"pHYs");
    out.extend_from_slice(&ppm.to_be_bytes());
    out.extend_from_slice(&ppm.to_be_bytes());
    out.push(1);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
    let mut i = IHDR_END;
    while i + 12 <= png.len() {
        let len = u32::from_be_bytes([png[i], png[i + 1], png[i + 2], png[i + 3]]) as usize;
        let end = i + 12 + len;
        if end > png.len() {
            return Err(CoreError::Render("truncated PNG chunk".into()));
        }
        if &png[i + 4..i + 8] != b"pHYs" {
            out.extend_from_slice(&png[i..end]);
        }
        i = end;
    }
    Ok(out)
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in bytes {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::tests::{fonts, rect};

    fn find_chunk<'a>(png: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
        let mut i = 8;
        while i + 12 <= png.len() {
            let len = u32::from_be_bytes([png[i], png[i + 1], png[i + 2], png[i + 3]]) as usize;
            if &png[i + 4..i + 8] == kind {
                return Some(&png[i + 8..i + 8 + len]);
            }
            i += 12 + len;
        }
        None
    }

    #[test]
    fn crc_matches_known_value() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
    }

    #[test]
    fn png_has_expected_size_and_phys() {
        let mut doc = Document::new(254.0, 127.0);
        doc.layers.push(rect("L1", [0.0, 0.0, 127.0, 127.0], "#FF0000"));
        assert_eq!(pixel_size(&doc, 10.0), (100, 50));
        let out = std::env::temp_dir().join(format!("hoist-core-export-{}.png", std::process::id()));
        let info = export_png(&doc, &std::env::temp_dir(), fonts(), 10.0, 10, &out).expect("export");
        assert_eq!((info.width_px, info.height_px), (Some(100), Some(50)));
        let bytes = std::fs::read(&out).expect("read");
        assert_eq!(info.bytes, bytes.len() as u64);
        let phys = find_chunk(&bytes, b"pHYs").expect("pHYs");
        let ppm = (100.0f64 / 0.0254).round() as u32;
        assert_eq!(
            phys,
            [ppm.to_be_bytes(), ppm.to_be_bytes()]
                .concat()
                .into_iter()
                .chain([1])
                .collect::<Vec<_>>()
        );
        let img = image::load_from_memory(&bytes).expect("decode").to_rgba8();
        assert_eq!(img.dimensions(), (100, 50));
        assert_eq!(img.get_pixel(10, 10).0, [255, 0, 0, 255]);
        assert_eq!(img.get_pixel(90, 10).0, [255, 255, 255, 255]);
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn svg_export_scales_physical_size() {
        let mut doc = Document::new(1000.0, 500.0);
        doc.layers.push(rect("L1", [0.0, 0.0, 10.0, 10.0], "#FF0000"));
        let out = std::env::temp_dir().join(format!("hoist-core-export-{}.svg", std::process::id()));
        let info = export_svg(&doc, &std::env::temp_dir(), fonts(), 10, true, &out).expect("export");
        let svg = std::fs::read_to_string(&out).expect("read");
        assert_eq!(info.bytes, svg.len() as u64);
        assert!(svg.contains("width=\"100mm\" height=\"50mm\""));
        assert!(svg.contains("viewBox=\"0 0 1000 500\""));
        let _ = std::fs::remove_file(&out);
    }
}
