//! The command bus.
//!
//! Every capability is one `Command` variant and one arm of `run`. The CLI, the
//! MCP server and the desktop app all build the same variants, so their surfaces
//! cannot drift apart. Errors are encoded here, at the boundary, as
//! `{"ok": false, "error": ..., "error_type": ...}`; callers never see a Rust
//! error type and never have to parse a message to decide what happened.

use crate::catalog::{Catalog, SearchMode};
use crate::error::{MagpieError, MagpieResult};
use crate::model::ItemKey;
use serde_json::{json, Map, Value};

#[derive(Debug, Clone)]
pub enum Command {
    Status,
    Tools,
    Sources,
    Libraries,
    LibraryAdd {
        name: String,
        root: String,
        kind: String,
        mature: bool,
        rtl_default: Option<bool>,
    },
    LibraryRemove {
        name: String,
        confirm: bool,
    },
    Scan {
        library: Option<String>,
        full: bool,
    },
    Browse {
        key: Option<String>,
    },
    Search {
        query: String,
        limit: usize,
        mode: SearchMode,
    },
    Item {
        key: String,
    },
    Pages {
        key: String,
    },
    PageOffset {
        key: String,
        offset: i32,
    },
    /// Write one page of an item to a file. Read-only on the collection.
    PageExport {
        key: String,
        page: usize,
        out: String,
    },
    /// Hand an item to the player or the OS handler, with its resume position.
    OpenItem {
        key: String,
    },
    Rtl {
        key: String,
        rtl: Option<bool>,
    },
    ProgressGet {
        key: String,
    },
    ProgressSet {
        key: String,
        page: usize,
        total: Option<usize>,
        finished: bool,
    },
    Continue {
        limit: usize,
    },
    Stats,
    Duplicates {
        library: Option<String>,
        limit: usize,
    },
    Hash {
        key: String,
    },
    ManifestWrite {
        library: String,
        out: Option<String>,
    },
    ManifestVerify {
        library: String,
        manifest: Option<String>,
    },
    Series {
        library: Option<String>,
    },
    SeriesRtl {
        library: String,
        series: String,
        rtl: Option<bool>,
    },
    Inspect {
        path: String,
    },
    Settings,
    SettingsSet {
        mature_visible: Option<bool>,
        imgdir_min_pages: Option<usize>,
        theme: Option<String>,
        zoom: Option<f32>,
    },
    /// Items whose filename parse was uncertain.
    Review {
        library: Option<String>,
        limit: usize,
    },
    /// What checksum verification is due, per library.
    VerifyStatus {
        every_days: i64,
    },
    /// Verify the libraries that are due (the first run writes the baseline).
    VerifyRun {
        library: Option<String>,
        every_days: i64,
        force: bool,
    },
    /// Where each library stands against the configured backup roots.
    BackupStatus,
    /// Set or clear the backup roots.
    BackupRoots {
        roots: Vec<String>,
        clear: bool,
    },
    /// Near duplicates: perceptual images, and videos with the same title and year.
    NearDuplicates {
        limit: usize,
    },
    /// Subtitle sidecars and embedded tracks for one item.
    Subs {
        key: String,
    },
    /// Shift a subtitle file by seconds into a new file.
    SubsShift {
        path: String,
        seconds: f64,
        out: Option<String>,
    },
    /// Convert a subtitle file to srt or vtt into a new file.
    SubsConvert {
        path: String,
        to: String,
        out: Option<String>,
    },
    /// Extract an embedded subtitle stream with ffmpeg.
    SubsExtract {
        key: String,
        subtitle_index: u32,
        out: String,
    },
    /// The ffmpeg preset catalogue.
    Presets,
    /// Build (but do not apply) a conversion plan for one item.
    PlanConvert {
        key: String,
        preset: String,
        out: Option<String>,
    },
    /// Build (but do not apply) a verified backup copy plan.
    PlanBackup {
        library: Option<String>,
        to: String,
        verify: bool,
        limit: usize,
    },
    /// Build (but do not apply) a rename plan.
    PlanRename {
        library: Option<String>,
        preset: String,
        limit: usize,
    },
    /// Build (but do not apply) a quarantine plan for one item.
    PlanQuarantine {
        key: String,
    },
    /// Build (but do not apply) a cleanup plan: empty folders, orphans, samples, junk.
    PlanCleanup {
        library: Option<String>,
    },
    Plans,
    /// Apply a saved plan. Requires confirm.
    ApplyPlan {
        id: String,
        confirm: bool,
    },
    Journals,
    /// Reverse an applied plan. Requires confirm.
    Undo {
        id: String,
        confirm: bool,
    },
}

