//! Cover thumbnails, cached as JPEG files under the app cache directory.
//!
//! A cover is page one of an item, or of the first readable item below a folder
//! (the prototype's folder covers). Rendering is a pure function of the source
//! and the cache path, so the desktop app can call it from a worker thread with
//! no database handle and no UI state.

use crate::error::{MagpieError, MagpieResult};
use crate::images;
use crate::model::ContainerKind;
use magpiecraft_comics::PageSet;
use std::path::{Path, PathBuf};

/// Everything needed to render one cover, resolved on the UI thread from the
/// database without touching the file itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverSource {
    /// Where the JPEG is written.
    pub cache_file: PathBuf,
    /// Archive file or page folder that provides page one.
    pub item_path: PathBuf,
    pub container: ContainerKind,
}

/// Render (or reuse) the cover JPEG. Returns the cache file path.
///
/// Existing files are reused; a render that fails leaves nothing behind so a
/// retry is not blocked by a zero-byte file.
pub fn render_cover(
    source: &CoverSource,
    width: u32,
    height: u32,
    quality: u8,
) -> MagpieResult<PathBuf> {
    if source.cache_file.is_file() {
        return Ok(source.cache_file.clone());
    }
    if let Some(dir) = source.cache_file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| MagpieError::io(dir, e))?;
    }
    let set = PageSet::open(&source.item_path, source.container)?;
    if set.is_empty() {
        return Err(MagpieError::not_found(format!(
            "{} has no pages",
            source.item_path.display()
        )));
    }
    let bytes = set.read(1)?;
    let img = images::decode(&bytes)?;
    let fitted = fit(&img, width, height);
    let jpeg = images::encode_jpeg(&fitted, quality)?;
    let tmp = source.cache_file.with_extension("jpg.tmp");
    std::fs::write(&tmp, &jpeg).map_err(|e| MagpieError::io(&tmp, e))?;
    std::fs::rename(&tmp, &source.cache_file)
        .map_err(|e| MagpieError::io(&source.cache_file, e))?;
    Ok(source.cache_file.clone())
}

/// Scale to fit inside the box without upscaling, keeping aspect ratio.
fn fit(img: &image::DynamicImage, width: u32, height: u32) -> image::DynamicImage {
    let w = img.width().max(1);
    let h = img.height().max(1);
    let scale = (f64::from(width) / f64::from(w)).min(f64::from(height) / f64::from(h));
    if scale >= 1.0 {
        return img.clone();
    }
    let target_w = ((f64::from(w) * scale).round() as u32).max(1);
    let target_h = ((f64::from(h) * scale).round() as u32).max(1);
    img.resize_exact(target_w, target_h, image::imageops::FilterType::Triangle)
}

/// Cache file name for a key: stable, short, no path separators, no user text.
pub fn cache_file_for(covers_dir: &Path, key: &str) -> PathBuf {
    let (hash, _) = magpiecraft_health::hash_bytes(key.as_bytes());
    let short = hash.get(..20).unwrap_or(&hash);
    covers_dir.join(format!("{short}.jpg"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "magpie-cover-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_cbz(path: &Path, pages: usize) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        for i in 1..=pages {
            let img = image::RgbaImage::from_fn(120, 180, |x, y| {
                image::Rgba([(x % 255) as u8, (y % 255) as u8, (i * 20 % 255) as u8, 255])
            });
            let mut png = Vec::new();
            let mut cursor = std::io::Cursor::new(&mut png);
            image::DynamicImage::ImageRgba8(img)
                .write_to(&mut cursor, image::ImageFormat::Png)
                .unwrap();
            zip.start_file(format!("{i:03}.png"), opts).unwrap();
            zip.write_all(&png).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn cover_is_rendered_cached_and_reused() {
        let dir = tmp("render");
        let cbz = dir.join("vol.cbz");
        write_cbz(&cbz, 3);
        let source = CoverSource {
            cache_file: cache_file_for(&dir.join("covers"), "lib:1|vol.cbz"),
            item_path: cbz.clone(),
            container: ContainerKind::Zip,
        };
        let path = render_cover(&source, 90, 130, 80).unwrap();
        assert!(path.is_file());
        let first = std::fs::metadata(&path).unwrap().len();
        assert!(first > 200);
        // Second call is a cache hit, not a re-render.
        let again = render_cover(&source, 90, 130, 80).unwrap();
        assert_eq!(std::fs::metadata(&again).unwrap().len(), first);
        let img = image::open(&path).unwrap();
        assert!(img.width() <= 90 && img.height() <= 130);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cache_names_are_stable_and_path_free() {
        let a = cache_file_for(Path::new("C:/c"), "lib:1|a/b.cbz");
        let b = cache_file_for(Path::new("C:/c"), "lib:1|a/b.cbz");
        assert_eq!(a, b);
        let name = a.file_name().unwrap().to_string_lossy().to_string();
        assert!(!name.contains('/') && !name.contains(':'), "{name}");
    }

    #[test]
    fn unreadable_source_is_an_error() {
        let dir = tmp("bad");
        let source = CoverSource {
            cache_file: dir.join("covers/x.jpg"),
            item_path: dir.join("missing.cbz"),
            container: ContainerKind::Zip,
        };
        assert!(render_cover(&source, 90, 130, 80).is_err());
        assert!(!source.cache_file.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
