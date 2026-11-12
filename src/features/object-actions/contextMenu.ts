import { createContext } from "react";
import type { EditorState } from "@/entities/editor";
import { isEffectivelyHidden } from "@/shared/lib";
import type { MenuItem } from "@/shared/ui";
import { alignMenu, BOOLEAN_OPS, objectActions } from "./objectActions";

export type Arrange = "front" | "forward" | "backward" | "back";

export type ContextCommands = {
  copy: () => void;
  paste: () => void;
  canPaste: boolean;
  arrange: (dir: Arrange) => void;
  crop: (() => void) | null;
};

export const ContextCommandsCtx = createContext<ContextCommands | null>(null);

const ARRANGE: { dir: Arrange; label: string; shortcut: string }[] = [
  { dir: "front", label: "맨 앞으로", shortcut: "Ctrl+Shift+]" },
  { dir: "forward", label: "앞으로", shortcut: "Ctrl+]" },
  { dir: "backward", label: "뒤로", shortcut: "Ctrl+[" },
  { dir: "back", label: "맨 뒤로", shortcut: "Ctrl+Shift+[" },
];

export function selectAll({ doc, setSelection }: EditorState) {
  const layers = doc?.layers ?? [];
  const ids = layers
    .filter((l) => l.kind.type !== "group" && !l.locked && !isEffectivelyHidden(layers, l))
    .map((l) => l.id);
  setSelection(ids);
  return ids.length > 0;
}

export function contextMenuItems(
  ed: EditorState,
  c: ContextCommands,
  { blank }: { blank: boolean },
): MenuItem[] {
  const a = objectActions(ed);
  const paste: MenuItem = {
    label: "붙여넣기",
    shortcut: "Ctrl+V",
    disabled: ed.locked || !c.canPaste,
    onSelect: c.paste,
  };
  if (blank || ed.selection.length === 0) {
    return [
      paste,
      { label: "전체 선택", shortcut: "Ctrl+A", onSelect: () => selectAll(ed) },
      "divider",
      { label: "그림 넣기…", icon: "image", disabled: !a.canImport, onSelect: () => void a.pickImage() },
      {
        label: "윤곽선 보기",
        checked: ed.outline,
        shortcut: "Ctrl+Y",
        onSelect: () => ed.setOutline(!ed.outline),
      },
    ];
  }
  const shape: MenuItem[] = [
    ...(a.canOutline
      ? [{ label: "윤곽선 만들기", icon: "pen", onSelect: () => void a.outline() } satisfies MenuItem]
      : []),
    ...(c.crop && !ed.locked
      ? [{ label: "그림 자르기", icon: "crop", onSelect: c.crop } satisfies MenuItem]
      : []),
  ];
  const paths: MenuItem[] = a.canCombine
    ? BOOLEAN_OPS.map((b) => ({ label: `패스 ${b.label}`, onSelect: () => void a.combine(b.op) }))
    : [];
  return [
    { label: "복사", icon: "copy", shortcut: "Ctrl+C", onSelect: c.copy },
    paste,
    { label: "복제", shortcut: "Ctrl+D", disabled: !a.canEdit, onSelect: a.duplicate },
    { label: "삭제", icon: "trash", shortcut: "Delete", disabled: !a.canEdit, onSelect: a.remove },
    ...(shape.length ? ["divider" as const, ...shape] : []),
    ...(paths.length ? ["divider" as const, ...paths] : []),
    "divider",
    ...ARRANGE.map((r): MenuItem => ({
      label: r.label,
      shortcut: r.shortcut,
      disabled: ed.locked,
      onSelect: () => c.arrange(r.dir),
    })),
    "divider",
    { heading: "정렬" },
    ...alignMenu(a),
  ];
}
