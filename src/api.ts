import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  Actor,
  AgentEvent,
  ChatItem,
  Document,
  ExportOptions,
  ExportResult,
  Op,
  OpEntry,
  Project,
  ProjectSummary,
  RenderResult,
  SendContext,
  Settings,
  Version,
} from "./types";

export interface OpenedProject {
  project: Project;
  document: Document;
  memory: string;
  chat: ChatItem[];
  versions: Version[];
  can_undo: boolean;
  can_redo: boolean;
}

export interface ApplyResult {
  document: Document;
  entries: OpEntry[];
  can_undo: boolean;
  can_redo: boolean;
}

export const api = {
  listProjects: () => invoke<ProjectSummary[]>("list_projects"),
  createProject: (args: {
    name: string;
    width_mm: number;
    height_mm: number;
    original_path?: string | null;
  }) => invoke<Project>("create_project", { args }),
  deleteProject: (id: string) => invoke<void>("delete_project", { id }),
  renameProject: (id: string, name: string) =>
    invoke<Project>("rename_project", { id, name }),
  duplicateProject: (id: string) => invoke<Project>("duplicate_project", { id }),
  openProject: (id: string) => invoke<OpenedProject>("open_project", { id }),
  showInFolder: (id: string) => invoke<void>("show_in_folder", { id }),

  applyOps: (id: string, ops: Op[], actor: Actor = "user") =>
    invoke<ApplyResult>("apply_ops", { id, ops, actor }),
  undo: (id: string) => invoke<ApplyResult>("undo", { id }),
  redo: (id: string) => invoke<ApplyResult>("redo", { id }),
  restoreVersion: (id: string, seq: number) =>
    invoke<ApplyResult>("restore_version", { id, seq }),

  renderLayers: (id: string, scalePxPerMm: number) =>
    invoke<RenderResult>("render_layers", { id, scalePxPerMm }),
  renderOriginal: (id: string, version: number, maxPx: number) =>
    invoke<string>("render_original", { id, version, maxPx }),
  renderComposite: (id: string, maxPx: number) =>
    invoke<string>("render_composite", { id, maxPx }),

  exportProject: (id: string, options: ExportOptions, destPath?: string | null) =>
    invoke<ExportResult>("export_project", { id, options, destPath }),
  exportCheck: (id: string, dpi: number) =>
    invoke<ExportResult["warnings"]>("export_check", { id, dpi }),

  replaceOriginal: (id: string, path: string) =>
    invoke<Project>("replace_original", { id, path }),

  agentSend: (id: string, text: string, context: SendContext) =>
    invoke<string>("agent_send", { id, text, context }),
  agentStop: (id: string) => invoke<void>("agent_stop", { id }),
  agentAnswer: (id: string, questionId: string, answer: string) =>
    invoke<void>("agent_answer", { id, questionId, answer }),
  onAgentEvent: (handler: (e: AgentEvent & { project_id: string }) => void) =>
    listen<AgentEvent & { project_id: string }>("agent-event", (ev) =>
      handler(ev.payload),
    ) as Promise<UnlistenFn>,

  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (patch: {
    openai_key?: string;
    projects_dir?: string;
    agent_model?: string;
    heavy_model?: string;
    cheap_model?: string;
    image_model?: string;
  }) => invoke<Settings>("set_settings", { patch }),
  pickImageFile: () => invoke<string | null>("pick_image_file"),
  pickFolder: () => invoke<string | null>("pick_folder"),
  pickSavePath: (defaultName: string) =>
    invoke<string | null>("pick_save_path", { defaultName }),
  readImageInfo: (path: string) =>
    invoke<{ width_px: number; height_px: number; preview_png: string }>(
      "read_image_info",
      { path },
    ),
};
