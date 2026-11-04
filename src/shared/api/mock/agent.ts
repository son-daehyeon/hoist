import { emitAgentEvent } from "../events";
import type { ActionItem, AgentEventKind, ChatItem, Layer, Question } from "../types";
import { renderComposite } from "./raster";
import { snapshot, uid, type MockProject } from "./store";

const stopped = new Set<string>();
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export interface Script {
  text: string;
  actions: string[];
  mutate?: (layers: Layer[], step: number) => Layer[];
  report?: boolean;
  question?: Omit<Question, "id">;
  summary: string;
}

export function stopTurn(projectId: string) {
  stopped.add(projectId);
}

export async function runTurn(p: MockProject, script: Script, turn = uid()) {
  const pid = p.project.id;
  const emit = (e: AgentEventKind & { turn: string }) => emitAgentEvent({ ...e, project_id: pid });
  stopped.delete(pid);

  const item: ChatItem = { id: turn, role: "agent", ts: new Date().toISOString(), text: "", actions: [] };
  p.chat.push(item);
  emit({ type: "turn_start", turn });

  for (const chunk of script.text.match(/.{1,6}/gsu) ?? []) {
    await sleep(70);
    item.text += chunk;
    emit({ type: "text_delta", turn, delta: chunk });
  }

  const actions: ActionItem[] = [];
  item.actions = actions;

  for (const [i, label] of script.actions.entries()) {
    if (stopped.has(pid)) break;
    const a: ActionItem = { id: uid(), label, status: "running" };
    actions.push(a);
    emit({ type: "action", turn, action: { ...a } });
    await sleep(500);
    if (script.mutate) {
      p.doc.layers = script.mutate(p.doc.layers, i);
      p.doc.rev += 1;
      emit({ type: "document_changed", turn, rev: p.doc.rev });
    }
    a.status = "done";
    emit({ type: "action", turn, action: { ...a } });
  }

  const wasStopped = stopped.has(pid);
  if (!wasStopped && script.report && p.original) {
    item.report = {
      original_png: renderComposite(p.original, 292),
      work_png: renderComposite(p.doc, 292),
      overlap: 0.987,
      similarity: 0.95,
      min_dpi: 100,
      promoted: 1,
      summary: "원본과 비교했어요.",
    };
    emit({ type: "report", turn, report: item.report });
  }
  if (!wasStopped && script.question) {
    item.question = { id: uid(), ...script.question };
    emit({ type: "question", turn, question: item.question });
  }
  if (script.mutate || script.report) snapshot(p, "agent", script.summary, turn);
  emit({ type: "turn_end", turn, stopped: wasStopped });
  stopped.delete(pid);
}

export function buildScript(p: MockProject): Script {
  const target = p.original?.layers ?? [];
  const groups = target.filter((l) => !l.parent);
  return {
    text: "원본을 분석하고 있어요. 글자·도형·그림·배경으로 나눠 다시 만든 뒤 원본과 비교해서 알려드릴게요.",
    actions: [
      "원본 분석",
      "레이어 처리 방식 정함",
      ...groups.map((g) => `${g.name} 만드는 중`),
      "원본과 비교",
    ],
    mutate: (_layers, step) => {
      const keep = new Set(groups.slice(0, Math.max(0, step - 1)).map((g) => g.id));
      return structuredClone(target.filter((l) => keep.has(l.parent ?? l.id)));
    },
    report: true,
    question: { text: "이대로 둘까요, 더 손볼까요?", options: ["이대로 좋아요", "더 다듬어줘"] },
    summary: "첫 재구성",
  };
}

