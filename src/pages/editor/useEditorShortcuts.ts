import { useEffect, useEffectEvent } from "react";
import type { EditorState } from "@/entities/editor";
import { selectAll } from "@/features/object-actions";
import { isFormField, modalOpen } from "@/shared/lib";
import { KEY_TOOL } from "./tools";

export function useEditorShortcuts(
  ed: EditorState,
  { onNew, onOpenFolder, onExport }: { onNew: () => void; onOpenFolder: () => void; onExport: () => void },
) {
  const { locked, selection, setTool } = ed;
  const withCtrl = (k: string, shift: boolean) => {
    if (k === "n") return onNew;
    if (k === "o") return onOpenFolder;
    if (k === "e") return onExport;
    if (locked) return null;
    if (k === "z") return shift ? ed.redo : ed.undo;
    if (k === "a") return () => void selectAll(ed);
    return null;
  };
  const onKey = useEffectEvent((e: KeyboardEvent) => {
    if (e.defaultPrevented || modalOpen() || isFormField(e.target)) return;
    const k = e.key.toLowerCase();
    if (e.ctrlKey || e.metaKey) {
      const action = withCtrl(k, e.shiftKey);
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
    if ((k === "delete" || k === "backspace") && selection.length && !ed.editing) {
      e.preventDefault();
      void ed.ops(selection.map((id) => ({ op: "delete_layer" as const, id })));
    }
  });
  useEffect(() => {
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
