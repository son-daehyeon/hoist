use std::path::PathBuf;

use banner_core::model::*;
use banner_core::render::{RenderOptions, render_document};
use banner_core::store::ProjectStore;
use banner_core::text::Fonts;
use banner_core::{analyze, compare, export, image as cimg, trace};

fn main() -> anyhow::Result<()> {
    let root = std::env::temp_dir().join("banner-smoke");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root)?;
    let sample = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/samples/cheongjeon_original.png");
    let fonts = Fonts::new(Some(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/fonts"),
    ));
    let store = ProjectStore::new(root.clone());
    let mut h = store.create("스모크", 6000.0, 900.0)?;
    let orig = h.add_original(&sample)?;
    println!("original {}x{}", orig.width_px, orig.height_px);
    let img = cimg::load(&h.original_path(1))?;
    let pal = analyze::palette(&img, 8);
    println!("palette {:?}", pal.colors);
    let regions = analyze::segment_regions(&img);
    println!("regions {}", regions.len());
    for r in regions.iter().take(8) {
        println!("  {:?} {} {:?}", r.kind_hint, r.color, r.bbox_px);
    }
    let mm_per_px = 900.0 / orig.height_px as f64;
    let off_x = (6000.0 - orig.width_px as f64 * mm_per_px) / 2.0;

    let bg_asset = h.new_asset_name("background", "png");
    cimg::save_png(&img, &h.asset_path(&bg_asset))?;
    let mut ops = vec![];
    let group = |id: &str, name: &str| Layer {
        id: id.into(),
        name: name.into(),
        parent: None,
        kind: LayerKind::Group,
        x_mm: 0.0,
        y_mm: 0.0,
        w_mm: 6000.0,
        h_mm: 900.0,
        rotation_deg: 0.0,
        opacity: 1.0,
        visible: true,
        locked: false,
        fills: vec![],
        strokes: vec![],
        effects: vec![],
        strategy: Strategy::NativeVector,
        strategy_reason: "그룹".into(),
        pinned: vec![],
        source: None,
    };
    ops.push(Op::AddLayer {
        layer: group("G1", "배경"),
        index: None,
    });
    ops.push(Op::AddLayer {
        layer: group("G2", "그래픽"),
        index: None,
    });
    ops.push(Op::AddLayer {
        layer: group("G3", "글자"),
        index: None,
    });
    ops.push(Op::AddLayer {
        layer: Layer {
            id: "L1".into(),
            name: "원본 배경".into(),
            parent: Some("G1".into()),
            kind: LayerKind::Image(ImageProps {
                asset: bg_asset,
                natural_w_px: orig.width_px,
                natural_h_px: orig.height_px,
                fit: ImageFit::Fill,
            }),
            x_mm: off_x,
            y_mm: 0.0,
            w_mm: orig.width_px as f64 * mm_per_px,
            h_mm: 900.0,
            rotation_deg: 0.0,
            opacity: 1.0,
            visible: true,
            locked: false,
            fills: vec![],
            strokes: vec![],
            effects: vec![],
            strategy: Strategy::Background,
            strategy_reason: "테스트".into(),
            pinned: vec![],
            source: None,
        },
        index: None,
    });
    ops.push(Op::AddLayer {
        layer: Layer {
            id: "L2".into(),
            name: "띠".into(),
            parent: Some("G2".into()),
            kind: LayerKind::Shape(ShapeProps::Rect { rx_mm: 20.0 }),
            x_mm: 2200.0,
            y_mm: 560.0,
            w_mm: 1600.0,
            h_mm: 120.0,
            rotation_deg: 0.0,
            opacity: 1.0,
            visible: true,
            locked: false,
            fills: vec![Paint::Solid {
                color: "#6B4FA0".into(),
                opacity: 1.0,
            }],
            strokes: vec![Stroke {
                paint: Paint::Solid {
                    color: "#FFFFFF".into(),
                    opacity: 1.0,
                },
                width_mm: 8.0,
                align: StrokeAlign::Outside,
                join: StrokeJoin::Round,
                dash_mm: vec![],
            }],
            effects: vec![Effect::DropShadow {
                x_mm: 0.0,
                y_mm: 6.0,
                blur_mm: 12.0,
                spread_mm: 0.0,
                color: "#000000".into(),
                opacity: 0.35,
            }],
            strategy: Strategy::NativeVector,
            strategy_reason: "테스트".into(),
            pinned: vec![],
            source: None,
        },
        index: None,
    });
    ops.push(Op::AddLayer {
        layer: Layer {
            id: "L3".into(),
            name: "제목 글자".into(),
            parent: Some("G3".into()),
            kind: LayerKind::Text(TextProps {
                text: "청전동 주민역량강화".into(),
                font_family: "Pretendard".into(),
                font_weight: 800,
                font_size_mm: 220.0,
                letter_spacing_mm: -4.0,
                line_height: 1.1,
                align: TextAlign::Center,
            }),
            x_mm: 1500.0,
            y_mm: 180.0,
            w_mm: 3000.0,
            h_mm: 300.0,
            rotation_deg: 0.0,
            opacity: 1.0,
            visible: true,
            locked: false,
            fills: vec![Paint::Solid {
                color: "#1E3F7A".into(),
                opacity: 1.0,
            }],
            strokes: vec![Stroke {
                paint: Paint::Solid {
                    color: "#FFFFFF".into(),
                    opacity: 1.0,
                },
                width_mm: 10.0,
                align: StrokeAlign::Outside,
                join: StrokeJoin::Round,
                dash_mm: vec![],
            }],
            effects: vec![],
            strategy: Strategy::Text,
            strategy_reason: "테스트".into(),
            pinned: vec![],
            source: None,
        },
        index: None,
    });
    h.apply_ops(ops, Actor::User, None)?;

    let title_color = regions
        .iter()
        .find(|r| r.kind_hint == analyze::RegionKind::TextLike)
        .map(|r| r.color.clone())
        .unwrap_or("#1E3F7A".into());
    let mask = trace::color_mask(&img, &title_color, 40);
    let up = trace::upsample_mask(&mask, 4);
    let d = trace::trace_mask(
        &up,
        &trace::TraceOptions {
            mm_per_px: mm_per_px / 4.0,
            smooth: true,
            corner_threshold_deg: 60.0,
            min_area_px: 16,
        },
    )?;
    let back = trace::rasterize_path(&d, up.width(), up.height(), 4.0 / mm_per_px)?;
    println!(
        "trace color {} path {} chars iou {:.3}",
        title_color,
        d.len(),
        compare::mask_overlap(&up, &back)
    );
    h.apply_ops(
        vec![Op::AddLayer {
            layer: Layer {
                id: "L4".into(),
                name: "트레이싱".into(),
                parent: Some("G3".into()),
                kind: LayerKind::Path(PathProps {
                    d,
                    fill_rule: FillRule::Nonzero,
                }),
                x_mm: off_x,
                y_mm: 0.0,
                w_mm: orig.width_px as f64 * mm_per_px,
                h_mm: 900.0,
                rotation_deg: 0.0,
                opacity: 1.0,
                visible: true,
                locked: false,
                fills: vec![Paint::Solid {
                    color: "#D92D20".into(),
                    opacity: 0.6,
                }],
                strokes: vec![],
                effects: vec![],
                strategy: Strategy::TextTraced,
                strategy_reason: "테스트".into(),
                pinned: vec![],
                source: None,
            },
            index: None,
        }],
        Actor::User,
        None,
    )?;

    let scale = 1400.0 / 6000.0;
    let preview = render_document(
        &h.document,
        &h.assets_dir(),
        &fonts,
        &RenderOptions {
            scale_px_per_mm: scale,
            background: true,
        },
    )?;
    let out = root.join("preview.png");
    cimg::save_png(&preview, &out)?;
    println!(
        "preview {} {}x{}",
        out.display(),
        preview.width(),
        preview.height()
    );
    let layers = banner_core::render::render_layers(&h.document, &h.assets_dir(), &fonts, scale)?;
    for l in &layers {
        println!("layer {} at {},{} {}x{}", l.id, l.x_px, l.y_px, l.w_px, l.h_px);
    }
    let png = export::export_png(
        &h.document,
        &h.assets_dir(),
        &fonts,
        100.0,
        10,
        &root.join("export_1to10.png"),
    )?;
    println!(
        "png {:?} {}x{} {} bytes",
        png.path,
        png.width_px.unwrap_or(0),
        png.height_px.unwrap_or(0),
        png.bytes
    );
    let svg = export::export_svg(
        &h.document,
        &h.assets_dir(),
        &fonts,
        10,
        true,
        &root.join("export_1to10.svg"),
    )?;
    println!("svg {:?} {} bytes", svg.path, svg.bytes);
    let fitted = cimg::resize_to(&img, preview.width(), preview.height());
    let cmp = compare::compare(&fitted, &preview);
    println!(
        "compare sim {:.3} overlap {:.3} edge {:.3}",
        cmp.similarity, cmp.overlap, cmp.edge_similarity
    );
    let undone = h.undo()?.map(|e| e.len()).unwrap_or(0);
    println!("undo {} entries, layers now {}", undone, h.document.layers.len());
    let v = h.snapshot(Actor::User, "스모크", None, Some(&cimg::to_png_bytes(&preview)?))?;
    println!("version {:?}", v.seq);
    Ok(())
}
