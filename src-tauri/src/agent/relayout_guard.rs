use std::collections::BTreeMap;

use hoist_core::model::{Document, Layer, LayerKind, ShapeProps, TextProps};
use hoist_core::store::ProjectHandle;
use hoist_core::text::Fonts;

use super::tools;

const FULL_BLEED: f64 = 0.95;
const CORNER_OVERLAP: f64 = 0.15;
const ROW_GAP_MIN: f64 = 0.025;
const ROW_GAP_TARGET: f64 = 0.04;

/// Sizes are read on ink (the real glyph extent), not on layer boxes; empty space is part of the layout.
pub(super) fn relayout_guard_message(h: &ProjectHandle, fonts: &Fonts) -> Option<String> {
    let texts = inked_texts(&h.document, fonts);
    let scene = Scene::new(&h.document, &texts)?;
    let mut problems = Vec::new();
    title_problems(&scene, &mut problems);
    tall_panel_problems(&scene, &mut problems);
    band_problems(&scene, &mut problems);
    panel_problems(&scene, &mut problems);
    scale_x_problems(&scene, &mut problems);
    rim_problems(&scene, &mut problems);
    title_crowding_problems(&scene, &mut problems);
    row_gap_problems(&scene, &mut problems);
    if problems.is_empty() {
        return None;
    }
    Some(format!(
        "[guard] Relayout check failed: {}. Re-flow following <relayout>: keep the margins, gaps and padding, place title words and row items with arrange_row, size every band, pill and panel to its content with fit_box, then view_work to confirm.",
        problems.join("; ")
    ))
}

struct Text<'a> {
    layer: &'a Layer,
    props: &'a TextProps,
    ink: [f64; 4],
    lines: usize,
}

fn inked_texts<'a>(doc: &'a Document, fonts: &Fonts) -> Vec<Text<'a>> {
    doc.layers
        .iter()
        .filter(|l| l.visible)
        .filter_map(|layer| match &layer.kind {
            LayerKind::Text(props) if props.font_size_mm > 0.0 => {
                let (ink, lines) = tools::ink_mm(fonts, layer);
                Some(Text {
                    layer,
                    props,
                    ink,
                    lines,
                })
            }
            _ => None,
        })
        .collect()
}

struct Scene<'a> {
    width: f64,
    height: f64,
    layers: &'a [Layer],
    texts: &'a [Text<'a>],
    title: Vec<&'a Text<'a>>,
    body: Vec<&'a Text<'a>>,
    shapes: Vec<&'a Layer>,
    paths: Vec<&'a Layer>,
    panels: Vec<&'a Layer>,
    max_size: f64,
    title_box: [f64; 4],
    ink_h: f64,
}

