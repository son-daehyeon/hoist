import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { api, type ApplyResult, type OpenedProject } from "../backend";
import { errorMessage } from "../lib/format";
import { last } from "../lib/tree";
import type { ActionItem, AgentEvent, Capture, ChatItem, Document, LayerId, Op, Project, Version } from "../types";
import { toast } from "../ui";

export type Tool = "select" | "hand" | "text" | "shape" | "capture";

function upsertAction(list: ActionItem[] = [], a: ActionItem) {
  const i = list.findIndex((x) => x.id === a.id);
  return i < 0 ? [...list, a] : list.map((x, j) => (j === i ? a : x));
}

export function useEditorState(projectId: string, firstMessage?: string) {
  const [project, setProject] = useState<Project | null>(null);
  const [doc, setDoc] = useState<Document | null>(null);
  const [chat, setChat] = useState<ChatItem[]>([]);
  const [versions, setVersions] = useState<Version[]>([]);
  const [memory, setMemory] = useState("");
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);
  const [busy, setBusy] = useState(false);
  const [sending, setSending] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [selection, setSelection] = useState<LayerId[]>([]);
  const [tool, setTool] = useState<Tool>("select");
  const [compare, setCompare] = useState(0);
  const [guides, setGuides] = useState(false);
  const [captures, setCaptures] = useState<Capture[]>([]);
  const [tab, setTab] = useState<"assistant" | "history">("assistant");
  const [note, setNote] = useState<string | null>(null);

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
          if (next !== null) refresh(next);
        });
    },
    [projectId, take],
  );

  const applied = useCallback(
    (r: ApplyResult) => {
      takeDoc(r.document);
      setCanUndo(r.can_undo);
      setCanRedo(r.can_redo);
      refresh();
    },
    [takeDoc, refresh],
  );

  const ops = useCallback(
    async (list: Op[]) => {
      try {
        applied(await api.applyOps(projectId, list, "user"));
        return true;
      } catch (e) {
        toast(errorMessage(e), "error");
        return false;
      }
    },
    [projectId, applied],
  );

  const send = useCallback(
    async (text: string, ctx = { layer_ids: [] as LayerId[], captures: [] as Capture[] }) => {
      setChat((c) => [...c, { id: `local-${Date.now()}`, role: "user", ts: new Date().toISOString(), text, context: ctx }]);
      setCaptures([]);
      setSending(true);
      try {
        await api.agentSend(projectId, text, ctx);
      } catch (e) {
        toast(errorMessage(e), "error");
      } finally {
        setSending(false);
      }
    },
    [projectId],
  );

  const first = useRef(firstMessage);
  useEffect(() => {
    let alive = true;
    api
      .openProject(projectId)
      .then((o) => {
        if (!alive) return;
        take(o, true);
        const msg = first.current;
        first.current = undefined;
        if (msg && o.chat.length === 0) void send(msg);
      })
      .catch((e) => alive && setLoadError(errorMessage(e)));
    return () => {
      alive = false;
    };
  }, [projectId, take, send]);

  useEffect(() => {
    const onEvent = (e: AgentEvent & { project_id: string }) => {
      if (e.project_id !== projectId) return;
      const patch = (fn: (it: ChatItem) => ChatItem) =>
        setChat((c) => {
          const i = c.findIndex((x) => x.id === e.turn);
          if (i < 0) return [...c, fn({ id: e.turn, role: "agent", ts: new Date().toISOString(), text: "" })];
          return c.map((x, j) => (j === i ? fn(x) : x));
        });
      switch (e.type) {
        case "turn_start":
          setBusy(true);
          return patch((it) => it);
        case "text_delta":
          return patch((it) => ({ ...it, text: it.text + e.delta }));
        case "text":
          return patch((it) => ({ ...it, text: e.text }));
        case "action":
          return patch((it) => ({ ...it, actions: upsertAction(it.actions, e.action) }));
        case "report":
          return patch((it) => ({ ...it, report: e.report }));
        case "question":
          return patch((it) => ({ ...it, question: e.question }));
        case "document_changed":
          return refresh();
        case "turn_end":
          setBusy(false);
          if (e.stopped) toast("작업을 멈췄어요");
          return refresh(true);
        case "error":
          return toast(e.message, "error");
      }
    };
    let off: (() => void) | undefined;
    let alive = true;
    api
      .onAgentEvent(onEvent)
      .then((fn) => (alive ? (off = fn) : fn()))
      .catch((e) => toast(errorMessage(e), "error"));
    return () => {
      alive = false;
      off?.();
    };
  }, [projectId, refresh]);

  useEffect(() => {
    if (!doc) return;
    const ids = new Set(doc.layers.map((l) => l.id));
    setSelection((s) => (s.every((id) => ids.has(id)) ? s : s.filter((id) => ids.has(id))));
  }, [doc]);

  useEffect(() => setNote(null), [selection]);

  const lastItem = last(chat);
  const pendingQuestion = lastItem?.question && lastItem.answer === undefined ? lastItem : null;
  const currentTurn = busy ? [...chat].reverse().find((c) => c.role === "agent") : undefined;

  const history = useCallback(
    async (fn: (id: string) => Promise<ApplyResult>) => {
      try {
        applied(await fn(projectId));
      } catch (e) {
        toast(errorMessage(e), "error");
      }
    },
    [projectId, applied],
  );

  return {
    projectId,
    project,
    doc,
    chat,
    versions,
    memory,
    canUndo,
    canRedo,
    busy,
    loadError,
    selection,
    setSelection,
    tool,
    setTool,
    compare,
    setCompare,
    guides,
    setGuides,
    captures,
    setCaptures,
    tab,
    setTab,
    note,
    setNote,
    setProject,
    pendingQuestion,
    currentTurn,
    locked: busy && !pendingQuestion,
    composerLock: pendingQuestion ? ("question" as const) : busy || sending ? ("busy" as const) : null,
    ops,
    send,
    undo: () => history(api.undo),
    redo: () => history(api.redo),
    restore: (seq: number) => history((id) => api.restoreVersion(id, seq)),
    stop: () => void api.agentStop(projectId).catch((e) => toast(errorMessage(e), "error")),
    answer: (item: ChatItem, option: string) => {
      if (!item.question) return;
      const qid = item.question.id;
      setChat((c) => c.map((x) => (x.id === item.id ? { ...x, answer: option } : x)));
      api.agentAnswer(projectId, qid, option).catch((e) => toast(errorMessage(e), "error"));
    },
    refresh,
  };
}

export type EditorState = ReturnType<typeof useEditorState>;

export const EditorCtx = createContext<EditorState | null>(null);

export function useEd() {
  const v = useContext(EditorCtx);
  if (!v) throw new Error("EditorCtx missing");
  return v;
}

export function useLayer(id: LayerId | undefined) {
  const { doc } = useEd();
  return useMemo(() => doc?.layers.find((l) => l.id === id), [doc, id]);
}
