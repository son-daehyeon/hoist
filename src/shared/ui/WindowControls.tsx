import { Icon } from "./Icon";
import "./WindowControls.css";

export function WindowControls({
  onMinimize,
  onMaximize,
  onClose,
}: {
  onMinimize: () => void;
  onMaximize: () => void;
  onClose: () => void;
}) {
  return (
    <div className="bs-winctl">
      <button type="button" aria-label="최소화" onClick={onMinimize}>
        <Icon name="minimize" size={10} />
      </button>
      <button type="button" aria-label="최대화" onClick={onMaximize}>
        <Icon name="maximize" size={10} />
      </button>
      <button type="button" aria-label="닫기" className="is-close" onClick={onClose}>
        <Icon name="close" size={10} />
      </button>
    </div>
  );
}
