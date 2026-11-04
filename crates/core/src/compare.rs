use image::imageops::{self, FilterType};
use image::{GrayImage, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

use crate::image::resize_to;

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
    if original.is_empty() || work.is_empty() {
        return CompareResult {
            similarity: 0.0,
            overlap: 0.0,
            edge_similarity: 0.0,
            worst_regions: Vec::new(),
        };
    }
    let (a, b) = fit(original, work);
    let (w, h) = (a.width() as usize, a.height() as usize);
    let (la, lb) = (luma(&a), luma(&b));
    let worst_regions = worst_regions(&la, &lb, w, h);
    CompareResult {
        similarity: similarity(&la, &lb),
        overlap: iou(&foreground(&la, w, h), &foreground(&lb, w, h)),
        edge_similarity: similarity(&sobel(&la, w, h), &sobel(&lb, w, h)),
        worst_regions,
    }
}

pub fn diff_heatmap(original: &RgbaImage, work: &RgbaImage) -> RgbaImage {
    if original.is_empty() || work.is_empty() {
        return RgbaImage::new(0, 0);
    }
    let (a, b) = fit(original, work);
    RgbaImage::from_fn(a.width(), a.height(), |x, y| {
        let (p, q) = (over_white(a.get_pixel(x, y)), over_white(b.get_pixel(x, y)));
        let t = (0..3).map(|c| (p[c] - q[c]).abs()).fold(0.0, f32::max) / 255.0;
        let red = [255.0, 0.0, 0.0];
        let [r, g, b] = [0, 1, 2].map(|c| {
            let faded = (q[c] + 255.0) / 2.0;
            (faded + (red[c] - faded) * t).round() as u8
        });
        Rgba([r, g, b, 255])
    })
}

pub fn mask_overlap(a: &GrayImage, b: &GrayImage) -> f64 {
    let mask = |m: &GrayImage| m.pixels().map(|p| p[0] >= 128).collect::<Vec<_>>();
    if b.dimensions() == a.dimensions() {
        return iou(&mask(a), &mask(b));
    }
    iou(
        &mask(a),
        &mask(&imageops::resize(b, a.width(), a.height(), FilterType::Nearest)),
    )
}

fn fit(original: &RgbaImage, work: &RgbaImage) -> (RgbaImage, RgbaImage) {
    let w = original.width().min(work.width()).min(1024);
    let h = ((f64::from(work.height()) * f64::from(w) / f64::from(work.width())).round() as u32).max(1);
    (resize_to(original, w, h), resize_to(work, w, h))
}

fn over_white(p: &Rgba<u8>) -> [f32; 3] {
    let a = f32::from(p[3]) / 255.0;
    [0, 1, 2].map(|c| f32::from(p[c]) * a + 255.0 * (1.0 - a))
}

fn luma(img: &RgbaImage) -> Vec<f32> {
    img.pixels()
        .map(|p| {
            let [r, g, b] = over_white(p);
            0.299 * r + 0.587 * g + 0.114 * b
        })
        .collect()
}

fn similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.is_empty() {
        return 1.0;
    }
    let total: f64 = a.iter().zip(b).map(|(x, y)| f64::from((x - y).abs())).sum();
    1.0 - total / a.len() as f64 / 255.0
}

fn sobel(l: &[f32], w: usize, h: usize) -> Vec<f32> {
    let at = |x: usize, y: usize, dx: isize, dy: isize| {
        let sx = x.saturating_add_signed(dx).min(w - 1);
        let sy = y.saturating_add_signed(dy).min(h - 1);
        l[sy * w + sx]
    };
    (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            let gx = at(x, y, 1, -1) + 2.0 * at(x, y, 1, 0) + at(x, y, 1, 1)
                - at(x, y, -1, -1)
                - 2.0 * at(x, y, -1, 0)
                - at(x, y, -1, 1);
            let gy = at(x, y, -1, 1) + 2.0 * at(x, y, 0, 1) + at(x, y, 1, 1)
                - at(x, y, -1, -1)
                - 2.0 * at(x, y, 0, -1)
                - at(x, y, 1, -1);
            (gx * gx + gy * gy).sqrt().min(255.0)
        })
        .collect()
}

fn foreground(l: &[f32], w: usize, h: usize) -> Vec<bool> {
    let mut counts = [0u32; 32];
    let mut sums = [0f64; 32];
    let border = (0..w)
        .flat_map(|x| [x, (h - 1) * w + x])
        .chain((0..h).flat_map(|y| [y * w, y * w + w - 1]));
    for i in border {
        let bin = ((l[i] / 8.0) as usize).min(31);
        counts[bin] += 1;
        sums[bin] += f64::from(l[i]);
    }
    let best = (0..32).max_by_key(|&b| counts[b]).unwrap_or(0);
    let background = sums[best] / f64::from(counts[best].max(1));
    l.iter()
        .map(|&v| (f64::from(v) - background).abs() > 40.0)
        .collect()
}

