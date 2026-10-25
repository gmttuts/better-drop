//! Ảnh thu nhỏ của ảnh/video gửi kèm yêu cầu nhận (từ 0.3.7), để máy nhận xem trước
//! khi bấm Chấp nhận và để hiện trong Lịch sử.
//!
//! Máy gửi tạo JPEG nhỏ (cạnh dài `MAX_SIDE`), gửi base64 trong trường `thumbs` của
//! `/prepare`. Máy nhận chỉ giữ JPEG hợp lệ, đúng giới hạn kích thước và số lượng; bản cũ
//! không gửi trường này (hoặc bỏ qua nó) nên vẫn gửi nhận được như trước.

use crate::models::{FileMeta, Thumb};
use base64::Engine as _;
use std::io::{BufRead, Seek};
use std::sync::Arc;
use std::time::Duration;

/// Cạnh dài nhất của ảnh thu nhỏ (pixel).
pub const MAX_SIDE: u32 = 160;
const QUALITY: u8 = 70;
/// Một ảnh thu nhỏ tối đa (byte JPEG).
pub const MAX_BYTES: usize = 20 * 1024;
/// Số ảnh thu nhỏ tối đa trong một yêu cầu (hộp thoại nhận hiện chừng này ô).
pub const MAX_THUMBS: usize = 6;
/// Tổng dung lượng ảnh thu nhỏ trong một yêu cầu (byte JPEG).
pub const MAX_TOTAL: usize = 150 * 1024;
/// Ảnh gốc lớn hơn thì không giải mã (tốn RAM, chậm).
const MAX_SOURCE: u64 = 64 * 1024 * 1024;
/// Chờ tạo ảnh thu nhỏ tối đa chừng này rồi gửi luôn (ảnh chưa xong thì bỏ).
pub const TIME_LIMIT: Duration = Duration::from_secs(3);

/// Lấy ảnh thu nhỏ bằng thư viện của hệ điều hành (điện thoại: khung hình video, HEIC).
/// Tham số: đường dẫn/URI, có phải video không. Trả về JPEG.
pub type MediaThumbFn = Arc<dyn Fn(&str, bool) -> Option<Vec<u8>> + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Media {
    Image,
    Video,
}

/// Ảnh hay video, đoán theo đuôi file.
pub fn media_kind(name: &str) -> Option<Media> {
    let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "heic" | "heif" | "avif" | "bmp" | "tif" | "tiff" => Some(Media::Image),
        "mp4" | "mov" | "m4v" | "3gp" | "mkv" | "webm" | "avi" => Some(Media::Video),
        _ => None,
    }
}

/// Đúng là một JPEG trọn vẹn: bắt đầu bằng SOI (FF D8 FF), kết thúc bằng EOI (FF D9).
pub fn is_jpeg(b: &[u8]) -> bool {
    b.len() > 4 && b.starts_with(&[0xFF, 0xD8, 0xFF]) && b.ends_with(&[0xFF, 0xD9])
}

pub fn data_url(jpeg: &[u8]) -> String {
    format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(jpeg))
}

fn encode_jpeg(img: &image::DynamicImage, quality: u8) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality)
        .encode_image(&img.to_rgb8())
        .ok()?;
    Some(out)
}

/// Thu nhỏ rồi nén JPEG; ảnh nhiều chi tiết thì giảm chất lượng/kích thước cho vừa `MAX_BYTES`.
pub fn shrink(img: &image::DynamicImage) -> Option<Vec<u8>> {
    let small = img.thumbnail(MAX_SIDE, MAX_SIDE);
    for q in [QUALITY, 55, 40] {
        let jpeg = encode_jpeg(&small, q)?;
        if jpeg.len() <= MAX_BYTES {
            return Some(jpeg);
        }
    }
    encode_jpeg(&img.thumbnail(MAX_SIDE * 3 / 4, MAX_SIDE * 3 / 4), 40).filter(|j| j.len() <= MAX_BYTES)
}

/// Giải mã một ảnh (JPEG/PNG/WebP/GIF), xoay theo EXIF rồi thu nhỏ.
pub fn from_reader(r: impl BufRead + Seek) -> Option<Vec<u8>> {
    use image::ImageDecoder as _;
    let mut reader = image::ImageReader::new(r).with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().ok()?;
    let orientation = decoder.orientation().ok();
    let mut img = image::DynamicImage::from_decoder(decoder).ok()?;
    if let Some(o) = orientation {
        img.apply_orientation(o);
    }
    shrink(&img)
}

/// Một file cần ảnh thu nhỏ.
pub struct Source {
    pub file_id: String,
    pub name: String,
    pub source: String,
    pub size: u64,
}

