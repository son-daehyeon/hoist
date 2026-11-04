use serde::{Deserialize, Serialize};

pub type LayerId = String;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub target_dpi: f64,
    #[serde(default)]
    pub originals: Vec<OriginalInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OriginalInfo {
    pub version: u32,
    pub file: String,
    pub width_px: u32,
    pub height_px: u32,
    pub uploaded_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Document {
    pub rev: u64,
    pub artboard: Artboard,
    pub layers: Vec<Layer>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Artboard {
    pub width_mm: f64,
    pub height_mm: f64,
    #[serde(default)]
    pub bleed_mm: f64,
    #[serde(default)]
    pub background: Option<Paint>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    #[serde(default)]
    pub parent: Option<LayerId>,
    pub kind: LayerKind,
    pub x_mm: f64,
    pub y_mm: f64,
    pub w_mm: f64,
    pub h_mm: f64,
    #[serde(default)]
    pub rotation_deg: f64,
    #[serde(default = "one")]
    pub opacity: f64,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub fills: Vec<Paint>,
    #[serde(default)]
    pub strokes: Vec<Stroke>,
    #[serde(default)]
    pub effects: Vec<Effect>,
    pub strategy: Strategy,
    #[serde(default)]
    pub strategy_reason: String,
    #[serde(default)]
    pub pinned: Vec<String>,
    #[serde(default)]
    pub source: Option<LayerSource>,
}

fn one() -> f64 {
    1.0
}
fn yes() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LayerKind {
    Group,
    Text(TextProps),
    Path(PathProps),
    Shape(ShapeProps),
    Image(ImageProps),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TextProps {
    pub text: String,
    pub font_family: String,
    #[serde(default = "w700")]
    pub font_weight: u16,
    pub font_size_mm: f64,
    #[serde(default)]
    pub letter_spacing_mm: f64,
    #[serde(default = "lh")]
    pub line_height: f64,
    #[serde(default)]
    pub align: TextAlign,
}

fn w700() -> u16 {
    700
}
fn lh() -> f64 {
    1.2
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PathProps {
    pub d: String,
    #[serde(default)]
    pub fill_rule: FillRule,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FillRule {
    #[default]
    Nonzero,
    Evenodd,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum ShapeProps {
    Rect {
        #[serde(default)]
        rx_mm: f64,
    },
    Ellipse,
    Line,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ImageProps {
    pub asset: String,
    pub natural_w_px: u32,
    pub natural_h_px: u32,
    #[serde(default)]
    pub fit: ImageFit,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ImageFit {
    #[default]
    Fill,
    Contain,
    Cover,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Paint {
    Solid {
        color: String,
        #[serde(default = "one")]
        opacity: f64,
    },
    LinearGradient {
        angle_deg: f64,
        stops: Vec<GradientStop>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GradientStop {
    pub offset: f64,
    pub color: String,
    #[serde(default = "one")]
    pub opacity: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Stroke {
    pub paint: Paint,
    pub width_mm: f64,
    #[serde(default)]
    pub align: StrokeAlign,
    #[serde(default)]
    pub join: StrokeJoin,
    #[serde(default)]
    pub dash_mm: Vec<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum StrokeAlign {
    Inside,
    #[default]
    Center,
    Outside,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum StrokeJoin {
    Miter,
    #[default]
    Round,
    Bevel,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Effect {
    DropShadow {
        x_mm: f64,
        y_mm: f64,
        blur_mm: f64,
        #[serde(default)]
        spread_mm: f64,
        color: String,
        #[serde(default = "one")]
        opacity: f64,
    },
    LayerBlur {
        radius_mm: f64,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Strategy {
    #[default]
    NativeVector,
    TextTraced,
    Text,
    TracedVector,
    GeneratedRaster,
    Background,
    OriginalCrop,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct LayerSource {
    #[serde(default)]
    pub original_version: Option<u32>,
    #[serde(default)]
    pub crop_px: Option<[u32; 4]>,
    #[serde(default)]
    pub asset: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    AddLayer {
        layer: Layer,
        #[serde(default)]
        index: Option<usize>,
    },
    UpdateLayer {
        id: LayerId,
        patch: serde_json::Value,
    },
    DeleteLayer {
        id: LayerId,
    },
    MoveLayer {
        id: LayerId,
        #[serde(default)]
        parent: Option<LayerId>,
        index: usize,
    },
    SetArtboard {
        patch: serde_json::Value,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Actor {
    User,
    Agent,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OpEntry {
    pub seq: u64,
    pub actor: Actor,
    pub ts: String,
    #[serde(default)]
    pub turn: Option<String>,
    pub op: Op,
    pub inverse: Op,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Version {
    pub seq: u64,
    pub ts: String,
    pub actor: Actor,
    pub summary: String,
    #[serde(default)]
    pub turn: Option<String>,
}

impl Document {
    pub fn new(width_mm: f64, height_mm: f64) -> Self {
        Document {
            rev: 0,
            artboard: Artboard {
                width_mm,
                height_mm,
                bleed_mm: 0.0,
                background: Some(Paint::Solid {
                    color: "#FFFFFF".into(),
                    opacity: 1.0,
                }),
            },
            layers: Vec::new(),
        }
    }

    pub fn layer(&self, id: &str) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    pub fn layer_mut(&mut self, id: &str) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.id == id)
    }

    pub fn children(&self, parent: Option<&str>) -> Vec<&Layer> {
        self.layers
            .iter()
            .filter(|l| l.parent.as_deref() == parent)
            .collect()
    }

    pub fn next_id(&self) -> LayerId {
        let max = self
            .layers
            .iter()
            .filter_map(|l| l.id.strip_prefix('L').and_then(|n| n.parse::<u64>().ok()))
            .max()
            .unwrap_or(0);
        format!("L{}", max + 1)
    }
}
