import type { IconName } from "./icons";

export type ToastTone = "default" | "error";
export type ToastAction = { label: string; kind?: "secondary" | "ghost"; onClick: () => void };
type ToastOptions = { key?: string; sticky?: boolean; icon?: IconName; actions?: ToastAction[] };
type ToastState = {
  id: number;
  key?: string;
  message: string;
  tone: ToastTone;
  icon?: IconName;
  actions?: ToastAction[];
} | null;

let current: ToastState = null;
let anchor: HTMLElement | null = null;
let timer = 0;
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());

export const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};
export const getToast = () => current;
export const getToastAnchor = () => anchor;

export function dismissToast() {
  window.clearTimeout(timer);
  current = null;
  emit();
}

export function toast(
  message: string,
  tone: ToastTone = "default",
  { key, sticky = false, icon, actions }: ToastOptions = {},
) {
  const id = key && current?.key === key ? current.id : Date.now();
  current = { id, key, message, tone, icon, actions };
  window.clearTimeout(timer);
  if (!sticky) timer = window.setTimeout(dismissToast, 3000);
  emit();
}

export function setToastAnchor(el: HTMLElement | null) {
  anchor = el;
  emit();
}
