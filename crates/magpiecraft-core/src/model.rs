//! Domain model and the JSON views returned by the command bus.
//!
//! Item identity is `lib:<library id>|<path relative to the library root>`. The
//! key survives a moved library root and a changed drive letter, which a path
//! would not, and it is readable in a CLI invocation, which a base64 blob is not.

use serde::{Deserialize, Serialize};

pub use magpiecraft_comics::ContainerKind;

/// What kind of collection a library holds. Drives default reading direction,
/// default extensions and the icons the UI uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Comics,
    Manga,
    Magazines,
    Video,
    Audio,
    Ebook,
    Photo,
    Other,
}

impl MediaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MediaKind::Comics => "comics",
            MediaKind::Manga => "manga",
            MediaKind::Magazines => "magazines",
            MediaKind::Video => "video",
            MediaKind::Audio => "audio",
            MediaKind::Ebook => "ebook",
            MediaKind::Photo => "photo",
            MediaKind::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "comics" | "comic" => Some(MediaKind::Comics),
            "manga" => Some(MediaKind::Manga),
            "magazines" | "magazine" => Some(MediaKind::Magazines),
            "video" => Some(MediaKind::Video),
            "audio" => Some(MediaKind::Audio),
            "ebook" | "book" => Some(MediaKind::Ebook),
            "photo" | "photos" => Some(MediaKind::Photo),
            "other" => Some(MediaKind::Other),
            _ => None,
        }
    }

    /// Manga and Japanese magazine libraries read right to left by default; the
    /// per-series and per-item overrides win over this.
    pub fn default_rtl(self) -> bool {
        matches!(self, MediaKind::Manga)
    }
}

/// Library id plus path relative to the library root. Empty `rel` is the root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemKey {
    pub library_id: i64,
    pub rel: String,
}

impl ItemKey {
    pub fn new(library_id: i64, rel: &str) -> Self {
        ItemKey {
            library_id,
            rel: rel.trim_matches('/').to_string(),
        }
    }

    pub fn root(library_id: i64) -> Self {
        ItemKey {
            library_id,
            rel: String::new(),
        }
    }

    pub fn as_string(&self) -> String {
        format!("lib:{}|{}", self.library_id, self.rel)
    }

    pub fn parse(key: &str) -> Option<Self> {
        let rest = key.trim().strip_prefix("lib:")?;
        // A missing `|` means the library root, so `lib:3` and `lib:3|` are the
        // same key (shells eat a trailing pipe).
        let (id, rel) = match rest.split_once('|') {
            Some((id, rel)) => (id, rel),
            None => (rest, ""),
        };
        let library_id = id.trim().parse::<i64>().ok()?;
        Some(ItemKey {
            library_id,
            rel: rel.trim_matches('/').to_string(),
        })
    }

    pub fn is_root(&self) -> bool {
        self.rel.is_empty()
    }

    /// Parent folder key. `None` only at the library root itself; a top-level
    /// item's parent is the root key.
    pub fn parent(&self) -> Option<ItemKey> {
        if self.rel.is_empty() {
            return None;
        }
        let parent = match self.rel.rsplit_once('/') {
            Some((p, _)) => p.to_string(),
            None => String::new(),
        };
        Some(ItemKey {
            library_id: self.library_id,
            rel: parent,
        })
    }

