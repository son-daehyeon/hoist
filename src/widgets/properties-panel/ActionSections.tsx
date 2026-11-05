import { useEd } from "@/entities/editor";
import { ALIGN_MODES, BOOLEAN_OPS, DISTRIBUTE_AXES, objectActions } from "@/features/object-actions";
import { Button } from "@/shared/ui";
import { PropSection } from "./Section";

export function AlignSection() {
  const a = objectActions(useEd());
  return (
    <PropSection id="align" title={`정렬 (${a.alignTarget} 기준)`}>
      <div className="pp-grid3">
        {ALIGN_MODES.map((m) => (
          <Button key={m.mode} title={m.menu} disabled={!a.canAlign} onClick={() => a.align(m.mode)}>
            {m.label}
          </Button>
        ))}
      </div>
      <div className="pp-grid2">
        {DISTRIBUTE_AXES.map((d) => (
          <Button key={d.axis} disabled={!a.canDistribute} onClick={() => a.distribute(d.axis)}>
            {d.label}
          </Button>
        ))}
      </div>
    </PropSection>
  );
}

export function ActionSection() {
  const a = objectActions(useEd());
  return (
    <PropSection id="actions" title="작업">
      {a.canCombine && (
        <div className="pp-grid2">
          {BOOLEAN_OPS.map((b) => (
            <Button key={b.op} title={b.hint} onClick={() => void a.combine(b.op)}>
              {b.label}
            </Button>
          ))}
        </div>
      )}
      {a.canOutline && (
        <Button icon="pen" onClick={() => void a.outline()}>
          윤곽선 만들기
        </Button>
      )}
      <div className="pp-grid2">
        <Button icon="copy" title="Ctrl+D" disabled={!a.canEdit} onClick={a.duplicate}>
          복제
        </Button>
        <Button icon="trash" title="Delete" disabled={!a.canEdit} onClick={a.remove}>
          삭제
        </Button>
      </div>
    </PropSection>
  );
}
