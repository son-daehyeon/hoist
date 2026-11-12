import type { Backend } from "../backend";
import type { ApplyResult } from "../types";
import { applyOp, getProject, sleep, snapshot, uid, type MockProject } from "./store";

function result(p: MockProject): ApplyResult {
  return {
    document: structuredClone(p.doc),
    entries: [],
    can_undo: p.undo.length > 0,
    can_redo: p.redo.length > 0,
  };
}

function change(p: MockProject, next: () => void, summary: string) {
  p.undo.push(structuredClone(p.doc));
  p.redo = [];
  next();
  p.doc.rev += 1;
  snapshot(p, "user", summary);
  return result(p);
}

export const editing = {
  applyOps: async (id, ops, actor = "user") => {
    const p = getProject(id);
    return change(
      p,
      () => ops.forEach((op) => applyOp(p.doc, op)),
      `${actor === "user" ? "직접 편집" : "편집"} ${ops.length}건`,
    );
  },
  undo: async (id) => {
    const p = getProject(id);
    const prev = p.undo.pop();
    if (prev) {
      p.redo.push(structuredClone(p.doc));
      p.doc = { ...prev, rev: p.doc.rev + 1 };
    }
    return result(p);
  },
  redo: async (id) => {
    const p = getProject(id);
    const next = p.redo.pop();
    if (next) {
      p.undo.push(structuredClone(p.doc));
      p.doc = { ...next, rev: p.doc.rev + 1 };
    }
    return result(p);
  },
  pathBoolean: async (id, layerIds, op) => {
    const p = getProject(id);
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
      ...structuredClone(picked[0]),
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
    const p = getProject(id);
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
    getProject(id);
    await sleep(1500);
    return { asset: `cutout-${uid()}-${asset}`, width_px: 1600, height_px: 1067 };
  },
  importImage: async (id, path) => {
    getProject(id);
    await sleep(150);
    return {
      asset: `imported-${uid()}-${path.split(/[\\/]/).pop() ?? "image.png"}`,
      width_px: 1600,
      height_px: 1067,
    };
  },
  restoreVersion: async (id, seq) => {
    const p = getProject(id);
    const snap = p.snapshots.get(seq);
    if (!snap) throw new Error("기록을 찾을 수 없어요");
    return change(p, () => (p.doc = { ...structuredClone(snap), rev: p.doc.rev }), "이전 버전으로 되돌림");
  },
} satisfies Partial<Backend>;
