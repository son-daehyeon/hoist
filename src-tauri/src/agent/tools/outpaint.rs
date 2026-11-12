use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::bail;
use hoist_core::image as cimg;
use image::RgbaImage;

pub(super) fn trim_bars(im: RgbaImage) -> RgbaImage {
    let (w, h) = im.dimensions();
    let empty = |p: &image::Rgba<u8>| p.0[3] < 16 || p.0[..3].iter().all(|v| *v < 24);
    let row = |y: u32| (0..w).filter(|x| empty(im.get_pixel(*x, y))).count() as f64 >= f64::from(w) * 0.97;
    let col = |x: u32| (0..h).filter(|y| empty(im.get_pixel(x, *y))).count() as f64 >= f64::from(h) * 0.97;
    let top = (0..h).take_while(|y| row(*y)).count() as u32;
    let bottom = (0..h).rev().take_while(|y| *y >= top && row(*y)).count() as u32;
    let left = (0..w).take_while(|x| col(*x)).count() as u32;
    let right = (0..w).rev().take_while(|x| *x >= left && col(*x)).count() as u32;
    let (nw, nh) = (w - left - right, h - top - bottom);
    if (top + bottom + left + right == 0) || nw < w / 2 || nh < h / 2 {
        return im;
    }
    // A couple of pixels more so the soft edge of the bar does not stay as a dark line.
    let (m_y, m_x) = (u32::from(top + bottom > 0) * 2, u32::from(left + right > 0) * 2);
    cimg::crop(
        &im,
        [
            left + m_x,
            top + m_y,
            nw.saturating_sub(2 * m_x),
            nh.saturating_sub(2 * m_y),
        ],
    )
}

pub(super) fn snap16(v: u32) -> u32 {
    ((v + 8) / 16 * 16).max(16)
}

