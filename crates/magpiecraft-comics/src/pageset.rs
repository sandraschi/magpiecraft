//! Page sources: an ordered list of pages plus a way to read one page's bytes.
//!
//! The list is built once when an item is opened; bytes are read on demand. Every
//! open is read-only: archives are extracted to the app cache, never modified, and
//! a broken archive can never damage the collection.

use crate::helpers::Helpers;
use crate::{pdf, rar, ComicError, ComicResult, ContainerKind};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

/// Hard ceiling for one page image. A "page" larger than this is a corrupt entry,
/// not a scan.
pub const MAX_PAGE_BYTES: u64 = 256 * 1024 * 1024;

/// Ordered pages of one volume.
#[derive(Debug, Clone)]
pub struct PageSet {
    path: PathBuf,
    container: ContainerKind,
    pages: Vec<String>,
    helpers: Helpers,
}

impl PageSet {
    /// List the pages of an item. Needs `helpers` for containers that go through an
    /// external tool (7-Zip for RAR, PdfCraft for PDF); without the tool the error
    /// is `not_configured`, not an empty list.
    pub fn open(path: &Path, container: ContainerKind, helpers: &Helpers) -> ComicResult<Self> {
        let pages = match container {
            ContainerKind::Zip => zip_pages(path)?,
            ContainerKind::ImageFolder => folder_pages(path)?,
            ContainerKind::Rar => rar::list(path, helpers)?,
            ContainerKind::Pdf => {
                let count = pdf::count(path, helpers)?;
                (1..=count).map(|n| format!("{n:04}")).collect()
            }
            ContainerKind::File => {
                return Err(ComicError::Invalid(
                    "single files have no page list in this build".to_string(),
                ))
            }
        };
        Ok(PageSet {
            path: path.to_path_buf(),
            container,
            pages,
            helpers: helpers.clone(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn container(&self) -> ContainerKind {
        self.container
    }

    pub fn pages(&self) -> &[String] {
        &self.pages
    }

    pub fn len(&self) -> usize {
        self.pages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    pub fn name(&self, page: usize) -> Option<&str> {
        page.checked_sub(1)
            .and_then(|i| self.pages.get(i))
            .map(String::as_str)
    }

    /// Read one page (1-based) as raw bytes.
    pub fn read(&self, page: usize) -> ComicResult<Vec<u8>> {
        let name = match self.name(page) {
            Some(n) => n.to_string(),
            None => return Err(ComicError::PageRange(page, self.pages.len())),
        };
        match self.container {
            ContainerKind::Zip => {
                let mut archive = open_zip(&self.path)?;
                let mut entry = archive
                    .by_name(&name)
                    .map_err(|e| ComicError::Archive(format!("{name}: {e}")))?;
                let declared = entry.size();
                if declared > MAX_PAGE_BYTES {
                    return Err(ComicError::Archive(format!(
                        "{name}: page is {declared} bytes, over the {MAX_PAGE_BYTES} byte ceiling"
                    )));
                }
                let mut buf = Vec::with_capacity(usize::try_from(declared).unwrap_or(0));
                entry
                    .read_to_end(&mut buf)
                    .map_err(|e| ComicError::Archive(format!("{name}: {e}")))?;
                Ok(buf)
            }
            ContainerKind::ImageFolder => {
                let full = self.path.join(&name);
                std::fs::read(&full).map_err(|e| ComicError::io(&full, e))
            }
            ContainerKind::Rar => rar::read(&self.path, &name, &self.helpers),
            ContainerKind::Pdf => {
                let rendered = pdf::render(&self.path, page, &self.helpers)?;
                std::fs::read(&rendered).map_err(|e| ComicError::io(&rendered, e))
            }
            ContainerKind::File => Err(ComicError::Invalid(
                "single files have no pages".to_string(),
            )),
        }
    }
}

fn open_zip(path: &Path) -> ComicResult<ZipArchive<File>> {
    let file = File::open(path).map_err(|e| ComicError::io(path, e))?;
    ZipArchive::new(file).map_err(|e| ComicError::Archive(e.to_string()))
}

/// Page entries of a ZIP, in natural order, excluding folders, dotfiles and the
/// `__MACOSX` resource fork that macOS adds to zips.
pub fn zip_pages(path: &Path) -> ComicResult<Vec<String>> {
    let mut archive = open_zip(path)?;
    let mut names: Vec<String> = Vec::new();
    for i in 0..archive.len() {
        let entry = match archive.by_index(i) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let raw = entry.name().replace('\\', "/");
        if raw.ends_with('/') || raw.contains("__MACOSX") {
            continue;
        }
        let base = raw.rsplit('/').next().unwrap_or(&raw);
        if base.starts_with('.') {
            continue;
        }
        if !magpiecraft_scan::is_image_name(base) {
            continue;
        }
        names.push(raw);
    }
    names.sort_by_key(|n| magpiecraft_scan::natural_key(n));
    Ok(names)
}

/// Page images directly in a folder, in natural order.
pub fn folder_pages(path: &Path) -> ComicResult<Vec<String>> {
    let mut names: Vec<String> = Vec::new();
    let entries = std::fs::read_dir(path).map_err(|e| ComicError::io(path, e))?;
    for entry in entries.flatten() {
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if magpiecraft_scan::is_image_name(&name) {
            names.push(name);
        }
    }
    names.sort_by_key(|n| magpiecraft_scan::natural_key(n));
    Ok(names)
}

/// Read `ComicInfo.xml` from a ZIP without listing its pages. Returns `None` when
/// the archive has no such entry.
pub fn zip_comic_info(path: &Path) -> ComicResult<Option<crate::ComicInfo>> {
    let mut archive = open_zip(path)?;
    for i in 0..archive.len() {
        let (name, is_dir) = match archive.by_index(i) {
            Ok(e) => (e.name().replace('\\', "/"), e.name().ends_with('/')),
            Err(_) => continue,
        };
        if is_dir {
            continue;
        }
        let base = name.rsplit('/').next().unwrap_or(&name);
        if !base.eq_ignore_ascii_case("comicinfo.xml") {
            continue;
        }
        let mut text = String::new();
        let mut entry = match archive.by_name(&name) {
            Ok(e) => e,
            Err(_) => continue,
        };
        if entry.read_to_string(&mut text).is_err() {
            return Ok(None);
        }
        return Ok(Some(crate::parse_comic_info(&text)));
    }
    Ok(None)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_dir(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "magpie-comics-{}-{}-{}",
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

    fn write_cbz(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        for (name, bytes) in entries {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    fn no_tools() -> Helpers {
        Helpers::new(None, None, std::env::temp_dir().join("magpie-comics-cache"))
    }

    #[test]
    fn zip_pages_are_natural_ordered_and_filtered() {
        let dir = tmp_dir("zip");
        let cbz = dir.join("vol.cbz");
        write_cbz(
            &cbz,
            &[
                ("p10.jpg", b"ten"),
                ("p2.jpg", b"two"),
                ("p1.jpg", b"one"),
                ("__MACOSX/._p1.jpg", b"junk"),
                (".hidden.jpg", b"junk"),
                ("notes.txt", b"junk"),
            ],
        );
        let set = PageSet::open(&cbz, ContainerKind::Zip, &no_tools()).unwrap();
        assert_eq!(set.pages(), &["p1.jpg", "p2.jpg", "p10.jpg"]);
        assert_eq!(set.read(2).unwrap(), b"two");
        assert!(matches!(set.read(0), Err(ComicError::PageRange(0, 3))));
        assert!(matches!(set.read(4), Err(ComicError::PageRange(4, 3))));
        assert_eq!(set.name(1), Some("p1.jpg"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn nested_zip_entries_keep_their_path() {
        let dir = tmp_dir("nested");
        let cbz = dir.join("vol.cbz");
        write_cbz(&cbz, &[("pages/002.jpg", b"b"), ("pages/001.jpg", b"a")]);
        let set = PageSet::open(&cbz, ContainerKind::Zip, &no_tools()).unwrap();
        assert_eq!(set.pages(), &["pages/001.jpg", "pages/002.jpg"]);
        assert_eq!(set.read(1).unwrap(), b"a");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn image_folder_is_a_page_sequence() {
        let dir = tmp_dir("folder");
        for (name, bytes) in [
            ("2.jpg", &b"2"[..]),
            ("1.jpg", &b"1"[..]),
            ("10.png", &b"10"[..]),
        ] {
            std::fs::write(dir.join(name), bytes).unwrap();
        }
        std::fs::write(dir.join("readme.txt"), b"skip").unwrap();
        let set = PageSet::open(&dir, ContainerKind::ImageFolder, &no_tools()).unwrap();
        assert_eq!(set.pages(), &["1.jpg", "2.jpg", "10.png"]);
        assert_eq!(set.read(3).unwrap(), b"10");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn containers_without_their_tool_say_not_configured() {
        let dir = tmp_dir("tools");
        let cbr = dir.join("x.cbr");
        std::fs::write(&cbr, b"Rar!").unwrap();
        match PageSet::open(&cbr, ContainerKind::Rar, &no_tools()) {
            Err(e) => assert_eq!(e.kind(), "not_configured"),
            Ok(_) => panic!("RAR without 7-Zip must not open"),
        }
        let pdf = dir.join("x.pdf");
        std::fs::write(&pdf, b"%PDF-1.4").unwrap();
        match PageSet::open(&pdf, ContainerKind::Pdf, &no_tools()) {
            Err(e) => assert_eq!(e.kind(), "not_configured"),
            Ok(_) => panic!("PDF without PdfCraft must not open"),
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
