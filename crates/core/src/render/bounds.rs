use crate::model::*;
use crate::text::Fonts;

pub(super) type Corners = [f64; 4];

pub(super) fn to_xywh(c: Corners) -> [f64; 4] {
    [c[0], c[1], c[2] - c[0], c[3] - c[1]]
}

fn union(a: Corners, b: Corners) -> Corners {
    [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]
}

fn grow(b: Corners, m: f64) -> Corners {
    [b[0] - m, b[1] - m, b[2] + m, b[3] + m]
}

fn transform_box(l: &Layer, b: Corners) -> Corners {
    let (cx, cy) = (l.w_mm / 2.0, l.h_mm / 2.0);
    let (sin, cos) = l.rotation_deg.to_radians().sin_cos();
    let mut out = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for (x, y) in [(b[0], b[1]), (b[2], b[1]), (b[0], b[3]), (b[2], b[3])] {
        let (dx, dy) = (x - cx, y - cy);
        let px = l.x_mm + cx + dx * cos - dy * sin;
        let py = l.y_mm + cy + dx * sin + dy * cos;
        out = [out[0].min(px), out[1].min(py), out[2].max(px), out[3].max(py)];
    }
    out
}

pub(super) fn is_line(l: &Layer) -> bool {
    matches!(l.kind, LayerKind::Shape(ShapeProps::Line { .. }))
}

fn has_arrows(l: &Layer) -> bool {
    matches!(
        l.kind,
        LayerKind::Shape(ShapeProps::Line {
            arrow_start: true,
            ..
        }) | LayerKind::Shape(ShapeProps::Line { arrow_end: true, .. })
    )
}

pub(super) const ARROW_LENGTH_PER_WIDTH: f64 = 4.0;

fn stroke_extent(l: &Layer) -> f64 {
    l.strokes
        .iter()
        .map(|s| {
            let w = s.width_mm.max(0.0);
            let reach = match s.align {
                _ if has_arrows(l) => w * ARROW_LENGTH_PER_WIDTH / 2.0,
                _ if is_line(l) => w / 2.0,
                StrokeAlign::Center => w / 2.0,
                StrokeAlign::Outside => w,
                StrokeAlign::Inside => 0.0,
            };
            if s.join == StrokeJoin::Miter {
                reach * 2.0
            } else {
                reach
            }
        })
        .fold(0.0, f64::max)
}

fn local_bounds(doc: &Document, l: &Layer, fonts: Option<&Fonts>, depth: usize) -> Corners {
    let own = [0.0, 0.0, l.w_mm, l.h_mm];
    match &l.kind {
        LayerKind::Group => {
            if depth > doc.layers.len() {
                return own;
            }
            doc.children(Some(&l.id))
                .into_iter()
                .filter(|c| c.visible)
                .map(|c| placed_bounds(doc, c, fonts, depth + 1))
                .reduce(union)
                .unwrap_or(own)
        }
        kind => {
            let mut b = own;
            if let (LayerKind::Text(t), Some(f)) = (kind, fonts)
                && let Ok(run) = f.layout_to_path(t, l.w_mm, l.h_mm)
                && run.bbox_mm[2] > 0.0
            {
                let r = run.bbox_mm;
                b = union(b, [r[0], r[1], r[0] + r[2], r[1] + r[3]]);
            }
            grow(b, stroke_extent(l))
        }
    }
}

fn effect_bounds(l: &Layer, b: Corners) -> Corners {
    let mut out = b;
    let mut blur = 0.0f64;
    for e in &l.effects {
        match e {
            Effect::DropShadow {
                x_mm,
                y_mm,
                blur_mm,
                spread_mm,
                ..
            } => {
                let m = spread_mm.max(0.0) + 1.5 * blur_mm.max(0.0);
                out = union(
                    out,
                    [b[0] + x_mm - m, b[1] + y_mm - m, b[2] + x_mm + m, b[3] + y_mm + m],
                );
            }
            Effect::LayerBlur { radius_mm } => blur = blur.max(1.5 * radius_mm.max(0.0)),
        }
    }
    grow(out, blur)
}

pub(super) fn placed_bounds(doc: &Document, l: &Layer, fonts: Option<&Fonts>, depth: usize) -> Corners {
    effect_bounds(l, transform_box(l, local_bounds(doc, l, fonts, depth)))
}

pub(super) fn bounds_on_artboard(doc: &Document, layer: &Layer, fonts: Option<&Fonts>) -> Corners {
    let mut b = placed_bounds(doc, layer, fonts, 0);
    for a in doc.ancestors(layer).iter().rev() {
        b = transform_box(a, b);
    }
    b
}

pub fn layer_bounds_mm(doc: &Document, layer: &Layer) -> [f64; 4] {
    to_xywh(bounds_on_artboard(doc, layer, None))
}
