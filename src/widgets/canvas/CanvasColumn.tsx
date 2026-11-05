import { useEffect, useEffectEvent, useRef } from "react";
import { useEd } from "@/entities/editor";
import { STRATEGY_LONG } from "@/entities/layer";
import { objectActions } from "@/features/object-actions";
import type { Arrange } from "./arrange";
import { arrange, copyLayers, hasClip, pasteLayers } from "./commands";
import { dpiLabel, dpiVerdict, sizeLabel } from "@/shared/lib";
import { IconButton, setToastAnchor } from "@/shared/ui";
import { CanvasView } from "./CanvasView";
import { Rulers } from "./Rulers";
import { useViewport } from "./viewport";
import "./canvas.css";

const ORDER: Record<string, Arrange> = {
  BracketRight: "forward",
  "BracketRight+": "front",
  BracketLeft: "backward",
  "BracketLeft+": "back",
};

export function CanvasColumn() {
  const ed = useEd();
  const {
    doc,
    project,
    selection,
    tool,
    locked,
    pendingQuestion,
    currentTurn,
    note,
    chat,
    outline,
    setOutline,
  } = ed;
  const ref = useRef<HTMLDivElement>(null);
  const W = doc?.artboard.width_mm ?? 0;
  const H = doc?.artboard.height_mm ?? 0;
  const hasOriginal = (project?.originals.length ?? 0) > 0;
  const vp = useViewport(ref, W, H, hasOriginal);
  const dpi = project?.target_dpi ?? 100;

  const onKey = useEffectEvent((e: KeyboardEvent) => {
    if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
    if (document.querySelector("[role=dialog]")) return;
    const k = e.shiftKey ? "" : e.key.toLowerCase();
    const textPicked = !(window.getSelection()?.isCollapsed ?? true);
    const act =
      k === "y"
        ? () => setOutline((v) => !v)
        : k === "d"
          ? () => {
              const a = objectActions(ed);
              if (a.canEdit) a.duplicate();
            }
          : k === "c" && !textPicked && selection.length
            ? () => copyLayers(ed)
            : k === "v" && hasClip() && !locked
              ? () => void pasteLayers(ed)
              : e.code === "BracketRight" || e.code === "BracketLeft"
                ? () => arrange(ed, ORDER[`${e.code}${e.shiftKey ? "+" : ""}`])
                : null;
    if (!act) return;
    e.preventDefault();
    act();
  });
  useEffect(() => {
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const actions = currentTurn?.actions ?? [];
  const report = [...chat].reverse().find((c) => c.report)?.report;
  const one = selection.length === 1 ? doc?.layers.find((l) => l.id === selection[0]) : undefined;

  const status = (() => {
    if (locked) {
      const running = actions.find((a) => a.status === "running");
      return `에이전트 작업 중${running ? ` · ${running.label}` : ""}`;
    }
    if (pendingQuestion) return "에이전트 답을 기다리는 중";
    if (tool === "capture") return "캡처: 원본이나 작업본에서 끌어서 영역을 표시하세요";
    if (note) return note;
    if (one) return `선택: ${one.name} · ${one.kind.type === "group" ? "그룹" : STRATEGY_LONG[one.strategy]}`;
    if (selection.length > 1) return `선택: 레이어 ${selection.length}개`;
    const count = `레이어 ${doc?.layers.length ?? 0}`;
    if (!report) return count;
    return `${count} · ${dpiVerdict(report, dpi)}`;
  })();

  return (
    <section className="cv" ref={setToastAnchor}>
      <div className="cv-body">
        <Rulers panes={vp.panes} zoom={vp.view.zoom} W={W} H={H} size={vp.size} />
        <CanvasView viewportRef={ref} view={vp.view} setView={vp.setView} panes={vp.panes} />
      </div>
      <footer className="cv-status">
        <span className="t-micro c-secondary cv-status-left">{status}</span>
        {doc && (
          <span className="t-micro c-secondary">
            {sizeLabel(W, H)} · {dpiLabel(W, H, dpi)}
          </span>
        )}
        <IconButton
          icon="shape"
          label="윤곽선 보기"
          shortcut="Ctrl+Y"
          size={14}
          active={outline}
          onClick={() => setOutline((v) => !v)}
        />
        <IconButton icon="zoom-out" label="축소" size={14} onClick={() => vp.zoomBy(1 / 1.25)} />
        <span className="t-micro cv-zoom">{Math.round(vp.view.zoom * 100)}%</span>
        <IconButton icon="zoom-in" label="확대" size={14} onClick={() => vp.zoomBy(1.25)} />
        <IconButton icon="fit" label="화면에 맞추기" size={14} onClick={vp.fit} />
      </footer>
    </section>
  );
}
