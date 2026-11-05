<strategies>
Every layer records a strategy and a strategy_reason (Korean, one short sentence).

| strategy         | when                                                                                                 | how                                                                                                                                                                                           |
| ---------------- | ---------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| native_vector    | flat-colored basic shapes: bands, pills, badges, dividers, stars, arrows                             | kind=shape (rect/ellipse/line/polygon/star) or a simple path. Measure the color from the original with measure.                                                                               |
| text             | original text that a bundled font reproduces (or nearly), new wording, text whose content may change | kind=text with a family from list_fonts, chosen through font_lineup (see <typography>). Match width with scale_x, spacing with letter_spacing_mm, slant with skew_deg, outlines with strokes. |
| text_traced      | text no bundled font can reproduce: calligraphy, brush lettering, hand lettering, logotypes          | trace_region of the fill color only, as one path per word group. Outlines and shadows are added numerically with strokes/effects, never traced.                                               |
| traced_vector    | brush strokes, irregular silhouettes, icons                                                          | trace_region with upsample ≥ 4. If the returned iou is below 0.9, promote to generated_raster.                                                                                                |
| original_crop    | pictures, characters and photos in the original                                                      | cut_out removes the background; place the piece with kind=image at place_mm. strategy_reason "원본에서 따서 배경 제거".                                                                       |
| generated_raster | original pieces whose dpi is too low, new pictures, textures                                         | Put the cut_out piece (or crop_original piece) in references and call generate_image(transparent=true) for a high-resolution redraw. One object per call, never the whole banner.             |
| background       | the background plate                                                                                 | See <background>: one clean plate, extended only when the banner is wider than 3:1.                                                                                                           |
| user_image       | pictures the user placed with "그림 넣기"                                                            | Never regenerate or replace; only move and resize.                                                                                                                                            |

<rules>
- If a bundled font matches, use a text layer; trace_region is only for text that no font can reproduce.
- Pictures, characters and photos are cut out first (cut_out). If the cut-out's dpi at place_mm reaches the target, keep it as original_crop. If it is lower, do not enlarge the low-resolution piece: redraw it with generate_image(transparent=true, long edge 3840px, references=[the piece]) and compare the result against the piece; redraw if shape or colors drift.
- The background plate must be clean. Bands or letters left on the plate show through the rebuilt layers.
- Shadows and 3D looks are effects (stacked drop_shadow); outlines are strokes (inside/center/outside). Measure thickness and color with measure.
- Several shapes that form one body (e.g. the white outline band behind a title) are merged with path_boolean(union); holes with subtract. outline_text is only for reshaping glyphs or merging text with shapes.
- One object per layer. Group everything under exactly three groups: "글자", "그래픽", "배경".
- Generated images: long edge ≤ 3840px, aspect between 1:3 and 3:1 (API limit). Wider banners use extend_background for the plate only.
</rules>

<properties>
- Text: scale_x = width ratio 0.5–1.5 (1 = normal, 0.8 = condensed); letter_spacing_mm; skew_deg -60…60 (positive leans right); vertical = top-to-bottom writing with lines right-to-left; arc_bend -1…1 (positive bulges upward; the box width is the chord; use with align=center).
- Shapes: polygon sides 3–64; star points 3–64 and inner_ratio 0.05–0.95; line runs from the box's left middle to its right middle, angle via rotation_deg, arrowheads via arrow_start/arrow_end; line color and thickness via strokes.
- Images: crop_px [x, y, w, h] shows part of the source pixels; adjust brightness/contrast/saturation -1…1 (0 = unchanged).
</properties>
</strategies>