impl<'a> Scene<'a> {
    fn new(doc: &'a Document, texts: &'a [Text<'a>]) -> Option<Self> {
        let height = doc.artboard.height_mm.max(1.0);
        let width = doc.artboard.width_mm.max(1.0);
        let max_size = texts.iter().map(|t| t.props.font_size_mm).fold(0.0, f64::max);
        if max_size <= 0.0 {
            return None;
        }
        let shapes: Vec<&Layer> = doc
            .layers
            .iter()
            .filter(|l| l.visible && matches!(l.kind, LayerKind::Shape(_)))
            .collect();
        let paths: Vec<&Layer> = doc
            .layers
            .iter()
            .filter(|l| l.visible && matches!(l.kind, LayerKind::Path(_)))
            .collect();
        let (title, body): (Vec<&Text>, Vec<&Text>) =
            texts.iter().partition(|t| t.props.font_size_mm >= max_size * 0.8);
        let title_box = tools::union_mm(title.iter().map(|t| t.ink))?;
        let ink_h = title.iter().map(|t| t.ink[3]).fold(0.0, f64::max);
        let panels: Vec<&Layer> = shapes.iter().copied().filter(|l| l.w_mm > width * 0.5).collect();
        Some(Scene {
            width,
            height,
            layers: &doc.layers,
            texts,
            title,
            body,
            shapes,
            paths,
            panels,
            max_size,
            title_box,
            ink_h,
        })
    }
}

fn rect(l: &Layer) -> [f64; 4] {
    [l.x_mm, l.y_mm, l.w_mm, l.h_mm]
}

fn inside(b: [f64; 4], c: [f64; 4]) -> bool {
    let (cx, cy) = (b[0] + b[2] / 2.0, b[1] + b[3] / 2.0);
    (c[0]..=c[0] + c[2]).contains(&cx) && (c[1]..=c[1] + c[3]).contains(&cy)
}

fn overlap_x(a: [f64; 4], b: [f64; 4]) -> f64 {
    (a[0] + a[2]).min(b[0] + b[2]) - a[0].max(b[0])
}

fn distance_y(a: [f64; 4], b: [f64; 4]) -> f64 {
    (b[1] - (a[1] + a[3])).max(a[1] - (b[1] + b[3]))
}

fn spread(v: &[f64]) -> f64 {
    v.iter().copied().fold(f64::MIN, f64::max) - v.iter().copied().fold(f64::MAX, f64::min)
}

fn is_divider(l: &Layer) -> bool {
    (l.rotation_deg.abs() < 1.0 && l.h_mm > l.w_mm * 3.0)
        || (l.rotation_deg.to_radians().sin().abs() > 0.9 && l.h_mm < l.w_mm * 0.34)
}

fn title_problems(scene: &Scene, problems: &mut Vec<String>) {
    let &Scene {
        width,
        height,
        max_size,
        title_box,
        ink_h,
        ..
    } = scene;
    let share = ink_h / height;
    if share > 0.32 {
        problems.push(format!(
            "the title's glyphs are {ink_h:.0} mm tall, {:.0}% of the banner height (target 25–30%): make the title smaller and give the height back to the margins and gaps",
            share * 100.0
        ));
    } else if share < 0.24 && title_box[2] < 0.9 * (width - 0.2 * height) {
        problems.push(format!(
            "the title's glyphs are {ink_h:.0} mm tall, only {:.0}% of the banner height, with width to spare (target 25–30%)",
            share * 100.0
        ));
    }
    let mut words = scene.title.clone();
    words.sort_by(|a, b| a.ink[0].total_cmp(&b.ink[0]));
    let gaps: Vec<f64> = words
        .windows(2)
        .map(|p| p[1].ink[0] - (p[0].ink[0] + p[0].ink[2]))
        .collect();
    let (gap_min, gap_max) = gaps
        .iter()
        .fold((f64::MAX, f64::MIN), |(lo, hi), g| (lo.min(*g), hi.max(*g)));
    if !gaps.is_empty() && gap_max > ink_h * 0.8 {
        problems.push(format!(
            "title words are {gap_max:.0} mm apart (keep about 0.3× the glyph height, {:.0} mm)",
            ink_h * 0.3
        ));
    }
    if gaps.len() >= 2 && gap_max - gap_min > ink_h * 0.08 {
        problems.push(format!(
            "title words are unevenly spaced (gaps {gap_min:.0} and {gap_max:.0} mm): call arrange_row on them with one gap of about 0.3× the glyph height"
        ));
    }
    let smallest = scene
        .texts
        .iter()
        .map(|t| t.props.font_size_mm)
        .fold(f64::MAX, f64::min);
    if smallest / height < 0.03 {
        problems.push(format!(
            "the smallest text is {smallest:.0} mm, under 3% of the height ({:.0} mm)",
            height * 0.03
        ));
    }
    for eyebrow in scene
        .body
        .iter()
        .filter(|t| t.ink[1] + t.ink[3] < title_box[1] && t.props.font_size_mm < max_size * 0.3)
    {
        problems.push(format!(
            "'{}' stands over the title at {:.0} mm, under 0.3× the title ({max_size:.0} mm): keep the original's step, an eyebrow is about half the title",
            eyebrow.layer.name, eyebrow.props.font_size_mm
        ));
    }
}

fn tall_panel_problems(scene: &Scene, problems: &mut Vec<String>) {
    for panel in scene.panels.iter().filter(|l| l.h_mm > scene.height * 0.33) {
        problems.push(format!(
            "'{}' is {:.0} mm tall, {:.0}% of the banner height: a box is its content plus padding (an info panel is about 26%)",
            panel.name,
            panel.h_mm,
            panel.h_mm / scene.height * 100.0
        ));
    }
}

fn band_problems(scene: &Scene, problems: &mut Vec<String>) {
    for band in &scene.shapes {
        let LayerKind::Shape(ShapeProps::Rect { rx_mm }) = band.kind else {
            continue;
        };
        if rx_mm < band.h_mm * 0.3 || band.w_mm < scene.width * 0.1 {
            continue;
        }
        let inks: Vec<[f64; 4]> = scene
            .body
            .iter()
            .filter(|t| inside(t.ink, rect(band)))
            .map(|t| t.ink)
            .collect();
        if inks.is_empty() {
            continue;
        }
        let inner = scene
            .shapes
            .iter()
            .filter(|other| other.id != band.id && other.w_mm < band.w_mm)
            .map(|other| rect(other))
            .filter(|b| inside(*b, rect(band)));
        let content = tools::union_mm(inks.into_iter().chain(inner)).map_or(0.0, |u| u[2]);
        if content < band.w_mm * 0.6 {
            problems.push(format!(
                "'{}' is {:.0} mm wide but its content is {content:.0} mm: fit_box it around its content",
                band.name, band.w_mm
            ));
        }
        band_row_problems(scene, band, problems);
    }
}

fn band_row_problems(scene: &Scene, band: &Layer, problems: &mut Vec<String>) {
    let pills: Vec<[f64; 4]> = scene
        .shapes
        .iter()
        .filter(|other| {
            other.id != band.id && other.w_mm < band.w_mm * 0.9 && inside(rect(other), rect(band))
        })
        .map(|other| rect(other))
        .filter(|b| scene.body.iter().any(|t| inside(t.ink, *b)))
        .collect();
    let parts: Vec<[f64; 4]> = pills
        .iter()
        .copied()
        .chain(
            scene
                .body
                .iter()
                .filter(|t| inside(t.ink, rect(band)) && !pills.iter().any(|p| inside(t.ink, *p)))
                .map(|t| t.ink),
        )
        .collect();
    if parts.len() <= 1 {
        return;
    }
    let mids: Vec<f64> = parts.iter().map(|p| p[1] + p[3] / 2.0).collect();
    let off = spread(&mids);
    if off > band.h_mm * 0.04 {
        problems.push(format!(
            "the parts of '{}' are {off:.0} mm off one center line: arrange_row them with align_y \"center\", then fit_box the band",
            band.name
        ));
    }
    let dividers: Vec<&&Layer> = scene
        .shapes
        .iter()
        .filter(|other| other.id != band.id && is_divider(other) && inside(rect(other), rect(band)))
        .collect();
    let font = scene
        .body
        .iter()
        .filter(|t| inside(t.ink, rect(band)))
        .map(|t| t.props.font_size_mm)
        .fold(0.0, f64::max);
    let mut row = parts.clone();
    row.sort_by(|a, b| a[0].total_cmp(&b[0]));
    for pair in row.windows(2) {
        let (from, to) = (pair[0][0] + pair[0][2], pair[1][0]);
        let divided = dividers
            .iter()
            .any(|d| (from..to).contains(&(d.x_mm + d.w_mm / 2.0)));
        if to - from > font * 1.2 && !divided {
            problems.push(format!(
                "'{}' has a {:.0} mm gap with nothing in it: put the original's divider at its center, or close it to about 0.6× the font size ({font:.0} mm)",
                band.name,
                to - from
            ));
        }
    }
    for divider in dividers {
        let cx = divider.x_mm + divider.w_mm / 2.0;
        let left = parts
            .iter()
            .map(|p| p[0] + p[2])
            .filter(|e| *e <= cx)
            .fold(f64::MIN, f64::max);
        let right = parts
            .iter()
            .map(|p| p[0])
            .filter(|e| *e >= cx)
            .fold(f64::MAX, f64::min);
        if left > f64::MIN && right < f64::MAX {
            let (a, b) = (cx - left, right - cx);
            if (a - b).abs() > a.max(b) * 0.25 {
                problems.push(format!(
                    "'{}' in '{}' is {a:.0} mm from its left part and {b:.0} mm from its right part: put it at the gap center arrange_row returns",
                    divider.name, band.name
                ));
            }
        }
    }
}

fn panel_problems(scene: &Scene, problems: &mut Vec<String>) {
    for panel in &scene.panels {
        let texts: Vec<&Text> = scene
            .body
            .iter()
            .copied()
            .filter(|t| inside(t.ink, rect(panel)))
            .collect();
        if texts.len() < 3 {
            continue;
        }
        for t in texts.iter().filter(|t| t.lines > 2) {
            problems.push(format!(
                "'{}' has {} lines: re-break the info columns to at most 2 lines",
                t.layer.name, t.lines
            ));
        }
        let badges = scene
            .shapes
            .iter()
            .filter(|other| other.id != panel.id && inside(rect(other), rect(panel)))
            .map(|other| rect(other))
            .filter(|b| texts.iter().any(|t| inside(t.ink, *b)));
        let Some(content) = tools::union_mm(texts.iter().map(|t| t.ink).chain(badges)) else {
            continue;
        };
        let mut sizes: Vec<f64> = texts.iter().map(|t| t.props.font_size_mm).collect();
        sizes.sort_by(f64::total_cmp);
        let body_size = sizes[sizes.len() / 2];
        padding_problem(panel, content, body_size, problems);
        let pills: Vec<&Layer> = scene
            .shapes
            .iter()
            .copied()
            .filter(|other| {
                other.id != panel.id
                    && other.w_mm < scene.width * 0.2
                    && inside(rect(other), rect(panel))
                    && texts.iter().any(|t| inside(t.ink, rect(other)))
            })
            .collect();
        let bodies: Vec<&Text> = texts
            .iter()
            .copied()
            .filter(|t| !pills.iter().any(|pill| inside(t.ink, rect(pill))))
            .collect();
        let columns: Vec<(&Layer, &Text)> = pills
            .iter()
            .filter_map(|pill| {
                bodies
                    .iter()
                    .filter(|t| t.ink[1] >= pill.y_mm + pill.h_mm * 0.5 && overlap_x(t.ink, rect(pill)) > 0.0)
                    .min_by(|a, b| a.ink[1].total_cmp(&b.ink[1]))
                    .map(|t| (*pill, *t))
            })
            .collect();
        badge_alignment_problems(&columns, body_size, problems);
        loose_divider_problem(scene, panel, &pills, &columns, body_size, problems);
        line_break_problems(&bodies, problems);
    }
}

fn padding_problem(panel: &Layer, content: [f64; 4], body_size: f64, problems: &mut Vec<String>) {
    let pads = [
        ("top", content[1] - panel.y_mm),
        ("bottom", panel.y_mm + panel.h_mm - (content[1] + content[3])),
        ("left", content[0] - panel.x_mm),
        ("right", panel.x_mm + panel.w_mm - (content[0] + content[2])),
    ];
    if let Some((side, pad)) = pads
        .into_iter()
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .filter(|(_, pad)| *pad < body_size * 0.4)
    {
        problems.push(format!(
            "'{}' leaves {pad:.0} mm between its content and its {side} edge: fit_box it with pad_y 0.6× and pad_x 1× the body size ({body_size:.0} mm)",
            panel.name
        ));
    }
}

fn badge_alignment_problems(columns: &[(&Layer, &Text)], body_size: f64, problems: &mut Vec<String>) {
    let astray = |(badge, body): &&(&Layer, &Text)| (body.ink[0] - badge.x_mm).abs() > body_size * 0.25;
    let off = columns.iter().filter(astray).count();
    if let Some((badge, body)) = columns.iter().find(astray) {
        problems.push(format!(
            "{off} badges do not start where their body starts ('{}' at {:.0} mm, its body at {:.0} mm): body align left and the badge's x on the body's ink left",
            badge.name, badge.x_mm, body.ink[0]
        ));
    }
    let badge_tops: Vec<f64> = columns.iter().map(|(badge, _)| badge.y_mm).collect();
    if badge_tops.len() > 1 && spread(&badge_tops) > body_size * 0.1 {
        problems.push("the info badges are not on one line: give them one y and height".into());
    }
    let body_tops: Vec<f64> = columns.iter().map(|(_, body)| body.ink[1]).collect();
    if body_tops.len() > 1 && spread(&body_tops) > body_size * 0.25 {
        problems.push("the info bodies do not share one top: give them one y".into());
    }
}

fn loose_divider_problem(
    scene: &Scene,
    panel: &Layer,
    pills: &[&Layer],
    columns: &[(&Layer, &Text)],
    body_size: f64,
    problems: &mut Vec<String>,
) {
    let lefts: Vec<f64> = columns
        .iter()
        .map(|(badge, body)| badge.x_mm.min(body.ink[0]))
        .collect();
    let loose = scene
        .shapes
        .iter()
        .filter(|other| {
            other.id != panel.id
                && is_divider(other)
                && inside(rect(other), rect(panel))
                && !pills.iter().any(|pill| pill.id == other.id)
        })
        .filter_map(|divider| {
            let cx = divider.x_mm + divider.w_mm / 2.0;
            lefts
                .iter()
                .copied()
                .filter(|l| *l > cx)
                .min_by(f64::total_cmp)
                .map(|l| l - cx)
        })
        .filter(|gap| *gap > body_size * 1.5)
        .count();
    if loose > 0 {
        problems.push(format!(
            "{loose} dividers stand away from the column on their right: a divider belongs to the column after it, put it 1× the body size ({body_size:.0} mm) left of that column's badge and body"
        ));
    }
}

fn line_break_problems(bodies: &[&Text], problems: &mut Vec<String>) {
    for t in bodies {
        let written: Vec<&str> = t.props.text.split('\n').collect();
        if t.lines > written.len() {
            problems.push(format!(
                "'{}' wrapped on its own into {} lines: write the breaks with \\n and make the box wider than the longest line",
                t.layer.name, t.lines
            ));
        }
        let last = written
            .last()
            .map_or(0, |l| l.chars().filter(|c| !c.is_whitespace()).count());
        if written.len() > 1 && last <= 1 {
            problems.push(format!(
                "'{}' ends in a one-character line: break between phrases instead",
                t.layer.name
            ));
        }
    }
}

/// Texts in one box play one role and share one fitted scale_x; a family elsewhere was fitted on its own.
fn scale_x_problems(scene: &Scene, problems: &mut Vec<String>) {
    let mut widened: BTreeMap<&str, (f64, f64, &str)> = BTreeMap::new();
    for shape in &scene.shapes {
        let mut ranges: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
        for t in scene.body.iter().filter(|t| inside(t.ink, rect(shape))) {
            let e = ranges
                .entry(t.props.font_family.as_str())
                .or_insert((f64::MAX, f64::MIN));
            *e = (e.0.min(t.props.scale_x), e.1.max(t.props.scale_x));
        }
        for (family, (lo, hi)) in ranges {
            if hi - lo > 0.05 && widened.get(family).is_none_or(|w| hi - lo > w.1 - w.0) {
                widened.insert(family, (lo, hi, shape.name.as_str()));
            }
        }
    }
    for (family, (lo, hi, name)) in widened {
        problems.push(format!(
            "texts in {family} inside '{name}' use different scale_x ({lo:.2}–{hi:.2}): texts that share a box use one fitted scale_x"
        ));
    }
}

/// The rim belongs to the hem and the rope eyelets: text and the boxes that carry it stay out of it.
/// Full-bleed bars and bare decorations (leaves, brush strokes without text) may reach the edge.
fn rim_problems(scene: &Scene, problems: &mut Vec<String>) {
    let &Scene { width, height, .. } = scene;
    let full_bleed: Vec<[f64; 4]> = scene
        .shapes
        .iter()
        .filter(|s| s.w_mm >= width * FULL_BLEED)
        .map(|s| rect(s))
        .collect();
    let carriers: Vec<(&str, [f64; 4])> = scene
        .texts
        .iter()
        .filter(|t| !full_bleed.iter().any(|f| inside(t.ink, *f)))
        .map(|t| (t.layer.name.as_str(), t.ink))
        .chain(
            scene
                .shapes
                .iter()
                .filter(|s| s.w_mm < width * FULL_BLEED)
                .map(|s| (s.name.as_str(), rect(s))),
        )
        .chain(
            scene
                .paths
                .iter()
                .filter(|p| scene.texts.iter().any(|t| inside(t.ink, rect(p))))
                .map(|p| (p.name.as_str(), rect(p))),
        )
        .collect();
    let nearest = |distance: &dyn Fn([f64; 4]) -> f64, limit: f64| {
        let close: Vec<(&str, f64)> = carriers
            .iter()
            .map(|(name, b)| (*name, distance(*b)))
            .filter(|(_, d)| *d < limit)
            .collect();
        close
            .iter()
            .copied()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(name, d)| (name, d, close.len() - 1))
    };
    let more = |n: usize| {
        if n > 0 {
            format!(" (and {n} more)")
        } else {
            String::new()
        }
    };
    for (edge, distance) in [
        ("top", &(|b: [f64; 4]| b[1]) as &dyn Fn([f64; 4]) -> f64),
        ("bottom", &|b: [f64; 4]| height - (b[1] + b[3])),
    ] {
        if let Some((name, d, n)) = nearest(distance, height * 0.04) {
            problems.push(format!(
                "'{name}' is {d:.0} mm from the {edge} edge{}: keep text and its boxes at least {:.0} mm (5% of the height) from the top and bottom",
                more(n),
                height * 0.05
            ));
        }
    }
    if let Some((name, d, n)) = nearest(&|b: [f64; 4]| b[0].min(width - (b[0] + b[2])), height * 0.08) {
        problems.push(format!(
            "'{name}' is {d:.0} mm from the banner's end{}: keep text and its boxes at least {:.0} mm (10% of the height) from the left and right ends",
            more(n),
            height * 0.10
        ));
    }
}

