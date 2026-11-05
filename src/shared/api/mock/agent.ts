import { AUTO_ANSWER } from "../answers";
import { emitAgentEvent } from "../events";
import type { ActionItem, AgentEventKind, ChatItem, Layer, Project, Question } from "../types";
import { renderComposite } from "./raster";
import { snapshot, uid, type MockProject } from "./store";

const stopped = new Set<string>();
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const totals = (p: Project) => ({
  project_krw: p.usage_krw,
  text_krw: p.usage_text_krw,
  image_krw: p.usage_image_krw,
});

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
    a.status = stopped.has(pid) ? "stopped" : "done";
    emit({ type: "action", turn, action: { ...a } });
  }

  const wasStopped = stopped.has(pid);
  if (!wasStopped && script.report && p.original) {
    item.report = {
      original_png: renderComposite(p.original, 292),
      work_png: renderComposite(p.doc, 292),
      overlap: 0.987,
      similarity: 0.95,
      min_dpi: 120,
      background_min_dpi: 61,
      dpi_ok: true,
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
  p.project.usage_krw += 120;
  p.project.usage_image_krw += 120;
  emit({ type: "usage", turn, turn_krw: 120, ...totals(p.project) });
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
  text: answer === AUTO_ANSWER ? "제가 알아서 정할게요." : `"${answer}"(으)로 할게요.`,
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

  let turnKrw = 0;
  const think = async (round: number, ms: number) => {
    if (stopped.has(pid)) return;
    emit({ type: "thinking_start", turn, round });
    await sleep(ms);
    turnKrw += 38;
    p.project.usage_krw += 38;
    p.project.usage_text_krw += 38;
    emit({ type: "usage", turn, turn_krw: turnKrw, ...totals(p.project) });
  };
  const act = async (
    round: number,
    label: string,
    detail: string,
    opts: { fail?: boolean; mutate?: () => void } = {},
  ) => {
    if (stopped.has(pid)) return;
    const a: ActionItem = { id: uid(), label, status: "running", detail, round };
    item.actions?.push(a);
    emit({ type: "action", turn, action: { ...a } });
    await sleep(500);
    if (opts.mutate) {
      opts.mutate();
      p.doc.rev += 1;
      emit({ type: "document_changed", turn, rev: p.doc.rev });
    }
    a.status = stopped.has(pid) ? "stopped" : opts.fail ? "failed" : "done";
    emit({ type: "action", turn, action: { ...a } });
  };

  await think(1, 2600);
  await act(1, "원본 보기 · 전체", "1536px");
  await act(1, "원본 보기 · 왼쪽 위", "1/4 영역, 1536px");
  if (!stopped.has(pid)) emit({ type: "compacted", turn, reason: "오래된 이미지 2장을 요약으로 바꿈" });
  await think(2, 1200);
  await act(2, "원본 분석 · 글자 영역", "글자 6곳");
  await think(3, 1800);
  for (const [target, detail] of [
    ["배경", "하늘·들판 그라데이션"],
    ["제목 글자", "제목 글자 · 52pt 굵게"],
    ["부제", "참여자 모집"],
    ["날짜", "2026년 하반기"],
    ["왼쪽 그림", "생성 그림 1024px"],
    ["오른쪽 그림", "생성 그림 1024px"],
  ]) {
    await act(3, `레이어 추가 · ${target}`, detail, { fail: target === "오른쪽 그림" });
  }
  await act(3, "레이어 추가 · 정보 띠", "", { fail: true });
  await act(3, "레이어 수정 · 제목 글자", "굵기 +100", {
    mutate: () => {
      p.doc.layers = editScript([]).mutate?.(p.doc.layers, 1) ?? p.doc.layers;
    },
  });
  await think(4, 900);

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
      min_dpi: 120,
      background_min_dpi: 61,
      dpi_ok: true,
      promoted: 0,
      summary: "원본과 비교했어요.",
    };
    emit({ type: "report", turn, report: item.report });
    emit({ type: "text", turn, text: item.text });
    item.question = {
      id: uid(),
      text: "원본과 현수막 비율이 달라요. 어떻게 맞출까요?",
      options: ["현수막 비율에 맞게 요소 재배치", "배경만 늘려서 채우기"],
    };
    emit({ type: "question", turn, question: item.question });
    snapshot(p, "agent", "글자 굵게", turn);
  }
  emit({ type: "turn_end", turn, stopped: wasStopped });
  stopped.delete(pid);
}
