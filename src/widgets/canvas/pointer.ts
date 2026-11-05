import {
  useCallback,
  useEffect,
  useEffectEvent,
  useRef,
  useState,
  type PointerEvent,
  type RefObject,
} from "react";
import {
  guidesFor,
  snapLines,
  snapMove,
  snapResize,
  useEd,
  type Guide,
  type SnapLines,
} from "@/entities/editor";
import { defaultRect, rectLayer, textLayer, type Box } from "@/entities/layer";
import type { Capture, Layer, LayerId, Op } from "@/shared/api";
import { isEffectivelyHidden, paintOrder } from "@/shared/lib";
import {
  aabb,
  centerOf,
  lockAxis,
  mapInto,
  moved,
  nudged,
  resizedFrame,
  rotatedAbout,
  unplaced,
  type Handle,
  type Params,
} from "./drag";
import {
  canTransform,
  frameFor,
  hasAncestor,
  movable,
  paramsOf,
  parentMatrix,
  type Edits,
} from "./selection";
import type { Panes, Rect, View } from "./viewport";

export const HANDLES: Handle[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];
type Pt = { x: number; y: number };
type Pane = Capture["pane"];
const NUDGE_MS = 150;
const SNAP_PX = 6;
const ROTATE_STEP = 15;
const PROBES: [number, number][] = [
  [0, 0],
  [3, 0],
  [-3, 0],
  [0, 3],
  [0, -3],
  [2, 2],
  [-2, -2],
  [2, -2],
  [-2, 2],
];

type Drag =
  | { kind: "pan"; start: Pt; pan0: Pt }
  | {
      kind: "edit";
      mode: "move" | "resize" | "rotate";
      handle: Handle | null;
      start: Pt;
      frame0: Params;
      frame: Params;
      lines: SnapLines;
      guides: Guide[];
      edits: Edits;
    }
  | { kind: "rect"; mode: "capture" | "shape"; pane: Pane; start: Pt; box: Box };

const inside = (r: Rect, p: Pt) => p.x >= r.x && p.x <= r.x + r.w && p.y >= r.y && p.y <= r.y + r.h;
const round = (v: number) => Math.round(v * 10) / 10;
const normDeg = (d: number) => round((((d % 360) + 540) % 360) - 180);
const mapValues = (o: Record<LayerId, Params>, fn: (p: Params) => Params) =>
  Object.fromEntries(Object.entries(o).map(([k, v]) => [k, fn(v)]));
const asBox = ({ x, y, w, h }: Params): Box => ({ x, y, w, h });

function useSpaceKey() {
  const [down, setDown] = useState(false);
  useEffect(() => {
    const typing = (e: KeyboardEvent) =>
      e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
    const kd = (e: KeyboardEvent) => {
      if (e.code === "Space" && !typing(e)) {
        e.preventDefault();
        setDown(true);
      }
    };
    const ku = (e: KeyboardEvent) => e.code === "Space" && setDown(false);
    window.addEventListener("keydown", kd);
    window.addEventListener("keyup", ku);
    return () => {
      window.removeEventListener("keydown", kd);
      window.removeEventListener("keyup", ku);
    };
  }, []);
  return down;
}