/// Các file ảnh/video đầu tiên (tối đa `MAX_THUMBS`) — đúng những ô hộp thoại nhận sẽ hiện.
pub fn pick<'a>(files: impl IntoIterator<Item = (&'a FileMeta, &'a str)>) -> Vec<Source> {
    files
        .into_iter()
        .filter(|(m, _)| media_kind(&m.rel_path).is_some())
        .take(MAX_THUMBS)
        .map(|(m, src)| Source {
            file_id: m.id.clone(),
            name: m.rel_path.clone(),
            source: src.to_string(),
            size: m.size,
        })
        .collect()
}

/// Tạo ảnh thu nhỏ cho một file (chạy trong luồng chặn). Lỗi thì trả None.
pub fn make_one(
    src: &Source,
    open: &(dyn Fn(&str) -> std::io::Result<std::fs::File> + Send + Sync),
    system: Option<&MediaThumbFn>,
) -> Option<Vec<u8>> {
    let kind = media_kind(&src.name)?;
    let decoded = if kind == Media::Image && src.size <= MAX_SOURCE {
        open(&src.source).ok().and_then(|f| from_reader(std::io::BufReader::new(f)))
    } else {
        None
    };
    let jpeg = decoded.or_else(|| {
        let f = system?;
        let raw = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&src.source, kind == Media::Video))).ok()??;
        if is_jpeg(&raw) && raw.len() <= MAX_BYTES {
            return Some(raw);
        }
        // Hệ điều hành trả ảnh to quá: thu nhỏ lại.
        from_reader(std::io::Cursor::new(raw))
    })?;
    (is_jpeg(&jpeg) && jpeg.len() <= MAX_BYTES).then_some(jpeg)
}

/// Máy nhận: giữ những ảnh thu nhỏ hợp lệ. Trả về (vị trí file trong `files`, JPEG), theo
/// thứ tự file. Bỏ: file không có trong yêu cầu, không phải một trong `MAX_THUMBS` file
/// ảnh/video đầu, trùng, không phải JPEG, quá `MAX_BYTES`, hoặc vượt tổng `MAX_TOTAL`.
pub fn accept(files: &[FileMeta], thumbs: &[Thumb]) -> Vec<(usize, Vec<u8>)> {
    if thumbs.is_empty() {
        return Vec::new();
    }
    let media: Vec<usize> = files
        .iter()
        .enumerate()
        .filter(|(_, f)| media_kind(&f.rel_path).is_some())
        .map(|(i, _)| i)
        .take(MAX_THUMBS)
        .collect();
    // Chuỗi base64 dài hơn thế này thì chắc chắn quá MAX_BYTES: khỏi giải mã.
    let max_b64 = MAX_BYTES.div_ceil(3) * 4;
    let mut out: Vec<(usize, Vec<u8>)> = Vec::new();
    let mut total = 0usize;
    for t in thumbs {
        if out.len() >= MAX_THUMBS {
            break;
        }
        let Some(&idx) = media.iter().find(|&&i| files[i].id == t.file) else { continue };
        if out.iter().any(|(i, _)| *i == idx) || t.jpeg.len() > max_b64 {
            continue;
        }
        let Ok(jpeg) = base64::engine::general_purpose::STANDARD.decode(&t.jpeg) else { continue };
        if !is_jpeg(&jpeg) || jpeg.len() > MAX_BYTES || total + jpeg.len() > MAX_TOTAL {
            continue;
        }
        total += jpeg.len();
        out.push((idx, jpeg));
    }
    out.sort_by_key(|(i, _)| *i);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x ^ y) % 256) as u8]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img).write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn shrinks_png_to_small_jpeg() {
        let jpeg = from_reader(std::io::Cursor::new(png(1200, 800))).unwrap();
        assert!(is_jpeg(&jpeg));
        assert!(jpeg.len() <= MAX_BYTES);
        let img = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((img.width(), img.height()), (160, 107));
    }

    #[test]
    fn noisy_image_still_fits() {
        let mut seed = 1u32;
        let img = image::RgbImage::from_fn(400, 400, |_, _| {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            image::Rgb([(seed >> 16) as u8, (seed >> 8) as u8, seed as u8])
        });
        let jpeg = shrink(&image::DynamicImage::ImageRgb8(img)).unwrap();
        assert!(jpeg.len() <= MAX_BYTES);
    }

    #[test]
    fn not_an_image_gives_none() {
        assert!(from_reader(std::io::Cursor::new(b"hello".to_vec())).is_none());
    }

    #[test]
    fn media_kind_by_extension() {
        assert_eq!(media_kind("a/B.JPG"), Some(Media::Image));
        assert_eq!(media_kind("clip.MOV"), Some(Media::Video));
        assert_eq!(media_kind("notes.txt"), None);
        assert_eq!(media_kind("jpg"), None);
    }
}
