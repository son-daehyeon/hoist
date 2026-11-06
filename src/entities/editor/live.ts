import type { ActionItem, AgentEvent, ChatItem } from "@/shared/api";

export type TimelineEntry =
  | { kind: "thinking"; round: number; started: number; seconds: number | null }
  | { kind: "text"; text: string }
  | { kind: "actions"; round: number | null; items: ActionItem[] }
  | { kind: "compacted"; reason: string };

export type LiveTurn = { entries: TimelineEntry[]; typing: boolean; krw?: number };

export type LiveTurns = Record<string, LiveTurn>;

const EMPTY: LiveTurn = { entries: [], typing: false };

function lastIndex(entries: TimelineEntry[], match: (e: TimelineEntry) => boolean) {
  for (let i = entries.length - 1; i >= 0; i--) if (match(entries[i])) return i;
  return -1;
}

function settle(entries: TimelineEntry[], now: number): TimelineEntry[] {
  const i = lastIndex(entries, (e) => e.kind === "thinking" && e.seconds === null);
  if (i < 0) return entries;
  const t = entries[i] as Extract<TimelineEntry, { kind: "thinking" }>;
  const seconds = Math.max(1, Math.round((now - t.started) / 1000));
  return entries.map((e, j) => (j === i ? { ...t, seconds } : e));
}

function replaceLast(entries: TimelineEntry[], entry: TimelineEntry) {
  return [...entries.slice(0, -1), entry];
}

function withText(entries: TimelineEntry[], delta: string) {
  const last = entries.at(-1);
  return last?.kind === "text"
    ? replaceLast(entries, { kind: "text", text: last.text + delta })
    : [...entries, { kind: "text" as const, text: delta }];
}

function withAction(entries: TimelineEntry[], action: ActionItem) {
  const at = entries.findIndex((e) => e.kind === "actions" && e.items.some((a) => a.id === action.id));
  if (at >= 0) {
    return entries.map((e) =>
      e.kind === "actions" && e === entries[at]
        ? { ...e, items: e.items.map((a) => (a.id === action.id ? action : a)) }
        : e,
    );
  }
  const round = action.round ?? null;
  const last = entries.at(-1);
  if (last?.kind === "actions" && (round === null || last.round === round)) {
    return replaceLast(entries, { ...last, items: [...last.items, action] });
  }
  return [...entries, { kind: "actions" as const, round, items: [action] }];
}

export function liveReducer(turns: LiveTurns, e: AgentEvent, now: number): LiveTurns {
  const cur = turns[e.turn] ?? EMPTY;
  let next: LiveTurn;
  switch (e.type) {
    case "thinking_start":
      next = {
        typing: false,
        entries: [
          ...settle(cur.entries, now),
          { kind: "thinking", round: e.round, started: now, seconds: null },
        ],
      };
      break;
    case "text_delta":
      next = { typing: true, entries: withText(settle(cur.entries, now), e.delta) };
      break;
    case "action":
      next = { typing: false, entries: withAction(settle(cur.entries, now), e.action) };
      break;
    case "compacted":
      next = {
        typing: false,
        entries: [...settle(cur.entries, now), { kind: "compacted", reason: e.reason }],
      };
      break;
    case "usage":
      return { ...turns, [e.turn]: { ...cur, krw: e.turn_krw } };
    case "text":
    case "report":
    case "question":
    case "turn_end":
    case "error":
      next = { typing: false, entries: settle(cur.entries, now) };
      break;
    default:
      return turns;
  }
  if (next.typing === cur.typing && next.entries === cur.entries) return turns;
  return { ...turns, [e.turn]: { ...next, krw: cur.krw } };
}

function groupActions(actions: ActionItem[]): TimelineEntry[] {
  return actions.reduce<TimelineEntry[]>((out, a) => withAction(out, a), []);
}

export function timelineOf(item: ChatItem, live?: LiveTurn): TimelineEntry[] {
  const actions = item.actions ?? [];
  let entries = live?.entries ?? groupActions(actions);
  if (live) {
    const shown = new Set(entries.flatMap((e) => (e.kind === "actions" ? e.items.map((a) => a.id) : [])));
    const missing = actions.filter((a) => !shown.has(a.id));
    if (missing.length) entries = [...groupActions(missing), ...entries];
  }
  const text = item.text || item.report?.summary || "";
  return text && !entries.some((e) => e.kind === "text") ? [...entries, { kind: "text", text }] : entries;
}

const KIND_SEP = " · ";
const VIEW = /(보기|읽기)$/;
const LAYER_EDITS = new Set(["줄 맞추기", "상자 맞추기"]);

export type TurnSummary = {
  count: number;
  kinds: [string, number][];
  failed: number;
  stopped: number;
  thinkingSec: number;
  thinkingSince: number | null;
  round: number | null;
};

export function summarize(timeline: TimelineEntry[]): TurnSummary | null {
  const actions = timeline.flatMap((e) => (e.kind === "actions" ? e.items : []));
  const thoughts = timeline.filter((e) => e.kind === "thinking");
  if (!actions.length && !thoughts.length) return null;
  const kinds = new Map<string, number>();
  for (const a of actions) {
    const kind = a.label.split(KIND_SEP)[0];
    if (VIEW.test(kind)) continue;
    const word = LAYER_EDITS.has(kind) ? "레이어" : kind.split(" ")[0];
    kinds.set(word, (kinds.get(word) ?? 0) + 1);
  }
  const rounds = timeline.flatMap((e) =>
    e.kind === "thinking" || (e.kind === "actions" && e.round !== null) ? [e.round ?? 0] : [],
  );
  return {
    count: actions.length,
    kinds: [...kinds].sort((a, b) => b[1] - a[1]).slice(0, 3),
    failed: actions.filter((a) => a.status === "failed").length,
    stopped: actions.filter((a) => a.status === "stopped").length,
    thinkingSec: thoughts.reduce((t, e) => t + (e.seconds ?? 0), 0),
    thinkingSince: thoughts.find((e) => e.seconds === null)?.started ?? null,
    round: rounds.length ? Math.max(...rounds) : null,
  };
}

const askedOf = (item: ChatItem) =>
  item.questions?.length ? item.questions : item.question ? [item.question] : [];

/**
 * Index of the first question still waiting for an answer, or -1. A turn can ask more than once, so an
 * answered first batch says nothing about a later one: every question is matched with its own answer.
 */
export function openQuestion(item: ChatItem): number {
  const given = item.answers ?? (item.answer != null ? [item.answer] : []);
  return askedOf(item).findIndex((_, i) => given[i] == null);
}

/** `answer` mirrors the stored chat, where it is the answer to the first question. */
export function withAnswer(item: ChatItem, option: string, questionId?: string): ChatItem {
  const byId = questionId ? askedOf(item).findIndex((q) => q.id === questionId) : -1;
  const at = byId >= 0 ? byId : Math.max(0, openQuestion(item));
  const answers: (string | null)[] = [...(item.answers ?? [])];
  while (answers.length < at) answers.push(null);
  answers[at] = option;
  return { ...item, answers, ...(at === 0 ? { answer: option } : {}) };
}
