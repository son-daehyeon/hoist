import {
  useCallback,
  useEffect,
  useEffectEvent,
  useRef,
  useState,
  type MouseEvent,
  type PointerEvent,
  type RefObject,
} from "react";
import { useEd } from "@/entities/editor";
import { objectActions } from "@/features/object-actions";
import { lastColor, lineWidth, shapeLayer, shapeSize, textLayer, type Box } from "@/entities/layer";
import type { Pt } from "@/entities/path";
import type { Capture, Layer, LayerId, Op } from "@/shared/api";
import { dialogOpen, isFormField, isTextEntry, modalOpen, round1 } from "@/shared/lib";
import { cropStart, type Edges } from "./crop";
import { useCropMode } from "./cropMode";
import { editOps, mapValues, moveEditDrag, startEditDrag, type EditDrag } from "./editDrag";
import { drawn, moved, nudged, type Frame, type Handle } from "./geometry";
import { boxHit, inFrame, insideSelection, pickAt, selectedOwner } from "./hitTest";
import { usePathTools } from "./pathTools";
import { canTransform, frameOf, movable, type Edits } from "./selection";
import { SHAPE_LABEL, shapeProps } from "./shapeProps";
import { LABEL_GAP_PX, paneRect, type Panes, type Rect, type View } from "./viewport";

type Pane = Capture["pane"];
const NUDGE_MS = 150;
const DRAG_MIN_PX = 4;
const BUBBLE_MS = 2000;

type Drag =
  | { kind: "pan"; start: Pt; pan0: Pt }
  | EditDrag
  | { kind: "rect"; pane: Pane; start: Pt; box: Box }
  | { kind: "draw"; start: Pt; startPx: Pt; frame: Frame; dragged: boolean }
  | { kind: "crop"; handle: Handle | "move"; start: Pt; edges0: Edges };

const inside = (r: Rect, p: Pt) => p.x >= r.x && p.x <= r.x + r.w && p.y >= r.y && p.y <= r.y + r.h;

