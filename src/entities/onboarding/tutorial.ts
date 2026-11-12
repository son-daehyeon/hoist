import { api } from "@/shared/api";

const ACTIVE_KEY = "hoist.tutorial.active";
const PROJECT_KEY = "hoist.tutorial.project";
const DONE_KEY = "hoist.onboarded";
const STEP_KEY = "hoist.tutorial.step";

/** The tour's step, kept so a remounted tour picks up where it was instead of losing its target. */
export const tutorialStep = () => Number(localStorage.getItem(STEP_KEY) ?? 0);
export const setTutorialStep = (i: number) => localStorage.setItem(STEP_KEY, String(i));

export const TUTORIAL_NAME = "연습용 현수막";

type TutorialPhase = "idle" | "reading" | "asking" | "building" | "built" | "editing" | "edited";

/** What the tour waits on; written by the scripted turns and by the editor (selection). */
export const tutorial = { phase: "idle" as TutorialPhase, selection: [] as string[] };

export const onboarded = () => localStorage.getItem(DONE_KEY) === "1";
export const tutorialActive = () => localStorage.getItem(ACTIVE_KEY) === "1";
export const tutorialProject = () => (tutorialActive() ? localStorage.getItem(PROJECT_KEY) : null);
export function startTutorial() {
  localStorage.setItem(ACTIVE_KEY, "1");
  tutorial.phase = "idle";
}
export function setTutorialProject(id: string) {
  localStorage.setItem(PROJECT_KEY, id);
}

function clearProgress() {
  localStorage.removeItem(ACTIVE_KEY);
  localStorage.removeItem(PROJECT_KEY);
  localStorage.removeItem(STEP_KEY);
}

export function discardUnfinishedTutorial() {
  if (onboarded()) return;
  const unfinished = localStorage.getItem(PROJECT_KEY);
  if (unfinished) void api.deleteProject(unfinished).catch(() => undefined);
  clearProgress();
  tutorial.phase = "idle";
  tutorial.selection = [];
}
export function finishOnboarding() {
  localStorage.setItem(DONE_KEY, "1");
  clearProgress();
}
