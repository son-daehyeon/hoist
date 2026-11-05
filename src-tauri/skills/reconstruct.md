## 원본에서 재구성하는 순서

라운드를 아끼는 순서: 1라운드에 읽기 도구(view_original·analyze_original·list_fonts)와, 원본과 현수막 비율이 다르면 ask_user를 함께 보낸다. 작은 글자(날짜·전화번호·주소·작은 숫자)를 확실히 못 읽었으면 그 확인 질문(읽은 값, allow_free_text, rect_px)도 이 첫 질문 묶음에 넣는다(묶음은 최대 3개). 배치 방식 답은 자동으로 저장되니(set_fit_mode 불필요) 2라운드에 set_plan(단계, 이미지 품질)과 첫 편집들을 함께 보낸다. 순차 도구가 먼저 실행되므로 같은 라운드의 편집은 막히지 않는다. set_plan 전에는 편집 도구가 막힌다. 계획까지는 깊게 생각하고, 실행 라운드는 실행 모델이 보통 추론으로 진행한다.

1. view_original로 전체를 보고, analyze_original로 팔레트와 영역(글자/도형/그림/배경)을 받는다. 글자는 직접 읽어 내용과 위치를 적는다.
2. 레이어 계획을 세워 set_plan으로 저장한다: 배경 1개, 그래픽(띠·아이콘·그림) n개, 글자 n개, 각각 strategy와 이유. image_quality는 배경 medium, 인물·그림은 원본 크기와 중요도에 맞게.
3. 배치 방식: 원본과 현수막 비율 차이가 5%를 넘으면 프로젝트의 fit_mode를 따른다. ask면 첫 편집 전에 ask_user로 '배경만 늘려 채우기 / 현수막 비율에 맞게 요소 재배치'를 묻는다. 답은 자동으로 저장된다. extend_background면 요소는 원본 위치 그대로, relayout이면 폭만 늘리지 말고 다시 흘려 놓는다(재흐름, 색·글꼴·문구는 그대로):
   - 원본을 행(제목 행, 프로그램 띠, 정보 행)과 행 안의 열로 보고, 늘어난 폭은 글자 크기와 줄 길이로 흡수한다. 요소 사이를 벌려 메우지 않는다.
   - 정보 칸: 줄바꿈을 줄여 한 줄에 더 많이 쓰고(원본 3줄이면 1~2줄) 글자를 키워 칸을 채운다. 정보 행 높이는 내용에 맞춰 낮추고, 남는 높이는 제목 행에 준다.
   - 제목: 단어 사이 간격은 글자 높이의 0.3~0.5배로 두고, 폭은 글자를 키워 채운다(현수막 높이의 40~55%). 띠는 제목 묶음 폭에 맞춘다.
   - 여백: 좌우 바깥 여백은 현수막 높이의 10~15%, 행 사이 여백은 균등. 가장 작은 글자는 현수막 높이의 3% 이상.
   - 자기 점검: compare 대신 view_work로 빈 공간 비율과 가장 작은 글자 높이를 본다. 단어 간격만 커졌거나 칸 안이 비어 있으면(폭만 늘어난 결과) 실패로 보고 다시 흘려 놓는다.
4. 배경: 클린 플레이트 생성 → 비율이 다르면 extend_background → add_layer(kind=image, strategy=background, 그룹 '배경'). 좌표를 null로 두면 아트보드 전체를 덮는다.
5. 그래픽: 단색 도형은 shape(rect·ellipse·line·polygon·star)로, 붓질·아이콘은 trace_region, 그림·캐릭터·사진은 cut_out으로 따서 image 레이어(strategy=original_crop, strategy_reason '원본에서 따서 배경 제거')로 place_mm에 놓는다. cut_out의 dpi가 목표 dpi보다 낮으면 조각을 키우지 말고 references로 generate_image(transparent=true, 긴 변 3840px)에 넣어 다시 그린 뒤 compare로 조각과 맞는지 확인한다. 그룹 '그래픽'.
6. 글자: list_fonts에 같은 글꼴이 있으면 kind=text(strategy=text)로 만들고 장평·자간·기울임·획으로 원본에 맞춘다. 붓글씨·손글씨처럼 글꼴로 못 만드는 글자만 trace_region(as=text_traced)로 한 덩어리씩 딴다. 외곽선·그림자는 strokes/effects로 재현. 그룹 '글자'.
7. 전체 compare. 유사도 0.85 미만이거나 worst_regions가 크면 해당 레이어를 고친다. 두 번 실패하면 승격. 재배치면 3단계의 자기 점검이 기준이다.
8. export_check 후 보고. 바뀐 점·남은 차이·확인 요청을 짧게.
