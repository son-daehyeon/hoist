import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

export type Mode = { mode: "install" | "uninstall"; projects_dir: string | null };
export type Plan = {
  default_dir: string;
  existing_dir: string | null;
  payload_ok: boolean;
};
export type Progress = { step: number; percent: number; detail: string };
type Installed = { dir: string; resources_error: string | null; resources_missing: boolean };

export const STEPS = [
  "WebView2 런타임 확인",
  "파일 복사",
  "바로가기 만들기",
  "프로그램 등록",
  "글꼴·모델 받기",
];
export const RESOURCES_START_PERCENT = 42;

export const mode = () => invoke<Mode>("mode");

export const plan = () => invoke<Plan>("plan");

export const install = (dir: string, desktopShortcut: boolean) =>
  invoke<Installed>("install", { args: { dir, desktop_shortcut: desktopShortcut } });

export const installResources = (dir: string) => invoke("install_resources", { dir });

export const uninstall = (keepProjects: boolean, removeSettings: boolean) =>
  invoke("uninstall", { args: { keep_projects: keepProjects, remove_settings: removeSettings } });

export const pickDir = () => invoke<string | null>("pick_dir");

export const launch = (dir: string) => invoke("launch", { dir });

export const onProgress = (handler: (progress: Progress) => void) =>
  listen<Progress>("install-progress", (e) => handler(e.payload));

export const closeWindow = () => getCurrentWindow().close();
