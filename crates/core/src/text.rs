use std::path::Path;

use crate::model::TextProps;
use crate::Result;

pub struct Fonts {
    inner: std::sync::Mutex<cosmic_text::FontSystem>,
}

pub struct GlyphRun {
    pub path_d: String,
    pub bbox_mm: [f64; 4],
}

impl Fonts {
    pub fn new(bundled_dir: Option<&Path>) -> Self {
        todo!()
    }

    pub fn families(&self) -> Vec<String> {
        todo!()
    }

    pub fn has_family(&self, family: &str) -> bool {
        todo!()
    }

    pub fn layout_to_path(&self, props: &TextProps, box_w_mm: f64, box_h_mm: f64) -> Result<GlyphRun> {
        todo!()
    }

    pub fn measure(&self, props: &TextProps, box_w_mm: f64) -> Result<[f64; 2]> {
        todo!()
    }
}