export function useCanvasPointer(
  viewportRef: RefObject<HTMLDivElement | null>,
  svgRef: RefObject<SVGSVGElement | null>,
  view: View,
  setView: (fn: (v: View) => View) => void,
  panes: Panes,
) {
  const { doc, selection, setSelection, tool, setTool, setCaptures, ops, locked, outline } = useEd();
  const [drag, setDrag] = useState<Drag | null>(null);
  const [preview, setPreview] = useState<Edits | null>(null);
  const [bubble, setBubble] = useState<Pt | null>(null);
  const bubbleTimer = useRef(0);
  const space = useSpaceKey();
  const nudge = useRef<Edits | null>(null);
  const nudgeTimer = useRef(0);
  useEffect(
    () => () => {
      window.clearTimeout(bubbleTimer.current);
      window.clearTimeout(nudgeTimer.current);
    },
    [],
  );
  const clearPreview = useCallback(() => {
    if (!nudge.current) setPreview(null);
  }, []);

  const layers = doc?.layers ?? [];
  const byId = new Map(layers.map((l) => [l.id, l]));
  const W = doc?.artboard.width_mm ?? 1;
  const H = doc?.artboard.height_mm ?? 1;
  const z = view.zoom;

  const commit = (edits: Edits) => {
    const list: Op[] = [];
    for (const [id, next] of Object.entries(edits.next)) {
      const base = edits.base[id];
      const layer = byId.get(id);
      if (!base || !layer) continue;
      const local = unplaced(parentMatrix(layers, layer), next);
      const patch: Record<string, number> = {};
      const sized = round(next.w) !== round(base.w) || round(next.h) !== round(base.h);
      if (sized || round(next.x) !== round(base.x) || round(next.y) !== round(base.y)) {
        patch.x_mm = round(local.x);
        patch.y_mm = round(local.y);
      }
      if (sized) {
        patch.w_mm = round(next.w);
        patch.h_mm = round(next.h);
      }
      if (round(next.deg) !== round(base.deg)) patch.rotation_deg = normDeg(local.deg);
      if (Object.keys(patch).length) list.push({ op: "update_layer", id, patch });
    }
    if (!list.length) return;
    setPreview(edits);
    void ops(list).then((ok) => ok || setPreview(null));
  };

  const sendNudge = () => {
    window.clearTimeout(nudgeTimer.current);
    const n = nudge.current;
    nudge.current = null;
    if (n) commit(n);
  };

  const onArrow = useEffectEvent((e: KeyboardEvent) => {
    if (e.ctrlKey || e.metaKey || e.altKey || !doc || locked || selection.length === 0) return;
    const t = e.target;
    if (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || t instanceof HTMLSelectElement)
      return;
    if (document.querySelector("[role=dialog]")) return;
    const step = nudged({ x: 0, y: 0, w: 0, h: 0 }, e.key, e.shiftKey);
    const ids = movable(layers, selection).map((l) => l.id);
    if (!step || ids.length === 0) return;
    e.preventDefault();
    const same = (x: Edits | null) => x && ids.every((id) => x.next[id]);
    const cur = same(nudge.current) ? nudge.current : same(preview) ? preview : null;
    const base = cur?.base ?? Object.fromEntries(ids.map((id) => [id, paramsOf(layers, byId.get(id)!)]));
    const from = cur?.next ?? base;
    nudge.current = { base, next: mapValues(from, (p) => moved(p, step.x, step.y)) };
    setPreview(nudge.current);
    window.clearTimeout(nudgeTimer.current);
    nudgeTimer.current = window.setTimeout(sendNudge, NUDGE_MS);
  });
  useEffect(() => {
    window.addEventListener("keydown", onArrow);
    return () => window.removeEventListener("keydown", onArrow);
  }, []);

  const point = (e: PointerEvent): Pt => {
    const r = viewportRef.current?.getBoundingClientRect();
    return { x: e.clientX - (r?.left ?? 0), y: e.clientY - (r?.top ?? 0) };
  };
  const paneAt = (p: Pt): Pane | null =>
    inside(panes.work, p) ? "work" : panes.original && inside(panes.original, p) ? "original" : null;
  const rectOf = (pane: Pane) => (pane === "work" ? panes.work : (panes.original ?? panes.work));
  const toMm = (pane: Pane, p: Pt): Pt => {
    const r = rectOf(pane);
    return { x: Math.min(W, Math.max(0, (p.x - r.x) / z)), y: Math.min(H, Math.max(0, (p.y - r.y) / z)) };
  };
  const toWorld = (p: Pt): Pt => ({ x: (p.x - panes.work.x) / z, y: (p.y - panes.work.y) / z });

  const pickable = (l: Layer | undefined): l is Layer => !!l && !l.locked && !isEffectivelyHidden(layers, l);

  const pickAt = (clientX: number, clientY: number) => {
    const svg = svgRef.current;
    if (!svg) return null;
    for (const [ox, oy] of outline ? PROBES : PROBES.slice(0, 1)) {
      for (const el of document.elementsFromPoint(clientX + ox, clientY + oy)) {
        if (!svg.contains(el)) continue;
        const node = el.closest("[data-layer]");
        const layer = byId.get(node?.getAttribute("data-layer") ?? "");
        if (node && pickable(layer)) return { layer, node };
      }
    }
    return null;
  };

  const boxHit = (mm: Pt) => {
    const order = paintOrder(layers);
    for (let i = order.length - 1; i >= 0; i--) {
      const l = order[i];
      if (l.kind.type === "group" || !pickable(l)) continue;
      const b = aabb(paramsOf(layers, l));
      if (mm.x >= b.x && mm.x <= b.x + b.w && mm.y >= b.y && mm.y <= b.y + b.h) return l;
    }
    return null;
  };

  const selectedOwner = (node: Element | null) => {
    for (let n = node; n; n = n.parentElement?.closest("[data-layer]") ?? null) {
      const id = n.getAttribute("data-layer");
      if (id && selection.includes(id)) return id;
    }
    return null;
  };

  const linesExcept = (ids: LayerId[]) => {
    const moving = new Set(movable(layers, ids).map((l) => l.id));
    const others = layers.filter(
      (l) => !moving.has(l.id) && !hasAncestor(layers, l, moving) && !isEffectivelyHidden(layers, l),
    );
    return snapLines(
      W,
      H,
      others.map((l) => aabb(paramsOf(layers, l))),
    );
  };

  const begin = (mode: "move" | "resize" | "rotate", handle: Handle | null, ids: LayerId[], p: Pt) => {
    const base = Object.fromEntries(movable(layers, ids).map((l) => [l.id, paramsOf(layers, l)]));
    const frame0 = frameFor(Object.values(base));
    if (!frame0) return;
    setDrag({
      kind: "edit",
      mode,
      handle,
      start: p,
      frame0,
      frame: frame0,
      lines: linesExcept(ids),
      guides: [],
      edits: { base, next: base },
    });
  };

  const add = (layer: Layer) => {
    void ops([{ op: "add_layer", layer }]).then((ok) => ok && setSelection([layer.id]));
    setTool("select");
  };

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (!doc) return;
    if (nudge.current) sendNudge();
    const p = point(e);
    const start = () => e.currentTarget.setPointerCapture(e.pointerId);
    if (e.button === 1 || tool === "hand" || space) {
      start();
      return setDrag({ kind: "pan", start: p, pan0: view.pan });
    }
    if (e.button !== 0) return;
    const pane = paneAt(p);

    if (tool === "capture" || tool === "shape") {
      if (!pane || (tool === "shape" && pane !== "work")) return;
      start();
      const s = toMm(pane, p);
      return setDrag({ kind: "rect", mode: tool, pane, start: s, box: { x: s.x, y: s.y, w: 0, h: 0 } });
    }
    if (tool === "text") {
      if (pane !== "work") return;
      const m = toMm(pane, p);
      return add(textLayer(doc, m.x, m.y));
    }

    const handle = (e.target as HTMLElement).dataset.handle as Handle | "rotate" | undefined;
    if (handle && canTransform(layers, selection)) {
      start();
      return begin(
        handle === "rotate" ? "rotate" : "resize",
        handle === "rotate" ? null : handle,
        selection,
        p,
      );
    }

    const fragments = svgRef.current?.querySelector("[data-layer]");
    const hit = pane !== "work" ? null : fragments ? pickAt(e.clientX, e.clientY) : null;
    const target = hit?.layer ?? (pane === "work" && !fragments ? boxHit(toMm(pane, p)) : null);
    if (!target) return setSelection([]);
    if (e.shiftKey) {
      return setSelection(
        selection.includes(target.id) ? selection.filter((s) => s !== target.id) : [...selection, target.id],
      );
    }
    const owner = hit ? selectedOwner(hit.node) : selection.includes(target.id) ? target.id : null;
    const ids = owner ? selection : [target.id];
    if (!owner) setSelection(ids);
    start();
    begin("move", null, ids, p);
  };

  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    if (!drag) return;
    const p = point(e);
    if (drag.kind === "pan") {
      return setView((v) => ({
        ...v,
        pan: { x: drag.pan0.x + p.x - drag.start.x, y: drag.pan0.y + p.y - drag.start.y },
      }));
    }
    if (drag.kind === "edit") {
      const { mode, handle, frame0, lines } = drag;
      const { base } = drag.edits;
      const snap = !e.altKey;
      const tolerance = SNAP_PX / z;
      const dxm = (p.x - drag.start.x) / z;
      const dym = (p.y - drag.start.y) / z;
      let frame: Params;
      let next: Record<LayerId, Params>;
      if (mode === "move") {
        let [dx, dy] = e.shiftKey ? lockAxis(dxm, dym) : [dxm, dym];
        if (snap) {
          const box = aabb(moved(frame0, dx, dy));
          const s = snapMove(box, lines, tolerance);
          dx += e.shiftKey && dx === 0 ? 0 : s.x - box.x;
          dy += e.shiftKey && dy === 0 ? 0 : s.y - box.y;
        }
        frame = moved(frame0, dx, dy);
        next = mapValues(base, (q) => moved(q, dx, dy));
      } else if (mode === "resize" && handle) {
        frame = resizedFrame(frame0, handle, dxm, dym, {
          keepRatio: e.shiftKey,
          snap: snap && frame0.deg === 0 ? (b) => snapResize(b, handle, lines, tolerance) : undefined,
        });
        const ids = Object.keys(base);
        next =
          ids.length === 1
            ? { [ids[0]]: frame }
            : mapValues(base, (q) => mapInto(q, asBox(frame0), asBox(frame)));
      } else {
        const c = centerOf(frame0);
        const angle = (q: Pt) => {
          const w = toWorld(q);
          return (Math.atan2(w.y - c.y, w.x - c.x) * 180) / Math.PI;
        };
        let d = angle(p) - angle(drag.start);
        if (e.shiftKey) d = Math.round((frame0.deg + d) / ROTATE_STEP) * ROTATE_STEP - frame0.deg;
        frame = rotatedAbout(frame0, c, d);
        next = mapValues(base, (q) => rotatedAbout(q, c, d));
      }
      const guides = snap && mode !== "rotate" ? guidesFor(aabb(frame), lines) : [];
      return setDrag({ ...drag, frame, guides, edits: { base, next } });
    }
    const m = toMm(drag.pane, p);
    const box = {
      x: Math.min(m.x, drag.start.x),
      y: Math.min(m.y, drag.start.y),
      w: Math.abs(m.x - drag.start.x),
      h: Math.abs(m.y - drag.start.y),
    };
    setDrag({ ...drag, box });
  };

  const onPointerUp = () => {
    const d = drag;
    setDrag(null);
    if (!d || !doc || d.kind === "pan") return;
    if (d.kind === "edit") return commit(d.edits);
    const big = d.box.w * z >= 4 && d.box.h * z >= 4;
    if (d.mode === "shape") {
      return add(rectLayer(doc, big ? d.box : defaultRect(doc, d.start.x, d.start.y)));
    }
    if (!big) return;
    const b = d.box;
    setCaptures((c) => [
      ...c,
      { pane: d.pane, x_mm: round(b.x), y_mm: round(b.y), w_mm: round(b.w), h_mm: round(b.h) },
    ]);
    const r = rectOf(d.pane);
    setBubble({ x: r.x + (b.x + b.w) * z, y: r.y + (b.y + b.h) * z + 6 });
    window.clearTimeout(bubbleTimer.current);
    bubbleTimer.current = window.setTimeout(() => setBubble(null), 2000);
  };

  const cursor =
    drag?.kind === "pan"
      ? "grabbing"
      : tool === "hand" || space
        ? "grab"
        : tool === "text"
          ? "text"
          : tool === "select"
            ? "default"
            : "crosshair";

  const live = drag?.kind === "edit" ? drag.edits : preview;
  const frame = drag?.kind === "edit" ? drag.frame : null;

  return {
    drag,
    live,
    frame,
    clearPreview,
    bubble,
    cursor,
    handlers: { onPointerDown, onPointerMove, onPointerUp, onPointerCancel: () => setDrag(null) },
  };
}
