use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

use base64::Engine;
use image::RgbaImage;
use serde::{Deserialize, Serialize};

use crate::model::*;
use crate::text::Fonts;
use crate::{CoreError, Result};

#[derive(Clone, Debug, PartialEq)]
pub struct LayerPng {
    pub id: LayerId,
    pub key: u64,
    pub png_base64: String,
    pub x_px: i32,
    pub y_px: i32,
    pub w_px: u32,
    pub h_px: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayerSvg {
    pub id: LayerId,
    pub key: u64,
    pub svg: String,
    pub bbox_mm: [f64; 4],
}

const RENDER_CACHE_BYTES: usize = 64 * 1024 * 1024;
const SVG_CACHE_BYTES: usize = 32 * 1024 * 1024;

struct Lru<V> {
    entries: HashMap<u64, (V, usize, u64)>,
    bytes: usize,
    cap: usize,
    tick: u64,
}

impl<V: Clone> Lru<V> {
    fn new(cap: usize) -> Self {
        Lru {
            entries: HashMap::new(),
            bytes: 0,
            cap,
            tick: 0,
        }
    }

    fn get(&mut self, key: u64) -> Option<V> {
        self.tick += 1;
        let tick = self.tick;
        self.entries.get_mut(&key).map(|e| {
            e.2 = tick;
            e.0.clone()
        })
    }

    fn put(&mut self, key: u64, value: V, bytes: usize) {
        self.tick += 1;
        self.bytes += bytes;
        if let Some(old) = self.entries.insert(key, (value, bytes, self.tick)) {
            self.bytes -= old.1;
        }
        while self.bytes > self.cap && self.entries.len() > 1 {
            let Some((&oldest, _)) = self.entries.iter().min_by_key(|(_, e)| e.2) else {
                break;
            };
            if let Some(e) = self.entries.remove(&oldest) {
                self.bytes -= e.1;
            }
        }
    }
}

pub struct RenderCache {
    rasters: Lru<Option<LayerPng>>,
    svgs: Lru<LayerSvg>,
}

impl Default for RenderCache {
    fn default() -> Self {
        RenderCache {
            rasters: Lru::new(RENDER_CACHE_BYTES),
            svgs: Lru::new(SVG_CACHE_BYTES),
        }
    }
}

fn hash_subtree(doc: &Document, root: &str, hasher: &mut DefaultHasher) -> Result<()> {
    for id in crate::ops::subtree(&doc.layers, root) {
        if let Some(l) = doc.layer(&id) {
            serde_json::to_string(l).map_err(CoreError::Json)?.hash(hasher);
        }
    }
    Ok(())
}

impl RenderCache {
    pub fn layers(
        &mut self,
        doc: &Document,
        assets_dir: &Path,
        fonts: &Fonts,
        scale_px_per_mm: f64,
    ) -> Result<(Vec<LayerPng>, usize)> {
        let artboard = serde_json::to_string(&doc.artboard).map_err(CoreError::Json)?;
        let mut out = Vec::new();
        let mut hits = 0;
        for root in roots(doc).into_iter().filter(|l| l.visible) {
            let mut hasher = DefaultHasher::new();
            scale_px_per_mm.to_bits().hash(&mut hasher);
            artboard.hash(&mut hasher);
            hash_subtree(doc, &root.id, &mut hasher)?;
            let key = hasher.finish();
            let png = match self.rasters.get(key) {
                Some(png) => {
                    hits += 1;
                    png
                }
                None => {
                    let r = render_layer(doc, &root.id, assets_dir, fonts, scale_px_per_mm)?;
                    let png = match &r.image {
                        Some(im) => Some(LayerPng {
                            id: r.id.clone(),
                            key,
                            png_base64: crate::image::to_base64_png(im)?,
                            x_px: r.x_px,
                            y_px: r.y_px,
                            w_px: r.w_px,
                            h_px: r.h_px,
                        }),
                        None => None,
                    };
                    let bytes = png.as_ref().map_or(0, |p| p.png_base64.len()) + 64;
                    self.rasters.put(key, png.clone(), bytes);
                    png
                }
            };
            out.extend(png);
        }
        Ok((out, hits))
    }

    pub fn background(
        &mut self,
        doc: &Document,
        assets_dir: &Path,
        fonts: &Fonts,
        scale_px_per_mm: f64,
    ) -> Result<LayerPng> {
        let mut hasher = DefaultHasher::new();
        "background".hash(&mut hasher);
        scale_px_per_mm.to_bits().hash(&mut hasher);
        serde_json::to_string(&doc.artboard)
            .map_err(CoreError::Json)?
            .hash(&mut hasher);
        let key = hasher.finish();
        if let Some(Some(png)) = self.rasters.get(key) {
            return Ok(png);
        }
        let mut bare = doc.clone();
        bare.layers.clear();
        let bg = render_document(
            &bare,
            assets_dir,
            fonts,
            &RenderOptions {
                scale_px_per_mm,
                background: true,
            },
        )?;
        let png = LayerPng {
            id: BACKGROUND_ID.into(),
            key,
            png_base64: crate::image::to_base64_png(&bg)?,
            x_px: 0,
            y_px: 0,
            w_px: bg.width(),
            h_px: bg.height(),
        };
        self.rasters
            .put(key, Some(png.clone()), png.png_base64.len() + 64);
        Ok(png)
    }

