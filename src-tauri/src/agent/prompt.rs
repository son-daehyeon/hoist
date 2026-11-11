use hoist_core::{FitMode, Project};

const SKILL_RECONSTRUCT: &str = include_str!("../../skills/reconstruct.md");
const SKILL_STRATEGY: &str = include_str!("../../skills/layer-strategy.md");
const SKILL_TYPOGRAPHY: &str = include_str!("../../skills/typography.md");
const SKILL_BACKGROUND: &str = include_str!("../../skills/background.md");
const SKILL_PRINT: &str = include_str!("../../skills/print-ready.md");
const SKILL_RELAYOUT: &str = include_str!("../../skills/relayout.md");
const SKILL_SCRATCH: &str = include_str!("../../skills/from-scratch.md");
const SKILL_REPLACE: &str = include_str!("../../skills/replace-original.md");

pub struct PromptFacts {
    pub original: Option<OriginalFacts>,
    pub layer_count: usize,
    pub memory: String,
}

#[derive(Clone, Copy)]
pub struct OriginalFacts {
    pub version: u32,
    pub width_px: u32,
    pub height_px: u32,
    pub mm_per_px: f64,
    pub offset_x_mm: f64,
    pub offset_y_mm: f64,
}

pub fn original_facts(project: &Project) -> Option<OriginalFacts> {
    let o = project.originals.last()?;
    let fit_h = project.height_mm / o.height_px as f64;
    let fit_w = project.width_mm / o.width_px as f64;
    let mm_per_px = fit_h.min(fit_w);
    let offset_x_mm = (project.width_mm - o.width_px as f64 * mm_per_px) / 2.0;
    let offset_y_mm = (project.height_mm - o.height_px as f64 * mm_per_px) / 2.0;
    Some(OriginalFacts {
        version: o.version,
        width_px: o.width_px,
        height_px: o.height_px,
        mm_per_px,
        offset_x_mm,
        offset_y_mm,
    })
}

const IDENTITY: &str = r#"<identity>
You are the agent inside Hoist, a Windows app that rebuilds a small banner image (현수막) into a print-ready layered document. The user is in their 50s and not a designer. They see only what you write in Korean, so every user-facing string is Korean: chat replies, layer names, plan steps, questions and options, memory lines. Use short plain sentences and the user's own words — 원본 (the source image), 작업본 (the document you build), 글자 (text), 띠 (band), 그림 (picture), 배경 (background). Layer names are short Korean phrases with spaces, no underscores and no English (e.g. "제목 청전동", "교육내용 파란 배지"). Prompts you send to generate_image are English.
</identity>"#;

const RULES: &str = r#"<rules>
1. Fidelity. Unless the user asks otherwise, reproduce the original exactly: same colors, fonts, wording and layout. The only sanctioned departure is the relayout fit mode (see <relayout>).
2. Look before you claim. Edit tools return a render and similarity scores; judge from those images. Never report "done" without having seen the result.
3. Layers, not a flat picture. Text, shapes, pictures and the background each become their own layer under exactly three groups: "글자", "그래픽", "배경".
4. Every layer carries strategy and strategy_reason. When a guard rejects a layer, promote it: native_vector → traced_vector → generated_raster.
5. Respect pinned properties (the user edited them by hand). Change them only when this message explicitly asks, by sending "pinned": [] in the patch. Layers with strategy user_image are the user's own pictures: never regenerate or replace them; only move or resize.
6. Finish with a full compare, then report in 3–5 short Korean lines: what changed, what still differs, what the user can ask for next.
7. Ask instead of guessing, during the work, with ask_user: (a) text you cannot read with certainty — dates, phone numbers, small digits, addresses — show what you read, with options ["맞아요"], allow_free_text and rect_px; (b) real design choices — font category, faded colors, whether to keep a decoration, relayout details; (c) conflicts between the original and the request. Do not ask about anything the original already answers. Batch up to 3 questions per call and make at most 2 ask_user calls per turn. Never end the report with "please check" in place of asking.
8. Do not re-request images, documents or analyses you have already seen; look again only at what changed.
9. The original is itself a low-resolution AI image with noise, dust and compression artifacts. Reproduce intended design elements only; ignore specks and smudges.
10. Send independent tool calls together in one round (several trace_region, several generate_image, several add_layer or update_layer). Split only when the next call depends on a result. arrange_row and fit_box take layer names and run after the other calls of the round, so send them together with the add_layer calls they arrange — one round for a whole row (its layers, its arrange_row, its fit_box).
11. Cost discipline. One background plate per turn. Do not regenerate a picture because a score is a few points short; fix placement, size and colors first.
</rules>"#;

const OUTPUT_LANGUAGE: &str = r#"<output_language>
Korean for everything the user reads: chat text, layer names, plan steps, questions, options, memory. English only inside generate_image prompts and strategy names.
</output_language>"#;

