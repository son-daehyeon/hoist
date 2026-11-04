import {
  useCallback,
  useEffect,
  useEffectEvent,
  useRef,
  useState,
  type PointerEvent,
  type RefObject,
} from "react";
import { useEd } from "@/entities/editor";
import { defaultRect, rectLayer, textLayer, type Box } from "@/entities/layer";
import type { Capture, Layer, LayerId } from "@/shared/api";
import { isEffectivelyHidden, paintOrder, parentOffset } from "@/shared/lib";
import { guidesFor, snapLines, snapMove, snapResize, type Guide, type SnapLines } from "@/entities/editor";
import { lockAxis, nudged, resize, withRatio, type Handle } from "./drag";
import type { Panes, Rect, View } from "./viewport";

export const HANDLES: Handle[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];
type Pt = { x: number; y: number };
type Pane = Capture["pane"];
export type Preview = { id: LayerId; from: Box; box: Box };
type Nudge = Preview & { patch: { x_mm: number; y_mm: number } };
const NUDGE_MS = 150;
const SNAP_PX = 6;

type Drag =
  | { kind: "pan"; start: Pt; pan0: Pt }
  | {
      kind: "box";
      id: LayerId;
      handle: Handle | null;
      start: Pt;
      box0: Box;
      box: Box;
      lines: SnapLines;
      guides: Guide[];
    }
  | { kind: "rect"; mode: "capture" | "shape"; pane: Pane; start: Pt; box: Box };

