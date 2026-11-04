use std::cmp::Reverse;
use std::collections::HashMap;

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
    let (cents, labels) = kmeans(img, max_colors);
    let raw = img.as_raw();
    let mut counts = vec![0u64; cents.len()];
    let mut total = 0u64;
    for (i, &l) in labels.iter().enumerate() {
        if raw[i * 4 + 3] >= 128 {
            counts[usize::from(l)] += 1;
            total += 1;
        }
    }
    let mut colors: Vec<(String, f64)> = cents
        .iter()
        .zip(&counts)
        .filter(|(_, n)| **n > 0)
        .map(|(c, n)| (hex(*c), *n as f64 / total as f64))
        .collect();
    colors.sort_by(|a, b| b.1.total_cmp(&a.1));
    Palette { colors }
}

pub fn segment_regions(img: &RgbaImage) -> Vec<Region> {
    let (w, h) = img.dimensions();
    let (cents, labels) = kmeans(img, 8);
    if cents.is_empty() {
        return Vec::new();
    }
    let raw = img.as_raw();
    let opaque = |i: usize| raw[i * 4 + 3] >= 128;
    let (wu, n) = (w as usize, labels.len());
    let mut parent: Vec<usize> = (0..n).collect();
    for i in 0..n {
        if !opaque(i) {
            continue;
        }
        if i % wu > 0 && opaque(i - 1) && labels[i - 1] == labels[i] {
            join(&mut parent, i, i - 1);
        }
        if i >= wu && opaque(i - wu) && labels[i - wu] == labels[i] {
            join(&mut parent, i, i - wu);
        }
    }
    let mut by_root: HashMap<usize, Comp> = HashMap::new();
    for (i, &label) in labels.iter().enumerate() {
        if !opaque(i) {
            continue;
        }
        let (x, y) = ((i % wu) as u32, (i / wu) as u32);
        let borders =
            u8::from(x == 0) | u8::from(x == w - 1) << 1 | u8::from(y == 0) << 2 | u8::from(y == h - 1) << 3;
        let c = by_root.entry(find(&mut parent, i)).or_insert(Comp {
            label,
            x0: x,
            y0: y,
            x1: x,
            y1: y,
            area: 0,
            borders: 0,
        });
        (c.x0, c.y0, c.x1, c.y1) = (c.x0.min(x), c.y0.min(y), c.x1.max(x), c.y1.max(y));
        c.area += 1;
        c.borders |= borders;
    }
    let min_area = (n as f64 * 0.0005).ceil().max(1.0) as u32;
    let mut comps: Vec<Comp> = by_root.into_values().filter(|c| c.area >= min_area).collect();
    comps.sort_by_key(|c| (Reverse(c.area), c.y0, c.x0));
    classify(&comps, &cents, w, h)
}

pub fn quantize(img: &RgbaImage, colors: usize) -> (RgbaImage, Vec<String>) {
    let (cents, labels) = kmeans(img, colors);
    if cents.is_empty() {
        return (img.clone(), Vec::new());
    }
    let mut out = img.clone();
    for (p, &l) in out.pixels_mut().zip(&labels) {
        let c = cents[usize::from(l)];
        p.0 = [c[0].round() as u8, c[1].round() as u8, c[2].round() as u8, p.0[3]];
    }
    (out, cents.iter().map(|c| hex(*c)).collect())
}

const MAX_SAMPLES: usize = 20_000;

fn hex(c: [f32; 3]) -> String {
    format!(
        "#{:02X}{:02X}{:02X}",
        c[0].round() as u8,
        c[1].round() as u8,
        c[2].round() as u8
    )
}

fn dist2(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] - b[i]) * (a[i] - b[i])).sum()
}

fn nearest(cents: &[[f32; 3]], p: &[f32; 3]) -> usize {
    let mut best = (0, f32::MAX);
    for (i, c) in cents.iter().enumerate() {
        let d = dist2(c, p);
        if d < best.1 {
            best = (i, d);
        }
    }
    best.0
}

