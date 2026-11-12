use std::path::Path;

use i_overlay::core::fill_rule::FillRule as OverlayFill;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::simplify::SimplifyShape;
use i_overlay::float::single::SingleFloatOverlay;
use serde::{Deserialize, Serialize};
use tiny_skia::{PathSegment, Point, Transform};

use crate::model::*;
use crate::ops::diff;
use crate::render::{SvgOptions, closed_path, esc, num, to_svg};
use crate::text::Fonts;
use crate::{CoreError, Result};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum BooleanOp {
    Union,
    Subtract,
    Intersect,
    Exclude,
}

const FLATTEN_TOLERANCE_MM: f64 = 0.05;

type Contour = Vec<[f64; 2]>;
type Shapes = Vec<Vec<Contour>>;

pub fn path_boolean(doc: &Document, fonts: &Fonts, ids: &[LayerId], op: BooleanOp) -> Result<Vec<Op>> {
    let layers = selected_layers(doc, ids)?;
    let mut areas = Vec::new();
    for l in &layers {
        let area = filled_area(doc, l, fonts)?;
        if area.is_empty() {
            return Err(CoreError::invalid(format!("면이 없는 레이어예요: {}", l.name)));
        }
        areas.push(area);
    }
    let rule = match op {
        BooleanOp::Union => OverlayRule::Union,
        BooleanOp::Subtract => OverlayRule::Difference,
        BooleanOp::Intersect => OverlayRule::Intersect,
        BooleanOp::Exclude => OverlayRule::Xor,
    };
    let mut acc = areas[0].clone();
    for area in &areas[1..] {
        acc = acc.overlay(area, rule, OverlayFill::NonZero);
    }
    let first = layers[0];
    let contours = to_parent_space(doc, first, acc)?;
    let b = corners_of(&contours);
    if contours.is_empty() || b[2] <= b[0] || b[3] <= b[1] {
        return Err(CoreError::invalid("겹치는 면이 없어 결과가 비어 있어요"));
    }
    let d: String = contours
        .iter()
        .map(|c| closed_path(&c.iter().map(|p| (p[0] - b[0], p[1] - b[1])).collect::<Vec<_>>()))
        .collect();
    let mut merged = first.clone();
    merged.kind = LayerKind::Path(PathProps {
        d,
        fill_rule: FillRule::Evenodd,
    });
    (merged.x_mm, merged.y_mm, merged.w_mm, merged.h_mm) = (b[0], b[1], b[2] - b[0], b[3] - b[1]);
    merged.rotation_deg = 0.0;
    let mut ops = vec![update_op(first, &merged)?];
    ops.extend(layers[1..].iter().map(|l| Op::DeleteLayer { id: l.id.clone() }));
    Ok(ops)
}

fn selected_layers<'a>(doc: &'a Document, ids: &[LayerId]) -> Result<Vec<&'a Layer>> {
    let mut layers: Vec<&Layer> = Vec::new();
    for id in ids {
        let l = doc
            .layer(id)
            .ok_or_else(|| CoreError::LayerNotFound(id.clone()))?;
        if !matches!(
            l.kind,
            LayerKind::Path(_) | LayerKind::Shape(_) | LayerKind::Text(_)
        ) {
            return Err(CoreError::invalid(format!(
                "패스·도형·글자만 합칠 수 있어요: {}",
                l.name
            )));
        }
        if !layers.iter().any(|x| x.id == l.id) {
            layers.push(l);
        }
    }
    if layers.len() < 2 {
        return Err(CoreError::invalid("레이어를 두 개 이상 골라 주세요"));
    }
    Ok(layers)
}

fn to_parent_space(doc: &Document, first: &Layer, shapes: Shapes) -> Result<Vec<Contour>> {
    let to_parent = doc
        .ancestors(first)
        .into_iter()
        .fold(Transform::identity(), |m, a| m.pre_concat(placement(a)))
        .invert()
        .ok_or_else(|| CoreError::invalid("상위 그룹 변형을 되돌릴 수 없어요"))?;
    Ok(shapes
        .into_iter()
        .flatten()
        .filter(|c| c.len() >= 3)
        .map(|c| c.into_iter().map(|p| transform_point(to_parent, p)).collect())
        .collect())
}

fn corners_of(contours: &[Contour]) -> [f64; 4] {
    let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for p in contours.iter().flatten() {
        b = [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])];
    }
    b
}

