import { getVersion } from "@tauri-apps/api/app";
import { Channel, convertFileSrc } from "@tauri-apps/api/core";
import { commands, events } from "./bindings";
import { emitAgentEvent, onAgentEvent } from "./events";
import { mockApi } from "./mock";
import type {
  LayerSvgs,
  Actor,
  AgentEvent,
  ChatItem,
  ExportOptions,
  Op,
  OpenedProject,
  SendContext,
  UpdateAvailable,
  UpdateProgress,
} from "./types";

export const isTauri = "__TAURI_INTERNALS__" in window;

function agentStream() {
  const channel = new Channel<AgentEvent>();
  channel.onmessage = emitAgentEvent;
  return channel;
}

function withAssetUrls(r: LayerSvgs): LayerSvgs {
  const dir = /[\\/]$/.test(r.assets_dir) ? r.assets_dir : `${r.assets_dir}/`;
  const url = (name: string) => convertFileSrc(`${dir}${name}`);
  return {
    ...r,
    layers: r.layers.map((l) => ({
      ...l,
      svg: l.svg && l.svg.replace(/href="asset:([^"]+)"/g, (_, name: string) => `href="${url(name)}"`),
    })),
  };
}

const tauriApi = {
  listProjects: commands.listProjects,
  createProject: commands.createProject,
  deleteProject: commands.deleteProject,
  renameProject: commands.renameProject,
  duplicateProject: commands.duplicateProject,
  openProject: (id: string) =>
    commands.openProject(id).then((o): OpenedProject => ({ ...o, chat: o.chat as unknown as ChatItem[] })),
  showInFolder: commands.showInFolder,
  setFitMode: commands.setFitMode,

  applyOps: (id: string, ops: Op[], actor: Actor = "user") => commands.applyOps(id, ops, actor),
  undo: commands.undo,
  redo: commands.redo,
  restoreVersion: commands.restoreVersion,
  pathBoolean: commands.pathBoolean,
  outlineText: commands.outlineText,
  importImage: commands.importImage,
  removeBackground: commands.removeBackground,

  layerSvgs: (id: string, have: string[]) => commands.layerSvgs(id, have).then(withAssetUrls),
  renderOriginal: commands.renderOriginal,
  renderComposite: commands.renderComposite,

  exportProject: (id: string, options: ExportOptions, destPath: string | null = null) =>
    commands.exportProject(id, options, destPath),
  exportCheck: commands.exportCheck,

  replaceOriginal: (id: string, path: string) => commands.replaceOriginal(id, path, agentStream()),
  agentSend: (id: string, text: string, context: SendContext) =>
    commands.agentSend(id, text, context, agentStream()),
  agentStop: commands.agentStop,
  agentAnswer: commands.agentAnswer,
  onAgentEvent,

  getSettings: commands.getSettings,
  setSettings: commands.setSettings,
  pickImageFile: commands.pickImageFile,
  pickFolder: commands.pickFolder,
  pickSavePath: commands.pickSavePath,
  readImageInfo: commands.readImageInfo,
  listFonts: commands.listFonts,

  appVersion: getVersion,
  updateCheck: commands.updateCheck,
  updateInstall: (onProgress: (progress: UpdateProgress) => void): Promise<void> => {
    const channel = new Channel<UpdateProgress>();
    channel.onmessage = onProgress;
    return commands.updateInstall(channel).then(() => {});
  },
  onUpdateAvailable: (handler: (update: UpdateAvailable) => void) => {
    let off: (() => void) | undefined;
    let alive = true;
    void events.updateAvailable.listen((e) => handler(e.payload)).then((fn) => (alive ? (off = fn) : fn()));
    return () => {
      alive = false;
      off?.();
    };
  },
};

export type Backend = typeof tauriApi;

export const api: Backend = isTauri ? tauriApi : mockApi;