    /// One fragment per root layer (hidden ones included), plus `__background` first when the artboard has one.
    pub fn layer_svgs(&mut self, doc: &Document, fonts: &Fonts) -> Result<(Vec<LayerSvg>, usize)> {
        let mut out = Vec::new();
        let mut hits = 0;
        if doc.artboard.background.is_some() {
            let mut hasher = DefaultHasher::new();
            BACKGROUND_ID.hash(&mut hasher);
            serde_json::to_string(&doc.artboard)
                .map_err(CoreError::Json)?
                .hash(&mut hasher);
            let key = hasher.finish();
            let svg = SvgBuilder::new(doc, Path::new(""), fonts, Href::Asset, true)
                .background()
                .unwrap_or_default();
            out.push(LayerSvg {
                id: BACKGROUND_ID.into(),
                key,
                svg,
                bbox_mm: [0.0, 0.0, doc.artboard.width_mm, doc.artboard.height_mm],
            });
        }
        for root in roots(doc) {
            let mut hasher = DefaultHasher::new();
            hash_subtree(doc, &root.id, &mut hasher)?;
            let key = hasher.finish();
            let svg = match self.svgs.get(key) {
                Some(svg) => {
                    hits += 1;
                    svg
                }
                None => {
                    let mut b = SvgBuilder::new(doc, Path::new(""), fonts, Href::Asset, true);
                    let r = placed_bounds(doc, root, Some(fonts), 0);
                    let svg = LayerSvg {
                        id: root.id.clone(),
                        key,
                        svg: b.fragment(root)?,
                        bbox_mm: [r[0], r[1], r[2] - r[0], r[3] - r[1]],
                    };
                    self.svgs.put(key, svg.clone(), svg.svg.len() + 128);
                    svg
                }
            };
            out.push(svg);
        }
        Ok((out, hits))
    }
}

const BACKGROUND_ID: &str = "__background";

#[derive(Clone, Debug)]
pub struct RenderOptions {
    pub scale_px_per_mm: f64,
    pub background: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LayerRaster {
    pub id: LayerId,
    pub x_px: i32,
    pub y_px: i32,
    pub w_px: u32,
    pub h_px: u32,
    #[serde(skip)]
    pub image: Option<RgbaImage>,
}

#[derive(Clone, Debug)]
pub struct SvgOptions {
    pub scale: f64,
    pub outline_text: bool,
    pub embed_images: bool,
}

pub fn render_document(
    doc: &Document,
    assets_dir: &Path,
    fonts: &Fonts,
    opts: &RenderOptions,
) -> Result<RgbaImage> {
    let ab = &doc.artboard;
    let view = [0.0, 0.0, ab.width_mm, ab.height_mm];
    let svg = build_svg(
        doc,
        assets_dir,
        fonts,
        None,
        view,
        [ab.width_mm, ab.height_mm],
        opts.background,
        false,
    )?;
    rasterize_svg(
        &svg,
        px(ab.width_mm, opts.scale_px_per_mm),
        px(ab.height_mm, opts.scale_px_per_mm),
    )
}

pub fn render_layer(
    doc: &Document,
    layer_id: &str,
    assets_dir: &Path,
    fonts: &Fonts,
    scale_px_per_mm: f64,
) -> Result<LayerRaster> {
    let layer = doc
        .layer(layer_id)
        .ok_or_else(|| CoreError::LayerNotFound(layer_id.to_string()))?;
    let mut raster = LayerRaster {
        id: layer.id.clone(),
        x_px: 0,
        y_px: 0,
        w_px: 0,
        h_px: 0,
        image: None,
    };
    let chain = ancestors(doc, layer);
    if !layer.visible || chain.iter().any(|a| !a.visible) {
        return Ok(raster);
    }
    let mut b = placed_bounds(doc, layer, Some(fonts), 0);
    for a in chain.iter().rev() {
        b = transform_box(a, b);
    }
    let ab = &doc.artboard;
    let s = scale_px_per_mm;
    let x0 = (b[0].max(-ab.bleed_mm) * s).floor();
    let y0 = (b[1].max(-ab.bleed_mm) * s).floor();
    let x1 = (b[2].min(ab.width_mm + ab.bleed_mm) * s).ceil();
    let y1 = (b[3].min(ab.height_mm + ab.bleed_mm) * s).ceil();
    if !(x1 > x0 && y1 > y0) {
        return Ok(raster);
    }
    let (w, h) = ((x1 - x0) as u32, (y1 - y0) as u32);
    let view = [x0 / s, y0 / s, f64::from(w) / s, f64::from(h) / s];
    let svg = build_svg(
        doc,
        assets_dir,
        fonts,
        Some(layer),
        view,
        [view[2], view[3]],
        false,
        false,
    )?;
    raster.image = Some(rasterize_svg(&svg, w, h)?);
    raster.x_px = x0 as i32;
    raster.y_px = y0 as i32;
    raster.w_px = w;
    raster.h_px = h;
    Ok(raster)
}

pub fn render_layers(
    doc: &Document,
    assets_dir: &Path,
    fonts: &Fonts,
    scale_px_per_mm: f64,
) -> Result<Vec<LayerRaster>> {
    roots(doc)
        .into_iter()
        .filter(|l| l.visible)
        .map(|l| render_layer(doc, &l.id, assets_dir, fonts, scale_px_per_mm))
        .collect()
}

pub fn render_region(
    doc: &Document,
    assets_dir: &Path,
    fonts: &Fonts,
    scale_px_per_mm: f64,
    rect_mm: [f64; 4],
) -> Result<RgbaImage> {
    let s = scale_px_per_mm;
    let (w, h) = (px(rect_mm[2], s), px(rect_mm[3], s));
    let view = [rect_mm[0], rect_mm[1], f64::from(w) / s, f64::from(h) / s];
    let svg = build_svg(
        doc,
        assets_dir,
        fonts,
        None,
        view,
        [view[2], view[3]],
        true,
        false,
    )?;
    rasterize_svg(&svg, w, h)
}

pub fn to_svg(doc: &Document, assets_dir: &Path, fonts: &Fonts, opts: &SvgOptions) -> Result<String> {
    let ab = &doc.artboard;
    let scale = if opts.scale > 0.0 { opts.scale } else { 1.0 };
    let view = [0.0, 0.0, ab.width_mm, ab.height_mm];
    build_svg(
        doc,
        assets_dir,
        fonts,
        None,
        view,
        [ab.width_mm / scale, ab.height_mm / scale],
        true,
        opts.embed_images,
    )
}

pub fn layer_bounds_mm(doc: &Document, layer: &Layer) -> [f64; 4] {
    let mut b = placed_bounds(doc, layer, None, 0);
    for a in ancestors(doc, layer).iter().rev() {
        b = transform_box(a, b);
    }
    [b[0], b[1], b[2] - b[0], b[3] - b[1]]
}

pub(crate) fn num(v: f64) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() {
        "0".into()
    } else {
        s.into()
    }
}

pub(crate) fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub(crate) fn rasterize_svg(svg: &str, w: u32, h: u32) -> Result<RgbaImage> {
    let fallback = usvg::ImageHrefResolver::default_string_resolver();
    let opt = usvg::Options {
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new(move |href: &str, o: &usvg::Options| match file_href_path(href) {
                Some(p) => fallback(&p.to_string_lossy(), o),
                None => fallback(href, o),
            }),
        },
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| CoreError::Render(e.to_string()))?;
    let mut pixmap = tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| CoreError::Render(format!("invalid raster size {w}x{h}")))?;
    let size = tree.size();
    let ts = tiny_skia::Transform::from_scale(w as f32 / size.width(), h as f32 / size.height());
    resvg::render(&tree, ts, &mut pixmap.as_mut());
    RgbaImage::from_raw(w, h, pixmap.take_demultiplied())
        .ok_or_else(|| CoreError::Render("raster buffer size mismatch".into()))
}

fn px(mm: f64, scale: f64) -> u32 {
    let v = (mm * scale).round();
    if v.is_finite() && v >= 1.0 { v as u32 } else { 1 }
}

fn file_href(path: &Path) -> String {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let s = abs.to_string_lossy().replace('\\', "/");
    let s = s.strip_prefix("//?/").unwrap_or(&s);
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        format!("file:///{s}")
    }
}

fn file_href_path(href: &str) -> Option<PathBuf> {
    let rest = href.strip_prefix("file://")?;
    let rest = if rest.as_bytes().get(2) == Some(&b':') {
        &rest[1..]
    } else {
        rest
    };
    Some(PathBuf::from(rest))
}

fn roots(doc: &Document) -> Vec<&Layer> {
    doc.layers
        .iter()
        .filter(|l| l.parent.as_deref().is_none_or(|p| doc.layer(p).is_none()))
        .collect()
}

fn ancestors<'a>(doc: &'a Document, layer: &Layer) -> Vec<&'a Layer> {
    let mut chain = Vec::new();
    let mut cur = layer.parent.as_deref();
    while let Some(p) = cur {
        let Some(l) = doc.layer(p) else { break };
        if chain.len() >= doc.layers.len() {
            break;
        }
        chain.push(l);
        cur = l.parent.as_deref();
    }
    chain.reverse();
    chain
}

type Bx = [f64; 4];

fn union(a: Bx, b: Bx) -> Bx {
    [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]
}

fn grow(b: Bx, m: f64) -> Bx {
    [b[0] - m, b[1] - m, b[2] + m, b[3] + m]
}

