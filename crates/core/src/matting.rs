use std::path::Path;

use image::imageops::{self, FilterType};
use image::{GrayImage, Luma, RgbaImage};
use ort::value::Tensor;

use crate::{CoreError, Result, onnx};

const MODEL_FILE: &str = "isnet-general-use.onnx";
const INPUT_SIDE_PX: u32 = 1024;

pub fn available(models_dir: &Path) -> bool {
    onnx::has(models_dir, MODEL_FILE)
}

pub fn remove_background(models_dir: &Path, img: &RgbaImage) -> Result<RgbaImage> {
    if !available(models_dir) {
        return Err(CoreError::AssetNotFound(MODEL_FILE.into()));
    }
    let mask = predict(models_dir, img)?;
    let mask = imageops::resize(&mask, img.width(), img.height(), FilterType::Triangle);
    let mut out = img.clone();
    for (p, m) in out.pixels_mut().zip(mask.pixels()) {
        p[3] = ((u32::from(p[3]) * u32::from(m[0]) + 127) / 255) as u8;
    }
    Ok(out)
}

fn fail(e: impl std::fmt::Display) -> CoreError {
    CoreError::Render(format!("누끼: {e}"))
}

fn predict(models_dir: &Path, img: &RgbaImage) -> Result<GrayImage> {
    let shared = onnx::session(models_dir, MODEL_FILE).map_err(fail)?;
    let mut session = shared.lock().unwrap_or_else(|e| e.into_inner());
    let small = imageops::resize(img, INPUT_SIDE_PX, INPUT_SIDE_PX, FilterType::Triangle);
    let plane = (INPUT_SIDE_PX * INPUT_SIDE_PX) as usize;
    let mut data = vec![0f32; plane * 3];
    for (i, p) in small.pixels().enumerate() {
        for c in 0..3 {
            data[c * plane + i] = f32::from(p[c]) / 255.0 - 0.5;
        }
    }
    let side = INPUT_SIDE_PX as usize;
    let input = Tensor::from_array(([1, 3, side, side], data)).map_err(fail)?;
    let outputs = session.run(ort::inputs![input]).map_err(fail)?;
    let (_, pred) = outputs[0].try_extract_tensor::<f32>().map_err(fail)?;
    if pred.len() != plane {
        return Err(fail(format!("출력 크기가 달라요: {}", pred.len())));
    }
    Ok(GrayImage::from_fn(INPUT_SIDE_PX, INPUT_SIDE_PX, |x, y| {
        Luma([(pred[(y * INPUT_SIDE_PX + x) as usize].clamp(0.0, 1.0) * 255.0).round() as u8])
    }))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use image::Rgba;

    use super::*;

    #[test]
    #[ignore = "needs scripts/fetch-models.ps1"]
    fn shape_on_flat_background_is_cut_out() {
        let models = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/models");
        let img = RgbaImage::from_fn(2000, 1200, |x, y| {
            let (dx, dy) = ((f64::from(x) - 800.0) / 300.0, (f64::from(y) - 600.0) / 350.0);
            if dx * dx + dy * dy <= 1.0 {
                Rgba([230, 40, 40, 255])
            } else {
                Rgba([40, 160, 70, 255])
            }
        });
        let out = remove_background(&models, &img).expect("matting");
        let mean = |x0: u32, y0: u32, x1: u32, y1: u32| {
            let n = f64::from((x1 - x0) * (y1 - y0));
            (y0..y1)
                .flat_map(|y| (x0..x1).map(move |x| (x, y)))
                .map(|(x, y)| f64::from(out.get_pixel(x, y)[3]) / 255.0)
                .sum::<f64>()
                / n
        };
        let fg = mean(650, 400, 950, 800);
        let bg = mean(1300, 0, 2000, 1200);
        assert!(fg > 0.9, "foreground alpha {fg}");
        assert!(bg < 0.1, "background alpha {bg}");
        assert_eq!(out.get_pixel(800, 600).0[..3], [230, 40, 40]);
    }
}