fn iou(a: &[bool], b: &[bool]) -> f64 {
    let inter = a.iter().zip(b).filter(|(x, y)| **x && **y).count();
    let union = a.iter().zip(b).filter(|(x, y)| **x || **y).count();
    if union == 0 {
        1.0
    } else {
        inter as f64 / union as f64
    }
}

fn worst_regions(la: &[f32], lb: &[f32], w: usize, h: usize) -> Vec<DiffRegion> {
    let mut regions = Vec::new();
    for row in 0..4 {
        for col in 0..8 {
            let (x0, x1) = (col * w / 8, (col + 1) * w / 8);
            let (y0, y1) = (row * h / 4, (row + 1) * h / 4);
            if x0 == x1 || y0 == y1 {
                continue;
            }
            let mut total = 0.0;
            for y in y0..y1 {
                for x in x0..x1 {
                    total += f64::from((la[y * w + x] - lb[y * w + x]).abs());
                }
            }
            let score = 1.0 - total / ((x1 - x0) * (y1 - y0)) as f64 / 255.0;
            if score < 1.0 {
                regions.push(DiffRegion {
                    x: x0 as f64 / w as f64,
                    y: y0 as f64 / h as f64,
                    w: (x1 - x0) as f64 / w as f64,
                    h: (y1 - y0) as f64 / h as f64,
                    score,
                });
            }
        }
    }
    regions.sort_by(|a, b| a.score.total_cmp(&b.score));
    regions.truncate(5);
    regions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn banner(box_x: u32) -> RgbaImage {
        RgbaImage::from_fn(160, 80, |x, y| {
            if (box_x..box_x + 40).contains(&x) && (20..60).contains(&y) {
                Rgba([20, 20, 120, 255])
            } else {
                Rgba([250, 250, 250, 255])
            }
        })
    }

    #[test]
    fn identical_images_score_perfectly() {
        let r = compare(&banner(20), &banner(20));
        assert!((r.similarity - 1.0).abs() < 1e-9);
        assert!((r.edge_similarity - 1.0).abs() < 1e-9);
        assert!((r.overlap - 1.0).abs() < 1e-9);
        assert!(r.worst_regions.is_empty());
    }

    #[test]
    fn shifted_box_is_located() {
        let r = compare(&banner(20), &banner(100));
        assert!(r.similarity < 0.9 && r.similarity > 0.5);
        assert!(r.edge_similarity < 1.0);
        assert!(r.overlap < 0.01);
        assert_eq!(r.worst_regions.len(), 5);
        assert!(r.worst_regions.windows(2).all(|p| p[0].score <= p[1].score));
        let worst = &r.worst_regions[0];
        assert!(worst.y >= 0.25 && worst.y < 0.75);
        assert!((worst.w - 0.125).abs() < 1e-9 && (worst.h - 0.25).abs() < 1e-9);
    }

    #[test]
    fn sizes_are_matched_to_work() {
        let big = image::imageops::resize(&banner(20), 320, 160, FilterType::Nearest);
        let r = compare(&big, &banner(20));
        assert!(r.similarity > 0.97);
        assert!(r.overlap > 0.9);
    }

    #[test]
    fn heatmap_marks_differences_red() {
        let map = diff_heatmap(&banner(20), &banner(100));
        assert_eq!(map.dimensions(), (160, 80));
        assert_eq!(*map.get_pixel(0, 0), Rgba([253, 253, 253, 255]));
        let p = map.get_pixel(30, 40);
        assert!(p[0] > 200 && p[1] < 40 && p[2] < 40, "{p:?}");
    }

    #[test]
    fn mask_iou() {
        let a = GrayImage::from_fn(10, 10, |x, _| image::Luma([if x < 5 { 255 } else { 0 }]));
        let b = GrayImage::from_fn(10, 10, |x, _| image::Luma([if x < 10 { 200 } else { 0 }]));
        assert!((mask_overlap(&a, &b) - 0.5).abs() < 1e-9);
        assert!((mask_overlap(&a, &a) - 1.0).abs() < 1e-9);
        let small = GrayImage::from_fn(5, 5, |x, _| image::Luma([if x < 3 { 255 } else { 0 }]));
        assert!(mask_overlap(&a, &small) > 0.5);
    }
}