impl Command {
    /// Stable id used in tests, logs and the command palette.
    pub fn id(&self) -> &'static str {
        match self {
            Command::Status => "status",
            Command::Tools => "tools",
            Command::Sources => "sources",
            Command::Libraries => "libraries",
            Command::LibraryAdd { .. } => "library.add",
            Command::LibraryRemove { .. } => "library.remove",
            Command::Scan { .. } => "scan",
            Command::Browse { .. } => "browse",
            Command::Search { .. } => "search",
            Command::Item { .. } => "item",
            Command::Pages { .. } => "pages",
            Command::PageOffset { .. } => "page_offset",
            Command::PageExport { .. } => "page_export",
            Command::OpenItem { .. } => "open",
            Command::Rtl { .. } => "rtl",
            Command::ProgressGet { .. } => "progress.get",
            Command::ProgressSet { .. } => "progress.set",
            Command::Continue { .. } => "continue",
            Command::Stats => "stats",
            Command::Duplicates { .. } => "duplicates",
            Command::Hash { .. } => "hash",
            Command::ManifestWrite { .. } => "manifest.write",
            Command::ManifestVerify { .. } => "manifest.verify",
            Command::Series { .. } => "series",
            Command::SeriesRtl { .. } => "series.rtl",
            Command::Inspect { .. } => "inspect",
            Command::Settings => "settings",
            Command::SettingsSet { .. } => "settings.set",
            Command::Review { .. } => "review",
            Command::VerifyStatus { .. } => "verify.status",
            Command::VerifyRun { .. } => "verify.run",
            Command::BackupStatus => "backup.status",
            Command::BackupRoots { .. } => "backup.roots",
            Command::NearDuplicates { .. } => "dupes.near",
            Command::PlanBackup { .. } => "backup.plan",
            Command::Subs { .. } => "subs",
            Command::SubsShift { .. } => "subs.shift",
            Command::SubsConvert { .. } => "subs.convert",
            Command::SubsExtract { .. } => "subs.extract",
            Command::Presets => "presets",
            Command::PlanConvert { .. } => "convert.plan",
            Command::PlanRename { .. } => "plan.rename",
            Command::PlanQuarantine { .. } => "plan.quarantine",
            Command::PlanCleanup { .. } => "plan.cleanup",
            Command::Plans => "plans",
            Command::ApplyPlan { .. } => "apply",
            Command::Journals => "journals",
            Command::Undo { .. } => "undo",
        }
    }
}

/// Run one command and encode the outcome. Never panics, never returns `Err`.
pub fn dispatch(catalog: &mut Catalog, cmd: Command) -> Value {
    let id = cmd.id();
    match run(catalog, cmd) {
        Ok(value) => match value {
            Value::Object(mut map) => {
                map.insert("ok".to_string(), Value::Bool(true));
                map.insert("command".to_string(), Value::String(id.to_string()));
                Value::Object(map)
            }
            other => json!({ "ok": true, "command": id, "value": other }),
        },
        Err(e) => {
            let mut map = Map::new();
            map.insert("ok".to_string(), Value::Bool(false));
            map.insert("command".to_string(), Value::String(id.to_string()));
            map.insert("error".to_string(), Value::String(e.to_string()));
            map.insert(
                "error_type".to_string(),
                Value::String(e.kind().to_string()),
            );
            Value::Object(map)
        }
    }
}

fn to_value<T: serde::Serialize>(value: T) -> MagpieResult<Value> {
    serde_json::to_value(value).map_err(|e| MagpieError::invalid(format!("serialize: {e}")))
}

