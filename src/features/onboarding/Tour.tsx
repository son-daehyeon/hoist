import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { setTutorialStep, tutorialStep } from "@/entities/onboarding";
import { Button } from "@/shared/ui";
import { exists, STEPS } from "./steps";
import "./onboarding.css";

type Rect = { x: number; y: number; w: number; h: number };
const PAD = 6;
const GAP = 14;
const BUBBLE_SPACE_W = 340;
const BUBBLE_SPACE_H = 220;
const BUBBLE_SPACE_H_BESIDE = 240;
const NARROW_RATIO = 0.45;

/** Where the bubble goes: beside a narrow target, above or below a wide one, always inside the window. */
function bubbleAt(r: Rect) {
  const W = window.innerWidth;
  const H = window.innerHeight;
  if (r.w < W * NARROW_RATIO) {
    const right = r.x + r.w + GAP;
    const x = right + BUBBLE_SPACE_W < W ? right : Math.max(GAP, r.x - GAP - BUBBLE_SPACE_W);
    return { left: x, top: Math.min(Math.max(GAP, r.y), H - BUBBLE_SPACE_H_BESIDE) };
  }
  const below = r.y + r.h + GAP;
  const top =
    below + BUBBLE_SPACE_H < H
      ? below
      : r.y - GAP - BUBBLE_SPACE_H > 0
        ? r.y - GAP - BUBBLE_SPACE_H
        : Math.max(GAP, r.y + GAP);
  return {
    left: Math.min(Math.max(GAP, r.x + r.w / 2 - BUBBLE_SPACE_W / 2), W - BUBBLE_SPACE_W - GAP),
    top,
  };
}

/** One target at a time is lit and usable; everything else is dimmed and takes no clicks. */
export function Tour({ onDone }: { onDone: () => void }) {
  const [index, setIndex] = useState(() => Math.min(tutorialStep(), STEPS.length - 1));
  const [rect, setRect] = useState<Rect | null>(null);
  const indexRef = useRef(0);

  useLayoutEffect(() => {
    indexRef.current = index;
    setTutorialStep(index);
  }, [index]);

  useLayoutEffect(() => {
    let raf = 0;
    let last = "";
    const tick = () => {
      const step = STEPS[indexRef.current];
      if (!step) return;
      const opener = step.need && !exists(step.need.selector) ? step.need.opener : null;
      if (opener) setIndex(STEPS.findIndex((s) => s.target === opener));
      else if (step.done?.()) setIndex((i) => (i === indexRef.current ? i + 1 : i));
      const el = document.querySelector(step.target);
      const r = el?.getBoundingClientRect();
      const next = r
        ? { x: r.left - PAD, y: r.top - PAD, w: r.width + PAD * 2, h: r.height + PAD * 2 }
        : null;
      const key = JSON.stringify(next);
      if (key !== last) {
        last = key;
        setRect(next);
      }
      raf = requestAnimationFrame(tick);
    };
    tick();
    return () => cancelAnimationFrame(raf);
  }, []);

  // Clicks are blocked by the overlay; keys would still reach the app's shortcuts (close tab, undo, delete).
  useEffect(() => {
    const block = (e: KeyboardEvent) => {
      const target = STEPS[indexRef.current]?.target;
      const lit = target ? document.querySelector(target) : null;
      const allowed =
        e.target instanceof Element && (lit?.contains(e.target) || e.target.closest(".ob-tour"));
      const tabKey = e.ctrlKey && (e.key === "Tab" || e.key.toLowerCase() === "w");
      if (allowed && !tabKey) return;
      e.stopPropagation();
      e.preventDefault();
    };
    window.addEventListener("keydown", block, true);
    return () => window.removeEventListener("keydown", block, true);
  }, []);

  useEffect(() => {
    const { target, fill } = STEPS[index] ?? {};
    const box = fill ? document.querySelector<HTMLTextAreaElement>(`${target} textarea`) : null;
    if (!box || !fill) return;
    // React tracks the value it set; only the native setter makes the input event count as a change.
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set?.call(box, fill);
    box.dispatchEvent(new Event("input", { bubbles: true }));
    box.focus();
  }, [index]);

  const finished = index >= STEPS.length;
  useEffect(() => {
    if (finished) onDone();
  }, [finished, onDone]);

  const step = STEPS[index];
  if (!step) return null;
  const W = window.innerWidth;
  const H = window.innerHeight;
  // Until the target is on screen (a page or dialog opening) everything is blocked and nothing is lit.
  const r = rect ?? { x: W / 2, y: H / 2, w: 0, h: 0 };
  const blocker = (style: CSSProperties) => <div className="ob-block" style={style} />;
  return (
    <div className="ob-tour" role="dialog" aria-label="사용법 따라 하기">
      {blocker({ left: 0, top: 0, width: W, height: Math.max(0, r.y) })}
      {blocker({ left: 0, top: r.y + r.h, width: W, height: Math.max(0, H - r.y - r.h) })}
      {blocker({ left: 0, top: r.y, width: Math.max(0, r.x), height: r.h })}
      {blocker({ left: r.x + r.w, top: r.y, width: Math.max(0, W - r.x - r.w), height: r.h })}
      {rect && <div className="ob-hole" style={{ left: r.x, top: r.y, width: r.w, height: r.h }} />}
      {rect && (
        <div className="ob-bubble" style={bubbleAt(r)}>
          <div className="t-micro c-tertiary">
            {index + 1} / {STEPS.length}
          </div>
          <div className="t-title-m">{step.title}</div>
          <div className="t-body-m c-secondary">{step.body}</div>
          {!step.done && (
            <div className="ob-bubble-actions">
              <Button kind="primary" onClick={() => setIndex((i) => i + 1)}>
                다음
              </Button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
