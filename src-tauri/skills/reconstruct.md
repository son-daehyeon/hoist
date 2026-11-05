## 원본에서 재구성하는 순서

첫 라운드: 원본 분석 → set_plan(단계, 실행 추론 강도, 이미지 품질) → 그 다음부터 실행. set_plan 전에는 편집 도구가 막힌다. 계획 라운드만 깊게 생각하고, 실행 라운드는 계획의 effort로 진행한다.

1. view_original로 전체를 보고, analyze_original로 팔레트와 영역(글자/도형/그림/배경)을 받는다. 글자는 직접 읽어 내용과 위치를 적는다.
2. 레이어 계획을 세워 set_plan으로 저장한다: 배경 1개, 그래픽(띠·아이콘·그림) n개, 글자 n개, 각각 strategy와 이유. effort는 기본 medium, 원본이 복잡할 때(겹친 요소가 많음, 손글씨가 여럿, 요소 30개 이상)만 high. image_quality는 배경 medium, 인물·그림은 원본 크기와 중요도에 맞게.
3. 배치 방식: 원본과 현수막 비율 차이가 5%를 넘으면 프로젝트의 fit_mode를 따른다. ask면 첫 편집 전에 ask_user로 '배경만 늘려 채우기 / 현수막 비율에 맞게 요소 재배치'를 묻고 set_fit_mode로 저장한다. extend_background면 요소는 원본 위치 그대로, relayout이면 정보 위계와 가독성을 우선해 요소 묶음을 현수막 비율에 맞게 자유롭게 다시 놓는다(줄 나눔·순서 조정 가능, 색·글꼴·문구는 그대로).
4. 배경: 클린 플레이트 생성 → 비율이 다르면 extend_background → add_layer(kind=image, strategy=background, 그룹 '배경'). 좌표를 null로 두면 아트보드 전체를 덮는다.
5. 그래픽: 단색 도형은 shape(rect·ellipse·line·polygon·star)로, 붓질·아이콘은 trace_region, 그림·캐릭터·사진은 cut_out으로 따서 image 레이어(strategy=original_crop, strategy_reason '원본에서 따서 배경 제거')로 place_mm에 놓는다. cut_out의 dpi가 목표 dpi보다 낮으면 조각을 키우지 말고 references로 generate_image(transparent=true, 긴 변 3840px)에 넣어 다시 그린 뒤 compare로 조각과 맞는지 확인한다. 그룹 '그래픽'.
6. 글자: list_fonts에 같은 글꼴이 있으면 kind=text(strategy=text)로 만들고 장평·자간·기울임·획으로 원본에 맞춘다. 붓글씨·손글씨처럼 글꼴로 못 만드는 글자만 trace_region(as=text_traced)로 한 덩어리씩 딴다. 외곽선·그림자는 strokes/effects로 재현. 그룹 '글자'.
7. 전체 compare. 유사도 0.85 미만이거나 worst_regions가 크면 해당 레이어를 고친다. 두 번 실패하면 승격.
8. export_check 후 보고. 바뀐 점·남은 차이·확인 요청을 짧게.
