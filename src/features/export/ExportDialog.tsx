import { useEffect, useState } from "react";
import { useEd } from "@/entities/editor";
import { api, type ExportResult, type ExportWarning, type LayerId } from "@/shared/api";
import { errorMessage, fileName, mmToPx, pngSrc } from "@/shared/lib";
import { Button, Checkbox, Dialog, Icon, Segmented, Select, toast } from "@/shared/ui";
import "./export.css";

const ILLUSTRATOR_MAX_MM = 5779;
const DPIS = [72, 100, 150, 200];

export function ExportDialog({
  onClose,
  onDone,
  onFix,
}: {
  onClose: () => void;
  onDone: (r: ExportResult, desktop: boolean) => void;
  onFix: (ids: LayerId[]) => void;
}) {
  const { projectId, project, doc } = useEd();
  const W = doc?.artboard.width_mm ?? 0;
  const H = doc?.artboard.height_mm ?? 0;
  const [format, setFormat] = useState<"png" | "svg">("png");
  const [scale, setScale] = useState<"1" | "10">(W > ILLUSTRATOR_MAX_MM ? "10" : "1");
  const [dpi, setDpi] = useState(project?.target_dpi ?? 100);
  const [outline, setOutline] = useState(true);
  const [warnings, setWarnings] = useState<ExportWarning[]>([]);
  const [preview, setPreview] = useState<string>();
  const [dest, setDest] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api.renderComposite(projectId, 560).then(
      (s) => setPreview(pngSrc(s)),
      () => undefined,
    );
  }, [projectId]);

  useEffect(() => {
    let alive = true;
    api
      .exportCheck(projectId, dpi)
      .then((w) => alive && setWarnings(w))
      .catch((e) => toast(errorMessage(e), "error"));
    return () => {
      alive = false;
    };
  }, [projectId, dpi]);

  const k = Number(scale);
  const name = `${(project?.name ?? "banner").replace(/\s+/g, "_")}_${k === 10 ? "1to10" : "1to1"}.${format}`;
  const rasterCount = doc?.layers.filter((l) => l.kind.type === "image").length ?? 0;

  const run = async () => {
    setBusy(true);
    try {
      const r = await api.exportProject(
        projectId,
        { format, scale: k === 10 ? 10 : 1, dpi, outline_text: outline },
        dest,
      );
      toast(dest ? `${fileName(r.path)}로 저장했어요` : "바탕화면에 저장했어요");
      onDone(r, dest === null);
    } catch (e) {
      toast(errorMessage(e), "error");
      setBusy(false);
    }
  };

  return (
    <Dialog
      title="내보내기"
      width={640}
      onClose={onClose}
      footer={
        warnings.length ? (
          <>
            <Button onClick={() => onFix(warnings.map((w) => w.layer_id))}>고치러 가기</Button>
            <Button kind="primary" icon="download" disabled={busy} onClick={run}>
              그대로 내보내기
            </Button>
          </>
        ) : (
          <>
            <Button onClick={onClose}>취소</Button>
            <Button kind="primary" icon="download" disabled={busy} onClick={run}>
              내보내기
            </Button>
          </>
        )
      }
    >
      <div className="ex-grid">
        <div>
          <div className="preview-box ex-preview">{preview && <img src={preview} alt="" />}</div>
          <div className="t-caption c-secondary ex-line">
            파일 크기 {Math.round(W / k)} × {Math.round(H / k)} mm · 1:{k}
          </div>
          <button
            type="button"
            className="ex-dest t-caption c-secondary"
            onClick={async () => {
              const p = await api.pickSavePath(name);
              if (p) setDest(p);
            }}
          >
            <Icon name="folder" size={14} />
            <span className="bs-ellipsis">{dest ?? `바탕화면 › ${name}`}</span>
          </button>
        </div>
        <div className="form-stack ex-right">
          <div className="ex-group">
            <span className="t-label-s c-secondary">형식</span>
            <Segmented
              value={format}
              onChange={setFormat}
              options={[
                { value: "png", label: "PNG" },
                { value: "svg", label: "SVG" },
              ]}
            />
          </div>
          <div className="ex-group">
            <span className="t-label-s c-secondary">축척</span>
            <Segmented
              value={scale}
              onChange={setScale}
              options={[
                { value: "1", label: "1:1 실물" },
                { value: "10", label: "1:10" },
              ]}
            />
            <span className="t-caption c-tertiary">
              {format === "svg" && W > ILLUSTRATOR_MAX_MM
                ? "일러스트레이터 대지 한계(약 5.78m)를 넘어서 1:10을 추천해요."
                : "파일에 적히는 크기만 바뀌어요. 픽셀 수는 같아서 화질은 그대로예요."}
            </span>
          </div>
          {format === "png" ? (
            <div className="ex-group">
              <Select
                size="l"
                label="해상도 · 실물 기준"
                value={String(dpi)}
                onChange={(e) => setDpi(Number(e.target.value))}
                options={DPIS.map((d) => ({
                  value: String(d),
                  label: d === 100 ? "100dpi · 권장" : `${d}dpi`,
                }))}
              />
              <span className="t-caption c-tertiary">
                {mmToPx(W, dpi)} × {mmToPx(H, dpi)} px · 파일 기준 {dpi * k}dpi
              </span>
            </div>
          ) : (
            <div className="ex-group">
              <Checkbox checked={outline} onChange={setOutline}>
                글자를 윤곽선으로 바꾸기 (출력소에 폰트가 없어도 돼요)
              </Checkbox>
              {rasterCount > 0 && (
                <span className="t-caption c-tertiary">
                  그림 레이어 {rasterCount}개는 원본 해상도 그대로 들어가요.
                </span>
              )}
            </div>
          )}
          {warnings.length > 0 && (
            <div className="ex-warn">
              <Icon name="alert" size={16} />
              <div>
                <div className="t-label-s">실물 기준 해상도가 모자란 레이어 {warnings.length}개</div>
                {warnings.map((w) => (
                  <div key={w.layer_id} className="t-caption">
                    · {w.layer_name} · {Math.round(w.effective_dpi)}dpi
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>
    </Dialog>
  );
}
