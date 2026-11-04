use anyhow::anyhow;
use hoist_core::model::{Layer, Strategy};
use serde_json::{Value, json};

fn obj(props: Value) -> Value {
    let required: Vec<String> = props
        .as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    json!({"type": "object", "properties": props, "required": required, "additionalProperties": false})
}

fn tag(value: &str) -> Value {
    json!({"type": "string", "enum": [value]})
}

fn nullable(t: &str) -> Value {
    json!({"type": [t, "null"]})
}

fn paint() -> Value {
    json!({"anyOf": [
        obj(json!({"type": tag("solid"), "color": {"type": "string", "description": "#RRGGBB"}, "opacity": {"type": "number"}})),
        obj(json!({"type": tag("linear_gradient"), "angle_deg": {"type": "number"}, "stops": {"type": "array", "items": obj(json!({"offset": {"type": "number"}, "color": {"type": "string"}, "opacity": {"type": "number"}}))}})),
    ]})
}

fn kind() -> Value {
    json!({"anyOf": [
        obj(json!({"type": tag("group")})),
        obj(json!({"type": tag("text"), "text": {"type": "string"}, "font_family": {"type": "string"}, "font_weight": {"type": "integer"}, "font_size_mm": {"type": "number"}, "letter_spacing_mm": {"type": "number"}, "line_height": {"type": "number"}, "align": {"type": "string", "enum": ["left", "center", "right"]}})),
        obj(json!({"type": tag("path"), "d": {"type": "string"}, "fill_rule": {"type": "string", "enum": ["nonzero", "evenodd"]}})),
        obj(json!({"type": tag("shape"), "shape": {"type": "string", "enum": ["rect", "ellipse", "line"]}, "rx_mm": {"type": "number", "description": "rect 모서리 반경, 다른 shape는 0"}})),
        obj(json!({"type": tag("image"), "asset": {"type": "string"}, "natural_w_px": {"type": "integer"}, "natural_h_px": {"type": "integer"}, "fit": {"type": "string", "enum": ["fill", "contain", "cover"]}})),
    ]})
}

fn nullable_kind() -> Value {
    let mut branches = kind()["anyOf"].as_array().cloned().unwrap_or_default();
    branches.push(json!({"type": "null"}));
    json!({"anyOf": branches, "description": "바꿀 때는 kind 전체를 보냄"})
}

fn stroke() -> Value {
    obj(json!({
        "paint": paint(),
        "width_mm": {"type": "number"},
        "align": {"type": "string", "enum": ["inside", "center", "outside"]},
        "join": {"type": "string", "enum": ["miter", "round", "bevel"]},
        "dash_mm": {"type": "array", "items": {"type": "number"}},
    }))
}

fn effect() -> Value {
    json!({"anyOf": [
        obj(json!({"type": tag("drop_shadow"), "x_mm": {"type": "number"}, "y_mm": {"type": "number"}, "blur_mm": {"type": "number"}, "spread_mm": {"type": "number"}, "color": {"type": "string"}, "opacity": {"type": "number"}})),
        obj(json!({"type": tag("layer_blur"), "radius_mm": {"type": "number"}})),
    ]})
}

const STRATEGIES: [&str; 7] = [
    "native_vector",
    "text_traced",
    "text",
    "traced_vector",
    "generated_raster",
    "background",
    "original_crop",
];

fn layer() -> Value {
    obj(json!({
        "id": {"type": ["string", "null"], "description": "비우면 새 id"},
        "name": {"type": "string"},
        "parent": {"type": ["string", "null"], "description": "부모 그룹 id 또는 이름 (없으면 만듦)"},
        "kind": kind(),
        "x_mm": {"type": ["number", "null"], "description": "image·group은 null이면 아트보드 전체"},
        "y_mm": nullable("number"),
        "w_mm": nullable("number"),
        "h_mm": nullable("number"),
        "rotation_deg": nullable("number"),
        "opacity": nullable("number"),
        "visible": nullable("boolean"),
        "locked": nullable("boolean"),
        "fills": {"type": "array", "items": paint()},
        "strokes": {"type": "array", "items": stroke()},
        "effects": {"type": "array", "items": effect()},
        "strategy": {"type": ["string", "null"], "enum": [STRATEGIES[0], STRATEGIES[1], STRATEGIES[2], STRATEGIES[3], STRATEGIES[4], STRATEGIES[5], STRATEGIES[6], null]},
        "strategy_reason": {"type": "string"},
        "source": {"anyOf": [
            obj(json!({"original_version": nullable("integer"), "crop_px": {"type": ["array", "null"], "items": {"type": "integer"}}, "asset": nullable("string"), "prompt": nullable("string")})),
            {"type": "null"},
        ]},
    }))
}

