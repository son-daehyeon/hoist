import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { getCurrentWebview, type DragDropEvent } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openPath as openWithSystem } from "@tauri-apps/plugin-opener";
import { isTauri } from "./backend";

const win = (fn: (w: ReturnType<typeof getCurrentWindow>) => Promise<void>) => () => {
  if (isTauri) void fn(getCurrentWindow());
};

export const windowControls = {
  minimize: win((w) => w.minimize()),
  toggleMaximize: win((w) => w.toggleMaximize()),
  close: win((w) => w.close()),
  showSnapOverlay: () => {
    if (isTauri && document.hasFocus()) void invoke("plugin:decorum|show_snap_overlay");
  },
};

export const onDragDrop = (handler: (event: DragDropEvent) => void) =>
  getCurrentWebview().onDragDropEvent(({ payload }) => handler(payload));

export function onFileDrop(handler: (paths: string[]) => void) {
  if (!isTauri) return () => {};
  let off: (() => void) | undefined;
  let alive = true;
  void onDragDrop((e) => {
    if (e.type === "drop") handler(e.paths);
  }).then((fn) => (alive ? (off = fn) : fn()));
  return () => {
    alive = false;
    off?.();
  };
}

export const openPath = (path: string) => (isTauri ? openWithSystem(path) : Promise.resolve());

export const fontFaceUrl = (path: string) => (isTauri ? convertFileSrc(path) : path);

const SYSTEM_FONT_DIR = /[\\/](?:microsoft[\\/])?windows[\\/]fonts[\\/]/i;
export const isSystemFontPath = (path: string) => SYSTEM_FONT_DIR.test(path);
