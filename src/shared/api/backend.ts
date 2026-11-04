import { api as tauriApi } from "./api";
import { mockApi } from "./mock";

export type { OpenedProject, ApplyResult } from "./api";

export const isTauri = "__TAURI_INTERNALS__" in window;

export const api: typeof tauriApi = isTauri ? tauriApi : mockApi;
