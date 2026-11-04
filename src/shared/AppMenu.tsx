import { useState } from "react";
import { Button, Dialog, IconButton, Menu } from "../ui";

const SHORTCUTS: [string, string][] = [
  ["V", "선택"],
  ["H", "이동 (스페이스를 누른 채 끌어도 돼요)"],
  ["T", "글자"],
  ["R", "도형"],
  ["C", "캡처"],
  ["Ctrl + Z / Ctrl + Y", "되돌리기 / 다시 하기"],
  ["Delete", "선택한 레이어 지우기"],
  ["Ctrl + 휠", "확대 · 축소"],
  ["Ctrl + E", "내보내기"],
  ["Ctrl + N", "새 프로젝트"],
];

export function AppMenu({
  onNew,
  onOpenFolder,
  onSettings,
}: {
  onNew: () => void;
  onOpenFolder: () => void;
  onSettings: () => void;
}) {
  const [anchor, setAnchor] = useState<DOMRect | null>(null);
  const [info, setInfo] = useState<"help" | "about" | null>(null);

  return (
    <>
      <IconButton icon="menu" label="메뉴" onClick={(e) => setAnchor(e.currentTarget.getBoundingClientRect())} />
      {anchor && (
        <Menu
          anchor={anchor}
          onClose={() => setAnchor(null)}
          items={[
            { label: "새 프로젝트", icon: "plus", shortcut: "Ctrl+N", onSelect: onNew },
            { label: "폴더에서 열기", icon: "folder-open", shortcut: "Ctrl+O", onSelect: onOpenFolder },
            "divider",
            { label: "설정", icon: "settings", onSelect: onSettings },
            "divider",
            { label: "도움말", icon: "book-open", onSelect: () => setInfo("help") },
            { label: "정보", icon: "info", onSelect: () => setInfo("about") },
          ]}
        />
      )}
      {info === "help" && (
        <Dialog
          title="도움말"
          onClose={() => setInfo(null)}
          footer={<Button kind="primary" onClick={() => setInfo(null)}>확인</Button>}
        >
          <div className="help-list">
            {SHORTCUTS.map(([k, v]) => (
              <div key={k} className="help-row">
                <span className="t-label-s">{k}</span>
                <span className="t-body-m c-secondary">{v}</span>
              </div>
            ))}
          </div>
        </Dialog>
      )}
      {info === "about" && (
        <Dialog
          title="정보"
          onClose={() => setInfo(null)}
          footer={<Button kind="primary" onClick={() => setInfo(null)}>확인</Button>}
        >
          <div className="t-title-s">Banner Studio 0.1.0</div>
          <div className="t-caption c-tertiary">받은 현수막 이미지를 레이어로 다시 만들어 실물 크기로 내보내요.</div>
        </Dialog>
      )}
    </>
  );
}
