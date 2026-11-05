import { useEffect, useState } from "react";
import { api, fontFaceUrl, isSystemFontPath } from "@/shared/api";

export type Font = Awaited<ReturnType<typeof api.listFonts>>[number];

export const SYSTEM_GROUP = "내 컴퓨터 글꼴";
export const fontLabel = (f: Font) => f.display_name ?? f.family;

let cache: Font[] | null = null;
let pending: Promise<Font[]> | null = null;

export function useFonts() {
  const [fonts, setFonts] = useState<Font[] | null>(cache);
  useEffect(() => {
    if (cache) return;
    let alive = true;
    pending ??= api.listFonts().then(
      (f) => (cache = f),
      () => (cache = []),
    );
    void pending.then((f) => alive && setFonts(f));
    return () => {
      alive = false;
    };
  }, []);
  return fonts;
}

const loaded = new Set<string>();

export function ensureFont(f: Font, weight = 400) {
  if (!f.bundled) return;
  const own = f.files.filter((x) => !isSystemFontPath(x.path));
  const pool = own.length ? own : f.files;
  const normal = pool.filter((x) => x.style === "normal");
  const file = [...(normal.length ? normal : pool)].sort(
    (a, b) => Math.abs(a.weight - weight) - Math.abs(b.weight - weight),
  )[0];
  const key = `${f.family}@${file?.weight}`;
  if (!file?.path || loaded.has(key)) return;
  loaded.add(key);
  new FontFace(f.family, `url("${fontFaceUrl(file.path)}")`, { weight: String(file.weight) })
    .load()
    .then((face) => document.fonts.add(face))
    .catch(() => loaded.delete(key));
}

export function groupFonts(fonts: Font[]): { title: string; fonts: Font[] }[] {
  const groups = new Map<string, Font[]>();
  for (const f of fonts) {
    const title = f.bundled ? (f.category ?? "그 외") : SYSTEM_GROUP;
    groups.set(title, [...(groups.get(title) ?? []), f]);
  }
  const system = groups.get(SYSTEM_GROUP);
  groups.delete(SYSTEM_GROUP);
  if (system) groups.set(SYSTEM_GROUP, system);
  return [...groups].map(([title, list]) => ({ title, fonts: list }));
}