fn kmeans(img: &RgbaImage, k: usize) -> (Vec<[f32; 3]>, Vec<u8>) {
    let raw = img.as_raw();
    let n = raw.len() / 4;
    let rgb = |i: usize| {
        [
            f32::from(raw[i * 4]),
            f32::from(raw[i * 4 + 1]),
            f32::from(raw[i * 4 + 2]),
        ]
    };
    let step = (n / MAX_SAMPLES).max(1);
    let samples: Vec<[f32; 3]> = (0..n)
        .step_by(step)
        .filter(|&i| raw[i * 4 + 3] >= 128)
        .map(rgb)
        .collect();
    if samples.is_empty() {
        return (Vec::new(), vec![0; n]);
    }
    let k = k.clamp(1, 16);
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    let mut cents = vec![samples[samples.len() / 2]];
    let mut d2: Vec<f32> = samples.iter().map(|s| dist2(s, &cents[0])).collect();
    while cents.len() < k {
        let total: f64 = d2.iter().map(|&v| f64::from(v)).sum();
        if total <= 0.0 {
            break;
        }
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let mut target = (seed >> 11) as f64 / (1u64 << 53) as f64 * total;
        let mut pick = samples.len() - 1;
        for (i, &v) in d2.iter().enumerate() {
            target -= f64::from(v);
            if target <= 0.0 && v > 0.0 {
                pick = i;
                break;
            }
        }
        let c = samples[pick];
        for (d, s) in d2.iter_mut().zip(&samples) {
            *d = d.min(dist2(s, &c));
        }
        cents.push(c);
    }
    for _ in 0..10 {
        let mut sums = vec![[0f64; 4]; cents.len()];
        for s in &samples {
            let c = nearest(&cents, s);
            for j in 0..3 {
                sums[c][j] += f64::from(s[j]);
            }
            sums[c][3] += 1.0;
        }
        let mut moved = false;
        for (c, s) in cents.iter_mut().zip(&sums) {
            if s[3] > 0.0 {
                let next = [(s[0] / s[3]) as f32, (s[1] / s[3]) as f32, (s[2] / s[3]) as f32];
                moved |= dist2(c, &next) > 0.25;
                *c = next;
            }
        }
        if !moved {
            break;
        }
    }
    let labels = (0..n).map(|i| nearest(&cents, &rgb(i)) as u8).collect();
    (cents, labels)
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

fn join(parent: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        parent[ra.max(rb)] = ra.min(rb);
    }
}

struct Comp {
    label: u8,
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
    area: u32,
    borders: u8,
}

impl Comp {
    fn w(&self) -> u32 {
        self.x1 - self.x0 + 1
    }

    fn h(&self) -> u32 {
        self.y1 - self.y0 + 1
    }

    fn lines_up_with(&self, o: &Comp) -> bool {
        let (ha, hb) = (f64::from(self.h()), f64::from(o.h()));
        let mh = ha.max(hb);
        let gap = f64::from(self.x0.max(o.x0)) - f64::from(self.x1.min(o.x1)) - 1.0;
        self.label == o.label
            && mh / ha.min(hb) <= 2.0
            && (f64::from(self.y1) - f64::from(o.y1)).abs() <= 0.2 * mh
            && gap < 1.5 * mh
    }
}

