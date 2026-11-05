import { useEd } from "@/entities/editor";
import { BOOLEAN_OPS, objectActions } from "@/features/object-actions";
import { Button } from "@/shared/ui";
import { PropSection } from "./Section";

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

export function InsertSection() {
  const a = objectActions(useEd());
  return (
    <PropSection id="insert" title="넣기">
      <Button icon="image" disabled={!a.canImport} onClick={() => void a.pickImage()}>
        그림 넣기…
      </Button>
      <span className="t-body-m c-tertiary">그림 파일을 창에 끌어다 놓아도 돼요.</span>
    </PropSection>
  );
}
