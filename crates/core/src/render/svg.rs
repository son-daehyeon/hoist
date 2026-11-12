use std::path::Path;

use base64::Engine;

use super::bounds::{ARROW_LENGTH_PER_WIDTH, Corners, is_line, placed_bounds, to_xywh};
use super::file_href;
use crate::model::*;
use crate::text::Fonts;
use crate::{CoreError, Result};

pub(super) const BACKGROUND_ID: &str = "__background";

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

pub(crate) fn closed_path(points: &[(f64, f64)]) -> String {
    let mut d = String::new();
    for (i, (x, y)) in points.iter().enumerate() {
        d.push(if i == 0 { 'M' } else { 'L' });
        d.push_str(&format!("{} {} ", num(*x), num(*y)));
    }
    d.push('Z');
    d
}

fn polygon_points(w: f64, h: f64, sides: u32) -> Vec<(f64, f64)> {
    let n = sides.clamp(3, 64);
    (0..n)
        .map(|i| {
            let a = -std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * f64::from(i) / f64::from(n);
            (w / 2.0 + w / 2.0 * a.cos(), h / 2.0 + h / 2.0 * a.sin())
        })
        .collect()
}

fn star_points(w: f64, h: f64, points: u32, inner_ratio: f64) -> Vec<(f64, f64)> {
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

pub(super) fn transform_attr(l: &Layer) -> String {
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
pub(super) fn build_svg(
    doc: &Document,
    assets_dir: &Path,
    fonts: &Fonts,
    only: Option<&Layer>,
    view: [f64; 4],
    size_mm: [f64; 2],
    background: bool,
    href: Href,
) -> Result<String> {
    let mut b = SvgBuilder::new(doc, assets_dir, fonts, href, false);
    let mut body = String::new();
    if background && let Some(bg) = b.background() {
        body += &bg;
    }
    match only {
        Some(layer) => {
            let chain = doc.ancestors(layer);
            for a in &chain {
                body += &format!("<g transform=\"{}\">", transform_attr(a));
            }
            body += &b.fragment(layer)?;
            for _ in &chain {
                body += "</g>";
            }
        }
        None => {
            for l in doc.roots() {
                body += &b.fragment(l)?;
            }
        }
    }
    Ok(svg_document(view, size_mm, &body))
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Href {
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

pub(super) struct SvgBuilder<'a> {
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
    pub(super) fn new(
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

    pub(super) fn fragment(&mut self, l: &Layer) -> Result<String> {
        self.prefix = svg_id(&l.id);
        self.next_id = 0;
        self.layer(l, 0)?;
        Ok(self.take())
    }

    pub(super) fn background(&mut self) -> Option<String> {
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
        let shadow = region.and_then(|r| self.shadow_filter(l, r));
        let blur = region.and_then(|r| self.blur_filter(l, r));
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
    fn shadow_filter(&mut self, l: &Layer, r: Corners) -> Option<String> {
        let mut prims = String::new();
        let mut merge = Vec::new();
        for (i, e) in l.effects.iter().enumerate() {
            let Effect::DropShadow {
                x_mm,
                y_mm,
                blur_mm,
                spread_mm,
                color,
                opacity,
            } = e
            else {
                continue;
            };
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
        if prims.is_empty() {
            return None;
        }
        prims += "<feMerge>";
        for m in &merge {
            prims += &format!("<feMergeNode in=\"{m}\"/>");
        }
        prims += "</feMerge>";
        Some(self.define_filter("s", r, &prims))
    }

    fn blur_filter(&mut self, l: &Layer, r: Corners) -> Option<String> {
        let mut prims = String::new();
        let mut source = "SourceGraphic".to_string();
        for (i, e) in l.effects.iter().enumerate() {
            if let Effect::LayerBlur { radius_mm } = e {
                prims += &format!(
                    "<feGaussianBlur in=\"{source}\" stdDeviation=\"{}\" result=\"lb{i}\"/>",
                    num(radius_mm.max(0.0) / 2.0)
                );
                source = format!("lb{i}");
            }
        }
        if prims.is_empty() {
            return None;
        }
        Some(self.define_filter("f", r, &prims))
    }

    fn define_filter(&mut self, kind: &str, r: Corners, prims: &str) -> String {
        let id = self.id(kind);
        let [x, y, w, h] = to_xywh(r);
        self.defs.push_str(&format!(
            "<filter id=\"{id}\" filterUnits=\"userSpaceOnUse\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" color-interpolation-filters=\"sRGB\">{prims}</filter>",
            num(x),
            num(y),
            num(w),
            num(h)
        ));
        id
    }

    fn content(&mut self, l: &Layer) -> Result<()> {
        let Some((shape, clip_rule)) = self.shape_element(l)? else {
            return Ok(());
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

    fn shape_element(&self, l: &Layer) -> Result<Option<(String, &'static str)>> {
        let (w, h) = (num(l.w_mm), num(l.h_mm));
        let mut clip_rule = "nonzero";
        let shape = match &l.kind {
            LayerKind::Group => return Ok(None),
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
        Ok(Some((shape, clip_rule)))
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
