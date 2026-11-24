//! Organise tests: plans, diffs, apply, undo, quarantine and the review queue.
//!
//! Every fixture is generated here. The point of these tests is the round trip:
//! after apply and undo, files, catalogue rows and reading progress must all be
//! exactly where they started.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use magpiecraft_core::organise::{sanitise_segment, target_for, NamingInput, NamingPreset};
use magpiecraft_core::{Catalog, Paths};
use std::io::Write;
use std::path::{Path, PathBuf};

fn tmp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "magpie-organise-{}-{}-{}",
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

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(w, h, image::Rgba([120, 40, 200, 255]));
    let mut out = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut out);
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut cursor, image::ImageFormat::Png)
        .unwrap();
    out
}

fn write_cbz(path: &Path, pages: usize) {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    let file = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default();
    for i in 1..=pages {
        zip.start_file(format!("{i:03}.png"), opts).unwrap();
        zip.write_all(&png(20, 30)).unwrap();
    }
    zip.finish().unwrap();
}

fn fixture(name: &str) -> (PathBuf, PathBuf) {
    let home = tmp(&format!("{name}-home"));
    let root = tmp(&format!("{name}-library"));
    write_cbz(&root.join("Saga/Saga 054.cbz"), 3);
    write_cbz(&root.join("Saga/Saga 055.cbz"), 3);
    write_cbz(&root.join("Misc/Some Interesting Book.cbz"), 2);
    (home, root)
}

fn catalog(home: &Path) -> Catalog {
    Catalog::open(Paths::at(home)).unwrap()
}

#[test]
fn rename_plan_apply_and_undo_round_trip() {
    let (home, root) = fixture("roundtrip");
    let mut c = catalog(&home);
    c.add_library("Comics", &root.display().to_string(), "comics", false, None)
        .unwrap();
    c.scan(None, false).unwrap();
    let item = c.browse(Some("lib:1|Saga")).unwrap().items.remove(0);
    let old_key = item.key.clone();
    c.progress_set(&old_key, 2, None, false).unwrap();

    let planned = c.plan_rename(Some("Comics"), "comics", 0).unwrap();
    let plans = planned["plans"].as_array().unwrap();
    assert!(!plans.is_empty(), "there is something to rename");
    let plan_id = plans[0]["id"].as_str().unwrap().to_string();
    let diff = plans[0]["diff"].as_str().unwrap();
    assert!(diff.contains("- ") && diff.contains("+ "), "{diff}");

    // Nothing changed yet.
    assert!(root.join("Saga/Saga 054.cbz").is_file());

    // Refuses without confirm.
    let refused = c.apply_plan(&plan_id, false).unwrap_err();
    assert_eq!(refused.kind(), "confirm_required");

    let applied = c.apply_plan(&plan_id, true).unwrap();
    assert!(applied["applied"].as_u64().unwrap() >= 1);
    assert_eq!(applied["failed"].as_u64().unwrap(), 0);
    let journal = applied["journal"].as_str().unwrap().to_string();

    // The old path is gone, the new one exists, and the catalogue followed.
    assert!(!root.join("Saga/Saga 054.cbz").exists());
    let renamed = c
        .browse(Some("lib:1|Saga"))
        .unwrap()
        .items
        .into_iter()
        .find(|i| i.rel.contains("v54"))
        .expect("the renamed item is still browsable");
    assert_ne!(renamed.key, old_key);
    assert!(root.join(&renamed.rel).is_file(), "row path matches disk");
    // Reading progress followed the rename.
    let moved_progress = c.progress_get(&renamed.key).unwrap();
    assert!(moved_progress.is_some(), "progress must follow the file");
    assert_eq!(moved_progress.unwrap().page, 2);
    assert!(c.progress_get(&old_key).unwrap().is_none());

    // Undo refuses without confirm, then puts everything back.
    assert_eq!(
        c.undo(&journal, false).unwrap_err().kind(),
        "confirm_required"
    );
    let undone = c.undo(&journal, true).unwrap();
    assert!(undone["restored"].as_u64().unwrap() >= 1);
    assert!(root.join("Saga/Saga 054.cbz").is_file(), "the file is back");
    let restored = c.browse(Some("lib:1|Saga")).unwrap().items;
    assert!(restored.iter().any(|i| i.rel == "Saga/Saga 054.cbz"));
    assert_eq!(c.progress_get(&old_key).unwrap().unwrap().page, 2);
    // A journal can only be undone once.
    assert_eq!(c.undo(&journal, true).unwrap_err().kind(), "invalid");

    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_rename_never_overwrites_an_existing_file() {
    let (home, root) = fixture("collision");
    // Occupy the target name of one item before planning.
    std::fs::create_dir_all(root.join("Saga")).unwrap();
    std::fs::write(root.join("Saga/Saga v54.cbz"), b"fake").unwrap();
    let mut c = catalog(&home);
    c.add_library("Comics", &root.display().to_string(), "comics", false, None)
        .unwrap();
    c.scan(None, false).unwrap();
    let planned = c.plan_rename(Some("Comics"), "comics", 0).unwrap();
    let plans = planned["plans"].as_array().unwrap().clone();
    let all_skipped: Vec<String> = plans
        .iter()
        .flat_map(|p| {
            p["diff"]
                .as_str()
                .unwrap()
                .lines()
                .filter(|l| l.contains("Saga 054"))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        all_skipped.iter().any(|s| s.contains("already exists")),
        "the collision is reported, not silently overwritten: {all_skipped:?}"
    );
    assert!(root.join("Saga/Saga 054.cbz").is_file());
    assert_eq!(
        std::fs::read(root.join("Saga/Saga v54.cbz")).unwrap(),
        b"fake"
    );
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn quarantine_moves_the_file_and_undo_returns_it() {
    let (home, root) = fixture("quarantine");
    let mut c = catalog(&home);
    c.add_library("Comics", &root.display().to_string(), "comics", false, None)
        .unwrap();
    c.scan(None, false).unwrap();
    let item = c.browse(Some("lib:1|Misc")).unwrap().items.remove(0);
    let bytes_before = std::fs::read(root.join(&item.rel)).unwrap();

    let planned = c.plan_quarantine(&item.key).unwrap();
    let plan_id = planned["plans"][0]["id"].as_str().unwrap().to_string();
    let applied = c.apply_plan(&plan_id, true).unwrap();
    assert_eq!(applied["failed"].as_u64().unwrap(), 0);
    assert!(!root.join(&item.rel).exists(), "moved out of the library");
    let quarantine = home.join("quarantine");
    let mut found: Option<PathBuf> = None;
    for entry in walk(&quarantine) {
        if entry.ends_with("Some Interesting Book.cbz") {
            found = Some(entry);
        }
    }
    let quarantined = found.expect("the file is in quarantine");
    assert_eq!(std::fs::read(&quarantined).unwrap(), bytes_before);

    let journal = applied["journal"].as_str().unwrap().to_string();
    c.undo(&journal, true).unwrap();
    assert!(root.join(&item.rel).is_file(), "undo brings it back");
    assert_eq!(std::fs::read(root.join(&item.rel)).unwrap(), bytes_before);
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&current) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    out.push(path);
                }
            }
        }
    }
    out
}

