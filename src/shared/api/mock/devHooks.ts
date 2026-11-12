import type { UpdateInfo } from "../types";

export const fakeUpdate: UpdateInfo | null = new URLSearchParams(location.search).has("mockUpdate")
  ? { version: "0.2.0", notes: null, pub_date: null }
  : null;

export const mockAgent = new URLSearchParams(location.search).has("mockAgent");

export const mockLayerCount = () => Number(new URLSearchParams(location.search).get("mockLayers") ?? 0);

const calls: Record<string, number> = {};
(globalThis as { __hoistMockCalls?: Record<string, number> }).__hoistMockCalls = calls;

export function counted<T extends object>(api: T): T {
  return Object.fromEntries(
    Object.entries(api).map(([name, fn]) => [
      name,
      typeof fn === "function"
        ? (...args: unknown[]) => {
            calls[name] = (calls[name] ?? 0) + 1;
            return (fn as (...a: unknown[]) => unknown)(...args);
          }
        : fn,
    ]),
  ) as T;
}
