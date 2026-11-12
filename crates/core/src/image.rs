use std::io::Cursor;
use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use image::imageops::{self, FilterType};
use image::{ImageFormat, ImageReader, Rgba, RgbaImage};

use crate::Result;

pub(crate) const JPEG_QUALITY: u8 = 92;

pub fn load(path: &Path) -> Result<RgbaImage> {
    Ok(ImageReader::open(path)?
        .with_guessed_format()?
        .decode()?
        .to_rgba8())
}

pub fn save_png(img: &RgbaImage, path: &Path) -> Result<()> {
    img.save_with_format(path, ImageFormat::Png)?;
    Ok(())
}

pub fn to_png_bytes(img: &RgbaImage) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    img.write_to(&mut buf, ImageFormat::Png)?;
    Ok(buf.into_inner())
}

pub fn from_png_bytes(bytes: &[u8]) -> Result<RgbaImage> {
    Ok(image::load_from_memory_with_format(bytes, ImageFormat::Png)?.to_rgba8())
}

pub fn to_base64_png(img: &RgbaImage) -> Result<String> {
    Ok(STANDARD.encode(to_png_bytes(img)?))
}

pub fn resize_max(img: &RgbaImage, max_px: u32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let longest = w.max(h);
    if longest <= max_px {
        return img.clone();
    }
    let scale = f64::from(max_px) / f64::from(longest);
    let nw = ((f64::from(w) * scale).round() as u32).max(1);
    let nh = ((f64::from(h) * scale).round() as u32).max(1);
    resize_to(img, nw, nh)
}

pub fn resize_to(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    imageops::resize(img, w, h, FilterType::Lanczos3)
}

pub fn crop(img: &RgbaImage, rect: [u32; 4]) -> RgbaImage {
    let [x, y, w, h] = clamp_rect(img, rect);
    imageops::crop_imm(img, x, y, w, h).to_image()
}

fn clamp_rect(img: &RgbaImage, [x, y, w, h]: [u32; 4]) -> [u32; 4] {
    let x = x.min(img.width());
    let y = y.min(img.height());
    [x, y, w.min(img.width() - x), h.min(img.height() - y)]
}

pub fn side_by_side(a: &RgbaImage, b: &RgbaImage, gap: u32) -> RgbaImage {
    let h = a.height().max(b.height());
    let mut out = RgbaImage::from_pixel(a.width() + gap + b.width(), h, Rgba([0xE7, 0xE7, 0xEA, 0xFF]));
    imageops::overlay(&mut out, a, 0, i64::from((h - a.height()) / 2));
    imageops::overlay(
        &mut out,
        b,
        i64::from(a.width() + gap),
        i64::from((h - b.height()) / 2),
    );
    out
}

pub fn dominant_color(img: &RgbaImage, rect: Option<[u32; 4]>) -> String {
    let [x0, y0, w, h] = clamp_rect(img, rect.unwrap_or([0, 0, img.width(), img.height()]));
    let mut counts = vec![0u64; 4096];
    let mut sums = vec![[0u64; 3]; 4096];
    for y in y0..y0 + h {
        for x in x0..x0 + w {
            let p = img.get_pixel(x, y);
            if p[3] <= 8 {
                continue;
            }
            let bin = (usize::from(p[0] >> 4) << 8) | (usize::from(p[1] >> 4) << 4) | usize::from(p[2] >> 4);
            counts[bin] += 1;
            for c in 0..3 {
                sums[bin][c] += u64::from(p[c]);
            }
        }
    }
    let Some(best) = (0..4096).filter(|&b| counts[b] > 0).max_by_key(|&b| counts[b]) else {
        return "#FFFFFF".into();
    };
    let n = counts[best];
    let [r, g, b] = sums[best].map(|s| (s + n / 2) / n);
    format!("#{r:02X}{g:02X}{b:02X}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split() -> RgbaImage {
        RgbaImage::from_fn(40, 20, |x, _| {
            if x < 30 {
                Rgba([0xE7, 0xE7, 0xEA, 255])
            } else {
                Rgba([10, 20, 200, 255])
            }
        })
    }

    #[test]
    fn png_and_base64_round_trip() {
        let img = split();
        assert_eq!(
            from_png_bytes(&to_png_bytes(&img).expect("png")).expect("decode"),
            img
        );
        let b64 = to_base64_png(&img).expect("b64");
        assert_eq!(
            from_png_bytes(&STANDARD.decode(b64).expect("decode b64")).expect("png"),
            img
        );
    }

    #[test]
    fn load_and_save() {
        let dir = std::env::temp_dir().join(format!("hoist-core-img-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("wrong-extension.jpg");
        save_png(&split(), &path).expect("save");
        assert_eq!(load(&path).expect("load"), split());
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn resize_and_crop() {
        let img = split();
        assert_eq!(resize_max(&img, 10).dimensions(), (10, 5));
        assert_eq!(resize_max(&img, 100).dimensions(), (40, 20));
        assert_eq!(crop(&img, [30, 5, 100, 100]).dimensions(), (10, 15));
        assert_eq!(crop(&img, [50, 50, 10, 10]).dimensions(), (0, 0));
    }

    #[test]
    fn side_by_side_and_dominant_color() {
        let a = RgbaImage::from_pixel(10, 10, Rgba([255, 0, 0, 255]));
        let b = RgbaImage::from_pixel(5, 4, Rgba([0, 255, 0, 255]));
        let out = side_by_side(&a, &b, 3);
        assert_eq!(out.dimensions(), (18, 10));
        assert_eq!(*out.get_pixel(11, 0), Rgba([0xE7, 0xE7, 0xEA, 0xFF]));
        assert_eq!(*out.get_pixel(14, 5), Rgba([0, 255, 0, 255]));

        assert_eq!(dominant_color(&split(), None), "#E7E7EA");
        assert_eq!(dominant_color(&split(), Some([30, 0, 10, 20])), "#0A14C8");
        assert_eq!(dominant_color(&RgbaImage::new(4, 4), None), "#FFFFFF");
    }
}
