import type { ReactNode } from "react";
import { windowControls } from "@/shared/api";
import { WindowControls } from "@/shared/ui";
import "./title-bar.css";

export function TitleBar({ left, title, right }: { left: ReactNode; title: ReactNode; right?: ReactNode }) {
  return (
    <header className="titlebar" data-tauri-drag-region>
      <div className="titlebar-left">{left}</div>
      <div className="titlebar-title" data-tauri-drag-region>
        {title}
      </div>
      <div className="titlebar-right">{right}</div>
      <WindowControls
        onMinimize={windowControls.minimize}
        onMaximize={windowControls.toggleMaximize}
        onClose={windowControls.close}
      />
    </header>
  );
}
