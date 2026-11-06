import { useEffect, useState } from "react";
import { api, type Notice } from "@/shared/api";
import { Icon, Logo } from "@/shared/ui";

const TEXT: Record<Notice["kind"], string> = {
  done: "작업이 끝났어요",
  stopped: "작업이 멈췄어요",
  question: "답을 기다리고 있어요",
};
/** A question waits on the user, so its notice stays longer than "done". */
const STAY_MS: Record<Notice["kind"], number> = { done: 8000, stopped: 8000, question: 20000 };

/** Says, in the corner of the screen, that a turn ended or needs an answer. A click opens that project. */
export function NoticeCard() {
  const [notice, setNotice] = useState<Notice | null>(null);
  useEffect(() => {
    let timer = 0;
    const off = api.onNotice((next) => {
      setNotice(next);
      window.clearTimeout(timer);
      timer = window.setTimeout(() => void api.popupHide(), STAY_MS[next.kind]);
    });
    return () => {
      window.clearTimeout(timer);
      off();
    };
  }, []);
  if (!notice) return null;
  return (
    <div
      className={`pop-notice is-${notice.kind}`}
      role="status"
      onClick={() => void api.popupOpen(notice.project_id)}
    >
      <Logo size={32} tile />
      <div className="pop-notice-text">
        <div className="t-label-m bs-ellipsis">{notice.project_name}</div>
        <div className="t-caption c-secondary">{TEXT[notice.kind]}</div>
      </div>
      <button
        type="button"
        className="pop-notice-close"
        aria-label="닫기"
        onClick={(e) => {
          e.stopPropagation();
          void api.popupHide();
        }}
      >
        <Icon name="close" size={14} />
      </button>
    </div>
  );
}
