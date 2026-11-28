//! Inventory walker.
//!
//! Read-only, single pass, no database knowledge. It answers three questions for
//! one library root: which folders exist, which files are readable, and which
//! folders are themselves volumes (numbered page folders).
//!
//! A folder is a volume when it holds at least `min_pages` images and none of its
//! subfolders is a volume. That "leaf-most" rule stops a series folder that
//! happens to contain a handful of scans from being indexed on top of its own
//! volumes. It is a deliberate difference from the browser prototype, which
//! indexes both; see `docs/READER_PARITY.md`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub type ScanResult<T> = Result<T, ScanError>;

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("io: {0}: {1}")]
    Io(String, String),
    #[error("not a directory: {0}")]
    NotADirectory(String),
    #[error("scan cancelled")]
    Cancelled,
}

impl ScanError {
    pub fn kind(&self) -> &'static str {
        match self {
            ScanError::Io(_, _) => "io",
            ScanError::NotADirectory(_) => "invalid",
            ScanError::Cancelled => "cancelled",
        }
    }
}

/// Image extensions the inventory counts. `magpiecraft-comics` has the container
/// list; a test in `magpiecraft-core` asserts both lists agree.
pub const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp", "avif"];

/// Video containers. A video file is an item in its own right, in any library.
pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "avi", "m4v", "mov", "wmv", "webm", "ts", "m2ts", "mpg", "mpeg", "ogv", "vob",
];

/// Audio containers.
pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "m4a", "aac", "ogg", "opus", "wav", "wma", "aiff", "ape", "dsf",
];

/// Ebook containers.
pub const EBOOK_EXTENSIONS: &[&str] = &["epub", "mobi", "azw3", "azw", "fb2", "djvu", "lit", "kfx"];

/// Comic and manga archives. A plain ZIP is one too; whether it is an item depends
/// on the library kind.
pub const COMIC_ARCHIVE_EXTENSIONS: &[&str] = &["cbz", "zip", "cbr", "rar", "cb7", "7z"];

/// Sidecar files that belong to a video but are not items themselves.
pub const SUBTITLE_EXTENSIONS: &[&str] = &["srt", "sub", "ass", "ssa", "vtt", "idx", "sup"];

/// What a file looks like from its name alone. The library kind then decides what
/// the file *is*: the same `.zip` is a comic in a comics library and not an item in
/// a music library, and the same `.jpg` is a page in a comics library and a photo in
/// a photo library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// CBZ, ZIP, CBR, RAR, CB7, 7Z.
    ComicArchive,
    Pdf,
    Image,
    Video,
    Audio,
    Ebook,
    /// A subtitle track: part of a video, never an item.
    Subtitle,
}

/// Classify a file name. `None` means "not media this app indexes" (documents,
/// metadata files, installers, and so on).
pub fn family_of(name: &str) -> Option<Family> {
    let ext = extension(name);
    if ext.is_empty() {
        return None;
    }
    if COMIC_ARCHIVE_EXTENSIONS.contains(&ext.as_str()) {
        Some(Family::ComicArchive)
    } else if ext == "pdf" {
        Some(Family::Pdf)
    } else if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        Some(Family::Image)
    } else if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
        Some(Family::Video)
    } else if AUDIO_EXTENSIONS.contains(&ext.as_str()) {
        Some(Family::Audio)
    } else if EBOOK_EXTENSIONS.contains(&ext.as_str()) {
        Some(Family::Ebook)
    } else if SUBTITLE_EXTENSIONS.contains(&ext.as_str()) {
        Some(Family::Subtitle)
    } else {
        None
    }
}

/// Directory names that never contain the user's media.
pub const SKIP_DIRS: &[&str] = &[
    "__macosx",
    "@eadir",
    "#recycle",
    "system volume information",
    "$recycle.bin",
    ".git",
    ".svn",
    "node_modules",
];

/// One folder, relative to the library root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirInfo {
    /// Relative path with forward slashes. Empty for the library root.
    pub rel: String,
    pub parent: String,
    pub name: String,
    /// Images directly in this folder.
    pub images: usize,
    /// All files directly in this folder, of any kind.
    pub files: usize,
    /// Subfolders that were walked.
    pub subdirs: usize,
    /// True when this folder is itself a readable volume.
    pub volume: bool,
}

/// One readable file, relative to the library root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub rel: String,
    pub parent: String,
    pub name: String,
    /// Lower-case extension without the dot. Empty when the file has none.
    pub ext: String,
    pub size: u64,
    /// Modification time, seconds since the Unix epoch (0 when unavailable).
    pub mtime: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalkOutcome {
    pub dirs: Vec<DirInfo>,
    pub files: Vec<FileInfo>,
    pub bytes: u64,
    pub ms: u128,
    pub skipped_dirs: usize,
}

#[derive(Debug, Clone)]
pub struct WalkOptions {
    /// A folder with at least this many images is a volume.
    pub min_pages: usize,
    /// Follow symlinks and directory junctions. Off by default: a loop here would
    /// walk a drive forever.
    pub follow_links: bool,
    /// Stop after this many entries (0 means no limit).
    pub limit: usize,
}

