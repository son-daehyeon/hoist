import { useState } from "react";
import { Button, Dialog, Field } from "../ui";

export function RenameDialog({
  initial,
  onClose,
  onSave,
}: {
  initial: string;
  onClose: () => void;
  onSave: (name: string) => void;
}) {
  const [name, setName] = useState(initial);
  const ok = name.trim() !== "" && name.trim() !== initial;
  return (
    <Dialog
      title="이름 바꾸기"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>취소</Button>
          <Button kind="primary" disabled={!ok} onClick={() => onSave(name.trim())}>
            바꾸기
          </Button>
        </>
      }
    >
      <Field
        size="l"
        label="이름"
        autoFocus
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && ok && onSave(name.trim())}
      />
    </Dialog>
  );
}

export function ConfirmDeleteDialog({
  name,
  onClose,
  onConfirm,
}: {
  name: string;
  onClose: () => void;
  onConfirm: () => void;
}) {
  return (
    <Dialog
      title="프로젝트 삭제"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>취소</Button>
          <Button kind="danger" onClick={onConfirm}>
            삭제
          </Button>
        </>
      }
    >
      <div className="t-body-m">‘{name}’을(를) 삭제할까요?</div>
      <div className="t-caption c-tertiary">삭제하면 되돌릴 수 없어요.</div>
    </Dialog>
  );
}
