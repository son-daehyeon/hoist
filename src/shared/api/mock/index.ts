import type { Backend } from "../backend";
import { onAgentEvent } from "../events";
import type { ExportOptions, Settings } from "../types";
import {
  answerScript,
  answerTurn,
  buildScript,
  editScript,
  replaceScript,
  runDemoTurn,
  runTurn,
  stopTurn,
  type Script,
} from "./agent";
import { emptyDocument, sampleFonts, sampleLayers, SAMPLES } from "./data";
import { counted, fakeUpdate, mockAgent } from "./devHooks";
import { editing } from "./editing";
import { library } from "./library";
import { renderComposite } from "./raster";
import { layerSvgs } from "./svg";
import { recordAnswer, runQuestionTurn } from "./questions";
import { getProject, sleep, uid, type MockProject } from "./store";

function startTurn(p: MockProject, script: Script, delay = 50) {
  const turn = uid();
  setTimeout(() => void runTurn(p, script, turn), delay);
  return turn;
}

let settings: Settings = {
  openai_key_set: false,
  projects_dir: "C:\\Users\\me\\Documents\\Hoist",
  agent_model: "gpt-6.1-terra",
  heavy_model: "gpt-6.1-sol",
  cheap_model: "gpt-6-luna",
  image_model: "gpt-image-2.6-sunburst",
};

export const mockApi: Backend = counted({
  ...library,
  ...editing,

  openProject: async (id) => {
    const p = getProject(id);
    await sleep(80);
    return structuredClone({
      project: p.project,
      document: p.doc,
      memory: p.memory,
      chat: p.chat,
      versions: [...p.versions].reverse(),
      can_undo: p.undo.length > 0,
      can_redo: p.redo.length > 0,
    });
  },

  layerSvgs: async (id, have) => {
    const p = getProject(id);
    await sleep(20);
    return layerSvgs(p.doc, new Set(have));
  },
  renderOriginal: async (id, _version, maxPx) => {
    const p = getProject(id);
    return renderComposite(p.original ?? p.doc, maxPx);
  },
  renderComposite: async (id, maxPx) => renderComposite(getProject(id).doc, maxPx),

  exportProject: async (id, options: ExportOptions, destPath) => {
    const p = getProject(id);
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
    const p = getProject(id);
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
    const p = getProject(id);
    const version = (p.project.originals.at(-1)?.version ?? 0) + 1;
    p.project.originals.push({
      version,
      file: `originals/v${version}.png`,
      width_px: 2172,
      height_px: 724,
      uploaded_at: new Date().toISOString(),
    });
    startTurn(p, replaceScript, 300);
    return structuredClone(p.project);
  },

  agentSend: async (id, text, context) => {
    const p = getProject(id);
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
    const p = getProject(id);
    if (recordAnswer(p, questionId, answer)) {
      answerTurn(id);
      startTurn(p, answerScript(answer));
    }
    return null;
  },
  onAgentEvent,

  getSettings: async () => structuredClone(settings),
  apiKeyStatus: async () => "ok",
  setSettings: async ({ openai_key, ...patch }) => {
    const given = Object.fromEntries(Object.entries(patch).filter(([, v]) => v != null));
    settings = { ...settings, ...given, openai_key_set: settings.openai_key_set || Boolean(openai_key) };
    return structuredClone(settings);
  },
  pickImageFile: async () => "C:\\Users\\me\\Downloads\\KakaoTalk_20261031.png",
  tutorialSample: async () => "C:/Users/me/AppData/Local/Temp/practice.png",
  pickFolder: async () => "C:\\Users\\me\\Documents\\Hoist",
  pickSavePath: async (defaultName) => `C:\\Users\\me\\Desktop\\${defaultName}`,
  listFonts: async () => sampleFonts(),
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
  onOpenProject: () => () => {},
  popupBusyProjects: async () => [],
  popupOpen: async () => {},
  popupHide: async () => {},
  popupQuit: async () => {},
  onPopupShown: () => () => {},
  onNotice: () => () => {},
  onUpdateAvailable: (handler) => {
    const update = fakeUpdate;
    if (!update) return () => {};
    const t = setTimeout(() => handler({ version: update.version, notes: update.notes }), 1500);
    return () => clearTimeout(t);
  },
});
