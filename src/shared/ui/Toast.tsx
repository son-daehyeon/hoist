import { useSyncExternalStore } from "react";
import { createPortal } from "react-dom";
import { Icon } from "./Icon";
import { getToast, getToastAnchor, subscribe } from "./toastStore";
import "./Toast.css";

export function Toast({ message, tone = "default" }: { message: string; tone?: "default" | "error" }) {
  return (
    <div className={`bs-toast t-label-m ${tone === "error" ? "is-error" : ""}`} role="status">
      <Icon name={tone === "error" ? "alert" : "check"} size={16} />
      {message}
    </div>
  );
}

export function ToastHost() {
  const t = useSyncExternalStore(subscribe, getToast);
  const target = useSyncExternalStore(subscribe, getToastAnchor);
  if (!t) return null;
  return createPortal(
    <div className={`bs-toast-host ${target ? "is-anchored" : ""}`}>
      <Toast key={t.id} message={t.message} tone={t.tone} />
    </div>,
    target ?? document.body,
  );
}
