export type LayerId = string;

export interface Project {
  id: string;
  name: string;
  created_at: string;
  updated_at: string;
  width_mm: number;
  height_mm: number;
  target_dpi: number;
  originals: OriginalInfo[];
}

export interface OriginalInfo {
  version: number;
  file: string;
  width_px: number;
  height_px: number;
  uploaded_at: string;
}

export interface ProjectSummary {
  id: string;
  name: string;
  width_mm: number;
  height_mm: number;
  updated_at: string;
  thumbnail_png?: string;
}

export interface Document {
  rev: number;
  artboard: Artboard;
  layers: Layer[];
}

export interface Artboard {
  width_mm: number;
  height_mm: number;
  bleed_mm: number;
  background?: Paint | null;
}

export interface Layer {
  id: LayerId;
  name: string;
  parent?: LayerId | null;
  kind: LayerKind;
  x_mm: number;
  y_mm: number;
  w_mm: number;
  h_mm: number;
  rotation_deg: number;
  opacity: number;
  visible: boolean;
  locked: boolean;
  fills: Paint[];
  strokes: Stroke[];
  effects: Effect[];
  strategy: Strategy;
  strategy_reason: string;
  pinned: string[];
  source?: LayerSource | null;
}

export type LayerKind =
  | { type: "group" }
  | ({ type: "text" } & TextProps)
  | ({ type: "path" } & PathProps)
  | ({ type: "shape" } & ShapeProps)
  | ({ type: "image" } & ImageProps);

export interface TextProps {
  text: string;
  font_family: string;
  font_weight: number;
  font_size_mm: number;
  letter_spacing_mm: number;
  line_height: number;
  align: "left" | "center" | "right";
}

export interface PathProps {
  d: string;
  fill_rule: "nonzero" | "evenodd";
}

export type ShapeProps = { shape: "rect"; rx_mm: number } | { shape: "ellipse" } | { shape: "line" };

export interface ImageProps {
  asset: string;
  natural_w_px: number;
  natural_h_px: number;
  fit: "fill" | "contain" | "cover";
}

export type Paint =
  | { type: "solid"; color: string; opacity: number }
  | { type: "linear_gradient"; angle_deg: number; stops: GradientStop[] };

export interface GradientStop {
  offset: number;
  color: string;
  opacity: number;
}

export interface Stroke {
  paint: Paint;
  width_mm: number;
  align: "inside" | "center" | "outside";
  join: "miter" | "round" | "bevel";
  dash_mm: number[];
}

export type Effect =
  | {
      type: "drop_shadow";
      x_mm: number;
      y_mm: number;
      blur_mm: number;
      spread_mm: number;
      color: string;
      opacity: number;
    }
  | { type: "layer_blur"; radius_mm: number };

export type Strategy =
  | "native_vector"
  | "text_traced"
  | "text"
  | "traced_vector"
  | "generated_raster"
  | "background"
  | "original_crop";

export interface LayerSource {
  original_version?: number | null;
  crop_px?: [number, number, number, number] | null;
  asset?: string | null;
  prompt?: string | null;
}

export type Op =
  | { op: "add_layer"; layer: Layer; index?: number | null }
  | { op: "update_layer"; id: LayerId; patch: Record<string, unknown> }
  | { op: "delete_layer"; id: LayerId }
  | { op: "move_layer"; id: LayerId; parent?: LayerId | null; index: number }
  | { op: "set_artboard"; patch: Record<string, unknown> };

export type Actor = "user" | "agent";

export interface OpEntry {
  seq: number;
  actor: Actor;
  ts: string;
  turn?: string | null;
  op: Op;
  inverse: Op;
}

export interface Version {
  seq: number;
  ts: string;
  actor: Actor;
  summary: string;
  turn?: string | null;
  thumbnail_png?: string;
}

export interface LayerRaster {
  id: LayerId;
  png_base64: string;
  x_px: number;
  y_px: number;
  w_px: number;
  h_px: number;
}

export interface RenderResult {
  scale_px_per_mm: number;
  width_px: number;
  height_px: number;
  layers: LayerRaster[];
}

export interface ExportOptions {
  format: "png" | "svg";
  scale: 1 | 10;
  dpi: number;
  outline_text: boolean;
}

export interface ExportResult {
  path: string;
  width_px?: number;
  height_px?: number;
  bytes: number;
  warnings: ExportWarning[];
}

export interface ExportWarning {
  layer_id: LayerId;
  layer_name: string;
  effective_dpi: number;
}

export interface Settings {
  openai_key_set: boolean;
  projects_dir: string;
  agent_model: string;
  heavy_model: string;
  cheap_model: string;
  image_model: string;
  usage_month_krw: number;
}

export interface Capture {
  pane: "original" | "work";
  x_mm: number;
  y_mm: number;
  w_mm: number;
  h_mm: number;
}

export interface SendContext {
  layer_ids: LayerId[];
  captures: Capture[];
}

export type ActionStatus = "running" | "done" | "failed" | "todo";

export interface ActionItem {
  id: string;
  label: string;
  status: ActionStatus;
}

export interface Report {
  original_png: string;
  work_png: string;
  overlap: number;
  min_dpi: number;
  promoted: number;
  summary: string;
}

export interface Question {
  id: string;
  text: string;
  options: string[];
}

export interface ChatItem {
  id: string;
  role: "user" | "agent";
  ts: string;
  text: string;
  context?: SendContext;
  actions?: ActionItem[];
  report?: Report;
  question?: Question;
  answer?: string;
  edit_summary?: string[];
}

export type AgentEvent =
  | { type: "turn_start"; turn: string }
  | { type: "text_delta"; turn: string; delta: string }
  | { type: "text"; turn: string; text: string }
  | { type: "action"; turn: string; action: ActionItem }
  | { type: "document_changed"; turn: string; rev: number }
  | { type: "report"; turn: string; report: Report }
  | { type: "question"; turn: string; question: Question }
  | { type: "turn_end"; turn: string; stopped: boolean }
  | { type: "error"; turn: string; message: string };
