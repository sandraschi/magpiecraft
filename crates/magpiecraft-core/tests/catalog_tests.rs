//! End-to-end catalogue tests over synthetic archives and page folders.
//!
//! Nothing here is copyrighted: every page is a generated gradient, every archive
//! is built in the test, and every library lives in the OS temp directory.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use magpiecraft_core::{Catalog, ContainerKind, MediaKind, Paths, SearchMode};
use std::io::Write;
use std::path::{Path, PathBuf};

fn tmp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "magpie-it-{}-{}-{}",
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

fn png(w: u32, h: u32, tint: u8) -> Vec<u8> {
    let img = image::RgbaImage::from_fn(w, h, |x, y| {
        image::Rgba([(x % 256) as u8, (y % 256) as u8, tint, 255])
    });
    let mut out = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut out);
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut cursor, image::ImageFormat::Png)
        .unwrap();
    out
}

fn write_cbz(path: &Path, pages: usize, comic_info: Option<&str>) {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    let file = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default();
    for i in 1..=pages {
        zip.start_file(format!("{i:03}.png"), opts).unwrap();
        zip.write_all(&png(40, 60, (i * 30) as u8)).unwrap();
    }
    if let Some(xml) = comic_info {
        zip.start_file("ComicInfo.xml", opts).unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

fn image_folder(dir: &Path, pages: usize) {
    std::fs::create_dir_all(dir).unwrap();
    for i in 1..=pages {
        std::fs::write(dir.join(format!("{i:03}.jpg")), png(40, 60, i as u8)).unwrap();
    }
}

/// A library with two series folders, four archives, one page folder and one
/// ComicInfo-carrying manga volume.
fn fixture(name: &str) -> (PathBuf, PathBuf) {
    let home = tmp(&format!("{name}-home"));
    let root = tmp(&format!("{name}-library"));
    write_cbz(&root.join("Saga/Saga 054.cbz"), 5, None);
    write_cbz(&root.join("Saga/Saga 055.cbz"), 6, None);
    write_cbz(
        &root.join("Berserk/Berserk v01.cbz"),
        4,
        Some(
            "<ComicInfo><Series>Berserk</Series><Number>1</Number><Year>1989</Year>\
             <Writer>Kentaro Miura</Writer><PageCount>4</PageCount></ComicInfo>",
        ),
    );
    // A folder of loose pages, read as a volume.
    image_folder(&root.join("Yotsuba/Yotsuba v05"), 8);
    // A loose archive directly in the root, to exercise series from the file name.
    write_cbz(&root.join("Loose Issue 003.cbz"), 3, None);
    (home, root)
}

fn catalog(home: &Path) -> Catalog {
    Catalog::open(Paths::at(home)).unwrap()
}

#[test]
fn scan_indexes_archives_and_page_folders() {
    let (home, root) = fixture("scan");
    let mut c = catalog(&home);
    c.add_library("Comics", &root.display().to_string(), "comics", false, None)
        .unwrap();
    let report = c.scan(None, false).unwrap();
    assert_eq!(report.added, 5, "four archives and one page folder");
    assert_eq!(report.missing, 0);
    let browse = c.browse(Some("lib:1|")).unwrap();
    assert_eq!(browse.total_folders, 3, "Saga, Berserk, Yotsuba");
    assert_eq!(browse.total_items, 1, "the loose archive in the root");
    assert_eq!(browse.crumbs.len(), 2);
    // The libraries overview counts what is below each root.
    let overview = c.browse(None).unwrap();
    assert_eq!(overview.folders.len(), 1);
    assert_eq!(
        overview.folders[0].items, 5,
        "five readable items below the root"
    );
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn second_scan_is_incremental_and_third_sees_a_change() {
    let (home, root) = fixture("incremental");
    let mut c = catalog(&home);
    c.add_library("Comics", &root.display().to_string(), "comics", false, None)
        .unwrap();
    let first = c.scan(None, false).unwrap();
    assert_eq!(first.added, 5);
    let second = c.scan(None, false).unwrap();
    assert_eq!(second.added, 0);
    assert_eq!(second.updated, 0);
    assert_eq!(
        second.unchanged, 5,
        "size and mtime match, so nothing is reopened"
    );
    // Replace one archive with a larger one.
    write_cbz(&root.join("Saga/Saga 054.cbz"), 9, None);
    let third = c.scan(None, false).unwrap();
    assert_eq!(third.updated, 1);
    assert_eq!(third.unchanged, 4);
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn deleted_files_are_marked_missing_not_dropped() {
    let (home, root) = fixture("missing");
    let mut c = catalog(&home);
    c.add_library("Comics", &root.display().to_string(), "comics", false, None)
        .unwrap();
    c.scan(None, false).unwrap();
    std::fs::remove_file(root.join("Saga/Saga 055.cbz")).unwrap();
    let report = c.scan(None, false).unwrap();
    assert_eq!(report.missing, 1);
    let browse = c.browse(Some("lib:1|Saga")).unwrap();
    assert_eq!(
        browse.total_items, 1,
        "the missing volume is hidden from the grid"
    );
    let stats = c.stats().unwrap();
    assert_eq!(stats.items, 4);
    assert_eq!(stats.missing, 1);
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn page_counts_come_from_the_archive_and_the_folder() {
    let (home, root) = fixture("pages");
    let mut c = catalog(&home);
    c.add_library("Comics", &root.display().to_string(), "comics", false, None)
        .unwrap();
    c.scan(None, false).unwrap();
    let saga = c
        .browse(Some("lib:1|Saga"))
        .unwrap()
        .items
        .into_iter()
        .find(|i| i.title == "Saga 054")
        .unwrap();
    assert_eq!(saga.page_count, Some(5));
    assert_eq!(saga.container, ContainerKind::Zip);
    assert!(saga.readable);
    let yotsuba = c
        .browse(Some("lib:1|Yotsuba"))
        .unwrap()
        .items
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(yotsuba.container, ContainerKind::ImageFolder);
    assert_eq!(yotsuba.page_count, Some(8));
    let pages = c.pages(&yotsuba.key).unwrap();
    assert_eq!(pages.count, 8);
    assert_eq!(pages.preview.len(), 3);
    assert!(pages.preview[0].ends_with("001.jpg"));
    assert!(pages.readable);
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn comicinfo_supplies_series_credits_and_direction() {
    let (home, root) = fixture("comicinfo");
    let mut c = catalog(&home);
    c.add_library("Manga", &root.display().to_string(), "manga", false, None)
        .unwrap();
    c.scan(None, false).unwrap();
    let berserk = c
        .browse(Some("lib:1|Berserk"))
        .unwrap()
        .items
        .into_iter()
        .next()
        .unwrap();
    assert!(berserk.comic_info);
    assert_eq!(berserk.series.as_deref(), Some("Berserk"));
    assert_eq!(berserk.volume.as_deref(), Some("1"));
    assert!(berserk.rtl, "a manga library reads right to left");
    let series = c.series_list(Some("Manga")).unwrap();
    assert_eq!(
        series.len(),
        4,
        "Saga, Berserk, Yotsuba and the loose issue"
    );
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn search_finds_words_across_series_and_paths() {
    let (home, root) = fixture("search");
    let mut c = catalog(&home);
    c.add_library("Comics", &root.display().to_string(), "comics", false, None)
        .unwrap();
    c.scan(None, false).unwrap();
    let fts = c.search("berserk", 20, SearchMode::Fts).unwrap();
    assert_eq!(fts.count, 1);
    assert_eq!(fts.mode, "fts");
    let substring = c.search("saga 05", 20, SearchMode::Substring).unwrap();
    assert_eq!(substring.count, 2, "Saga 054 and Saga 055");
    let auto = c.search("yotsuba", 20, SearchMode::Auto).unwrap();
    assert!(auto.count >= 1);
    let nothing = c.search("zzzzz", 20, SearchMode::Auto).unwrap();
    assert_eq!(nothing.count, 0);
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn progress_roundtrip_and_continue_row() {
    let (home, root) = fixture("progress");
    let mut c = catalog(&home);
    c.add_library("Comics", &root.display().to_string(), "comics", false, None)
        .unwrap();
    c.scan(None, false).unwrap();
    let item = c.browse(Some("lib:1|Saga")).unwrap().items.remove(0);
    let saved = c.progress_set(&item.key, 3, None, false).unwrap();
    assert_eq!(saved.page, 3);
    assert!(saved.total > 0, "total falls back to the page count");
    let read_back = c.progress_get(&item.key).unwrap().unwrap();
    assert_eq!(read_back.page, 3);
    let again = c.browse(Some("lib:1|Saga")).unwrap();
    let with_progress = again.items.iter().find(|i| i.key == item.key).unwrap();
    assert_eq!(with_progress.progress.as_ref().unwrap().page, 3);
    let continuing = c.continue_reading(10).unwrap();
    assert_eq!(continuing["count"], serde_json::json!(1));
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn page_offset_and_item_direction_are_remembered() {
    let (home, root) = fixture("offset");
    let mut c = catalog(&home);
    c.add_library("Comics", &root.display().to_string(), "comics", false, None)
        .unwrap();
    c.scan(None, false).unwrap();
    let item = c.browse(Some("lib:1|Saga")).unwrap().items.remove(0);
    c.set_page_offset(&item.key, 2).unwrap();
    c.set_item_rtl(&item.key, Some(true)).unwrap();
    let after = c.item(&item.key).unwrap();
    assert_eq!(after.page_offset, 2);
    assert!(after.rtl);
    let pages = c.pages(&item.key).unwrap();
    assert_eq!(pages.page_offset, 2);
    assert!(pages.rtl);
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn series_direction_override_beats_the_library_default() {
    let (home, root) = fixture("seriesrtl");
    let mut c = catalog(&home);
    c.add_library("Comics", &root.display().to_string(), "comics", false, None)
        .unwrap();
    c.scan(None, false).unwrap();
    let item = c.browse(Some("lib:1|Saga")).unwrap().items.remove(0);
    assert!(!item.rtl, "a comics library reads left to right by default");
    c.series_set_rtl("Comics", "Saga", Some(true)).unwrap();
    let after = c.item(&item.key).unwrap();
    assert!(after.rtl, "the series override wins");
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn covers_and_page_images_are_rendered_into_the_cache_only() {
    let (home, root) = fixture("cover");
    let mut c = catalog(&home);
    c.add_library("Comics", &root.display().to_string(), "comics", false, None)
        .unwrap();
    c.scan(None, false).unwrap();
    let item = c.browse(Some("lib:1|Saga")).unwrap().items.remove(0);
    let cover = c.cover_path(&item.key).unwrap();
    assert!(cover.is_file());
    assert!(cover.starts_with(home.join("cache")));
    assert!(!cover.starts_with(&root), "never write into a library");
    let folder_cover = c.cover_path("lib:1|Saga").unwrap();
    assert!(
        folder_cover.is_file(),
        "a folder uses its first item as cover"
    );
    let image = c.page_image(&item.key, 2, 80, 120).unwrap();
    assert!(image.width() <= 80 && image.height() <= 120);
    assert!(image.width() > 0);
    // The library is untouched.
    assert!(root.join("Saga/Saga 054.cbz").is_file());
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn exact_duplicates_are_found_by_hash() {
    let (home, root) = fixture("dupes");
    let copy = std::fs::read(root.join("Saga/Saga 054.cbz")).unwrap();
    std::fs::write(root.join("Saga/Saga 054 copy.cbz"), &copy).unwrap();
    std::fs::write(root.join("Saga/Saga 055 different.cbz"), &copy).unwrap();
    let mut c = catalog(&home);
    c.add_library("Comics", &root.display().to_string(), "comics", false, None)
        .unwrap();
    c.scan(None, false).unwrap();
    let dupes = c.duplicates(None, 50).unwrap();
    assert_eq!(
        dupes.groups_total, 1,
        "three identical files make one group"
    );
    assert_eq!(dupes.groups[0].paths.len(), 3);
    assert!(dupes.files_hashed >= 3);
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn manifest_write_and_verify_detect_bitrot() {
    let (home, root) = fixture("manifest");
    let mut c = catalog(&home);
    c.add_library("Comics", &root.display().to_string(), "comics", false, None)
        .unwrap();
    c.scan(None, false).unwrap();
    let written = c.manifest_write("Comics", None).unwrap();
    assert!(written["files"].as_u64().unwrap() >= 5);
    let manifest = written["manifest"].as_str().unwrap().to_string();
    assert!(Path::new(&manifest).is_file());
    let clean = c.manifest_verify("Comics", None).unwrap();
    assert_eq!(clean["clean"], serde_json::json!(true));
    // Corrupt one archive in place.
    let mut bytes = std::fs::read(root.join("Saga/Saga 055.cbz")).unwrap();
    if let Some(last) = bytes.last_mut() {
        *last ^= 0xFF;
    }
    std::fs::write(root.join("Saga/Saga 055.cbz"), &bytes).unwrap();
    let dirty = c.manifest_verify("Comics", None).unwrap();
    assert_eq!(dirty["clean"], serde_json::json!(false));
    assert_eq!(dirty["changed"].as_array().unwrap().len(), 1);
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn an_unreachable_root_keeps_its_index() {
    let (home, root) = fixture("offline");
    let mut c = catalog(&home);
    c.add_library("Comics", &root.display().to_string(), "comics", false, None)
        .unwrap();
    c.scan(None, false).unwrap();
    let moved = root.with_extension("unplugged");
    std::fs::rename(&root, &moved).unwrap();
    let report = c.scan(None, false).unwrap();
    assert!(report.libraries[0].skipped);
    assert!(report.libraries[0]
        .note
        .as_deref()
        .unwrap()
        .contains("not reachable"));
    let stats = c.stats().unwrap();
    assert_eq!(
        stats.items, 5,
        "an unplugged drive does not lose the catalogue"
    );
    assert!(!stats.drives.is_empty());
    std::fs::rename(&moved, &root).unwrap();
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn unreadable_containers_say_so_instead_of_showing_nothing() {
    let (home, root) = fixture("rar");
    std::fs::write(root.join("Saga/Saga 056.cbr"), b"Rar! not really").unwrap();
    std::fs::write(root.join("Saga/Saga 057.pdf"), b"%PDF-1.4 not really").unwrap();
    let mut c = catalog(&home);
    // No tools: the containers are supported by the build but not readable here.
    c.set_tools(magpiecraft_core::Toolchain::default());
    c.add_library("Comics", &root.display().to_string(), "comics", false, None)
        .unwrap();
    c.scan(None, false).unwrap();
    let items = c.browse(Some("lib:1|Saga")).unwrap().items;
    let cbr = items.iter().find(|i| i.title.contains("056")).unwrap();
    assert_eq!(cbr.container, ContainerKind::Rar);
    assert!(!cbr.readable, "without 7-Zip a CBR is not readable");
    let pages = c.pages(&cbr.key).unwrap();
    assert!(!pages.readable);
    assert!(pages.reason.as_deref().unwrap().contains("7-Zip"));
    let err = c.page_set(&cbr.key).unwrap_err();
    assert_eq!(err.kind(), "not_configured");
    let pdf = items.iter().find(|i| i.title.contains("057")).unwrap();
    assert!(!pdf.readable, "without PdfCraft a PDF is not readable");
    assert!(c
        .pages(&pdf.key)
        .unwrap()
        .reason
        .as_deref()
        .unwrap()
        .contains("PdfCraft"));
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

/// End to end on the RAR path: 7-Zip reads a ZIP renamed to .cbr, so the test can
/// build the fixture without a RAR encoder. Skipped when 7-Zip is not installed.
#[test]
fn cbr_pages_are_listed_and_read_through_seven_zip() {
    let tools = magpiecraft_core::Toolchain::detect();
    let Some(seven) = tools.seven_zip.clone() else {
        eprintln!("7-Zip not installed: skipping the CBR end-to-end test");
        return;
    };
    let (home, root) = fixture("cbr");
    let cbz = root.join("Saga/Saga 054.cbz");
    std::fs::copy(&cbz, root.join("Saga/Saga 058.cbr")).unwrap();
    let mut c = catalog(&home);
    let forced = magpiecraft_core::Toolchain {
        seven_zip: Some(seven),
        ..Default::default()
    };
    c.set_tools(forced);
    c.add_library("Comics", &root.display().to_string(), "comics", false, None)
        .unwrap();
    c.scan(None, false).unwrap();
    let items = c.browse(Some("lib:1|Saga")).unwrap().items;
    let cbr = items.iter().find(|i| i.title.contains("058")).unwrap();
    assert!(cbr.readable, "7-Zip is present, so the CBR is readable");
    assert_eq!(
        cbr.page_count,
        Some(5),
        "page count comes from the index read"
    );
    let pages = c.pages(&cbr.key).unwrap();
    assert!(pages.readable);
    assert_eq!(pages.count, 5);
    assert_eq!(pages.preview.first().map(String::as_str), Some("001.png"));
    let image = c.page_image(&cbr.key, 2, 80, 120).unwrap();
    assert!(image.width() > 0 && image.height() > 0);
    // The archive itself is untouched and the extraction went to the cache.
    assert!(root.join("Saga/Saga 058.cbr").is_file());
    let cache = home.join("cache").join("extract");
    assert!(cache.is_dir(), "extraction happens under the app cache");
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn libraries_can_be_added_listed_and_removed_with_confirmation() {
    let (home, root) = fixture("libs");
    let mut c = catalog(&home);
    let lib = c
        .add_library("Comics", &root.display().to_string(), "comics", false, None)
        .unwrap();
    assert_eq!(lib.kind, MediaKind::Comics);
    assert!(c
        .add_library("Comics", &root.display().to_string(), "comics", false, None)
        .is_err());
    assert!(c
        .add_library("Bad", "Z:/definitely/not/here", "comics", false, None)
        .is_err());
    c.scan(None, false).unwrap();
    assert!(c.remove_library("Comics", false).is_err());
    let removed = c.remove_library("Comics", true).unwrap();
    assert_eq!(removed["removed"], serde_json::json!("Comics"));
    assert!(c.libraries().unwrap().is_empty());
    assert!(
        root.join("Saga/Saga 054.cbz").is_file(),
        "files are never deleted"
    );
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn mature_libraries_stay_hidden_until_enabled() {
    let (home, root) = fixture("mature");
    let mut c = catalog(&home);
    c.add_library("Adults", &root.display().to_string(), "comics", true, None)
        .unwrap();
    c.scan(None, false).unwrap();
    let overview = c.browse(None).unwrap();
    assert!(overview.folders.is_empty(), "hidden by default");
    assert!(c.browse(Some("lib:1|")).is_err());
    let mut settings = c.settings().clone();
    settings.mature_visible = true;
    c.set_settings(settings).unwrap();
    let overview = c.browse(None).unwrap();
    assert_eq!(overview.folders.len(), 1);
    assert!(c.browse(Some("lib:1|")).is_ok());
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn page_export_writes_the_raw_page_outside_the_library() {
    let (home, root) = fixture("export");
    let mut c = catalog(&home);
    c.add_library("Comics", &root.display().to_string(), "comics", false, None)
        .unwrap();
    c.scan(None, false).unwrap();
    let item = c.browse(Some("lib:1|Saga")).unwrap().items.remove(0);
    let out = home.join("exported/page-2.png");
    let report = c
        .export_page(&item.key, 2, &out.display().to_string())
        .unwrap();
    assert_eq!(report["page"], serde_json::json!(2));
    assert_eq!(report["page_name"], serde_json::json!("002.png"));
    let bytes = std::fs::read(&out).unwrap();
    assert_eq!(bytes.len() as u64, report["bytes"].as_u64().unwrap());
    assert_eq!(&bytes[1..4], b"PNG", "the page is copied byte for byte");
    assert!(out.starts_with(home.join("exported")));
    assert!(!root.join("exported").exists(), "never inside the library");
    assert!(c
        .export_page(&item.key, 999, &out.display().to_string())
        .is_err());
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn every_readable_file_becomes_an_item_of_its_kind() {
    use magpiecraft_core::catalog::classify_item;
    // A film is a video in any library.
    assert_eq!(
        classify_item("movie.mkv", MediaKind::Comics),
        Some((ContainerKind::File, MediaKind::Video))
    );
    assert_eq!(
        classify_item("track.flac", MediaKind::Photo),
        Some((ContainerKind::File, MediaKind::Audio))
    );
    assert_eq!(
        classify_item("book.epub", MediaKind::Video),
        Some((ContainerKind::File, MediaKind::Ebook))
    );
    // A photo is an item in a photo library, a page elsewhere.
    assert_eq!(
        classify_item("shot.jpg", MediaKind::Photo),
        Some((ContainerKind::File, MediaKind::Photo))
    );
    assert_eq!(classify_item("page.jpg", MediaKind::Comics), None);
    // A comic archive is an item in a comics library, not in a music one.
    assert_eq!(
        classify_item("vol.cbz", MediaKind::Comics),
        Some((ContainerKind::Zip, MediaKind::Comics))
    );
    assert_eq!(classify_item("vol.cbz", MediaKind::Audio), None);
    // A PDF takes its kind from the library.
    assert_eq!(
        classify_item("scan.pdf", MediaKind::Manga),
        Some((ContainerKind::Pdf, MediaKind::Manga))
    );
    assert_eq!(
        classify_item("paper.pdf", MediaKind::Ebook),
        Some((ContainerKind::Pdf, MediaKind::Ebook))
    );
    // Subtitles and metadata files are never items.
    assert_eq!(classify_item("movie.srt", MediaKind::Video), None);
    assert_eq!(classify_item("metadata.opf", MediaKind::Ebook), None);
}

#[test]
fn a_video_library_is_catalogued_as_video() {
    let home = tmp("video-home");
    let root = tmp("video-library");
    std::fs::write(
        root.join("Despicable Me Minion Madness 2010.mp4"),
        b"not really a film",
    )
    .unwrap();
    std::fs::write(root.join("Zootopia .2 2025.mkv"), b"nor this").unwrap();
    std::fs::write(
        root.join("movie.srt"),
        b"1\n00:00:01,000 --> 00:00:02,000\nHi\n",
    )
    .unwrap();
    std::fs::write(root.join("poster.jpg"), b"not indexed in a video library").unwrap();
    let mut c = catalog(&home);
    c.add_library("Video", &root.display().to_string(), "video", false, None)
        .unwrap();
    c.scan(None, false).unwrap();
    let items = c.browse(Some("lib:1|")).unwrap().items;
    assert_eq!(items.len(), 2, "two films, no subtitle, no poster");
    assert!(items.iter().all(|i| i.kind == MediaKind::Video));
    assert!(!items[0].readable, "video is not read as pages here");
    let pages = c.pages(&items[0].key).unwrap();
    assert_eq!(pages.count, 0);
    assert!(
        pages.reason.as_deref().unwrap().contains("mpv"),
        "the reason names a real player: {:?}",
        pages.reason
    );
    let stats = c.stats().unwrap();
    let video = stats
        .per_kind
        .iter()
        .find(|k| k.kind == MediaKind::Video)
        .expect("video totals");
    assert_eq!(video.items, 2);
    assert!(video.bytes > 0);
    // The film preset renames by title and year, in place.
    let planned = c.plan_rename(Some("Video"), "auto", 0).unwrap();
    let diff = planned["plans"][0]["diff"].as_str().unwrap();
    assert!(
        diff.contains("Despicable Me Minion Madness (2010).mp4"),
        "{diff}"
    );
    assert!(diff.contains("Zootopia 2 (2025).mkv"), "{diff}");
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn opening_a_film_records_it_even_when_the_player_is_missing() {
    use magpiecraft_core::{Player, PlayerKind};
    let home = tmp("open-home");
    let root = tmp("open-library");
    std::fs::write(root.join("Alien 1979.mkv"), b"not really a film").unwrap();
    let mut c = catalog(&home);
    c.add_library("Video", &root.display().to_string(), "video", false, None)
        .unwrap();
    c.scan(None, false).unwrap();
    let item = c.browse(Some("lib:1|")).unwrap().items.remove(0);

    // A player that is not there: the launch fails, loudly, but the attempt is
    // still recorded so the film shows up in "continue" rather than vanishing.
    c.set_player_override(Some(Player {
        kind: PlayerKind::Vlc,
        exe: Some(home.join("no-such-player.exe")),
    }));
    c.set_position(&item.key, 90).unwrap();
    let err = c.open_item(&item.key).unwrap_err();
    assert_eq!(err.kind(), "tool");
    assert!(err.to_string().contains("no-such-player"), "{err}");

    let continuing = c.continue_reading(10).unwrap();
    let entry = continuing["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["key"] == serde_json::json!(item.key))
        .expect("the film is in the continue row");
    assert_eq!(entry["kind"], serde_json::json!("video"));
    assert_eq!(entry["label"], serde_json::json!("1:30"));
    assert_eq!(entry["position_secs"], serde_json::json!(90));
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn comics_are_read_in_the_app_not_handed_to_a_player() {
    let (home, root) = fixture("no-open");
    let mut c = catalog(&home);
    c.add_library("Comics", &root.display().to_string(), "comics", false, None)
        .unwrap();
    c.scan(None, false).unwrap();
    let item = c.browse(Some("lib:1|Saga")).unwrap().items.remove(0);
    let err = c.open_item(&item.key).unwrap_err();
    assert_eq!(err.kind(), "invalid");
    assert!(err.to_string().contains("reader"), "{err}");
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn status_reports_the_engine_and_the_optional_tools() {
    let (home, root) = fixture("status");
    let mut c = catalog(&home);
    c.add_library("Comics", &root.display().to_string(), "comics", false, None)
        .unwrap();
    c.scan(None, false).unwrap();
    let status = c.status().unwrap();
    assert_eq!(status["app"], serde_json::json!("magpiecraft"));
    assert_eq!(status["libraries"], serde_json::json!(1));
    assert_eq!(status["items"], serde_json::json!(5));
    assert!(status["tools"]["ffprobe"]["found"].is_boolean());
    assert!(status["database"]
        .as_str()
        .unwrap()
        .contains("catalogue.db"));
    let sources = c.source_registry();
    assert!(sources["sources"].as_array().unwrap().len() >= 5);
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}