fn patch() -> Value {
    obj(json!({
        "name": nullable("string"),
        "kind": nullable_kind(),
        "x_mm": nullable("number"),
        "y_mm": nullable("number"),
        "w_mm": nullable("number"),
        "h_mm": nullable("number"),
        "rotation_deg": nullable("number"),
        "opacity": nullable("number"),
        "visible": nullable("boolean"),
        "locked": nullable("boolean"),
        "fills": {"type": ["array", "null"], "items": paint()},
        "strokes": {"type": ["array", "null"], "items": stroke()},
        "effects": {"type": ["array", "null"], "items": effect()},
        "strategy": {"type": ["string", "null"], "enum": [STRATEGIES[0], STRATEGIES[1], STRATEGIES[2], STRATEGIES[3], STRATEGIES[4], STRATEGIES[5], STRATEGIES[6], null]},
        "strategy_reason": nullable("string"),
        "pinned": {"type": ["array", "null"], "items": {"type": "string"}, "description": "사용자가 이번에 바꾸라고 한 속성만 []로 풀기"},
    }))
}

const ADD_EXAMPLES: &str = r##"예시 1 (배경 그림): {"layer":{"id":null,"name":"배경","parent":"배경","kind":{"type":"image","asset":"background_1.png","natural_w_px":3840,"natural_h_px":576,"fit":"fill"},"x_mm":null,"y_mm":null,"w_mm":null,"h_mm":null,"rotation_deg":null,"opacity":null,"visible":null,"locked":null,"fills":[],"strokes":[],"effects":[],"strategy":"background","strategy_reason":"원본 배경을 현수막 비율로 이어 그림","source":null},"index":0}
예시 2 (따 낸 글자): {"layer":{"id":null,"name":"제목","parent":"글자","kind":{"type":"path","d":"M 0 0 L 120 0 L 120 40 L 0 40 Z","fill_rule":"nonzero"},"x_mm":1200,"y_mm":150,"w_mm":3600,"h_mm":320,"rotation_deg":null,"opacity":null,"visible":null,"locked":null,"fills":[{"type":"solid","color":"#1C6B33","opacity":1}],"strokes":[],"effects":[],"strategy":"text_traced","strategy_reason":"붓글씨라 글꼴로 재현할 수 없음","source":null},"index":null}"##;

pub fn add_layer_tool(strict: bool) -> Value {
    let description = format!(
        "레이어를 추가합니다. x·y·w·h는 mm. image·group 레이어는 좌표를 null로 두면 아트보드 전체를 덮습니다. parent에는 그룹 id나 이름을 넣으면 그 그룹 안에 넣습니다(없으면 만듦). 결과로 해당 영역 렌더와 원본 비교 점수를 돌려줍니다.\n{ADD_EXAMPLES}"
    );
    tool(
        "add_layer",
        &description,
        obj(
            json!({"layer": layer(), "index": {"type": ["integer", "null"], "description": "0이 맨 아래, null이면 맨 위"}}),
        ),
        strict,
    )
}

pub fn update_layer_tool(strict: bool) -> Value {
    tool(
        "update_layer",
        "레이어 속성을 바꿉니다. patch에서 바꿀 필드만 값을 넣고 나머지는 null. 결과로 렌더와 비교 점수를 돌려줍니다. 예: {\"id\":\"L3\",\"patch\":{\"name\":null,\"kind\":null,\"x_mm\":1210,\"y_mm\":null,\"w_mm\":null,\"h_mm\":null,\"rotation_deg\":null,\"opacity\":null,\"visible\":null,\"locked\":null,\"fills\":[{\"type\":\"solid\",\"color\":\"#F2511B\",\"opacity\":1}],\"strokes\":null,\"effects\":null,\"strategy\":null,\"strategy_reason\":null,\"pinned\":null}}",
        obj(json!({"id": {"type": "string"}, "patch": patch()})),
        strict,
    )
}

