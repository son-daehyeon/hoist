use super::bounds::placed_bounds;
use super::svg::transform_attr;
use super::*;
use crate::fixtures::{fonts, rect, shape_layer, solid};

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
            embed_images: true,
        },
    )
    .expect("svg");
    assert!(svg.contains("width=\"10mm\"") && svg.contains("viewBox=\"0 0 100 30\""));
    assert!(!svg.contains("<text"));

    doc.layers.push(rect("r", [0.0, 0.0, 10.0, 10.0], "#FF0000"));
    let (svgs, _) = RenderCache::default().layer_svgs(&doc, f).expect("svgs");
    let of = |id: &str| &svgs.iter().find(|s| s.id == id).expect(id).inks;
    let ink = of("T");
    assert_eq!(ink.len(), 1);
    assert_eq!(ink[0].id, "T");
    let b = run.bbox_mm;
    assert_eq!(ink[0].frac, [b[0] / 90.0, b[1] / 20.0, b[2] / 90.0, b[3] / 20.0]);
    assert!(ink[0].frac[2] > 0.0 && ink[0].frac[3] > 0.0);
    assert!(of("r").is_empty());
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
