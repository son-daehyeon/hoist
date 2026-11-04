use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use cosmic_text::{Align, Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight, Wrap};
use ttf_parser::{GlyphId, OutlineBuilder, Tag};

use crate::Result;
use crate::model::{TextAlign, TextProps};
use crate::render::num;

pub struct Fonts {
    inner: std::sync::Mutex<cosmic_text::FontSystem>,
}

pub struct GlyphRun {
    pub path_d: String,
    pub bbox_mm: [f64; 4],
}

const KOREAN_FALLBACKS: [&str; 7] = [
    "Pretendard",
    "Malgun Gothic",
    "Noto Sans KR",
    "Noto Sans CJK KR",
    "Apple SD Gothic Neo",
    "NanumGothic",
    "Source Han Sans KR",
];

impl Fonts {
    pub fn new(bundled_dir: Option<&Path>) -> Self {
        let mut fs = FontSystem::new();
        if let Some(dir) = bundled_dir {
            fs.db_mut().load_fonts_dir(dir);
        }
        Fonts {
            inner: Mutex::new(fs),
        }
    }

    fn lock(&self) -> MutexGuard<'_, FontSystem> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn families(&self) -> Vec<String> {
        let fs = self.lock();
        let mut names: Vec<String> = fs
            .db()
            .faces()
            .flat_map(|f| f.families.iter().map(|(n, _)| n.clone()))
            .collect();
        names.sort();
        names.dedup();
        names
    }

    pub fn has_family(&self, family: &str) -> bool {
        find_family(&self.lock(), family).is_some()
    }

    pub fn layout_to_path(&self, props: &TextProps, box_w_mm: f64, _box_h_mm: f64) -> Result<GlyphRun> {
        let mut fs = self.lock();
        let mut run = GlyphRun {
            path_d: String::new(),
            bbox_mm: [0.0; 4],
        };
        let Some(buf) = shape(&mut fs, props, box_w_mm) else {
            return Ok(run);
        };
        let mut ink = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for line in buf.layout_runs() {
            for g in line.glyphs {
                let Some(font) = fs.get_font(g.font_id, g.font_weight) else {
                    continue;
                };
                let index = fs.db().face(g.font_id).map_or(0, |f| f.index);
                let Ok(mut face) = ttf_parser::Face::parse(font.data(), index) else {
                    continue;
                };
                let _ = face.set_variation(Tag::from_bytes(b"wght"), f32::from(g.font_weight.0));
                let mut sink = PathSink {
                    d: &mut run.path_d,
                    ox: f64::from(g.x + g.font_size * g.x_offset),
                    oy: f64::from(line.line_y + g.y - g.font_size * g.y_offset),
                    s: f64::from(g.font_size) / f64::from(face.units_per_em().max(1)),
                };
                if let Some(r) = face.outline_glyph(GlyphId(g.glyph_id), &mut sink) {
                    let (x0, y0) = sink.pt(f32::from(r.x_min), f32::from(r.y_max));
                    let (x1, y1) = sink.pt(f32::from(r.x_max), f32::from(r.y_min));
                    ink = [ink[0].min(x0), ink[1].min(y0), ink[2].max(x1), ink[3].max(y1)];
                }
            }
        }
        if ink[2] >= ink[0] {
            run.bbox_mm = [ink[0], ink[1], ink[2] - ink[0], ink[3] - ink[1]];
        }
        Ok(run)
    }

    pub fn measure(&self, props: &TextProps, box_w_mm: f64) -> Result<[f64; 2]> {
        let mut fs = self.lock();
        let Some(buf) = shape(&mut fs, props, box_w_mm) else {
            return Ok([0.0, 0.0]);
        };
        let (mut w, mut h) = (0f32, 0f32);
        for line in buf.layout_runs() {
            w = w.max(line.line_w);
            h = h.max(line.line_top + line.line_height);
        }
        Ok([f64::from(w), f64::from(h)])
    }
}

fn find_family(fs: &FontSystem, family: &str) -> Option<String> {
    fs.db()
        .faces()
        .flat_map(|f| f.families.iter())
        .find(|(n, _)| n.eq_ignore_ascii_case(family))
        .map(|(n, _)| n.clone())
}

