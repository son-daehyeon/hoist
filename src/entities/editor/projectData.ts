import { useCallback, useRef, useState } from "react";
import {
  api,
  errorMessage,
  type ApplyResult,
  type ChatItem,
  type Document,
  type Op,
  type OpenedProject,
  type Project,
  type Version,
} from "@/shared/api";
import { toast } from "@/shared/ui";

export function useProjectData(projectId: string) {
  const [project, setProject] = useState<Project | null>(null);
  const [doc, setDoc] = useState<Document | null>(null);
  const [chat, setChat] = useState<ChatItem[]>([]);
  const [versions, setVersions] = useState<Version[]>([]);
  const [memory, setMemory] = useState("");
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);

  const takeDoc = useCallback((d: Document) => setDoc((cur) => (!cur || d.rev >= cur.rev ? d : cur)), []);

  const take = useCallback(
    (o: OpenedProject, withChat: boolean) => {
      setProject(o.project);
      takeDoc(o.document);
      setVersions(o.versions);
      setMemory(o.memory);
      setCanUndo(o.can_undo);
      setCanRedo(o.can_redo);
      if (withChat) setChat(o.chat);
    },
    [takeDoc],
  );

  const inflight = useRef(false);
  const again = useRef<boolean | null>(null);
  const refresh = useCallback(
    (withChat = false) => {
      function run(withChat: boolean) {
        if (inflight.current) {
          again.current = (again.current ?? false) || withChat;
          return;
        }
        inflight.current = true;
        api
          .openProject(projectId)
          .then((o) => take(o, withChat))
          .catch((e) => toast(errorMessage(e), "error"))
          .finally(() => {
            inflight.current = false;
            const next = again.current;
            again.current = null;
            if (next !== null) run(next);
          });
      }
      run(withChat);
    },
    [projectId, take],
  );

  const applyResult = useCallback(
    (r: ApplyResult) => {
      takeDoc(r.document);
      setCanUndo(r.can_undo);
      setCanRedo(r.can_redo);
    },
    [takeDoc],
  );

  const ops = useCallback(
    async (list: Op[]) => {
      try {
        applyResult(await api.applyOps(projectId, list, "user"));
        return true;
      } catch (e) {
        toast(errorMessage(e), "error");
        return false;
      }
    },
    [projectId, applyResult],
  );

  const runHistory = useCallback(
    async (fn: (id: string) => Promise<ApplyResult>) => {
      try {
        applyResult(await fn(projectId));
      } catch (e) {
        toast(errorMessage(e), "error");
      }
    },
    [projectId, applyResult],
  );

  return {
    project,
    setProject,
    doc,
    chat,
    setChat,
    versions,
    memory,
    canUndo,
    canRedo,
    take,
    refresh,
    applyResult,
    ops,
    runHistory,
  };
}
