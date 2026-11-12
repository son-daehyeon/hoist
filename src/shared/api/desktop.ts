import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { getCurrentWebview, type DragDropEvent } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openPath as openWithSystem } from "@tauri-apps/plugin-opener";
import { isTauri, listening } from "./backend";

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
  isTauri ? listening(getCurrentWebview().onDragDropEvent(({ payload }) => handler(payload))) : () => {};

export const onFileDrop = (handler: (paths: string[]) => void) =>
  onDragDrop((e) => {
    if (e.type === "drop") handler(e.paths);
  });

export const openPath = (path: string) => (isTauri ? openWithSystem(path) : Promise.resolve());

export const fontFaceUrl = (path: string) => (isTauri ? convertFileSrc(path) : path);

const SYSTEM_FONT_DIR = /[\\/](?:microsoft[\\/])?windows[\\/]fonts[\\/]/i;
export const isSystemFontPath = (path: string) => SYSTEM_FONT_DIR.test(path);

/** The desktop window hides to the tray when closed (see `build_tray`); only a browser tab can lose a running turn. */
export function guardClose(busy: () => boolean): () => void {
  if (isTauri) return () => {};
  const onUnload = (e: BeforeUnloadEvent) => {
    if (busy()) e.preventDefault();
  };
  window.addEventListener("beforeunload", onUnload);
  return () => window.removeEventListener("beforeunload", onUnload);
}
