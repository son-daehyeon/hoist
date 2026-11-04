import { getCurrentWindow } from "@tauri-apps/api/window";
import type { ReactNode } from "react";
import { isTauri } from "../backend";
import { WindowControls } from "../ui";
import "./shared.css";

const win = (fn: (w: ReturnType<typeof getCurrentWindow>) => Promise<void>) => () => {
  if (isTauri) void fn(getCurrentWindow());
};

export function TitleBar({ left, title, right }: { left: ReactNode; title: ReactNode; right?: ReactNode }) {
  return (
    <header className="titlebar" data-tauri-drag-region>
      <div className="titlebar-left">{left}</div>
      <div className="titlebar-title" data-tauri-drag-region>
        {title}
      </div>
      <div className="titlebar-right">{right}</div>
      <WindowControls
        onMinimize={win((w) => w.minimize())}
        onMaximize={win((w) => w.toggleMaximize())}
        onClose={win((w) => w.close())}
      />
    </header>
  );
}
