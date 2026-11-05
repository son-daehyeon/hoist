use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use cosmic_text::fontdb::{self, Source};
use cosmic_text::{Align, Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight, Wrap};
use serde::{Deserialize, Serialize};
use ttf_parser::{GlyphId, OutlineBuilder, Tag};

use crate::Result;
use crate::model::{TextAlign, TextProps};
use crate::render::num;

pub struct Fonts {
    inner: std::sync::Mutex<cosmic_text::FontSystem>,
    catalog: HashMap<String, CatalogEntry>,
}

struct CatalogEntry {
    display_name: String,
    category: String,
    weight: Option<u16>,
}

#[derive(Deserialize)]
struct ManifestEntry {
    display_name: String,
    category: String,
    #[serde(default)]
    weights: Vec<String>,
    #[serde(default)]
    files: Vec<String>,
}

pub const FONT_CATEGORIES: [&str; 5] = ["고딕", "명조", "둥근", "손글씨", "디스플레이"];

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FontFace {
    pub family: String,
    pub display_name: Option<String>,
    pub category: Option<String>,
    pub weights: Vec<u16>,
    pub style: FontStyle,
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FontFamily {
    pub family: String,
    pub display_name: Option<String>,
    pub category: Option<String>,
    pub weights: Vec<u16>,
    pub faces: Vec<FontFace>,
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
        let mut catalog = HashMap::new();
        if let Some(dir) = bundled_dir {
            fs.db_mut().load_fonts_dir(dir);
            catalog = read_manifest(dir);
            fix_weights(fs.db_mut(), &catalog);
        }
        Fonts {
            inner: Mutex::new(fs),
            catalog,
        }
    }

    /// Families sorted by manifest category, then by display name; fonts outside the manifest come last.
    pub fn catalog(&self) -> Vec<FontFamily> {
        let mut by_family: BTreeMap<String, FontFamily> = BTreeMap::new();
        for face in self.faces() {
            let f = by_family
                .entry(face.family.clone())
                .or_insert_with(|| FontFamily {
                    family: face.family.clone(),
                    display_name: None,
                    category: None,
                    weights: Vec::new(),
                    faces: Vec::new(),
                });
            f.display_name = f.display_name.take().or_else(|| face.display_name.clone());
            f.category = f.category.take().or_else(|| face.category.clone());
            f.weights.extend(&face.weights);
            f.faces.push(face);
        }
        let rank = |c: &Option<String>| {
            c.as_deref()
                .and_then(|c| FONT_CATEGORIES.iter().position(|k| *k == c))
                .unwrap_or(FONT_CATEGORIES.len())
        };
        let mut out: Vec<FontFamily> = by_family
            .into_values()
            .map(|mut f| {
                f.weights.sort_unstable();
                f.weights.dedup();
                f.faces.sort_by_key(|x| x.weights.first().copied());
                f
            })
            .collect();
        out.sort_by(|a, b| {
            rank(&a.category).cmp(&rank(&b.category)).then_with(|| {
                a.display_name
                    .as_ref()
                    .unwrap_or(&a.family)
                    .cmp(b.display_name.as_ref().unwrap_or(&b.family))
            })
        });
        out
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

    /// Variable fonts list every hundred weight their `wght` axis covers.
    pub fn faces(&self) -> Vec<FontFace> {
        let fs = self.lock();
        let db = fs.db();
        db.faces()
            .filter_map(|f| {
                let path = source_path(&f.source)?.to_path_buf();
                let entry = file_name(&path).and_then(|n| self.catalog.get(n));
                let style = match f.style {
                    fontdb::Style::Normal => FontStyle::Normal,
                    fontdb::Style::Italic => FontStyle::Italic,
                    fontdb::Style::Oblique => FontStyle::Oblique,
                };
                Some(FontFace {
                    family: f.families.first()?.0.clone(),
                    display_name: entry.map(|e| e.display_name.clone()),
                    category: entry.map(|e| e.category.clone()),
                    weights: db
                        .with_face_data(f.id, variable_weights)
                        .flatten()
                        .unwrap_or_else(|| vec![f.weight.0]),
                    style,
                    path,
                })
            })
            .collect()
    }

    pub fn has_family(&self, family: &str) -> bool {
        find_family(&self.lock(), family).is_some()
    }

    pub fn layout_to_path(&self, props: &TextProps, box_w_mm: f64, box_h_mm: f64) -> Result<GlyphRun> {
        let mut fs = self.lock();
        let look = Look::of(props);
        let mut out = Outline::default();
        if props.vertical {
            layout_vertical(&mut fs, props, box_w_mm, box_h_mm, &look, &mut out);
        } else {
            layout_horizontal(&mut fs, props, box_w_mm, &look, &mut out);
        }
        Ok(out.finish())
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

struct Look {
    scale_x: f64,
    shear: f64,
}

impl Look {
    fn of(props: &TextProps) -> Self {
        let scale_x = if props.scale_x.is_finite() {
            props.scale_x.clamp(0.1, 4.0)
        } else {
            1.0
        };
        let skew = if props.skew_deg.is_finite() {
            props.skew_deg.clamp(-60.0, 60.0)
        } else {
            0.0
        };
        Look {
            scale_x,
            shear: skew.to_radians().tan(),
        }
    }
}

#[derive(Default)]
struct Outline {
    d: String,
    ink: Option<[f64; 4]>,
}

impl Outline {
    fn finish(self) -> GlyphRun {
        GlyphRun {
            path_d: self.d,
            bbox_mm: self
                .ink
                .map_or([0.0; 4], |i| [i[0], i[1], i[2] - i[0], i[3] - i[1]]),
        }
    }
}

#[derive(Clone, Copy)]
struct Place {
    ox: f64,
    oy: f64,
    turn: Option<(f64, f64, f64, f64, f64)>,
}

struct ArcMap {
    center_x: f64,
    radius: f64,
    half_angle: f64,
    half_chord: f64,
    sagitta: f64,
    sign: f64,
}

impl ArcMap {
    fn new(bend: f64, chord: f64) -> Option<Self> {
        let bend = if bend.is_finite() {
            bend.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        if bend.abs() < 1e-3 || chord.is_nan() || chord <= 0.0 {
            return None;
        }
        let half_angle = bend.abs() * std::f64::consts::FRAC_PI_2;
        let radius = (chord / 2.0) / half_angle.sin();
        Some(ArcMap {
            center_x: chord / 2.0,
            radius,
            half_angle,
            half_chord: chord / 2.0,
            sagitta: radius * (1.0 - half_angle.cos()),
            sign: bend.signum(),
        })
    }

    fn turn(&self, glyph_center_x: f64, baseline: f64) -> (f64, f64, f64, f64, f64) {
        let phi = (glyph_center_x - self.center_x) / self.half_chord * self.half_angle;
        let x = self.center_x + self.radius * phi.sin();
        let y = baseline - self.sign * self.sagitta + self.sign * self.radius * (1.0 - phi.cos());
        (glyph_center_x, baseline, x, y, self.sign * phi)
    }
}

fn layout_horizontal(fs: &mut FontSystem, props: &TextProps, box_w_mm: f64, look: &Look, out: &mut Outline) {
    let Some(buf) = shape(fs, props, box_w_mm / look.scale_x) else {
        return;
    };
    let arc = ArcMap::new(props.arc_bend, box_w_mm);
    let glyphs: Vec<(cosmic_text::LayoutGlyph, f32)> = buf
        .layout_runs()
        .flat_map(|line| line.glyphs.iter().map(move |g| (g.clone(), line.line_y)))
        .collect();
    for (g, line_y) in glyphs {
        let ox = f64::from(g.x + g.font_size * g.x_offset) * look.scale_x;
        let oy = f64::from(line_y + g.y - g.font_size * g.y_offset);
        let turn = arc
            .as_ref()
            .map(|a| a.turn(f64::from(g.x + g.w / 2.0) * look.scale_x, f64::from(line_y)));
        draw_glyph(fs, &g, Place { ox, oy, turn }, look, out);
    }
}

fn layout_vertical(
    fs: &mut FontSystem,
    props: &TextProps,
    box_w_mm: f64,
    box_h_mm: f64,
    look: &Look,
    out: &mut Outline,
) {
    let size = props.font_size_mm;
    let pitch = size * props.line_height.max(0.1);
    let step = pitch + props.letter_spacing_mm;
    for (col, text) in props.text.split('\n').enumerate() {
        let line_props = TextProps {
            text: text.to_string(),
            letter_spacing_mm: 0.0,
            ..props.clone()
        };
        let Some(buf) = shape(fs, &line_props, f64::NAN) else {
            continue;
        };
        let Some(run) = buf.layout_runs().next() else {
            continue;
        };
        let ascent = f64::from(run.line_y - run.line_top);
        let glyphs: Vec<cosmic_text::LayoutGlyph> = run.glyphs.to_vec();
        let height = step * glyphs.len() as f64 - props.letter_spacing_mm;
        let top = match props.align {
            TextAlign::Left => 0.0,
            TextAlign::Center => (box_h_mm - height) / 2.0,
            TextAlign::Right => box_h_mm - height,
        };
        let center_x = box_w_mm - (col as f64 + 0.5) * pitch;
        for (k, g) in glyphs.iter().enumerate() {
            let ox = center_x - f64::from(g.w) * look.scale_x / 2.0;
            let oy = top + k as f64 * step + ascent;
            draw_glyph(fs, g, Place { ox, oy, turn: None }, look, out);
        }
    }
}

fn draw_glyph(
    fs: &mut FontSystem,
    g: &cosmic_text::LayoutGlyph,
    place: Place,
    look: &Look,
    out: &mut Outline,
) {
    let Some(font) = fs.get_font(g.font_id, g.font_weight) else {
        return;
    };
    let index = fs.db().face(g.font_id).map_or(0, |f| f.index);
    let Ok(mut face) = ttf_parser::Face::parse(font.data(), index) else {
        return;
    };
    let _ = face.set_variation(Tag::from_bytes(b"wght"), f32::from(g.font_weight.0));
    let mut sink = PathSink {
        d: &mut out.d,
        place,
        s: f64::from(g.font_size) / f64::from(face.units_per_em().max(1)),
        scale_x: look.scale_x,
        shear: look.shear,
    };
    if let Some(r) = face.outline_glyph(GlyphId(g.glyph_id), &mut sink) {
        for (x, y) in [
            (r.x_min, r.y_min),
            (r.x_min, r.y_max),
            (r.x_max, r.y_min),
            (r.x_max, r.y_max),
        ] {
            let (px, py) = sink.pt(f32::from(x), f32::from(y));
            let i = out.ink.get_or_insert([px, py, px, py]);
            *i = [i[0].min(px), i[1].min(py), i[2].max(px), i[3].max(py)];
        }
    }
}

fn source_path(source: &Source) -> Option<&Path> {
    match source {
        Source::File(p) | Source::SharedFile(p, _) => Some(p),
        Source::Binary(_) => None,
    }
}

fn file_name(path: &Path) -> Option<&str> {
    path.file_name().and_then(|n| n.to_str())
}

fn weight_named(name: &str) -> Option<u16> {
    let key: String = name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase();
    Some(match key.as_str() {
        "thin" | "hairline" => 100,
        "extralight" | "ultralight" => 200,
        "light" => 300,
        "regular" | "normal" | "book" => 400,
        "medium" => 500,
        "semibold" | "demibold" => 600,
        "bold" => 700,
        "extrabold" | "ultrabold" => 800,
        "black" | "heavy" => 900,
        _ => return None,
    })
}

fn read_manifest(dir: &Path) -> HashMap<String, CatalogEntry> {
    let entries: Vec<ManifestEntry> = std::fs::read_to_string(dir.join("manifest.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let mut out = HashMap::new();
    for e in entries {
        let paired = e.weights.len() == e.files.len();
        for (i, file) in e.files.iter().enumerate() {
            let weight = paired
                .then(|| e.weights.get(i))
                .flatten()
                .and_then(|w| weight_named(w));
            out.insert(
                file.clone(),
                CatalogEntry {
                    display_name: e.display_name.clone(),
                    category: e.category.clone(),
                    weight,
                },
            );
        }
    }
    out
}

/// Some bundled fonts report usWeightClass 400 for every style (Suncheon R/B, MuseumClassic L/M/B), so the manifest's weight wins.
fn fix_weights(db: &mut fontdb::Database, catalog: &HashMap<String, CatalogEntry>) {
    let fixes: Vec<(fontdb::ID, u16)> = db
        .faces()
        .filter_map(|f| {
            let w = catalog.get(file_name(source_path(&f.source)?)?)?.weight?;
            (w != f.weight.0).then_some((f.id, w))
        })
        .collect();
    for (id, w) in fixes {
        if let Some(mut info) = db.face(id).cloned() {
            db.remove_face(id);
            info.weight = fontdb::Weight(w);
            db.push_face_info(info);
        }
    }
}

fn variable_weights(data: &[u8], index: u32) -> Option<Vec<u16>> {
    let raw = ttf_parser::RawFace::parse(data, index).ok()?;
    let fvar = ttf_parser::fvar::Table::parse(raw.table(Tag::from_bytes(b"fvar"))?)?;
    let axis = fvar
        .axes
        .into_iter()
        .find(|a| a.tag == Tag::from_bytes(b"wght"))?;
    let weights: Vec<u16> = (1..=9u16)
        .map(|i| i * 100)
        .filter(|w| (axis.min_value..=axis.max_value).contains(&f32::from(*w)))
        .collect();
    (!weights.is_empty()).then_some(weights)
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
    place: Place,
    s: f64,
    scale_x: f64,
    shear: f64,
}

impl PathSink<'_> {
    fn pt(&self, x: f32, y: f32) -> (f64, f64) {
        let (x, y) = (f64::from(x) * self.s, f64::from(y) * self.s);
        let flat = (
            self.place.ox + x * self.scale_x + y * self.shear,
            self.place.oy - y,
        );
        match self.place.turn {
            None => flat,
            Some((pivot_x, pivot_y, to_x, to_y, angle)) => {
                let (sin, cos) = angle.sin_cos();
                let (dx, dy) = (flat.0 - pivot_x, flat.1 - pivot_y);
                (to_x + dx * cos - dy * sin, to_y + dx * sin + dy * cos)
            }
        }
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

    #[test]
    fn manifest_names_sorts_and_fixes_weights() {
        let bundled = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/fonts");
        if !bundled.join("manifest.json").is_file() {
            return;
        }
        let fonts = Fonts::new(Some(&bundled));
        let faces = fonts.faces();
        let weights_of = |prefix: &str| {
            let mut w: Vec<u16> = faces
                .iter()
                .filter(|f| file_name(&f.path).is_some_and(|n| n.starts_with(prefix)))
                .flat_map(|f| f.weights.clone())
                .collect();
            w.sort_unstable();
            w.dedup();
            w
        };
        assert_eq!(weights_of("Suncheon-"), [400, 700]);
        assert_eq!(weights_of("MuseumClassic-"), [300, 500, 700]);

        let catalog = fonts.catalog();
        let pretendard = catalog
            .iter()
            .find(|f| f.family == "Pretendard")
            .expect("Pretendard");
        assert_eq!(pretendard.category.as_deref(), Some("고딕"));
        assert!(pretendard.display_name.is_some());
        let ranks: Vec<usize> = catalog
            .iter()
            .map(|f| {
                f.category
                    .as_deref()
                    .and_then(|c| FONT_CATEGORIES.iter().position(|k| *k == c))
                    .unwrap_or(FONT_CATEGORIES.len())
            })
            .collect();
        assert!(
            ranks.windows(2).all(|w| w[0] <= w[1]),
            "sorted by category {ranks:?}"
        );

        let family = faces
            .iter()
            .find(|f| file_name(&f.path) == Some("Suncheon-Bold.ttf"))
            .expect("Suncheon bold")
            .family
            .clone();
        let mut p = TextProps {
            text: "순천".into(),
            font_family: family,
            font_weight: 400,
            font_size_mm: 20.0,
            ..Default::default()
        };
        let regular = fonts.layout_to_path(&p, 100.0, 30.0).expect("regular").path_d;
        p.font_weight = 700;
        let bold = fonts.layout_to_path(&p, 100.0, 30.0).expect("bold").path_d;
        assert_ne!(regular, bold, "weight 700 picks the Bold file");
    }

    #[test]
    fn faces_list_files_and_expand_variable_weights() {
        let bundled = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/fonts");
        let faces = Fonts::new(Some(&bundled)).faces();
        let pretendard: Vec<&FontFace> = faces.iter().filter(|f| f.family == "Pretendard").collect();
        let mut weights: Vec<u16> = pretendard.iter().flat_map(|f| f.weights.clone()).collect();
        weights.sort_unstable();
        weights.dedup();
        assert!(
            [400, 500, 600, 700, 800, 900].iter().all(|w| weights.contains(w)),
            "{weights:?}"
        );
        assert!(pretendard.iter().any(|f| f.path.starts_with(&bundled)));
        assert!(pretendard.iter().all(|f| f.path.is_file()));
        if let Some(bahnschrift) = faces.iter().find(|f| f.family == "Bahnschrift") {
            assert!(bahnschrift.weights.len() > 1, "variable axis {bahnschrift:?}");
        }
    }

    fn props(text: &str, family: &str) -> TextProps {
        TextProps {
            text: text.into(),
            font_family: family.into(),
            font_weight: 700,
            font_size_mm: 10.0,
            letter_spacing_mm: 0.0,
            line_height: 1.2,
            align: TextAlign::Left,
            ..Default::default()
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

    fn bbox(p: &TextProps, w: f64, h: f64) -> [f64; 4] {
        fonts().layout_to_path(p, w, h).expect("layout").bbox_mm
    }

    #[test]
    fn scale_skew_vertical_and_arc_change_the_outline() {
        if fonts().families().is_empty() {
            return;
        }
        let base = props("가나다라마", "No Such Font 123");
        let plain = bbox(&base, 200.0, 20.0);

        let narrow = bbox(
            &TextProps {
                scale_x: 0.5,
                ..base.clone()
            },
            200.0,
            20.0,
        );
        assert!(
            (narrow[2] / plain[2] - 0.5).abs() < 0.08,
            "scale_x {narrow:?} vs {plain:?}"
        );
        assert!((narrow[3] - plain[3]).abs() < 0.2);

        let mut centered = base.clone();
        centered.align = TextAlign::Center;
        let mid = bbox(
            &TextProps {
                scale_x: 0.5,
                ..centered
            },
            200.0,
            20.0,
        );
        assert!(
            (mid[0] + mid[2] / 2.0 - 100.0).abs() < 3.0,
            "centered after scale {mid:?}"
        );

        let leaning = bbox(
            &TextProps {
                skew_deg: 20.0,
                ..base.clone()
            },
            200.0,
            20.0,
        );
        assert!(leaning[2] > plain[2] + 1.0, "skew widens {leaning:?}");

        let tall = bbox(
            &TextProps {
                vertical: true,
                ..base.clone()
            },
            20.0,
            100.0,
        );
        assert!(tall[3] > tall[2] * 3.0, "vertical {tall:?}");
        assert!(
            tall[0] > 0.0 && tall[0] + tall[2] <= 20.5,
            "one column inside the box {tall:?}"
        );

        let wide = props("가나다라마바사아자차", "No Such Font 123");
        let flat = bbox(&wide, 100.0, 20.0);
        let up = bbox(
            &TextProps {
                arc_bend: 0.6,
                ..wide.clone()
            },
            100.0,
            20.0,
        );
        let down = bbox(
            &TextProps {
                arc_bend: -0.6,
                ..wide.clone()
            },
            100.0,
            20.0,
        );
        assert!(up[3] > flat[3] + 5.0, "arch is taller {up:?} vs {flat:?}");
        assert!(up[1] < flat[1] - 5.0, "arch rises above the baseline {up:?}");
        assert!(
            down[1] + down[3] > flat[1] + flat[3] + 5.0,
            "valley sinks below {down:?}"
        );
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
