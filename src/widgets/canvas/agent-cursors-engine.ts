import type { RefObject } from "react";
import type { Pt } from "@/entities/path";
import {
  frameAt,
  planBatch,
  restingSpot,
  SLOTS,
  WORK_LABEL,
  type Change,
  type Clip,
  type CursorFrame,
} from "./agent-cursors";
import { applyEdits, type PreviewState } from "./preview";
import type { Rect } from "./viewport";

export const SLOT_IDS = Array.from({ length: SLOTS }, (_, i) => i);

export type Geom = { pane: Rect; z: number };
export type SlotEls = {
  slot: HTMLElement;
  cursor: HTMLElement;
  tag: HTMLElement;
  box: HTMLElement;
  badge: HTMLElement;
};
export type Engine = ReturnType<typeof makeEngine>;

export function makeEngine(
  svgRef: RefObject<SVGSVGElement | null>,
  worldRef: RefObject<SVGGElement | null>,
  geom: RefObject<Geom>,
  els: RefObject<SlotEls[]>,
  onActive: (active: boolean[]) => void,
  onBusy: (busy: boolean) => void,
) {
  let queue: Change[] = [];
  let clips: Clip[] = [];
  let free = SLOT_IDS.map(() => 0);
  let pos: Pt[] = [];
  let raf = 0;
  let activeKey = "";
  let busy = false;
  const setBusy = (b: boolean) => {
    if (b !== busy) onBusy((busy = b));
  };
  const touched: PreviewState = new Map();
  const hidden = new Set<string>();

  const layerEls = (id: string) =>
    svgRef.current?.querySelectorAll<SVGElement>(`[data-layer="${CSS.escape(id)}"]`) ?? [];

  function hide(ids: Set<string>) {
    for (const id of hidden) if (!ids.has(id)) for (const el of layerEls(id)) el.style.visibility = "";
    for (const id of ids) for (const el of layerEls(id)) el.style.visibility = "hidden";
    hidden.clear();
    for (const id of ids) hidden.add(id);
  }

  function place(slot: number, p: Pt) {
    const e = els.current[slot];
    const { pane, z } = geom.current;
    if (e) e.cursor.style.transform = `translate(${pane.x + p.x * z}px, ${pane.y + p.y * z}px)`;
  }

  function paint(c: Clip, f: CursorFrame) {
    const e = els.current[c.slot];
    if (!e) return;
    const { pane, z } = geom.current;
    place(c.slot, f.cursor);
    e.slot.dataset.state = f.box ? (f.press ? "press" : "act") : "fly";
    e.tag.textContent = WORK_LABEL[c.work];
    if (f.box) {
      const s = e.box.style;
      s.left = `${pane.x + f.box.x * z}px`;
      s.top = `${pane.y + f.box.y * z}px`;
      s.width = `${f.box.w * z}px`;
      s.height = `${f.box.h * z}px`;
      s.transform = f.box.deg ? `rotate(${f.box.deg}deg)` : "";
    }
    e.badge.textContent = f.badge ?? "";
  }

  function tick() {
    const now = performance.now();
    if (!clips.length && queue.length) {
      const plan = planBatch(queue, now, free);
      queue = [];
      clips = plan.clips;
      free = plan.free;
    }
    const live = { base: {} as Record<string, Clip["to"]>, next: {} as Record<string, Clip["to"]> };
    const hiding = new Set(queue.filter((c) => c.kind === "add").map((c) => c.id));
    const rest: Clip[] = [];
    for (const c of clips) {
      if (now < c.start) {
        if (c.kind === "add") hiding.add(c.id);
        rest.push(c);
        continue;
      }
      const f = frameAt(c, now, pos[c.slot]);
      paint(c, f);
      if (f.layer) {
        live.base[c.id] = c.to;
        live.next[c.id] = f.layer;
      }
      if (f.hide) hiding.add(c.id);
      if (f.done) {
        pos[c.slot] = f.cursor;
        const e = els.current[c.slot];
        if (e) e.slot.dataset.state = "idle";
      } else rest.push(c);
    }
    clips = rest;
    applyEdits(svgRef.current, worldRef.current, Object.keys(live.next).length ? live : null, touched);
    hide(hiding);
    const active = SLOT_IDS.map((i) => clips.some((c) => c.slot === i && c.start <= now));
    const key = active.join();
    if (key !== activeKey) {
      activeKey = key;
      onActive(active);
    }
    raf = clips.length || queue.length ? requestAnimationFrame(tick) : 0;
    setBusy(raf !== 0);
  }

  return {
    enqueue(changes: Change[], W: number, H: number) {
      if (!pos.length) pos = SLOT_IDS.map((i) => restingSpot(i, W, H));
      queue = [...queue, ...changes];
      if (!raf) raf = requestAnimationFrame(tick);
      setBusy(true);
    },
    repaint() {
      pos.forEach((p, i) => place(i, p));
    },
    stop(forget = false) {
      cancelAnimationFrame(raf);
      raf = 0;
      setBusy(false);
      queue = [];
      clips = [];
      free = SLOT_IDS.map(() => 0);
      applyEdits(svgRef.current, worldRef.current, null, touched);
      hide(new Set());
      for (const e of els.current) {
        if (forget || e.slot.dataset.state !== "off") e.slot.dataset.state = forget ? "off" : "idle";
      }
      if (forget) pos = [];
      if (activeKey) {
        activeKey = "";
        onActive(SLOT_IDS.map(() => false));
      }
    },
  };
}
