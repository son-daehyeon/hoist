#![allow(clippy::print_stdout)]

use std::path::PathBuf;

use hoist_core::{compare, image as cimg, trace};
use image::{GrayImage, Luma, Rgba, RgbaImage};

struct Sample {
    name: &'static str,
    search: [u32; 4],
    color: &'static str,
    rgb: [u8; 3],
}

const SAMPLES: [Sample; 2] = [
    Sample {
        name: "주민역량강화",
        search: [500, 70, 600, 130],
        color: "#1C6B33",
        rgb: [0x1C, 0x6B, 0x33],
    },
    Sample {
        name: "참여자 모집",
        search: [1090, 70, 460, 130],
        color: "#F2511B",
        rgb: [0xF2, 0x51, 0x1B],
    },
];

const TOLERANCE: u8 = 40;
const PAD: u32 = 4;

fn bbox(mask: &GrayImage) -> Option<[u32; 4]> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in mask.enumerate_pixels() {
        if p[0] >= 128 {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
    }
    (x0 != u32::MAX).then(|| [x0, y0, x1 - x0 + 1, y1 - y0 + 1])
}

const WIDTH_PX: u32 = 1000;

fn old_upsample(mask: &GrayImage, f: u32) -> GrayImage {
    let (w, h) = (mask.width() * f, mask.height() * f);
    let up = GrayImage::from_fn(w, h, |x, y| *mask.get_pixel(x / f, y / f));
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

fn nearest(mask: &GrayImage, f: u32) -> GrayImage {
    GrayImage::from_fn(mask.width() * f, mask.height() * f, |x, y| {
        *mask.get_pixel(x / f, y / f)
    })
}

fn render(d: &str, rect: [u32; 4], rgb: [u8; 3]) -> anyhow::Result<RgbaImage> {
    let scale = f64::from(WIDTH_PX) / f64::from(rect[2]);
    let h = (f64::from(rect[3]) * scale).round() as u32;
    let alpha = trace::rasterize_path(d, WIDTH_PX, h, scale)?;
    Ok(RgbaImage::from_fn(WIDTH_PX, h, |x, y| {
        let a = u32::from(alpha.get_pixel(x, y)[0]);
        let mix = |c: u8| ((u32::from(c) * a + 255 * (255 - a)) / 255) as u8;
        Rgba([mix(rgb[0]), mix(rgb[1]), mix(rgb[2]), 255])
    }))
}

fn stack(rows: &[RgbaImage]) -> RgbaImage {
    let gap = 24;
    let h = rows.iter().map(|r| r.height()).sum::<u32>() + gap * (rows.len() as u32 + 1);
    let mut out = RgbaImage::from_pixel(WIDTH_PX, h, Rgba([255, 255, 255, 255]));
    let mut y = gap;
    for r in rows {
        image::imageops::overlay(&mut out, r, 0, i64::from(y));
        y += r.height() + gap;
    }
    out
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/samples");
    let original = cimg::load(&root.join("cheongjeon_original.png"))?;
    let (mut before, mut after) = (Vec::new(), Vec::new());
    for s in &SAMPLES {
        let area = cimg::crop(&original, s.search);
        let b = bbox(&trace::color_mask(&area, s.color, TOLERANCE))
            .ok_or_else(|| anyhow::anyhow!("{} not found", s.name))?;
        let rect = [
            s.search[0] + b[0].saturating_sub(PAD),
            s.search[1] + b[1].saturating_sub(PAD),
            b[2] + 2 * PAD,
            b[3] + 2 * PAD,
        ];
        let crop = cimg::crop(&original, rect);
        let mask = trace::color_mask(&crop, s.color, TOLERANCE);

        let old_mask = old_upsample(&mask, 4);
        let old_opts = trace::TraceOptions {
            mm_per_px: 0.25,
            min_area_px: 16,
            ..Default::default()
        };
        let old_d = trace::trace_mask(&old_mask, &old_opts)?;
        let old_back = trace::rasterize_path(&old_d, old_mask.width(), old_mask.height(), 4.0)?;

        let f = trace::SMOOTH_FACTOR;
        let coverage = trace::color_coverage(&crop, s.color, TOLERANCE);
        let new_mask = trace::smooth_mask(&coverage, f, trace::SMOOTH_SIGMA_SOURCE_PX);
        let new_d = trace::trace_mask(&new_mask, &trace::TraceOptions::upsampled(1.0, f))?;
        let new_back = trace::rasterize_path(&new_d, new_mask.width(), new_mask.height(), f64::from(f))?;

        println!("{} crop {:?}", s.name, rect);
        println!(
            "{}: before iou {:.3} (vs source {:.3}, {} chars) · after iou {:.3} (vs source {:.3}, {} chars)",
            s.name,
            compare::mask_overlap(&old_mask, &old_back),
            compare::mask_overlap(&nearest(&mask, 4), &old_back),
            old_d.len(),
            compare::mask_overlap(&new_mask, &new_back),
            compare::mask_overlap(&nearest(&mask, f), &new_back),
            new_d.len(),
        );
        before.push(render(&old_d, rect, s.rgb)?);
        after.push(render(&new_d, rect, s.rgb)?);
    }
    cimg::save_png(&stack(&before), &root.join("trace-before.png"))?;
    cimg::save_png(&stack(&after), &root.join("trace-after.png"))?;
    println!("wrote docs/samples/trace-before.png, docs/samples/trace-after.png");
    Ok(())
}
