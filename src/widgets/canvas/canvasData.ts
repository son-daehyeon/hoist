import { useEffect, useRef, useState } from "react";
import { api, errorMessage } from "@/shared/api";
import { pngSrc } from "@/shared/lib";
import { toast } from "@/shared/ui";
import { rememberAssetsDir } from "./commands";
import type { Frac } from "./geometry";

const FRAGMENT_DELAY_MS = 16;
const ORIGINAL_RENDER_PX = 2400;

type Fragment = { id: string; key: string; svg: string };

export function useLayerFragments(projectId: string, rev: number | undefined, onLoaded: () => void) {
  const known = useRef(new Map<string, string>());
  const [fragments, setFragments] = useState<Fragment[]>([]);
  const [inks, setInks] = useState<Record<string, Frac>>({});

  useEffect(() => {
    if (rev === undefined) return;
    let alive = true;
    const t = window.setTimeout(() => {
      api
        .layerSvgs(projectId, [...known.current.keys()])
        .then((r) => {
          if (!alive) return;
          const byKey = new Map<string, string>();
          const list: Fragment[] = [];
          const ink: Record<string, Frac> = {};
          for (const l of r.layers) {
            for (const t of l.inks) ink[t.id] = t.frac;
            const svg = l.svg ?? known.current.get(l.key);
            if (svg === undefined) continue;
            byKey.set(l.key, svg);
            list.push({ id: l.id, key: l.key, svg });
          }
          known.current = byKey;
          rememberAssetsDir(r.assets_dir);
          setFragments(list);
          setInks(ink);
          onLoaded();
        })
        .catch((e) => alive && toast(errorMessage(e), "error"));
    }, FRAGMENT_DELAY_MS);
    return () => {
      alive = false;
      window.clearTimeout(t);
    };
  }, [projectId, rev, onLoaded]);

  return { fragments, inks };
}

export function useOriginalSrc(projectId: string, version: number | undefined) {
  const [originalSrc, setOriginalSrc] = useState<string>();

  if (version === undefined && originalSrc !== undefined) setOriginalSrc(undefined);

  useEffect(() => {
    if (version === undefined) return;
    let alive = true;
    api
      .renderOriginal(projectId, version, ORIGINAL_RENDER_PX)
      .then((s) => alive && setOriginalSrc(pngSrc(s)))
      .catch((e) => alive && toast(errorMessage(e), "error"));
    return () => {
      alive = false;
    };
  }, [projectId, version]);

  return originalSrc;
}
