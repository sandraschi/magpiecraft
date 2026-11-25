//! Backup status (pillar C, 3-2-1).
//!
//! Read-only: it compares each library with the copies found under the configured
//! backup roots and says what is missing. It never copies anything here; a copy is a
//! plan with a journal, like every other mutation, and lands in the next tier.
//!
//! "Complete" means the backup holds at least as many files as the library does. That
//! catches a backup that was never started, one that was interrupted, and one that is
//! a stale copy from before the last import. It is a count, not a checksum; manifests
//! and `verify_run` are what prove integrity.

use crate::catalog::Catalog;
use crate::error::MagpieResult;
use serde_json::{json, Value};
use std::path::Path;

impl Catalog {
    /// Where each library stands against every configured backup root.
    pub fn backup_status(&self) -> MagpieResult<Value> {
        let libs = self.store().libraries()?;
        let roots = self.settings().backup_roots.clone();
        let mut entries: Vec<Value> = Vec::new();
        let (mut complete, mut partial, mut absent) = (0usize, 0usize, 0usize);
        for lib in &libs {
            let items = self.store().count_items_under(lib.id, "")?;
            let mut copies: Vec<Value> = Vec::new();
            for root in &roots {
                let target = Path::new(root).join(&lib.name);
                if !Path::new(root).is_dir() {
                    copies.push(json!({
                        "root": root,
                        "path": target.display().to_string(),
                        "reachable": false,
                        "files": 0,
                        "bytes": 0,
                        "complete": false,
                        "note": "backup root is not reachable",
                    }));
                    continue;
                }
                let walk = magpiecraft_scan::walk(
                    &target,
                    &magpiecraft_scan::WalkOptions {
                        min_pages: 3,
                        follow_links: false,
                        limit: 0,
                    },
                )
                .ok();
                let files = walk.as_ref().map(|w| w.files.len()).unwrap_or(0);
                let bytes = walk.as_ref().map(|w| w.bytes).unwrap_or(0);
                let is_complete = items > 0 && files >= items;
                copies.push(json!({
                    "root": root,
                    "path": target.display().to_string(),
                    "reachable": true,
                    "exists": target.is_dir(),
                    "files": files,
                    "bytes": bytes,
                    "complete": is_complete,
                    "missing": items.saturating_sub(files),
                }));
            }
            let best = copies
                .iter()
                .filter(|c| c.get("complete").and_then(Value::as_bool) == Some(true))
                .count();
            let any = copies
                .iter()
                .filter(|c| c.get("files").and_then(Value::as_u64).unwrap_or(0) > 0)
                .count();
            let status = if roots.is_empty() {
                "unconfigured"
            } else if best > 0 {
                complete += 1;
                "complete"
            } else if any > 0 {
                partial += 1;
                "partial"
            } else {
                absent += 1;
                "not backed up"
            };
            entries.push(json!({
                "library": lib.name,
                "library_id": lib.id,
                "items": items,
                "status": status,
                "copies": copies,
            }));
        }
        Ok(json!({
            "roots": roots,
            "libraries": libs.len(),
            "complete": complete,
            "partial": partial,
            "not_backed_up": absent,
            "unconfigured": roots.is_empty(),
            "entries": entries,
            "note": if roots.is_empty() {
                "set a backup root with backup_roots (or set-settings --backup-root) to see what is backed up"
            } else {
                "a count, not a checksum: use verify_run for integrity"
            },
        }))
    }

    /// Replace (or append to) the backup roots.
    pub fn set_backup_roots(&mut self, roots: &[String], clear: bool) -> MagpieResult<Value> {
        let mut settings = self.settings().clone();
        if clear {
            settings.backup_roots.clear();
        }
        for root in roots {
            let trimmed = root.trim();
            if trimmed.is_empty() {
                continue;
            }
            let existing = settings
                .backup_roots
                .iter()
                .any(|r| r.eq_ignore_ascii_case(trimmed));
            if !existing {
                settings.backup_roots.push(trimmed.to_string());
            }
        }
        let saved = self.set_settings(settings)?;
        Ok(json!({
            "backup_roots": saved.backup_roots,
            "count": saved.backup_roots.len(),
        }))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use crate::{Catalog, Paths};
    use std::path::PathBuf;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "magpie-backup-{}-{}-{}",
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
    fn backup_status_counts_copies_and_names_what_is_missing() {
        let home = tmp("home");
        let library = tmp("library");
        let backup = tmp("backup");
        // Two volumes in the library, one copied.
        std::fs::write(library.join("a.cbz"), b"one").unwrap();
        std::fs::write(library.join("b.cbz"), b"two").unwrap();
        std::fs::create_dir_all(backup.join("Comics")).unwrap();
        std::fs::write(backup.join("Comics/a.cbz"), b"one").unwrap();

        let mut c = Catalog::open(Paths::at(home.clone())).unwrap();
        c.add_library(
            "Comics",
            &library.display().to_string(),
            "comics",
            false,
            None,
        )
        .unwrap();
        c.scan(None, false).unwrap();

        // No roots configured: honest "unconfigured", not a fake pass.
        let status = c.backup_status().unwrap();
        assert_eq!(status["unconfigured"], serde_json::json!(true));

        c.set_backup_roots(&[backup.display().to_string()], false)
            .unwrap();
        let status = c.backup_status().unwrap();
        let entry = &status["entries"][0];
        assert_eq!(entry["items"], serde_json::json!(2));
        assert_eq!(entry["status"], serde_json::json!("partial"));
        assert_eq!(entry["copies"][0]["files"], serde_json::json!(1));
        assert_eq!(entry["copies"][0]["missing"], serde_json::json!(1));

        // Copy the second: complete.
        std::fs::write(backup.join("Comics/b.cbz"), b"two").unwrap();
        let status = c.backup_status().unwrap();
        assert_eq!(
            status["entries"][0]["status"],
            serde_json::json!("complete")
        );
        assert_eq!(status["complete"], serde_json::json!(1));
        drop(c);
        std::fs::remove_dir_all(&home).unwrap();
        std::fs::remove_dir_all(&library).unwrap();
        std::fs::remove_dir_all(&backup).unwrap();
    }

    #[test]
    fn an_unreachable_root_is_reported_not_guessed() {
        let home = tmp("home2");
        let library = tmp("library2");
        std::fs::write(library.join("a.cbz"), b"one").unwrap();
        let mut c = Catalog::open(Paths::at(home.clone())).unwrap();
        c.add_library(
            "Comics",
            &library.display().to_string(),
            "comics",
            false,
            None,
        )
        .unwrap();
        c.scan(None, false).unwrap();
        c.set_backup_roots(&["Z:/not-a-drive".to_string()], false)
            .unwrap();
        let status = c.backup_status().unwrap();
        let copy = &status["entries"][0]["copies"][0];
        assert_eq!(copy["reachable"], serde_json::json!(false));
        assert_eq!(
            status["entries"][0]["status"],
            serde_json::json!("not backed up")
        );
        drop(c);
        std::fs::remove_dir_all(&home).unwrap();
        std::fs::remove_dir_all(&library).unwrap();
    }
}
