import { useEffect, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { IconButton } from "./Button";
import "./Dialog.css";

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
  onClose: () => void;
  tabs?: ReactNode;
  footer?: ReactNode;
  children: ReactNode;
}) {
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [onClose]);

  return createPortal(
    <div className="bs-dialog-backdrop">
      <div className="bs-dialog" role="dialog" aria-label={title} style={{ width }}>
        <div className="bs-dialog-title">
          <span className="t-title-m">{title}</span>
          <IconButton icon="close" label="닫기" onClick={onClose} />
        </div>
        {tabs && <div className="bs-dialog-tabs">{tabs}</div>}
        <div className="bs-dialog-body">{children}</div>
        {footer && <div className="bs-dialog-footer">{footer}</div>}
      </div>
    </div>,
    document.body,
  );
}