#[test]
fn the_review_queue_only_lists_uncertain_names() {
    let (home, root) = fixture("review");
    write_cbz(&root.join("Misc/Untitled Scan 198512.cbz"), 2);
    let mut c = catalog(&home);
    c.add_library("Comics", &root.display().to_string(), "comics", false, None)
        .unwrap();
    c.scan(None, false).unwrap();
    let queue = c.review(None, 50).unwrap();
    let entries = queue["entries"].as_array().unwrap();
    assert!(
        entries
            .iter()
            .any(|e| e["rel"].as_str().unwrap().contains("Untitled Scan")),
        "an unparseable name is listed: {queue}"
    );
    assert!(
        entries
            .iter()
            .all(|e| e["confidence"].as_f64().unwrap() < 0.6),
        "certain names stay out of the queue"
    );
    assert_eq!(queue["total_in_queue"], serde_json::json!(entries.len()));
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn golden_plan_diff_for_a_known_library() {
    let (home, root) = fixture("golden");
    let mut c = catalog(&home);
    c.add_library("Comics", &root.display().to_string(), "comics", false, None)
        .unwrap();
    c.scan(None, false).unwrap();
    let planned = c.plan_rename(Some("Comics"), "comics", 0).unwrap();
    let diff = planned["plans"][0]["diff"].as_str().unwrap();
    // The exact expected diff: deterministic rule output, absolute paths.
    let root_text = root.display().to_string();
    let expected_first = format!("- {root_text}\\Saga\\Saga 054.cbz");
    let expected_second = format!("+ {root_text}\\Saga\\Saga v54.cbz");
    assert!(diff.contains(&expected_first), "{diff}");
    assert!(diff.contains(&expected_second), "{diff}");
    drop(c);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn naming_rules_are_pure_and_predictable() {
    let input = NamingInput {
        rel: "Saga/Saga 054.cbz".to_string(),
        title: "Saga 054".to_string(),
        series: Some("Saga".to_string()),
        volume: Some("54".to_string()),
        year: Some(2018),
        edition: None,
        is_folder: false,
    };
    assert_eq!(
        target_for(&input, NamingPreset::Comics).as_deref(),
        Some("Saga/Saga v54 (2018).cbz")
    );
    let mut edition = input.clone();
    edition.edition = Some("de".to_string());
    assert_eq!(
        target_for(&edition, NamingPreset::Manga).as_deref(),
        Some("Saga/Saga v54 (2018) [de].cbz")
    );
    assert_eq!(sanitise_segment("a/b:c"), "a_b_c");
}
