import { api, errorMessage, type UpdateAvailable, type UpdateProgress } from "@/shared/api";
import { toast } from "@/shared/ui";

const KEY = "update";

export const updateErrorMessage = (e: unknown) =>
  typeof e === "object" && e !== null && "kind" in e && e.kind === "updater"
    ? "업데이트 서버에 연결하지 못했어요"
    : errorMessage(e);

const progressText = (p: UpdateProgress) =>
  p.type === "progress" && p.total
    ? `업데이트 내려받는 중 · ${Math.floor((p.downloaded / p.total) * 100)}%`
    : "업데이트 내려받는 중";

export async function installUpdate() {
  let shown = "";
  const show = (text: string) => {
    if (text === shown) return;
    shown = text;
    toast(text, "default", { key: KEY, sticky: true, icon: "download" });
  };
  show("업데이트 내려받는 중");
  try {
    await api.updateInstall((p) =>
      show(p.type === "finished" ? "설치 프로그램이 실행돼요. 앱이 잠시 닫혀요." : progressText(p)),
    );
  } catch (e) {
    toast(updateErrorMessage(e), "error");
  }
}

export function announceUpdate(update: UpdateAvailable) {
  toast(`새 버전 ${update.version}이 있어요`, "default", {
    key: KEY,
    sticky: true,
    icon: "download",
    actions: [
      { label: "지금 설치", onClick: () => void installUpdate() },
      { label: "나중에", kind: "ghost", onClick: () => {} },
    ],
  });
}
