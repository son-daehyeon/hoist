import { liveReducer, summarize, type TimelineEntry } from "./live.ts";

const same = (actual: unknown, expected: unknown) => {
  if (JSON.stringify(actual) !== JSON.stringify(expected))
    throw new Error(`${JSON.stringify(actual)} != ${JSON.stringify(expected)}`);
};

const act = (id: string, label: string, status: "done" | "failed" | "stopped", round: number) => ({
  id,
  label,
  status,
  round,
});
const timeline: TimelineEntry[] = [
  { kind: "thinking", round: 1, started: 0, seconds: 30 },
  {
    kind: "actions",
    round: 1,
    items: [act("a", "원본 보기", "done", 1), act("b", "레이어 추가 · 제목", "done", 1)],
  },
  { kind: "thinking", round: 2, started: 0, seconds: 45 },
  {
    kind: "actions",
    round: 2,
    items: [
      act("c", "레이어 고치기 · L3", "done", 2),
      act("d", "그림 만들기 · 꽃", "failed", 2),
      act("e", "누끼 따기 · 꽃", "stopped", 2),
      act("f", "레이어 보기 · L3", "done", 2),
    ],
  },
  { kind: "thinking", round: 3, started: 1000, seconds: null },
  { kind: "text", text: "끝" },
];
same(summarize(timeline), {
  count: 6,
  kinds: [
    ["레이어", 2],
    ["그림", 1],
    ["누끼", 1],
  ],
  failed: 1,
  stopped: 1,
  thinkingSec: 75,
  thinkingSince: 1000,
  round: 3,
});
same(summarize([{ kind: "text", text: "안녕" }]), null);

const turns = liveReducer(
  {},
  {
    project_id: "p",
    turn: "t",
    type: "usage",
    turn_krw: 120,
    project_krw: 900,
    text_krw: 800,
    image_krw: 100,
  },
  0,
);
same(turns.t.krw, 120);
const after = liveReducer(turns, { project_id: "p", turn: "t", type: "thinking_start", round: 1 }, 5);
same([after.t.krw, after.t.entries.length], [120, 1]);

console.log("live: ok");
