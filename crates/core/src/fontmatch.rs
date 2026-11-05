use image::RgbaImage;
use image::imageops::{self, FilterType};
use serde::Serialize;

use crate::Result;
use crate::model::TextProps;
use crate::render::{esc, num, rasterize_svg};
use crate::text::Fonts;

const UPSCALE: usize = 4;
const SIZE_MM: f64 = 40.0;
const PX_PER_MM: f64 = 4.0;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FontMatch {
    pub family: String,
    pub score: f64,
    pub scale_x: Option<f64>,
    pub letter_spacing_em: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FontFit {
    pub matches: Vec<FontMatch>,
    pub reliable: bool,
}

struct Mask {
    w: usize,
    h: usize,
    cover: Vec<u8>,
}

impl Mask {
    fn on(&self, x: usize, y: usize) -> bool {
        self.cover[y * self.w + x] >= 128
    }

    fn bbox(&self) -> Option<[usize; 4]> {
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        for y in 0..self.h {
            for x in 0..self.w {
                if self.on(x, y) {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        (x0 != usize::MAX).then(|| [x0, y0, x1 - x0 + 1, y1 - y0 + 1])
    }
}

fn otsu(vals: &[u8]) -> u8 {
    let mut hist = [0f64; 256];
    for &v in vals {
        hist[usize::from(v)] += 1.0;
    }
    let total = vals.len() as f64;
    let sum: f64 = hist.iter().enumerate().map(|(i, n)| i as f64 * n).sum();
    let (mut sum_b, mut w_b, mut best, mut thr) = (0.0, 0.0, 0.0, 0u8);
    for (i, n) in hist.iter().enumerate() {
        w_b += n;
        if w_b == 0.0 {
            continue;
        }
        let w_f = total - w_b;
        if w_f == 0.0 {
            break;
        }
        sum_b += i as f64 * n;
        let (m_b, m_f) = (sum_b / w_b, (sum - sum_b) / w_f);
        let v = w_b * w_f * (m_b - m_f).powi(2);
        if v > best {
            best = v;
            thr = i as u8;
        }
    }
    thr
}

/// Ink of an original crop: upscaled, background = median of the border, Otsu on the distance from it.
fn crop_mask(original: &RgbaImage, rect: [u32; 4]) -> Mask {
    let crop = crate::image::crop(original, rect);
    let k = UPSCALE as u32;
    let (w, h) = (crop.width() * k, crop.height() * k);
    if w == 0 || h == 0 {
        return Mask {
            w: 0,
            h: 0,
            cover: vec![],
        };
    }
    let up = imageops::resize(&crop, w, h, FilterType::Triangle);
    let mut border: [Vec<u8>; 3] = Default::default();
    let mut push = |x: u32, y: u32| {
        let p = up.get_pixel(x, y);
        for (c, b) in border.iter_mut().enumerate() {
            b.push(p[c]);
        }
    };
    for x in 0..w {
        push(x, 0);
        push(x, h - 1);
    }
    for y in 0..h {
        push(0, y);
        push(w - 1, y);
    }
    let bg = border.map(|mut b| {
        b.sort_unstable();
        b[b.len() / 2]
    });
    let dist: Vec<u8> = up
        .pixels()
        .map(|p| (0..3).map(|c| p[c].abs_diff(bg[c])).max().unwrap_or(0))
        .collect();
    let thr = otsu(&dist);
    Mask {
        w: w as usize,
        h: h as usize,
        cover: dist.iter().map(|&d| if d > thr { 255 } else { 0 }).collect(),
    }
}

fn render_mask(
    fonts: &Fonts,
    text: &str,
    family: &str,
    weight: u16,
    tracking_em: f64,
) -> Result<Option<Mask>> {
    let props = TextProps {
        text: text.to_string(),
        font_family: family.to_string(),
        font_weight: weight,
        font_size_mm: SIZE_MM,
        letter_spacing_mm: tracking_em * SIZE_MM,
        ..Default::default()
    };
    let run = fonts.layout_to_path(&props, f64::NAN, SIZE_MM * 2.0)?;
    let [bx, by, bw, bh] = run.bbox_mm;
    if bw <= 0.0 || bh <= 0.0 {
        return Ok(None);
    }
    let pad = 1.0;
    let (w, h) = (
        ((bw + 2.0 * pad) * PX_PER_MM).ceil() as u32,
        ((bh + 2.0 * pad) * PX_PER_MM).ceil() as u32,
    );
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\"><path transform=\"scale({}) translate({} {})\" d=\"{}\"/></svg>",
        num(PX_PER_MM),
        num(pad - bx),
        num(pad - by),
        esc(&run.path_d)
    );
    let img = rasterize_svg(&svg, w, h)?;
    Ok(Some(Mask {
        w: w as usize,
        h: h as usize,
        cover: img.pixels().map(|p| p[3]).collect(),
    }))
}

fn bilinear(m: &Mask, x: f64, y: f64) -> f64 {
    let x = (x - 0.5).clamp(0.0, (m.w - 1) as f64);
    let y = (y - 0.5).clamp(0.0, (m.h - 1) as f64);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(m.w - 1), (y0 + 1).min(m.h - 1));
    let (fx, fy) = (x - x0 as f64, y - y0 as f64);
    let at = |x: usize, y: usize| f64::from(m.cover[y * m.w + x]);
    let top = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
    let bottom = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
    top * (1.0 - fy) + bottom * fy
}

/// IoU of the crop's ink with the render's ink stretched onto the crop's ink box.
fn overlap(crop: &Mask, cb: [usize; 4], render: &Mask, rb: [usize; 4]) -> f64 {
    let (sx, sy) = (rb[2] as f64 / cb[2] as f64, rb[3] as f64 / cb[3] as f64);
    let (mut inter, mut union) = (0u64, 0u64);
    for y in 0..crop.h {
        for x in 0..crop.w {
            let a = crop.on(x, y);
            let inside = (cb[0]..cb[0] + cb[2]).contains(&x) && (cb[1]..cb[1] + cb[3]).contains(&y);
            let b = inside
                && bilinear(
                    render,
                    rb[0] as f64 + (x - cb[0]) as f64 * sx + sx / 2.0,
                    rb[1] as f64 + (y - cb[1]) as f64 * sy + sy / 2.0,
                ) >= 128.0;
            inter += u64::from(a && b);
            union += u64::from(a || b);
        }
    }
    if union == 0 {
        0.0
    } else {
        inter as f64 / union as f64
    }
}

/// The first text line's band inside a multi-line crop, in original pixels.
fn first_line(original: &RgbaImage, rect: [u32; 4]) -> [u32; 4] {
    let m = crop_mask(original, rect);
    let mut bands = Vec::new();
    let mut start: Option<usize> = None;
    for y in 0..m.h {
        let on = (0..m.w).filter(|&x| m.on(x, y)).count() as f64 > m.w as f64 * 0.01;
        if on && start.is_none() {
            start = Some(y);
        }
        if let Some(s) = start
            && (!on || y == m.h - 1)
        {
            bands.push((s, y));
            start = None;
        }
    }
    match bands.into_iter().find(|(s, e)| e - s >= 6 * UPSCALE) {
        Some((s, e)) => [
            rect[0],
            rect[1] + (s / UPSCALE).saturating_sub(1) as u32,
            rect[2],
            (e - s).div_ceil(UPSCALE) as u32 + 2,
        ],
        None => rect,
    }
}

fn round(v: f64, digits: i32) -> f64 {
    let p = 10f64.powi(digits);
    (v * p).round() / p
}

/// How well each family reproduces the first line of `text` in `rect` of the original, with the scale_x and
/// tracking that make its ink box match the original's.
pub fn fit_fonts(
    fonts: &Fonts,
    original: &RgbaImage,
    rect: [u32; 4],
    text: &str,
    families: &[String],
    weight: u16,
) -> Result<FontFit> {
    let lines: Vec<&str> = text.split('\n').filter(|l| !l.trim().is_empty()).collect();
    let none = FontFit {
        matches: vec![],
        reliable: false,
    };
    let Some(line) = lines.first() else {
        return Ok(none);
    };
    let rect = if lines.len() > 1 {
        first_line(original, rect)
    } else {
        rect
    };
    let crop = crop_mask(original, rect);
    let Some(cb) = crop.bbox() else {
        return Ok(none);
    };
    let mut matches = Vec::new();
    for family in families {
        let mut best: Option<FontMatch> = None;
        for step in 0..=8 {
            let t = -0.10 + 0.02 * f64::from(step);
            let Some(render) = render_mask(fonts, line, family, weight, t)? else {
                continue;
            };
            let Some(rb) = render.bbox() else {
                continue;
            };
            let score = overlap(&crop, cb, &render, rb);
            if best.as_ref().is_none_or(|b| score > b.score) {
                let scale_x = (cb[2] as f64 / cb[3] as f64) / (rb[2] as f64 / rb[3] as f64);
                best = Some(FontMatch {
                    family: family.clone(),
                    score,
                    scale_x: Some(scale_x),
                    letter_spacing_em: Some(t),
                });
            }
        }
        if let Some(mut m) = best {
            m.score = round(m.score, 3);
            m.scale_x = m.scale_x.map(|s| round(s, 2)).filter(|s| (0.6..=1.3).contains(s));
            m.letter_spacing_em = m.scale_x.and(m.letter_spacing_em.map(|t| round(t, 2)));
            matches.push(m);
        }
    }
    matches.sort_by(|a, b| b.score.total_cmp(&a.score));
    let ink_h = cb[3] as f64 / UPSCALE as f64;
    let reliable = matches
        .first()
        .is_some_and(|m| m.score >= 0.55 && ink_h >= 20.0 && m.scale_x.is_some());
    Ok(FontFit { matches, reliable })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn the_true_family_ranks_first_with_its_squeeze() {
        let fonts = Fonts::new(Some(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/fonts"),
        ));
        let text = "현수막 제작";
        let run = fonts
            .layout_to_path(
                &TextProps {
                    text: text.into(),
                    font_family: "Pretendard".into(),
                    font_weight: 700,
                    font_size_mm: 30.0,
                    scale_x: 0.85,
                    ..Default::default()
                },
                f64::NAN,
                60.0,
            )
            .expect("layout");
        let [bx, by, bw, bh] = run.bbox_mm;
        let (w, h) = (((bw + 20.0) * 4.0) as u32, ((bh + 20.0) * 4.0) as u32);
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\"><rect width=\"{w}\" height=\"{h}\" fill=\"#F4EEDC\"/><path fill=\"#1E2A44\" transform=\"scale(4) translate({} {})\" d=\"{}\"/></svg>",
            num(10.0 - bx),
            num(10.0 - by),
            esc(&run.path_d)
        );
        let original = rasterize_svg(&svg, w, h).expect("original");
        let families: Vec<String> = ["Noto Serif KR", "Pretendard", "Gaegu"]
            .map(String::from)
            .to_vec();
        let fit = fit_fonts(&fonts, &original, [0, 0, w, h], text, &families, 700).expect("fit");
        let top = &fit.matches[0];
        assert_eq!(top.family, "Pretendard", "{fit:?}");
        let sx = top.scale_x.expect("fitted scale_x");
        assert!((sx - 0.85).abs() <= 0.05, "{fit:?}");
        assert!(fit.reliable, "{fit:?}");
        assert!(fit.matches.windows(2).all(|p| p[0].score >= p[1].score));

        let two = fit_fonts(
            &fonts,
            &original,
            [0, 0, w, h],
            &format!("{text}\n두 번째 줄"),
            &families,
            700,
        )
        .expect("multi-line");
        assert_eq!(two.matches[0].family, "Pretendard", "{two:?}");
    }
}
