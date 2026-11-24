//! Comics and manga engine: what a volume is, how its pages are ordered, what
//! ComicInfo.xml says, and how the reader turns a page number into something on
//! screen.
//!
//! Nothing here touches the database or the UI. Containers are opened read-only
//! and never modified; a page is read on demand so a 400-page archive costs
//! nothing until it is looked at.

mod comicinfo;
mod grouping;
mod helpers;
mod pageset;
pub mod pdf;
pub mod rar;
mod reader;

pub use comicinfo::{parse_comic_info, ComicInfo};
pub use grouping::{parse_volume_name, ParsedName};
pub use helpers::{enforce_cap, Helpers};
pub use pageset::{folder_pages, zip_comic_info, zip_pages, PageSet};
pub use reader::{Crop, FitMode, Reader, ReaderSettings, Slot, Step};

use serde::{Deserialize, Serialize};

pub type ComicResult<T> = Result<T, ComicError>;

#[derive(Debug, thiserror::Error)]
pub enum ComicError {
    #[error("io: {0}: {1}")]
    Io(String, String),
    #[error("archive: {0}")]
    Archive(String),
    #[error("not readable yet: {0}")]
    NotReadable(&'static str),
    #[error("not configured: {0}")]
    NotConfigured(&'static str),
    #[error("page {0} is outside 1..={1}")]
    PageRange(usize, usize),
    #[error("parse: {0}")]
    Parse(String),
    #[error("invalid: {0}")]
    Invalid(String),
}

impl ComicError {
    pub fn kind(&self) -> &'static str {
        match self {
            ComicError::Io(_, _) => "io",
            ComicError::Archive(_) => "archive",
            ComicError::NotReadable(_) => "not_implemented",
            ComicError::NotConfigured(_) => "not_configured",
            ComicError::PageRange(_, _) => "range",
            ComicError::Parse(_) => "parse",
            ComicError::Invalid(_) => "invalid",
        }
    }

    pub fn io(path: &std::path::Path, e: std::io::Error) -> Self {
        ComicError::Io(path.display().to_string(), e.to_string())
    }
}

/// How the pages of one item are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerKind {
    /// CBZ or plain ZIP of images.
    Zip,
    /// CBR, RAR, CB7 or 7Z. Indexed in Tier 1, read from Tier 2 via 7-Zip.
    Rar,
    /// PDF. Indexed in Tier 1, pages rendered by PdfCraft from Tier 2.
    Pdf,
    /// A folder of numbered page images.
    ImageFolder,
    /// Anything else that is a single readable file.
    File,
}

impl ContainerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ContainerKind::Zip => "zip",
            ContainerKind::Rar => "rar",
            ContainerKind::Pdf => "pdf",
            ContainerKind::ImageFolder => "image_folder",
            ContainerKind::File => "file",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "zip" => Some(ContainerKind::Zip),
            "rar" => Some(ContainerKind::Rar),
            "pdf" => Some(ContainerKind::Pdf),
            "image_folder" => Some(ContainerKind::ImageFolder),
            "file" => Some(ContainerKind::File),
            _ => None,
        }
    }

    /// Can this build produce page bytes for this container, assuming its external
    /// tool is installed? ZIP and page folders need nothing; CBR/RAR needs 7-Zip and
    /// PDF needs PdfCraft, and a missing tool is reported as `not_configured`.
    pub fn readable_now(self) -> bool {
        matches!(
            self,
            ContainerKind::Zip
                | ContainerKind::ImageFolder
                | ContainerKind::Rar
                | ContainerKind::Pdf
        )
    }

    /// True when reading this container needs an external binary.
    pub fn needs_tool(self) -> bool {
        matches!(self, ContainerKind::Rar | ContainerKind::Pdf)
    }
}

/// Archive extensions that hold page images.
pub const ZIP_EXTENSIONS: &[&str] = &["cbz", "zip"];
pub const RAR_EXTENSIONS: &[&str] = &["cbr", "rar", "cb7", "7z"];
pub const PDF_EXTENSIONS: &[&str] = &["pdf"];

/// Container for a file name, from its extension. `None` means not a media item.
pub fn container_for_name(name: &str) -> Option<ContainerKind> {
    let ext = magpiecraft_scan::extension(name);
    if ZIP_EXTENSIONS.contains(&ext.as_str()) {
        Some(ContainerKind::Zip)
    } else if RAR_EXTENSIONS.contains(&ext.as_str()) {
        Some(ContainerKind::Rar)
    } else if PDF_EXTENSIONS.contains(&ext.as_str()) {
        Some(ContainerKind::Pdf)
    } else {
        None
    }
}

/// Title shown in the grid: the file stem for archives, the folder name for page
/// folders, matching the prototype.
pub fn display_title(name: &str, container: ContainerKind) -> String {
    if container == ContainerKind::ImageFolder {
        return name.to_string();
    }
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem.to_string(),
        _ => name.to_string(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use magpiecraft_scan::IMAGE_EXTENSIONS;

    #[test]
    fn container_detection_covers_the_supported_extensions() {
        assert_eq!(container_for_name("A.CBZ"), Some(ContainerKind::Zip));
        assert_eq!(container_for_name("a.zip"), Some(ContainerKind::Zip));
        assert_eq!(container_for_name("a.cbr"), Some(ContainerKind::Rar));
        assert_eq!(container_for_name("a.7z"), Some(ContainerKind::Rar));
        assert_eq!(container_for_name("a.pdf"), Some(ContainerKind::Pdf));
        assert_eq!(container_for_name("a.epub"), None);
        assert_eq!(container_for_name("noext"), None);
    }

    #[test]
    fn image_lists_agree_between_inventory_and_comics() {
        // The walker counts images to decide what a page folder is; the archive
        // reader filters entries with the same list. They must not drift.
        for ext in IMAGE_EXTENSIONS {
            assert!(
                magpiecraft_scan::is_image_name(&format!("p.{ext}")),
                "{ext}"
            );
        }
    }

    #[test]
    fn titles_drop_only_the_archive_extension() {
        assert_eq!(
            display_title("Saga 054.cbz", ContainerKind::Zip),
            "Saga 054"
        );
        assert_eq!(display_title("Vol 3", ContainerKind::ImageFolder), "Vol 3");
    }

    #[test]
    fn rar_and_pdf_need_an_external_tool() {
        assert!(ContainerKind::Rar.needs_tool());
        assert!(ContainerKind::Pdf.needs_tool());
        assert!(ContainerKind::Rar.readable_now(), "supported, with 7-Zip");
        assert!(
            ContainerKind::Pdf.readable_now(),
            "supported, with PdfCraft"
        );
        assert!(ContainerKind::Zip.readable_now());
        assert!(!ContainerKind::File.readable_now());
        assert_eq!(
            ComicError::NotConfigured("7-Zip").kind(),
            "not_configured",
            "a missing tool is a configuration fact, not a missing feature"
        );
        assert_eq!(ComicError::NotReadable("x").kind(), "not_implemented");
    }
}
