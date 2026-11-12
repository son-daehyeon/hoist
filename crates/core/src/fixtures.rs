use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::model::*;
use crate::ops::apply_op;
use crate::store::{ProjectHandle, ProjectStore};
use crate::text::Fonts;

pub(crate) fn bundled_fonts_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/fonts")
}

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
        fills: vec![solid(fill)],
        ..Layer::new(id, kind, rect)
    }
}

pub(crate) fn rect(id: &str, r: [f64; 4], fill: &str) -> Layer {
    shape_layer(id, LayerKind::Shape(ShapeProps::Rect { rx_mm: 0.0 }), r, fill)
}

pub(crate) fn layer(id: &str, name: &str, parent: Option<&str>, kind: LayerKind) -> Layer {
    Layer {
        name: name.into(),
        parent: parent.map(String::from),
        ..Layer::new(id, kind, [100.0, 10.0, 50.0, 20.0])
    }
}

pub(crate) fn text(t: &str) -> LayerKind {
    LayerKind::Text(TextProps {
        text: t.into(),
        font_family: "Pretendard".into(),
        font_weight: 700,
        font_size_mm: 10.0,
        letter_spacing_mm: 0.0,
        line_height: 1.2,
        align: TextAlign::Left,
        ..Default::default()
    })
}

pub(crate) fn sample() -> Document {
    let mut doc = Document::new(900.0, 300.0);
    for op in [
        Op::AddLayer {
            layer: Box::new(layer(
                "L1",
                "배경",
                None,
                LayerKind::Shape(ShapeProps::Rect { rx_mm: 0.0 }),
            )),
            index: None,
        },
        Op::AddLayer {
            layer: Box::new(layer("L2", "정보 띠", None, LayerKind::Group)),
            index: None,
        },
        Op::AddLayer {
            layer: Box::new(layer("L3", "청전동 주민역량강화", Some("L2"), text("청전동"))),
            index: None,
        },
        Op::AddLayer {
            layer: Box::new(layer("L4", "문인화", Some("L2"), LayerKind::Group)),
            index: None,
        },
        Op::AddLayer {
            layer: Box::new(layer(
                "L5",
                "잎",
                Some("L4"),
                LayerKind::Shape(ShapeProps::Ellipse),
            )),
            index: None,
        },
        Op::AddLayer {
            layer: Box::new(layer("L6", "제목", None, text("제목"))),
            index: None,
        },
    ] {
        apply_op(&mut doc, &op).expect("sample op");
    }
    doc
}

pub(crate) struct TempRoot(pub(crate) PathBuf);

impl TempRoot {
    pub(crate) fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("hoist-core-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(&dir).expect("temp dir");
        TempRoot(dir)
    }

    pub(crate) fn store(&self) -> ProjectStore {
        ProjectStore::new(self.0.clone())
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(crate) fn seeded(store: &ProjectStore) -> ProjectHandle {
    let mut h = store.create("청전동 현수막", 900.0, 300.0).expect("create");
    let ops = sample()
        .layers
        .into_iter()
        .map(|layer| Op::AddLayer {
            layer: Box::new(layer),
            index: None,
        })
        .collect();
    h.apply_ops(ops, Actor::Agent, Some("t1".into())).expect("seed");
    h
}
