import { useState } from "react";
import { useEd } from "@/entities/editor";
import { api, errorMessage } from "@/shared/api";
import { fileName, pngSrc } from "@/shared/lib";
import { Button, Dialog, Dropzone, Icon, toast } from "@/shared/ui";

export function ReplaceOriginalDialog({ onClose }: { onClose: () => void }) {
  const { projectId, setProject } = useEd();
  const [picked, setPicked] = useState<{ path: string; preview: string } | null>(null);
  const [busy, setBusy] = useState(false);

  const load = async (path: string) => {
    try {
      const info = await api.readImageInfo(path);
      setPicked({ path, preview: pngSrc(info.preview_png) });
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };
  const choose = async () => {
    const path = await api.pickImageFile();
    if (path) await load(path);
  };
  const upload = async () => {
    if (!picked) return;
    setBusy(true);
    try {
      setProject(await api.replaceOriginal(projectId, picked.path));
      onClose();
    } catch (e) {
      toast(errorMessage(e), "error");
      setBusy(false);
    }
  };

  return (
    <Dialog
      title="원본 교체"
      width={560}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>취소</Button>
          <Button kind="primary" icon="upload" disabled={!picked || busy} onClick={upload}>
            올리기
          </Button>
        </>
      }
    >
      {picked ? (
        <div>
          <div className="preview-box">
            <img src={picked.preview} alt="" />
          </div>
          <div className="file-caption t-caption c-secondary">
            <Icon name="image" size={14} />
            <span className="bs-ellipsis">{fileName(picked.path)}</span>
            <Button kind="ghost" onClick={choose}>
              바꾸기
            </Button>
          </div>
        </div>
      ) : (
        <Dropzone onPick={load}>
          <Icon name="upload" size={24} />
          <div className="t-body-m">새 원본 이미지를 여기에 놓으세요</div>
          <Button onClick={choose}>파일 선택</Button>
        </Dropzone>
      )}
    </Dialog>
  );
}
