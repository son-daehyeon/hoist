import type { api, ApplyResult } from "../tauri";
import type { ExportOptions, ProjectSummary, Settings } from "../types";
import {
  answerScript,
  buildScript,
  editScript,
  listeners,
  replaceScript,
  runTurn,
  stopTurn,
  type Script,
} from "./agent";
import { emptyDocument, sampleLayers, SAMPLES } from "./data";
import { effectiveScale, renderComposite, renderLayers } from "./raster";
import { applyOp, createMock, projects, seed, snapshot, uid, type MockProject } from "./store";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const clone = structuredClone;

function get(id: string): MockProject {
  seed();
  const p = projects.get(id);
  if (!p) throw new Error("프로젝트를 찾을 수 없어요");
  return p;
}

function result(p: MockProject): ApplyResult {
  return { document: clone(p.doc), entries: [], can_undo: p.undo.length > 0, can_redo: p.redo.length > 0 };
}

function change(p: MockProject, next: () => void, summary: string) {
  p.undo.push(clone(p.doc));
  p.redo = [];
  next();
  p.doc.rev += 1;
  snapshot(p, "user", summary);
  return result(p);
}

function startTurn(p: MockProject, script: Script, delay = 50) {
  const turn = uid();
  setTimeout(() => void runTurn(p, script, turn), delay);
  return turn;
}

let settings: Settings = {
  openai_key_set: false,
  projects_dir: "C:\\Users\\me\\Documents\\Banner Studio",
  agent_model: "gpt-6.1-terra",
  heavy_model: "gpt-6.1-sol",
  cheap_model: "gpt-6-luna",
  image_model: "gpt-image-2.5-sunburst",
  usage_month_krw: 12400,
};