fn run(catalog: &mut Catalog, cmd: Command) -> MagpieResult<Value> {
    match cmd {
        Command::Status => catalog.status(),
        Command::Tools => Ok(catalog.tools_report()),
        Command::Sources => Ok(catalog.source_registry()),
        Command::Libraries => to_value(catalog.libraries()?),
        Command::LibraryAdd {
            name,
            root,
            kind,
            mature,
            rtl_default,
        } => to_value(catalog.add_library(&name, &root, &kind, mature, rtl_default)?),
        Command::LibraryRemove { name, confirm } => catalog.remove_library(&name, confirm),
        Command::Scan { library, full } => to_value(catalog.scan(library.as_deref(), full)?),
        Command::Browse { key } => to_value(catalog.browse(key.as_deref())?),
        Command::Search { query, limit, mode } => to_value(catalog.search(&query, limit, mode)?),
        Command::Item { key } => to_value(catalog.item(&key)?),
        Command::Pages { key } => to_value(catalog.pages(&key)?),
        Command::PageOffset { key, offset } => catalog.set_page_offset(&key, offset),
        Command::PageExport { key, page, out } => catalog.export_page(&key, page, &out),
        Command::OpenItem { key } => catalog.open_item(&key),
        Command::Rtl { key, rtl } => catalog.set_item_rtl(&key, rtl),
        Command::ProgressGet { key } => to_value(catalog.progress_get(&key)?),
        Command::ProgressSet {
            key,
            page,
            total,
            finished,
        } => to_value(catalog.progress_set(&key, page, total, finished)?),
        Command::Continue { limit } => catalog.continue_reading(limit),
        Command::Stats => to_value(catalog.stats()?),
        Command::Duplicates { library, limit } => {
            to_value(catalog.duplicates(library.as_deref(), limit)?)
        }
        Command::Hash { key } => catalog.hash_item(&key),
        Command::ManifestWrite { library, out } => catalog.manifest_write(&library, out.as_deref()),
        Command::ManifestVerify { library, manifest } => {
            catalog.manifest_verify(&library, manifest.as_deref())
        }
        Command::Series { library } => to_value(catalog.series_list(library.as_deref())?),
        Command::SeriesRtl {
            library,
            series,
            rtl,
        } => catalog.series_set_rtl(&library, &series, rtl),
        Command::Inspect { path } => catalog.inspect(&path),
        Command::Settings => to_value(catalog.settings().clone()),
        Command::SettingsSet {
            mature_visible,
            imgdir_min_pages,
            theme,
            zoom,
        } => {
            let mut settings = catalog.settings().clone();
            if let Some(v) = mature_visible {
                settings.mature_visible = v;
            }
            if let Some(v) = imgdir_min_pages {
                settings.imgdir_min_pages = v;
            }
            if let Some(v) = theme {
                settings.theme = v;
            }
            if let Some(v) = zoom {
                settings.zoom = v;
            }
            to_value(catalog.set_settings(settings)?)
        }
        Command::Review { library, limit } => catalog.review(library.as_deref(), limit),
        Command::VerifyStatus { every_days } => catalog.verify_status(every_days),
        Command::VerifyRun {
            library,
            every_days,
            force,
        } => catalog.verify_run(library.as_deref(), every_days, force),
        Command::BackupStatus => catalog.backup_status(),
        Command::BackupRoots { roots, clear } => catalog.set_backup_roots(&roots, clear),
        Command::NearDuplicates { limit } => catalog.near_duplicates(limit),
        Command::PlanBackup {
            library,
            to,
            verify,
            limit,
        } => catalog.plan_backup(library.as_deref(), &to, verify, limit),
        Command::Subs { key } => catalog.subs_list(&key),
        Command::SubsShift { path, seconds, out } => {
            catalog.subs_shift(&path, seconds, out.as_deref())
        }
        Command::SubsConvert { path, to, out } => catalog.subs_convert(&path, &to, out.as_deref()),
        Command::SubsExtract {
            key,
            subtitle_index,
            out,
        } => catalog.subs_extract(&key, subtitle_index, &out),
        Command::Presets => Ok(catalog.presets()),
        Command::PlanConvert { key, preset, out } => {
            catalog.plan_convert(&key, &preset, out.as_deref())
        }
        Command::PlanRename {
            library,
            preset,
            limit,
        } => catalog.plan_rename(library.as_deref(), &preset, limit),
        Command::PlanQuarantine { key } => catalog.plan_quarantine(&key),
        Command::PlanCleanup { library } => catalog.plan_cleanup(library.as_deref()),
        Command::Plans => catalog.plans(),
        Command::ApplyPlan { id, confirm } => catalog.apply_plan(&id, confirm),
        Command::Journals => catalog.journals(),
        Command::Undo { id, confirm } => catalog.undo(&id, confirm),
    }
}

