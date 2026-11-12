#![allow(clippy::print_stdout)]

use std::path::{Path, PathBuf};
use std::time::Instant;

use hoist_core::image as cimg;
use hoist_core::model::*;
use hoist_core::render::{RenderCache, RenderOptions, render_document, render_layers};
use hoist_core::store::ProjectStore;
use hoist_core::text::Fonts;

const SCALE: f64 = 0.5;

fn layer(id: &str, parent: Option<&str>, kind: LayerKind, r: [f64; 4], fill: &str) -> Layer {
    Layer {
        parent: parent.map(str::to_string),
        fills: vec![Paint::Solid {
            color: fill.into(),
            opacity: 1.0,
        }],
        ..Layer::new(id, kind, r)
    }
}

fn text(s: &str, size: f64) -> LayerKind {
    LayerKind::Text(TextProps {
        text: s.into(),
        font_family: "Pretendard".into(),
        font_weight: 800,
        font_size_mm: size,
        letter_spacing_mm: 0.0,
        line_height: 1.2,
        align: TextAlign::Left,
        ..Default::default()
    })
}

fn layers(bg_asset: &str, grouped: bool) -> Vec<Layer> {
    let g = |name: &'static str| grouped.then_some(name);
    let mut out = Vec::new();
    if grouped {
        for name in ["배경", "그래픽", "글자"] {
            out.push(layer(
                name,
                None,
                LayerKind::Group,
                [0.0, 0.0, 6000.0, 900.0],
                "#000000",
            ));
        }
    }
    let image = LayerKind::Image(ImageProps {
        asset: bg_asset.into(),
        natural_w_px: 1774,
        natural_h_px: 470,
        fit: ImageFit::Cover,
        ..Default::default()
    });
    out.push(layer(
        "bg",
        g("배경"),
        image,
        [0.0, 0.0, 6000.0, 900.0],
        "#FFFFFF",
    ));
    let rect = || LayerKind::Shape(ShapeProps::Rect { rx_mm: 40.0 });
    out.push(layer(
        "band",
        g("그래픽"),
        rect(),
        [300.0, 380.0, 5400.0, 220.0],
        "#8E2EA3",
    ));
    out.push(layer(
        "chip",
        g("그래픽"),
        rect(),
        [500.0, 410.0, 900.0, 160.0],
        "#FFFFFF",
    ));
    out.push(layer(
        "box",
        g("그래픽"),
        rect(),
        [200.0, 640.0, 5600.0, 230.0],
        "#F8FBF9",
    ));
    let lines = [
        (
            "t1",
            "2026년 하반기",
            [2200.0, 30.0, 1800.0, 160.0],
            140.0,
            "#054EA1",
        ),
        (
            "t2",
            "청전동 주민역량강화 참여자 모집",
            [700.0, 190.0, 4600.0, 200.0],
            170.0,
            "#1C6B33",
        ),
        (
            "t3",
            "건강한 일상, 함께하는 청전동",
            [1600.0, 420.0, 3600.0, 150.0],
            120.0,
            "#EFE89C",
        ),
        (
            "t4",
            "교육내용 · 교육일정 · 교육장소",
            [300.0, 680.0, 5400.0, 90.0],
            70.0,
            "#222222",
        ),
    ];
    for (id, s, r, size, color) in lines {
        out.push(layer(id, g("글자"), text(s, size), r, color));
    }
    while out.len() < 11 {
        let n = out.len();
        out.push(layer(
            &format!("extra{n}"),
            None,
            rect(),
            [100.0 * n as f64, 20.0, 80.0, 80.0],
            "#F2511B",
        ));
    }
    out
}

fn uncached(doc: &Document, assets: &Path, fonts: &Fonts) -> anyhow::Result<usize> {
    let mut bare = doc.clone();
    bare.layers.clear();
    let bg = render_document(
        &bare,
        assets,
        fonts,
        &RenderOptions {
            scale_px_per_mm: SCALE,
            background: true,
        },
    )?;
    let mut n = cimg::to_base64_png(&bg)?.len();
    for r in render_layers(doc, assets, fonts, SCALE)? {
        if let Some(im) = &r.image {
            n += cimg::to_base64_png(im)?.len();
        }
    }
    Ok(n)
}