function useSpaceKey() {
  const [down, setDown] = useState(false);
  useEffect(() => {
    const kd = (e: KeyboardEvent) => {
      if (e.code === "Space" && !isTextEntry(e.target) && !modalOpen()) {
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
  const ed = useEd();
  const { doc, selection, setSelection, tool, setTool, setCaptures, ops, locked, outline, shapeTool } = ed;
  const { editing, setEditing } = ed;
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
    setEditing((e) => (e?.pending ? null : e));
  }, [setEditing]);

  const layers = doc?.layers ?? [];
  const byId = new Map(layers.map((l) => [l.id, l]));
  const W = doc?.artboard.width_mm ?? 1;
  const H = doc?.artboard.height_mm ?? 1;
  const z = view.zoom;
  const toWorld = (p: Pt): Pt => ({ x: (p.x - panes.work.x) / z, y: (p.y - panes.work.y) / z });
  const paths = usePathTools(z, toWorld);

  const commit = (edits: Edits) => {
    const { ops: list, copyAt } = editOps(layers, edits);
    if (Object.keys(copyAt).length) {
      setPreview(edits);
      void objectActions(ed)
        .duplicateAt(copyAt)
        .then((ok) => ok || setPreview(null));
      return;
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
    if (e.ctrlKey || e.metaKey || e.altKey || !doc || locked || editing || selection.length === 0) return;
    if (isFormField(e.target) || dialogOpen()) return;
    const step = nudged({ x: 0, y: 0, w: 0, h: 0 }, e.key, e.shiftKey);
    const ids = movable(layers, selection).map((l) => l.id);
    if (!step || ids.length === 0) return;
    e.preventDefault();
    const same = (x: Edits | null) => x && ids.every((id) => x.next[id]);
    const cur = same(nudge.current) ? nudge.current : same(preview) ? preview : null;
    const base = cur?.base ?? Object.fromEntries(ids.map((id) => [id, frameOf(layers, byId.get(id)!)]));
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
  const crop = useCropMode(layers, z, toWorld);

  const point = (e: MouseEvent): Pt => {
    const r = viewportRef.current?.getBoundingClientRect();
    return { x: e.clientX - (r?.left ?? 0), y: e.clientY - (r?.top ?? 0) };
  };
  const paneAt = (p: Pt): Pane | null =>
    inside(panes.work, p) ? "work" : panes.original && inside(panes.original, p) ? "original" : null;
  const toMm = (pane: Pane, p: Pt): Pt => {
    const r = paneRect(panes, pane);
    return { x: Math.min(W, Math.max(0, (p.x - r.x) / z)), y: Math.min(H, Math.max(0, (p.y - r.y) / z)) };
  };

  const thick = doc ? lineWidth(doc) * 4 : 1;
  const drawFrame = (a: Pt, b: Pt, shift: boolean) =>
    drawn(a, b, { line: shapeTool.kind === "line", shift, thick });
  const defaultFrame = (c: Pt): Frame => {
    const s = doc ? shapeSize(doc) : 1;
    const h = shapeTool.kind === "line" ? thick : s;
    return { x: c.x - s / 2, y: c.y - h / 2, w: s, h, deg: 0 };
  };

  const begin = (mode: EditDrag["mode"], handle: Handle | null, ids: LayerId[], p: Pt, copy = false) => {
    const d = doc && startEditDrag(doc, { mode, handle, ids, start: p, copy });
    if (d) setDrag(d);
  };

  const add = (layer: Layer, edit = false) => {
    void ops([{ op: "add_layer", layer }]).then((ok) => {
      if (!ok) return;
      setSelection([layer.id]);
      if (edit) setEditing({ kind: "text", id: layer.id });
    });
    setTool("select");
  };

  const finishText = (id: LayerId, text: string | null) => {
    if (text === null) return setEditing(null);
    const op: Op = text.trim()
      ? { op: "update_layer", id, patch: { kind: { text } } }
      : { op: "delete_layer", id };
    setEditing({ kind: "text", id, pending: true });
    void ops([op]).then((ok) => ok || setEditing(null));
  };

  const [menu, setMenu] = useState<{ x: number; y: number; blank: boolean } | null>(null);
  const onContextMenu = (e: MouseEvent<HTMLDivElement>) => {
    if (e.target instanceof HTMLTextAreaElement) return;
    e.preventDefault();
    if (!doc || drag || editing) return;
    const hit = pickAt(svgRef.current, layers, e.clientX, e.clientY, outline);
    const mm = toWorld(point(e));
    const inside = insideSelection(layers, selection, mm);
    if (hit && !selectedOwner(hit.node, selection)) setSelection([hit.layer.id]);
    if (!hit && !inside) setSelection([]);
    setMenu({ x: e.clientX, y: e.clientY, blank: !hit && !inside });
  };

  const onDoubleClick = (e: MouseEvent<HTMLDivElement>) => {
    if (!doc || locked || tool !== "select") return;
    const only = selection.length === 1 ? byId.get(selection[0]) : undefined;
    const p = point(e);
    const hit =
      pickAt(svgRef.current, layers, e.clientX, e.clientY, outline)?.layer ??
      (only && inFrame(layers, only, toWorld(p)) ? only : null);
    if (!hit || hit.locked || paths.enter(hit)) return;
    const next = hit.kind.type === "text" ? ({ kind: "text", id: hit.id } as const) : cropStart(hit);
    if (!next) return;
    setSelection([hit.id]);
    setEditing(next);
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
    if (crop.cropping) {
      const h = (e.target as HTMLElement).dataset.crop as Handle | undefined;
      if (!h && !crop.contains(p)) return crop.commit();
      start();
      return setDrag({ kind: "crop", handle: h ?? "move", start: p, edges0: crop.cropping.edges });
    }
    if (paths.down(e, p, start)) return;
    const pane = paneAt(p);

    if (tool === "capture") {
      if (!pane) return;
      start();
      const s = toMm(pane, p);
      return setDrag({ kind: "rect", pane, start: s, box: { x: s.x, y: s.y, w: 0, h: 0 } });
    }
    if (tool === "shape") {
      if (pane !== "work") return;
      start();
      const s = toWorld(p);
      return setDrag({ kind: "draw", start: s, startPx: p, frame: drawFrame(s, s, false), dragged: false });
    }
    if (tool === "text") {
      if (pane !== "work") return;
      e.preventDefault();
      const m = toMm(pane, p);
      return add(textLayer(doc, m.x, m.y), true);
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
    const hit =
      pane !== "work"
        ? null
        : fragments
          ? pickAt(svgRef.current, layers, e.clientX, e.clientY, outline)
          : null;
    const target = hit?.layer ?? (pane === "work" && !fragments ? boxHit(layers, toMm(pane, p)) : null);
    if (!target) {
      if (!insideSelection(layers, selection, toWorld(p))) return setSelection([]);
      start();
      return begin("move", null, selection, p, e.altKey);
    }
    if (e.shiftKey) {
      return setSelection(
        selection.includes(target.id) ? selection.filter((s) => s !== target.id) : [...selection, target.id],
      );
    }
    const owner = hit ? selectedOwner(hit.node, selection) : selection.includes(target.id) ? target.id : null;
    const ids = owner ? selection : [target.id];
    if (!owner) setSelection(ids);
    start();
    begin("move", null, ids, p, e.altKey);
  };

  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const p = point(e);
    if (paths.move(p) || !drag) return;
    if (drag.kind === "pan") {
      return setView((v) => ({
        ...v,
        pan: { x: drag.pan0.x + p.x - drag.start.x, y: drag.pan0.y + p.y - drag.start.y },
      }));
    }
    if (drag.kind === "edit") {
      return setDrag(moveEditDrag(drag, p, { z, shift: e.shiftKey, alt: e.altKey, toWorld }));
    }
    if (drag.kind === "crop") return crop.dragTo(drag, p);
    if (drag.kind === "draw") {
      return setDrag({
        ...drag,
        frame: drawFrame(drag.start, toWorld(p), e.shiftKey),
        dragged: drag.dragged || Math.hypot(p.x - drag.startPx.x, p.y - drag.startPx.y) >= DRAG_MIN_PX,
      });
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
    if (paths.up()) return;
    const d = drag;
    setDrag(null);
    if (!d || !doc || d.kind === "pan" || d.kind === "crop") return;
    if (d.kind === "edit") return commit(d.edits);
    if (d.kind === "draw") {
      const f = d.dragged ? d.frame : defaultFrame(d.start);
      return add(shapeLayer(doc, SHAPE_LABEL[shapeTool.kind], shapeProps(shapeTool, f), f, lastColor()));
    }
    if (d.box.w * z < DRAG_MIN_PX || d.box.h * z < DRAG_MIN_PX) return;
    const b = d.box;
    setCaptures((c) => [
      ...c,
      { pane: d.pane, x_mm: round1(b.x), y_mm: round1(b.y), w_mm: round1(b.w), h_mm: round1(b.h) },
    ]);
    const r = paneRect(panes, d.pane);
    setBubble({ x: r.x + (b.x + b.w) * z, y: r.y + (b.y + b.h) * z + LABEL_GAP_PX });
    window.clearTimeout(bubbleTimer.current);
    bubbleTimer.current = window.setTimeout(() => setBubble(null), BUBBLE_MS);
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
    pathView: paths.view,
    finishText,
    bubble,
    cursor,
    handlers: {
      onPointerDown,
      onPointerMove,
      onPointerUp,
      onPointerCancel: () => setDrag(null),
      onDoubleClick,
      onContextMenu,
    },
    menu,
    closeMenu: () => setMenu(null),
  };
}
