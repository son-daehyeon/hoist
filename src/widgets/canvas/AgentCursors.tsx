import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type RefObject } from "react";
import { useEd } from "@/entities/editor";
import type { Layer } from "@/shared/api";
import { ProgressLine } from "@/shared/ui";
import { diffLayers } from "./agent-cursors";
import { makeEngine, SLOT_IDS, type Engine, type Geom, type SlotEls } from "./agent-cursors-engine";
import { frameOf } from "./selection";
import type { Rect } from "./viewport";
import "./agent-cursors.css";

const COLORS = ["var(--blue-500)", "#12b76a", "#f0640d", "#7a5af8"];

const reducedMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

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
  const [replaying, setReplaying] = useState(false);

  useLayoutEffect(() => {
    els.current = [...(root.current?.querySelectorAll<HTMLElement>(".ac-slot") ?? [])].map((slot) => ({
      slot,
      cursor: slot.querySelector<HTMLElement>(".ac-cursor")!,
      tag: slot.querySelector<HTMLElement>(".ac-tag")!,
      box: slot.querySelector<HTMLElement>(".ac-box")!,
      badge: slot.querySelector<HTMLElement>(".ac-badge")!,
    }));
    const e = makeEngine(svgRef, worldRef, geom, els, setActive, setReplaying);
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
    const changes = diffLayers(prev, layers, frameOf);
    if (changes.length) engine.current?.enqueue(changes, doc.artboard.width_mm, doc.artboard.height_mm);
  }, [doc, locked, projectId]);

  useEffect(() => {
    engine.current?.stop(true);
  }, [projectId]);

  const running = currentTurn?.actions?.find((a) => a.status === "running")?.label;

  return (
    <div ref={root} className="ac" data-on={locked || replaying}>
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
        <span className="ac-pill-text">{running ?? (locked ? "생각하는 중" : "작업 다시 보여 주는 중")}</span>
        <ProgressLine value={null} />
      </div>
    </div>
  );
}
