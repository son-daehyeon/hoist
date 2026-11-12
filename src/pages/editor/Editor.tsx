import { useEffect, useState, type ReactNode } from "react";
import { AssistantPanel } from "@/widgets/assistant-panel";
import { arrange, CanvasColumn, copyLayers, cropStart, pasteLayers, useHasClip } from "@/widgets/canvas";
import { ExportDialog } from "@/features/export";
import { AgentQuestion } from "@/features/agent-question";
import { LayerPanel } from "@/widgets/layer-panel";
import { PropertiesPanel } from "@/widgets/properties-panel";
import { ReplaceOriginalDialog } from "@/features/replace-original";
import { EditorCtx, useEditorState } from "@/entities/editor";
import { tutorial, tutorialProject } from "@/entities/onboarding";
import { practiceAgent } from "@/features/onboarding";
import { fileName } from "@/shared/lib";
import { pickProjectFolder } from "@/features/project-open";
import { ContextCommandsCtx, objectActions, type ContextCommands } from "@/features/object-actions";
import { SettingsDialog } from "@/features/settings";
import { TitleBar } from "@/widgets/title-bar";
import { Button, ColumnResizer, EmptyState, Logo, useColumnWidth } from "@/shared/ui";
import { EditorTitleBar, type EditorModal } from "./EditorTitleBar";
import { ToolRail } from "./ToolRail";
import { useEditorShortcuts } from "./useEditorShortcuts";
import { useImageDrop } from "./useImageDrop";
import { usePropsHeight } from "./usePropsHeight";
import "./editor.css";

function LoadFailed({ title, message, onHome }: { title: ReactNode; message: string; onHome: () => void }) {
  return (
    <div className="app">
      <TitleBar left={null} title={title} />
      <div className="app-error">
        <EmptyState
          icon="alert"
          title="프로젝트를 열지 못했어요"
          description={message}
          action={
            <Button kind="primary" onClick={onHome}>
              처음 화면으로
            </Button>
          }
        />
      </div>
    </div>
  );
}

export function Editor({
  tabs,
  projectId,
  firstMessage,
  onName,
  onHome,
  onOpen,
}: {
  tabs: ReactNode;
  projectId: string;
  firstMessage?: string;
  onName: (name: string) => void;
  onHome: (newProject?: boolean) => void;
  onOpen: (id: string) => void;
}) {
  const ed = useEditorState(
    projectId,
    firstMessage,
    projectId === tutorialProject() ? practiceAgent : undefined,
  );
  // The practice tour waits for the title to be picked.
  useEffect(() => {
    if (projectId === tutorialProject()) tutorial.selection = ed.selection;
  }, [projectId, ed.selection]);
  const [modal, setModal] = useState<EditorModal | null>(null);
  const propsHeight = usePropsHeight();
  const left = useColumnWidth("hoist.left-width", { min: 200, max: 560, initial: 240, edge: "right" });
  const { project, doc, locked, selection } = ed;
  const name = project?.name;
  useEffect(() => {
    if (name) onName(name);
  }, [name, onName]);
  const actions = objectActions(ed);
  const canPaste = useHasClip();
  const onlySelected = selection.length === 1 ? doc?.layers.find((l) => l.id === selection[0]) : undefined;
  const cropTarget = onlySelected?.kind.type === "image" && !onlySelected.locked ? onlySelected : undefined;
  const commands: ContextCommands = {
    copy: () => void copyLayers(ed),
    paste: () => void pasteLayers(ed),
    canPaste,
    arrange: (dir) => arrange(ed, dir),
    crop: cropTarget ? () => ed.setEditing(cropStart(cropTarget)) : null,
  };
  const openNew = () => onHome(true);
  const openFolder = () => void pickProjectFolder().then((id) => id && onOpen(id));
  const htmlDrop = useImageDrop(actions.importImages, modal !== null);
  useEditorShortcuts(ed, {
    onNew: openNew,
    onOpenFolder: openFolder,
    onExport: () => setModal("export"),
  });
  const title = (
    <>
      <Logo size={20} />
      {tabs}
    </>
  );

  if (ed.loadError) return <LoadFailed title={title} message={ed.loadError} onHome={() => onHome()} />;

  return (
    <EditorCtx.Provider value={ed}>
      <ContextCommandsCtx.Provider value={commands}>
        <div className="app" {...htmlDrop}>
          <EditorTitleBar
            title={title}
            onNew={openNew}
            onOpenFolder={openFolder}
            onModal={setModal}
            canImport={actions.canImport}
            onPickImage={() => void actions.pickImage()}
          />
          <div className="ed-body">
            <div className="ed-work">
              <ToolRail />
              <div className="ed-left" style={{ width: left.width }}>
                <ColumnResizer column={left} label="속성·레이어와 작업 화면 사이 경계" />
                <div className="ed-props" style={{ height: propsHeight.height }}>
                  <PropertiesPanel />
                </div>
                <div
                  className={`ed-split ${propsHeight.resizing ? "is-active" : ""}`}
                  role="separator"
                  aria-orientation="horizontal"
                  aria-label="속성과 레이어 사이 경계"
                  title="끌어서 높이 바꾸기"
                  {...propsHeight.handle}
                />
                <LayerPanel />
              </div>
              <CanvasColumn />
              {locked && <div className="ed-lock" />}
            </div>
            <AssistantPanel />
          </div>

          <AgentQuestion />
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
      </ContextCommandsCtx.Provider>
    </EditorCtx.Provider>
  );
}
