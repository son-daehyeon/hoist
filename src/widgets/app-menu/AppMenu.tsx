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

const SHORTCUTS: { title: string; rows: [string, string][] }[] = [
  {
    title: "도구",
    rows: [
      ["V", "선택"],
      ["H", "이동"],
      ["T", "글자"],
      ["R", "도형"],
      ["P", "펜"],
      ["C", "캡처"],
    ],
  },
  {
    title: "마우스",
    rows: [
      ["오른쪽 클릭", "메뉴"],
      ["더블 클릭", "글자·그림·패스 고치기"],
      ["Shift+클릭", "선택 더하기 / 빼기"],
      ["Alt+끌기", "복제"],
      ["Shift+끌기", "방향·비율 고정, 15°씩 회전"],
    ],
  },
  {
    title: "파일 · 탭",
    rows: [
      ["Ctrl+N", "새 프로젝트"],
      ["Ctrl+O", "폴더에서 열기"],
      ["Ctrl+E", "내보내기"],
      ["Ctrl+Tab / Ctrl+Shift+Tab", "다음 / 이전 탭"],
      ["Ctrl+W", "탭 닫기"],
    ],
  },
  {
    title: "편집",
    rows: [
      ["Ctrl+Z / Ctrl+Shift+Z", "되돌리기 / 다시 하기"],
      ["Ctrl+C / Ctrl+V", "복사 / 붙여넣기"],
      ["Ctrl+D", "복제"],
      ["Ctrl+A", "모두 선택"],
      ["Delete", "삭제"],
      ["Esc", "선택 해제"],
      ["방향키 / Shift+방향키", "1mm / 10mm 이동"],
      ["Ctrl+] / Ctrl+[", "앞으로 / 뒤로"],
      ["Ctrl+Shift+] / Ctrl+Shift+[", "맨 앞으로 / 맨 뒤로"],
    ],
  },
  {
    title: "고치는 중",
    rows: [
      ["Enter", "적용, 펜 마치기"],
      ["Ctrl+Enter", "글자 적용"],
      ["Esc", "취소, 펜 마치기"],
      ["Delete", "점 지우기"],
      ["Alt+클릭", "점 모양 바꾸기"],
    ],
  },
  {
    title: "보기",
    rows: [
      ["Ctrl+휠", "확대 / 축소"],
      ["휠 / Shift+휠", "위아래 / 좌우 이동"],
      ["Space+끌기 / 휠 버튼 끌기", "화면 이동"],
      ["Ctrl+Y", "윤곽선 보기"],
    ],
  },
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
          width={720}
          onClose={() => setInfo(null)}
          footer={
            <Button kind="primary" onClick={() => setInfo(null)}>
              확인
            </Button>
          }
        >
          <div className="help-list">
            {SHORTCUTS.map((g) => (
              <div key={g.title} className="help-group">
                <div className="t-label-s c-tertiary">{g.title}</div>
                {g.rows.map(([k, v]) => (
                  <div key={k} className="help-row">
                    <span className="t-label-s">{k}</span>
                    <span className="t-body-m c-secondary">{v}</span>
                  </div>
                ))}
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
          <div className="about-fonts">
            <Button icon="book-open" onClick={() => void openFontLicenses()}>
              글꼴 라이선스 보기
            </Button>
          </div>
        </Dialog>
      )}
    </>
  );
}
