import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { isTauri } from "../backend";

const IMAGE = /\.(png|jpe?g|webp|bmp|gif)$/i;

export function Dropzone({ onPick, children }: { onPick: (path: string) => void; children: ReactNode }) {
  const [over, setOver] = useState(false);
  const pick = useRef(onPick);
  pick.current = onPick;

  useEffect(() => {
    if (!isTauri) return;
    let off: (() => void) | undefined;
    let alive = true;
    void getCurrentWebview()
      .onDragDropEvent(({ payload }) => {
        if (payload.type === "enter" || payload.type === "over") setOver(true);
        else if (payload.type === "leave") setOver(false);
        else {
          setOver(false);
          const path = payload.paths.find((p) => IMAGE.test(p));
          if (path) pick.current(path);
        }
      })
      .then((fn) => (alive ? (off = fn) : fn()));
    return () => {
      alive = false;
      off?.();
    };
  }, []);

  return (
    <div
      className={`dropzone ${over ? "is-over" : ""}`}
      onDragOver={(e) => {
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={(e) => {
        e.preventDefault();
        setOver(false);
        const file = [...e.dataTransfer.files].find((f) => IMAGE.test(f.name));
        if (file) onPick(file.name);
      }}
    >
      {children}
    </div>
  );
}
