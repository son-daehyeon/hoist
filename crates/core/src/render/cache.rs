use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;

use image::RgbaImage;

use super::bounds::{placed_bounds, to_xywh};
use super::svg::{BACKGROUND_ID, Href, SvgBuilder};
use super::{RenderOptions, render_document, render_layer};
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
    pub inks: Vec<TextInk>,
}

/// Glyph extent of one text layer as fractions `[x, y, w, h]` of its own box.
#[derive(Clone, Debug, PartialEq)]
pub struct TextInk {
    pub id: LayerId,
    pub frac: [f64; 4],
}

fn text_inks(doc: &Document, root: &str, fonts: &Fonts) -> Vec<TextInk> {
    crate::ops::subtree(&doc.layers, root)
        .iter()
        .filter_map(|id| doc.layer(id))
        .filter_map(|l| {
            let LayerKind::Text(t) = &l.kind else {
                return None;
            };
            let [x, y, w, h] = fonts.layout_to_path(t, l.w_mm, l.h_mm).ok()?.bbox_mm;
            (w > 0.0 && h > 0.0 && l.w_mm > 0.0 && l.h_mm > 0.0).then(|| TextInk {
                id: l.id.clone(),
                frac: [x / l.w_mm, y / l.h_mm, w / l.w_mm, h / l.h_mm],
            })
        })
        .collect()
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

fn artboard_json(doc: &Document) -> Result<String> {
    serde_json::to_string(&doc.artboard).map_err(CoreError::Json)
}

fn layer_png(id: &str, key: u64, image: &RgbaImage, x_px: i32, y_px: i32) -> Result<LayerPng> {
    Ok(LayerPng {
        id: id.into(),
        key,
        png_base64: crate::image::to_base64_png(image)?,
        x_px,
        y_px,
        w_px: image.width(),
        h_px: image.height(),
    })
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
        let artboard = artboard_json(doc)?;
        let mut out = Vec::new();
        let mut hits = 0;
        for root in doc.roots().into_iter().filter(|l| l.visible) {
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
                        Some(im) => Some(layer_png(&r.id, key, im, r.x_px, r.y_px)?),
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
        artboard_json(doc)?.hash(&mut hasher);
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
        let png = layer_png(BACKGROUND_ID, key, &bg, 0, 0)?;
        self.rasters
            .put(key, Some(png.clone()), png.png_base64.len() + 64);
        Ok(png)
    }

    pub fn layer_svgs(&mut self, doc: &Document, fonts: &Fonts) -> Result<(Vec<LayerSvg>, usize)> {
        let mut out = Vec::new();
        let mut hits = 0;
        if doc.artboard.background.is_some() {
            let mut hasher = DefaultHasher::new();
            BACKGROUND_ID.hash(&mut hasher);
            artboard_json(doc)?.hash(&mut hasher);
            let key = hasher.finish();
            let svg = SvgBuilder::new(doc, Path::new(""), fonts, Href::Asset, true)
                .background()
                .unwrap_or_default();
            out.push(LayerSvg {
                id: BACKGROUND_ID.into(),
                key,
                svg,
                bbox_mm: [0.0, 0.0, doc.artboard.width_mm, doc.artboard.height_mm],
                inks: vec![],
            });
        }
        for root in doc.roots() {
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
                        bbox_mm: to_xywh(r),
                        inks: text_inks(doc, &root.id, fonts),
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
