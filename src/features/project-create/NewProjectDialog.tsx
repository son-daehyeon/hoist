import { useState } from "react";
import { api, errorMessage, type Project } from "@/shared/api";
import { dpiLabel, fileName, pngSrc, ratioLabel } from "@/shared/lib";
import { Button, Dialog, Dropzone, Field, Icon, Tabs, toast } from "@/shared/ui";
import "./project-create.css";

type Picked = { path: string; width_px: number; height_px: number; preview: string };

const ratioDiffers = (img: Picked, w: number, h: number) =>
  Math.abs(img.width_px / img.height_px - w / h) / (w / h) > 0.05;

export function NewProjectDialog({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: (project: Project, withOriginal: boolean) => void;
}) {
  const [mode, setMode] = useState<"upload" | "blank">("upload");
  const [img, setImg] = useState<Picked | null>(null);
  const [name, setName] = useState("");
  const [w, setW] = useState("");
  const [h, setH] = useState("");
  const [busy, setBusy] = useState(false);

  const wn = Number(w);
  const hn = Number(h);
  const sizeOk = wn > 0 && hn > 0 && wn <= 100_000 && hn <= 100_000;
  const valid = name.trim() !== "" && sizeOk && (mode === "blank" || img !== null);
  const differs = mode === "upload" && img !== null && sizeOk && ratioDiffers(img, wn, hn);

  const load = async (path: string) => {
    try {
      const info = await api.readImageInfo(path);
      setImg({ path, width_px: info.width_px, height_px: info.height_px, preview: pngSrc(info.preview_png) });
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };
  const choose = async () => {
    const path = await api.pickImageFile();
    if (path) await load(path);
  };

  const create = async () => {
    setBusy(true);
    try {
      const withOriginal = mode === "upload" && img !== null;
      const project = await api.createProject({
        name: name.trim(),
        width_mm: wn,
        height_mm: hn,
        original_path: withOriginal ? img?.path : null,
        fit_mode: null,
      });
      onCreated(project, withOriginal);
    } catch (e) {
      toast(errorMessage(e), "error");
      setBusy(false);
    }
  };

  const previewW = sizeOk ? Math.min(300, 60 * (wn / hn)) : 0;
  const previewH = sizeOk ? previewW / (wn / hn) : 0;

  return (
    <Dialog
      title="새 프로젝트"
      width={720}
      onClose={onClose}
      tabs={
        <Tabs
          value={mode}
          onChange={setMode}
          options={[
            { value: "upload", label: "원본 이미지로 시작" },
            { value: "blank", label: "빈 현수막으로 시작" },
          ]}
        />
      }
      footer={
        <>
          <Button onClick={onClose}>취소</Button>
          <Button
            kind="primary"
            icon={mode === "upload" ? "sparkles" : undefined}
            disabled={!valid || busy}
            onClick={create}
          >
            {mode === "upload" ? "만들고 분석 시작" : "만들기"}
          </Button>
        </>
      }
    >
      <div key={mode} className={`np-grid ${mode === "blank" ? "is-blank" : ""}`}>
        {mode === "upload" && (
          <div className="np-left">
            {img ? (
              <>
                <div className="preview-box">
                  <img src={img.preview} alt="" />
                </div>
                <div className="file-caption t-caption c-secondary">
                  <Icon name="image" size={14} />
                  <span className="bs-ellipsis">
                    {fileName(img.path)} · {img.width_px} × {img.height_px} px
                  </span>
                  <Button kind="ghost" onClick={choose}>
                    바꾸기
                  </Button>
                </div>
              </>
            ) : (
              <Dropzone onPick={load}>
                <Icon name="upload" size={24} />
                <div className="t-body-m">현수막 이미지를 여기에 놓으세요</div>
                <Button onClick={choose}>파일 선택</Button>
                <div className="t-caption c-tertiary">PNG · JPG, 카카오톡으로 받은 그림도 돼요</div>
              </Dropzone>
            )}
          </div>
        )}
        <div className="form-stack np-right">
          <Field
            size="l"
            label="이름"
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="예: 가을 축제 현수막"
          />
          <div className="np-size">
            <Field
              size="l"
              label="실물 가로"
              suffix="mm"
              inputMode="numeric"
              value={w}
              onChange={(e) => setW(e.target.value.replace(/[^\d.]/g, ""))}
            />
            <Field
              size="l"
              label="실물 세로"
              suffix="mm"
              inputMode="numeric"
              value={h}
              onChange={(e) => setH(e.target.value.replace(/[^\d.]/g, ""))}
            />
          </div>
          <div className="np-preview">
            <div className="np-preview-head">
              <span className="t-label-s c-secondary">미리보기</span>
              <span className="t-caption c-tertiary">{sizeOk ? ratioLabel(wn, hn) : ""}</span>
            </div>
            <div className="np-preview-stage">
              {sizeOk && <div className="np-preview-rect" style={{ width: previewW, height: previewH }} />}
            </div>
            <div className="t-caption c-tertiary">
              {sizeOk ? dpiLabel(wn, hn, 100) : "실물 크기를 mm로 적어 주세요"}
            </div>
          </div>
          {differs ? (
            <div className="t-caption c-secondary">
              원본 {ratioLabel(img.width_px, img.height_px)}, 현수막 {ratioLabel(wn, hn)} — 비율이 달라요
            </div>
          ) : (
            mode === "upload" &&
            img &&
            sizeOk && <div className="t-caption c-secondary">원본과 현수막 비율이 같아요.</div>
          )}
        </div>
      </div>
    </Dialog>
  );
}
