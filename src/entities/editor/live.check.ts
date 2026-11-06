import { liveReducer, openQuestion, summarize, withAnswer, type TimelineEntry } from "./live.ts";

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
      act("g", "줄 맞추기 · 제목", "done", 2),
    ],
  },
  { kind: "thinking", round: 3, started: 1000, seconds: null },
  { kind: "text", text: "끝" },
];
same(summarize(timeline), {
  count: 7,
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

const q = (id: string) => ({ id, text: id, options: ["a", "b"] });
const asked = {
  id: "t",
  role: "agent" as const,
  ts: "",
  text: "",
  question: q("q1"),
  questions: [q("q1"), q("q2")],
};
same(openQuestion(asked), 0);
const first = withAnswer(asked, "a");
same([first.answers, first.answer, openQuestion(first)], [["a"], "a", 1]);
const second = withAnswer(first, "b");
same([second.answers, second.answer, openQuestion(second)], [["a", "b"], "a", -1]);
same(withAnswer({ ...asked, questions: undefined }, "a").answer, "a");
// a second ask in the same turn: the first answer is in, the new questions are still open
const later = { ...asked, questions: [q("q1"), q("q2"), q("q3")], answer: "a", answers: ["a", null, null] };
same(openQuestion(later), 1);
const third = withAnswer(later, "c", "q3");
same([third.answers, openQuestion(third)], [["a", null, "c"], 1]);
same(openQuestion(withAnswer(third, "b", "q2")), -1);
same(openQuestion({ ...asked, questions: undefined, answer: "a" }), -1);

console.log("live: ok");
