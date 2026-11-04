import { api as tauriApi } from "./tauri";
import { mockApi } from "./mock";

export type { OpenedProject, ApplyResult } from "./tauri";

export const isTauri = "__TAURI_INTERNALS__" in window;

export const api: typeof tauriApi = isTauri ? tauriApi : mockApi;
