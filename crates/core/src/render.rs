use std::path::{Path, PathBuf};

use image::RgbaImage;

use crate::model::*;
use crate::text::Fonts;
use crate::{CoreError, Result};

mod bounds;
mod cache;
mod svg;
#[cfg(test)]
mod tests;

use bounds::bounds_on_artboard;
pub use bounds::layer_bounds_mm;
pub use cache::{LayerPng, LayerSvg, RenderCache, TextInk};
pub use svg::svg_document;
use svg::{Href, build_svg};
pub(crate) use svg::{closed_path, esc, num};

#[derive(Clone, Debug)]
pub struct RenderOptions {
    pub scale_px_per_mm: f64,
    pub background: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayerRaster {
    pub id: LayerId,
    pub x_px: i32,
    pub y_px: i32,
    pub w_px: u32,
    pub h_px: u32,
    pub image: Option<RgbaImage>,
}

#[derive(Clone, Debug)]
pub struct SvgOptions {
    pub scale: f64,
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
        Href::File,
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
    if !layer.visible || doc.ancestors(layer).iter().any(|a| !a.visible) {
        return Ok(raster);
    }
    let b = bounds_on_artboard(doc, layer, Some(fonts));
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
        Href::File,
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
    doc.roots()
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
        Href::File,
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
        if opts.embed_images {
            Href::Embed
        } else {
            Href::File
        },
    )
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

pub(crate) fn px(mm: f64, scale: f64) -> u32 {
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
