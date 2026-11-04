use crate::model::*;
use crate::Result;

pub fn apply_op(doc: &mut Document, op: &Op) -> Result<Op> {
    todo!()
}

pub fn merge_patch(target: &mut serde_json::Value, patch: &serde_json::Value) {
    todo!()
}

pub fn patch_paths(patch: &serde_json::Value) -> Vec<String> {
    todo!()
}

pub fn describe(op: &Op, doc: &Document) -> String {
    todo!()
}