fn title_crowding_problems(scene: &Scene, problems: &mut Vec<String>) {
    let &Scene {
        width,
        height,
        title_box,
        ..
    } = scene;
    let neighbours = scene
        .body
        .iter()
        .map(|t| (t.layer.name.as_str(), t.ink, false))
        .chain(
            scene
                .layers
                .iter()
                .filter(|l| {
                    l.visible
                        && matches!(
                            l.kind,
                            LayerKind::Shape(_) | LayerKind::Path(_) | LayerKind::Image(_)
                        )
                        && l.w_mm >= width * 0.05
                        && l.w_mm < width * 0.9
                })
                .map(|l| (l.name.as_str(), rect(l), matches!(l.kind, LayerKind::Image(_)))),
        );
    let mut crowding = 0;
    for (name, bounds, picture) in neighbours {
        // A cut-out picture's loose box may clip a corner of the title without sitting on it.
        if overlap_x(title_box, bounds) <= bounds[2] * CORNER_OVERLAP {
            continue;
        }
        let d = distance_y(title_box, bounds);
        let limit = if picture {
            -height * 0.02
        } else {
            height * ROW_GAP_MIN
        };
        if d >= limit || crowding >= 3 {
            continue;
        }
        crowding += 1;
        problems.push(if d < 0.0 {
            format!(
                "'{name}' overlaps the title by {:.0} mm vertically: rows must not overlap, move it clear or shrink the band",
                -d
            )
        } else {
            format!(
                "'{name}' is only {d:.0} mm from the title's glyphs: leave about 4% of the height ({:.0} mm) between rows",
                height * ROW_GAP_TARGET
            )
        });
    }
}