impl Default for WalkOptions {
    fn default() -> Self {
        WalkOptions {
            min_pages: 3,
            follow_links: false,
            limit: 0,
        }
    }
}

/// Lower-case extension without the dot.
pub fn extension(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => ext.to_ascii_lowercase(),
        _ => String::new(),
    }
}

pub fn is_image_name(name: &str) -> bool {
    let ext = extension(name);
    IMAGE_EXTENSIONS.contains(&ext.as_str())
}

/// Natural sort key: digit runs compare as numbers, text compares case-insensitively.
/// `["p2.jpg", "p10.jpg"]` sorts 2 before 10, which is what every page folder needs.
pub fn natural_key(name: &str) -> Vec<NatPart> {
    let mut parts = Vec::new();
    let mut digits = String::new();
    let mut text = String::new();
    for ch in name.chars() {
        if ch.is_ascii_digit() {
            if !text.is_empty() {
                parts.push(NatPart::Text(text.to_lowercase()));
                text.clear();
            }
            digits.push(ch);
        } else {
            if !digits.is_empty() {
                parts.push(NatPart::Num(digits.parse::<u128>().unwrap_or(u128::MAX)));
                digits.clear();
            }
            text.push(ch);
        }
    }
    if !digits.is_empty() {
        parts.push(NatPart::Num(digits.parse::<u128>().unwrap_or(u128::MAX)));
    }
    if !text.is_empty() {
        parts.push(NatPart::Text(text.to_lowercase()));
    }
    parts
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum NatPart {
    Text(String),
    Num(u128),
}

fn is_skipped_dir(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    SKIP_DIRS.contains(&lower.as_str())
}

fn is_hidden(name: &str) -> bool {
    name.starts_with('.') || name.starts_with('$')
}

fn mtime_secs(meta: &std::fs::Metadata) -> i64 {
    match meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
    {
        Some(d) => i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
        None => 0,
    }
}

struct RawDir {
    rel: String,
    parent: String,
    name: String,
    images: usize,
    files: usize,
    subdirs: usize,
    children: Vec<usize>,
    volume: bool,
}

fn join_rel(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

/// Walk one root. Symlinks are not followed by default; unreadable folders are
/// counted and skipped rather than aborting the whole scan.
pub fn walk(root: &Path, opts: &WalkOptions) -> ScanResult<WalkOutcome> {
    if !root.is_dir() {
        return Err(ScanError::NotADirectory(root.display().to_string()));
    }
    let started = std::time::Instant::now();
    let mut dirs: Vec<RawDir> = Vec::new();
    let mut files: Vec<FileInfo> = Vec::new();
    let mut bytes: u64 = 0;
    let mut skipped = 0usize;
    let mut stack: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), usize::MAX)];

    while let Some((path, parent_idx)) = stack.pop() {
        let parent_rel = if parent_idx == usize::MAX {
            String::new()
        } else {
            dirs.get(parent_idx)
                .map(|d| d.rel.clone())
                .unwrap_or_default()
        };
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "library".to_string());
        let rel = if parent_idx == usize::MAX {
            String::new()
        } else {
            join_rel(&parent_rel, &name)
        };
        let idx = dirs.len();
        dirs.push(RawDir {
            name,
            parent: parent_rel,
            rel: rel.clone(),
            images: 0,
            files: 0,
            subdirs: 0,
            children: Vec::new(),
            volume: false,
        });
        if parent_idx != usize::MAX {
            if let Some(p) = dirs.get_mut(parent_idx) {
                p.children.push(idx);
            }
        }

        let entries = match std::fs::read_dir(&path) {
            Ok(e) => e,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        let mut local_dirs: Vec<(PathBuf, String)> = Vec::new();
        let mut images = 0usize;
        let mut dir_files = 0usize;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let file_type = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            if is_hidden(&name) {
                continue;
            }
            if file_type.is_symlink() {
                if !opts.follow_links {
                    continue;
                }
                let followed = match std::fs::metadata(entry.path()) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                if followed.is_dir() {
                    local_dirs.push((entry.path(), name));
                }
                continue;
            }
            if file_type.is_dir() {
                if is_skipped_dir(&name) {
                    continue;
                }
                local_dirs.push((entry.path(), name));
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let ext = extension(&name);
            if is_image_name(&name) {
                images += 1;
            }
            dir_files += 1;
            bytes = bytes.saturating_add(meta.len());
            files.push(FileInfo {
                rel: join_rel(&rel, &name),
                parent: rel.clone(),
                name,
                ext,
                size: meta.len(),
                mtime: mtime_secs(&meta),
            });
            if opts.limit > 0 && files.len() >= opts.limit {
                break;
            }
        }
        if let Some(d) = dirs.get_mut(idx) {
            d.images = images;
            d.files = dir_files;
            d.subdirs = local_dirs.len();
        }
        // Reverse push so the walk visits folders in natural name order.
        local_dirs.sort_by_key(|d| natural_key(&d.1));
        for (child_path, _) in local_dirs.into_iter().rev() {
            stack.push((child_path, idx));
        }
        if opts.limit > 0 && files.len() >= opts.limit {
            break;
        }
    }

    // Post-order volume decision: children always sit after their parent in this
    // vector, so a reverse pass sees every child before deciding the parent.
    for i in (0..dirs.len()).rev() {
        let has_volume_child = dirs
            .get(i)
            .map(|d| {
                d.children
                    .iter()
                    .any(|c| dirs.get(*c).map(|x| x.volume).unwrap_or(false))
            })
            .unwrap_or(false);
        let images = dirs.get(i).map(|d| d.images).unwrap_or(0);
        let is_root = dirs.get(i).map(|d| d.rel.is_empty()).unwrap_or(true);
        if let Some(d) = dirs.get_mut(i) {
            d.volume = !is_root && images >= opts.min_pages && !has_volume_child;
        }
    }

    let out_dirs: Vec<DirInfo> = dirs
        .iter()
        .map(|d| DirInfo {
            rel: d.rel.clone(),
            parent: d.parent.clone(),
            name: d.name.clone(),
            images: d.images,
            files: d.files,
            subdirs: d.subdirs,
            volume: d.volume,
        })
        .collect();

    files.sort_by_key(|f| natural_key(&f.rel));

    Ok(WalkOutcome {
        dirs: out_dirs,
        files,
        bytes,
        ms: started.elapsed().as_millis(),
        skipped_dirs: skipped,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "magpie-scan-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&base).unwrap();
        base
    }

    fn touch(path: &Path, bytes: usize) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, vec![b'x'; bytes]).unwrap();
    }

    #[test]
    fn finds_files_dirs_and_sizes() {
        let root = tmp("basic");
        touch(&root.join("a/one.cbz"), 10);
        touch(&root.join("a/b/two.cbz"), 20);
        fs::create_dir_all(root.join("empty")).unwrap();
        let out = walk(&root, &WalkOptions::default()).unwrap();
        assert_eq!(out.files.len(), 2);
        assert_eq!(out.bytes, 30);
        assert!(out.dirs.iter().any(|d| d.rel == "empty"));
        assert_eq!(out.dirs.iter().filter(|d| d.rel == "a").count(), 1);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn leaf_image_folder_is_a_volume_and_skipped_dirs_are_ignored() {
        let root = tmp("volume");
        for i in 1..=4 {
            touch(&root.join(format!("Series/Vol 1/{i:02}.jpg")), 4);
        }
        touch(&root.join("__MACOSX/junk.jpg"), 1);
        let out = walk(&root, &WalkOptions::default()).unwrap();
        let vol = out.dirs.iter().find(|d| d.rel == "Series/Vol 1").unwrap();
        assert!(vol.volume, "leaf page folder is a volume");
        assert!(!out.dirs.iter().any(|d| d.rel.contains("__MACOSX")));
        let series = out.dirs.iter().find(|d| d.rel == "Series").unwrap();
        assert!(!series.volume);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn folder_below_the_image_threshold_is_not_a_volume() {
        let root = tmp("below");
        touch(&root.join("Odd/1.jpg"), 1);
        touch(&root.join("Odd/2.jpg"), 1);
        let out = walk(&root, &WalkOptions::default()).unwrap();
        assert!(!out.dirs.iter().find(|d| d.rel == "Odd").unwrap().volume);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn families_cover_every_media_kind() {
        assert_eq!(family_of("a.cbz"), Some(Family::ComicArchive));
        assert_eq!(family_of("a.CBR"), Some(Family::ComicArchive));
        assert_eq!(family_of("a.cb7"), Some(Family::ComicArchive));
        assert_eq!(family_of("a.pdf"), Some(Family::Pdf));
        assert_eq!(family_of("scan.JPG"), Some(Family::Image));
        assert_eq!(family_of("movie.mkv"), Some(Family::Video));
        assert_eq!(family_of("movie.M2TS"), Some(Family::Video));
        assert_eq!(family_of("track.flac"), Some(Family::Audio));
        assert_eq!(family_of("book.epub"), Some(Family::Ebook));
        assert_eq!(family_of("movie.srt"), Some(Family::Subtitle));
        assert_eq!(family_of("cover.opf"), None);
        assert_eq!(family_of("notes.txt"), None);
        assert_eq!(family_of("noext"), None);
        // Image folders still count images with the same list.
        for ext in IMAGE_EXTENSIONS {
            assert_eq!(family_of(&format!("p.{ext}")), Some(Family::Image), "{ext}");
        }
    }

    #[test]
    fn natural_sort_beats_lexicographic() {
        let mut names = vec!["p10.jpg", "p2.jpg", "p1.jpg", "p20.jpg"];
        names.sort_by_key(|n| natural_key(n));
        assert_eq!(names, vec!["p1.jpg", "p2.jpg", "p10.jpg", "p20.jpg"]);
        assert_eq!(extension("A.CBZ"), "cbz");
        assert!(is_image_name("page.01.JPEG"));
        assert!(!is_image_name("notes.txt"));
    }
}
