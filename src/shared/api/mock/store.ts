import { childrenOf } from "../../lib";
import type { Actor, ChatItem, Document, Layer, Op, Project, Report, Version } from "../types";
import { emptyDocument, gridLayers, sampleLayers, SAMPLES } from "./data";
import { mockLayerCount } from "./devHooks";
import { renderComposite } from "./raster";

export interface MockProject {
  project: Project;
  doc: Document;
  original: Document | null;
  chat: ChatItem[];
  versions: Version[];
  snapshots: Map<number, Document>;
  undo: Document[];
  redo: Document[];
  memory: string;
}

export const projects = new Map<string, MockProject>();
export const uid = () => Math.random().toString(36).slice(2, 10);
export const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const iso = (msAgo = 0) => new Date(Date.now() - msAgo).toISOString();

export const sampleReport = (
  original: Document,
  work: Document,
  promoted: number,
  summary: string,
): Report => ({
  original_png: renderComposite(original, 292),
  work_png: renderComposite(work, 292),
  overlap: 0.987,
  similarity: 0.95,
  min_dpi: 120,
  background_min_dpi: 61,
  dpi_ok: true,
  promoted,
  summary,
});

export function snapshot(p: MockProject, actor: Actor, summary: string, turn?: string) {
  const seq = (p.versions.at(-1)?.seq ?? 0) + 1;
  p.snapshots.set(seq, structuredClone(p.doc));
  p.versions.push({
    seq,
    ts: iso(),
    actor,
    summary,
    thumbnail_png: renderComposite(p.doc, 112),
    turn: turn ?? null,
  });
  p.project.updated_at = iso();
}

export function createMock(
  name: string,
  w: number,
  h: number,
  withOriginal: boolean,
  msAgo = 0,
): MockProject {
  const id = uid();
  const sample =
    SAMPLES.find((s) => s.name === name)?.titles ??
    ([name, "참여자 모집", "2026년 하반기"] as [string, string, string]);
  const full: Document = { ...emptyDocument(w, h), layers: sampleLayers(w, h, sample) };
  const p: MockProject = {
    project: {
      id,
      name,
      created_at: iso(msAgo),
      updated_at: iso(msAgo),
      width_mm: w,
      height_mm: h,
      target_dpi: 100,
      usage_krw: 0,
      usage_text_krw: 0,
      usage_image_krw: 0,
      fit_mode: "ask",
      originals: withOriginal
        ? [{ version: 1, file: "originals/v1.png", width_px: 2172, height_px: 724, uploaded_at: iso(msAgo) }]
        : [],
    },
    doc: emptyDocument(w, h),
    original: withOriginal ? full : null,
    chat: [],
    versions: [],
    snapshots: new Map(),
    undo: [],
    redo: [],
    memory: "",
  };
  projects.set(id, p);
  return p;
}

export function seed() {
  if (projects.size) return;
  const extra = mockLayerCount();
  SAMPLES.forEach((s, i) => {
    const p = createMock(s.name, s.w, s.h, true, s.daysAgo * 86_400_000 + 3_600_000);
    p.doc = structuredClone(p.original ?? p.doc);
    if (i === 0 && extra > 0) p.doc.layers.push(...gridLayers(extra, s.w, s.h));
    p.doc.rev = 1;
    snapshot(p, "user", "원본 v1 올림");
    p.chat.push(
      { id: uid(), role: "user", ts: iso(), text: "원본이랑 똑같이 만들어줘" },
      {
        id: uid(),
        role: "agent",
        ts: iso(),
        text: "다 만들었어요. 원본과 비교한 결과예요. 파란 붓터치 띠는 벡터로는 붓 질감이 안 나와서 고해상도 그림으로 바꿨어요.",
        report: sampleReport(p.doc, p.doc, 1, ""),
        actions: [
          { id: uid(), label: "원본 보기", status: "done", round: 1 },
          { id: uid(), label: "원본 분석 · 글자 영역", status: "done", round: 1 },
          { id: uid(), label: "레이어 추가 · 제목", status: "done", round: 2 },
          { id: uid(), label: "레이어 추가 · 부제", status: "done", round: 2 },
          { id: uid(), label: "그림 만들기 · 파란 붓터치", status: "done", round: 3 },
        ],
        turn_krw: 2480,
        thinking_sec: 214,
      },
    );
    snapshot(p, "agent", "첫 재구성");
    p.project.usage_krw = 2480;
    p.project.usage_text_krw = 2160;
    p.project.usage_image_krw = 320;
    p.project.updated_at = iso(s.daysAgo * 86_400_000 + 3_600_000);
  });
}

export function getProject(id: string): MockProject {
  seed();
  const p = projects.get(id);
  if (!p) throw new Error("프로젝트를 찾을 수 없어요");
  return p;
}

function mergePatch(target: unknown, patch: unknown): unknown {
  if (typeof patch !== "object" || patch === null || Array.isArray(patch)) return patch;
  const out: Record<string, unknown> =
    typeof target === "object" && target !== null && !Array.isArray(target)
      ? { ...(target as Record<string, unknown>) }
      : {};
  for (const [k, v] of Object.entries(patch)) {
    if (v === null) delete out[k];
    else out[k] = mergePatch(out[k], v);
  }
  return out;
}

function descendants(layers: Layer[], id: string): string[] {
  return childrenOf(layers, id).flatMap((c) => [c.id, ...descendants(layers, c.id)]);
}

export function applyOp(doc: Document, op: Op) {
  switch (op.op) {
    case "add_layer":
      doc.layers.splice(op.index ?? doc.layers.length, 0, op.layer as Layer);
      break;
    case "update_layer":
      doc.layers = doc.layers.map((l) => (l.id === op.id ? (mergePatch(l, op.patch) as Layer) : l));
      break;
    case "delete_layer": {
      const gone = new Set([op.id, ...descendants(doc.layers, op.id)]);
      doc.layers = doc.layers.filter((l) => !gone.has(l.id));
      break;
    }
    case "move_layer": {
      const layer = doc.layers.find((l) => l.id === op.id);
      if (!layer) return;
      const rest = doc.layers.filter((l) => l.id !== op.id);
      const moved = { ...layer, parent: op.parent ?? null };
      const siblings = childrenOf(rest, moved.parent ?? null);
      const before = siblings[op.index];
      const after = siblings.at(-1);
      const at = before ? rest.indexOf(before) : after ? rest.indexOf(after) + 1 : rest.length;
      rest.splice(at, 0, moved);
      doc.layers = rest;
      break;
    }
    case "set_artboard":
      doc.artboard = mergePatch(doc.artboard, op.patch) as Document["artboard"];
      break;
  }
}
