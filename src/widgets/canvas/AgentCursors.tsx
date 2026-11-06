import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type RefObject } from "react";
import { useEd } from "@/entities/editor";
import type { Layer } from "@/shared/api";
import { ProgressLine } from "@/shared/ui";
import {
  diffLayers,
  frameAt,
  MAX_QUEUE,
  planBatch,
  restingSpot,
  sample,
  SLOTS,
  WORK_LABEL,
  type Change,
  type Clip,
  type Frame,
  type Pt,
} from "./agent-cursors";
import { applyEdits, type PreviewState } from "./preview";
import { paramsOf } from "./selection";
import type { Rect } from "./viewport";
import "./agent-cursors.css";

const COLORS = ["var(--blue-500)", "#12b76a", "#f0640d", "#7a5af8"];
const SLOT_IDS = Array.from({ length: SLOTS }, (_, i) => i);

type Geom = { pane: Rect; z: number };
type SlotEls = {
  slot: HTMLElement;
  cursor: HTMLElement;
  tag: HTMLElement;
  box: HTMLElement;
  badge: HTMLElement;
};
type Engine = ReturnType<typeof makeEngine>;

const reducedMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

function makeEngine(
  svgRef: RefObject<SVGSVGElement | null>,
  worldRef: RefObject<SVGGElement | null>,
  geom: RefObject<Geom>,
  els: RefObject<SlotEls[]>,
  onActive: (active: boolean[]) => void,
) {
  let queue: Change[] = [];
  let clips: Clip[] = [];
  let free = SLOT_IDS.map(() => 0);
  let pos: Pt[] = [];
  let raf = 0;
  let activeKey = "";
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

  function paint(c: Clip, f: Frame) {
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
  }

  return {
    enqueue(changes: Change[], W: number, H: number) {
      if (!pos.length) pos = SLOT_IDS.map((i) => restingSpot(i, W, H));
      queue = sample([...queue, ...changes], MAX_QUEUE);
      if (!raf) raf = requestAnimationFrame(tick);
    },
    repaint() {
      pos.forEach((p, i) => place(i, p));
    },
    stop(forget = false) {
      cancelAnimationFrame(raf);
      raf = 0;
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

export function AgentCursors({
  svgRef,
  worldRef,
  pane,
  z,
}: {
  svgRef: RefObject<SVGSVGElement | null>;
  worldRef: RefObject<SVGGElement | null>;
  pane: Rect;
  z: number;
}) {
  const { doc, locked, projectId, currentTurn } = useEd();
  const root = useRef<HTMLDivElement>(null);
  const geom = useRef<Geom>({ pane, z });
  const els = useRef<SlotEls[]>([]);
  const seen = useRef<{ pid: string; layers: Layer[] | undefined }>({ pid: projectId, layers: undefined });
  const engine = useRef<Engine | null>(null);
  const [active, setActive] = useState(() => SLOT_IDS.map(() => false));

  useLayoutEffect(() => {
    els.current = [...(root.current?.querySelectorAll<HTMLElement>(".ac-slot") ?? [])].map((slot) => ({
      slot,
      cursor: slot.querySelector<HTMLElement>(".ac-cursor")!,
      tag: slot.querySelector<HTMLElement>(".ac-tag")!,
      box: slot.querySelector<HTMLElement>(".ac-box")!,
      badge: slot.querySelector<HTMLElement>(".ac-badge")!,
    }));
    const e = makeEngine(svgRef, worldRef, geom, els, setActive);
    engine.current = e;
    return () => {
      e.stop(true);
      engine.current = null;
    };
  }, [svgRef, worldRef]);

  useLayoutEffect(() => {
    geom.current = { pane, z };
    engine.current?.repaint();
  }, [pane, z]);

  useEffect(() => {
    const layers = doc?.layers;
    const prev = seen.current.pid === projectId ? seen.current.layers : undefined;
    seen.current = { pid: projectId, layers };
    if (!layers || !doc || !prev || prev === layers || !locked) return;
    if (reducedMotion() || document.visibilityState !== "visible") return;
    const changes = diffLayers(prev, layers, paramsOf);
    if (changes.length) engine.current?.enqueue(changes, doc.artboard.width_mm, doc.artboard.height_mm);
  }, [doc, locked, projectId]);

  useEffect(() => {
    if (!locked) engine.current?.stop();
  }, [locked]);

  useEffect(() => {
    engine.current?.stop(true);
  }, [projectId]);

  const running = currentTurn?.actions?.find((a) => a.status === "running")?.label;

  return (
    <div ref={root} className="ac" data-on={locked}>
      {SLOT_IDS.map((i) => (
        <div key={i} className="ac-slot" data-state="off" style={{ "--ac-c": COLORS[i] } as CSSProperties}>
          <div className="ac-box">
            <span className="ac-badge" />
          </div>
          <div className="ac-cursor">
            <div className="ac-cursor-body">
              <svg viewBox="0 0 16 16" aria-hidden="true">
                <path d="M1 1v12.6l3.3-2.9 2.3 4.9 2.2-1-2.3-4.8 4.4-.4z" />
              </svg>
              <span className="ac-tag t-micro" />
            </div>
          </div>
        </div>
      ))}
      <div className="ac-pill t-micro" aria-live="polite">
        <span className="ac-dots">
          {SLOT_IDS.map((i) => (
            <i key={i} className="ac-dot" data-on={active[i]} style={{ background: COLORS[i] }} />
          ))}
        </span>
        <span className="ac-pill-text">{running ?? "생각하는 중"}</span>
        <span className="ac-pill-count">레이어 {doc?.layers.length ?? 0}</span>
        <ProgressLine value={null} />
      </div>
    </div>
  );
}