pub fn outline_text(doc: &Document, fonts: &Fonts, id: &str) -> Result<Vec<Op>> {
    let layer = doc
        .layer(id)
        .ok_or_else(|| CoreError::LayerNotFound(id.to_string()))?;
    if !matches!(layer.kind, LayerKind::Text(_)) {
        return Err(CoreError::invalid(format!(
            "글자 레이어가 아니에요: {}",
            layer.name
        )));
    }
    let d = write_d(
        rendered_paths(doc, layer, fonts, false)?
            .iter()
            .map(|(path, _)| path),
    );
    if d.is_empty() {
        return Err(CoreError::invalid(format!(
            "윤곽선으로 바꿀 글자가 없어요: {}",
            layer.name
        )));
    }
    let mut outlined = layer.clone();
    outlined.kind = LayerKind::Path(PathProps {
        d,
        fill_rule: FillRule::Nonzero,
    });
    outlined.strategy_reason = "글자 윤곽선화".into();
    Ok(vec![update_op(layer, &outlined)?])
}

fn write_d<'a>(paths: impl Iterator<Item = &'a tiny_skia::Path>) -> String {
    let mut d = String::new();
    for path in paths {
        for seg in path.segments() {
            let (cmd, pts): (char, &[Point]) = match &seg {
                PathSegment::MoveTo(p) => ('M', std::slice::from_ref(p)),
                PathSegment::LineTo(p) => ('L', std::slice::from_ref(p)),
                PathSegment::QuadTo(a, p) => ('Q', &[*a, *p]),
                PathSegment::CubicTo(a, b, p) => ('C', &[*a, *b, *p]),
                PathSegment::Close => ('Z', &[]),
            };
            d.push(cmd);
            for p in pts {
                d.push_str(&format!("{} {} ", num(f64::from(p.x)), num(f64::from(p.y))));
            }
        }
    }
    d
}

fn parse_d(d: &str) -> Result<Vec<tiny_skia::Path>> {
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><path d="{}"/></svg>"#,
        esc(d)
    );
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default())
        .map_err(|e| CoreError::Render(e.to_string()))?;
    let mut paths = Vec::new();
    collect_paths(tree.root(), Transform::identity(), &mut paths);
    if paths.is_empty() {
        return Err(CoreError::invalid("패스에 그릴 모양이 없어요"));
    }
    Ok(paths.into_iter().map(|(path, _)| path).collect())
}

fn transform_d(paths: Vec<tiny_skia::Path>, t: Transform) -> String {
    let moved: Vec<_> = paths.into_iter().filter_map(|p| p.transform(t)).collect();
    write_d(moved.iter())
}

/// `d` is drawn as written in the layer's own mm, so resizing the box must scale `d` too.
pub fn scale_path_d(d: &str, sx: f64, sy: f64) -> Result<String> {
    Ok(transform_d(
        parse_d(d)?,
        Transform::from_scale(sx as f32, sy as f32),
    ))
}

/// Moves and scales `d` so its outline covers exactly `0, 0, w_mm, h_mm`.
pub fn fit_path_d(d: &str, w_mm: f64, h_mm: f64) -> Result<String> {
    let paths = parse_d(d)?;
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for b in paths.iter().filter_map(|p| p.compute_tight_bounds()) {
        (x0, y0) = (x0.min(b.left()), y0.min(b.top()));
        (x1, y1) = (x1.max(b.right()), y1.max(b.bottom()));
    }
    if x0 > x1 {
        return Err(CoreError::invalid("패스에 그릴 모양이 없어요"));
    }
    let factor = |from: f32, to: f64| {
        if from > 0.0 && to > 0.0 {
            to as f32 / from
        } else {
            1.0
        }
    };
    let t = Transform::from_translate(-x0, -y0).post_scale(factor(x1 - x0, w_mm), factor(y1 - y0, h_mm));
    Ok(transform_d(paths, t))
}

fn update_op(old: &Layer, new: &Layer) -> Result<Op> {
    Ok(Op::UpdateLayer {
        id: old.id.clone(),
        patch: diff(&serde_json::to_value(old)?, &serde_json::to_value(new)?),
    })
}

fn placement(l: &Layer) -> Transform {
    Transform::from_translate(l.x_mm as f32, l.y_mm as f32).pre_concat(Transform::from_rotate_at(
        l.rotation_deg as f32,
        (l.w_mm / 2.0) as f32,
        (l.h_mm / 2.0) as f32,
    ))
}

fn transform_point(t: Transform, p: [f64; 2]) -> [f64; 2] {
    let (sx, kx, ky, sy) = (f64::from(t.sx), f64::from(t.kx), f64::from(t.ky), f64::from(t.sy));
    [
        sx * p[0] + kx * p[1] + f64::from(t.tx),
        ky * p[0] + sy * p[1] + f64::from(t.ty),
    ]
}

