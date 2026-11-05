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
          <Button
            key={d.axis}
            title="3개 이상 고르면 끝의 두 개는 그대로 두고 사이 간격을 똑같이 맞춰요"
            disabled={!a.canDistribute}
            onClick={() => a.distribute(d.axis)}
          >
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
        <Button
          icon="pen"
          title="글자를 도형 패스로 바꿔요 (글꼴 없이도 똑같이 나와요)"
          onClick={() => void a.outline()}
        >
          윤곽선 만들기
        </Button>
      )}
      <div className="pp-grid2">
        <Button
          icon="copy"
          title="오른쪽 아래로 10mm 옮겨서 복제해요 (Ctrl+D)"
          disabled={!a.canEdit}
          onClick={a.duplicate}
        >
          복제
        </Button>
        <Button icon="trash" title="지워요 (Delete)" disabled={!a.canEdit} onClick={a.remove}>
          삭제
        </Button>
      </div>
    </PropSection>
  );
}
