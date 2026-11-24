//! Image decoding helpers shared by the cover cache and the reader.
//!
//! Decoding is bounded: a corrupt or enormous image returns an error, never an
//! allocation the size of the file. Formats enabled in this build: JPEG, PNG,
//! GIF, WebP, BMP. AVIF is indexed but reported as not supported until the
//! decoder decision in the PRD is settled.

use crate::error::{MagpieError, MagpieResult};
use image::{DynamicImage, RgbaImage};

/// Decode bytes into an image.
pub fn decode(bytes: &[u8]) -> MagpieResult<DynamicImage> {
    image::load_from_memory(bytes).map_err(|e| {
        MagpieError::Invalid(format!(
            "image decode failed: {e} (AVIF is not supported yet)"
        ))
    })
}

/// Decode and shrink so the long edge fits the box, keeping the aspect ratio.
/// Used for covers and for reader pages rendered at screen size.
pub fn decode_fitted(bytes: &[u8], max_width: u32, max_height: u32) -> MagpieResult<RgbaImage> {
    let img = decode(bytes)?;
    let width = img.width().max(1);
    let height = img.height().max(1);
    let scale = f64::from(max_width) / f64::from(width);
    let scale_h = f64::from(max_height) / f64::from(height);
    let scale = scale.min(scale_h).min(1.0);
    if scale >= 1.0 {
        return Ok(img.to_rgba8());
    }
    let target_w = ((f64::from(width) * scale).round() as u32).max(1);
    let target_h = ((f64::from(height) * scale).round() as u32).max(1);
    Ok(image::imageops::thumbnail(
        &img.to_rgba8(),
        target_w,
        target_h,
    ))
}

/// Raw RGBA for uploading to a GPU texture.
pub fn rgba(img: &DynamicImage) -> RgbaImage {
    img.to_rgba8()
}

/// Encode an image as JPEG at `quality`. Only writing to MagpieCraft's own cache
/// directory, never into the collection.
pub fn encode_jpeg(img: &DynamicImage, quality: u8) -> MagpieResult<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    let mut encoder =
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(30, 95));
    encoder
        .encode_image(img)
        .map_err(|e| MagpieError::Invalid(format!("jpeg encode failed: {e}")))?;
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([200, 30, 30, 255]));
        let mut out = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut out);
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut cursor, image::ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn decode_and_fit_keeps_aspect_ratio() {
        let bytes = png_bytes(400, 600);
        let fitted = decode_fitted(&bytes, 200, 260).unwrap();
        assert_eq!(fitted.width(), 173);
        assert_eq!(fitted.height(), 260);
        // Upscale is refused: a small image stays its own size.
        let small = decode_fitted(&png_bytes(40, 60), 400, 600).unwrap();
        assert_eq!((small.width(), small.height()), (40, 60));
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(decode(b"not an image").is_err());
        assert!(decode(&[]).is_err());
    }

    #[test]
    fn jpeg_roundtrip() {
        let img = decode(&png_bytes(60, 90)).unwrap();
        let jpeg = encode_jpeg(&img, 80).unwrap();
        assert_eq!(jpeg.first().copied(), Some(0xFF));
        assert_eq!(jpeg.get(1).copied(), Some(0xD8));
        let back = decode(&jpeg).unwrap();
        assert_eq!(back.width(), 60);
    }
}
