import { useSyncExternalStore } from "react";
import { api, type ActionItem, type AgentEvent, type ChatItem } from "@/shared/api";
import { liveReducer, withAnswer, type LiveTurns } from "./live";

export type AgentSession = {
  busy: boolean;
  turn: string | null;
  item: ChatItem | null;
  live: LiveTurns;
  lastLabel: string | null;
  startedAt: number | null;
  doneAt: number | null;
};

const IDLE: AgentSession = {
  busy: false,
  turn: null,
  item: null,
  live: {},
  lastLabel: null,
  startedAt: null,
  doneAt: null,
};
const ACTIVE = new Set<AgentEvent["type"]>(["turn_start", "thinking_start", "text_delta", "action"]);

let sessions: ReadonlyMap<string, AgentSession> = new Map();
const listeners = new Set<() => void>();

export function upsertAction(list: ActionItem[] = [], a: ActionItem) {
  const i = list.findIndex((x) => x.id === a.id);
  return i < 0 ? [...list, a] : list.map((x, j) => (j === i ? a : x));
}

export function withItem(chat: ChatItem[], item: ChatItem) {
  const i = chat.findIndex((x) => x.id === item.id);
  return i < 0 ? [...chat, item] : chat.map((x, j) => (j === i ? item : x));
}

function patched(item: ChatItem, e: AgentEvent): ChatItem {
  switch (e.type) {
    case "text_delta":
      return { ...item, text: item.text + e.delta };
    case "text":
      return { ...item, text: e.text };
    case "action":
      return { ...item, actions: upsertAction(item.actions, e.action) };
    case "report":
      return { ...item, report: e.report };
    case "question":
      return {
        ...item,
        question: item.question ?? e.question,
        questions: [...(item.questions ?? []).filter((x) => x.id !== e.question.id), e.question],
      };
    case "usage":
      return { ...item, turn_krw: e.turn_krw };
    default:
      return item;
  }
}

function onEvent(e: AgentEvent) {
  const s = sessions.get(e.project_id) ?? IDLE;
  const now = Date.now();
  const item =
    s.turn === e.turn && s.item
      ? s.item
      : { id: e.turn, role: "agent" as const, ts: new Date(now).toISOString(), text: "" };
  const next: AgentSession = {
    turn: e.turn,
    item: patched(item, e),
    live: liveReducer(s.live, e, now),
    busy: e.type === "turn_end" ? false : s.busy || ACTIVE.has(e.type),
    startedAt: e.type === "turn_start" ? now : s.startedAt,
    doneAt: e.type === "turn_end" ? now : e.type === "turn_start" ? null : s.doneAt,
    lastLabel: e.type === "action" ? e.action.label : e.type === "turn_start" ? null : s.lastLabel,
  };
  sessions = new Map(sessions).set(e.project_id, next);
  listeners.forEach((fn) => fn());
}

api.onAgentEvent(onEvent);

export function answerSession(projectId: string, itemId: string, option: string, questionId: string) {
  const s = sessions.get(projectId);
  if (!s?.item || s.item.id !== itemId) return;
  sessions = new Map(sessions).set(projectId, { ...s, item: withAnswer(s.item, option, questionId) });
  listeners.forEach((fn) => fn());
}

const subscribe = (fn: () => void) => {
  listeners.add(fn);
  return () => {
    listeners.delete(fn);
  };
};

export const sessionOf = (projectId: string) => sessions.get(projectId) ?? IDLE;
export const anyBusy = () => [...sessions.values()].some((s) => s.busy);
export const useAgentSession = (projectId: string) =>
  useSyncExternalStore(subscribe, () => sessionOf(projectId));
export const useAgentSessions = () => useSyncExternalStore(subscribe, () => sessions);
