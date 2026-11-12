import type { ReactNode } from "react";
import { useEd } from "@/entities/editor";
import { Button, IconButton } from "@/shared/ui";
import { AppMenu } from "@/widgets/app-menu";
import { TitleBar } from "@/widgets/title-bar";

export type EditorModal = "export" | "replace" | "settings";

const COMPARE_LEVELS = [0, 0.5, 1];

export function EditorTitleBar({
  title,
  onNew,
  onOpenFolder,
  onModal,
  canImport,
  onPickImage,
}: {
  title: ReactNode;
  onNew: () => void;
  onOpenFolder: () => void;
  onModal: (modal: EditorModal) => void;
  canImport: boolean;
  onPickImage: () => void;
}) {
  const ed = useEd();
  const { doc, locked, compare, setCompare } = ed;
  return (
    <TitleBar
      left={
        <>
          <AppMenu
            onNew={onNew}
            onOpenFolder={onOpenFolder}
            onSettings={() => onModal("settings")}
            fileItems={[
              {
                label: "그림 넣기…",
                icon: "image",
                disabled: !canImport,
                onSelect: onPickImage,
              },
            ]}
            sections={[
              {
                title: "보기",
                items: [
                  {
                    label: "윤곽선 보기",
                    checked: ed.outline,
                    shortcut: "Ctrl+Y",
                    onSelect: () => ed.setOutline(!ed.outline),
                  },
                  {
                    label: "재단선 · 안전 영역 보기",
                    checked: ed.guides,
                    onSelect: () => ed.setGuides(!ed.guides),
                  },
                  "divider",
                  ...COMPARE_LEVELS.map((v) => ({
                    label: v ? `원본 겹쳐 보기 ${v * 100}%` : "원본 겹쳐 보기 끄기",
                    checked: compare === v,
                    onSelect: () => setCompare(v),
                  })),
                ],
              },
            ]}
          />
        </>
      }
      title={title}
      right={
        <>
          <IconButton
            icon="undo"
            label="되돌리기"
            shortcut="Ctrl+Z"
            disabled={!ed.canUndo || locked}
            onClick={ed.undo}
          />
          <IconButton
            icon="redo"
            label="다시 하기"
            shortcut="Ctrl+Shift+Z"
            disabled={!ed.canRedo || locked}
            onClick={ed.redo}
          />
          <span className="titlebar-sep" />
          <Button kind="ghost" icon="clock" onClick={() => ed.setTab("history")} data-tour="history">
            기록
          </Button>
          <Button kind="ghost" icon="swap" disabled={locked} onClick={() => onModal("replace")}>
            원본 교체
          </Button>
          <span className="titlebar-sep" />
          <Button
            kind="primary"
            icon="download"
            disabled={!doc}
            onClick={() => onModal("export")}
            data-tour="export"
          >
            내보내기
          </Button>
        </>
      }
    />
  );
}
