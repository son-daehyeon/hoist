## 레이어 처리 방식 (strategy)

| strategy         | 언제                                   | 어떻게                                                                                                                                                                                        |
| ---------------- | -------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| native_vector    | 띠·알약·배지·구분선처럼 단색 기본 도형 | kind=shape(rect/ellipse/line) 또는 단순 path. 색은 measure로 원본에서 잰다                                                                                                                    |
| text_traced      | 원본에 이미 있는 모든 글자             | trace_region으로 글자 채움색만 따서 path. 외곽선·그림자는 strokes/effects로 수치로 넣는다 (트레이싱하지 않는다)                                                                               |
| text             | 새로 쓰는 문구, 내용이 바뀌는 글자     | kind=text + list_fonts에서 가장 비슷한 글꼴. 원본 글자가 있으면 text_traced가 우선                                                                                                            |
| traced_vector    | 붓질·불규칙한 윤곽, 아이콘             | trace_region(upsample 4) 후 결과 iou가 0.9 미만이면 generated_raster로 승격                                                                                                                   |
| generated_raster | 사진·그림·질감, 벡터로 안 되는 붓질    | crop_original로 조각을 잘라 generate_image(references=[조각], transparent=true)로 고해상도 재생성. 한 객체씩, 통째로 하지 않는다                                                              |
| background       | 배경                                   | 글자·도형·객체가 모두 빠진 클린 플레이트: generate_image(references=[원본], prompt는 '글자와 장식 요소를 모두 지우고 배경만, 구도·색 유지') → extend_background로 현수막 비율까지 이어 그린다 |

규칙

- 배경 위에 원본에 있던 띠·글자가 남아 있으면 겹쳐 보이므로 반드시 지운 배경(클린 플레이트)을 쓴다.
- 그림자·입체는 effects(drop_shadow 여러 겹)로, 외곽선은 strokes(inside/center/outside)로. 두께·색은 measure로 잰다.
- 객체는 각각 다른 레이어. 그룹은 '글자', '그래픽', '배경' 세 개로 묶는다.
- 생성 이미지는 긴 변 최대 3840px, 비율 1:3~3:1. 현수막 전체 비율이 그보다 넓으면 extend_background를 쓴다.
