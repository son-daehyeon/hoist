import { useSyncExternalStore } from "react";
import { createPortal } from "react-dom";
import { Button } from "./Button";
import { Icon } from "./Icon";
import type { IconName } from "./icons";
import {
  dismissToast,
  getToast,
  getToastAnchor,
  subscribe,
  type ToastAction,
  type ToastTone,
} from "./toastStore";
import "./Toast.css";

export function Toast({
  message,
  tone = "default",
  icon,
  actions = [],
}: {
  message: string;
  tone?: ToastTone;
  icon?: IconName;
  actions?: ToastAction[];
}) {
  return (
    <div
      className={`bs-toast t-label-m ${tone === "error" ? "is-error" : ""} ${actions.length ? "has-actions" : ""}`}
      role="status"
    >
      <Icon name={icon ?? (tone === "error" ? "alert" : "check")} size={16} />
      {message}
      {actions.map((a) => (
        <Button key={a.label} kind={a.kind ?? "secondary"} onClick={a.onClick}>
          {a.label}
        </Button>
      ))}
    </div>
  );
}

export function ToastHost() {
  const t = useSyncExternalStore(subscribe, getToast);
  const target = useSyncExternalStore(subscribe, getToastAnchor);
  if (!t) return null;
  const actions = t.actions?.map((a) => ({
    ...a,
    onClick: () => {
      dismissToast();
      a.onClick();
    },
  }));
  return createPortal(
    <div className={`bs-toast-host ${target ? "is-anchored" : ""}`}>
      <Toast key={t.id} message={t.message} tone={t.tone} icon={t.icon} actions={actions} />
    </div>,
    target ?? document.body,
  );
}