    /// Last path segment, or the library name for a root key.
    pub fn name(&self) -> String {
        self.rel
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("")
            .to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub id: i64,
    pub name: String,
    pub root: String,
    pub kind: MediaKind,
    pub mature: bool,
    /// Library-level override; `None` means use the kind's default.
    pub rtl_default: Option<bool>,
    pub created_at: String,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Crumb {
    /// `None` for the libraries overview.
    pub key: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressView {
    pub page: i64,
    pub total: i64,
    pub finished: bool,
    pub percent: f32,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemView {
    pub key: String,
    pub library_id: i64,
    pub library: String,
    pub rel: String,
    pub parent: String,
    pub name: String,
    pub title: String,
    pub container: ContainerKind,
    pub readable: bool,
    pub size: i64,
    pub mtime: i64,
    pub page_count: Option<i64>,
    pub series: Option<String>,
    pub series_id: Option<i64>,
    pub volume: Option<String>,
    /// Effective reading direction after library, series and item rules.
    pub rtl: bool,
    /// Scan page minus printed page, remembered per item.
    pub page_offset: i32,
    /// How confident the filename parse was, 0.0 to 1.0.
    pub confidence: f64,
    /// True when the parse was uncertain and the item is in the review queue.
    pub needs_review: bool,
    pub comic_info: bool,
    pub progress: Option<ProgressView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderView {
    pub key: String,
    pub library_id: i64,
    pub rel: String,
    pub name: String,
    /// A folder card shows the first readable item below it as its cover.
    pub cover_key: Option<String>,
    /// Items at or below this folder, for the "N volumes" line.
    pub items: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowseView {
    pub kind: Option<MediaKind>,
    pub library: Option<String>,
    pub mature: bool,
    pub crumbs: Vec<Crumb>,
    pub folders: Vec<FolderView>,
    pub items: Vec<ItemView>,
    pub total_folders: usize,
    pub total_items: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub key: String,
    pub title: String,
    pub library: String,
    pub where_: String,
    pub container: ContainerKind,
    pub page_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchView {
    pub query: String,
    pub mode: String,
    pub count: usize,
    pub truncated: bool,
    pub hits: Vec<SearchHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageSetInfo {
    pub key: String,
    pub title: String,
    pub container: ContainerKind,
    pub readable: bool,
    /// Why it is not readable, when it is not.
    pub reason: Option<String>,
    pub count: usize,
    pub rtl: bool,
    pub page_offset: i32,
    pub fit: String,
    /// First few page names, enough to see the ordering is right.
    pub preview: Vec<String>,
    pub progress: Option<ProgressView>,
    pub back_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanItemReport {
    pub library: String,
    pub library_id: i64,
    pub root: String,
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub missing: usize,
    pub dirs: usize,
    pub items: usize,
    pub ms: u128,
    pub skipped: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanReportView {
    pub full: bool,
    pub libraries: Vec<ScanItemReport>,
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub missing: usize,
    pub ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryStatsView {
    pub library_id: i64,
    pub name: String,
    pub kind: MediaKind,
    pub mature: bool,
    pub items: usize,
    pub missing: usize,
    pub dirs: usize,
    pub pages: i64,
    pub bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveView {
    /// Volume prefix, for example `L:` or `/mnt/media`.
    pub volume: String,
    pub online: bool,
    pub libraries: Vec<String>,
    pub items: usize,
    pub bytes: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatsView {
    pub libraries: usize,
    pub items: usize,
    pub missing: usize,
    pub series: usize,
    pub dirs: usize,
    pub pages: i64,
    pub bytes: i64,
    pub per_library: Vec<LibraryStatsView>,
    pub drives: Vec<DriveView>,
    pub last_scan_ms: Option<usize>,
    pub last_scan_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DupGroupView {
    pub size: i64,
    pub hash: String,
    pub paths: Vec<String>,
    pub libraries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicatesView {
    pub groups: Vec<DupGroupView>,
    pub groups_total: usize,
    pub truncated: bool,
    pub files_hashed: usize,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn keys_roundtrip_and_parse() {
        let key = ItemKey::new(3, "Series/Vol 1");
        assert_eq!(key.as_string(), "lib:3|Series/Vol 1");
        assert_eq!(ItemKey::parse(&key.as_string()), Some(key));
        assert_eq!(ItemKey::parse("nonsense"), None);
        assert_eq!(ItemKey::parse("lib:notanumber|x"), None);
        let root = ItemKey::root(7);
        assert_eq!(root.as_string(), "lib:7|");
        assert!(root.is_root());
        assert_eq!(ItemKey::parse("lib:7|"), Some(root.clone()));
        assert_eq!(
            ItemKey::parse("lib:7"),
            Some(root),
            "no pipe means the root"
        );
    }

    #[test]
    fn parent_and_name() {
        let key = ItemKey::new(1, "A/B/c.cbz");
        assert_eq!(key.parent().map(|p| p.rel), Some("A/B".to_string()));
        assert_eq!(key.name(), "c.cbz");
        let top = ItemKey::new(1, "A");
        assert_eq!(top.parent().map(|p| p.rel), Some(String::new()));
    }

    #[test]
    fn media_kinds_parse_and_drive_reading_direction() {
        assert_eq!(MediaKind::parse("Manga"), Some(MediaKind::Manga));
        assert_eq!(MediaKind::parse("comics"), Some(MediaKind::Comics));
        assert_eq!(MediaKind::parse("vinyl"), None);
        assert!(MediaKind::Manga.default_rtl());
        assert!(!MediaKind::Comics.default_rtl());
    }
}
