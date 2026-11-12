import type { ChatItem } from "@/shared/api";

/** What the app sends on the user's behalf when a project is made from an original. */
export const AUTO_FIRST_MESSAGE = "원본과 똑같이 레이어로 만들어줘";

/** That automatic opener is not something the user said, so the chat does not show it. */
export const isAutoFirst = (item: ChatItem, index: number) =>
  index === 0 && item.role === "user" && item.text === AUTO_FIRST_MESSAGE;

export const questionsOf = (item: ChatItem) =>
  item.questions?.length ? item.questions : item.question ? [item.question] : [];

/** A turn can ask more than once, so every question is matched with its own answer; -1 when none is open. */
export function openQuestion(item: ChatItem): number {
  const given = item.answers ?? (item.answer != null ? [item.answer] : []);
  return questionsOf(item).findIndex((_, i) => given[i] == null);
}

/** `answer` mirrors the stored chat, where it is the answer to the first question. */
export function withAnswer(item: ChatItem, option: string, questionId?: string): ChatItem {
  const byId = questionId ? questionsOf(item).findIndex((q) => q.id === questionId) : -1;
  const at = byId >= 0 ? byId : Math.max(0, openQuestion(item));
  const answers: (string | null)[] = [...(item.answers ?? [])];
  while (answers.length < at) answers.push(null);
  answers[at] = option;
  return { ...item, answers, ...(at === 0 ? { answer: option } : {}) };
}
