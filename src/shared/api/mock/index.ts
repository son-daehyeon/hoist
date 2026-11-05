import type { Backend } from "../backend";
import { onAgentEvent } from "../events";
import type { ApplyResult, ExportOptions, ProjectSummary, Settings, UpdateInfo } from "../types";
import {
  answerScript,
  buildScript,
  editScript,
  replaceScript,
  runDemoTurn,
  runTurn,
  stopTurn,
  type Script,
} from "./agent";
import { emptyDocument, sampleLayers, SAMPLES } from "./data";
import { renderComposite } from "./raster";
import { layerSvgs } from "./svg";
import { recordAnswer, runQuestionTurn } from "./questions";
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

const fakeUpdate: UpdateInfo | null = new URLSearchParams(location.search).has("mockUpdate")
  ? { version: "0.2.0", notes: null, pub_date: null }
  : null;

const mockAgent = new URLSearchParams(location.search).has("mockAgent");

let settings: Settings = {
  openai_key_set: false,
  projects_dir: "C:\\Users\\me\\Documents\\Hoist",
  agent_model: "gpt-6.1-terra",
  heavy_model: "gpt-6.1-sol",
  cheap_model: "gpt-6-luna",
  image_model: "gpt-image-2.5-sunburst",
  usage_month_krw: 12400,
};

const calls: Record<string, number> = {};
(globalThis as { __hoistMockCalls?: Record<string, number> }).__hoistMockCalls = calls;

function counted<T extends object>(api: T): T {
  return Object.fromEntries(
    Object.entries(api).map(([name, fn]) => [
      name,
      typeof fn === "function"
        ? (...args: unknown[]) => {
            calls[name] = (calls[name] ?? 0) + 1;
            return (fn as (...a: unknown[]) => unknown)(...args);
          }
        : fn,
    ]),
  ) as T;
}