export const mockApi: typeof api = {
  listProjects: async () => {
    seed();
    await sleep(60);
    return [...projects.values()].map(({ project: pr, doc }): ProjectSummary => ({
      id: pr.id,
      name: pr.name,
      width_mm: pr.width_mm,
      height_mm: pr.height_mm,
      updated_at: pr.updated_at,
      thumbnail_png: renderComposite(doc, 480),
    }));
  },
  createProject: async ({ name, width_mm, height_mm, original_path }) => {
    seed();
    const p = createMock(name, width_mm, height_mm, Boolean(original_path));
    return clone(p.project);
  },
  deleteProject: async (id) => {
    get(id);
    projects.delete(id);
  },
  renameProject: async (id, name) => {
    const p = get(id);
    p.project.name = name;
    return clone(p.project);
  },
  duplicateProject: async (id) => {
    const src = get(id);
    const p = createMock(
      `${src.project.name} 사본`,
      src.project.width_mm,
      src.project.height_mm,
      Boolean(src.original),
    );
    p.doc = clone(src.doc);
    snapshot(p, "user", "복제");
    return clone(p.project);
  },
  openProject: async (id) => {
    const p = get(id);
    await sleep(80);
    return clone({
      project: p.project,
      document: p.doc,
      memory: p.memory,
      chat: p.chat,
      versions: p.versions,
      can_undo: p.undo.length > 0,
      can_redo: p.redo.length > 0,
    });
  },
  showInFolder: async (id) => {
    get(id);
  },

  applyOps: async (id, ops, actor = "user") => {
    const p = get(id);
    return change(
      p,
      () => ops.forEach((op) => applyOp(p.doc, op)),
      `${actor === "user" ? "직접 편집" : "편집"} ${ops.length}건`,
    );
  },
  undo: async (id) => {
    const p = get(id);
    const prev = p.undo.pop();
    if (prev) {
      p.redo.push(clone(p.doc));
      p.doc = { ...prev, rev: p.doc.rev + 1 };
    }
    return result(p);
  },
  redo: async (id) => {
    const p = get(id);
    const next = p.redo.pop();
    if (next) {
      p.undo.push(clone(p.doc));
      p.doc = { ...next, rev: p.doc.rev + 1 };
    }
    return result(p);
  },
  restoreVersion: async (id, seq) => {
    const p = get(id);
    const snap = p.snapshots.get(seq);
    if (!snap) throw new Error("기록을 찾을 수 없어요");
    return change(p, () => (p.doc = { ...clone(snap), rev: p.doc.rev }), "이전 버전으로 되돌림");
  },

  renderLayers: async (id, scalePxPerMm) => {
    const p = get(id);
    await sleep(40);
    const s = effectiveScale(p.doc, scalePxPerMm);
    const { width_mm: W, height_mm: H } = p.doc.artboard;
    return {
      scale_px_per_mm: s,
      width_px: Math.round(W * s),
      height_px: Math.round(H * s),
      layers: renderLayers(p.doc, s),
    };
  },
  renderOriginal: async (id, _version, maxPx) => {
    const p = get(id);
    return renderComposite(p.original ?? p.doc, maxPx);
  },
  renderComposite: async (id, maxPx) => renderComposite(get(id).doc, maxPx),

  exportProject: async (id, options: ExportOptions, destPath) => {
    const p = get(id);
    await sleep(700);
    const { width_mm: W, height_mm: H } = p.doc.artboard;
    const name = `${p.project.name.replace(/\s+/g, "_")}_${options.scale === 10 ? "1to10" : "1to1"}.${options.format}`;
    return {
      path: destPath ?? `C:\\Users\\me\\Desktop\\${name}`,
      width_px: Math.round((W / 25.4) * options.dpi),
      height_px: Math.round((H / 25.4) * options.dpi),
      bytes: 48_000_000,
      warnings: [],
    };
  },
  exportCheck: async (id, dpi) => {
    const p = get(id);
    await sleep(120);
    if (dpi <= 100) return [];
    return p.doc.layers
      .filter((l) => l.kind.type === "image" && l.strategy !== "background")
      .map((l) => ({ layer_id: l.id, layer_name: l.name, effective_dpi: Math.round(dpi * 0.62) }));
  },

  replaceOriginal: async (id, _path) => {
    const p = get(id);
    const version = (p.project.originals.at(-1)?.version ?? 0) + 1;
    p.project.originals.push({
      version,
      file: `originals/v${version}.png`,
      width_px: 2172,
      height_px: 724,
      uploaded_at: new Date().toISOString(),
    });
    startTurn(p, replaceScript, 300);
    return clone(p.project);
  },

  agentSend: async (id, text, context) => {
    const p = get(id);
    p.chat.push({ id: uid(), role: "user", ts: new Date().toISOString(), text, context });
    const script = p.doc.layers.length === 0 && p.original ? buildScript(p) : editScript(context.layer_ids);
    return startTurn(p, script);
  },
  agentStop: async (id) => stopTurn(id),
  agentAnswer: async (id, questionId, answer) => {
    const p = get(id);
    const item = p.chat.find((c) => c.question?.id === questionId);
    if (item) item.answer = answer;
    startTurn(p, answerScript(answer));
  },
  onAgentEvent: async (handler) => {
    listeners.add(handler);
    return () => {
      listeners.delete(handler);
    };
  },

  getSettings: async () => clone(settings),
  setSettings: async ({ openai_key, ...patch }) => {
    settings = { ...settings, ...patch, openai_key_set: settings.openai_key_set || Boolean(openai_key) };
    return clone(settings);
  },
  pickImageFile: async () => "C:\\Users\\me\\Downloads\\KakaoTalk_20261031.png",
  pickFolder: async () => "C:\\Users\\me\\Documents\\Banner Studio",
  pickSavePath: async (defaultName) => `C:\\Users\\me\\Desktop\\${defaultName}`,
  readImageInfo: async () => {
    const doc = { ...emptyDocument(2172, 724), layers: sampleLayers(2172, 724, SAMPLES[0].titles) };
    return { width_px: 2172, height_px: 724, preview_png: renderComposite(doc, 640) };
  },
};
