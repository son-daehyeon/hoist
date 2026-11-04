use banner_core::Project;

const SKILL_RECONSTRUCT: &str = include_str!("../../skills/reconstruct.md");
const SKILL_STRATEGY: &str = include_str!("../../skills/layer-strategy.md");
const SKILL_PRINT: &str = include_str!("../../skills/print-ready.md");
const SKILL_SCRATCH: &str = include_str!("../../skills/from-scratch.md");
const SKILL_REPLACE: &str = include_str!("../../skills/replace-original.md");

pub struct PromptFacts {
    pub original: Option<OriginalFacts>,
    pub layer_count: usize,
    pub memory: String,
}

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

pub fn instructions(project: &Project, facts: &PromptFacts, kind: TurnKind) -> String {
    let mut s = String::new();
    s.push_str("당신은 Banner Studio의 도우미입니다. 사용자는 50대 비디자이너이고, 작은 현수막 이미지를 실물 크기로 출력할 수 있는 레이어 문서로 다시 만드는 일을 당신에게 맡깁니다. 모든 말은 한국어로, 짧고 쉬운 말로 합니다. 전문 용어 대신 사용자가 쓰는 말(원본, 작업본, 글자, 띠, 그림, 배경)을 씁니다.\n\n");
    s.push_str("## 절대 규칙\n");
    s.push_str(
        "1. 따로 요청이 없으면 원본과 똑같이 재현합니다. 디자인·색·배치를 마음대로 바꾸지 않습니다.\n",
    );
    s.push_str("2. 편집 도구는 결과 렌더와 원본 비교 점수를 돌려줍니다. 반드시 그 이미지를 보고 판단합니다. 보지 않고 \"됐다\"고 말하지 않습니다.\n");
    s.push_str("3. 통짜 이미지로 두지 않습니다. 글자·도형·그림·배경을 각각 레이어로 나눕니다.\n");
    s.push_str("4. 레이어마다 strategy와 strategy_reason을 채웁니다. 가드(비교 점수)에 떨어지면 다음 방식으로 승격합니다: native_vector → traced_vector → generated_raster.\n");
    s.push_str("5. 사용자가 직접 고친 속성(pinned)은 바꾸지 않습니다. 사용자가 이번 메시지에서 명시적으로 바꾸라고 했을 때만 patch에 \"pinned\": [] 를 넣어 풀고 바꿉니다.\n");
    s.push_str("6. 작업이 끝나면 compare로 전체를 확인하고, 사용자에게 바뀐 것·남은 차이·다음에 할 수 있는 것을 3~5줄로 보고합니다.\n");
    s.push_str(
        "7. 모르는 것은 추측하지 말고 ask_user로 묻습니다. 단, 사소한 것은 묻지 말고 원본대로 합니다.\n\n",
    );
    s.push_str("## 단위와 좌표\n");
    s.push_str(&format!(
        "- 문서 단위는 mm. 아트보드 {} × {} mm, 출력 기준 {} dpi. 글자 크기도 mm.\n",
        project.width_mm, project.height_mm, project.target_dpi
    ));
    if let Some(o) = &facts.original {
        s.push_str(&format!(
            "- 원본 v{}: {} × {} px. 원본 px → 작업본 mm 변환: mm = px × {:.4} + 오프셋(x {:.1} mm, y {:.1} mm). 원본과 현수막 비율이 다르면 남는 양옆은 배경을 이어 그려 채웁니다.\n",
            o.version, o.width_px, o.height_px, o.mm_per_px, o.offset_x_mm, o.offset_y_mm
        ));
    } else {
        s.push_str("- 이 프로젝트에는 원본이 없습니다. 사용자의 설명으로 시안을 만듭니다.\n");
    }
    s.push_str(&format!("- 지금 레이어 수: {}\n\n", facts.layer_count));
    if !facts.memory.trim().is_empty() {
        s.push_str("## 프로젝트 메모리 (확정된 사실, 반드시 지킬 것)\n");
        s.push_str(facts.memory.trim());
        s.push_str("\n\n");
    }
    s.push_str(SKILL_STRATEGY);
    s.push_str("\n\n");
    s.push_str(SKILL_PRINT);
    s.push_str("\n\n");
    match kind {
        TurnKind::Reconstruct => {
            s.push_str(SKILL_RECONSTRUCT);
        }
        TurnKind::Scratch => {
            s.push_str(SKILL_SCRATCH);
        }
        TurnKind::ReplaceOriginal => {
            s.push_str(SKILL_REPLACE);
        }
        TurnKind::Normal => {}
    }
    s
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnKind {
    Normal,
    Reconstruct,
    Scratch,
    ReplaceOriginal,
}

pub const GUARD_FAIL_MESSAGE: &str = "[가드] 작업본이 원본과 아직 많이 다릅니다. 아래 비교 이미지에서 차이가 큰 영역을 보고 더 고치거나, 왜 더 못 맞추는지 설명하세요. 고칠 수 있으면 고친 뒤 다시 compare를 호출하세요.";

pub const REPLACE_PROMPT: &str = "원본이 새 버전으로 바뀌었어요. 이전 원본과 새 원본을 view_original로 둘 다 본 뒤 무엇이 달라졌는지 두 줄로 말하고, 반드시 ask_user로 '바뀐 부분만 반영 / 전부 새로 만들기 / 참고 이미지만 바꾸기' 중 하나를 묻고 답에 따라 진행하세요. 묻기 전에는 어떤 편집도 하지 마세요.";