fn rendered_paths(
    doc: &Document,
    layer: &Layer,
    fonts: &Fonts,
    absolute: bool,
) -> Result<Vec<(tiny_skia::Path, bool)>> {
    let mut solo = layer.clone();
    solo.strokes.clear();
    solo.effects.clear();
    solo.opacity = 1.0;
    solo.visible = true;
    solo.fills = vec![Paint::Solid {
        color: "#000000".into(),
        opacity: 1.0,
    }];
    let mut layers: Vec<Layer> = Vec::new();
    if absolute {
        for a in doc.ancestors(layer) {
            let mut a = a.clone();
            a.visible = true;
            a.opacity = 1.0;
            a.effects.clear();
            layers.push(a);
        }
    } else {
        solo.parent = None;
        (solo.x_mm, solo.y_mm, solo.rotation_deg) = (0.0, 0.0, 0.0);
    }
    layers.push(solo);
    let tmp = Document {
        rev: doc.rev,
        artboard: Artboard {
            background: None,
            ..doc.artboard.clone()
        },
        layers,
    };
    let opts = SvgOptions {
        scale: 1.0,
        embed_images: false,
    };
    let svg = to_svg(&tmp, Path::new(""), fonts, &opts)?;
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default())
        .map_err(|e| CoreError::Render(e.to_string()))?;
    let size = tree.size();
    let back = Transform::from_scale(
        tmp.artboard.width_mm as f32 / size.width(),
        tmp.artboard.height_mm as f32 / size.height(),
    );
    let mut out = Vec::new();
    collect_paths(tree.root(), back, &mut out);
    Ok(out)
}

fn collect_paths(group: &usvg::Group, back: Transform, out: &mut Vec<(tiny_skia::Path, bool)>) {
    for node in group.children() {
        match node {
            usvg::Node::Group(g) => collect_paths(g, back, out),
            usvg::Node::Path(p) => {
                if let Some(fill) = p.fill()
                    && let Some(path) = p.data().clone().transform(p.abs_transform().post_concat(back))
                {
                    out.push((path, fill.rule() == usvg::FillRule::EvenOdd));
                }
            }
            _ => {}
        }
    }
}

fn filled_area(doc: &Document, layer: &Layer, fonts: &Fonts) -> Result<Shapes> {
    let mut area = Shapes::new();
    for (path, evenodd) in rendered_paths(doc, layer, fonts, true)? {
        let rule = if evenodd {
            OverlayFill::EvenOdd
        } else {
            OverlayFill::NonZero
        };
        let part = flatten(&path).simplify_shape(rule);
        area = if area.is_empty() {
            part
        } else {
            area.overlay(&part, OverlayRule::Union, OverlayFill::NonZero)
        };
    }
    Ok(area)
}

fn flatten(path: &tiny_skia::Path) -> Vec<Contour> {
    let pt = |p: Point| [f64::from(p.x), f64::from(p.y)];
    let mut out = Vec::new();
    let mut cur: Contour = Vec::new();
    let mut last = [0.0, 0.0];
    for seg in path.segments() {
        match seg {
            PathSegment::MoveTo(p) => {
                finish_contour(&mut out, &mut cur);
                last = pt(p);
                cur.push(last);
            }
            PathSegment::LineTo(p) => {
                last = pt(p);
                cur.push(last);
            }
            PathSegment::QuadTo(a, p) => {
                let ctrl = [last, pt(a), pt(p)];
                let n = steps(second_diff(&ctrl) / 4.0);
                for i in 1..=n {
                    cur.push(bezier(&ctrl, i as f64 / n as f64));
                }
                last = ctrl[2];
            }
            PathSegment::CubicTo(a, b, p) => {
                let ctrl = [last, pt(a), pt(b), pt(p)];
                let n = steps(second_diff(&ctrl) * 3.0 / 4.0);
                for i in 1..=n {
                    cur.push(bezier(&ctrl, i as f64 / n as f64));
                }
                last = ctrl[3];
            }
            PathSegment::Close => finish_contour(&mut out, &mut cur),
        }
    }
    finish_contour(&mut out, &mut cur);
    out
}

fn finish_contour(out: &mut Vec<Contour>, cur: &mut Contour) {
    if cur.len() >= 3 {
        out.push(std::mem::take(cur));
    } else {
        cur.clear();
    }
}

