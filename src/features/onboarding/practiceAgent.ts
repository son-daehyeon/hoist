// Scripted, so the practice needs no API key, costs nothing, and its messages never reach the backend.
import type { ScriptedAgent } from "@/entities/editor";
import { tutorial } from "@/entities/onboarding";
import { api, emitAgentEvent, type AgentEventKind, type Question } from "@/shared/api";
import { solid } from "@/shared/lib";
import { boldFamily, PRACTICE_INFO, practiceLayers, TUTORIAL_TITLE } from "./practiceBanner";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

let resolveAnswer: ((answer: string) => void) | null = null;
function answerTutorial(answer: string) {
  resolveAnswer?.(answer);
  resolveAnswer = null;
}

function scriptedTurn(projectId: string) {
  const turn = `tutorial-${Date.now()}`;
  const emit = (e: AgentEventKind) => emitAgentEvent({ ...e, project_id: projectId, turn });
  const say = async (line: string) => {
    for (const chunk of line.match(/.{1,4}/gsu) ?? []) {
      emit({ type: "text_delta", delta: chunk });
      await sleep(40);
    }
  };
  let n = 0;
  const act = async (label: string, work?: () => Promise<void>, detail?: string) => {
    const action = {
      id: `${turn}-${++n}`,
      label,
      detail: detail ?? null,
      status: "running" as const,
      round: 1,
    };
    emit({ type: "action", action });
    await (work ? work() : sleep(500));
    emit({ type: "action", action: { ...action, status: "done" } });
  };
  return { turn, emit, say, act };
}

async function buildTurn(projectId: string) {
  const { turn, emit, say, act } = scriptedTurn(projectId);
  emit({ type: "turn_start" });
  tutorial.phase = "reading";
  emit({ type: "thinking_start", round: 1 });
  await sleep(1000);
  const opening = "원본을 읽고 글자, 띠, 배경을 따로 고칠 수 있는 레이어로 다시 만들게요.";
  await say(opening);
  await act("원본 보기", undefined, "전체");
  await act("원본 분석");
  await act("글꼴 목록");
  const question: Question = {
    id: `${turn}-q`,
    text: `아래 작은 글씨를 ‘${PRACTICE_INFO}’로 읽었어요. 맞나요?`,
    options: ["맞아요"],
    allow_free_text: true,
    image_png: null,
    auto_label: "",
  };
  tutorial.phase = "asking";
  const answer = new Promise<string>((r) => (resolveAnswer = r));
  emit({ type: "question", question });
  const reply = await answer;
  const info = reply === "맞아요" || !reply.trim() ? PRACTICE_INFO : reply.trim();
  tutorial.phase = "building";
  emit({ type: "thinking_start", round: 2 });
  await sleep(600);
  const family = await boldFamily();
  for (const { label, op } of practiceLayers(family, info)) {
    await act(label, async () => {
      const r = await api.applyOps(projectId, [op], "agent");
      emit({ type: "document_changed", rev: r.document.rev });
      await sleep(900);
    });
  }
  await sleep(1500);
  const report =
    "원본과 같게 레이어로 다시 만들었어요.\n- 배경: 하늘색 바탕\n- 그래픽: 보라 띠\n- 글자: 제목, 부제, 안내";
  emit({ type: "text_delta", delta: "\n\n" });
  await say(report);
  emit({ type: "text", text: `${opening}\n\n${report}` });
  tutorial.phase = "built";
  emit({ type: "turn_end", stopped: false });
}

async function editTurn(projectId: string) {
  const { emit, say, act } = scriptedTurn(projectId);
  emit({ type: "turn_start" });
  tutorial.phase = "editing";
  emit({ type: "thinking_start", round: 1 });
  await sleep(900);
  const opening = "제목 글자 색을 빨간색으로 바꿀게요.";
  await say(opening);
  await act(
    "레이어 고치기 · 제목",
    async () => {
      const r = await api.applyOps(
        projectId,
        [{ op: "update_layer", id: TUTORIAL_TITLE, patch: { fills: [solid("#D93025")] } }],
        "agent",
      );
      emit({ type: "document_changed", rev: r.document.rev });
      await sleep(2500);
    },
    "색",
  );
  const report = "제목을 **빨간색**으로 바꿨어요.";
  emit({ type: "text_delta", delta: "\n\n" });
  await say(report);
  emit({ type: "text", text: `${opening}\n\n${report}` });
  tutorial.phase = "edited";
  emit({ type: "turn_end", stopped: false });
}

async function runTutorialTurn(projectId: string) {
  await (tutorial.phase === "idle" || tutorial.phase === "reading"
    ? buildTurn(projectId)
    : editTurn(projectId));
}

export const practiceAgent: ScriptedAgent = { send: runTutorialTurn, answer: answerTutorial };