/// Spread ratio held to 0.7–1.4 so flat strips do not blow up.
fn match_colors(
    img: &mut RgbaImage,
    img_cols: std::ops::Range<u32>,
    reference: &RgbaImage,
    ref_cols: std::ops::Range<u32>,
) {
    let stats = |im: &RgbaImage, cols: &std::ops::Range<u32>| {
        let mut sum = [0.0f64; 3];
        let mut sq = [0.0f64; 3];
        let mut n = 0.0;
        for y in (0..im.height()).step_by(2) {
            for x in cols.clone().step_by(2) {
                let p = im.get_pixel(x, y).0;
                for c in 0..3 {
                    let v = f64::from(p[c]);
                    sum[c] += v;
                    sq[c] += v * v;
                }
                n += 1.0;
            }
        }
        let n = f64::max(n, 1.0);
        let mean = sum.map(|v| v / n);
        let std = [0, 1, 2].map(|c| (sq[c] / n - mean[c] * mean[c]).max(0.0).sqrt());
        (mean, std)
    };
    let (gm, gs) = stats(img, &img_cols);
    let (rm, rs) = stats(reference, &ref_cols);
    let scale = [0, 1, 2].map(|c| {
        if gs[c] < 1.0 {
            1.0
        } else {
            (rs[c] / gs[c]).clamp(0.7, 1.4)
        }
    });
    for p in img.pixels_mut() {
        for c in 0..3 {
            p.0[c] = ((f64::from(p.0[c]) - gm[c]) * scale[c] + rm[c])
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
}

const EXTEND_MIN_STEP: u32 = 16;
const MAX_EXTEND_CALLS: u32 = 12;
/// Half-width of the cross-fade around the splice path: two different paintings cross-faded over a wide band
/// leave ghosted, milky strips.
const SPLICE_FEATHER: f64 = 24.0;

/// Minimum-difference seam through the overlap, at most one column per row (dynamic programming).
fn splice_path(
    cur: &RgbaImage,
    generated: &RgbaImage,
    overlap: u32,
    cur_at: &dyn Fn(u32) -> u32,
    gen_at: &dyn Fn(u32) -> u32,
) -> Vec<u32> {
    let h = cur.height() as usize;
    let margin = (SPLICE_FEATHER as u32 + 1).min(overlap / 2);
    let (lo, hi) = (margin, overlap.saturating_sub(margin).max(margin + 1));
    let w = (hi - lo) as usize;
    let cost = |y: usize, i: usize| -> f64 {
        let x = lo + i as u32;
        let a = cur.get_pixel(cur_at(x), y as u32).0;
        let b = generated
            .get_pixel(gen_at(x).min(generated.width() - 1), y as u32)
            .0;
        (0..3).map(|k| (f64::from(a[k]) - f64::from(b[k])).abs()).sum()
    };
    let mut acc = vec![0.0f64; h * w];
    let mut from = vec![0usize; h * w];
    for (i, slot) in acc.iter_mut().take(w).enumerate() {
        *slot = cost(0, i);
    }
    for y in 1..h {
        for i in 0..w {
            let (mut best, mut at) = (acc[(y - 1) * w + i], i);
            for j in [i.wrapping_sub(1), i + 1] {
                if j < w && acc[(y - 1) * w + j] < best {
                    (best, at) = (acc[(y - 1) * w + j], j);
                }
            }
            acc[y * w + i] = best + cost(y, i);
            from[y * w + i] = at;
        }
    }
    let mut i = (0..w)
        .min_by(|a, b| acc[(h - 1) * w + a].total_cmp(&acc[(h - 1) * w + b]))
        .unwrap_or(0);
    let mut path = vec![0u32; h];
    for y in (0..h).rev() {
        path[y] = lo + i as u32;
        i = from[y * w + i];
    }
    path
}

fn chunk_canvas(cur: &RgbaImage, overlap: u32, chunk_w: u32, right_side: bool) -> (RgbaImage, RgbaImage) {
    let target_h = cur.height();
    let mut canvas = RgbaImage::from_pixel(chunk_w, target_h, image::Rgba([0, 0, 0, 0]));
    let mut mask = canvas.clone();
    for y in 0..target_h {
        for i in 0..overlap {
            let (src_x, dst_x) = if right_side {
                (cur.width() - overlap + i, i)
            } else {
                (i, chunk_w - overlap + i)
            };
            canvas.put_pixel(dst_x, y, *cur.get_pixel(src_x, y));
            mask.put_pixel(dst_x, y, image::Rgba([0, 0, 0, 255]));
        }
    }
    (canvas, mask)
}

fn splice_chunk(
    cur: &RgbaImage,
    generated: &RgbaImage,
    overlap: u32,
    take: u32,
    right_side: bool,
) -> RgbaImage {
    let (chunk_w, target_h) = generated.dimensions();
    let gen_at = |i: u32| if right_side { i } else { chunk_w - overlap + i };
    let cur_at = |i: u32| if right_side { cur.width() - overlap + i } else { i };
    let seam = splice_path(cur, generated, overlap, &cur_at, &gen_at);
    let mut next = RgbaImage::new(cur.width() + take, target_h);
    let (cur_off, gen_src, gen_off) = if right_side {
        (0, overlap, cur.width())
    } else {
        (take, chunk_w - overlap - take, 0)
    };
    for y in 0..target_h {
        for x in 0..cur.width() {
            next.put_pixel(x + cur_off, y, *cur.get_pixel(x, y));
        }
        for x in 0..take {
            next.put_pixel(x + gen_off, y, *generated.get_pixel(gen_src + x, y));
        }
        let path = f64::from(seam[y as usize]);
        for i in 0..overlap {
            let toward_new = if right_side {
                f64::from(i) - path
            } else {
                path - f64::from(i)
            };
            let t = ((toward_new + SPLICE_FEATHER) / (2.0 * SPLICE_FEATHER)).clamp(0.0, 1.0);
            if t == 0.0 {
                continue;
            }
            let (cur_x, gen_x) = (cur_at(i), gen_at(i));
            let a = cur.get_pixel(cur_x, y).0;
            let b = generated.get_pixel(gen_x, y).0;
            let mix = |k: usize| (f64::from(a[k]) * (1.0 - t) + f64::from(b[k]) * t).round() as u8;
            next.put_pixel(cur_x + cur_off, y, image::Rgba([mix(0), mix(1), mix(2), 255]));
        }
    }
    next
}

/// Ends on its own: a last gap under 16 px, the call cap, or a step that adds nothing is closed by resizing.
pub(super) async fn outpaint<F, Fut>(
    mut cur: RgbaImage,
    target_w: u32,
    stop: &AtomicBool,
    mut generate: F,
) -> anyhow::Result<RgbaImage>
where
    F: FnMut(RgbaImage, RgbaImage, RgbaImage) -> Fut,
    Fut: Future<Output = anyhow::Result<RgbaImage>>,
{
    let target_h = cur.height();
    let plate_w = cur.width();
    let mut left_added = 0u32;
    let max_w = snap16((3 * target_h).min(3840));
    let mut calls = 0;
    while cur.width() + EXTEND_MIN_STEP <= target_w && calls < MAX_EXTEND_CALLS {
        if stop.load(Ordering::SeqCst) {
            bail!("사용자가 작업을 중단했어요");
        }
        let before = cur.width();
        let remaining = target_w - cur.width();
        let (left, right) = if remaining < 2 * EXTEND_MIN_STEP {
            (0, remaining)
        } else {
            (remaining / 2, remaining - remaining / 2)
        };
        for (right_side, need) in [(true, right), (false, left)] {
            if need == 0 || calls >= MAX_EXTEND_CALLS {
                continue;
            }
            let overlap = cur.width().min(target_h);
            let chunk_w = ((overlap + need).div_ceil(16) * 16).min(max_w);
            let new_px = chunk_w.saturating_sub(overlap);
            if new_px == 0 {
                continue;
            }
            let (canvas, mask) = chunk_canvas(&cur, overlap, chunk_w, right_side);
            let generated = generate(canvas, mask, cimg::resize_max(&cur, 1024)).await?;
            calls += 1;
            let generated = trim_bars(generated);
            let mut generated = if generated.dimensions() == (chunk_w, target_h) {
                generated
            } else {
                cimg::resize_to(&generated, chunk_w, target_h)
            };
            // The model repaints the kept strip a little paler each time.
            let (cur_strip, gen_strip) = if right_side {
                (cur.width() - overlap..cur.width(), 0..overlap)
            } else {
                (0..overlap, chunk_w - overlap..chunk_w)
            };
            match_colors(&mut generated, gen_strip, &cur, cur_strip);
            let take = new_px.min(need);
            if !right_side {
                left_added += take;
            }
            cur = splice_chunk(&cur, &generated, overlap, take, right_side);
        }
        if cur.width() == before {
            break;
        }
    }
    even_out_extensions(&mut cur, left_added, left_added + plate_w);
    if cur.width() != target_w {
        cur = cimg::resize_to(&cur, target_w, target_h);
    }
    Ok(cur)
}

/// Outpainted chunks drift in tone and the drift adds up toward the ends; only the low-frequency tone is levelled.
fn even_out_extensions(img: &mut RgbaImage, plate_x0: u32, plate_x1: u32) {
    let (w, h) = img.dimensions();
    if plate_x0 == 0 && plate_x1 >= w {
        return;
    }
    const GRID: u32 = 8;
    let small = cimg::resize_to(img, (w / GRID).max(1), (h / GRID).max(1));
    let (sw, sh) = (small.width() as usize, small.height() as usize);
    let q0 = (plate_x0 / GRID) as usize;
    let q1 = ((plate_x1 / GRID) as usize).min(sw);
    if q1 <= q0 {
        return;
    }
    let edge = ((q1 - q0) / 6).max(1);
    let px = |x: usize, y: usize| small.get_pixel(x as u32, y as u32).0;
    // Row-band sums so a window mean is two subtractions: prefix[y][x] over columns, rows y-2..=y+2 pooled.
    let band = |y: usize| y.saturating_sub(2)..(y + 3).min(sh);
    let mut prefix = vec![[0.0f64; 3]; sh * (sw + 1)];
    for y in 0..sh {
        for x in 0..sw {
            let mut acc = prefix[y * (sw + 1) + x];
            for yy in band(y) {
                let p = px(x, yy);
                for c in 0..3 {
                    acc[c] += f64::from(p[c]);
                }
            }
            prefix[y * (sw + 1) + x + 1] = acc;
        }
    }
    let mean = |y: usize, a: usize, b: usize| {
        let (lo, hi) = (prefix[y * (sw + 1) + a], prefix[y * (sw + 1) + b]);
        let n = ((b - a) * band(y).len()).max(1) as f64;
        [0, 1, 2].map(|c| (hi[c] - lo[c]) / n)
    };
    let win = sh;
    let mut corr = vec![[0.0f64; 3]; sw * sh];
    for y in 0..sh {
        let left_target = mean(y, q0, (q0 + edge).min(q1));
        let right_target = mean(y, q1.saturating_sub(edge).max(q0), q1);
        for x in (0..q0).chain(q1..sw) {
            let (target, fade) = if x < q0 {
                (left_target, (q0 - x) as f64 / edge as f64)
            } else {
                (right_target, (x + 1 - q1) as f64 / edge as f64)
            };
            let m = mean(y, x.saturating_sub(win), (x + win + 1).min(sw));
            let k = fade.min(1.0);
            corr[y * sw + x] = [0, 1, 2].map(|c| (target[c] - m[c]) * k);
        }
    }
    let at = |x: f64, y: f64| {
        let (x0, y0) = (x.floor().max(0.0) as usize, y.floor().max(0.0) as usize);
        let (x0, y0) = (x0.min(sw - 1), y0.min(sh - 1));
        let (x1, y1) = ((x0 + 1).min(sw - 1), (y0 + 1).min(sh - 1));
        let (fx, fy) = ((x - x0 as f64).clamp(0.0, 1.0), (y - y0 as f64).clamp(0.0, 1.0));
        let g = |xx: usize, yy: usize| corr[yy * sw + xx];
        [0, 1, 2].map(|c| {
            let top = g(x0, y0)[c] * (1.0 - fx) + g(x1, y0)[c] * fx;
            let bottom = g(x0, y1)[c] * (1.0 - fx) + g(x1, y1)[c] * fx;
            top * (1.0 - fy) + bottom * fy
        })
    };
    for y in 0..h {
        for x in (0..plate_x0.min(w)).chain(plate_x1.min(w)..w) {
            let d = at(
                f64::from(x) / f64::from(GRID) - 0.5,
                f64::from(y) / f64::from(GRID) - 0.5,
            );
            let p = img.get_pixel_mut(x, y);
            for (v, dv) in p.0.iter_mut().zip(d) {
                *v = (f64::from(*v) + dv).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    async fn painted(canvas: RgbaImage, _mask: RgbaImage, _plate: RgbaImage) -> anyhow::Result<RgbaImage> {
        Ok(RgbaImage::from_pixel(
            canvas.width(),
            canvas.height(),
            image::Rgba([200, 30, 30, 255]),
        ))
    }

    /// HOIST_EVEN_IN=<plate+extensions png> HOIST_EVEN_PLATE=<plate png> HOIST_EVEN_OUT=<png>: levels a saved background.
    #[test]
    #[ignore = "reads HOIST_EVEN_IN"]
    fn even_out_a_saved_background() {
        let var = |k: &str| std::env::var(k).expect(k);
        let mut img = cimg::load(std::path::Path::new(&var("HOIST_EVEN_IN"))).expect("in");
        let plate = cimg::load(std::path::Path::new(&var("HOIST_EVEN_PLATE"))).expect("plate");
        let plate_w =
            (f64::from(plate.width()) * f64::from(img.height()) / f64::from(plate.height())).round() as u32;
        let p0 = (img.width() - plate_w) / 2;
        even_out_extensions(&mut img, p0, p0 + plate_w);
        cimg::save_png(&img, std::path::Path::new(&var("HOIST_EVEN_OUT"))).expect("out");
    }

    #[test]
    fn letterbox_bars_are_trimmed() {
        let boxed = RgbaImage::from_fn(300, 100, |_, y| {
            if (20..80).contains(&y) {
                image::Rgba([120, 180, 230, 255])
            } else {
                image::Rgba([0, 0, 0, 255])
            }
        });
        let t = trim_bars(boxed);
        assert_eq!(t.height(), 56, "bars and a 2 px edge gone");
        assert!(t.pixels().all(|p| p.0[2] == 230));
        let plain = RgbaImage::from_pixel(300, 100, image::Rgba([0, 0, 0, 255]));
        assert_eq!(
            trim_bars(plain).dimensions(),
            (300, 100),
            "an all-dark picture is left alone"
        );
    }

    #[test]
    fn extensions_that_drift_darker_come_back_to_the_plate() {
        let mut img = RgbaImage::from_fn(2400, 400, |x, _| {
            let v = if (800..1600).contains(&x) {
                200.0
            } else {
                let d = if x < 800 { 800 - x } else { x + 1 - 1600 } as f64;
                200.0 - 120.0 * d / 800.0
            };
            image::Rgba([v as u8, v as u8, v as u8, 255])
        });
        even_out_extensions(&mut img, 800, 1600);
        for x in [0, 200, 2200, 2399] {
            let v = img.get_pixel(x, 200).0[0];
            assert!(v >= 170, "column {x} still dark: {v}");
        }
        assert_eq!(img.get_pixel(1200, 200).0[0], 200, "the plate is untouched");
    }

    #[test]
    fn the_splice_runs_where_both_pictures_agree() {
        let cur = RgbaImage::from_fn(60, 20, |x, _| {
            image::Rgba([if (30..34).contains(&x) { 90 } else { 10 }, 0, 0, 255])
        });
        let generated = RgbaImage::from_fn(60, 20, |_, _| image::Rgba([90, 0, 0, 255]));
        let path = splice_path(&cur, &generated, 60, &|i| i, &|i| i);
        assert!(path.iter().all(|x| (30..34).contains(x)), "{path:?}");
    }

    #[test]
    fn a_paler_chunk_takes_the_colors_it_continues() {
        let reference = RgbaImage::from_fn(40, 10, |x, _| image::Rgba([100 + x as u8, 150, 60, 255]));
        let mut pale = RgbaImage::from_fn(40, 10, |x, _| image::Rgba([200 + x as u8, 230, 180, 255]));
        match_colors(&mut pale, 0..20, &reference, 20..40);
        let p = pale.get_pixel(10, 5).0;
        let r = reference.get_pixel(30, 5).0;
        assert!(
            (i32::from(p[1]) - i32::from(r[1])).abs() <= 2,
            "green {p:?} vs {r:?}"
        );
        assert!(
            (i32::from(p[2]) - i32::from(r[2])).abs() <= 2,
            "blue {p:?} vs {r:?}"
        );
        assert!(p[0] < 140, "red pulled down from 210: {p:?}");
    }

    #[tokio::test]
    async fn outpaint_ends_for_every_remaining_width() {
        let stop = AtomicBool::new(false);
        for (start_w, h, extra) in [
            (100, 64, 1),
            (100, 64, 7),
            (100, 64, 15),
            (100, 64, 16),
            (100, 64, 17),
            (100, 64, 31),
            (100, 64, 1000),
            (100, 60, 1),
            (100, 60, 9),
            (100, 60, 500),
            (40, 64, 300),
        ] {
            let calls = Cell::new(0);
            let start = RgbaImage::from_pixel(start_w, h, image::Rgba([10, 120, 200, 255]));
            let out = outpaint(start, start_w + extra, &stop, |c, m, p| {
                calls.set(calls.get() + 1);
                painted(c, m, p)
            })
            .await
            .expect("outpaint");
            assert_eq!(
                out.dimensions(),
                (start_w + extra, h),
                "start {start_w} h {h} +{extra}"
            );
            assert!(
                calls.get() <= MAX_EXTEND_CALLS,
                "start {start_w} h {h} +{extra}: {} calls",
                calls.get()
            );
            if extra < EXTEND_MIN_STEP {
                assert_eq!(calls.get(), 0, "a gap under 16 px is closed without the API");
            }
        }
    }

    #[tokio::test]
    async fn outpaint_stops_when_asked() {
        let stop = AtomicBool::new(true);
        let start = RgbaImage::from_pixel(100, 64, image::Rgba([0, 0, 0, 255]));
        let err = outpaint(start, 1000, &stop, painted).await.expect_err("stopped");
        assert!(err.to_string().contains("중단"), "{err}");
    }
}
