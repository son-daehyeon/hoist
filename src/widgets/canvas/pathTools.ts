import { useEffect, useEffectEvent, useState, type PointerEvent } from "react";
import { useEd } from "@/entities/editor";
import { lastColor, pathLayer } from "@/entities/layer";
import {
  appendAnchor,
  closeSubpath,
  insertPoint,
  moveAnchor,
  moveHandle,
  nearestOnPath,
  normalize,
  parsePath,
  removeAnchor,
  serializePath,
  toggleKind,
  type Anchor,
  type Path,
  type Pt,
} from "@/entities/path";
import type { Layer, LayerId } from "@/shared/api";
import { centerOf, refit, rotatePoint, type Params } from "./drag";
import type { PathSel } from "./PathView";
import { localOf, paramsOf } from "./selection";
import type { Panes } from "./viewport";

const CLOSE_PX = 6;
const HIT_PX = 6;
const BEND_PX = 2;
const MIN_MM = 0.1;
const WORLD: Params = { x: 0, y: 0, w: 0, h: 0, deg: 0 };
const TOOL_KEYS = new Set(["v", "h", "t", "r", "c"]);

type Grab =
  | { kind: "pen"; ai: number }
  | { kind: "anchor"; si: number; ai: number; start: Pt; path0: Path }
  | { kind: "handle"; si: number; ai: number; side: "in" | "out"; start: Pt; path0: Path };

const corner = (p: Pt): Anchor => ({ p, hIn: null, hOut: null, kind: "corner" });
const exact = (v: number) => Math.round(v * 100) / 100;
const filled = (path: Path): Path => ({ subpaths: path.subpaths.filter((s) => s.anchors.length > 1) });
const solidFill = (l: Layer) => (l.fills[0]?.type === "solid" ? l.fills[0].color : null);

