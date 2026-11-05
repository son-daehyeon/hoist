<typography>
<font_matching>
Korean display type differs mainly in stroke contrast (고딕 sans vs 명조 serif), terminals (둥근 rounded vs squared), weight, width (장평), slant, and whether it is handwriting or brush (손글씨). A wrong family is the most visible mistake a reader notices, so match deliberately:
1. Classify the original text: category, weight, width, slant, outline/shadow.
2. Pick 3–5 candidates from list_fonts in that category (plus its neighbors, e.g. 둥근 and 디스플레이 for a soft bold title).
3. Call font_lineup(text, families, rect_px) with a tight crop of ONE line — one word of 2–6 large glyphs works best. It returns the picture (original crop on top, each candidate below) and `matches`: for every family a score (overlap of its glyphs with the original's ink, 0–1) and the scale_x and letter_spacing_em that reproduce the original's proportions. When `reliable` is true, use the top-scoring family; families within 0.02 of each other are a tie, broken by stroke ends (round vs square) in the picture. When `reliable` is false (small, multi-colored or busy crops), judge by the picture. Name the runner-up in strategy_reason.
4. Build the text layer with that family and its fitted scale_x and letter spacing (letter_spacing_mm = letter_spacing_em × font_size_mm). These two values are part of the font match: reuse them unchanged for every text of the same role, and never change scale_x to make text fit a space. Tune font_weight and strokes for thickness, skew_deg for slant.
5. Trace (text_traced) only when no candidate is close: calligraphy, brush strokes, hand lettering, logotypes. Never trace text a bundled font can reproduce — traced glyphs have ragged edges and cannot be edited later.
6. Use one family per role across the banner (title, subtitle, body) exactly as the original does; do not mix families within a role.
</font_matching>
<clean_vectors>
Print shops want clean paths. Text layers are outlined at export from real glyphs, so they are already clean. For traced elements: trace_region with upsample ≥ 4 and a tolerance tight enough to exclude anti-aliasing halos; prefer basic shapes (rect/ellipse/polygon/star/line) whenever the element is geometric; check traced results at 2× zoom with view_work — wobbly edges, stair-steps or speckles mean trace again with a tighter tolerance, higher upsample, or replace with a shape.
</clean_vectors>
</typography>
