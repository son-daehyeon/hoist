#![allow(clippy::print_stdout)]

use std::path::PathBuf;

use hoist_core::image as cimg;
use hoist_core::model::*;
use hoist_core::render::{RenderOptions, render_document};
use hoist_core::text::Fonts;

fn layer(id: &str, kind: LayerKind, r: [f64; 4], fill: Option<&str>) -> Layer {
    Layer {
        id: id.into(),
        name: id.into(),
        parent: None,
        kind,
        x_mm: r[0],
        y_mm: r[1],
        w_mm: r[2],
        h_mm: r[3],
        rotation_deg: 0.0,
        opacity: 1.0,
        visible: true,
        locked: false,
        fills: fill
            .map(|c| {
                vec![Paint::Solid {
                    color: c.into(),
                    opacity: 1.0,
                }]
            })
            .unwrap_or_default(),
        strokes: vec![],
        effects: vec![],
        strategy: Strategy::NativeVector,
        strategy_reason: String::new(),
        pinned: vec![],
        source: None,
    }
}

fn text(s: &str, tweak: impl FnOnce(&mut TextProps)) -> LayerKind {
    let mut t = TextProps {
        text: s.into(),
        font_family: "Pretendard".into(),
        font_weight: 800,
        font_size_mm: 14.0,
        ..Default::default()
    };
    tweak(&mut t);
    LayerKind::Text(t)
}

fn main() -> anyhow::Result<()> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fonts = Fonts::new(Some(&manifest.join("../../src-tauri/fonts")));
    let mut doc = Document::new(300.0, 160.0);
    let mut arrow = layer(
        "arrow",
        LayerKind::Shape(ShapeProps::Line {
            arrow_start: true,
            arrow_end: true,
        }),
        [10.0, 140.0, 120.0, 10.0],
        None,
    );
    arrow.strokes.push(Stroke {
        paint: Paint::Solid {
            color: "#222222".into(),
            opacity: 1.0,
        },
        width_mm: 1.5,
        align: StrokeAlign::Center,
        join: StrokeJoin::Round,
        dash_mm: vec![],
    });
    doc.layers = vec![
        layer(
            "plain",
            text("청전동 주민", |_| {}),
            [10.0, 5.0, 130.0, 20.0],
            Some("#054EA1"),
        ),
        layer(
            "narrow",
            text("청전동 주민", |t| t.scale_x = 0.6),
            [10.0, 28.0, 130.0, 20.0],
            Some("#1C6B33"),
        ),
        layer(
            "skew",
            text("청전동 주민", |t| t.skew_deg = 15.0),
            [10.0, 51.0, 130.0, 20.0],
            Some("#F2511B"),
        ),
        layer(
            "arch",
            text("참여자 모집 안내", |t| {
                t.arc_bend = 0.5;
                t.align = TextAlign::Center;
            }),
            [150.0, 40.0, 140.0, 20.0],
            Some("#8E2EA3"),
        ),
        layer(
            "valley",
            text("참여자 모집 안내", |t| {
                t.arc_bend = -0.5;
                t.align = TextAlign::Center;
            }),
            [150.0, 80.0, 140.0, 20.0],
            Some("#8E2EA3"),
        ),
        layer(
            "vertical",
            text("세로쓰기", |t| t.vertical = true),
            [130.0, 80.0, 18.0, 70.0],
            Some("#222222"),
        ),
        layer(
            "hexagon",
            LayerKind::Shape(ShapeProps::Polygon { sides: 6 }),
            [10.0, 80.0, 50.0, 50.0],
            Some("#EFE89C"),
        ),
        layer(
            "star",
            LayerKind::Shape(ShapeProps::Star {
                points: 5,
                inner_ratio: 0.45,
            }),
            [70.0, 80.0, 50.0, 50.0],
            Some("#F2511B"),
        ),
        arrow,
    ];
    let img = render_document(
        &doc,
        &std::env::temp_dir(),
        &fonts,
        &RenderOptions {
            scale_px_per_mm: 4.0,
            background: true,
        },
    )?;
    let out = std::env::temp_dir().join("hoist-features.png");
    cimg::save_png(&img, &out)?;
    println!("wrote {}", out.display());
    Ok(())
}