fn transform_box(l: &Layer, b: Bx) -> Bx {
    let (cx, cy) = (l.w_mm / 2.0, l.h_mm / 2.0);
    let (sin, cos) = l.rotation_deg.to_radians().sin_cos();
    let mut out = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for (x, y) in [(b[0], b[1]), (b[2], b[1]), (b[0], b[3]), (b[2], b[3])] {
        let (dx, dy) = (x - cx, y - cy);
        let px = l.x_mm + cx + dx * cos - dy * sin;
        let py = l.y_mm + cy + dx * sin + dy * cos;
        out = [out[0].min(px), out[1].min(py), out[2].max(px), out[3].max(py)];
    }
    out
}

fn is_line(l: &Layer) -> bool {
    matches!(l.kind, LayerKind::Shape(ShapeProps::Line { .. }))
}

fn has_arrows(l: &Layer) -> bool {
    matches!(
        l.kind,
        LayerKind::Shape(ShapeProps::Line {
            arrow_start: true,
            ..
        }) | LayerKind::Shape(ShapeProps::Line { arrow_end: true, .. })
    )
}

const ARROW_LENGTH_PER_WIDTH: f64 = 4.0;

fn closed_path(points: &[(f64, f64)]) -> String {
    let mut d = String::new();
    for (i, (x, y)) in points.iter().enumerate() {
        d.push(if i == 0 { 'M' } else { 'L' });
        d.push_str(&format!("{} {} ", num(*x), num(*y)));
    }
    d.push('Z');
    d
}

pub(crate) fn polygon_points(w: f64, h: f64, sides: u32) -> Vec<(f64, f64)> {
    let n = sides.clamp(3, 64);
    (0..n)
        .map(|i| {
            let a = -std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * f64::from(i) / f64::from(n);
            (w / 2.0 + w / 2.0 * a.cos(), h / 2.0 + h / 2.0 * a.sin())
        })
        .collect()
}

pub(crate) fn star_points(w: f64, h: f64, points: u32, inner_ratio: f64) -> Vec<(f64, f64)> {
    let n = points.clamp(3, 64);
    let inner = inner_ratio.clamp(0.05, 0.95);
    (0..2 * n)
        .map(|i| {
            let a = -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * f64::from(i) / f64::from(n);
            let r = if i % 2 == 0 { 1.0 } else { inner };
            (w / 2.0 + w / 2.0 * r * a.cos(), h / 2.0 + h / 2.0 * r * a.sin())
        })
        .collect()
}

fn stroke_extent(l: &Layer) -> f64 {
    l.strokes
        .iter()
        .map(|s| {
            let w = s.width_mm.max(0.0);
            let reach = match s.align {
                _ if has_arrows(l) => w * ARROW_LENGTH_PER_WIDTH / 2.0,
                _ if is_line(l) => w / 2.0,
                StrokeAlign::Center => w / 2.0,
                StrokeAlign::Outside => w,
                StrokeAlign::Inside => 0.0,
            };
            if s.join == StrokeJoin::Miter {
                reach * 2.0
            } else {
                reach
            }
        })
        .fold(0.0, f64::max)
}

fn local_bounds(doc: &Document, l: &Layer, fonts: Option<&Fonts>, depth: usize) -> Bx {
    let own = [0.0, 0.0, l.w_mm, l.h_mm];
    match &l.kind {
        LayerKind::Group => {
            if depth > doc.layers.len() {
                return own;
            }
            doc.children(Some(&l.id))
                .into_iter()
                .filter(|c| c.visible)
                .map(|c| placed_bounds(doc, c, fonts, depth + 1))
                .reduce(union)
                .unwrap_or(own)
        }
        kind => {
            let mut b = own;
            if let (LayerKind::Text(t), Some(f)) = (kind, fonts)
                && let Ok(run) = f.layout_to_path(t, l.w_mm, l.h_mm)
                && run.bbox_mm[2] > 0.0
            {
                let r = run.bbox_mm;
                b = union(b, [r[0], r[1], r[0] + r[2], r[1] + r[3]]);
            }
            grow(b, stroke_extent(l))
        }
    }
}

fn effect_bounds(l: &Layer, b: Bx) -> Bx {
    let mut out = b;
    let mut blur = 0.0f64;
    for e in &l.effects {
        match e {
            Effect::DropShadow {
                x_mm,
                y_mm,
                blur_mm,
                spread_mm,
                ..
            } => {
                let m = spread_mm.max(0.0) + 1.5 * blur_mm.max(0.0);
                out = union(
                    out,
                    [b[0] + x_mm - m, b[1] + y_mm - m, b[2] + x_mm + m, b[3] + y_mm + m],
                );
            }
            Effect::LayerBlur { radius_mm } => blur = blur.max(1.5 * radius_mm.max(0.0)),
        }
    }
    grow(out, blur)
}

fn placed_bounds(doc: &Document, l: &Layer, fonts: Option<&Fonts>, depth: usize) -> Bx {
    effect_bounds(l, transform_box(l, local_bounds(doc, l, fonts, depth)))
}

fn transform_attr(l: &Layer) -> String {
    let mut t = format!("translate({} {})", num(l.x_mm), num(l.y_mm));
    if l.rotation_deg != 0.0 {
        t += &format!(
            " rotate({} {} {})",
            num(l.rotation_deg),
            num(l.w_mm / 2.0),
            num(l.h_mm / 2.0)
        );
    }
    t
}

pub fn svg_document(view: [f64; 4], size_mm: [f64; 2], fragments: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" viewBox=\"{} {} {} {}\" width=\"{}mm\" height=\"{}mm\">{fragments}</svg>",
        num(view[0]),
        num(view[1]),
        num(view[2]),
        num(view[3]),
        num(size_mm[0]),
        num(size_mm[1]),
    )
}

#[allow(clippy::too_many_arguments)]
fn build_svg(
    doc: &Document,
    assets_dir: &Path,
    fonts: &Fonts,
    only: Option<&Layer>,
    view: [f64; 4],
    size_mm: [f64; 2],
    background: bool,
    embed: bool,
) -> Result<String> {
    let href = if embed { Href::Embed } else { Href::File };
    let mut b = SvgBuilder::new(doc, assets_dir, fonts, href, false);
    let mut body = String::new();
    if background && let Some(bg) = b.background() {
        body += &bg;
    }
    match only {
        Some(layer) => {
            let chain = ancestors(doc, layer);
            for a in &chain {
                body += &format!("<g transform=\"{}\">", transform_attr(a));
            }
            body += &b.fragment(layer)?;
            for _ in &chain {
                body += "</g>";
            }
        }
        None => {
            for l in roots(doc) {
                body += &b.fragment(l)?;
            }
        }
    }
    Ok(svg_document(view, size_mm, &body))
}

#[derive(Clone, Copy, PartialEq)]
enum Href {
    File,
    Embed,
    Asset,
}

fn svg_id(layer_id: &str) -> String {
    layer_id
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
        .collect()
}

struct SvgBuilder<'a> {
    doc: &'a Document,
    assets_dir: &'a Path,
    fonts: &'a Fonts,
    href: Href,
    include_hidden: bool,
    prefix: String,
    defs: String,
    body: String,
    next_id: usize,
}

impl<'a> SvgBuilder<'a> {
    fn new(
        doc: &'a Document,
        assets_dir: &'a Path,
        fonts: &'a Fonts,
        href: Href,
        include_hidden: bool,
    ) -> Self {
        SvgBuilder {
            doc,
            assets_dir,
            fonts,
            href,
            include_hidden,
            prefix: String::new(),
            defs: String::new(),
            body: String::new(),
            next_id: 0,
        }
    }

