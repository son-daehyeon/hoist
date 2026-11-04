import type { ApiError } from "./bindings";

export type { ApiError };

const isApiError = (e: unknown): e is ApiError =>
  typeof e === "object" &&
  e !== null &&
  typeof (e as ApiError).kind === "string" &&
  typeof (e as ApiError).message === "string";

export function errorMessage(e: unknown) {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  if (isApiError(e)) return e.message;
  return "문제가 생겼어요";
}