export function editScript(ids: string[]): Script {
  return {
    text: "알겠어요. 고른 부분을 고쳐서 원본과 다시 비교할게요.",
    actions: ["고칠 부분 확인", "레이어 수정", "원본과 비교"],
    mutate: (layers, step) =>
      step !== 1
        ? layers
        : layers.map((l) =>
            (ids.length === 0 || ids.includes(l.id)) && l.kind.type === "text"
              ? { ...l, kind: { ...l.kind, font_weight: Math.min(900, l.kind.font_weight + 100) } }
              : l,
          ),
    report: true,
    question: { text: "이대로 둘까요, 더 손볼까요?", options: ["이대로 좋아요", "더 다듬어줘"] },
    summary: "글자 굵게",
  };
}

export const answerScript = (answer: string): Script => ({
  text: `"${answer}"(으)로 할게요.`,
  actions: ["작업본 확인"],
  summary: answer,
});

export const replaceScript: Script = {
  text: "새 원본을 받았어요. v1과 비교해 봤어요.",
  actions: ["v1과 v2 비교"],
  question: {
    text: "원본이 v2로 바뀌었어요. 날짜와 장소 문구가 달라졌어요. 작업본도 바꿀까요?",
    options: ["바뀐 부분만 반영", "전부 새로 만들기", "참고 이미지만 바꾸기"],
  },
  summary: "원본 v2 비교",
};

export async function runDemoTurn(p: MockProject, turn: string) {
  const pid = p.project.id;
  const emit = (e: AgentEventKind & { turn: string }) => emitAgentEvent({ ...e, project_id: pid });
  stopped.delete(pid);
  const item: ChatItem = { id: turn, role: "agent", ts: new Date().toISOString(), text: "", actions: [] };
  p.chat.push(item);
  emit({ type: "turn_start", turn });

  const think = async (round: number, text: string) => {
    if (stopped.has(pid)) return;
    emit({ type: "thinking_start", turn, round });
    for (const delta of text.match(/.{1,5}/gsu) ?? []) {
      await sleep(60);
      emit({ type: "thinking_delta", turn, delta });
    }
  };
  const act = async (label: string, detail: string, mutate?: () => void) => {
    if (stopped.has(pid)) return;
    const a: ActionItem = { id: uid(), label, status: "running", detail };
    item.actions?.push(a);
    emit({ type: "action", turn, action: { ...a } });
    await sleep(900);
    if (mutate) {
      mutate();
      p.doc.rev += 1;
      emit({ type: "document_changed", turn, rev: p.doc.rev });
    }
    a.status = "done";
    emit({ type: "action", turn, action: { ...a } });
  };

  await think(
    1,
    "원본 전체를 먼저 보고, 글자가 몰린 왼쪽 위를 확대해서 한 번 더 볼게요. 그다음 굵기를 맞춰요.",
  );
  await act("원본 보기", "전체, 1536px");
  await act("원본 보기", "왼쪽 위 1/4 영역, 1536px");
  if (!stopped.has(pid)) emit({ type: "compacted", turn, reason: "오래된 이미지 2장을 요약으로 바꿈" });
  await think(2, "제목 글자가 원본보다 얇아요. 굵기를 한 단계 올리면 맞을 것 같아요.");
  await act("레이어 수정", "글자 굵기 +100", () => {
    p.doc.layers = editScript([]).mutate?.(p.doc.layers, 1) ?? p.doc.layers;
  });

  const wasStopped = stopped.has(pid);
  if (!wasStopped) {
    for (const delta of "제목 글자 굵기를 원본에 맞췄어요. 원본과 비교한 결과예요.".match(/.{1,6}/gsu) ??
      []) {
      await sleep(70);
      item.text += delta;
      emit({ type: "text_delta", turn, delta });
    }
    item.report = {
      original_png: renderComposite(p.original ?? p.doc, 292),
      work_png: renderComposite(p.doc, 292),
      overlap: 0.987,
      similarity: 0.95,
      min_dpi: 100,
      promoted: 0,
      summary: "원본과 비교했어요.",
    };
    emit({ type: "report", turn, report: item.report });
    snapshot(p, "agent", "글자 굵게", turn);
  }
  emit({ type: "turn_end", turn, stopped: wasStopped });
  stopped.delete(pid);
}
