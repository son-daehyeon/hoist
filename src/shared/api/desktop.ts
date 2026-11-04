import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview, type DragDropEvent } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
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
