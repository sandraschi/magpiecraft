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
    /// List bundled skills, or read one skill body by name.
    SkillRead {
        name: Option<String>,
    },
    /// Redacted LLM config: provider, model, endpoint, key presence. No network.
    LlmStatus,
    /// Live model list for one provider (curated names where no list API exists).
    LlmListModels {
        provider: Option<String>,
    },
    /// Persist the provider and model, switching Ollama VRAM to the pick.
    LlmSelect {
        provider: String,
        model: String,
    },
    /// Store one provider key locally. The key is never echoed back.
    LlmSetKey {
        provider: String,
        api_key: String,
        clear: bool,
    },
    /// Evict resident Ollama models, optionally keeping one warm.
    LlmEvict {
        keep: Option<String>,
    },
    /// One live completion through the selected provider. Ok only on a reply.
    LlmTest {
        prompt: Option<String>,
    },
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
        skin: Option<String>,
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
    /// Generate an SRT sidecar with whisperx/whisper on this machine.
    SubsGenerate {
        key: String,
        model: Option<String>,
        lang: Option<String>,
        out: Option<String>,
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
    /// Physical disks the libraries sit on, with health warnings.
    DriveHealth,
}

impl Command {
    /// Stable id used in tests, logs and the command palette.
    pub fn id(&self) -> &'static str {
        match self {
            Command::Status => "status",
            Command::Tools => "tools",
            Command::Sources => "sources",
            Command::SkillRead { .. } => "skill.read",
            Command::LlmStatus => "llm.status",
            Command::LlmListModels { .. } => "llm.list_models",
            Command::LlmSelect { .. } => "llm.select",
            Command::LlmSetKey { .. } => "llm.set_key",
            Command::LlmEvict { .. } => "llm.evict",
            Command::LlmTest { .. } => "llm.test",
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
            Command::SubsGenerate { .. } => "subs.generate",
            Command::Presets => "presets",
            Command::PlanConvert { .. } => "convert.plan",
            Command::PlanRename { .. } => "plan.rename",
            Command::PlanQuarantine { .. } => "plan.quarantine",
            Command::PlanCleanup { .. } => "plan.cleanup",
            Command::Plans => "plans",
            Command::ApplyPlan { .. } => "apply",
            Command::Journals => "journals",
            Command::Undo { .. } => "undo",
            Command::DriveHealth => "drive.health",
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

/// Redacted LLM status: provider, model, endpoint and key presence flags.
/// No network, so Settings and agents can call it freely. The key values
/// never leave the config file.
fn llm_status() -> MagpieResult<Value> {
    let config = crate::llm::load_config();
    let provider = config.provider_enum();
    Ok(json!({
        "provider": provider.id(),
        "provider_label": provider.label(),
        "model": config.model,
        "endpoint": config.endpoint(),
        "local": provider.local(),
        "keys_present": config.keys_present().into_iter().map(|(id, present)| {
            json!({"provider": id, "present": present})
        }).collect::<Vec<_>>(),
        "tools_enabled": config.tools_enabled,
        "save_transcripts": config.save_transcripts,
        "config_file": crate::llm::config_path(),
    }))
}

/// Live model list for one provider (or the configured one). Anthropic and
/// Gemini have no list API and return their curated names.
fn llm_list_models(provider: Option<&str>) -> MagpieResult<Value> {
    use crate::llm::{
        probe_ollama, probe_openai_models, LlmProvider, ANTHROPIC_MODELS, GEMINI_MODELS,
    };
    let name = provider.unwrap_or("").trim();
    let selected = if name.is_empty() {
        crate::llm::load_config().provider_enum()
    } else {
        LlmProvider::parse(name)
            .ok_or_else(|| MagpieError::invalid(format!("unknown provider: {name}")))?
    };
    if !selected.has_list_api() {
        let curated = match selected {
            LlmProvider::Anthropic => ANTHROPIC_MODELS,
            _ => GEMINI_MODELS,
        };
        return Ok(json!({
            "provider": selected.id(),
            "live": false,
            "models": curated.iter().map(|m| json!({"name": m})).collect::<Vec<_>>(),
        }));
    }
    let full = crate::llm::load_config();
    let models = match selected {
        LlmProvider::Ollama => probe_ollama(&full)?,
        _ => {
            let endpoint = match selected {
                LlmProvider::LmStudio => full.lmstudio_endpoint.clone(),
                _ => full.endpoint(),
            };
            probe_openai_models(&endpoint)?
        }
    };
    Ok(json!({
        "provider": selected.id(),
        "live": true,
        "models": models.iter().map(|m| {
            json!({"name": m.name, "size_gb": m.size_gb, "resident": m.resident})
        }).collect::<Vec<_>>(),
    }))
}

/// Persist the provider and model, switching Ollama VRAM to the pick.
/// An empty model refuses: there is no fallback model.
fn llm_select(provider: &str, model: &str) -> MagpieResult<Value> {
    use crate::llm::{load_config, save_config, switch_ollama_model, LlmProvider};
    let selected = LlmProvider::parse(provider)
        .ok_or_else(|| MagpieError::invalid(format!("unknown provider: {provider}")))?;
    if model.trim().is_empty() {
        return Err(MagpieError::invalid(
            "model is empty: discovery lists installed models",
        ));
    }
    let mut config = load_config();
    config.provider = selected.id().to_string();
    config.model = model.trim().to_string();
    save_config(&config).map_err(MagpieError::llm)?;
    let vram = if selected == LlmProvider::Ollama {
        switch_ollama_model(&config, config.model.trim()).map_err(crate::MagpieError::from)?
    } else {
        json!({"evicted": [], "warmed": model.trim(), "warm_ok": true})
    };
    Ok(json!({
        "provider": selected.id(),
        "model": config.model,
        "vram": vram,
    }))
}

/// Store one provider key locally. The reply carries presence, never the key.
/// `clear` forgets the stored key. Empty input without clear keeps storage.
fn llm_set_key(provider: &str, api_key: &str, clear: bool) -> MagpieResult<Value> {
    use crate::llm::{load_config, save_config, LlmProvider};
    let selected = LlmProvider::parse(provider)
        .ok_or_else(|| MagpieError::invalid(format!("unknown provider: {provider}")))?;
    if selected.local() {
        return Err(MagpieError::invalid(format!(
            "{} is local and takes no key",
            selected.id()
        )));
    }
    let mut config = load_config();
    let slot = match selected {
        LlmProvider::OpenAi => &mut config.openai_key,
        LlmProvider::Anthropic => &mut config.anthropic_key,
        LlmProvider::DeepSeek => &mut config.deepseek_key,
        LlmProvider::Meta => &mut config.meta_key,
        LlmProvider::OpenRouter => &mut config.openrouter_key,
        LlmProvider::Custom => &mut config.custom_key,
        LlmProvider::Gemini => &mut config.gemini_key,
        _ => return Err(MagpieError::invalid("no key slot for a local provider")),
    };
    if clear {
        slot.clear();
    } else if !api_key.trim().is_empty() {
        *slot = api_key.trim().to_string();
    }
    save_config(&config).map_err(MagpieError::llm)?;
    Ok(json!({
        "provider": selected.id(),
        "key_present": !config.resolve_key(selected).is_empty(),
    }))
}

/// Evict resident Ollama models, optionally keeping one.
fn llm_evict(keep: Option<&str>) -> MagpieResult<Value> {
    use crate::llm::{load_config, switch_ollama_model};
    let config = load_config();
    switch_ollama_model(&config, keep.unwrap_or("").trim()).map_err(crate::MagpieError::from)
}

/// One live completion through the selected provider. Ok only on a reply.
fn llm_test(prompt: Option<&str>) -> MagpieResult<Value> {
    use crate::llm::{load_config, test_completion};
    let config = load_config();
    test_completion(&config, prompt).map_err(crate::MagpieError::from)
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
        Command::SkillRead { name } => crate::skills::read(name.as_deref()),
        Command::LlmStatus => llm_status(),
        Command::LlmListModels { provider } => llm_list_models(provider.as_deref()),
        Command::LlmSelect { provider, model } => llm_select(&provider, &model),
        Command::LlmSetKey {
            provider,
            api_key,
            clear,
        } => llm_set_key(&provider, &api_key, clear),
        Command::LlmEvict { keep } => llm_evict(keep.as_deref()),
        Command::LlmTest { prompt } => llm_test(prompt.as_deref()),
        Command::Settings => to_value(catalog.settings().clone()),
        Command::SettingsSet {
            mature_visible,
            imgdir_min_pages,
            theme,
            skin,
            zoom,
        } => {
            let mut settings = catalog.settings().clone();
            if let Some(v) = mature_visible {
                settings.mature_visible = v;
            }
            if let Some(v) = imgdir_min_pages {
                settings.imgdir_min_pages = v;
            }
            // The skin wins when both are given; a bare theme maps to the default
            // skin of that mode so the old flag keeps working.
            if let Some(v) = skin {
                settings.skin = v;
            } else if let Some(v) = theme {
                match v.trim().to_ascii_lowercase().as_str() {
                    "light" | "paper" => settings.skin = "paper-library".to_string(),
                    "dark" | "ink" => settings.skin = "magpie-ink".to_string(),
                    "neon" => settings.skin = "neon-shelf".to_string(),
                    _ => {}
                }
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
        Command::SubsGenerate {
            key,
            model,
            lang,
            out,
        } => catalog.subs_generate(&key, model.as_deref(), lang.as_deref(), out.as_deref()),
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
        Command::DriveHealth => drive_health_report(catalog),
    }
}

/// Physical disks behind every library root, with warnings for anything that
/// looks sick or nearly full. A slow or missing counter set degrades to
/// unknown, never to invented zeros.
fn drive_health_report(catalog: &mut Catalog) -> MagpieResult<Value> {
    let libraries = catalog
        .libraries()?
        .into_iter()
        .map(|lib| (lib.name, lib.root))
        .collect::<Vec<_>>();
    let report = magpiecraft_health::smart::drive_health(&libraries)?;
    to_value(report)
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

    #[test]
    fn llm_failure_paths_refuse_before_writing() {
        let mut c = catalog();
        // Unknown provider: refused, nothing persisted.
        let bad = dispatch(
            &mut c,
            Command::LlmSelect {
                provider: "nope".to_string(),
                model: "x".to_string(),
            },
        );
        assert_eq!(bad["ok"], json!(false));
        assert_eq!(bad["error_type"], json!("invalid"));
        // Local providers take no key.
        let local = dispatch(
            &mut c,
            Command::LlmSetKey {
                provider: "ollama".to_string(),
                api_key: "k".to_string(),
                clear: false,
            },
        );
        assert_eq!(local["ok"], json!(false));
        // Empty model: refused, so there is never a fallback model.
        let empty = dispatch(
            &mut c,
            Command::LlmSelect {
                provider: "ollama".to_string(),
                model: "  ".to_string(),
            },
        );
        assert_eq!(empty["ok"], json!(false));
    }

    #[test]
    fn subs_generate_refuses_unknown_keys_without_a_binary() {
        let mut c = catalog();
        // No library, no file, no whisper binary involved: resolution fails first.
        let missing = dispatch(
            &mut c,
            Command::SubsGenerate {
                key: "lib:9|nope.mkv".to_string(),
                model: None,
                lang: Some("en".to_string()),
                out: None,
            },
        );
        assert_eq!(missing["ok"], json!(false));
        let empty = dispatch(
            &mut c,
            Command::SubsGenerate {
                key: "".to_string(),
                model: None,
                lang: None,
                out: None,
            },
        );
        assert_eq!(empty["ok"], json!(false));
    }

    #[test]
    fn llm_status_is_redacted_and_lists_skills() {
        let mut c = catalog();
        let status = dispatch(&mut c, Command::LlmStatus);
        assert_eq!(status["ok"], json!(true));
        let text = status.to_string();
        assert!(!text.contains("sk-"), "no key material in status");
        assert!(status["keys_present"].is_array());
        let list = dispatch(&mut c, Command::SkillRead { name: None });
        assert_eq!(list["ok"], json!(true));
        let names: Vec<&str> = list["skills"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|s| s.get("name").and_then(Value::as_str))
            .collect();
        assert!(names.contains(&"magpiecraft-video"));
        assert!(names.contains(&"magpiecraft-verify"));
        // Unknown skill is an invalid error, never an empty success.
        let missing = dispatch(
            &mut c,
            Command::SkillRead {
                name: Some("nope".to_string()),
            },
        );
        assert_eq!(missing["ok"], json!(false));
        assert_eq!(missing["error_type"], json!("invalid"));
    }
}
