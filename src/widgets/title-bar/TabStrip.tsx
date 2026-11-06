import { Icon } from "@/shared/ui";

export type ProjectTab = { id: string; name: string };

/**
 * The open projects, side by side in the title bar like browser tabs, with the home screen first. A project
 * stays open (and its agent keeps working) while another tab is shown.
 */
export function TabStrip({
  tabs,
  active,
  busy,
  onSelect,
  onClose,
}: {
  tabs: ProjectTab[];
  active: string | null;
  busy: (id: string) => boolean;
  onSelect: (id: string | null) => void;
  onClose: (id: string) => void;
}) {
  return (
    <div className="tabstrip" role="tablist">
      <button
        type="button"
        role="tab"
        aria-selected={active === null}
        aria-label="처음 화면"
        title="처음 화면"
        className={`tab is-home ${active === null ? "is-active" : ""}`}
        onClick={() => onSelect(null)}
      >
        <Icon name="home" size={16} />
      </button>
      {tabs.map((t) => (
        <div
          key={t.id}
          role="tab"
          aria-selected={active === t.id}
          title={t.name}
          className={`tab ${active === t.id ? "is-active" : ""}`}
          onClick={() => onSelect(t.id)}
          onAuxClick={(e) => e.button === 1 && onClose(t.id)}
        >
          {busy(t.id) && <span className="tab-busy" aria-label="작업 중" />}
          <span className="tab-name t-label-m">{t.name || "여는 중…"}</span>
          <button
            type="button"
            className="tab-close"
            aria-label="탭 닫기"
            onClick={(e) => {
              e.stopPropagation();
              onClose(t.id);
            }}
          >
            <Icon name="close" size={12} />
          </button>
        </div>
      ))}
    </div>
  );
}
