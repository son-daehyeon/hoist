use serde::{Deserialize, Serialize};

use crate::model::*;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ExportWarning {
    pub layer_id: LayerId,
    pub layer_name: String,
    pub effective_dpi: f64,
}

pub fn export_check(doc: &Document, dpi: f64) -> Vec<ExportWarning> {
    todo!()
}

pub fn effective_dpi(layer: &Layer) -> Option<f64> {
    todo!()
}
