import { useEffect, useState } from "react";
import { api } from "@/shared/api";
import { Icon, Logo } from "@/shared/ui";

const MAX_LINES = 2;

/** The tray icon's menu, drawn by the app: open the window, see what is being worked on, quit. */
export function TrayMenu() {
  const [busy, setBusy] = useState<string[]>([]);
  useEffect(() => {
    const load = () =>
      void api
        .popupBusyProjects()
        .then(setBusy)
        .catch(() => setBusy([]));
    load();
    return api.onPopupShown(load);
  }, []);
  return (
    <div className="pop-menu" role="menu">
      <button
        type="button"
        role="menuitem"
        className="pop-item t-label-m"
        onClick={() => void api.popupOpen(null)}
      >
        <Logo size={16} />
        Hoist 열기
      </button>
      {busy.length > 0 && (
        <>
          <div className="pop-divider" />
          <div className="pop-heading t-micro c-tertiary">작업 중</div>
          {busy.slice(0, MAX_LINES).map((name) => (
            <div key={name} className="pop-line t-caption">
              <span className="pop-dot" />
              <span className="bs-ellipsis">{name}</span>
            </div>
          ))}
          {busy.length > MAX_LINES && (
            <div className="pop-line t-caption c-tertiary">외 {busy.length - MAX_LINES}개</div>
          )}
        </>
      )}
      <div className="pop-divider" />
      <button
        type="button"
        role="menuitem"
        className="pop-item t-label-m"
        onClick={() => void api.popupQuit()}
      >
        <Icon name="close" size={16} />
        종료
      </button>
    </div>
  );
}
