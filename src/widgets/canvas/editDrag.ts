import type { Box } from "@/entities/layer";
import { parsePath, scale, serializePath, type Pt } from "@/entities/path";
import type { Document, Layer, LayerId, Op } from "@/shared/api";
import { isEffectivelyHidden, round1 } from "@/shared/lib";
import {
  aabb,
  centerOf,
  HANDLE_UV,
  lockAxis,
  mapInto,
  moved,
  pointOn,
  resizedFrame,
  rotatedAbout,
  unplaced,
  type Frame,
  type Handle,
} from "./geometry";
import { frameFor, frameOf, hasAncestor, movable, parentMatrix, type Edits } from "./selection";
import { guidesFor, offsetTo, snapLines, snapMove, snapResize, type Guide, type SnapLines } from "./snap";

const SNAP_PX = 6;
const ROTATE_STEP = 15;

export type EditDrag = {
  kind: "edit";
  mode: "move" | "resize" | "rotate";
  handle: Handle | null;
  start: Pt;
  frame0: Frame;
  frame: Frame;
  lines: SnapLines;
  guides: Guide[];
  edits: Edits;
};

const normDeg = (d: number) => round1((((d % 360) + 540) % 360) - 180);
export const mapValues = (o: Record<LayerId, Frame>, fn: (p: Frame) => Frame) =>
  Object.fromEntries(Object.entries(o).map(([k, v]) => [k, fn(v)]));
const asBox = ({ x, y, w, h }: Frame): Box => ({ x, y, w, h });

function linesExcept({ layers, artboard }: Document, ids: LayerId[]) {
  const moving = new Set(movable(layers, ids).map((l) => l.id));
  const others = layers.filter(
    (l) => !moving.has(l.id) && !hasAncestor(layers, l, moving) && !isEffectivelyHidden(layers, l),
  );
  return snapLines(
    artboard.width_mm,
    artboard.height_mm,
    others.map((l) => aabb(frameOf(layers, l))),
  );
}

export function startEditDrag(
  doc: Document,
  o: { mode: EditDrag["mode"]; handle: Handle | null; ids: LayerId[]; start: Pt; copy: boolean },
): EditDrag | null {
  const { layers } = doc;
  const base = Object.fromEntries(movable(layers, o.ids).map((l) => [l.id, frameOf(layers, l)]));
  const frame0 = frameFor(Object.values(base));
  if (!frame0) return null;
  return {
    kind: "edit",
    mode: o.mode,
    handle: o.handle,
    start: o.start,
    frame0,
    frame: frame0,
    lines: linesExcept(doc, o.ids),
    guides: [],
    edits: { base, next: base, copy: o.copy },
  };
}

export function moveEditDrag(
  drag: EditDrag,
  p: Pt,
  o: { z: number; shift: boolean; alt: boolean; toWorld: (p: Pt) => Pt },
): EditDrag {
  const { mode, handle, frame0, lines } = drag;
  const { base } = drag.edits;
  const snap = drag.edits.copy || !o.alt;
  const tolerance = SNAP_PX / o.z;
  const dxm = (p.x - drag.start.x) / o.z;
  const dym = (p.y - drag.start.y) / o.z;
  let frame: Frame;
  let next: Record<LayerId, Frame>;
  if (mode === "move") {
    let [dx, dy] = o.shift ? lockAxis(dxm, dym) : [dxm, dym];
    if (snap) {
      const box = aabb(moved(frame0, dx, dy));
      const s = snapMove(box, lines, tolerance);
      dx += o.shift && dx === 0 ? 0 : s.x - box.x;
      dy += o.shift && dy === 0 ? 0 : s.y - box.y;
    }
    frame = moved(frame0, dx, dy);
    next = mapValues(base, (q) => moved(q, dx, dy));
  } else if (mode === "resize" && handle) {
    frame = resizedFrame(frame0, handle, dxm, dym, {
      keepRatio: o.shift,
      snap: snap && frame0.deg === 0 ? (b) => snapResize(b, handle, lines, tolerance) : undefined,
      snapTurned:
        snap && frame0.deg !== 0
          ? { pull: (axis, v) => offsetTo(v, axis === "x" ? lines.xs : lines.ys, tolerance), tolerance }
          : undefined,
    });
    const ids = Object.keys(base);
    next =
      ids.length === 1
        ? { [ids[0]]: frame }
        : mapValues(base, (q) => mapInto(q, asBox(frame0), asBox(frame)));
  } else {
    const c = centerOf(frame0);
    const angle = (q: Pt) => {
      const w = o.toWorld(q);
      return (Math.atan2(w.y - c.y, w.x - c.x) * 180) / Math.PI;
    };
    let d = angle(p) - angle(drag.start);
    if (o.shift) d = Math.round((frame0.deg + d) / ROTATE_STEP) * ROTATE_STEP - frame0.deg;
    frame = rotatedAbout(frame0, c, d);
    next = mapValues(base, (q) => rotatedAbout(q, c, d));
  }
  const tip = mode === "resize" && handle && frame.deg !== 0 ? pointOn(frame, ...HANDLE_UV[handle]) : null;
  const marks = tip ? { ...tip, w: 0, h: 0 } : aabb(frame);
  const guides = snap && mode !== "rotate" ? guidesFor(marks, lines) : [];
  return { ...drag, frame, guides, edits: { ...drag.edits, next } };
}

export function editOps(layers: Layer[], edits: Edits): { ops: Op[]; copyAt: Record<LayerId, Pt> } {
  const byId = new Map(layers.map((l) => [l.id, l]));
  const ops: Op[] = [];
  const copyAt: Record<LayerId, Pt> = {};
  for (const [id, next] of Object.entries(edits.next)) {
    const base = edits.base[id];
    const layer = byId.get(id);
    if (!base || !layer) continue;
    const local = unplaced(parentMatrix(layers, layer), next);
    if (edits.copy) {
      if (round1(next.x) !== round1(base.x) || round1(next.y) !== round1(base.y))
        copyAt[id] = { x: round1(local.x), y: round1(local.y) };
      continue;
    }
    const patch: Record<string, number | { d: string }> = {};
    const sized = round1(next.w) !== round1(base.w) || round1(next.h) !== round1(base.h);
    if (sized || round1(next.x) !== round1(base.x) || round1(next.y) !== round1(base.y)) {
      patch.x_mm = round1(local.x);
      patch.y_mm = round1(local.y);
    }
    if (sized) {
      patch.w_mm = round1(next.w);
      patch.h_mm = round1(next.h);
      if (layer.kind.type === "path" && base.w > 0 && base.h > 0) {
        const d = scale(parsePath(layer.kind.d), next.w / base.w, next.h / base.h, { x: 0, y: 0 });
        patch.kind = { d: serializePath(d) };
      }
    }
    if (round1(next.deg) !== round1(base.deg)) patch.rotation_deg = normDeg(local.deg);
    if (Object.keys(patch).length) ops.push({ op: "update_layer", id, patch });
  }
  return { ops, copyAt };
}