    fn id(&mut self, kind: &str) -> String {
        self.next_id += 1;
        format!("{}-{kind}{}", self.prefix, self.next_id)
    }

    fn fragment(&mut self, l: &Layer) -> Result<String> {
        self.prefix = svg_id(&l.id);
        self.next_id = 0;
        self.layer(l, 0)?;
        Ok(self.take())
    }

    fn background(&mut self) -> Option<String> {
        let p = self.doc.artboard.background.as_ref()?;
        self.prefix = BACKGROUND_ID.into();
        self.next_id = 0;
        let paint = self.paint(p, "fill");
        self.body = format!(
            "<g data-layer=\"{BACKGROUND_ID}\"><rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" {paint}/></g>",
            num(self.doc.artboard.width_mm),
            num(self.doc.artboard.height_mm)
        );
        Some(self.take())
    }

    fn take(&mut self) -> String {
        let body = std::mem::take(&mut self.body);
        let defs = std::mem::take(&mut self.defs);
        if defs.is_empty() {
            return body;
        }
        let at = body.find('>').map_or(0, |i| i + 1);
        format!("{}<defs>{defs}</defs>{}", &body[..at], &body[at..])
    }

    fn layer(&mut self, l: &Layer, depth: usize) -> Result<()> {
        if (!l.visible && !self.include_hidden) || depth > self.doc.layers.len() {
            return Ok(());
        }
        let mut open = format!("<g data-layer=\"{}\"", esc(&l.id));
        if !l.visible {
            open += " data-hidden=\"1\" display=\"none\"";
        }
        if l.opacity < 1.0 {
            open += &format!(" opacity=\"{}\"", num(l.opacity.max(0.0)));
        }
        open.push('>');
        let region = (!l.effects.is_empty()).then(|| placed_bounds(self.doc, l, Some(self.fonts), 0));
        let shadow = region.and_then(|r| self.effect_filter(l, r, true));
        let blur = region.and_then(|r| self.effect_filter(l, r, false));
        let body_id = shadow.as_ref().map(|_| self.id("b"));
        if let (Some(s), Some(b)) = (&shadow, &body_id) {
            open += &format!("<use xlink:href=\"#{b}\" filter=\"url(#{s})\"/>");
        }
        if let Some(f) = &blur {
            open += &format!("<g filter=\"url(#{f})\">");
        }
        open += "<g";
        if let Some(b) = &body_id {
            open += &format!(" id=\"{b}\"");
        }
        open += &format!(" transform=\"{}\">", transform_attr(l));
        self.body.push_str(&open);
        if let LayerKind::Group = l.kind {
            let doc = self.doc;
            for c in doc.children(Some(&l.id)) {
                self.layer(c, depth + 1)?;
            }
        } else {
            self.content(l)?;
        }
        self.body.push_str(if blur.is_some() {
            "</g></g></g>"
        } else {
            "</g></g>"
        });
        Ok(())
    }

    /// Shadows and blur get separate filters so the body itself stays unfiltered vector (svg2pdf rasterizes filtered groups).
    fn effect_filter(&mut self, l: &Layer, r: Bx, shadows: bool) -> Option<String> {
        let mut prims = String::new();
        let mut merge = Vec::new();
        let mut source = "SourceGraphic".to_string();
        for (i, e) in l.effects.iter().enumerate() {
            match e {
                Effect::DropShadow {
                    x_mm,
                    y_mm,
                    blur_mm,
                    spread_mm,
                    color,
                    opacity,
                } if shadows => {
                    let mut input = "SourceAlpha".to_string();
                    if *spread_mm != 0.0 {
                        let op = if *spread_mm > 0.0 { "dilate" } else { "erode" };
                        prims += &format!(
                            "<feMorphology in=\"SourceAlpha\" operator=\"{op}\" radius=\"{}\" result=\"sp{i}\"/>",
                            num(spread_mm.abs())
                        );
                        input = format!("sp{i}");
                    }
                    prims += &format!(
                        "<feGaussianBlur in=\"{input}\" stdDeviation=\"{}\" result=\"bl{i}\"/><feOffset in=\"bl{i}\" dx=\"{}\" dy=\"{}\" result=\"of{i}\"/><feFlood flood-color=\"{}\" flood-opacity=\"{}\" result=\"fl{i}\"/><feComposite in=\"fl{i}\" in2=\"of{i}\" operator=\"in\" result=\"sh{i}\"/>",
                        num(blur_mm.max(0.0) / 2.0),
                        num(*x_mm),
                        num(*y_mm),
                        esc(color),
                        num(opacity.clamp(0.0, 1.0))
                    );
                    merge.push(format!("sh{i}"));
                }
                Effect::LayerBlur { radius_mm } if !shadows => {
                    prims += &format!(
                        "<feGaussianBlur in=\"{source}\" stdDeviation=\"{}\" result=\"lb{i}\"/>",
                        num(radius_mm.max(0.0) / 2.0)
                    );
                    source = format!("lb{i}");
                }
                _ => {}
            }
        }
        if prims.is_empty() {
            return None;
        }
        if shadows {
            prims += "<feMerge>";
            for m in &merge {
                prims += &format!("<feMergeNode in=\"{m}\"/>");
            }
            prims += "</feMerge>";
        }
        let id = self.id(if shadows { "s" } else { "f" });
        self.defs.push_str(&format!(
            "<filter id=\"{id}\" filterUnits=\"userSpaceOnUse\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" color-interpolation-filters=\"sRGB\">{prims}</filter>",
            num(r[0]),
            num(r[1]),
            num(r[2] - r[0]),
            num(r[3] - r[1])
        ));
        Some(id)
    }

    fn content(&mut self, l: &Layer) -> Result<()> {
        let (w, h) = (num(l.w_mm), num(l.h_mm));
        let mut clip_rule = "nonzero";
        let shape = match &l.kind {
            LayerKind::Group => return Ok(()),
            LayerKind::Shape(ShapeProps::Rect { rx_mm }) => {
                format!(
                    "rect x=\"0\" y=\"0\" width=\"{w}\" height=\"{h}\" rx=\"{}\"",
                    num(rx_mm.max(0.0))
                )
            }
            LayerKind::Shape(ShapeProps::Ellipse) => format!(
                "ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\"",
                num(l.w_mm / 2.0),
                num(l.h_mm / 2.0),
                num(l.w_mm / 2.0),
                num(l.h_mm / 2.0)
            ),
            LayerKind::Shape(ShapeProps::Line { .. }) => {
                let y = num(l.h_mm / 2.0);
                format!("line x1=\"0\" y1=\"{y}\" x2=\"{w}\" y2=\"{y}\"")
            }
            LayerKind::Shape(ShapeProps::Polygon { sides }) => {
                format!(
                    "path d=\"{}\"",
                    closed_path(&polygon_points(l.w_mm, l.h_mm, *sides))
                )
            }
            LayerKind::Shape(ShapeProps::Star { points, inner_ratio }) => format!(
                "path d=\"{}\"",
                closed_path(&star_points(l.w_mm, l.h_mm, *points, *inner_ratio))
            ),
            LayerKind::Path(p) => {
                if p.fill_rule == FillRule::Evenodd {
                    clip_rule = "evenodd";
                }
                format!("path d=\"{}\" fill-rule=\"{clip_rule}\"", esc(&p.d))
            }
            LayerKind::Text(t) => {
                let run = self.fonts.layout_to_path(t, l.w_mm, l.h_mm)?;
                format!("path d=\"{}\"", run.path_d)
            }
            LayerKind::Image(_) => format!("rect x=\"0\" y=\"0\" width=\"{w}\" height=\"{h}\""),
        };
        let line = is_line(l);
        for s in l
            .strokes
            .iter()
            .filter(|s| s.align == StrokeAlign::Outside && !line)
        {
            self.stroke(&shape, s, 2.0, None);
        }
        if let LayerKind::Image(img) = &l.kind {
            self.image(l, img)?;
        } else if !line {
            for f in &l.fills {
                let paint = self.paint(f, "fill");
                self.body.push_str(&format!("<{shape} {paint}/>"));
            }
        }
        for s in &l.strokes {
            match s.align {
                _ if line => {
                    self.stroke(&shape, s, 1.0, None);
                    self.arrows(l, s);
                }
                StrokeAlign::Outside => {}
                StrokeAlign::Center => self.stroke(&shape, s, 1.0, None),
                StrokeAlign::Inside => {
                    let clip = self.id("c");
                    self.defs.push_str(&format!(
                        "<clipPath id=\"{clip}\"><{shape} clip-rule=\"{clip_rule}\"/></clipPath>"
                    ));
                    self.stroke(&shape, s, 2.0, Some(&clip));
                }
            }
        }
        Ok(())
    }