fn tool(name: &str, description: &str, parameters: Value, strict: bool) -> Value {
    let mut t =
        json!({"type": "function", "name": name, "description": description, "parameters": parameters});
    if strict {
        t["strict"] = json!(true);
    }
    t
}

fn drop_nulls(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.retain(|_, x| !x.is_null());
            m.values_mut().for_each(drop_nulls);
        }
        Value::Array(a) => a.iter_mut().for_each(drop_nulls),
        _ => {}
    }
}

pub fn patch_from_args(raw: &Value) -> Value {
    let mut patch = raw.clone();
    if let Some(fields) = patch.as_object_mut() {
        fields.retain(|_, x| !x.is_null());
    }
    patch
}

fn guess_strategy(kind: &str, full_bleed: bool) -> Strategy {
    match kind {
        "image" if full_bleed => Strategy::Background,
        "image" => Strategy::GeneratedRaster,
        "path" => Strategy::TracedVector,
        _ => Strategy::NativeVector,
    }
}

pub fn normalize_layer(raw: &Value, artboard_mm: (f64, f64)) -> Value {
    let mut v = raw.clone();
    drop_nulls(&mut v);
    let Some(m) = v.as_object_mut() else {
        return v;
    };
    if let Some(Value::String(t)) = m.get("kind").cloned() {
        m.insert("kind".into(), json!({"type": t}));
    }
    if let Some(Value::String(group)) = m.remove("group") {
        m.entry("parent").or_insert(json!(group));
    }
    let kind = m
        .get("kind")
        .and_then(|k| k["type"].as_str())
        .unwrap_or("")
        .to_string();
    if kind == "shape"
        && let Some(k) = m.get_mut("kind").and_then(Value::as_object_mut)
        && k.get("shape").and_then(Value::as_str) != Some("rect")
    {
        k.remove("rx_mm");
    }
    m.entry("id").or_insert(json!(""));
    m.entry("name").or_insert(json!("레이어"));
    let background = m.get("strategy").and_then(Value::as_str) == Some("background");
    if kind == "image" || kind == "group" || background {
        let (w, h) = artboard_mm;
        for (key, value) in [("x_mm", 0.0), ("y_mm", 0.0), ("w_mm", w), ("h_mm", h)] {
            m.entry(key).or_insert(json!(value));
        }
    }
    if !m.contains_key("strategy") {
        let full_bleed = (["x_mm", "y_mm"]
            .iter()
            .all(|k| m.get(*k).and_then(Value::as_f64) == Some(0.0)))
            && m.get("w_mm").and_then(Value::as_f64) == Some(artboard_mm.0)
            && m.get("h_mm").and_then(Value::as_f64) == Some(artboard_mm.1);
        m.insert("strategy".into(), json!(guess_strategy(&kind, full_bleed)));
    }
    v
}

fn minimal_shape(kind: &str) -> &'static str {
    match kind {
        "text" => {
            r##"{"name":"부제","kind":{"type":"text","text":"건강한 일상","font_family":"Pretendard","font_size_mm":120},"x_mm":800,"y_mm":420,"w_mm":2400,"h_mm":160,"strategy":"text","strategy_reason":"…"}"##
        }
        "path" => {
            r##"{"name":"제목","kind":{"type":"path","d":"M 0 0 L 10 0 L 10 10 Z"},"x_mm":1200,"y_mm":150,"w_mm":3600,"h_mm":320,"fills":[{"type":"solid","color":"#1C6B33","opacity":1}],"strategy":"text_traced","strategy_reason":"…"}"##
        }
        "shape" => {
            r##"{"name":"띠","kind":{"type":"shape","shape":"rect","rx_mm":40},"x_mm":300,"y_mm":380,"w_mm":5400,"h_mm":220,"fills":[{"type":"solid","color":"#8E2EA3","opacity":1}],"strategy":"native_vector","strategy_reason":"…"}"##
        }
        "image" => {
            r##"{"name":"배경","kind":{"type":"image","asset":"background_1.png","natural_w_px":3840,"natural_h_px":576},"strategy":"background","strategy_reason":"…"}"##
        }
        "group" => r##"{"name":"글자","kind":{"type":"group"}}"##,
        _ => {
            r##"{"name":"…","kind":{"type":"group|text|path|shape|image", …},"x_mm":0,"y_mm":0,"w_mm":100,"h_mm":100,"strategy":"native_vector","strategy_reason":"…"}"##
        }
    }
}

