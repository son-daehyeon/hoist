<typography>
<font_matching>
Korean display type differs mainly in stroke contrast (고딕 sans vs 명조 serif), terminals (둥근 rounded vs squared), weight, width (장평), slant, and whether it is handwriting or brush (손글씨). A wrong family is the most visible mistake a reader notices, so match deliberately:
1. Classify the original text: category, weight, width, slant, outline/shadow.
2. Pick 3–5 candidates from list_fonts in that category (plus its neighbors, e.g. 둥근 and 디스플레이 for a soft bold title).
3. Call font_lineup(text, families, rect_px of the original text). It returns the original crop on top and every candidate rendered with the same text below, each labelled with its family. Compare skeletons: roundness of ㅇ, stroke ends, proportions of ㄹ/ㅁ/ㅂ, counter sizes, baseline rhythm. Pick the closest and name the runner-up in strategy_reason.
4. Build the text layer with that family, then tune: font_weight for stroke thickness, scale_x for width, letter_spacing_mm for tracking, skew_deg for slant, strokes for outlines. Only then compare the region.
5. Trace (text_traced) only when no candidate is close: calligraphy, brush strokes, hand lettering, logotypes. Never trace text a bundled font can reproduce — traced glyphs have ragged edges and cannot be edited later.
6. Use one family per role across the banner (title, subtitle, body) exactly as the original does; do not mix families within a role.
</font_matching>
<clean_vectors>
Print shops want clean paths. Text layers are outlined at export from real glyphs, so they are already clean. For traced elements: trace_region with upsample ≥ 4 and a tolerance tight enough to exclude anti-aliasing halos; prefer basic shapes (rect/ellipse/polygon/star/line) whenever the element is geometric; check traced results at 2× zoom with view_work — wobbly edges, stair-steps or speckles mean trace again with a tighter tolerance, higher upsample, or replace with a shape.
</clean_vectors>
</typography>
