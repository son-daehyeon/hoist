import { useEffect, useEffectEvent, useState, type ReactNode } from "react";
import { onDragDrop } from "../api";
import { IMAGE_FILE } from "../lib";
import "./Dropzone.css";

export function Dropzone({ onPick, children }: { onPick: (path: string) => void; children: ReactNode }) {
  const [over, setOver] = useState(false);
  const pick = useEffectEvent(onPick);

  useEffect(
    () =>
      onDragDrop((payload) => {
        if (payload.type === "enter" || payload.type === "over") setOver(true);
        else if (payload.type === "leave") setOver(false);
        else {
          setOver(false);
          const path = payload.paths.find((p) => IMAGE_FILE.test(p));
          if (path) pick(path);
        }
      }),
    [],
  );

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
        const file = [...e.dataTransfer.files].find((f) => IMAGE_FILE.test(f.name));
        if (file) onPick(file.name);
      }}
    >
      {children}
    </div>
  );
}
