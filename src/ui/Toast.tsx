import { useSyncExternalStore } from "react";
import { createPortal } from "react-dom";
import { Icon } from "./Icon";
import "./Toast.css";

type ToastState = { id: number; message: string; tone: "default" | "error" } | null;

let current: ToastState = null;
let anchor: HTMLElement | null = null;
let timer = 0;
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());
const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};

export function toast(message: string, tone: "default" | "error" = "default") {
  current = { id: Date.now(), message, tone };
  window.clearTimeout(timer);
  timer = window.setTimeout(() => {
    current = null;
    emit();
  }, 3000);
  emit();
}

export function setToastAnchor(el: HTMLElement | null) {
  anchor = el;
  emit();
}

export function Toast({ message, tone = "default" }: { message: string; tone?: "default" | "error" }) {
  return (
    <div className={`bs-toast t-label-m ${tone === "error" ? "is-error" : ""}`} role="status">
      <Icon name={tone === "error" ? "alert" : "check"} size={16} />
      {message}
    </div>
  );
}

export function ToastHost() {
  const t = useSyncExternalStore(subscribe, () => current);
  const target = useSyncExternalStore(subscribe, () => anchor);
  if (!t) return null;
  return createPortal(
    <div className={`bs-toast-host ${target ? "is-anchored" : ""}`}>
      <Toast key={t.id} message={t.message} tone={t.tone} />
    </div>,
    target ?? document.body,
  );
}
