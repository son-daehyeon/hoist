import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { api, errorMessage, type AgentEvent, type Capture, type ChatItem, type LayerId } from "@/shared/api";
import { toast } from "@/shared/ui";
import { openQuestion, withAnswer } from "./chat";
import { useProjectData } from "./projectData";
import { answerSession, sessionOf, upsert, useAgentSession } from "./session";
import { useToolPrefs, type Tool } from "./tools";

export type ScriptedAgent = { send: (projectId: string) => Promise<void>; answer: (option: string) => void };

export type Editing =
  | { kind: "text"; id: LayerId; pending?: boolean }
  | { kind: "path"; id: LayerId; pending?: boolean }
  | {
      kind: "crop";
      id: LayerId;
      pending?: boolean;
      edges: [number, number, number, number];
      href: string | null;
    };

export function useEditorState(projectId: string, firstMessage?: string, scripted?: ScriptedAgent) {
  const session = useAgentSession(projectId);
  const { busy, live } = session;
  const { outline, setOutline, shapeTool, setShapeTool } = useToolPrefs();
  const {
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
  } = useProjectData(projectId);
  const [sending, setSending] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [selection, setSelection] = useState<LayerId[]>([]);
  const [tool, setTool] = useState<Tool>("select");
  const [compare, setCompare] = useState(0);
  const [guides, setGuides] = useState(false);
  const [captures, setCaptures] = useState<Capture[]>([]);
  const [tab, setTab] = useState<"assistant" | "history">("assistant");
  const [note, setNote] = useState<string | null>(null);
  const [editing, setEditing] = useState<Editing | null>(null);

  const send = useCallback(
    async (text: string, ctx = { layer_ids: [] as LayerId[], captures: [] as Capture[] }) => {
      setChat((c) => [
        ...c,
        { id: `local-${Date.now()}`, role: "user", ts: new Date().toISOString(), text, context: ctx },
      ]);
      setCaptures([]);
      setSending(true);
      try {
        if (scripted) await scripted.send(projectId);
        else await api.agentSend(projectId, text, ctx);
      } catch (e) {
        toast(errorMessage(e), "error");
      } finally {
        setSending(false);
      }
    },
    [projectId, scripted, setChat],
  );

  const unsentFirstMessage = useRef(firstMessage);
  useEffect(() => {
    let alive = true;
    api
      .openProject(projectId)
      .then((o) => {
        if (!alive) return;
        take(o, true);
        const msg = unsentFirstMessage.current;
        unsentFirstMessage.current = undefined;
        if (msg && o.chat.length === 0) void send(msg);
      })
      .catch((e) => alive && setLoadError(errorMessage(e)));
    return () => {
      alive = false;
    };
  }, [projectId, take, send]);

  useEffect(() => {
    const onEvent = (e: AgentEvent) => {
      if (e.project_id !== projectId) return;
      switch (e.type) {
        case "document_changed":
          return refresh();
        case "turn_end": {
          const item = sessionOf(projectId).item;
          if (item) setChat((c) => upsert(c, item));
          if (e.stopped) toast("작업을 멈췄어요");
          // The practice turns are not saved, so the chat on screen is the only copy of them.
          return refresh(!scripted);
        }
        case "usage":
          return setProject(
            (p) =>
              p && {
                ...p,
                usage_krw: e.project_krw,
                usage_text_krw: e.text_krw,
                usage_image_krw: e.image_krw,
              },
          );
        case "error":
          return toast(e.message, "error");
      }
    };
    return api.onAgentEvent(onEvent);
  }, [projectId, scripted, refresh, setChat, setProject]);

  const [prunedFor, setPrunedFor] = useState(doc);
  if (prunedFor !== doc) {
    setPrunedFor(doc);
    if (doc) {
      const ids = new Set(doc.layers.map((l) => l.id));
      setSelection((s) => (s.every((id) => ids.has(id)) ? s : s.filter((id) => ids.has(id))));
    }
  }

  const [editFor, setEditFor] = useState(selection);
  if (editFor !== selection) {
    setEditFor(selection);
    if (editing?.kind === "crop" && !editing.pending && !selection.includes(editing.id)) setEditing(null);
  }

  const [noteFor, setNoteFor] = useState(selection);
  if (noteFor !== selection) {
    setNoteFor(selection);
    setNote(null);
  }

  const shownChat = useMemo(() => {
    const item = session.item;
    return item && (session.busy || !chat.some((c) => c.id === item.id)) ? upsert(chat, item) : chat;
  }, [chat, session.item, session.busy]);
  const lastItem = shownChat.at(-1);
  const pendingQuestion = busy && lastItem && openQuestion(lastItem) >= 0 ? lastItem : null;
  const currentTurn = busy ? [...shownChat].reverse().find((c) => c.role === "agent") : undefined;

  const openTab = useCallback(
    (next: "assistant" | "history") => {
      setTab(next);
      if (next === "history") refresh();
    },
    [refresh],
  );

  return {
    projectId,
    project,
    doc,
    chat: shownChat,
    live,
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
    outline,
    setOutline,
    shapeTool,
    setShapeTool,
    editing,
    setEditing,
    captures,
    setCaptures,
    tab,
    setTab: openTab,
    note,
    setNote,
    setProject,
    pendingQuestion,
    currentTurn,
    locked: busy && !pendingQuestion,
    composerLock: pendingQuestion ? ("question" as const) : busy || sending ? ("busy" as const) : null,
    ops,
    applyResult,
    send,
    undo: () => runHistory(api.undo),
    redo: () => runHistory(api.redo),
    restore: (seq: number) => runHistory((id) => api.restoreVersion(id, seq)).then(() => refresh()),
    stop: () => void api.agentStop(projectId).catch((e) => toast(errorMessage(e), "error")),
    answer: (item: ChatItem, option: string, question = item.question) => {
      if (!question) return;
      setChat((c) => c.map((x) => (x.id === item.id ? withAnswer(x, option, question.id) : x)));
      answerSession(projectId, item.id, option, question.id);
      if (scripted) scripted.answer(option);
      else api.agentAnswer(projectId, question.id, option).catch((e) => toast(errorMessage(e), "error"));
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
