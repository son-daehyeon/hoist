import { useCallback, useEffect, useLayoutEffect, useRef, useState, type RefObject } from "react";

export type Rect = { x: number; y: number; w: number; h: number };
export type View = { zoom: number; pan: { x: number; y: number } };
export type Panes = { original: Rect | null; work: Rect };

export const LABEL_H = 22;
const GAP = 36;
const PAD = 40;
const MIN_ZOOM = 0.01;
const MAX_ZOOM = 8;

const clampZoom = (z: number) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z));

export function panesFor(view: View, W: number, H: number, hasOriginal: boolean): Panes {
  const w = W * view.zoom;
  const h = H * view.zoom;
  const first = { x: view.pan.x, y: view.pan.y + LABEL_H, w, h };
  if (!hasOriginal) return { original: null, work: first };
  return { original: first, work: { x: first.x, y: first.y + h + GAP + LABEL_H, w, h } };
}

function fitView(vw: number, vh: number, W: number, H: number, hasOriginal: boolean): View {
  const rows = hasOriginal ? 2 : 1;
  const zw = (vw - PAD * 2) / W;
  const zh = (vh - PAD * 2 - rows * LABEL_H - (rows - 1) * GAP) / (rows * H);
  const zoom = clampZoom(Math.min(zw, zh));
  return { zoom, pan: { x: (vw - W * zoom) / 2, y: PAD } };
}

export function zoomAt(v: View, factor: number, px: number, py: number): View {
  const zoom = clampZoom(v.zoom * factor);
  const k = zoom / v.zoom;
  return { zoom, pan: { x: px - (px - v.pan.x) * k, y: py - (py - v.pan.y) * k } };
}

export function rulerStep(zoom: number) {
  const major = [1, 5, 10, 50, 100, 500, 1000, 5000, 10000].find((m) => m * zoom >= 50) ?? 10000;
  return { major, minor: major / 5 };
}

export function renderScale(zoom: number, W: number, H: number) {
  const want = zoom * (window.devicePixelRatio || 1);
  const step = [0.0625, 0.125, 0.25, 0.5, 1, 2, 4, 8].find((s) => s >= want) ?? 8;
  // ponytail: whole-document rasters capped at 4096 px per side; render visible tiles if deep zoom gets blurry
  return Math.min(step, 4096 / Math.max(W, H));
}

export function useViewport(ref: RefObject<HTMLDivElement | null>, W: number, H: number, hasOriginal: boolean) {
  const [view, setViewState] = useState<View>({ zoom: 0.1, pan: { x: PAD, y: PAD } });
  const [size, setSize] = useState({ w: 0, h: 0 });
  const auto = useRef(true);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setSize({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el);
    return () => ro.disconnect();
  }, [ref]);

  useEffect(() => {
    if (auto.current && size.w > 0 && W > 0 && H > 0) setViewState(fitView(size.w, size.h, W, H, hasOriginal));
  }, [size, W, H, hasOriginal]);

  const setView = useCallback((fn: (v: View) => View) => {
    auto.current = false;
    setViewState(fn);
  }, []);

  const fit = useCallback(() => {
    auto.current = true;
    if (size.w > 0) setViewState(fitView(size.w, size.h, W, H, hasOriginal));
  }, [size, W, H, hasOriginal]);

  const zoomBy = useCallback(
    (factor: number) => setView((v) => zoomAt(v, factor, size.w / 2, size.h / 2)),
    [setView, size],
  );

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const wheel = (e: WheelEvent) => {
      e.preventDefault();
      const r = el.getBoundingClientRect();
      if (e.ctrlKey || e.metaKey) {
        setView((v) => zoomAt(v, Math.exp(-e.deltaY * 0.0025), e.clientX - r.left, e.clientY - r.top));
      } else {
        const dx = e.shiftKey ? e.deltaY : e.deltaX;
        const dy = e.shiftKey ? 0 : e.deltaY;
        setView((v) => ({ ...v, pan: { x: v.pan.x - dx, y: v.pan.y - dy } }));
      }
    };
    el.addEventListener("wheel", wheel, { passive: false });
    return () => el.removeEventListener("wheel", wheel);
  }, [ref, setView]);

  return { view, setView, fit, zoomBy, size, panes: panesFor(view, W, H, hasOriginal) };
}
