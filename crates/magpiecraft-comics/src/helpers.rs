//! External helpers and cache locations for containers that need them.
//!
//! CBR/RAR is read through the 7-Zip subprocess (licence-clean for us, no RAR code
//! linked) and PDF pages through PdfCraft (`pdfcraft-cli`). Both write into the app
//! cache, never into a library, and both honour a size cap with oldest-first
//! eviction so a 400 GB collection cannot fill the disk with extracted copies.

use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Default ceiling for extracted CBR/RAR copies.
pub const DEFAULT_EXTRACT_CAP_BYTES: u64 = 8 * 1024 * 1024 * 1024;
/// Default ceiling for rendered PDF pages.
pub const DEFAULT_RENDER_CAP_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Everything a container needs that is not the file itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Helpers {
    /// Path to `7z`. `None` means RAR pages report `not_configured`.
    pub seven_zip: Option<PathBuf>,
    /// Path to `pdfcraft-cli`. `None` means PDF pages report `not_configured`.
    pub pdfcraft: Option<PathBuf>,
    /// Root of the app cache. Extracted copies and rendered pages live below it.
    pub cache_dir: PathBuf,
    /// PDF render resolution. 200 dpi is roughly 1700 x 2200 for a comic page.
    pub pdf_dpi: u32,
    pub extract_cap_bytes: u64,
    pub render_cap_bytes: u64,
}

impl Default for Helpers {
    fn default() -> Self {
        Helpers {
            seven_zip: None,
            pdfcraft: None,
            cache_dir: std::env::temp_dir().join("magpiecraft-cache"),
            pdf_dpi: 200,
            extract_cap_bytes: DEFAULT_EXTRACT_CAP_BYTES,
            render_cap_bytes: DEFAULT_RENDER_CAP_BYTES,
        }
    }
}

impl Helpers {
    pub fn new(seven_zip: Option<PathBuf>, pdfcraft: Option<PathBuf>, cache_dir: PathBuf) -> Self {
        Helpers {
            seven_zip,
            pdfcraft,
            cache_dir,
            ..Helpers::default()
        }
    }

    /// Stable cache key for one archive: path, size and mtime, so a re-download or
    /// an edited file gets a fresh extraction instead of a stale one.
    pub fn key(&self, path: &Path) -> String {
        let meta = std::fs::metadata(path).ok();
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let mtime = meta
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut hasher = DefaultHasher::new();
        path.to_string_lossy().to_lowercase().hash(&mut hasher);
        size.hash(&mut hasher);
        mtime.hash(&mut hasher);
        format!("{:016x}-{size}", hasher.finish())
    }

    pub fn extract_root(&self) -> PathBuf {
        self.cache_dir.join("extract")
    }

    pub fn render_root(&self) -> PathBuf {
        self.cache_dir.join("pdf")
    }

    pub fn extract_dir(&self, key: &str) -> PathBuf {
        self.extract_root().join(key)
    }

    pub fn render_dir(&self, key: &str) -> PathBuf {
        self.render_root().join(key)
    }

    pub fn has_seven_zip(&self) -> bool {
        self.seven_zip.as_ref().is_some_and(|p| p.is_file())
    }

    pub fn has_pdfcraft(&self) -> bool {
        self.pdfcraft.as_ref().is_some_and(|p| p.is_file())
    }
}

/// Total size of the files below `dir`, in bytes. Unreadable entries count as 0.
pub fn dir_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = match std::fs::read_dir(&current) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                total = total.saturating_add(meta.len());
            }
        }
    }
    total
}

/// Delete the oldest subdirectories of `dir` until its total size fits `cap`.
/// Oldest means least recently modified, which for an extraction directory is the
/// last time it was read or written. Returns the number of directories removed.
pub fn enforce_cap(dir: &Path, cap: u64) -> usize {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return 0,
    };
    let mut children: Vec<(std::time::SystemTime, PathBuf, u64)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !meta.is_dir() {
            continue;
        }
        let touched = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
        children.push((touched, path.clone(), dir_size(&path)));
    }
    let mut total: u64 = children.iter().map(|(_, _, size)| *size).sum();
    if total <= cap {
        return 0;
    }
    children.sort_by_key(|(touched, _, _)| *touched);
    let mut removed = 0usize;
    for (_, path, size) in children {
        if total <= cap {
            break;
        }
        if std::fs::remove_dir_all(&path).is_ok() {
            total = total.saturating_sub(size);
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "magpie-helpers-{}-{}-{}",
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

    #[test]
    fn keys_change_when_the_file_changes() {
        let dir = tmp("key");
        let file = dir.join("a.cbr");
        std::fs::write(&file, b"one").unwrap();
        let helpers = Helpers::new(None, None, dir.join("cache"));
        let first = helpers.key(&file);
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(&file, b"two-two").unwrap();
        let second = helpers.key(&file);
        assert_ne!(
            first, second,
            "a rewritten archive must not reuse its extraction"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cap_evicts_the_oldest_directory_first() {
        let dir = tmp("cap");
        let cache = dir.join("cache").join("extract");
        std::fs::create_dir_all(&cache).unwrap();
        for (name, size) in [("old", 4000usize), ("middle", 4000), ("new", 4000)] {
            let d = cache.join(name);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("page.png"), vec![0u8; size]).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(dir_size(&cache), 12000);
        let removed = enforce_cap(&cache, 8000);
        assert!(removed >= 1);
        assert!(dir_size(&cache) <= 8000);
        assert!(!cache.join("old").exists(), "the oldest goes first");
        assert!(cache.join("new").exists(), "the newest stays");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn tool_presence_is_checked_not_assumed() {
        let helpers = Helpers::new(
            Some(PathBuf::from("Z:/nope/7z.exe")),
            None,
            PathBuf::from("C:/tmp"),
        );
        assert!(!helpers.has_seven_zip());
        assert!(!helpers.has_pdfcraft());
    }
}