export function boxOf(layers: Layer[], l: Layer): Box {
  const o = parentOffset(layers, l);
  return { x: o.x + l.x_mm, y: o.y + l.y_mm, w: l.w_mm, h: l.h_mm };
}
const inside = (r: Rect, p: Pt) => p.x >= r.x && p.x <= r.x + r.w && p.y >= r.y && p.y <= r.y + r.h;
const round = (v: number) => Math.round(v * 10) / 10;

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
  view: View,
  setView: (fn: (v: View) => View) => void,
  panes: Panes,
) {
  const { doc, selection, setSelection, tool, setTool, setCaptures, ops, locked } = useEd();
  const [drag, setDrag] = useState<Drag | null>(null);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [bubble, setBubble] = useState<Pt | null>(null);
  const bubbleTimer = useRef(0);
  const space = useSpaceKey();
  const nudge = useRef<Nudge | null>(null);
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

  const sendNudge = () => {
    window.clearTimeout(nudgeTimer.current);
    const n = nudge.current;
    nudge.current = null;
    if (n) void ops([{ op: "update_layer", id: n.id, patch: n.patch }]).then((ok) => ok || setPreview(null));
  };

  const onArrow = useEffectEvent((e: KeyboardEvent) => {
    if (e.ctrlKey || e.metaKey || e.altKey || !doc || locked || selection.length !== 1) return;
    const t = e.target;
    if (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || t instanceof HTMLSelectElement)
      return;
    if (document.querySelector("[role=dialog]")) return;
    const layer = doc.layers.find((l) => l.id === selection[0]);
    if (!layer || layer.locked || layer.kind.type === "group") return;
    const prev = nudge.current?.id === layer.id ? nudge.current : preview?.id === layer.id ? preview : null;
    const from = prev?.from ?? boxOf(doc.layers, layer);
    const box = nudged(prev?.box ?? from, e.key, e.shiftKey);
    if (!box) return;
    e.preventDefault();
    if (nudge.current && nudge.current.id !== layer.id) sendNudge();
    const off = parentOffset(doc.layers, layer);
    nudge.current = {
      id: layer.id,
      from,
      box,
      patch: { x_mm: round(box.x - off.x), y_mm: round(box.y - off.y) },
    };
    setPreview({ id: layer.id, from, box });
    window.clearTimeout(nudgeTimer.current);
    nudgeTimer.current = window.setTimeout(sendNudge, NUDGE_MS);
  });
  useEffect(() => {
    window.addEventListener("keydown", onArrow);
    return () => window.removeEventListener("keydown", onArrow);
  }, []);

  const W = doc?.artboard.width_mm ?? 1;
  const H = doc?.artboard.height_mm ?? 1;
  const z = view.zoom;
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

  const linesExcept = (layers: Layer[], id: LayerId) =>
    snapLines(
      W,
      H,
      layers.filter((l) => l.id !== id && !isEffectivelyHidden(layers, l)).map((l) => boxOf(layers, l)),
    );

  const hit = (mm: Pt) => {
    if (!doc) return null;
    const order = paintOrder(doc.layers);
    for (let i = order.length - 1; i >= 0; i--) {
      const l = order[i];
      if (l.kind.type === "group" || l.locked || isEffectivelyHidden(doc.layers, l)) continue;
      const b = boxOf(doc.layers, l);
      if (mm.x >= b.x && mm.x <= b.x + b.w && mm.y >= b.y && mm.y <= b.y + b.h) return l;
    }
    return null;
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
    const handle = (e.target as HTMLElement).dataset.handle as Handle | undefined;

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
    const sel = selection.length === 1 ? doc.layers.find((l) => l.id === selection[0]) : undefined;
    if (handle && sel) {
      start();
      return setDrag({
        kind: "box",
        id: sel.id,
        handle,
        start: p,
        box0: boxOf(doc.layers, sel),
        box: boxOf(doc.layers, sel),
        lines: linesExcept(doc.layers, sel.id),
        guides: [],
      });
    }
    const target = pane === "work" ? hit(toMm(pane, p)) : null;
    if (!target) return setSelection([]);
    if (e.shiftKey) {
      return setSelection(
        selection.includes(target.id) ? selection.filter((s) => s !== target.id) : [...selection, target.id],
      );
    }
    if (!selection.includes(target.id) || selection.length > 1) setSelection([target.id]);
    start();
    setDrag({
      kind: "box",
      id: target.id,
      handle: null,
      start: p,
      box0: boxOf(doc.layers, target),
      box: boxOf(doc.layers, target),
      lines: linesExcept(doc.layers, target.id),
      guides: [],
    });
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
    if (drag.kind === "box") {
      const snap = !e.altKey;
      const tolerance = SNAP_PX / z;
      const { box0, handle, lines } = drag;
      let dx = (p.x - drag.start.x) / z;
      let dy = (p.y - drag.start.y) / z;
      let box: Box;
      if (handle) {
        box = resize(box0, handle, dx, dy);
        if (snap) box = snapResize(box, handle, lines, tolerance);
        if (e.shiftKey) box = withRatio(box0, box, handle);
      } else {
        if (e.shiftKey) [dx, dy] = lockAxis(dx, dy);
        box = { ...box0, x: box0.x + dx, y: box0.y + dy };
        if (snap) {
          const s = snapMove(box, lines, tolerance);
          box = !e.shiftKey ? s : dx === 0 ? { ...s, x: box.x } : { ...s, y: box.y };
        }
      }
      return setDrag({ ...drag, box, guides: snap ? guidesFor(box, lines) : [] });
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
    if (d.kind === "box") {
      const b = { x: round(d.box.x), y: round(d.box.y), w: round(d.box.w), h: round(d.box.h) };
      const o = d.box0;
      if (b.x === round(o.x) && b.y === round(o.y) && b.w === round(o.w) && b.h === round(o.h)) return;
      const layer = doc.layers.find((l) => l.id === d.id);
      const off = layer ? parentOffset(doc.layers, layer) : { x: 0, y: 0 };
      setPreview({ id: d.id, from: d.box0, box: b });
      const patch = { x_mm: round(b.x - off.x), y_mm: round(b.y - off.y), w_mm: b.w, h_mm: b.h };
      void ops([{ op: "update_layer", id: d.id, patch }]).then((ok) => ok || setPreview(null));
      return;
    }
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

  return {
    drag,
    preview,
    clearPreview,
    bubble,
    cursor,
    handlers: { onPointerDown, onPointerMove, onPointerUp, onPointerCancel: () => setDrag(null) },
  };
}
