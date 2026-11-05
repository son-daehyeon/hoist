use serde::{Deserialize, Serialize};

use crate::model::*;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, specta::Type)]
pub struct ExportWarning {
    pub layer_id: LayerId,
    pub layer_name: String,
    pub effective_dpi: f64,
}

pub fn export_check(doc: &Document, dpi: f64) -> Vec<ExportWarning> {
    doc.layers
        .iter()
        .filter(|l| shown(doc, l))
        .filter_map(|l| {
            let effective_dpi = effective_dpi(l)?;
            (effective_dpi < dpi).then(|| ExportWarning {
                layer_id: l.id.clone(),
                layer_name: l.name.clone(),
                effective_dpi,
            })
        })
        .collect()
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
                effective_dpi: 50.0
            }]
        );
    }
}
