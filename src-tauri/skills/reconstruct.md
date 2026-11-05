<workflow kind="reconstruct">
Round budget: round 1 sends the reading tools together (view_original, analyze_original, list_fonts) plus one ask_user when the banner's aspect differs from the original's. If small text (dates, phone numbers, addresses, small digits) is not certainly readable, put those confirmation questions (what you read, allow_free_text, rect_px) into the same batch (max 3 questions). The fit answer is saved automatically (no set_fit_mode), so round 2 sends set_plan together with the first edits — sequential tools run first, so edits in the same round are not blocked. Edit tools are refused before set_plan. Think deeply through the plan; execution rounds run on the standard model.

<steps>
1. Look at the whole original (view_original), get palette and regions (analyze_original), read every text yourself and note content and position.
2. Plan the layers and save with set_plan: one background, n graphics (bands, icons, pictures), n texts, each with strategy and reason. image_quality: background medium; figures by their size and importance.
3. Fit mode (aspect gap over 5%): follow the project's fit_mode. extend_background keeps elements where the original had them; relayout follows <relayout>.
4. Background per <background>: one clean plate, extend only if needed, add as strategy=background in group "배경" (coordinates null = whole artboard).
5. Graphics: flat shapes as shape layers; brush strokes and icons with trace_region; pictures, characters and photos with cut_out as image layers (strategy original_crop, reason "원본에서 따서 배경 제거") at place_mm. Low-dpi cut-outs are redrawn with generate_image(transparent=true, references=[piece]) and compared against the piece. Group "그래픽".
6. Text per <typography>: font_lineup against the original crop, then a text layer tuned with weight/scale_x/spacing/skew/strokes. Only calligraphy, brush and hand lettering become text_traced, one word group per path. Outlines and shadows via strokes/effects. Group "글자".
7. Full compare. Below 0.85 similarity or with large worst_regions, fix the responsible layers; after two failures promote the strategy. In relayout the <self_check> replaces this threshold.
8. export_check, then the report: what changed, what still differs, what to ask next — short, Korean.
</steps>
</workflow>
