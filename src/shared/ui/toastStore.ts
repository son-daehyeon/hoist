type ToastState = { id: number; message: string; tone: "default" | "error" } | null;

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
