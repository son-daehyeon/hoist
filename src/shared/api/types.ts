import type {
  ActionItem_Deserialize,
  AgentEventKind_Deserialize,
  AgentEvent_Deserialize,
  ApplyResult_Serialize,
  Artboard_Serialize,
  CreateArgs_Deserialize,
  Document_Serialize,
  Effect_Serialize,
  ExportOptionsIn_Deserialize,
  ExportResultView_Serialize,
  GradientStop_Serialize,
  ImageProps_Serialize,
  LayerKind_Serialize,
  LayerRasterView_Serialize,
  LayerSvgView_Serialize,
  LayerSvgsView_Serialize,
  LayerSource_Serialize,
  Layer_Serialize,
  OpEntry_Serialize,
  Op_Deserialize,
  OpenedProject_Serialize,
  Paint_Serialize,
  PathProps_Serialize,
  Project_Serialize,
  ProjectSummaryView_Serialize,
  Question,
  RenderResultView_Serialize,
  Report,
  SendContext_Serialize,
  SettingsPatch_Deserialize,
  SettingsView,
  ShapeProps_Serialize,
  Stroke_Serialize,
  TextProps_Serialize,
  UpdateAvailable_Serialize,
  UpdateInfo_Serialize,
  UpdateProgress_Deserialize,
  VersionView_Serialize,
} from "./bindings";

export type {
  ActionStatus,
  Actor,
  Capture,
  ExportWarning,
  FitMode,
  OriginalInfo,
  Question,
  Report,
  Strategy,
} from "./bindings";

export type LayerId = string;

export type ActionItem = ActionItem_Deserialize;
export type AgentEventKind = AgentEventKind_Deserialize;
export type AgentEvent = AgentEvent_Deserialize;

export type Project = Project_Serialize;
export type ProjectSummary = ProjectSummaryView_Serialize;
export type Document = Document_Serialize;
export type Artboard = Artboard_Serialize;
export type Layer = Layer_Serialize;
export type LayerKind = LayerKind_Serialize;
export type TextProps = TextProps_Serialize;
export type PathProps = PathProps_Serialize;
export type ShapeProps = ShapeProps_Serialize;
export type ImageProps = ImageProps_Serialize;
export type Paint = Paint_Serialize;
export type GradientStop = GradientStop_Serialize;
export type Stroke = Stroke_Serialize;
export type Effect = Effect_Serialize;
export type LayerSource = LayerSource_Serialize;
export type OpEntry = OpEntry_Serialize;
export type Version = VersionView_Serialize;
export type LayerRaster = LayerRasterView_Serialize;
export type RenderResult = RenderResultView_Serialize;
export type ExportResult = ExportResultView_Serialize;
export type Settings = SettingsView;
export type SendContext = SendContext_Serialize;
export type ApplyResult = ApplyResult_Serialize;
export type UpdateInfo = UpdateInfo_Serialize;
export type UpdateAvailable = UpdateAvailable_Serialize;
export type UpdateProgress = UpdateProgress_Deserialize;

export type Op = Op_Deserialize;
export type ExportOptions = ExportOptionsIn_Deserialize;
export type SettingsPatch = SettingsPatch_Deserialize;
export type CreateArgs = CreateArgs_Deserialize;

export type LayerSvg = LayerSvgView_Serialize;
export type LayerSvgs = LayerSvgsView_Serialize;

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
  questions?: Question[];
  answers?: (string | null)[];
  edit_summary?: string[];
  turn_krw?: number;
  thinking_sec?: number;
}

export type OpenedProject = Omit<OpenedProject_Serialize, "chat"> & { chat: ChatItem[] };
