import { openQuestion, withAnswer } from "./chat.ts";

const same = (actual: unknown, expected: unknown) => {
  if (JSON.stringify(actual) !== JSON.stringify(expected))
    throw new Error(`${JSON.stringify(actual)} != ${JSON.stringify(expected)}`);
};

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

console.log("chat: ok");