export const mockApi: Backend = counted({
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
  createProject: async ({ name, width_mm, height_mm, original_path, fit_mode }) => {
    seed();
    const p = createMock(name, width_mm, height_mm, Boolean(original_path));
    p.project.fit_mode = fit_mode ?? "ask";
    return clone(p.project);
  },
  deleteProject: async (id) => {
    get(id);
    projects.delete(id);
    return null;
  },
  renameProject: async (id, name) => {
    const p = get(id);
    p.project.name = name;
    return clone(p.project);
  },
  duplicateProject: async (id, name) => {
    const src = get(id);
    const p = createMock(name, src.project.width_mm, src.project.height_mm, Boolean(src.original));
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
    return null;
  },
  setFitMode: async (id, mode) => {
    const p = get(id);
    p.project.fit_mode = mode;
    return clone(p.project);
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
  pathBoolean: async (id, layerIds, op) => {
    const p = get(id);
    const picked = p.doc.layers.filter((l) => layerIds.includes(l.id));
    if (picked.length < 2) throw new Error("레이어를 두 개 이상 골라 주세요");
    const lo = (f: (l: (typeof picked)[number]) => number) => Math.min(...picked.map(f));
    const hi = (f: (l: (typeof picked)[number]) => number) => Math.max(...picked.map(f));
    const box =
      op === "intersect"
        ? [hi((l) => l.x_mm), hi((l) => l.y_mm), lo((l) => l.x_mm + l.w_mm), lo((l) => l.y_mm + l.h_mm)]
        : [lo((l) => l.x_mm), lo((l) => l.y_mm), hi((l) => l.x_mm + l.w_mm), hi((l) => l.y_mm + l.h_mm)];
    const name = { union: "합친 도형", subtract: "뺀 도형", intersect: "교차 도형", exclude: "제외 도형" }[
      op
    ];
    const layer = {
      ...clone(picked[0]),
      id: `L${uid()}`,
      name,
      kind: { type: "path" as const, d: "M0 0H1V1H0Z", fill_rule: "evenodd" as const },
      strategy: "native_vector" as const,
      x_mm: box[0],
      y_mm: box[1],
      w_mm: Math.max(1, box[2] - box[0]),
      h_mm: Math.max(1, box[3] - box[1]),
    };
    return change(
      p,
      () => {
        picked.forEach((l) => applyOp(p.doc, { op: "delete_layer", id: l.id }));
        applyOp(p.doc, { op: "add_layer", layer });
      },
      name,
    );
  },
  outlineText: async (id, layerId) => {
    const p = get(id);
    const l = p.doc.layers.find((x) => x.id === layerId);
    if (l?.kind.type !== "text") throw new Error("글자 레이어를 골라 주세요");
    const patch = {
      name: `${l.name} 윤곽선`,
      strategy: "traced_vector",
      kind: { type: "path", d: "M0 0H1V1H0Z", fill_rule: "nonzero" },
    };
    return change(p, () => applyOp(p.doc, { op: "update_layer", id: layerId, patch }), "글자를 윤곽선으로");
  },
  removeBackground: async (id, asset) => {
    get(id);
    await sleep(1500);
    return { asset: `cutout-${uid()}-${asset}`, width_px: 1600, height_px: 1067 };
  },
  importImage: async (id, path) => {
    get(id);
    await sleep(150);
    return {
      asset: `imported-${uid()}-${path.split(/[\\/]/).pop() ?? "image.png"}`,
      width_px: 1600,
      height_px: 1067,
    };
  },
  restoreVersion: async (id, seq) => {
    const p = get(id);
    const snap = p.snapshots.get(seq);
    if (!snap) throw new Error("기록을 찾을 수 없어요");
    return change(p, () => (p.doc = { ...clone(snap), rev: p.doc.rev }), "이전 버전으로 되돌림");
  },

  layerSvgs: async (id, have) => {
    const p = get(id);
    await sleep(20);
    return layerSvgs(p.doc, new Set(have));
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
      .map((l) => ({
        layer_id: l.id,
        layer_name: l.name,
        effective_dpi: Math.round(dpi * 0.62),
        required_dpi: dpi,
      }));
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
    if (text.includes("질문")) {
      const turn = uid();
      setTimeout(() => void runQuestionTurn(p, turn), 50);
      return turn;
    }
    if (mockAgent) {
      const turn = uid();
      setTimeout(() => void runDemoTurn(p, turn), 50);
      return turn;
    }
    const script = p.doc.layers.length === 0 && p.original ? buildScript(p) : editScript(context.layer_ids);
    return startTurn(p, script);
  },
  agentStop: async (id) => {
    stopTurn(id);
    return null;
  },
  agentAnswer: async (id, questionId, answer) => {
    const p = get(id);
    if (recordAnswer(p, questionId, answer)) startTurn(p, answerScript(answer));
    return null;
  },
  onAgentEvent,

  getSettings: async () => clone(settings),
  setSettings: async ({ openai_key, ...patch }) => {
    const given = Object.fromEntries(Object.entries(patch).filter(([, v]) => v != null));
    settings = { ...settings, ...given, openai_key_set: settings.openai_key_set || Boolean(openai_key) };
    return clone(settings);
  },
  pickImageFile: async () => "C:\\Users\\me\\Downloads\\KakaoTalk_20261031.png",
  pickFolder: async () => "C:\\Users\\me\\Documents\\Hoist",
  pickSavePath: async (defaultName) => `C:\\Users\\me\\Desktop\\${defaultName}`,
  listFonts: async () =>
    [
      {
        family: "Pretendard",
        display_name: "프리텐다드",
        category: "고딕",
        weights: [400, 500, 600, 700, 800],
      },
      { family: "Noto Sans KR", display_name: "본고딕", category: "고딕", weights: [400, 700, 900] },
      { family: "Noto Serif KR", display_name: "본명조", category: "명조", weights: [400, 700] },
      { family: "Cafe24 Ssurround OTF", display_name: "카페24 써라운드", category: "둥근", weights: [700] },
      { family: "BM Hanna Pro", display_name: "배민 한나 Pro", category: "디스플레이", weights: [400] },
      { family: "Malgun Gothic", display_name: null, category: null, weights: [400, 700] },
    ].map((f) => ({
      ...f,
      bundled: f.display_name !== null,
      files: f.weights.map((weight) => ({
        weight,
        style: "normal" as const,
        path: f.family === "Pretendard" ? "/fonts/PretendardVariable.woff2" : "",
      })),
    })),
  readImageInfo: async () => {
    const doc = { ...emptyDocument(2172, 724), layers: sampleLayers(2172, 724, SAMPLES[0].titles) };
    return { width_px: 2172, height_px: 724, preview_png: renderComposite(doc, 640) };
  },

  appVersion: async () => "0.1.0",
  updateCheck: async () => {
    await sleep(400);
    return fakeUpdate;
  },
  updateInstall: async (onProgress) => {
    if (!fakeUpdate) throw new Error("새 버전이 없어요");
    const total = 48_000_000;
    onProgress({ type: "started", total });
    for (let downloaded = 0; downloaded < total;) {
      await sleep(120);
      downloaded = Math.min(total, downloaded + 4_000_000);
      onProgress({ type: "progress", downloaded, total });
    }
    onProgress({ type: "finished" });
  },
  onUpdateAvailable: (handler) => {
    if (!fakeUpdate) return () => {};
    const t = setTimeout(() => handler({ version: fakeUpdate.version, notes: fakeUpdate.notes }), 1500);
    return () => clearTimeout(t);
  },
});