/// Names accepted by the CLI and MCP for a search mode.
pub fn parse_search_mode(s: &str) -> MagpieResult<SearchMode> {
    SearchMode::parse(s).ok_or_else(|| {
        MagpieError::invalid(format!(
            "unknown search mode {s}: use auto, fts or substring"
        ))
    })
}

/// Split a key argument, accepting either `lib:3|rel` or a bare relative path
/// when a library id was supplied separately.
pub fn key_with_library(key: &str, library_id: Option<i64>) -> MagpieResult<String> {
    if ItemKey::parse(key).is_some() {
        return Ok(key.to_string());
    }
    match library_id {
        Some(id) => Ok(ItemKey::new(id, key).as_string()),
        None => Err(MagpieError::invalid(format!(
            "not an item key: {key} (expected lib:<id>|<path>)"
        ))),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::config::Paths;

    fn catalog() -> Catalog {
        let home = std::env::temp_dir().join(format!(
            "magpie-cmd-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        Catalog::open_in_memory(Paths::at(home)).unwrap()
    }

    #[test]
    fn every_result_carries_ok_and_the_command_id() {
        let mut c = catalog();
        let status = dispatch(&mut c, Command::Status);
        assert_eq!(status["ok"], json!(true));
        assert_eq!(status["command"], json!("status"));
        assert_eq!(status["app"], json!("magpiecraft"));
        let libs = dispatch(&mut c, Command::Libraries);
        assert_eq!(libs["ok"], json!(true));
        assert!(
            libs["value"].is_array(),
            "a list result is wrapped under value"
        );
    }

    #[test]
    fn errors_are_encoded_with_a_kind_and_never_panic() {
        let mut c = catalog();
        let missing = dispatch(
            &mut c,
            Command::Item {
                key: "lib:9|nope.cbz".to_string(),
            },
        );
        assert_eq!(missing["ok"], json!(false));
        assert_eq!(missing["error_type"], json!("not_found"));
        let bad_key = dispatch(
            &mut c,
            Command::Pages {
                key: "nonsense".to_string(),
            },
        );
        assert_eq!(bad_key["error_type"], json!("invalid"));
        let remove = dispatch(
            &mut c,
            Command::LibraryRemove {
                name: "x".to_string(),
                confirm: false,
            },
        );
        assert_eq!(remove["error_type"], json!("confirm_required"));
    }

    #[test]
    fn commands_have_stable_ids() {
        assert_eq!(
            Command::Search {
                query: "a".into(),
                limit: 1,
                mode: SearchMode::Auto
            }
            .id(),
            "search"
        );
        assert_eq!(
            Command::SeriesRtl {
                library: "l".into(),
                series: "s".into(),
                rtl: Some(true)
            }
            .id(),
            "series.rtl"
        );
    }

    #[test]
    fn search_mode_parsing_is_strict() {
        assert_eq!(parse_search_mode("auto").unwrap(), SearchMode::Auto);
        assert_eq!(parse_search_mode("fts").unwrap(), SearchMode::Fts);
        assert!(parse_search_mode("fuzzy").is_err());
        assert_eq!(
            key_with_library("a/b.cbz", Some(2)).unwrap(),
            "lib:2|a/b.cbz"
        );
        assert!(key_with_library("a/b.cbz", None).is_err());
    }
}
