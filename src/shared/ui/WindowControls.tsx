import { useEffect, useRef } from "react";
import { Icon } from "./Icon";
import "./WindowControls.css";

const HOVER_MS = 400;

export function WindowControls({
  onMinimize,
  onMaximize,
  onMaximizeHover,
  onClose,
}: {
  onMinimize: () => void;
  onMaximize: () => void;
  onMaximizeHover?: () => void;
  onClose: () => void;
}) {
  const hover = useRef(0);
  const cancel = () => window.clearTimeout(hover.current);
  useEffect(() => () => window.clearTimeout(hover.current), []);

  return (
    <div className="bs-winctl">
      <button type="button" aria-label="최소화" onClick={onMinimize}>
        <Icon name="minimize" size={10} />
      </button>
      <button
        type="button"
        aria-label="최대화"
        onMouseEnter={() => {
          cancel();
          if (onMaximizeHover) hover.current = window.setTimeout(onMaximizeHover, HOVER_MS);
        }}
        onMouseLeave={cancel}
        onMouseDown={cancel}
        onClick={onMaximize}
      >
        <Icon name="maximize" size={10} />
      </button>
      <button type="button" aria-label="닫기" className="is-close" onClick={onClose}>
        <Icon name="close" size={10} />
      </button>
    </div>
  );
}
