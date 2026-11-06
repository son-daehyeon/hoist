import { useState } from "react";
import type { Folder } from "@/shared/api";
import { Button, Dialog, Field, Icon } from "@/shared/ui";
import "./project-dialogs.css";

/** Asks for a name: a new folder, or a new name for a project or folder. */
export function NameDialog({
  title,
  initial,
  confirm = "바꾸기",
  onClose,
  onSave,
}: {
  title: string;
  initial: string;
  confirm?: string;
  onClose: () => void;
  onSave: (name: string) => void;
}) {
  const [name, setName] = useState(initial);
  const ok = name.trim() !== "" && name.trim() !== initial;
  return (
    <Dialog
      title={title}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>취소</Button>
          <Button kind="primary" disabled={!ok} onClick={() => onSave(name.trim())}>
            {confirm}
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

/** Confirms something that cannot be undone. Moving a project to the trash does not ask. */
export function ConfirmDialog({
  title,
  body,
  note,
  confirm,
  onClose,
  onConfirm,
}: {
  title: string;
  body: string;
  note: string;
  confirm: string;
  onClose: () => void;
  onConfirm: () => void;
}) {
  return (
    <Dialog
      title={title}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>취소</Button>
          <Button kind="danger" onClick={onConfirm}>
            {confirm}
          </Button>
        </>
      }
    >
      <div className="t-body-m">{body}</div>
      <div className="t-caption c-tertiary">{note}</div>
    </Dialog>
  );
}

/** Picks the folder a project goes to: the folder tree, with the top level first. */
export function MoveDialog({
  name,
  folders,
  current,
  onClose,
  onMove,
}: {
  name: string;
  folders: Folder[];
  current: string | null;
  onClose: () => void;
  onMove: (folder: string | null) => void;
}) {
  const [picked, setPicked] = useState<string | null>(current);
  const rows: { id: string | null; name: string; depth: number }[] = [
    { id: null, name: "프로젝트", depth: 0 },
  ];
  const walk = (parent: string | null, depth: number) => {
    for (const f of folders
      .filter((x) => (x.parent ?? null) === parent)
      .sort((a, b) => a.name.localeCompare(b.name, "ko"))) {
      rows.push({ id: f.id, name: f.name, depth });
      walk(f.id, depth + 1);
    }
  };
  walk(null, 1);
  return (
    <Dialog
      title="폴더로 옮기기"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>취소</Button>
          <Button kind="primary" disabled={picked === current} onClick={() => onMove(picked)}>
            옮기기
          </Button>
        </>
      }
    >
      <div className="t-caption c-tertiary">‘{name}’을(를) 옮길 곳을 고르세요.</div>
      <div className="pm-folders" role="listbox">
        {rows.map((r) => (
          <button
            key={r.id ?? "root"}
            type="button"
            role="option"
            aria-selected={picked === r.id}
            className={`pm-folder t-label-m ${picked === r.id ? "is-picked" : ""}`}
            style={{ paddingLeft: 8 + r.depth * 16 }}
            onClick={() => setPicked(r.id)}
            onDoubleClick={() => r.id !== current && onMove(r.id)}
          >
            <Icon name={r.id === null ? "home" : "folder"} size={16} />
            <span className="bs-ellipsis">{r.name}</span>
            {r.id === current && <span className="t-caption c-tertiary">지금 위치</span>}
          </button>
        ))}
      </div>
    </Dialog>
  );
}
