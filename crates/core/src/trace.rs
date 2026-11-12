use image::imageops::{self, FilterType};
use image::{GrayImage, Luma, RgbaImage};
use visioncortex::{BinaryImage, PathSimplifyMode, PointF64};

use crate::Result;
use crate::render::{esc, num, rasterize_svg};

pub const SMOOTH_FACTOR: u32 = 8;
pub const SMOOTH_SIGMA_SOURCE_PX: f32 = 0.5;
const UPSAMPLED_CORNER_DEG: f64 = 120.0;
const UPSAMPLED_SPECKLE_SOURCE_PX: u32 = 4;
const UPSAMPLED_LENGTH_SOURCE_PX: f64 = 1.0;
const UPSAMPLED_SPLICE_DEG: i32 = 75;

pub struct TraceOptions {
    pub mm_per_px: f64,
    pub corner_threshold_deg: f64,
    pub min_area_px: u32,
    pub length_threshold: f64,
    pub splice_threshold: i32,
}

impl Default for TraceOptions {
    fn default() -> Self {
        TraceOptions {
            mm_per_px: 1.0,
            corner_threshold_deg: 60.0,
            min_area_px: 4,
            length_threshold: 4.0,
            splice_threshold: 45,
        }
    }
}

impl TraceOptions {
    pub fn upsampled(mm_per_px_at_source: f64, factor: u32) -> Self {
        let f = factor.max(1);
        TraceOptions {
            mm_per_px: mm_per_px_at_source / f64::from(f),
            corner_threshold_deg: UPSAMPLED_CORNER_DEG,
            min_area_px: (f * f) * UPSAMPLED_SPECKLE_SOURCE_PX,
            length_threshold: f64::from(f) * UPSAMPLED_LENGTH_SOURCE_PX,
            splice_threshold: UPSAMPLED_SPLICE_DEG,
        }
    }
}

pub fn trace_mask(mask: &GrayImage, opts: &TraceOptions) -> Result<String> {
    let (w, h) = mask.dimensions();
    if w == 0 || h == 0 {
        return Ok(String::new());
    }
    let mut img = BinaryImage::new_w_h(w as usize, h as usize);
    for (x, y, p) in mask.enumerate_pixels() {
        img.set_pixel(x as usize, y as usize, p[0] >= 128);
    }
    let speckle = f64::from(opts.min_area_px).sqrt().ceil() as usize;
    let corner = opts.corner_threshold_deg.to_radians();
    let splice = f64::from(opts.splice_threshold).to_radians();
    let clusters = img.to_clusters(false);
    let mut d = String::new();
    for i in 0..clusters.len() {
        let cluster = clusters.get_cluster(i);
        if cluster.size() < speckle * speckle {
            continue;
        }
        let path = cluster.to_compound_path(
            PathSimplifyMode::Spline,
            corner,
            opts.length_threshold,
            10,
            splice,
        );
        let (raw, offset) = path.to_svg_string(true, PointF64::default(), Some(2));
        transform_d(&raw, offset.x, offset.y, opts.mm_per_px, &mut d);
    }
    Ok(d.trim_end().to_string())
}

fn transform_d(d: &str, tx: f64, ty: f64, s: f64, out: &mut String) {
    let mut token = String::new();
    let mut idx = 0usize;
    let flush = |token: &mut String, idx: &mut usize, out: &mut String| {
        if let Ok(v) = token.parse::<f64>() {
            let v = if idx.is_multiple_of(2) {
                (v + tx) * s
            } else {
                (v + ty) * s
            };
            out.push_str(&num(v));
            out.push(' ');
            *idx += 1;
        }
        token.clear();
    };
    for ch in d.chars() {
        if ch.is_ascii_alphabetic() {
            flush(&mut token, &mut idx, out);
            out.push(ch);
            idx = 0;
        } else if ch.is_whitespace() || ch == ',' {
            flush(&mut token, &mut idx, out);
        } else if ch == '-' && !token.is_empty() {
            flush(&mut token, &mut idx, out);
            token.push(ch);
        } else {
            token.push(ch);
        }
    }
    flush(&mut token, &mut idx, out);
}

pub fn color_mask(img: &RgbaImage, color: &str, tolerance: u8) -> GrayImage {
    let (w, h) = img.dimensions();
    let Some(target) = parse_hex(color) else {
        return GrayImage::new(w, h);
    };
    GrayImage::from_fn(w, h, |x, y| {
        let p = img.get_pixel(x, y).0;
        let hit = p[3] >= 128 && (0..3).all(|i| p[i].abs_diff(target[i]) <= tolerance);
        Luma([if hit { 255 } else { 0 }])
    })
}

