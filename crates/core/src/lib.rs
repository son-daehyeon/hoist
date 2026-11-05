pub mod analyze;
pub mod boolean;
pub mod check;
pub mod compare;
pub mod export;
pub mod fontmatch;
pub mod image;
pub mod matting;
pub mod model;
mod onnx;
pub mod ops;
pub mod render;
pub mod store;
pub mod text;
pub mod trace;

pub use model::*;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("layer not found: {0}")]
    LayerNotFound(String),
    #[error("invalid op: {0}")]
    InvalidOp(String),
    #[error("project not found: {0}")]
    ProjectNotFound(String),
    #[error("asset not found: {0}")]
    AssetNotFound(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("image: {0}")]
    Image(#[from] ::image::ImageError),
    #[error("render: {0}")]
    Render(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;
