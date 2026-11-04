import { useEffect, useState } from "react";
import { AssistantPanel } from "../editor/AssistantPanel";
import { CanvasColumn } from "../editor/CanvasColumn";
import { ExportDialog } from "../editor/ExportDialog";
import { LayerPanel } from "../editor/LayerPanel";
import { ReplaceOriginalDialog } from "../editor/ReplaceOriginalDialog";
import { EditorCtx, useEditorState, type Tool } from "../editor/state";
import { fileName, sizeLabel } from "../lib/format";
import { AppMenu, pickProjectFolder } from "../shared/AppMenu";
import { SettingsDialog } from "../shared/SettingsDialog";
import { TitleBar } from "../shared/TitleBar";
import { Button, EmptyState, IconButton, ToolButton } from "../ui";
import "../editor/editor.css";

type Modal = "export" | "replace" | "settings" | null;

const TOOLS: { tool: Tool; icon: string; label: string; key: string }[] = [
  { tool: "select", icon: "cursor", label: "선택", key: "V" },
  { tool: "hand", icon: "hand", label: "이동", key: "H" },
  { tool: "text", icon: "type", label: "글자", key: "T" },
  { tool: "shape", icon: "square", label: "도형", key: "R" },
  { tool: "capture", icon: "crop", label: "캡처", key: "C" },
];
const KEY_TOOL: Record<string, Tool> = { v: "select", h: "hand", t: "text", r: "shape", c: "capture" };

export default function Editor({
  projectId,
  firstMessage,
  onHome,
  onOpen,
}: {
  projectId: string;
  firstMessage?: string;
  onHome: (newProject?: boolean) => void;
  onOpen: (id: string) => void;
}) {
  const ed = useEditorState(projectId, firstMessage);
  const [modal, setModal] = useState<Modal>(null);
  const { project, doc, locked, selection, tool, setTool, compare, setCompare } = ed;

  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      const t = e.target;
      if (modal || t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || t instanceof HTMLSelectElement) return;
      const k = e.key.toLowerCase();
      if (e.ctrlKey || e.metaKey) {
        const action =
          k === "n" ? () => onHome(true)
          : k === "o" ? () => void pickProjectFolder().then((id) => id && onOpen(id))
          : k === "e" ? () => setModal("export")
          : locked ? null
          : k === "z" && !e.shiftKey ? ed.undo
          : k === "y" || (k === "z" && e.shiftKey) ? ed.redo
          : null;
        if (action) {
          e.preventDefault();
          action();
        }
        return;
      }
      if (e.altKey) return;
      if (k === "escape") {
        ed.setSelection([]);
        ed.setCaptures([]);
        return setTool("select");
      }
      if (locked) return;
      if (KEY_TOOL[k]) return setTool(KEY_TOOL[k]);
      if ((k === "delete" || k === "backspace") && selection.length) {
        e.preventDefault();
        void ed.ops(selection.map((id) => ({ op: "delete_layer" as const, id })));
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [modal, locked, selection, ed, onHome, onOpen, setTool]);

  if (ed.loadError) {
    return (
      <div className="app">
        <TitleBar left={<IconButton icon="arrow-left" label="처음 화면" onClick={() => onHome()} />} title={null} />
        <div className="app-error">
          <EmptyState
            icon="alert"
            title="프로젝트를 열지 못했어요"
            description={ed.loadError}
            action={<Button kind="primary" onClick={() => onHome()}>처음 화면으로</Button>}
          />
        </div>
      </div>
    );
  }

  return (
    <EditorCtx.Provider value={ed}>
      <div className="app">
        <TitleBar
          left={
            <>
              <AppMenu
                onNew={() => onHome(true)}
                onOpenFolder={() => void pickProjectFolder().then((id) => id && onOpen(id))}
                onSettings={() => setModal("settings")}
              />
              <IconButton icon="arrow-left" label="처음 화면" onClick={() => onHome()} />
            </>
          }
          title={
            project && (
              <>
                <span className="t-label-m">{project.name}</span>
                <span className="t-caption c-tertiary">{sizeLabel(project.width_mm, project.height_mm)}</span>
              </>
            )
          }
          right={
            <>
              <IconButton icon="undo" label="되돌리기" shortcut="Ctrl+Z" disabled={!ed.canUndo || locked} onClick={ed.undo} />
              <IconButton icon="redo" label="다시 하기" shortcut="Ctrl+Y" disabled={!ed.canRedo || locked} onClick={ed.redo} />
              <span className="titlebar-sep" />
              <Button kind="ghost" icon="clock" onClick={() => ed.setTab("history")}>
                기록
              </Button>
              <Button kind="ghost" icon="swap" disabled={locked} onClick={() => setModal("replace")}>
                원본 교체
              </Button>
              <span className="titlebar-sep" />
              <Button kind="primary" icon="download" disabled={!doc} onClick={() => setModal("export")}>
                내보내기
              </Button>
            </>
          }
        />
        <div className="ed-body">
          <div className="ed-work">
            <nav className="ed-rail">
              {TOOLS.map((t) => (
                <ToolButton
                  key={t.tool}
                  icon={t.icon}
                  label={t.label}
                  shortcut={t.key}
                  active={tool === t.tool}
                  onClick={() => setTool(t.tool)}
                />
              ))}
              <ToolButton
                icon="compare"
                label="비교"
                active={compare > 0}
                onClick={() => setCompare(compare > 0 ? 0 : 0.5)}
              />
            </nav>
            <LayerPanel />
            <CanvasColumn onReplaceOriginal={() => setModal("replace")} />
            {locked && <div className="ed-lock" />}
          </div>
          <AssistantPanel />
        </div>

        {modal === "settings" && <SettingsDialog onClose={() => setModal(null)} />}
        {modal === "replace" && <ReplaceOriginalDialog onClose={() => setModal(null)} />}
        {modal === "export" && (
          <ExportDialog
            onClose={() => setModal(null)}
            onDone={(r, desktop) => {
              setModal(null);
              ed.setNote(`${desktop ? "바탕화면에 " : ""}저장함 · ${fileName(r.path)}`);
            }}
            onFix={(ids) => {
              setModal(null);
              ed.setSelection(ids);
            }}
          />
        )}
      </div>
    </EditorCtx.Provider>
  );
}
