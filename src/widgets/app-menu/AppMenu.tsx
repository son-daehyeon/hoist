import { useEffect, useState } from "react";
import { api, errorMessage, isSystemFontPath, openPath } from "@/shared/api";
import { Button, Dialog, IconButton, Menu, toast, type MenuItem } from "@/shared/ui";
import "./app-menu.css";

async function openFontLicenses() {
  try {
    const fonts = await api.listFonts();
    const path = fonts
      .flatMap((f) => (f.bundled ? f.files : []))
      .find((x) => x.path && !isSystemFontPath(x.path))?.path;
    if (!path) return toast("함께 들어 있는 글꼴을 찾지 못했어요", "error");
    const dir = path.slice(0, Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\")) + 1);
    await openPath(`${dir}LICENSES.md`);
  } catch (e) {
    toast(errorMessage(e), "error");
  }
}

const SHORTCUTS: [string, string][] = [
  ["V", "선택"],
  ["H", "이동 (스페이스를 누른 채 끌어도 돼요)"],
  ["T", "글자"],
  ["R", "도형"],
  ["C", "캡처"],
  ["Ctrl + Z / Ctrl + Shift + Z", "되돌리기 / 다시 하기"],
  ["Ctrl + Y", "윤곽선 보기 켜고 끄기"],
  ["Ctrl + D", "선택한 레이어 복제"],
  ["Delete", "선택한 레이어 지우기"],
  ["Ctrl + 휠", "확대 · 축소"],
  ["Ctrl + E", "내보내기"],
  ["Ctrl + N", "새 프로젝트"],
];

export function AppMenu({
  onNew,
  onOpenFolder,
  onSettings,
  fileItems = [],
  sections = [],
}: {
  onNew: () => void;
  onOpenFolder: () => void;
  onSettings: () => void;
  fileItems?: MenuItem[];
  sections?: { title: string; items: MenuItem[] }[];
}) {
  const [anchor, setAnchor] = useState<DOMRect | null>(null);
  const [info, setInfo] = useState<"help" | "about" | null>(null);
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    void api.appVersion().then(setVersion);
  }, []);

  return (
    <>
      <IconButton
        icon="menu"
        label="메뉴"
        onClick={(e) => setAnchor(e.currentTarget.getBoundingClientRect())}
      />
      {anchor && (
        <Menu
          anchor={anchor}
          onClose={() => setAnchor(null)}
          items={[
            ...(sections.length ? [{ heading: "파일" }] : []),
            { label: "새 프로젝트", icon: "plus", shortcut: "Ctrl+N", onSelect: onNew },
            { label: "폴더에서 열기", icon: "folder-open", shortcut: "Ctrl+O", onSelect: onOpenFolder },
            ...fileItems,
            ...sections.flatMap((sec): MenuItem[] => ["divider", { heading: sec.title }, ...sec.items]),
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
          footer={
            <Button kind="primary" onClick={() => setInfo(null)}>
              확인
            </Button>
          }
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
          footer={
            <Button kind="primary" onClick={() => setInfo(null)}>
              확인
            </Button>
          }
        >
          <div className="t-title-s">{version ? `Hoist ${version}` : "Hoist"}</div>
          <div className="t-caption c-tertiary">
            받은 현수막 이미지를 레이어로 다시 만들어 실물 크기로 내보내요.
          </div>
          <div className="about-fonts">
            <Button icon="book-open" onClick={() => void openFontLicenses()}>
              글꼴 라이선스 보기
            </Button>
            <span className="t-body-m c-tertiary">함께 들어 있는 글꼴의 출처와 사용 조건이에요.</span>
          </div>
        </Dialog>
      )}
    </>
  );
}
