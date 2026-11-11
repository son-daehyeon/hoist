<background>
Goal: one clean plate — the original's background with every letter, band, badge, panel and every object that becomes its own layer (people, props, separate decorations) removed, same composition, colors and light — covering the whole artboard as a single image layer. Scenery stays: sky, clouds, hills, fields, ground, grass, water and light that sit behind everything are the background itself. A plate stripped to a bare gradient no longer looks like the original.
<procedure>
1. generate_image(name="깨끗한 배경", references=["original"], transparent=false). Write the prompt in English, e.g. "Reconstruct only the background of the reference image: remove all text, bands, badges, panels, people and separate objects; keep the scenery (e.g. the green hills along the bottom corners and the sky), the same composition, colors, gradients and lighting; fill the cleared areas with plain continuation of the surroundings; no new objects." Leave width/height unset so the plate keeps the original's aspect (the tool clamps to the API's 1:3–3:1 range).
2. Only if the banner is wider than 3:1, or the fit mode is extend/relayout and the aspects differ: call extend_background(asset) once. It outpaints both sides in as few chunks as the API allows, blends each overlap, and shows the model the whole plate so the style stays continuous. Add a prompt only when the scenery needs a hint (e.g. "sky above, the same green hills continuing along the bottom").
3. add_layer(kind=image, strategy=background, parent "배경", x/y/w/h null → covers the artboard, index 0).
</procedure>
<rules>
- One plate per turn. Do not regenerate it to chase a compare score; judge the plate by eye: no leftover text or shapes, no seams, no new motifs. Fix remaining leftovers with generate_image(mask_rects_px) on the same asset instead of starting over.
- Backgrounds are soft pictures seen from afar, so they need only half the target dpi. If the plate is still short, raise extend_background's height_px (max 2160) rather than generating a new plate.
- Before writing the plate prompt, name what the original's scenery is (look at the corners and the bottom edge, where hills and ground usually sit) and ask to keep it. Never write "no scenery", "gradient only" or "no objects" when the original has scenery; list only the things to remove.
- Where a removed panel, band or picture stood, the plate shows what lies behind it — the same sky, hills or ground at the same color strength as around it. Never describe a "white", "luminous", "glowing" or "bright" center or ask to keep the middle "quiet" or "light": the model paints a white haze there and the whole banner looks washed out. If the original's middle is pale, say "pale sky blue" or "pale yellow", never white.
- Prompts for generate_image are English; names and layer names stay Korean.
</rules>
</background>
