use serde::{Deserialize, Serialize};

use crate::model::*;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, specta::Type)]
pub struct ExportWarning {
    pub layer_id: LayerId,
    pub layer_name: String,
    pub effective_dpi: f64,
    pub required_dpi: f64,
}

/// Backgrounds are soft pictures seen from afar, so they only need half the target.
pub fn required_dpi(layer: &Layer, dpi: f64) -> f64 {
    if layer.strategy == Strategy::Background {
        dpi / 2.0
    } else {
        dpi
    }
}

pub fn export_check(doc: &Document, dpi: f64) -> Vec<ExportWarning> {
    doc.layers
        .iter()
        .filter(|l| shown(doc, l))
        .filter_map(|l| {
            let effective_dpi = effective_dpi(l)?;
            let required_dpi = required_dpi(l, dpi);
            (effective_dpi < required_dpi).then(|| ExportWarning {
                layer_id: l.id.clone(),
                layer_name: l.name.clone(),
                effective_dpi,
                required_dpi,
            })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DpiSummary {
    pub min_dpi: Option<f64>,
    pub background_min_dpi: Option<f64>,
    pub ok: bool,
}

/// Lowest effective dpi of shown image layers, backgrounds apart, each judged by its own requirement.
pub fn dpi_summary(doc: &Document, dpi: f64) -> DpiSummary {
    let mut out = DpiSummary {
        min_dpi: None,
        background_min_dpi: None,
        ok: true,
    };
    for l in doc.layers.iter().filter(|l| shown(doc, l)) {
        let Some(d) = effective_dpi(l) else {
            continue;
        };
        let slot = if l.strategy == Strategy::Background {
            &mut out.background_min_dpi
        } else {
            &mut out.min_dpi
        };
        *slot = Some(slot.map_or(d, |m| m.min(d)));
        out.ok &= d >= required_dpi(l, dpi);
    }
    out
}

pub fn effective_dpi(layer: &Layer) -> Option<f64> {
    let LayerKind::Image(img) = &layer.kind else {
        return None;
    };
    if !(layer.w_mm > 0.0 && layer.h_mm > 0.0) {
        return None;
    }
    let x = f64::from(img.natural_w_px) / (layer.w_mm / 25.4);
    let y = f64::from(img.natural_h_px) / (layer.h_mm / 25.4);
    Some(x.min(y))
}

fn shown(doc: &Document, layer: &Layer) -> bool {
    let mut current = Some(layer);
    for _ in 0..=doc.layers.len() {
        match current {
            None => return true,
            Some(l) if !l.visible => return false,
            Some(l) => current = l.parent.as_deref().and_then(|p| doc.layer(p)),
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::tests::layer;

    fn image_layer(id: &str, parent: Option<&str>, px: u32, w_mm: f64) -> Layer {
        let mut l = layer(
            id,
            id,
            parent,
            LayerKind::Image(ImageProps {
                asset: "a.png".into(),
                natural_w_px: px,
                natural_h_px: px,
                fit: ImageFit::Fill,
                ..Default::default()
            }),
        );
        l.w_mm = w_mm;
        l.h_mm = w_mm / 2.0;
        l
    }

    #[test]
    fn effective_dpi_uses_smaller_axis() {
        let l = image_layer("L1", None, 1000, 254.0);
        assert!(effective_dpi(&l).is_some_and(|d| (d - 100.0).abs() < 1e-9));
        let mut text = layer("L2", "t", None, LayerKind::Group);
        text.w_mm = 10.0;
        assert_eq!(effective_dpi(&text), None);
    }

    #[test]
    fn export_check_flags_visible_low_res_images() {
        let mut doc = Document::new(900.0, 300.0);
        let mut hidden_group = layer("G", "그룹", None, LayerKind::Group);
        hidden_group.visible = false;
        let mut hidden = image_layer("L3", None, 100, 254.0);
        hidden.visible = false;
        doc.layers = vec![
            image_layer("L1", None, 2000, 254.0),
            image_layer("L2", None, 500, 254.0),
            hidden,
            hidden_group,
            image_layer("L4", Some("G"), 100, 254.0),
        ];
        let warnings = export_check(&doc, 100.0);
        assert_eq!(
            warnings,
            [ExportWarning {
                layer_id: "L2".into(),
                layer_name: "L2".into(),
                effective_dpi: 50.0,
                required_dpi: 100.0
            }]
        );
    }

    #[test]
    fn backgrounds_need_half_the_target() {
        let mut doc = Document::new(900.0, 300.0);
        let mut soft = image_layer("BG", None, 600, 254.0);
        soft.strategy = Strategy::Background;
        doc.layers = vec![soft.clone(), image_layer("P", None, 1200, 254.0)];
        assert!(
            export_check(&doc, 100.0).is_empty(),
            "60dpi background passes at 100"
        );
        assert_eq!(
            dpi_summary(&doc, 100.0),
            DpiSummary {
                min_dpi: Some(120.0),
                background_min_dpi: Some(60.0),
                ok: true
            }
        );

        doc.layers[0].kind = image_layer("BG", None, 400, 254.0).kind;
        let warnings = export_check(&doc, 100.0);
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            (warnings[0].effective_dpi, warnings[0].required_dpi),
            (40.0, 50.0)
        );
        assert!(!dpi_summary(&doc, 100.0).ok);

        doc.layers[0] = soft;
        doc.layers[1].strategy = Strategy::OriginalCrop;
        doc.layers[1].kind = image_layer("P", None, 600, 254.0).kind;
        let warnings = export_check(&doc, 100.0);
        assert_eq!(warnings.len(), 1, "a 60dpi picture still needs the full target");
        assert_eq!(warnings[0].required_dpi, 100.0);
    }
}
