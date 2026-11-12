import { tutorial, tutorialActive } from "@/entities/onboarding";
import { TUTORIAL_TITLE } from "./practiceBanner";

export const exists = (selector: string) => document.querySelector(selector) !== null;
const NEW = '.bs-dialog[aria-label="새 프로젝트"]';
const QUESTION = '.bs-dialog[aria-label="에이전트 질문"]';
const EXPORT = '.bs-dialog[aria-label="내보내기"]';
const NEW_OPENER = '[data-tour="new-project"]';
const EXPORT_OPENER = '[data-tour="export"]';
const TUTORIAL_ASK = "제목을 빨간색으로 바꿔줘";

type Step = {
  target: string;
  title: string;
  body: string;
  /** Moves on by itself once this holds; without it the bubble shows a button. */
  done?: () => boolean;
  /** When this is gone (a dialog closed with Esc), the tour goes back to the step that opens it. */
  need?: { selector: string; opener: string };
  /** Typed into the target's text box when the step starts. */
  fill?: string;
};

export const STEPS: Step[] = [
  {
    target: '[data-tour="start"]',
    title: "사용법 따라 하기",
    body: "현수막 이미지를 올리고, 에이전트가 레이어로 다시 만들고, 고치고, 내보내는 과정을 한 번 따라 해 볼게요.",
    done: tutorialActive,
  },
  {
    target: NEW_OPENER,
    title: "새 프로젝트",
    body: "받은 현수막 이미지로 시작할 때 눌러요. 눌러 보세요.",
    done: () => exists(NEW),
  },
  {
    target: `${NEW} .np-left`,
    title: "현수막 이미지 올리기",
    body: "현수막 이미지를 여기에 끌어다 놓거나 ‘파일 선택’을 눌러 골라요. 연습에서는 ‘파일 선택’을 누르면 연습용 이미지가 들어가요.",
    done: () => exists(`${NEW} .preview-box`),
    need: { selector: NEW, opener: NEW_OPENER },
  },
  {
    target: `${NEW} .np-right`,
    title: "이름과 실제 크기",
    body: "이름과 실제로 인쇄할 현수막 크기를 mm로 적어요. 연습은 미리 채워 뒀어요.",
    need: { selector: NEW, opener: NEW_OPENER },
  },
  {
    target: '[data-tour="np-create"]',
    title: "만들고 분석 시작",
    body: "누르면 에이전트가 원본을 읽기 시작해요. 눌러 보세요.",
    done: () => exists('[data-tour="canvas"]'),
    need: { selector: NEW, opener: NEW_OPENER },
  },
  {
    target: '[data-tour="canvas"]',
    title: "원본과 작업본",
    body: "위가 올린 원본, 아래가 에이전트가 만드는 작업본이에요. 지금 에이전트가 원본을 읽고 있어요.",
    done: () => exists(QUESTION),
  },
  {
    target: QUESTION,
    title: "에이전트의 질문",
    body: "확실하지 않은 글자나 정해야 할 게 있으면 이렇게 물어봐요. 답해 보세요.",
    done: () => !exists(QUESTION) && tutorial.phase !== "asking",
  },
  {
    target: '[data-tour="canvas"]',
    title: "레이어로 다시 만드는 중",
    body: "배경, 띠, 글자를 하나씩 만들어요. 잠깐 기다려 주세요.",
    done: () => tutorial.phase === "built",
  },
  {
    target: '[data-tour="chat"]',
    title: "한 일 보고",
    body: "다 만들면 무엇을 했는지 여기에 정리해요.",
  },
  {
    target: '[data-tour="layers"]',
    title: "레이어 목록",
    body: "만든 레이어가 여기 있어요. ‘제목’을 눌러 보세요.",
    done: () => tutorial.selection.includes(TUTORIAL_TITLE),
  },
  {
    target: '[data-tour="props"]',
    title: "직접 고치기",
    body: "고른 레이어의 글자, 글꼴, 색, 크기, 위치를 여기서 바꿔요.",
  },
  {
    target: '[data-tour="composer"]',
    title: "말로 부탁하기",
    body: "직접 고치기 어려우면 말로 부탁해요. 보내기를 눌러 보세요.",
    fill: TUTORIAL_ASK,
    done: () => tutorial.phase === "editing" || tutorial.phase === "edited",
  },
  {
    target: '[data-tour="canvas"]',
    title: "말한 대로 고치는 중",
    body: "에이전트가 제목 색을 바꾸고 있어요. 잠깐 기다려 주세요.",
    done: () => tutorial.phase === "edited",
  },
  {
    target: 'button[aria-label="되돌리기"]',
    title: "되돌리기",
    body: "결과가 마음에 안 들면 바로 전으로 돌아가요.",
  },
  {
    target: '[data-tour="history"]',
    title: "기록",
    body: "‘기록’에서 예전 버전을 보고 되돌릴 수 있어요.",
  },
  {
    target: EXPORT_OPENER,
    title: "내보내기",
    body: "다 되면 인쇄소에 보낼 파일을 만들어요. ‘내보내기’를 눌러 보세요.",
    done: () => exists(EXPORT),
  },
  {
    target: EXPORT,
    title: "인쇄용 파일 저장",
    body: "파일 형식과 해상도를 고르고 ‘내보내기’를 눌러요.",
    done: () => !exists(EXPORT),
  },
];