    fn arrows(&mut self, l: &Layer, s: &Stroke) {
        let LayerKind::Shape(ShapeProps::Line {
            arrow_start,
            arrow_end,
        }) = l.kind
        else {
            return;
        };
        if !s.width_mm.is_finite() || s.width_mm <= 0.0 {
            return;
        }
        let len = s.width_mm * ARROW_LENGTH_PER_WIDTH;
        let cy = l.h_mm / 2.0;
        let mut heads = Vec::new();
        if arrow_start {
            heads.push([(0.0, cy), (len, cy - len / 2.0), (len, cy + len / 2.0)]);
        }
        if arrow_end {
            heads.push([
                (l.w_mm, cy),
                (l.w_mm - len, cy - len / 2.0),
                (l.w_mm - len, cy + len / 2.0),
            ]);
        }
        for head in heads {
            let paint = self.paint(&s.paint, "fill");
            self.body
                .push_str(&format!("<path d=\"{}\" {paint}/>", closed_path(&head)));
        }
    }

    fn adjust_filter(&mut self, a: &ImageAdjust) -> Option<String> {
        let clamp = |v: f64| if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 };
        let (b, c, s) = (clamp(a.brightness), clamp(a.contrast), clamp(a.saturation));
        if b == 0.0 && c == 0.0 && s == 0.0 {
            return None;
        }
        let id = self.id("adj");
        let slope = 1.0 + c;
        let intercept = 0.5 - 0.5 * slope + 0.5 * b;
        let func = |ch: char| {
            format!(
                "<feFunc{ch} type=\"linear\" slope=\"{}\" intercept=\"{}\"/>",
                num(slope),
                num(intercept)
            )
        };
        self.defs.push_str(&format!(
            "<filter id=\"{id}\" color-interpolation-filters=\"sRGB\"><feComponentTransfer>{}{}{}</feComponentTransfer><feColorMatrix type=\"saturate\" values=\"{}\"/></filter>",
            func('R'),
            func('G'),
            func('B'),
            num(1.0 + s)
        ));
        Some(id)
    }

    fn stroke(&mut self, shape: &str, s: &Stroke, mult: f64, clip: Option<&str>) {
        if !s.width_mm.is_finite() || s.width_mm <= 0.0 {
            return;
        }
        let paint = self.paint(&s.paint, "stroke");
        let join = match s.join {
            StrokeJoin::Miter => "miter",
            StrokeJoin::Round => "round",
            StrokeJoin::Bevel => "bevel",
        };
        let mut e = format!(
            "<{shape} fill=\"none\" {paint} stroke-width=\"{}\" stroke-linejoin=\"{join}\"",
            num(s.width_mm * mult)
        );
        if s.dash_mm.iter().any(|d| *d > 0.0) {
            let dash: Vec<String> = s.dash_mm.iter().map(|d| num(d.max(0.0))).collect();
            e += &format!(" stroke-dasharray=\"{}\"", dash.join(" "));
        }
        if let Some(c) = clip {
            e += &format!(" clip-path=\"url(#{c})\"");
        }
        e += "/>";
        self.body.push_str(&e);
    }

    fn paint(&mut self, p: &Paint, attr: &str) -> String {
        match p {
            Paint::Solid { color, opacity } => {
                format!(
                    "{attr}=\"{}\" {attr}-opacity=\"{}\"",
                    esc(color),
                    num(opacity.clamp(0.0, 1.0))
                )
            }
            Paint::LinearGradient { angle_deg, stops } => {
                let id = self.id("g");
                let mut g = format!(
                    "<linearGradient id=\"{id}\" gradientTransform=\"rotate({} 0.5 0.5)\">",
                    num(*angle_deg)
                );
                for s in stops {
                    g += &format!(
                        "<stop offset=\"{}\" stop-color=\"{}\" stop-opacity=\"{}\"/>",
                        num(s.offset.clamp(0.0, 1.0)),
                        esc(&s.color),
                        num(s.opacity.clamp(0.0, 1.0))
                    );
                }
                g += "</linearGradient>";
                self.defs.push_str(&g);
                format!("{attr}=\"url(#{id})\"")
            }
        }
    }

    fn image(&mut self, l: &Layer, img: &ImageProps) -> Result<()> {
        let path = self.assets_dir.join(&img.asset);
        let href = if self.href == Href::Asset {
            format!("asset:{}", img.asset)
        } else if !path.is_file() {
            return Err(CoreError::AssetNotFound(img.asset.clone()));
        } else if self.href == Href::Embed {
            let bytes = std::fs::read(&path)?;
            let mime = if bytes.starts_with(&[0xFF, 0xD8]) {
                "image/jpeg"
            } else if bytes.get(8..12) == Some(b"WEBP".as_slice()) {
                "image/webp"
            } else {
                "image/png"
            };
            format!(
                "data:{mime};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&bytes)
            )
        } else {
            file_href(&path)
        };
        let fit = match img.fit {
            ImageFit::Fill => "none",
            ImageFit::Contain => "xMidYMid meet",
            ImageFit::Cover => "xMidYMid slice",
        };
        let filter = self.adjust_filter(&img.adjust);
        if let Some(f) = &filter {
            self.body.push_str(&format!("<g filter=\"url(#{f})\">"));
        }
        let (w, h) = (num(l.w_mm), num(l.h_mm));
        match img.crop_px {
            Some([cx, cy, cw, ch]) if cw > 0 && ch > 0 && img.natural_w_px > 0 && img.natural_h_px > 0 => {
                self.body.push_str(&format!(
                    "<svg x=\"0\" y=\"0\" width=\"{w}\" height=\"{h}\" viewBox=\"{cx} {cy} {cw} {ch}\" preserveAspectRatio=\"{fit}\"><image x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" xlink:href=\"{}\"/></svg>",
                    img.natural_w_px,
                    img.natural_h_px,
                    esc(&href)
                ));
            }
            _ => self.body.push_str(&format!(
                "<image x=\"0\" y=\"0\" width=\"{w}\" height=\"{h}\" preserveAspectRatio=\"{fit}\" xlink:href=\"{}\"/>",
                esc(&href)
            )),
        }
        if filter.is_some() {
            self.body.push_str("</g>");
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::OnceLock;

    use super::*;

    pub(crate) fn fonts() -> &'static Fonts {
        static FONTS: OnceLock<Fonts> = OnceLock::new();
        FONTS.get_or_init(|| Fonts::new(None))
    }

    pub(crate) fn solid(color: &str) -> Paint {
        Paint::Solid {
            color: color.into(),
            opacity: 1.0,
        }
    }

    pub(crate) fn shape_layer(id: &str, kind: LayerKind, rect: [f64; 4], fill: &str) -> Layer {
        Layer {
            id: id.into(),
            name: id.into(),
            parent: None,
            kind,
            x_mm: rect[0],
            y_mm: rect[1],
            w_mm: rect[2],
            h_mm: rect[3],
            rotation_deg: 0.0,
            opacity: 1.0,
            visible: true,
            locked: false,
            fills: vec![solid(fill)],
            strokes: vec![],
            effects: vec![],
            strategy: Strategy::NativeVector,
            strategy_reason: String::new(),
            pinned: vec![],
            source: None,
        }
    }

    pub(crate) fn rect(id: &str, r: [f64; 4], fill: &str) -> Layer {
        shape_layer(id, LayerKind::Shape(ShapeProps::Rect { rx_mm: 0.0 }), r, fill)
    }

    fn stroke(width: f64, align: StrokeAlign, color: &str) -> Stroke {
        Stroke {
            paint: solid(color),
            width_mm: width,
            align,
            join: StrokeJoin::Round,
            dash_mm: vec![],
        }
    }

    fn rgb(img: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
        img.get_pixel(x, y).0
    }

    fn opts(scale: f64) -> RenderOptions {
        RenderOptions {
            scale_px_per_mm: scale,
            background: true,
        }
    }

    fn tmp() -> PathBuf {
        std::env::temp_dir()
    }

    #[test]
    fn render_cache_rerenders_only_the_changed_layer() {
        let mut doc = Document::new(100.0, 50.0);
        doc.layers = vec![
            rect("a", [0.0, 0.0, 20.0, 10.0], "#FF0000"),
            rect("b", [30.0, 0.0, 20.0, 10.0], "#00FF00"),
            rect("c", [60.0, 0.0, 20.0, 10.0], "#0000FF"),
        ];
        let mut cache = RenderCache::default();
        let (first, hits) = cache.layers(&doc, &tmp(), fonts(), 2.0).expect("first");
        assert_eq!((first.len(), hits), (3, 0));

        doc.layers[1].x_mm = 35.0;
        let (second, hits) = cache.layers(&doc, &tmp(), fonts(), 2.0).expect("second");
        assert_eq!(hits, 2);
        assert_eq!(second[0], first[0]);
        assert_eq!(second[2], first[2]);
        assert_eq!(second[1].x_px, 70);

        let (_, hits) = cache.layers(&doc, &tmp(), fonts(), 4.0).expect("new scale");
        assert_eq!(hits, 0);
        let bg = cache.background(&doc, &tmp(), fonts(), 2.0).expect("bg");
        assert_eq!(
            cache.background(&doc, &tmp(), fonts(), 2.0).expect("bg again"),
            bg
        );
    }

    #[test]
    fn layer_svgs_compose_into_the_document_and_cache_by_content() {
        let gradient = Paint::LinearGradient {
            angle_deg: 0.0,
            stops: vec![
                GradientStop {
                    offset: 0.0,
                    color: "#FF0000".into(),
                    opacity: 1.0,
                },
                GradientStop {
                    offset: 1.0,
                    color: "#0000FF".into(),
                    opacity: 1.0,
                },
            ],
        };
        let mut doc = Document::new(100.0, 50.0);
        doc.artboard.background = Some(gradient.clone());
        let mut group = shape_layer("G", LayerKind::Group, [10.0, 5.0, 60.0, 30.0], "#000000");
        group.fills.clear();
        let mut shown = rect("c1", [0.0, 0.0, 20.0, 10.0], "#000000");
        shown.parent = Some("G".into());
        shown.fills = vec![gradient.clone()];
        let mut hidden = rect("c2", [25.0, 0.0, 20.0, 10.0], "#00FF00");
        hidden.parent = Some("G".into());
        hidden.visible = false;
        let mut other = rect("b", [70.0, 20.0, 20.0, 20.0], "#000000");
        other.fills = vec![gradient];
        doc.layers = vec![group, shown, hidden, other];

        let mut cache = RenderCache::default();
        let (svgs, hits) = cache.layer_svgs(&doc, fonts()).expect("svgs");
        assert_eq!(hits, 0);
        let ids: Vec<&str> = svgs.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["__background", "G", "b"]);
        assert!(
            svgs[1].svg.starts_with("<g data-layer=\"G\"><defs>"),
            "{}",
            svgs[1].svg
        );
        assert!(svgs[1].svg.contains("<g data-layer=\"c1\">"));
        assert!(
            svgs[1]
                .svg
                .contains("<g data-layer=\"c2\" data-hidden=\"1\" display=\"none\">")
        );
        assert!(svgs[0].svg.contains("id=\"__background-g1\""));
        assert!(svgs[1].svg.contains("id=\"G-g1\""));
        assert!(svgs[2].svg.contains("id=\"b-g1\""));
        assert_eq!(svgs[1].bbox_mm, [10.0, 5.0, 20.0, 10.0]);
        assert_eq!(format!("{:016x}", svgs[1].key).len(), 16);

        let fragments: String = svgs.iter().map(|s| s.svg.as_str()).collect();
        let composed = rasterize_svg(
            &svg_document([0.0, 0.0, 100.0, 50.0], [100.0, 50.0], &fragments),
            200,
            100,
        )
        .expect("composed");
        let direct = render_document(&doc, &tmp(), fonts(), &opts(2.0)).expect("direct");
        assert!(composed == direct, "fragments and render_document differ");
        assert_eq!(rgb(&direct, 55, 25)[1], 0, "hidden layer stays hidden");

        let (again, hits) = cache.layer_svgs(&doc, fonts()).expect("again");
        assert_eq!((hits, &again), (2, &svgs));
        doc.layers[1].x_mm = 1.0;
        let (changed, hits) = cache.layer_svgs(&doc, fonts()).expect("changed");
        assert_eq!(hits, 1);
        assert_ne!(changed[1].key, svgs[1].key);
        assert_eq!(changed[2], svgs[2]);

        doc.layers.push(shape_layer(
            "pic",
            LayerKind::Image(ImageProps {
                asset: "photo 1.png".into(),
                natural_w_px: 40,
                natural_h_px: 20,
                ..Default::default()
            }),
            [0.0, 0.0, 40.0, 20.0],
            "#000000",
        ));
        let (with_image, _) = cache.layer_svgs(&doc, fonts()).expect("image placeholder");
        assert!(with_image[3].svg.contains("xlink:href=\"asset:photo 1.png\""));
    }

    #[test]
    fn drop_shadow_is_a_filtered_copy_under_an_unfiltered_body() {
        let mut doc = Document::new(60.0, 40.0);
        let mut l = rect("s", [10.0, 10.0, 30.0, 15.0], "#1C6B33");
        l.rotation_deg = 15.0;
        l.effects = vec![
            Effect::DropShadow {
                x_mm: 3.0,
                y_mm: 2.0,
                blur_mm: 2.0,
                spread_mm: 0.5,
                color: "#000000".into(),
                opacity: 0.6,
            },
            Effect::LayerBlur { radius_mm: 0.4 },
        ];
        doc.layers.push(l.clone());
        let svg = to_svg(
            &doc,
            &tmp(),
            fonts(),
            &SvgOptions {
                scale: 1.0,
                outline_text: true,
                embed_images: false,
            },
        )
        .expect("svg");
        assert!(
            svg.contains(
                "<use xlink:href=\"#s-b3\" filter=\"url(#s-s1)\"/><g filter=\"url(#s-f2)\"><g id=\"s-b3\""
            ),
            "{svg}"
        );
        assert!(
            !svg.contains("SourceGraphic\"/></feMerge>"),
            "shadow filter must not merge the body"
        );

        let r = placed_bounds(&doc, &l, None, 0);
        let old = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 60 40\" width=\"60mm\" height=\"40mm\"><defs><filter id=\"o\" filterUnits=\"userSpaceOnUse\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" color-interpolation-filters=\"sRGB\"><feMorphology in=\"SourceAlpha\" operator=\"dilate\" radius=\"0.5\" result=\"sp\"/><feGaussianBlur in=\"sp\" stdDeviation=\"1\" result=\"bl\"/><feOffset in=\"bl\" dx=\"3\" dy=\"2\" result=\"of\"/><feFlood flood-color=\"#000000\" flood-opacity=\"0.6\" result=\"fl\"/><feComposite in=\"fl\" in2=\"of\" operator=\"in\" result=\"sh\"/><feGaussianBlur in=\"SourceGraphic\" stdDeviation=\"0.2\" result=\"lb\"/><feMerge><feMergeNode in=\"sh\"/><feMergeNode in=\"lb\"/></feMerge></filter></defs><rect width=\"60\" height=\"40\" fill=\"#FFFFFF\"/><g filter=\"url(#o)\"><g transform=\"{}\"><rect width=\"30\" height=\"15\" fill=\"#1C6B33\"/></g></g></svg>",
            num(r[0]),
            num(r[1]),
            num(r[2] - r[0]),
            num(r[3] - r[1]),
            transform_attr(&l)
        );
        let before = rasterize_svg(&old, 240, 160).expect("old structure");
        let after = render_document(&doc, &tmp(), fonts(), &opts(4.0)).expect("new structure");
        let worst = before
            .pixels()
            .zip(after.pixels())
            .flat_map(|(a, b)| a.0.iter().zip(b.0.iter()).map(|(x, y)| x.abs_diff(*y)))
            .max()
            .unwrap_or(0);
        assert!(
            worst <= 2,
            "split shadow differs from the merged filter by {worst}"
        );
    }

    #[test]
    fn red_rect_renders_red_at_the_right_pixel() {
        let mut doc = Document::new(100.0, 50.0);
        doc.layers.push(rect("L1", [10.0, 10.0, 20.0, 10.0], "#FF0000"));
        let img = render_document(&doc, &tmp(), fonts(), &opts(2.0)).expect("render");
        assert_eq!(img.dimensions(), (200, 100));
        assert_eq!(rgb(&img, 40, 30), [255, 0, 0, 255]);
        assert_eq!(rgb(&img, 10, 10), [255, 255, 255, 255]);
        assert_eq!(rgb(&img, 61, 30), [255, 255, 255, 255]);
        let no_bg = render_document(
            &doc,
            &tmp(),
            fonts(),
            &RenderOptions {
                scale_px_per_mm: 2.0,
                background: false,
            },
        )
        .expect("render");
        assert_eq!(rgb(&no_bg, 10, 10)[3], 0);
    }

    #[test]
    fn outside_stroke_widens_bounds_and_raster() {
        let mut doc = Document::new(100.0, 50.0);
        let mut l = rect("L1", [10.0, 10.0, 20.0, 10.0], "#FF0000");
        assert_eq!(layer_bounds_mm(&doc, &l), [10.0, 10.0, 20.0, 10.0]);
        l.strokes.push(stroke(2.0, StrokeAlign::Outside, "#0000FF"));
        assert_eq!(layer_bounds_mm(&doc, &l), [8.0, 8.0, 24.0, 14.0]);
        doc.layers.push(l);
        let r = render_layer(&doc, "L1", &tmp(), fonts(), 1.0).expect("render");
        assert_eq!((r.x_px, r.y_px, r.w_px, r.h_px), (8, 8, 24, 14));
        let img = r.image.expect("image");
        assert_eq!(rgb(&img, 1, 7), [0, 0, 255, 255]);
        assert_eq!(rgb(&img, 12, 7), [255, 0, 0, 255]);
    }

    #[test]
    fn inside_stroke_stays_inside_and_center_straddles() {
        let mut doc = Document::new(40.0, 40.0);
        let mut l = rect("L1", [10.0, 10.0, 20.0, 20.0], "#FF0000");
        l.strokes.push(stroke(2.0, StrokeAlign::Inside, "#0000FF"));
        assert_eq!(layer_bounds_mm(&doc, &l), [10.0, 10.0, 20.0, 20.0]);
        doc.layers.push(l);
        let img = render_document(&doc, &tmp(), fonts(), &opts(4.0)).expect("render");
        assert_eq!(rgb(&img, 42, 80), [0, 0, 255, 255]);
        assert_eq!(rgb(&img, 38, 80), [255, 255, 255, 255]);
        assert_eq!(rgb(&img, 80, 80), [255, 0, 0, 255]);
    }

    #[test]
    fn groups_offset_children_and_rotation_affects_bounds() {
        let mut doc = Document::new(100.0, 100.0);
        let mut g = rect("G", [50.0, 50.0, 10.0, 10.0], "#000000");
        g.kind = LayerKind::Group;
        let mut child = rect("C", [0.0, 0.0, 10.0, 10.0], "#00FF00");
        child.parent = Some("G".into());
        doc.layers.push(child);
        doc.layers.push(g);
        let img = render_document(&doc, &tmp(), fonts(), &opts(1.0)).expect("render");
        assert_eq!(rgb(&img, 55, 55), [0, 255, 0, 255]);
        assert_eq!(rgb(&img, 5, 5), [255, 255, 255, 255]);
        let c = doc.layer("C").expect("child").clone();
        assert_eq!(layer_bounds_mm(&doc, &c), [50.0, 50.0, 10.0, 10.0]);
        let rasters = render_layers(&doc, &tmp(), fonts(), 1.0).expect("layers");
        assert_eq!(rasters.len(), 1);
        assert_eq!(
            (rasters[0].id.as_str(), rasters[0].x_px, rasters[0].y_px),
            ("G", 50, 50)
        );

        let mut r = rect("R", [0.0, 0.0, 20.0, 10.0], "#000000");
        r.rotation_deg = 90.0;
        let b = layer_bounds_mm(&doc, &r);
        assert!(
            (b[0] - 5.0).abs() < 1e-9
                && (b[1] + 5.0).abs() < 1e-9
                && (b[2] - 10.0).abs() < 1e-9
                && (b[3] - 20.0).abs() < 1e-9,
            "{b:?}"
        );
    }

    #[test]
    fn drop_shadow_and_gradient_render() {
        let mut doc = Document::new(60.0, 60.0);
        let mut l = rect("L1", [10.0, 10.0, 20.0, 20.0], "#FF0000");
        l.fills = vec![Paint::LinearGradient {
            angle_deg: 0.0,
            stops: vec![
                GradientStop {
                    offset: 0.0,
                    color: "#000000".into(),
                    opacity: 1.0,
                },
                GradientStop {
                    offset: 1.0,
                    color: "#FFFFFF".into(),
                    opacity: 1.0,
                },
            ],
        }];
        l.effects.push(Effect::DropShadow {
            x_mm: 10.0,
            y_mm: 10.0,
            blur_mm: 0.0,
            spread_mm: 0.0,
            color: "#0000FF".into(),
            opacity: 1.0,
        });
        doc.layers.push(l.clone());
        let img = render_document(&doc, &tmp(), fonts(), &opts(1.0)).expect("render");
        assert_eq!(rgb(&img, 35, 35), [0, 0, 255, 255]);
        let left = rgb(&img, 11, 20)[0];
        let right = rgb(&img, 28, 20)[0];
        assert!(left < 40 && right > 215, "gradient {left} {right}");
        let b = layer_bounds_mm(&doc, &l);
        assert_eq!(b, [10.0, 10.0, 30.0, 30.0]);
    }

    #[test]
    fn text_layer_renders_ink() {
        let f = fonts();
        if f.families().is_empty() {
            return;
        }
        let mut doc = Document::new(100.0, 30.0);
        let props = TextProps {
            text: "배너 Banner".into(),
            font_family: "No Such Font".into(),
            font_weight: 700,
            font_size_mm: 12.0,
            letter_spacing_mm: 0.0,
            line_height: 1.2,
            align: TextAlign::Left,
            ..Default::default()
        };
        let run = f.layout_to_path(&props, 90.0, 20.0).expect("layout");
        assert!(!run.path_d.is_empty());
        doc.layers.push(shape_layer(
            "T",
            LayerKind::Text(props),
            [5.0, 5.0, 90.0, 20.0],
            "#000000",
        ));
        let img = render_document(&doc, &tmp(), f, &opts(2.0)).expect("render");
        let dark = img.pixels().filter(|p| p[0] < 100).count();
        assert!(dark > 200, "dark pixels {dark}");
        let svg = to_svg(
            &doc,
            &tmp(),
            f,
            &SvgOptions {
                scale: 10.0,
                outline_text: true,
                embed_images: true,
            },
        )
        .expect("svg");
        assert!(svg.contains("width=\"10mm\"") && svg.contains("viewBox=\"0 0 100 30\""));
        assert!(!svg.contains("<text"));
    }

    #[test]
    fn image_layer_renders_from_assets_dir() {
        let dir = std::env::temp_dir().join(format!("hoist-core-render-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let src = RgbaImage::from_pixel(4, 4, image::Rgba([0, 128, 255, 255]));
        src.save(dir.join("a.png")).expect("save");
        let mut doc = Document::new(20.0, 20.0);
        let mut l = shape_layer(
            "I",
            LayerKind::Image(ImageProps {
                asset: "a.png".into(),
                natural_w_px: 4,
                natural_h_px: 4,
                fit: ImageFit::Fill,
                ..Default::default()
            }),
            [5.0, 5.0, 10.0, 10.0],
            "#000000",
        );
        l.fills.clear();
        doc.layers.push(l);
        let img = render_document(&doc, &dir, fonts(), &opts(1.0)).expect("render");
        assert_eq!(rgb(&img, 10, 10), [0, 128, 255, 255]);
        let svg = to_svg(
            &doc,
            &dir,
            fonts(),
            &SvgOptions {
                scale: 1.0,
                outline_text: true,
                embed_images: true,
            },
        )
        .expect("svg");
        assert!(svg.contains("data:image/png;base64,"));
        let missing = render_document(&doc, &tmp().join("nope-dir"), fonts(), &opts(1.0));
        assert!(matches!(missing, Err(CoreError::AssetNotFound(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn new_shapes_arrows_and_image_options_render() {
        let mut doc = Document::new(100.0, 50.0);
        let mut line = shape_layer(
            "l",
            LayerKind::Shape(ShapeProps::Line {
                arrow_start: false,
                arrow_end: true,
            }),
            [0.0, 30.0, 100.0, 4.0],
            "#000000",
        );
        line.fills.clear();
        line.strokes.push(stroke(1.0, StrokeAlign::Center, "#0000FF"));
        doc.layers = vec![
            shape_layer(
                "p",
                LayerKind::Shape(ShapeProps::Polygon { sides: 6 }),
                [0.0, 0.0, 20.0, 20.0],
                "#FF0000",
            ),
            shape_layer(
                "s",
                LayerKind::Shape(ShapeProps::Star {
                    points: 5,
                    inner_ratio: 0.4,
                }),
                [50.0, 0.0, 20.0, 20.0],
                "#00FF00",
            ),
            line,
        ];
        let img = render_document(&doc, &tmp(), fonts(), &opts(4.0)).expect("render");
        assert_eq!(rgb(&img, 40, 40), [255, 0, 0, 255]);
        assert_ne!(rgb(&img, 2, 2), [255, 0, 0, 255], "hexagon corner is empty");
        assert_eq!(rgb(&img, 240, 40), [0, 255, 0, 255]);
        assert_ne!(rgb(&img, 204, 4), [0, 255, 0, 255], "star corner is empty");
        assert_eq!(
            rgb(&img, 200, 128),
            [0, 0, 255, 255],
            "line runs through the middle"
        );
        assert_eq!(rgb(&img, 388, 123), [0, 0, 255, 255], "arrowhead beside the line");
        assert_ne!(rgb(&img, 20, 123), [0, 0, 255, 255], "no arrowhead at the start");

        let dir = std::env::temp_dir().join(format!("hoist-core-adjust-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        RgbaImage::from_pixel(40, 40, image::Rgba([100, 100, 100, 255]))
            .save(dir.join("a.png"))
            .expect("save");
        let mut photo = Document::new(20.0, 20.0);
        let mut l = shape_layer(
            "I",
            LayerKind::Image(ImageProps {
                asset: "a.png".into(),
                natural_w_px: 40,
                natural_h_px: 40,
                crop_px: Some([10, 10, 20, 20]),
                adjust: ImageAdjust {
                    brightness: 0.4,
                    contrast: 0.0,
                    saturation: -1.0,
                },
                ..Default::default()
            }),
            [0.0, 0.0, 20.0, 20.0],
            "#000000",
        );
        l.fills.clear();
        photo.layers.push(l);
        let svg = to_svg(
            &photo,
            &dir,
            fonts(),
            &SvgOptions {
                scale: 1.0,
                outline_text: true,
                embed_images: false,
            },
        )
        .expect("svg");
        assert!(svg.contains("viewBox=\"10 10 20 20\""), "{svg}");
        assert!(
            svg.contains("feColorMatrix type=\"saturate\" values=\"0\""),
            "{svg}"
        );
        let lit = render_document(&photo, &dir, fonts(), &opts(1.0)).expect("render");
        assert!(
            rgb(&lit, 10, 10)[0] > 150,
            "brightness raised {:?}",
            rgb(&lit, 10, 10)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn region_matches_document_crop() {
        let mut doc = Document::new(50.0, 50.0);
        doc.layers.push(rect("L1", [10.0, 10.0, 10.0, 10.0], "#FF0000"));
        let region = render_region(&doc, &tmp(), fonts(), 2.0, [5.0, 5.0, 20.0, 20.0]).expect("region");
        assert_eq!(region.dimensions(), (40, 40));
        assert_eq!(rgb(&region, 20, 20), [255, 0, 0, 255]);
        assert_eq!(rgb(&region, 2, 2), [255, 255, 255, 255]);
    }
}
