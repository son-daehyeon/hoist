import { useEffect, useEffectEvent, useLayoutEffect, useRef, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { IconButton } from "./Button";
import "./Dialog.css";

function fadeOutCopy(el: HTMLElement) {
  queueMicrotask(() => {
    if (el.isConnected || matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const copy = el.cloneNode(true) as HTMLElement;
    copy.classList.add("is-leaving");
    copy.inert = true;
    copy.addEventListener("animationend", (e) => e.target === copy && copy.remove());
    document.body.append(copy);
  });
}

export function Dialog({
  title,
  width = 480,
  onClose,
  tabs,
  footer,
  children,
}: {
  title: string;
  width?: 480 | 560 | 640 | 720;
  onClose?: () => void;
  tabs?: ReactNode;
  footer?: ReactNode;
  children: ReactNode;
}) {
  const root = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const el = root.current;
    return () => {
      if (el) fadeOutCopy(el);
    };
  }, []);

  const onKey = useEffectEvent((e: KeyboardEvent) => {
    if (e.key !== "Escape" || !onClose) return;
    e.stopPropagation();
    onClose();
  });
  useEffect(() => {
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return createPortal(
    <div ref={root} className="bs-dialog-backdrop">
      <div className="bs-dialog" role="dialog" aria-label={title} style={{ width }}>
        <div className="bs-dialog-title">
          <span className="t-title-m">{title}</span>
          {onClose && <IconButton icon="close" label="닫기" onClick={onClose} />}
        </div>
        {tabs && <div className="bs-dialog-tabs">{tabs}</div>}
        <div className="bs-dialog-body">{children}</div>
        {footer && <div className="bs-dialog-footer">{footer}</div>}
      </div>
    </div>,
    document.body,
  );
}