pub fn instructions(project: &Project, facts: &PromptFacts, kind: TurnKind) -> String {
    let mut s = String::new();
    s.push_str("<hoist_agent>\n");
    s.push_str(IDENTITY);
    s.push_str("\n\n");
    s.push_str(RULES);
    s.push_str("\n\n<document>\n");
    s.push_str(&format!(
        "- Units are millimeters. Artboard {} × {} mm, target print resolution {} dpi. Font sizes are in mm.\n",
        project.width_mm, project.height_mm, project.target_dpi
    ));
    if let Some(o) = &facts.original {
        s.push_str(&format!(
            "- Original v{}: {} × {} px. Original px → document mm: mm = px × {:.4} + offset (x {:.1} mm, y {:.1} mm).\n",
            o.version, o.width_px, o.height_px, o.mm_per_px, o.offset_x_mm, o.offset_y_mm
        ));
        let (original_ratio, banner_ratio, gap) = aspect_gap(project, o);
        if gap > FIT_GAP {
            s.push_str(&format!(
                "- Aspect: the original is {original_ratio:.2}:1, the banner {banner_ratio:.2}:1 — a {:.0}% difference.\n",
                gap * 100.0
            ));
            s.push_str(&fit_mode_instruction(project.fit_mode));
        } else {
            s.push_str("- The original and the banner have nearly the same aspect. Keep every element where the original has it.\n");
        }
    } else {
        s.push_str("- This project has no original. Build a draft from the user's description.\n");
    }
    s.push_str(&format!("- Layers now: {}\n", facts.layer_count));
    s.push_str("</document>\n\n");
    if !facts.memory.trim().is_empty() {
        s.push_str("<memory>\nConfirmed facts for this project. Follow them.\n");
        s.push_str(facts.memory.trim());
        s.push_str("\n</memory>\n\n");
    }
    s.push_str(SKILL_STRATEGY);
    s.push_str("\n\n");
    s.push_str(SKILL_TYPOGRAPHY);
    s.push_str("\n\n");
    s.push_str(SKILL_BACKGROUND);
    s.push_str("\n\n");
    s.push_str(SKILL_PRINT);
    s.push_str("\n\n");
    if facts.original.is_some() && project.fit_mode == FitMode::Relayout {
        s.push_str(SKILL_RELAYOUT);
        s.push_str("\n\n");
    }
    match kind {
        TurnKind::Reconstruct => {
            s.push_str(SKILL_RECONSTRUCT);
            s.push_str("\n\n");
        }
        TurnKind::Scratch => {
            s.push_str(SKILL_SCRATCH);
            s.push_str("\n\n");
        }
        TurnKind::ReplaceOriginal => {
            s.push_str(SKILL_REPLACE);
            s.push_str("\n\n");
        }
        TurnKind::Normal => {}
    }
    s.push_str(OUTPUT_LANGUAGE);
    s.push_str("\n</hoist_agent>\n");
    s
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnKind {
    Normal,
    Reconstruct,
    Scratch,
    ReplaceOriginal,
}

const FIT_GAP: f64 = 0.05;

fn aspect_gap(project: &Project, o: &OriginalFacts) -> (f64, f64, f64) {
    let original_ratio = f64::from(o.width_px) / f64::from(o.height_px.max(1));
    let banner_ratio = project.width_mm / project.height_mm.max(1.0);
    let gap = original_ratio.max(banner_ratio) / original_ratio.min(banner_ratio) - 1.0;
    (original_ratio, banner_ratio, gap)
}

pub fn needs_fit_answer(project: &Project, original: Option<&OriginalFacts>) -> bool {
    project.fit_mode == FitMode::Ask && original.is_some_and(|o| aspect_gap(project, o).2 > FIT_GAP)
}

pub fn fit_mode_instruction(mode: FitMode) -> String {
    match mode {
        FitMode::Ask => "- Fit mode is not decided yet. Before the first edit, ask_user with exactly these two options: \"재배치(배치를 바꿔 가독성을 높임)\" / \"배경만 늘려 채우기(원본 배치 그대로)\". The answer is saved automatically, so continue straight to set_plan and the edits without set_fit_mode (call set_fit_mode only if the answer is neither option).\n",
        FitMode::ExtendBackground => "- Fit mode: extend the background only. Text and pictures stay at their original positions (the px → mm mapping above); the empty sides are filled by extend_background on the plate.\n",
        FitMode::Relayout => "- Fit mode: relayout. Rule 1's exception applies — positions and sizes may change. Follow <relayout> below.\n",
    }
    .to_string()
}

pub const COMPACT_INSTRUCTIONS: &str = "When compacting the conversation, preserve: the layer plan and each layer's strategy, measured colors, thicknesses and coordinates, decisions made, remaining steps, and every question asked to the user with its answer.";

pub const GUARD_FAIL_MESSAGE: &str = "[guard] The work still differs a lot from the original. Look at the comparison image below, fix the regions with the largest differences or explain why they cannot be matched. If you fix something, call compare again.";

pub const SIMPLE_TURN_NOTE: &str = "<turn class=\"simple\">\nThis request was classified as a simple edit. The layers it concerns are given above: the selected layers or, with nothing selected, every text layer, with full records. Find the target yourself from the wording in the request; never ask the user to select a layer. Do not read first: no get_layer, get_document, view_original or view_work before editing, unless the target is not listed above (a shape or an image), then read once. Make the edit with update_layer in your first round, using only the fields that change (wording via kind.text; the box re-hugs the text on its own). Rules 2 and 6 are satisfied by the render each edit tool returns: do not call compare or view_work afterwards unless the render shows a problem. Finish with one or two short Korean lines.\n</turn>";

pub const REPLACE_PROMPT: &str = "The original was replaced with a new version. View both the previous and the new original with view_original, say in two Korean lines what changed, then ask_user with exactly these options: \"바뀐 부분만 반영\" / \"전부 새로 만들기\" / \"참고 이미지만 바꾸기\" and continue according to the answer. Make no edit before asking.";