fn classify(comps: &[Comp], cents: &[[f32; 3]], w: u32, h: u32) -> Vec<Region> {
    let bg = comps.iter().position(|c| c.borders.count_ones() >= 3);
    let small: Vec<usize> = (0..comps.len())
        .filter(|&i| Some(i) != bg && comps[i].w() <= w / 4 && comps[i].h() <= h / 4)
        .collect();
    let mut uf: Vec<usize> = (0..comps.len()).collect();
    for (k, &a) in small.iter().enumerate() {
        for &b in &small[k + 1..] {
            if comps[a].lines_up_with(&comps[b]) {
                join(&mut uf, a, b);
            }
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for &i in &small {
        groups.entry(find(&mut uf, i)).or_default().push(i);
    }
    let mut groups: Vec<Vec<usize>> = groups.into_values().filter(|g| g.len() >= 2).collect();
    groups.sort();
    let mut used = vec![false; comps.len()];
    for g in &groups {
        for &i in g {
            used[i] = true;
        }
    }
    for g in &mut groups {
        let label = comps[g[0]].label;
        let (x0, y0, x1, y1) = g.iter().fold((u32::MAX, u32::MAX, 0, 0), |b, &i| {
            let c = &comps[i];
            (b.0.min(c.x0), b.1.min(c.y0), b.2.max(c.x1), b.3.max(c.y1))
        });
        let m = f64::from(y1 - y0 + 1) * 0.2;
        for &i in &small {
            let c = &comps[i];
            let inside = f64::from(c.x0) >= f64::from(x0) - m
                && f64::from(c.y0) >= f64::from(y0) - m
                && f64::from(c.x1) <= f64::from(x1) + m
                && f64::from(c.y1) <= f64::from(y1) + m;
            if !used[i] && c.label == label && inside {
                used[i] = true;
                g.push(i);
            }
        }
    }
    let region = |c: &Comp, kind: RegionKind| Region {
        bbox_px: [c.x0, c.y0, c.w(), c.h()],
        color: hex(cents[usize::from(c.label)]),
        area_px: c.area,
        kind_hint: kind,
    };
    let mut regions = Vec::new();
    if let Some(b) = bg {
        used[b] = true;
        regions.push(region(&comps[b], RegionKind::Background));
    }
    for g in &groups {
        let first = &comps[g[0]];
        let mut merged = Comp {
            label: first.label,
            x0: first.x0,
            y0: first.y0,
            x1: first.x1,
            y1: first.y1,
            area: 0,
            borders: 0,
        };
        for &i in g {
            let c = &comps[i];
            (merged.x0, merged.y0) = (merged.x0.min(c.x0), merged.y0.min(c.y0));
            (merged.x1, merged.y1) = (merged.x1.max(c.x1), merged.y1.max(c.y1));
            merged.area += c.area;
        }
        regions.push(region(&merged, RegionKind::TextLike));
    }
    for (i, c) in comps.iter().enumerate() {
        if !used[i] {
            let fill = f64::from(c.area) / (f64::from(c.w()) * f64::from(c.h()));
            regions.push(region(
                c,
                if fill >= 0.6 {
                    RegionKind::Shape
                } else {
                    RegionKind::Photo
                },
            ));
        }
    }
    regions.sort_by_key(|r| Reverse(r.area_px));
    regions
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn synthetic() -> RgbaImage {
        let mut img = RgbaImage::from_pixel(200, 100, Rgba([255, 255, 255, 255]));
        for y in 30..70 {
            for x in 100..160 {
                img.put_pixel(x, y, Rgba([20, 60, 200, 255]));
            }
        }
        for x0 in [10u32, 22, 34, 46] {
            for y in 10..20 {
                for x in x0..x0 + 8 {
                    img.put_pixel(x, y, Rgba([0, 0, 0, 255]));
                }
            }
        }
        img
    }

    #[test]
    fn segments_background_shape_and_text() {
        let regions = segment_regions(&synthetic());
        let bg = regions
            .iter()
            .find(|r| r.kind_hint == RegionKind::Background)
            .expect("background");
        assert_eq!(bg.color, "#FFFFFF");
        let shape = regions
            .iter()
            .find(|r| r.kind_hint == RegionKind::Shape)
            .expect("shape");
        assert_eq!(shape.bbox_px, [100, 30, 60, 40]);
        assert_eq!(shape.color, "#143CC8");
        let text = regions
            .iter()
            .find(|r| r.kind_hint == RegionKind::TextLike)
            .expect("text");
        assert_eq!(text.bbox_px, [10, 10, 44, 10]);
        assert_eq!(text.area_px, 320);
        assert_eq!(regions.len(), 3);
    }

    #[test]
    fn palette_and_quantize() {
        let img = synthetic();
        let p = palette(&img, 8);
        assert_eq!(p.colors.len(), 3);
        assert_eq!(p.colors[0].0, "#FFFFFF");
        let sum: f64 = p.colors.iter().map(|c| c.1).sum();
        assert!((sum - 1.0).abs() < 1e-9);
        let (q, colors) = quantize(&img, 2);
        assert_eq!(colors.len(), 2);
        assert!(q.pixels().all(|px| colors.contains(&hex([
            f32::from(px[0]),
            f32::from(px[1]),
            f32::from(px[2])
        ]))));
    }
}