fn shape(fs: &mut FontSystem, props: &TextProps, box_w_mm: f64) -> Option<Buffer> {
    let size = props.font_size_mm as f32;
    if props.text.is_empty() || !size.is_finite() || size <= 0.0 {
        return None;
    }
    let family = find_family(fs, &props.font_family)
        .or_else(|| KOREAN_FALLBACKS.iter().find_map(|f| find_family(fs, f)));
    let mut attrs = Attrs::new()
        .weight(Weight(props.font_weight))
        .letter_spacing(props.letter_spacing_mm as f32 / size);
    if let Some(name) = &family {
        attrs = attrs.family(Family::Name(name));
    }
    let align = match props.align {
        TextAlign::Left => Align::Left,
        TextAlign::Center => Align::Center,
        TextAlign::Right => Align::Right,
    };
    let line_h = size * (props.line_height as f32).max(0.1);
    let mut buf = Buffer::new(fs, Metrics::new(size, line_h));
    buf.set_wrap(Wrap::WordOrGlyph);
    let width = box_w_mm as f32;
    buf.set_size((width.is_finite() && width > 0.0).then_some(width), None);
    buf.set_text(&props.text, &attrs, Shaping::Advanced, Some(align));
    buf.shape_until_scroll(fs, false);
    Some(buf)
}

struct PathSink<'a> {
    d: &'a mut String,
    ox: f64,
    oy: f64,
    s: f64,
}

impl PathSink<'_> {
    fn pt(&self, x: f32, y: f32) -> (f64, f64) {
        (self.ox + f64::from(x) * self.s, self.oy - f64::from(y) * self.s)
    }

    fn cmd(&mut self, c: char, pts: &[(f32, f32)]) {
        self.d.push(c);
        for &(x, y) in pts {
            let (x, y) = self.pt(x, y);
            self.d.push_str(&num(x));
            self.d.push(' ');
            self.d.push_str(&num(y));
            self.d.push(' ');
        }
    }
}

impl OutlineBuilder for PathSink<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.cmd('M', &[(x, y)]);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.cmd('L', &[(x, y)]);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.cmd('Q', &[(x1, y1), (x, y)]);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.cmd('C', &[(x1, y1), (x2, y2), (x, y)]);
    }

    fn close(&mut self) {
        self.d.push('Z');
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::tests::fonts;

    fn props(text: &str, family: &str) -> TextProps {
        TextProps {
            text: text.into(),
            font_family: family.into(),
            font_weight: 700,
            font_size_mm: 10.0,
            letter_spacing_mm: 0.0,
            line_height: 1.2,
            align: TextAlign::Left,
        }
    }

    #[test]
    fn missing_family_falls_back_and_produces_glyphs() {
        let f = fonts();
        if f.families().is_empty() {
            return;
        }
        let run = f
            .layout_to_path(&props("가A", "No Such Font 123"), 100.0, 20.0)
            .expect("layout");
        assert!(run.path_d.contains('M'));
        assert!(run.bbox_mm[2] > 1.0 && run.bbox_mm[3] > 1.0);
        let [w, h] = f
            .measure(&props("가A", "No Such Font 123"), 100.0)
            .expect("measure");
        assert!(w > 5.0 && w < 30.0, "w={w}");
        assert!((h - 12.0).abs() < 0.5, "h={h}");
    }

    #[test]
    fn center_align_and_wrap() {
        let f = fonts();
        if f.families().is_empty() {
            return;
        }
        let mut p = props("A", "No Such Font 123");
        p.align = TextAlign::Center;
        let run = f.layout_to_path(&p, 100.0, 20.0).expect("layout");
        let cx = run.bbox_mm[0] + run.bbox_mm[2] / 2.0;
        assert!((cx - 50.0).abs() < 3.0, "cx={cx}");
        let [_, h] = f
            .measure(&props("AAAA AAAA AAAA", "No Such Font 123"), 30.0)
            .expect("measure");
        assert!(h > 20.0, "wrapped height {h}");
    }
}
