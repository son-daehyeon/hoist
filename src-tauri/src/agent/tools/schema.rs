use serde_json::{Value, json};

use crate::agent::layer_args;

pub fn definitions() -> Vec<Value> {
    let f = |name: &str, desc: &str, props: Value, required: &[&str]| json!({"type":"function","name":name,"description":desc,"parameters":{"type":"object","properties":props,"required":required,"additionalProperties":false}});
    let rect_px = json!({"type":"array","items":{"type":"integer"},"minItems":4,"maxItems":4,"description":"[x, y, w, h] in original pixels"});
    let rect_mm = json!({"type":"array","items":{"type":"number"},"minItems":4,"maxItems":4,"description":"[x, y, w, h] in mm"});
    vec![
        f(
            "get_document",
            "Returns the current document: artboard and a summary of every layer.",
            json!({}),
            &[],
        ),
        f(
            "get_layer",
            "Returns the full JSON of one layer.",
            json!({"id":{"type":"string"}}),
            &["id"],
        ),
        f(
            "view_original",
            "Shows the original image, or a zoomed part of it when rect_px is given.",
            json!({"version":{"type":["integer","null"]},"rect_px":{"anyOf":[rect_px.clone(),{"type":"null"}]},"max_px":{"type":["integer","null"],"description":"longest edge in px (default 1536)"}}),
            &[],
        ),
        f(
            "view_work",
            "Renders the current document. rect_mm limits the region, layer_ids limits the layers.",
            json!({"rect_mm":{"anyOf":[rect_mm.clone(),{"type":"null"}]},"layer_ids":{"type":["array","null"],"items":{"type":"string"}},"max_px":{"type":["integer","null"]}}),
            &[],
        ),
        f(
            "compare",
            "Shows the original and the work side by side and returns scores (similarity 0–1, overlap, edge_similarity) plus worst_regions (fractional coordinates 0–1).",
            json!({"rect_mm":{"anyOf":[rect_mm.clone(),{"type":"null"}]}}),
            &[],
        ),
        f(
            "analyze_original",
            "Returns the original's palette and candidate regions (text / shape / photo / background, bbox_px).",
            json!({"version":{"type":["integer","null"]}}),
            &[],
        ),
        f(
            "measure",
            "Measures a region of the original: dominant color, palette, and the bbox of one color.",
            json!({"rect_px":rect_px.clone(),"version":{"type":["integer","null"]},"color":{"type":["string","null"],"description":"#RRGGBB whose bbox to measure"},"tolerance":{"type":["integer","null"]}}),
            &["rect_px"],
        ),
        layer_args::add_layer_tool(),
        layer_args::update_layer_tool(),
        f(
            "delete_layer",
            "Deletes a layer (and its children if it is a group).",
            json!({"id":{"type":"string"}}),
            &["id"],
        ),
        f(
            "move_layer",
            "Changes a layer's parent or z-order. parent takes a group id or name (created when missing); index 0 is the bottom.",
            json!({"id":{"type":"string"},"parent":{"type":["string","null"]},"index":{"type":"integer"}}),
            &["id", "index"],
        ),
        json!({"type":"function","name":"arrange_row","strict":true,
            "description":"Lays items out left to right on one line using their real ink boxes (glyph outlines for text, the box for shapes and pictures), so gaps are exact instead of guessed. Each item is one layer or several layers that move together (e.g. the badge, its label and the body text of one info column). Only x changes unless align_y is set; sizes and fonts stay. align_y \"center\" moves every item so its ink's vertical center sits on the tallest item's center (a pill label beside its text in one band), \"top\" lines up the tops with the tallest item's top; null leaves y alone (info columns). justify \"center\" centers the row in [x0_mm, x1_mm] with gap_mm between items; \"start\" and \"end\" push it to that side with gap_mm; \"space_between\" spreads the items across the whole range with equal gaps (gap_mm ignored). Returns each item's ink box, the content width, the gap used, the row extent and the x of every gap center (for dividers). When the content is wider than the range it reports overflow: reduce the font size or re-break lines and call again. Layers are given by id or exact name, so send it in the same round as the add_layer calls that create them; it runs after them.",
            "parameters":{"type":"object","additionalProperties":false,"required":["items","x0_mm","x1_mm","justify","gap_mm","align_y"],"properties":{
                "items":{"type":"array","items":{"type":"array","items":{"type":"string"}},"description":"left to right; each item lists the ids or names of the layers that move together"},
                "x0_mm":{"type":"number"},
                "x1_mm":{"type":"number"},
                "justify":{"type":"string","enum":["center","start","end","space_between"]},
                "gap_mm":{"type":["number","null"],"description":"gap between neighboring items; null only with space_between"},
                "align_y":{"type":["string","null"],"enum":["center","top",null],"description":"vertical alignment to the tallest item; null keeps y"}}}}),
        json!({"type":"function","name":"fit_box","strict":true,
            "description":"Moves and resizes a shape layer so it hugs the ink boxes of the given layers plus padding — for pills, bands, badges and panels that must be exactly as large as their content. axis \"x\" changes only x and width, \"y\" only y and height, \"both\" all four. Returns the new box. Layers are given by id or exact name; when sent in the same round it runs after add_layer and arrange_row.",
            "parameters":{"type":"object","additionalProperties":false,"required":["id","around","pad_x_mm","pad_y_mm","axis"],"properties":{
                "id":{"type":"string","description":"the container shape, by id or name"},
                "around":{"type":"array","items":{"type":"string"},"description":"ids or names of the layers it must contain"},
                "pad_x_mm":{"type":"number"},
                "pad_y_mm":{"type":"number"},
                "axis":{"type":"string","enum":["both","x","y"]}}}}),
        f(
            "path_boolean",
            "Merges several layers (path / shape / text) into one path. union joins, subtract cuts the others out of the first, intersect keeps the overlap, exclude removes it. The result replaces the first layer as an evenodd path (fills, strokes and effects kept); the others are deleted. Use union for shapes that form one body, e.g. the white outline band behind a title.",
            json!({"layer_ids":{"type":"array","items":{"type":"string"},"minItems":2},"op":{"type":"string","enum":["union","subtract","intersect","exclude"]}}),
            &["layer_ids", "op"],
        ),
        f(
            "outline_text",
            "Converts a text layer into an identical path layer (width ratio, slant and arc kept). Only for reshaping glyphs or merging with shapes through path_boolean; the wording can no longer be edited afterwards, so keep text as text otherwise.",
            json!({"layer_id":{"type":"string"}}),
            &["layer_id"],
        ),
        f(
            "trace_region",
            "Traces pixels close to color inside a region of the original into a vector path layer. For calligraphy, brush lettering, logotypes and flat-colored artwork that no font can reproduce. Text that a bundled font matches must be a text layer tuned with scale_x, letter spacing and strokes instead. Returns the mask iou and a render.",
            json!({"rect_px":rect_px.clone(),"color":{"type":"string","description":"#RRGGBB"},"tolerance":{"type":["integer","null"],"description":"per-channel tolerance, default 40"},"upsample":{"type":["integer","null"],"description":"upscale factor before tracing, default 8 (reduced automatically for large regions)"},"name":{"type":"string"},"group":{"type":["string","null"],"description":"parent group name (created when missing)"},"strategy":{"type":"string","enum":["text_traced","traced_vector","native_vector"]},"strategy_reason":{"type":"string"},"version":{"type":["integer","null"]}}),
            &["rect_px", "color", "name", "strategy", "strategy_reason"],
        ),
        f(
            "crop_original",
            "Crops a region of the original into an asset. transparent_color makes that color transparent. Returns the asset name and size.",
            json!({"rect_px":rect_px.clone(),"name":{"type":"string"},"version":{"type":["integer","null"]},"transparent_color":{"type":["string","null"]},"tolerance":{"type":["integer","null"]}}),
            &["rect_px", "name"],
        ),
        f(
            "cut_out",
            "Cuts a region of the original and removes its background into a PNG asset (1–3 s). Use it before generate_image for pictures, characters and photos in the original. Returns asset, size, place_mm (the original position in artboard mm), dpi (print resolution when placed at place_mm) and a preview. Place it with add_layer(kind=image, strategy=original_crop, strategy_reason='원본에서 따서 배경 제거') at place_mm. If dpi is below the target, do not enlarge it: redraw it with generate_image(transparent=true, references=[asset]).",
            json!({"rect_px":rect_px.clone(),"name":{"type":"string"},"version":{"type":["integer","null"]}}),
            &["rect_px", "name"],
        ),
        f(
            "generate_image",
            "Creates or edits a picture with the OpenAI image model. Write the prompt in English. references may hold asset names, 'original' or 'original:x,y,w,h' (a crop of the original) to redraw from. Saves the result as an asset and returns its name, size and a preview. Make one object per call; with transparent=true the result can be placed directly as a layer. Sizes are clamped to the API limits (multiples of 16, aspect 1:3–3:1, longest edge 3840 px).",
            json!({"prompt":{"type":"string"},"name":{"type":"string"},"width_px":{"type":["integer","null"]},"height_px":{"type":["integer","null"]},"quality":{"type":["string","null"],"enum":["low","medium","high","xhigh","max",null]},"transparent":{"type":["boolean","null"]},"references":{"type":["array","null"],"items":{"type":"string"}},"mask_rects_px":{"type":["array","null"],"items":rect_px.clone(),"description":"regions of the first reference to repaint (mask)"}}),
            &["prompt", "name"],
        ),
        f(
            "extend_background",
            "Outpaints a background asset on both sides until it matches the banner's aspect, blending each overlap and keeping the whole plate as a style reference. Call it once per plate, only when the banner is wider than the plate. Returns the new asset name and size.",
            json!({"asset":{"type":"string"},"prompt":{"type":["string","null"],"description":"optional English hint about the scenery to continue"},"height_px":{"type":["integer","null"],"description":"result height in px; leave null — the default already meets the background dpi requirement (never lower than that, max 2160)"}}),
            &["asset"],
        ),
        f(
            "list_fonts",
            "Lists the available fonts: family (the value for font_family), display_name (Korean name), category (고딕 / 명조 / 둥근 / 손글씨 / 디스플레이) and weights. Pick candidates here, then compare them with font_lineup.",
            json!({}),
            &[],
        ),
        f(
            "font_lineup",
            "Renders the same text in several candidate fonts, stacked and labelled, with the original crop on top, and measures how well each family matches the crop. Give rect_px as a tight crop of ONE line of the original text (one word of 2–6 large glyphs works best). Returns the picture plus `matches` — per family a score (overlap with the original's ink, 0–1) and the scale_x and letter_spacing_em that reproduce the original's proportions — and `reliable` (false for small, multi-colored or busy crops: then judge by the picture). Call it before creating a text layer whose font must match the original.",
            json!({"text":{"type":"string","description":"the text to render, usually the original wording"},"families":{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":8,"description":"family names from list_fonts"},"weight":{"type":["integer","null"],"description":"font weight 100–900, default 700"},"rect_px":{"anyOf":[rect_px.clone(),{"type":"null"}],"description":"region of the original with this text, shown on top for comparison"},"version":{"type":["integer","null"]}}),
            &["text", "families"],
        ),
        f(
            "export_check",
            "Lists image layers whose print resolution is too low (current dpi and required dpi). The requirement is the target dpi, or half of it for strategy=background.",
            json!({"dpi":{"type":["number","null"]}}),
            &[],
        ),
        f(
            "set_memory",
            "Replaces the project memory (confirmed facts, written in Korean) as a whole.",
            json!({"text":{"type":"string"}}),
            &["text"],
        ),
        f(
            "append_memory",
            "Appends one Korean line to the project memory.",
            json!({"line":{"type":"string"}}),
            &["line"],
        ),
        f(
            "set_plan",
            "Saves the plan for a reconstruct or scratch turn. Call it once after looking at the original and before any edit (edit tools are refused until then); later rounds run on the standard model. steps are the ordered to-dos in Korean; image_quality sets the default generate_image quality (figure: people and pictures, background: the plate and extend_background).",
            json!({"steps":{"type":"array","items":{"type":"string"},"minItems":1},"image_quality":{"type":"object","properties":{"background":{"type":"string","enum":["low","medium","high"]},"figure":{"type":"string","enum":["low","medium","high"]}},"required":["background","figure"],"additionalProperties":false},"notes":{"type":["string","null"]}}),
            &["steps", "image_quality"],
        ),
        f(
            "set_fit_mode",
            "Saves how to fit the original into a banner of a different aspect. The ask_user answers '배경만 늘려 채우기' and '재배치' are saved automatically, so call this only when the answer was something else.",
            json!({"mode":{"type":"string","enum":["extend_background","relayout"],"description":"extend_background: keep positions and extend the plate; relayout: re-flow the layout for legibility"}}),
            &["mode"],
        ),
        f(
            "ask_user",
            "Asks the user and waits for the answers. Batch 1–3 questions in questions, all in Korean. options holds 2–4 choices (omit for free text only), allow_free_text permits typing, rect_px shows a region of the original next to the question (required when confirming text you read). To confirm text you read, give options [\"맞아요\"] with allow_free_text true: the user either confirms or types the right text, so never add a \"수정할게요\" choice. Pick the shape by the question: a yes/no question gets two short options and no typing, a pick-one question 2–4 options, an open question no options (typing only). The user may also choose '에이전트가 알아서'; the result then carries a note that the decision is yours.",
            json!({"questions":{"type":"array","minItems":1,"maxItems":3,"items":{"type":"object","properties":{"text":{"type":"string"},"options":{"type":["array","null"],"items":{"type":"string"}},"allow_free_text":{"type":["boolean","null"]},"rect_px":{"type":["array","null"],"items":{"type":"integer"}}},"required":["text"],"additionalProperties":false}}}),
            &["questions"],
        ),
    ]
}
