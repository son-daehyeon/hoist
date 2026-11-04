import { useRef } from "react";
import { useEd } from "@/entities/editor";
import { STRATEGY_LONG } from "@/entities/layer";
import { dpiLabel, sizeLabel } from "@/shared/lib";
import { IconButton, ProgressLine, setToastAnchor } from "@/shared/ui";
import { CanvasView } from "./CanvasView";
import { ContextToolbar } from "./ContextToolbar";
import { Rulers } from "./Rulers";
import { useViewport } from "./viewport";
import "./canvas.css";

export function CanvasColumn({ onReplaceOriginal }: { onReplaceOriginal: () => void }) {
  const ed = useEd();
  const { doc, project, selection, tool, locked, pendingQuestion, currentTurn, note, chat } = ed;
  const ref = useRef<HTMLDivElement>(null);
  const W = doc?.artboard.width_mm ?? 0;
  const H = doc?.artboard.height_mm ?? 0;
  const hasOriginal = (project?.originals.length ?? 0) > 0;
  const vp = useViewport(ref, W, H, hasOriginal);
  const dpi = project?.target_dpi ?? 100;

  const actions = currentTurn?.actions ?? [];
  const progress = actions.length ? actions.filter((a) => a.status === "done").length / actions.length : null;
  const report = [...chat].reverse().find((c) => c.report)?.report;
  const one = selection.length === 1 ? doc?.layers.find((l) => l.id === selection[0]) : undefined;

  const status = (() => {
    if (locked) {
      const running = actions.find((a) => a.status === "running");
      return `에이전트 작업 중${running ? ` · ${running.label}` : ""}`;
    }
    if (pendingQuestion)
      return `원본 v${project?.originals.at(-1)?.version ?? 1} · 에이전트 질문에 답해 주세요`;
    if (tool === "capture") return "캡처: 원본이나 작업본에서 끌어서 영역을 표시하세요";
    if (note) return note;
    if (one) return `선택: ${one.name} · ${one.kind.type === "group" ? "그룹" : STRATEGY_LONG[one.strategy]}`;
    if (selection.length > 1) return `선택: 레이어 ${selection.length}개`;
    const count = `레이어 ${doc?.layers.length ?? 0}`;
    if (!report) return count;
    return `${count} · ${report.min_dpi >= dpi ? `실물 ${dpi}dpi 충족` : `실물 해상도 모자람 (최저 ${report.min_dpi}dpi)`}`;
  })();

  return (
    <section className="cv" ref={setToastAnchor}>
      <ContextToolbar onFit={vp.fit} onReplaceOriginal={onReplaceOriginal} />
      {locked && (
        <div className="cv-progress">
          <ProgressLine value={progress} />
        </div>
      )}
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
        <IconButton icon="zoom-out" label="축소" size={14} onClick={() => vp.zoomBy(1 / 1.25)} />
        <span className="t-micro cv-zoom">{Math.round(vp.view.zoom * 100)}%</span>
        <IconButton icon="zoom-in" label="확대" size={14} onClick={() => vp.zoomBy(1.25)} />
        <IconButton icon="fit" label="화면에 맞추기" size={14} onClick={vp.fit} />
      </footer>
    </section>
  );
}