fn second_diff(ctrl: &[[f64; 2]]) -> f64 {
    ctrl.windows(3)
        .map(|w| (w[0][0] - 2.0 * w[1][0] + w[2][0]).hypot(w[0][1] - 2.0 * w[1][1] + w[2][1]))
        .fold(0.0, f64::max)
}

/// Wang's formula: segments needed to stay within FLATTEN_TOLERANCE_MM.
fn steps(wang: f64) -> usize {
    (wang / FLATTEN_TOLERANCE_MM).sqrt().ceil().clamp(1.0, 1000.0) as usize
}

fn bezier(ctrl: &[[f64; 2]], t: f64) -> [f64; 2] {
    let mut pts = ctrl.to_vec();
    while pts.len() > 1 {
        pts = pts
            .windows(2)
            .map(|w| {
                [
                    w[0][0] + (w[1][0] - w[0][0]) * t,
                    w[0][1] + (w[1][1] - w[0][1]) * t,
                ]
            })
            .collect();
    }
    pts[0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{fonts, rect, shape_layer};
    use crate::ops::apply_op;
    use crate::render::{RenderOptions, layer_bounds_mm, render_document};

    fn run(doc: &mut Document, ops: Vec<Op>) {
        for op in ops {
            apply_op(doc, &op).expect("apply");
        }
    }

    fn area(doc: &Document, id: &str) -> f64 {
        let l = doc.layer(id).expect("layer");
        filled_area(doc, l, fonts())
            .expect("area")
            .iter()
            .flatten()
            .map(|c| {
                let n = c.len();
                (0..n)
                    .map(|i| c[i][0] * c[(i + 1) % n][1] - c[(i + 1) % n][0] * c[i][1])
                    .sum::<f64>()
                    / 2.0
            })
            .sum::<f64>()
            .abs()
    }

    fn two_rects() -> Document {
        let mut doc = Document::new(100.0, 100.0);
        doc.layers.push(rect("A", [10.0, 10.0, 20.0, 20.0], "#FF0000"));
        doc.layers.push(rect("B", [20.0, 20.0, 20.0, 20.0], "#0000FF"));
        doc
    }

    #[test]
    fn union_of_two_rects_has_combined_area() {
        let mut doc = two_rects();
        let ops = path_boolean(&doc, fonts(), &["A".into(), "B".into()], BooleanOp::Union).expect("union");
        run(&mut doc, ops);
        assert_eq!(doc.layers.len(), 1);
        let a = doc.layer("A").expect("A");
        assert!(matches!(a.kind, LayerKind::Path(_)));
        assert_eq!(a.fills, rect("x", [0.0; 4], "#FF0000").fills);
        assert_eq!([a.x_mm, a.y_mm, a.w_mm, a.h_mm], [10.0, 10.0, 30.0, 30.0]);
        assert!((area(&doc, "A") - 700.0).abs() < 0.5, "{}", area(&doc, "A"));
    }

    #[test]
    fn subtract_cuts_an_evenodd_hole() {
        let mut doc = Document::new(60.0, 60.0);
        doc.layers.push(rect("A", [10.0, 10.0, 40.0, 40.0], "#000000"));
        doc.layers.push(rect("B", [20.0, 20.0, 20.0, 20.0], "#0000FF"));
        let ops =
            path_boolean(&doc, fonts(), &["A".into(), "B".into()], BooleanOp::Subtract).expect("subtract");
        run(&mut doc, ops);
        let LayerKind::Path(p) = &doc.layer("A").expect("A").kind else {
            panic!("not a path");
        };
        assert_eq!(p.fill_rule, FillRule::Evenodd);
        assert!((area(&doc, "A") - 1200.0).abs() < 0.5);
        let opts = RenderOptions {
            scale_px_per_mm: 1.0,
            background: true,
        };
        let img = render_document(&doc, Path::new(""), fonts(), &opts).expect("render");
        assert_eq!(img.get_pixel(30, 30).0, [255, 255, 255, 255]);
        assert_eq!(img.get_pixel(15, 15).0, [0, 0, 0, 255]);
    }

    #[test]
    fn intersect_and_exclude() {
        let mut doc = two_rects();
        let ops =
            path_boolean(&doc, fonts(), &["A".into(), "B".into()], BooleanOp::Intersect).expect("intersect");
        run(&mut doc, ops);
        assert!((area(&doc, "A") - 100.0).abs() < 0.5);
        let mut doc = two_rects();
        let ops =
            path_boolean(&doc, fonts(), &["A".into(), "B".into()], BooleanOp::Exclude).expect("exclude");
        run(&mut doc, ops);
        assert!((area(&doc, "A") - 600.0).abs() < 0.5);
        let far = {
            let mut d = Document::new(100.0, 100.0);
            d.layers.push(rect("A", [0.0, 0.0, 10.0, 10.0], "#000000"));
            d.layers.push(rect("B", [50.0, 50.0, 10.0, 10.0], "#000000"));
            d
        };
        assert!(path_boolean(&far, fonts(), &["A".into(), "B".into()], BooleanOp::Intersect).is_err());
    }

    #[test]
    fn result_lands_in_the_first_layers_group_space() {
        let mut doc = Document::new(200.0, 200.0);
        let mut g = rect("G", [100.0, 50.0, 40.0, 40.0], "#000000");
        g.kind = LayerKind::Group;
        g.rotation_deg = 90.0;
        let mut a = rect("A", [0.0, 0.0, 20.0, 10.0], "#000000");
        a.parent = Some("G".into());
        doc.layers.push(g);
        doc.layers.push(a);
        let mut ellipse = shape_layer(
            "E",
            LayerKind::Shape(ShapeProps::Ellipse),
            [0.0, 0.0, 10.0, 10.0],
            "#000000",
        );
        ellipse.x_mm = 150.0;
        doc.layers.push(ellipse);
        let before = layer_bounds_mm(&doc, doc.layer("A").expect("A"));
        let ops = path_boolean(&doc, fonts(), &["A".into(), "E".into()], BooleanOp::Union).expect("union");
        run(&mut doc, ops);
        let a = doc.layer("A").expect("A");
        assert_eq!(a.parent.as_deref(), Some("G"));
        let after = layer_bounds_mm(&doc, a);
        assert!((after[0] - before[0]).abs() < 0.01, "{before:?} {after:?}");
        assert!((after[1] - 0.0).abs() < 0.01, "{after:?}");
        assert!((area(&doc, "A") - (200.0 + std::f64::consts::PI * 25.0)).abs() < 1.0);
    }

    #[test]
    fn a_hand_written_path_is_fitted_to_its_box_and_follows_a_resize() {
        let bounds = |d: &str| {
            let b = parse_d(d).expect("path")[0]
                .compute_tight_bounds()
                .expect("bounds");
            [b.left(), b.top(), b.right(), b.bottom()].map(|v| (f64::from(v) * 10.0).round() / 10.0)
        };
        let band = "M 100 38 L 1530 5 L 1530 81 L 100 61 Z";
        let fitted = fit_path_d(band, 850.0, 105.0).expect("fit");
        assert_eq!(bounds(&fitted), [0.0, 0.0, 850.0, 105.0]);
        let resized = scale_path_d(&fitted, 0.5, 2.0).expect("scale");
        assert_eq!(bounds(&resized), [0.0, 0.0, 425.0, 210.0]);
        let flat = fit_path_d("M 0 7 L 40 7", 200.0, 10.0).expect("a flat line keeps its thickness axis");
        assert_eq!(bounds(&flat), [0.0, 0.0, 200.0, 0.0]);
        assert!(fit_path_d("", 10.0, 10.0).is_err());
    }

    #[test]
    fn text_becomes_an_identical_path() {
        let f = fonts();
        if f.families().is_empty() {
            return;
        }
        let mut doc = Document::new(100.0, 30.0);
        let props = TextProps {
            text: "윤곽 Ag".into(),
            font_family: "No Such Font".into(),
            font_weight: 700,
            font_size_mm: 14.0,
            letter_spacing_mm: 0.0,
            line_height: 1.2,
            align: TextAlign::Center,
            scale_x: 0.8,
            skew_deg: 10.0,
            ..Default::default()
        };
        let mut t = shape_layer("T", LayerKind::Text(props), [5.0, 5.0, 90.0, 20.0], "#000000");
        t.rotation_deg = 8.0;
        doc.layers.push(t);
        let opts = RenderOptions {
            scale_px_per_mm: 4.0,
            background: true,
        };
        let before = render_document(&doc, Path::new(""), f, &opts).expect("render");
        let ops = outline_text(&doc, f, "T").expect("outline");
        run(&mut doc, ops);
        let t = doc.layer("T").expect("T");
        assert!(matches!(t.kind, LayerKind::Path(_)));
        assert_eq!(t.strategy_reason, "글자 윤곽선화");
        let after = render_document(&doc, Path::new(""), f, &opts).expect("render");
        let differ = before
            .pixels()
            .zip(after.pixels())
            .filter(|(a, b)| a.0.iter().zip(b.0.iter()).any(|(x, y)| x.abs_diff(*y) > 8))
            .count();
        assert!(differ < 20, "{differ} pixels differ");
    }
}