export function usePathTools(panes: Panes, z: number) {
  const { doc, tool, ops, editing, setEditing, setSelection, locked } = useEd();
  const [pen, setPen] = useState<{ path: Path; hover: Pt | null } | null>(null);
  const [edit, setEdit] = useState<{ id: LayerId; path: Path; d0: string; sel: PathSel } | null>(null);
  const [grab, setGrab] = useState<Grab | null>(null);

  const layers = doc?.layers ?? [];
  const toWorld = (p: Pt): Pt => ({ x: (p.x - panes.work.x) / z, y: (p.y - panes.work.y) / z });
  const editLayer =
    editing?.kind === "path" && edit?.id === editing.id ? layers.find((l) => l.id === edit.id) : undefined;
  const active = editLayer && edit && !editing?.pending ? edit : null;
  const frame = editLayer ? paramsOf(layers, editLayer) : WORLD;
  const toLocal = (p: Pt) => {
    const q = rotatePoint(toWorld(p), centerOf(frame), -frame.deg);
    return { x: q.x - frame.x, y: q.y - frame.y };
  };
  const localDelta = (from: Pt, to: Pt) =>
    rotatePoint({ x: (to.x - from.x) / z, y: (to.y - from.y) / z }, { x: 0, y: 0 }, -frame.deg);

  const finishPen = (draft = pen) => {
    setPen(null);
    setGrab(null);
    const path = draft && filled(draft.path);
    if (!doc || !path?.subpaths.length) return;
    const n = normalize(path);
    const box = { x: n.x, y: n.y, w: Math.max(n.w, MIN_MM), h: Math.max(n.h, MIN_MM) };
    const closed = path.subpaths.some((s) => s.closed);
    const layer = pathLayer(doc, serializePath(n.path), box, closed, lastColor());
    void ops([{ op: "add_layer", layer }]).then((ok) => ok && setSelection([layer.id]));
  };

  const enter = (l: Layer) => {
    if (l.kind.type !== "path") return false;
    const path = parsePath(l.kind.d);
    setEdit({ id: l.id, path, d0: serializePath(path), sel: null });
    setEditing({ kind: "path", id: l.id });
    setSelection([l.id]);
    return true;
  };

  const commitPath = () => {
    if (!active || !editLayer) return setEditing(null);
    const id = editLayer.id;
    if (serializePath(active.path) === active.d0) return setEditing(null);
    const path = filled(active.path);
    setEditing({ kind: "path", id, pending: true });
    if (!path.subpaths.length) {
      void ops([{ op: "delete_layer", id }]).then((ok) => ok || setEditing(null));
      return;
    }
    const n = normalize(path);
    const box = { x: n.x, y: n.y, w: Math.max(n.w, MIN_MM), h: Math.max(n.h, MIN_MM) };
    const f = refit(localOf(editLayer), box);
    const patch = {
      kind: { d: serializePath(n.path) },
      x_mm: exact(f.x),
      y_mm: exact(f.y),
      w_mm: exact(box.w),
      h_mm: exact(box.h),
    };
    void ops([{ op: "update_layer", id, patch }]).then((ok) => ok || setEditing(null));
  };

  const down = (e: PointerEvent<HTMLDivElement>, p: Pt, start: () => void): boolean => {
    if (tool === "pen") {
      if (locked || !doc) return true;
      const w = toWorld(p);
      const anchors = pen?.path.subpaths[0].anchors ?? [];
      const first = anchors[0]?.p;
      if (pen && first && anchors.length >= 2 && Math.hypot(w.x - first.x, w.y - first.y) * z <= CLOSE_PX) {
        finishPen({ ...pen, path: closeSubpath(pen.path, 0) });
        return true;
      }
      const path = pen
        ? appendAnchor(pen.path, 0, corner(w))
        : { subpaths: [{ closed: false, anchors: [corner(w)] }] };
      setPen({ path, hover: w });
      setGrab({ kind: "pen", ai: anchors.length });
      start();
      return true;
    }
    if (!active) return false;
    const t = (e.target as HTMLElement).dataset;
    if (t.anchor) {
      const [si, ai] = t.anchor.split(":").map(Number);
      if (e.altKey) {
        setEdit({ ...active, path: toggleKind(active.path, si, ai), sel: [si, ai] });
        return true;
      }
      setEdit({ ...active, sel: [si, ai] });
      setGrab({ kind: "anchor", si, ai, start: p, path0: active.path });
      start();
      return true;
    }
    if (t.hpt) {
      const [si, ai, side] = t.hpt.split(":");
      setGrab({
        kind: "handle",
        si: +si,
        ai: +ai,
        side: side === "in" ? "in" : "out",
        start: p,
        path0: active.path,
      });
      start();
      return true;
    }
    const hit = nearestOnPath(active.path, toLocal(p), HIT_PX / z);
    if (hit) {
      const path = insertPoint(active.path, hit.si, hit.segIndex, hit.t);
      setEdit({ ...active, path, sel: [hit.si, hit.segIndex + 1] });
      return true;
    }
    commitPath();
    return true;
  };

  const move = (p: Pt): boolean => {
    if (tool === "pen" && pen) {
      const w = toWorld(p);
      if (grab?.kind !== "pen") {
        setPen({ ...pen, hover: w });
        return true;
      }
      const path = structuredClone(pen.path);
      const a = path.subpaths[0].anchors[grab.ai];
      if (Math.hypot(w.x - a.p.x, w.y - a.p.y) * z >= BEND_PX) {
        a.hOut = w;
        a.hIn = { x: 2 * a.p.x - w.x, y: 2 * a.p.y - w.y };
        a.kind = "smooth";
      }
      setPen({ path, hover: null });
      return true;
    }
    if (!active || !grab || grab.kind === "pen") return false;
    const d = localDelta(grab.start, p);
    const a = grab.path0.subpaths[grab.si].anchors[grab.ai];
    if (grab.kind === "anchor") {
      setEdit({
        ...active,
        path: moveAnchor(grab.path0, grab.si, grab.ai, { x: a.p.x + d.x, y: a.p.y + d.y }),
      });
      return true;
    }
    const h = (grab.side === "in" ? a.hIn : a.hOut) ?? a.p;
    setEdit({
      ...active,
      path: moveHandle(grab.path0, grab.si, grab.ai, grab.side, { x: h.x + d.x, y: h.y + d.y }),
    });
    return true;
  };

  const up = (): boolean => {
    if (!grab) return false;
    setGrab(null);
    return true;
  };

  const onKey = useEffectEvent((e: KeyboardEvent) => {
    const t = e.target;
    if (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || t instanceof HTMLSelectElement)
      return;
    const k = e.key.toLowerCase();
    if (pen && (k === "enter" || k === "escape" || (TOOL_KEYS.has(k) && !e.ctrlKey && !e.metaKey))) {
      if (k === "enter") e.preventDefault();
      finishPen();
    } else if (active) {
      if (k === "enter") {
        e.preventDefault();
        commitPath();
      } else if (k === "escape") setEditing(null);
      else if ((k === "delete" || k === "backspace") && active.sel) {
        e.preventDefault();
        setEdit({ ...active, path: removeAnchor(active.path, ...active.sel), sel: null });
      }
    }
  });
  useEffect(() => {
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const last = pen ? pen.path.subpaths[0].anchors.length - 1 : -1;
  const view =
    tool === "pen" && pen
      ? { path: pen.path, f: WORLD, sel: [0, last] as PathSel, fill: null, rubber: grab ? null : pen.hover }
      : editLayer && edit
        ? { path: edit.path, f: frame, sel: edit.sel, fill: solidFill(editLayer), rubber: null }
        : null;

  return { down, move, up, enter, view, drawing: !!pen };
}