pub fn color_coverage(img: &RgbaImage, color: &str, tolerance: u8) -> GrayImage {
    let (w, h) = img.dimensions();
    let Some(target) = parse_hex(color) else {
        return GrayImage::new(w, h);
    };
    let tol = u32::from(tolerance.max(1));
    GrayImage::from_fn(w, h, |x, y| {
        let p = img.get_pixel(x, y).0;
        if p[3] < 128 {
            return Luma([0]);
        }
        let d = (0..3)
            .map(|i| u32::from(p[i].abs_diff(target[i])))
            .max()
            .unwrap_or(0);
        Luma([match d {
            d if d <= tol => 255,
            d if d >= 2 * tol => 0,
            d => (255 * (2 * tol - d) / tol) as u8,
        }])
    })
}

pub fn smooth_mask(mask: &GrayImage, factor: u32, sigma_source_px: f32) -> GrayImage {
    let f = factor.max(1);
    let (w, h) = (mask.width() * f, mask.height() * f);
    if w == 0 || h == 0 {
        return GrayImage::new(w, h);
    }
    let up = imageops::resize(mask, w, h, FilterType::Lanczos3);
    let sigma = sigma_source_px * f as f32;
    let soft = if sigma > 0.0 {
        imageops::fast_blur(&up, sigma)
    } else {
        up
    };
    GrayImage::from_fn(w, h, |x, y| {
        Luma([if soft.get_pixel(x, y)[0] >= 128 { 255 } else { 0 }])
    })
}

pub fn rasterize_path(d: &str, w: u32, h: u32, px_per_unit: f64) -> Result<GrayImage> {
    if w == 0 || h == 0 {
        return Ok(GrayImage::new(w, h));
    }
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\"><path transform=\"scale({})\" d=\"{}\"/></svg>",
        num(px_per_unit),
        esc(d)
    );
    let img = rasterize_svg(&svg, w, h)?;
    Ok(GrayImage::from_fn(w, h, |x, y| Luma([img.get_pixel(x, y)[3]])))
}

pub(crate) fn parse_hex(color: &str) -> Option<[u8; 3]> {
    let hex = color.strip_prefix('#')?;
    let hex = match hex.len() {
        6 => hex.to_string(),
        3 => hex.chars().flat_map(|c| [c, c]).collect(),
        _ => return None,
    };
    let v = u32::from_str_radix(&hex, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect_mask(w: u32, h: u32, r: [u32; 4]) -> GrayImage {
        GrayImage::from_fn(w, h, |x, y| {
            let on = x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3];
            Luma([if on { 255 } else { 0 }])
        })
    }

    fn iou(a: &GrayImage, b: &GrayImage) -> f64 {
        let (mut i, mut u) = (0, 0);
        for (p, q) in a.pixels().zip(b.pixels()) {
            let (x, y) = (p[0] >= 128, q[0] >= 128);
            i += u32::from(x && y);
            u += u32::from(x || y);
        }
        f64::from(i) / f64::from(u.max(1))
    }

    #[test]
    fn traced_rectangle_rasterizes_back() {
        let mask = rect_mask(40, 30, [10, 10, 20, 10]);
        let d = trace_mask(&mask, &TraceOptions::default()).expect("trace");
        assert!(d.starts_with('M'), "{d}");
        let back = rasterize_path(&d, 40, 30, 1.0).expect("raster");
        assert!(iou(&mask, &back) >= 0.95, "iou {}", iou(&mask, &back));

        let half = TraceOptions {
            mm_per_px: 0.5,
            ..Default::default()
        };
        let d = trace_mask(&mask, &half).expect("trace");
        let back = rasterize_path(&d, 40, 30, 2.0).expect("raster");
        assert!(iou(&mask, &back) >= 0.95);
    }

    #[test]
    fn traced_ring_keeps_its_hole() {
        let mut mask = rect_mask(50, 50, [5, 5, 40, 40]);
        for y in 15..35 {
            for x in 15..35 {
                mask.put_pixel(x, y, Luma([0]));
            }
        }
        let d = trace_mask(&mask, &TraceOptions::default()).expect("trace");
        let back = rasterize_path(&d, 50, 50, 1.0).expect("raster");
        assert_eq!(back.get_pixel(25, 25)[0], 0);
        assert!(iou(&mask, &back) >= 0.93, "iou {}", iou(&mask, &back));
    }

    #[test]
    fn color_mask_and_smooth_mask() {
        let mut img = RgbaImage::from_pixel(10, 10, image::Rgba([255, 255, 255, 255]));
        img.put_pixel(2, 3, image::Rgba([250, 5, 3, 255]));
        img.put_pixel(4, 4, image::Rgba([255, 0, 0, 0]));
        let m = color_mask(&img, "#FF0000", 10);
        assert_eq!(m.get_pixel(2, 3)[0], 255);
        assert_eq!(m.pixels().filter(|p| p[0] == 255).count(), 1);

        let up = smooth_mask(&rect_mask(8, 8, [2, 2, 4, 4]), 8, SMOOTH_SIGMA_SOURCE_PX);
        assert_eq!(up.dimensions(), (64, 64));
        assert_eq!(up.get_pixel(32, 32)[0], 255);
        assert_eq!(up.get_pixel(4, 4)[0], 0);
    }
}