fn row_gap_problems(scene: &Scene, problems: &mut Vec<String>) {
    let boxed: Vec<&Layer> = scene
        .shapes
        .iter()
        .copied()
        .filter(|s| s.w_mm >= scene.width * 0.1 && scene.body.iter().any(|t| inside(t.ink, rect(s))))
        .collect();
    for (i, a) in boxed.iter().enumerate() {
        for b in &boxed[i + 1..] {
            let (ra, rb) = (rect(a), rect(b));
            if inside(ra, rb) || inside(rb, ra) || overlap_x(ra, rb) <= ra[2].min(rb[2]) * CORNER_OVERLAP {
                continue;
            }
            let d = distance_y(ra, rb);
            if d < scene.height * ROW_GAP_MIN {
                problems.push(format!(
                    "'{}' and '{}' are {d:.0} mm apart: leave about 4% of the height ({:.0} mm) between rows",
                    a.name,
                    b.name,
                    scene.height * ROW_GAP_TARGET
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relayout_guard_judges_ink_air_and_boxes() {
        let root = std::env::temp_dir().join(format!("hoist-relayout-{}", uuid::Uuid::new_v4().simple()));
        let mut h = hoist_core::store::ProjectStore::new(root)
            .create("relayout", 6000.0, 900.0)
            .expect("project");
        let layer = |id: &str, kind: hoist_core::model::LayerKind, rect: [f64; 4]| hoist_core::model::Layer {
            id: id.into(),
            name: id.into(),
            parent: None,
            kind,
            x_mm: rect[0],
            y_mm: rect[1],
            w_mm: rect[2],
            h_mm: rect[3],
            rotation_deg: 0.0,
            opacity: 1.0,
            visible: true,
            locked: false,
            fills: vec![],
            strokes: vec![],
            effects: vec![],
            strategy: hoist_core::model::Strategy::NativeVector,
            strategy_reason: String::new(),
            pinned: vec![],
            source: None,
        };
        let text = |id: &str, size: f64, rect: [f64; 4]| {
            layer(
                id,
                hoist_core::model::LayerKind::Text(hoist_core::model::TextProps {
                    text: id.into(),
                    font_family: "Pretendard".into(),
                    font_size_mm: size,
                    ..Default::default()
                }),
                rect,
            )
        };
        let shape = |id: &str, rx_mm: f64, rect: [f64; 4]| {
            layer(
                id,
                hoist_core::model::LayerKind::Shape(hoist_core::model::ShapeProps::Rect { rx_mm }),
                rect,
            )
        };
        let fonts = Fonts::new(Some(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fonts"),
        ));
        assert!(
            relayout_guard_message(&h, &fonts).is_none(),
            "no text, nothing to judge"
        );
        let ink = |h: &ProjectHandle, id: &str| tools::ink_mm(&fonts, h.document.layer(id).expect("layer")).0;
        let set = |h: &mut ProjectHandle, id: &str, edit: &dyn Fn(&mut hoist_core::model::Layer)| {
            edit(h.document.layer_mut(id).expect("layer"));
        };
        let size_of = |h: &mut ProjectHandle, id: &str, size: f64| {
            if let Some(hoist_core::model::LayerKind::Text(t)) = h.document.layer_mut(id).map(|l| &mut l.kind)
            {
                t.font_size_mm = size;
            }
        };

        // A calm layout: title ink near 27% of the height, a pill under it, a padded panel with three columns.
        h.document.layers = vec![
            text("청전동", 280.0, [1000.0, 150.0, 1300.0, 400.0]),
            text("주민역량강화", 280.0, [2400.0, 150.0, 2600.0, 400.0]),
            shape("알약", 50.0, [2000.0, 525.0, 600.0, 100.0]),
            text("건강한 일상 함께하는 청전동", 60.0, [2100.0, 540.0, 1200.0, 75.0]),
            shape("board", 0.0, [800.0, 660.0, 4400.0, 195.0]),
            text("칸 0", 45.0, [900.0, 700.0, 900.0, 110.0]),
            text("칸 1", 45.0, [2400.0, 700.0, 900.0, 110.0]),
            text("칸 2", 45.0, [3900.0, 700.0, 900.0, 110.0]),
        ];
        let title_ink = ink(&h, "청전동");
        assert!(
            (0.24..0.32).contains(&(title_ink[3] / 900.0)),
            "the fixture's title ink is {} mm",
            title_ink[3]
        );
        let second = ink(&h, "주민역량강화")[0];
        set(&mut h, "주민역량강화", &|l| {
            l.x_mm += title_ink[0] + title_ink[2] + 100.0 - second
        });
        let label = ink(&h, "건강한 일상 함께하는 청전동");
        set(&mut h, "알약", &|l| {
            (l.x_mm, l.w_mm) = (label[0] - 40.0, label[2] + 80.0)
        });
        assert_eq!(relayout_guard_message(&h, &fonts), None, "a calm layout passes");

        let calm = h.document.layers.clone();
        set(&mut h, "알약", &|l| {
            (l.x_mm, l.w_mm) = (label[0] - 300.0, label[2] + 340.0)
        });
        let mid = label[1] + label[3] / 2.0;
        h.document.layers.push(shape(
            "띠 머리표",
            30.0,
            [label[0] - 260.0, mid - 30.0 + 15.0, 200.0, 60.0],
        ));
        h.document.layers.push(text(
            "머리",
            40.0,
            [label[0] - 240.0, mid - 25.0 + 15.0, 160.0, 50.0],
        ));
        let msg = relayout_guard_message(&h, &fonts).expect("a band part off the center line");
        assert!(msg.contains("off one center line"), "{msg}");
        set(&mut h, "띠 머리표", &|l| l.y_mm = mid - 30.0);
        let badge_mid = {
            let i = ink(&h, "머리");
            i[1] + i[3] / 2.0
        };
        set(&mut h, "머리", &|l| l.y_mm += mid - badge_mid);
        let msg = relayout_guard_message(&h, &fonts).unwrap_or_default();
        assert!(!msg.contains("off one center line"), "{msg}");
        h.document
            .layers
            .push(shape("띠 구분선", 0.0, [label[0] - 50.0, mid - 30.0, 3.0, 60.0]));
        let msg = relayout_guard_message(&h, &fonts).unwrap_or_default();
        assert!(
            !msg.contains("gap with nothing in it"),
            "a 60 mm gap is fine: {msg}"
        );
        set(&mut h, "띠 머리표", &|l| l.x_mm -= 100.0);
        set(&mut h, "머리", &|l| l.x_mm -= 100.0);
        set(&mut h, "띠 구분선", &|l| l.visible = false);
        let msg = relayout_guard_message(&h, &fonts).expect("a wide empty gap in a band");
        assert!(msg.contains("gap with nothing in it"), "{msg}");
        set(&mut h, "띠 머리표", &|l| l.x_mm += 100.0);
        set(&mut h, "머리", &|l| l.x_mm += 100.0);
        set(&mut h, "띠 구분선", &|l| l.visible = true);
        let msg = relayout_guard_message(&h, &fonts).expect("a divider off the gap center");
        assert!(msg.contains("from its left part"), "{msg}");
        set(&mut h, "띠 구분선", &|l| l.x_mm = label[0] - 31.5);
        let msg = relayout_guard_message(&h, &fonts).unwrap_or_default();
        assert!(!msg.contains("from its left part"), "{msg}");
        h.document.layers = calm.clone();
        let body_ink = ink(&h, "칸 0");
        h.document
            .layers
            .push(shape("머리표", 20.0, [body_ink[0] + 40.0, 672.0, 200.0, 60.0]));
        h.document
            .layers
            .push(text("표", 45.0, [body_ink[0] + 60.0, 675.0, 100.0, 55.0]));
        let msg = relayout_guard_message(&h, &fonts).expect("a badge off its body's left line");
        assert!(msg.contains("do not start where their body starts"), "{msg}");
        set(&mut h, "머리표", &|l| l.x_mm = body_ink[0]);
        let msg = relayout_guard_message(&h, &fonts).unwrap_or_default();
        assert!(!msg.contains("do not start where"), "{msg}");
        set(&mut h, "머리표", &|l| (l.y_mm, l.h_mm) = (662.0, 40.0));
        set(&mut h, "표", &|l| (l.y_mm, l.h_mm) = (662.0, 40.0));
        h.document
            .layers
            .push(shape("구분선", 0.0, [body_ink[0] - 90.0, 672.0, 3.0, 150.0]));
        let msg = relayout_guard_message(&h, &fonts).expect("a divider in the middle of a gap");
        assert!(msg.contains("dividers stand away"), "{msg}");
        set(&mut h, "구분선", &|l| l.x_mm = body_ink[0] - 45.0);
        let msg = relayout_guard_message(&h, &fonts).unwrap_or_default();
        assert!(!msg.contains("dividers stand away"), "{msg}");
        let reword = |h: &mut ProjectHandle, wording: &str, w: f64| {
            let l = h.document.layer_mut("칸 0").expect("layer");
            l.w_mm = w;
            if let hoist_core::model::LayerKind::Text(t) = &mut l.kind {
                t.text = wording.into();
            }
        };
        reword(&mut h, "제천시노인종합복지관 4층", 300.0);
        let msg = relayout_guard_message(&h, &fonts).expect("a line that wrapped on its own");
        assert!(msg.contains("wrapped on its own"), "{msg}");
        reword(&mut h, "복지관 4\n층", 900.0);
        let msg = relayout_guard_message(&h, &fonts).expect("a stranded character");
        assert!(msg.contains("one-character line"), "{msg}");
        h.document.layers = calm.clone();

        h.document
            .layers
            .push(text("연도", 60.0, [2400.0, 45.0, 600.0, 75.0]));
        let msg = relayout_guard_message(&h, &fonts).expect("an eyebrow far under the title");
        assert!(msg.contains("stands over the title"), "{msg}");
        h.document.layers = calm;

        size_of(&mut h, "청전동", 420.0);
        size_of(&mut h, "주민역량강화", 420.0);
        let msg = relayout_guard_message(&h, &fonts).expect("a title that fills the banner");
        assert!(
            msg.contains("the title's glyphs are") && msg.contains("target 25–30%"),
            "{msg}"
        );
        size_of(&mut h, "청전동", 200.0);
        size_of(&mut h, "주민역량강화", 200.0);
        let msg = relayout_guard_message(&h, &fonts).expect("a small title with width to spare");
        assert!(msg.contains("with width to spare"), "{msg}");
        size_of(&mut h, "청전동", 280.0);
        size_of(&mut h, "주민역량강화", 280.0);

        set(&mut h, "주민역량강화", &|l| l.x_mm += 400.0);
        let msg = relayout_guard_message(&h, &fonts).expect("title words far apart");
        assert!(msg.contains("title words are") && msg.contains("apart"), "{msg}");
        set(&mut h, "주민역량강화", &|l| l.x_mm -= 400.0);

        let against = title_ink[1] + title_ink[3] + 5.0;
        set(&mut h, "알약", &|l| l.y_mm = against);
        set(&mut h, "건강한 일상 함께하는 청전동", &|l| {
            l.y_mm = against + 15.0
        });
        let msg = relayout_guard_message(&h, &fonts).expect("a band against the title");
        assert!(
            msg.contains("'알약' is only") && msg.contains("from the title's glyphs"),
            "{msg}"
        );
        set(&mut h, "알약", &|l| l.y_mm = 525.0);
        set(&mut h, "건강한 일상 함께하는 청전동", &|l| {
            l.y_mm = 540.0
        });

        set(&mut h, "알약", &|l| l.w_mm = 1500.0);
        let msg = relayout_guard_message(&h, &fonts).expect("a pill longer than its text");
        assert!(
            msg.contains("'알약' is 1500 mm wide but its content is") && msg.contains("fit_box it"),
            "{msg}"
        );
        set(&mut h, "알약", &|l| l.w_mm = label[2] + 80.0);

        set(&mut h, "board", &|l| l.y_mm = 640.0);
        let msg = relayout_guard_message(&h, &fonts).expect("rows too close");
        assert!(
            msg.contains("'알약' and 'board' are") || msg.contains("'board' and '알약' are"),
            "{msg}"
        );
        set(&mut h, "board", &|l| l.y_mm = 660.0);

        set(&mut h, "board", &|l| l.h_mm = 225.0);
        let msg = relayout_guard_message(&h, &fonts).expect("a panel on the bottom edge");
        assert!(msg.contains("'board' is 15 mm from the bottom edge"), "{msg}");
        set(&mut h, "board", &|l| l.h_mm = 195.0);

        set(&mut h, "칸 0", &|l| l.y_mm = 662.0);
        let msg = relayout_guard_message(&h, &fonts).expect("text against the panel's edge");
        assert!(
            msg.contains("'board' leaves") && msg.contains("its top edge"),
            "{msg}"
        );
        set(&mut h, "칸 0", &|l| l.y_mm = 700.0);

        set(&mut h, "칸 1", &|l| l.w_mm = 200.0);
        if let Some(hoist_core::model::LayerKind::Text(t)) = h.document.layer_mut("칸 1").map(|l| &mut l.kind)
        {
            t.text = "교육일정은 십일월 구일부터 십이월 십사일까지".into();
        }
        let msg = relayout_guard_message(&h, &fonts).expect("a column wraps into many lines");
        assert!(
            msg.contains("'칸 1' has") && msg.contains("at most 2 lines"),
            "{msg}"
        );
        if let Some(hoist_core::model::LayerKind::Text(t)) = h.document.layer_mut("칸 1").map(|l| &mut l.kind)
        {
            t.text = "칸 1".into();
        }
        set(&mut h, "칸 1", &|l| l.w_mm = 900.0);

        let squeeze = |h: &mut ProjectHandle, scale_x: f64| {
            if let Some(hoist_core::model::LayerKind::Text(t)) =
                h.document.layer_mut("칸 0").map(|l| &mut l.kind)
            {
                t.scale_x = scale_x;
            }
        };
        squeeze(&mut h, 1.2);
        let msg = relayout_guard_message(&h, &fonts).expect("widened letters");
        assert!(
            msg.contains("texts in Pretendard inside 'board' use different scale_x (1.00–1.20)"),
            "{msg}"
        );
        squeeze(&mut h, 1.03);
        assert_eq!(
            relayout_guard_message(&h, &fonts),
            None,
            "a small difference in scale_x passes"
        );

        // A year on a brush stroke: the stroke carries text, so it may not sit in the top rim either; its
        // scale_x was fitted on its own and may differ from the columns.
        let mut year = text("연도", 85.0, [2700.0, 50.0, 600.0, 90.0]);
        if let hoist_core::model::LayerKind::Text(t) = &mut year.kind {
            t.scale_x = 1.2;
        }
        h.document.layers.push(year);
        h.document.layers.push(layer(
            "붓띠",
            hoist_core::model::LayerKind::Path(hoist_core::model::PathProps {
                d: "M 0 0 L 800 0 L 800 90 L 0 90 Z".into(),
                fill_rule: Default::default(),
            }),
            [2600.0, 50.0, 800.0, 90.0],
        ));
        assert_eq!(
            relayout_guard_message(&h, &fonts),
            None,
            "a text fitted on its own, clear of the rim"
        );
        set(&mut h, "붓띠", &|l| l.y_mm = 20.0);
        let msg = relayout_guard_message(&h, &fonts).expect("a band in the top rim");
        assert!(msg.contains("'붓띠' is 20 mm from the top edge"), "{msg}");
    }

    /// Runs the app's relayout guard on a saved project: HOIST_GUARD_PROJECT=<id> cargo test -p hoist guard_on_saved -- --ignored
    #[test]
    #[ignore = "reads a real project named by HOIST_GUARD_PROJECT"]
    fn guard_on_saved_project() {
        let id = std::env::var("HOIST_GUARD_PROJECT").expect("HOIST_GUARD_PROJECT");
        let docs = std::path::PathBuf::from(std::env::var("USERPROFILE").expect("home"))
            .join("Documents")
            .join("Hoist");
        let h = hoist_core::store::ProjectStore::new(docs)
            .open(&id)
            .expect("project");
        let fonts = Fonts::new(Some(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fonts"),
        ));
        assert_eq!(
            relayout_guard_message(&h, &fonts),
            None,
            "the saved project passes the guard"
        );
    }
}
