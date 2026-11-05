## 원본에서 재구성하는 순서

1. view_original로 전체를 보고, analyze_original로 팔레트와 영역(글자/도형/그림/배경)을 받는다. 글자는 직접 읽어 내용과 위치를 적는다.
2. 레이어 계획을 세운다: 배경 1개, 그래픽(띠·아이콘·그림) n개, 글자 n개. 각각 strategy와 이유.
3. 배치 방식: 원본과 현수막 비율 차이가 5%를 넘으면 프로젝트의 fit_mode를 따른다. ask면 첫 편집 전에 ask_user로 '배경만 늘려 채우기 / 현수막 비율에 맞게 요소 재배치'를 묻고 set_fit_mode로 저장한다. extend_background면 요소는 원본 위치 그대로, relayout이면 정보 위계와 가독성을 우선해 요소 묶음을 현수막 비율에 맞게 자유롭게 다시 놓는다(줄 나눔·순서 조정 가능, 색·글꼴·문구는 그대로).
4. 배경: 클린 플레이트 생성 → 비율이 다르면 extend_background → add_layer(kind=image, strategy=background, 그룹 '배경'). 좌표를 null로 두면 아트보드 전체를 덮는다.
5. 그래픽: 단색 도형은 shape(rect·ellipse·line·polygon·star)로, 붓질·아이콘은 trace_region, 사진성 객체는 crop_original + generate_image(transparent). 그룹 '그래픽'.
6. 글자: list_fonts에 같은 글꼴이 있으면 kind=text(strategy=text)로 만들고 장평·자간·기울임·획으로 원본에 맞춘다. 붓글씨·손글씨처럼 글꼴로 못 만드는 글자만 trace_region(as=text_traced)로 한 덩어리씩 딴다. 외곽선·그림자는 strokes/effects로 재현. 그룹 '글자'.
7. 전체 compare. 유사도 0.85 미만이거나 worst_regions가 크면 해당 레이어를 고친다. 두 번 실패하면 승격.
8. export_check 후 보고. 바뀐 점·남은 차이·확인 요청을 짧게.
