use image::{GrayImage, Luma, RgbaImage};

use crate::model::ShapeProps;
use crate::render::{esc, num, rasterize_svg};
use crate::{CoreError, Result};

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
    let (w, h) = mask.dimensions();
    if w == 0 || h == 0 {
        return Ok(String::new());
    }
    let mut pixels = Vec::with_capacity(w as usize * h as usize * 4);
    for p in mask.pixels() {
        let v = if p[0] >= 128 { 0 } else { 255 };
        pixels.extend_from_slice(&[v, v, v, 255]);
    }
    let img = vtracer::ColorImage { pixels, width: w as usize, height: h as usize };
    let config = vtracer::Config {
        color_mode: vtracer::ColorMode::Binary,
        filter_speckle: f64::from(opts.min_area_px).sqrt().ceil() as usize,
        corner_threshold: if opts.smooth { opts.corner_threshold_deg.round() as i32 } else { 0 },
        ..Default::default()
    };
    let svg = vtracer::convert(img, config).map_err(CoreError::Render)?;
    let mut d = String::new();
    for path in &svg.paths {
        let el = path.to_string();
        let (Some(raw), Some((tx, ty))) = (attr(&el, "d=\""), translate(&el)) else {
            return Err(CoreError::Render(format!("unexpected vtracer output: {el}")));
        };
        transform_d(raw, tx, ty, opts.mm_per_px, &mut d);
    }
    Ok(d.trim_end().to_string())
}

fn attr<'a>(el: &'a str, key: &str) -> Option<&'a str> {
    let start = el.find(key)? + key.len();
    let len = el[start..].find('"')?;
    Some(&el[start..start + len])
}

fn translate(el: &str) -> Option<(f64, f64)> {
    let t = attr(el, "transform=\"")?;
    let inner = t.strip_prefix("translate(")?.strip_suffix(')')?;
    let (x, y) = inner.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

fn transform_d(d: &str, tx: f64, ty: f64, s: f64, out: &mut String) {
    let mut token = String::new();
    let mut idx = 0usize;
    let flush = |token: &mut String, idx: &mut usize, out: &mut String| {
        if let Ok(v) = token.parse::<f64>() {
            let v = if idx.is_multiple_of(2) { (v + tx) * s } else { (v + ty) * s };
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

pub fn alpha_mask(img: &RgbaImage, threshold: u8) -> GrayImage {
    let (w, h) = img.dimensions();
    GrayImage::from_fn(w, h, |x, y| Luma([if img.get_pixel(x, y)[3] > threshold { 255 } else { 0 }]))
}

pub fn upsample_mask(mask: &GrayImage, factor: u32) -> GrayImage {
    let f = factor.max(1);
    let (w, h) = (mask.width() * f, mask.height() * f);
    let up = GrayImage::from_fn(w, h, |x, y| *mask.get_pixel(x / f, y / f));
    if w == 0 || h == 0 {
        return up;
    }
    GrayImage::from_fn(w, h, |x, y| {
        let mut sum = 0u32;
        for dy in -1i64..=1 {
            for dx in -1i64..=1 {
                let sx = (i64::from(x) + dx).clamp(0, i64::from(w) - 1) as u32;
                let sy = (i64::from(y) + dy).clamp(0, i64::from(h) - 1) as u32;
                sum += u32::from(up.get_pixel(sx, sy)[0]);
            }
        }
        Luma([if sum / 9 >= 128 { 255 } else { 0 }])
    })
}

pub fn detect_shape(mask: &GrayImage) -> Option<(ShapeProps, f64)> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    let mut area = 0u64;
    for (x, y, p) in mask.enumerate_pixels() {
        if p[0] >= 128 {
            area += 1;
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
    }
    if area == 0 {
        return None;
    }
    let (bw, bh) = (f64::from(x1 - x0 + 1), f64::from(y1 - y0 + 1));
    let fill = area as f64 / (bw * bh);
    if fill >= 0.97 {
        return Some((ShapeProps::Rect { rx_mm: 0.0 }, fill));
    }
    let (cx, cy) = (f64::from(x0) + bw / 2.0, f64::from(y0) + bh / 2.0);
    let (mut inter, mut uni) = (0u64, 0u64);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = (f64::from(x) + 0.5 - cx) / (bw / 2.0);
            let dy = (f64::from(y) + 0.5 - cy) / (bh / 2.0);
            let inside = dx * dx + dy * dy <= 1.0;
            let on = mask.get_pixel(x, y)[0] >= 128;
            inter += u64::from(inside && on);
            uni += u64::from(inside || on);
        }
    }
    let iou = inter as f64 / uni.max(1) as f64;
    (iou >= 0.95).then_some((ShapeProps::Ellipse, iou))
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

        let half = TraceOptions { mm_per_px: 0.5, ..Default::default() };
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
    fn masks_and_shapes() {
        let mut img = RgbaImage::from_pixel(10, 10, image::Rgba([255, 255, 255, 255]));
        img.put_pixel(2, 3, image::Rgba([250, 5, 3, 255]));
        img.put_pixel(4, 4, image::Rgba([255, 0, 0, 0]));
        let m = color_mask(&img, "#FF0000", 10);
        assert_eq!(m.get_pixel(2, 3)[0], 255);
        assert_eq!(m.pixels().filter(|p| p[0] == 255).count(), 1);
        assert_eq!(alpha_mask(&img, 0).get_pixel(4, 4)[0], 0);

        let up = upsample_mask(&rect_mask(4, 4, [1, 1, 2, 2]), 4);
        assert_eq!(up.dimensions(), (16, 16));
        assert_eq!(up.get_pixel(8, 8)[0], 255);
        assert_eq!(up.get_pixel(1, 1)[0], 0);

        let (shape, conf) = detect_shape(&rect_mask(30, 30, [5, 5, 20, 10])).expect("rect");
        assert_eq!(shape, ShapeProps::Rect { rx_mm: 0.0 });
        assert!(conf >= 0.97);
        let ellipse = GrayImage::from_fn(60, 40, |x, y| {
            let dx = (f64::from(x) + 0.5 - 30.0) / 25.0;
            let dy = (f64::from(y) + 0.5 - 20.0) / 15.0;
            Luma([if dx * dx + dy * dy <= 1.0 { 255 } else { 0 }])
        });
        assert_eq!(detect_shape(&ellipse).map(|s| s.0), Some(ShapeProps::Ellipse));
        let tri = GrayImage::from_fn(40, 40, |x, y| Luma([if x <= y { 255 } else { 0 }]));
        assert!(detect_shape(&tri).is_none());
    }
}
