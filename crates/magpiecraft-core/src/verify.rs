//! Checksum verification schedule (pillar C).
//!
//! A manifest is only useful if it is checked again later. There is no daemon in
//! this product, so "the schedule" is two things: a report of which libraries are
//! due (manifest older than N days, or never written), and a run that writes the
//! first manifest as a baseline and verifies against it afterwards. Every run is
//! recorded, so the next report knows when it last happened.

use crate::catalog::Catalog;
use crate::error::{MagpieError, MagpieResult};
use chrono::{DateTime, Utc};
use magpiecraft_health as health;
use magpiecraft_store::VerificationRow;
use serde_json::{json, Value};
use std::path::PathBuf;

/// Default re-verification interval, in days. Once a month is plenty for a media
/// library on healthy disks; bitrot is slow, and hashing 5 TB is not free.
pub const DEFAULT_EVERY_DAYS: i64 = 30;

fn days_since(stamp: &str) -> Option<i64> {
    let then = DateTime::parse_from_rfc3339(stamp).ok()?;
    Some((Utc::now() - then.with_timezone(&Utc)).num_days())
}

impl Catalog {
    /// Per-library verification state: what the last run found, how long ago it was,
    /// and whether it is due again.
    pub fn verify_status(&self, every_days: i64) -> MagpieResult<Value> {
        let every_days = if every_days <= 0 {
            DEFAULT_EVERY_DAYS
        } else {
            every_days
        };
        let libs = self.store().libraries()?;
        let mut entries: Vec<Value> = Vec::new();
        let mut due = 0usize;
        for lib in &libs {
            let manifest = health::manifest_path(&self.paths().manifests_dir(), &lib.name);
            let last = self.store().last_verification(lib.id)?;
            let age = last.as_ref().and_then(|v| days_since(&v.checked_at));
            let is_due = match age {
                Some(days) => days >= every_days,
                None => true,
            };
            if is_due {
                due += 1;
            }
            entries.push(json!({
                "library": lib.name,
                "library_id": lib.id,
                "manifest": manifest.display().to_string(),
                "manifest_exists": manifest.is_file(),
                "last_checked_at": last.as_ref().map(|v| v.checked_at.clone()),
                "days_since_check": age,
                "last_checked": last.as_ref().map(|v| v.checked),
                "last_ok": last.as_ref().map(|v| v.ok),
                "last_missing": last.as_ref().map(|v| v.missing),
                "last_changed": last.as_ref().map(|v| v.changed),
                "last_was_baseline": last.as_ref().map(|v| v.baseline),
                "due": is_due,
            }));
        }
        Ok(json!({
            "every_days": every_days,
            "libraries": entries.len(),
            "due": due,
            "entries": entries,
            "note": "verify_run checks the due libraries; the first run writes the baseline",
        }))
    }

    /// Verify the libraries that are due, or one named library with `force`.
    ///
    /// The first run for a library writes the manifest (a baseline: it proves
    /// nothing yet, it records the state now) and says so. Later runs compare.
    pub fn verify_run(
        &mut self,
        library: Option<&str>,
        every_days: i64,
        force: bool,
    ) -> MagpieResult<Value> {
        let every_days = if every_days <= 0 {
            DEFAULT_EVERY_DAYS
        } else {
            every_days
        };
        let libs = match library {
            Some(key) => vec![self.target_libraries(Some(key))?.remove(0)],
            None => self.store().libraries()?,
        };
        let mut results: Vec<Value> = Vec::new();
        let mut checked_total = 0usize;
        let mut problems = 0usize;
        let mut skipped = 0usize;
        for lib in libs {
            let last = self.store().last_verification(lib.id)?;
            let due = force
                || match last.as_ref().and_then(|v| days_since(&v.checked_at)) {
                    Some(days) => days >= every_days,
                    None => true,
                };
            if !due {
                skipped += 1;
                continue;
            }
            let root = PathBuf::from(&lib.root);
            if !root.is_dir() {
                results.push(json!({
                    "library": lib.name,
                    "skipped": true,
                    "note": "root is not reachable",
                }));
                continue;
            }
            let manifest = health::manifest_path(&self.paths().manifests_dir(), &lib.name);
            let started = std::time::Instant::now();
            let baseline = !manifest.is_file();
            let row = if baseline {
                let report = health::write_manifest(&root, &manifest)?;
                VerificationRow {
                    library_id: lib.id,
                    checked_at: now(),
                    checked: report.files,
                    ok: report.files,
                    missing: 0,
                    changed: 0,
                    baseline: true,
                    ms: usize::try_from(started.elapsed().as_millis()).unwrap_or(usize::MAX),
                }
            } else {
                let report = health::verify_manifest(&root, &manifest)?;
                VerificationRow {
                    library_id: lib.id,
                    checked_at: now(),
                    checked: report.checked,
                    ok: report.ok,
                    missing: report.missing.len(),
                    changed: report.changed.len(),
                    baseline: false,
                    ms: usize::try_from(started.elapsed().as_millis()).unwrap_or(usize::MAX),
                }
            };
            checked_total += row.checked;
            if row.missing > 0 || row.changed > 0 {
                problems += row.missing + row.changed;
            }
            self.store_mut().record_verification(&row)?;
            results.push(json!({
                "library": lib.name,
                "baseline": row.baseline,
                "checked": row.checked,
                "ok": row.ok,
                "missing": row.missing,
                "changed": row.changed,
                "ms": row.ms,
                "manifest": manifest.display().to_string(),
            }));
        }
        if results.is_empty() && skipped > 0 {
            return Err(MagpieError::invalid(format!(
                "nothing is due (checked within {every_days} days); pass force=true to verify anyway"
            )));
        }
        Ok(json!({
            "ok": true,
            "every_days": every_days,
            "libraries_checked": results.len(),
            "libraries_skipped": skipped,
            "files_checked": checked_total,
            "problems": problems,
            "results": results,
            "note": if problems > 0 {
                "problems found: re-download or restore those files, then verify again"
            } else {
                "every listed file matched its checksum"
            },
        }))
    }
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}
