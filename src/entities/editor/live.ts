import type { AgentEvent } from "@/shared/api";

export type LiveEntry =
  | { kind: "thinking"; round: number; text: string; started: number; seconds: number | null }
  | { kind: "compacted"; reason: string };

export type LiveTurn = { entries: LiveEntry[]; typing: boolean };

export type LiveTurns = Record<string, LiveTurn>;

const EMPTY: LiveTurn = { entries: [], typing: false };

function settle(entries: LiveEntry[], now: number): LiveEntry[] {
  const last = entries.at(-1);
  if (last?.kind !== "thinking" || last.seconds !== null) return entries;
  const seconds = Math.max(1, Math.round((now - last.started) / 1000));
  return [...entries.slice(0, -1), { ...last, seconds }];
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
          { kind: "thinking", round: e.round, text: "", started: now, seconds: null },
        ],
      };
      break;
    case "thinking_delta": {
      const last = cur.entries.at(-1);
      if (last?.kind !== "thinking" || last.seconds !== null) return turns;
      next = { ...cur, entries: [...cur.entries.slice(0, -1), { ...last, text: last.text + e.delta }] };
      break;
    }
    case "compacted":
      next = {
        typing: false,
        entries: [...settle(cur.entries, now), { kind: "compacted", reason: e.reason }],
      };
      break;
    case "document_changed":
      return turns;
    default:
      next = { typing: e.type === "text_delta", entries: settle(cur.entries, now) };
  }
  if (next.typing === cur.typing && next.entries === cur.entries) return turns;
  return { ...turns, [e.turn]: next };
}
