import { emitAgentEvent } from "../events";
import type { AgentEventKind, ChatItem, Question } from "../types";
import { renderComposite } from "./raster";
import { uid, type MockProject } from "./store";

type AskQuestion = Question & { allow_free_text: boolean; image_png?: string | null; auto_label: string };

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export async function runQuestionTurn(p: MockProject, turn: string) {
  const emit = (e: AgentEventKind & { turn: string }) => emitAgentEvent({ ...e, project_id: p.project.id });
  const item: ChatItem = { id: turn, role: "agent", ts: new Date().toISOString(), text: "" };
  p.chat.push(item);
  emit({ type: "turn_start", turn });
  await sleep(200);
  item.text = "다시 만들기 전에 세 가지만 여쭤볼게요.";
  emit({ type: "text", turn, text: item.text });

  const auto_label = "에이전트가 알아서";
  const questions: AskQuestion[] = [
    {
      id: uid(),
      text: "원본의 이 제목 글자를 어떻게 할까요?",
      options: ["원본 그대로", "조금 더 크게", "더 굵게"],
      allow_free_text: false,
      image_png: renderComposite(p.original ?? p.doc, 480),
      auto_label,
    },
    {
      id: uid(),
      text: "행사 날짜가 흐려서 잘 안 보여요. 정확한 날짜를 적어 주세요.",
      options: [],
      allow_free_text: true,
      auto_label,
    },
    {
      id: uid(),
      text: "배경 하늘색은 어떻게 할까요?",
      options: ["원본과 똑같이", "조금 더 밝게"],
      allow_free_text: true,
      auto_label,
    },
  ];
  item.question = questions[0];
  item.questions = questions;
  for (const question of questions) {
    await sleep(120);
    emit({ type: "question", turn, question });
  }
  emit({ type: "turn_end", turn, stopped: false });
}

export function recordAnswer(p: MockProject, questionId: string, answer: string) {
  const item = p.chat.find(
    (c) => c.question?.id === questionId || c.questions?.some((q) => q.id === questionId),
  );
  if (!item) return true;
  item.answers = [...(item.answers ?? []), answer];
  const done = item.answers.length >= (item.questions?.length ?? 1);
  if (done) item.answer = answer;
  return done;
}
