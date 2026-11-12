import { api, type Op } from "@/shared/api";
import { solid } from "@/shared/lib";

export const TUTORIAL_TITLE = "T6";
export const PRACTICE_INFO = "매주 토요일 오전 10시 · 동네 공원";

export async function boldFamily() {
  const fonts = await api.listFonts().catch(() => []);
  const pick = (re: RegExp) => fonts.find((f) => re.test(f.family) && f.weights.some((w) => w >= 700));
  return (
    (pick(/Gmarket/i) ?? pick(/Nanum|Barun/i) ?? fonts.find((f) => f.bundled) ?? fonts[0])?.family ??
    "sans-serif"
  );
}

/** The layers that rebuild the practice banner (src-tauri/assets/tutorial-banner.png), in build order. */
export function practiceLayers(family: string, info: string): { label: string; op: Op }[] {
  const group = (id: string, name: string): Op => ({
    op: "add_layer",
    layer: {
      id,
      name,
      kind: { type: "group" },
      x_mm: 0,
      y_mm: 0,
      w_mm: 6000,
      h_mm: 900,
      strategy: "native_vector",
    },
  });
  const text = (
    id: string,
    name: string,
    content: string,
    size: number,
    color: string,
    box: [number, number, number, number],
  ): Op => ({
    op: "add_layer",
    layer: {
      id,
      name,
      parent: "T5",
      kind: {
        type: "text",
        text: content,
        font_family: family,
        font_weight: 700,
        font_size_mm: size,
        align: "center",
        line_height: 1.2,
      },
      x_mm: box[0],
      y_mm: box[1],
      w_mm: box[2],
      h_mm: box[3],
      fills: [solid(color)],
      strategy: "text",
    },
  });
  return [
    { label: "그룹 · 배경", op: group("T1", "배경") },
    {
      label: "레이어 추가 · 하늘색 바탕",
      op: {
        op: "add_layer",
        layer: {
          id: "T2",
          name: "하늘색 바탕",
          parent: "T1",
          kind: { type: "shape", shape: "rect" },
          x_mm: 0,
          y_mm: 0,
          w_mm: 6000,
          h_mm: 900,
          fills: [
            {
              type: "linear_gradient",
              angle_deg: 90,
              stops: [
                { offset: 0, color: "#DCEBFA", opacity: 1 },
                { offset: 1, color: "#FFF6D8", opacity: 1 },
              ],
            },
          ],
          strategy: "background",
        },
      },
    },
    { label: "그룹 · 그래픽", op: group("T3", "그래픽") },
    {
      label: "레이어 추가 · 보라 띠",
      op: {
        op: "add_layer",
        layer: {
          id: "T4",
          name: "보라 띠",
          parent: "T3",
          kind: { type: "shape", shape: "rect", rx_mm: 55 },
          x_mm: 2100,
          y_mm: 545,
          w_mm: 1800,
          h_mm: 110,
          fills: [solid("#7B4FBF")],
          strategy: "native_vector",
        },
      },
    },
    { label: "그룹 · 글자", op: group("T5", "글자") },
    {
      label: "레이어 추가 · 제목",
      op: text(TUTORIAL_TITLE, "제목", "우리 동네 주민 모임", 250, "#1F3F8F", [1000, 150, 4000, 330]),
    },
    {
      label: "레이어 추가 · 부제",
      op: text("T7", "부제", "함께 걷고 이야기 나눠요", 70, "#FFFFFF", [2100, 557, 1800, 90]),
    },
    { label: "레이어 추가 · 안내", op: text("T8", "안내", info, 50, "#333333", [1500, 718, 3000, 70]) },
  ];
}