pub fn parse_layer(raw: &Value, artboard_mm: (f64, f64)) -> anyhow::Result<Layer> {
    let normalized = normalize_layer(raw, artboard_mm);
    serde_json::from_value::<Layer>(normalized).map_err(|e| {
        let kind = raw["kind"]["type"]
            .as_str()
            .or_else(|| raw["kind"].as_str())
            .unwrap_or("");
        anyhow!(
            "layer JSON 오류: {e}. 최소 모양(이대로 고쳐 다시 호출): {}",
            minimal_shape(kind)
        )
    })
}

pub fn strip_strict(tools: &mut [Value]) {
    for t in tools.iter_mut() {
        if let Some(m) = t.as_object_mut() {
            m.remove("strict");
        }
    }
}

#[cfg(test)]
mod tests {
    use hoist_core::model::LayerKind;

    use super::*;

    const ARTBOARD: (f64, f64) = (6000.0, 900.0);

    #[test]
    fn group_given_as_a_string_kind_parses() {
        let layer = parse_layer(&json!({"name": "배경", "kind": "group"}), ARTBOARD).expect("lenient group");
        assert_eq!(layer.kind, LayerKind::Group);
        assert_eq!(
            [layer.x_mm, layer.y_mm, layer.w_mm, layer.h_mm],
            [0.0, 0.0, 6000.0, 900.0]
        );
        assert_eq!(layer.strategy, Strategy::NativeVector);
    }

    #[test]
    fn background_image_without_coordinates_fills_the_artboard() {
        let raw = json!({"name": "배경", "group": "배경", "kind": {"type": "image", "asset": "bg.png", "natural_w_px": 3840, "natural_h_px": 576}, "x_mm": null});
        let layer = parse_layer(&raw, ARTBOARD).expect("lenient image");
        assert_eq!(
            [layer.x_mm, layer.y_mm, layer.w_mm, layer.h_mm],
            [0.0, 0.0, 6000.0, 900.0]
        );
        assert_eq!(layer.parent.as_deref(), Some("배경"));
        assert_eq!(layer.strategy, Strategy::Background);
    }

    #[test]
    fn text_without_coordinates_explains_the_expected_shape() {
        let raw = json!({"name": "부제", "kind": {"type": "text", "text": "건강한 일상", "font_family": "Pretendard", "font_size_mm": 120}});
        let err = parse_layer(&raw, ARTBOARD)
            .expect_err("text needs coordinates")
            .to_string();
        assert!(err.contains("missing field `x_mm`"), "{err}");
        assert!(err.contains("최소 모양"), "{err}");
        assert!(err.contains(r#""x_mm":800"#), "{err}");
    }

    fn assert_strict(schema: &Value, path: &str) {
        if let Some(branches) = schema["anyOf"].as_array() {
            assert!(
                branches.iter().all(|b| b.get("anyOf").is_none()),
                "{path}: nested anyOf"
            );
            for (i, b) in branches.iter().enumerate() {
                assert_strict(b, &format!("{path}/anyOf{i}"));
            }
        }
        let is_object = schema["type"] == "object"
            || schema["type"]
                .as_array()
                .is_some_and(|t| t.contains(&json!("object")));
        if is_object {
            assert_eq!(schema["additionalProperties"], json!(false), "{path}");
            let props: Vec<&String> = schema["properties"]
                .as_object()
                .map(|m| m.keys().collect())
                .unwrap_or_default();
            let required: Vec<&str> = schema["required"]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            for p in props {
                assert!(required.contains(&p.as_str()), "{path}: {p} not required");
                assert_strict(&schema["properties"][p], &format!("{path}/{p}"));
            }
        }
        if let Some(items) = schema.get("items") {
            assert_strict(items, &format!("{path}[]"));
        }
    }

    #[test]
    fn strict_schemas_follow_the_rules() {
        assert_strict(&add_layer_tool(true)["parameters"], "add_layer");
        assert_strict(&update_layer_tool(true)["parameters"], "update_layer");
        assert_eq!(add_layer_tool(true)["strict"], json!(true));
        assert!(add_layer_tool(false).get("strict").is_none());
    }

    #[test]
    fn examples_in_the_description_parse() {
        for line in ADD_EXAMPLES.lines() {
            let example: Value =
                serde_json::from_str(line.split_once(": ").expect("example").1).expect("json");
            parse_layer(&example["layer"], ARTBOARD).expect("example parses");
        }
    }
}
