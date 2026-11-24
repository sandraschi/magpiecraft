//! Synthetic scan benchmark.
//!
//! Builds a tree of empty `.jpg` files under the data home (never inside a real
//! library), indexes it, and reports the walk and database timings. Empty files
//! are enough: the walker and the store are what is being measured, not decoding.

use magpiecraft_core::{Catalog, Paths};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Files per page folder. 200 images is a typical manga chapter folder.
const PER_FOLDER: usize = 200;

pub fn run(paths: Paths, files: usize, dir: Option<&str>) -> Result<Value, String> {
    let files = files.clamp(1, 2_000_000);
    let root = match dir {
        Some(d) if !d.trim().is_empty() => PathBuf::from(d),
        _ => paths.home().join("bench").join("tree"),
    };
    let started = Instant::now();
    let mut created = 0usize;
    if !root.is_dir() {
        create_tree(&root, files).map_err(|e| e.to_string())?;
        created = files;
    }
    let create_ms = started.elapsed().as_millis();

    let bench_home = paths.home().join("bench").join("catalogue");
    let mut catalog = Catalog::open(Paths::at(bench_home)).map_err(|e| e.to_string())?;
    let root_text = root.display().to_string();
    let exists = catalog
        .libraries()
        .map_err(|e| e.to_string())?
        .iter()
        .any(|l| l.name == "bench");
    if !exists {
        catalog
            .add_library("bench", &root_text, "manga", false, None)
            .map_err(|e| e.to_string())?;
    }

    let scan_started = Instant::now();
    let report = catalog
        .scan(Some("bench"), false)
        .map_err(|e| e.to_string())?;
    let scan_ms = scan_started.elapsed().as_millis();

    let per_second = if scan_ms > 0 {
        (files as f64) / (scan_ms as f64 / 1000.0)
    } else {
        0.0
    };

    Ok(json!({
        "ok": true,
        "benchmark": "synthetic-scan",
        "root": root_text,
        "files": files,
        "files_created_now": created,
        "create_ms": create_ms,
        "scan_ms": scan_ms,
        "files_per_second": per_second.round(),
        "added": report.added,
        "updated": report.updated,
        "unchanged": report.unchanged,
        "missing": report.missing,
        "items": report.libraries.iter().map(|l| l.items).sum::<usize>(),
        "dirs": report.libraries.iter().map(|l| l.dirs).sum::<usize>(),
        "note": "empty files; measures the walker plus SQLite inserts, not image decoding",
    }))
}

fn create_tree(root: &Path, files: usize) -> std::io::Result<()> {
    let folders = files.div_ceil(PER_FOLDER).max(1);
    for f in 0..folders {
        let dir = root.join("Series").join(format!("Vol {:04}", f + 1));
        std::fs::create_dir_all(&dir)?;
        for i in 0..PER_FOLDER {
            if f * PER_FOLDER + i >= files {
                break;
            }
            std::fs::File::create(dir.join(format!("{:04}.jpg", i + 1)))?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_small_benchmark_runs_and_reports_numbers() {
        let home = std::env::temp_dir().join(format!(
            "magpie-bench-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let value = run(Paths::at(home.clone()), 600, None).unwrap();
        assert_eq!(value["ok"], json!(true));
        assert_eq!(value["files"], json!(600));
        assert_eq!(value["items"], json!(3), "three page folders of 200");
        std::fs::remove_dir_all(&home).unwrap();
    }
}