fn ms(t: Instant) -> u128 {
    t.elapsed().as_millis()
}

fn svg_bench(fonts: &Fonts) -> anyhow::Result<()> {
    let mut doc = Document::new(6000.0, 900.0);
    let traced: String = (0..2000)
        .map(|i| format!("L {} {} ", i % 300, (i * 7) % 200))
        .collect();
    for i in 0..100 {
        let kind = match i % 4 {
            0 => text("청전동 주민역량강화", 40.0),
            1 => LayerKind::Path(PathProps {
                d: format!("M 0 0 {traced}Z"),
                fill_rule: FillRule::Nonzero,
            }),
            2 => LayerKind::Shape(ShapeProps::Star {
                points: 5,
                inner_ratio: 0.5,
            }),
            _ => LayerKind::Shape(ShapeProps::Rect { rx_mm: 4.0 }),
        };
        let r = [60.0 * f64::from(i), 100.0, 300.0, 60.0];
        doc.layers.push(layer(&format!("L{i}"), None, kind, r, "#1C6B33"));
    }
    let mut cache = RenderCache::default();
    let t = Instant::now();
    let (first, _) = cache.layer_svgs(&doc, fonts)?;
    let cold = ms(t);
    let kb = first.iter().map(|s| s.svg.len()).sum::<usize>() / 1024;
    doc.layers[40].x_mm += 5.0;
    let t = Instant::now();
    let (_, hits) = cache.layer_svgs(&doc, fonts)?;
    let warm = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "svg fragments: {} layers ({kb} KB) cold {cold} ms · one text layer moved {warm:.1} ms ({hits}/100 reused)",
        doc.layers.len()
    );
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let root = std::env::temp_dir().join("hoist-render-bench");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root)?;
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fonts = Fonts::new(Some(&manifest.join("../../src-tauri/fonts")));
    svg_bench(&fonts)?;
    let store = ProjectStore::new(root.clone());
    for grouped in [false, true] {
        let mut h = store.create("bench", 6000.0, 900.0)?;
        h.add_original(&manifest.join("../../docs/samples/cheongjeon_original.png"))?;
        let bg_asset = h.new_asset_name("background", "png");
        std::fs::copy(h.original_path(1), h.asset_path(&bg_asset))?;
        let ops: Vec<Op> = layers(&bg_asset, grouped)
            .into_iter()
            .map(|l| Op::AddLayer {
                layer: Box::new(l),
                index: None,
            })
            .collect();
        h.apply_ops(ops, Actor::User, None)?;
        let assets = h.assets_dir();
        let roots = h.document.layers.iter().filter(|l| l.parent.is_none()).count();

        let t = Instant::now();
        uncached(&h.document, &assets, &fonts)?;
        let before = ms(t);

        let mut cache = RenderCache::default();
        let t = Instant::now();
        cache.background(&h.document, &assets, &fonts, SCALE)?;
        cache.layers(&h.document, &assets, &fonts, SCALE)?;
        let cold = ms(t);

        h.apply_ops(
            vec![Op::UpdateLayer {
                id: "t3".into(),
                patch: serde_json::json!({"x_mm": 1650.0}),
            }],
            Actor::User,
            None,
        )?;
        let t = Instant::now();
        uncached(&h.document, &assets, &fonts)?;
        let before_move = ms(t);
        let t = Instant::now();
        cache.background(&h.document, &assets, &fonts, SCALE)?;
        let (_, hits) = cache.layers(&h.document, &assets, &fonts, SCALE)?;
        let after_move = ms(t);
        println!(
            "{} layers ({} roots, {}): uncached {before} ms · cache cold {cold} ms · move one layer: uncached {before_move} ms → cached {after_move} ms ({hits}/{roots} roots reused)",
            h.document.layers.len(),
            roots,
            if grouped { "grouped" } else { "flat" },
        );
    }
    Ok(())
}
