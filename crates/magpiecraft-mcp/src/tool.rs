//! MCP tool surface: four compound tools over the same command bus the CLI uses.
//!
//! Tool names use underscores, which some MCP hosts validate strictly. The dotted
//! form (`media.catalog`) is accepted too, for symmetry with the command ids.
//!
//! Read tools are the default; the only mutations in Tier 1 are library add and
//! remove (remove needs `confirm=true`) and reading progress.

use magpiecraft_core::command::{dispatch, parse_search_mode, Command};
use magpiecraft_core::Catalog;
use serde_json::{json, Value};

fn tool(name: &str, description: &str, props: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": props,
            "required": required,
            "additionalProperties": true,
        },
    })
}

fn ops(values: &[&str]) -> Value {
    json!({ "type": "string", "enum": values, "description": "Operation to run." })
}

pub fn tools() -> Value {
    let key =
        json!({ "type": "string", "description": "Item key, for example lib:1|Manga/Yotsuba v01" });
    let library = json!({ "type": "string", "description": "Library name or numeric id. Omit for all libraries." });
    json!([
        tool(
            "media_catalog",
            "Inventory of the user's own media. Operations: status (version, home, counts, external tools), \
             libraries, browse (one folder, or all libraries when key is omitted), stats (counts, pages, bytes, \
             drives), scan (incremental by path, size and mtime; full re-reads everything), series, continue \
             (recently read), library_add, library_remove (requires confirm), settings, settings_set. \
             Scanning is read-only on the collection. Nothing is ever downloaded.",
            json!({
                "operation": ops(&[
                    "status", "libraries", "browse", "stats", "scan", "series", "continue",
                    "library_add", "library_remove", "settings", "settings_set", "sources", "tools"
                ]),
                "key": { "type": "string", "description": "Folder key for browse, e.g. lib:1|Manga" },
                "library": library,
                "full": { "type": "boolean", "description": "scan: re-read unchanged files too" },
                "name": { "type": "string", "description": "library_add or library_remove: library name" },
                "root": { "type": "string", "description": "library_add: absolute folder that holds the library" },
                "kind": { "type": "string", "description": "library_add: comics, manga, magazines, video, audio, ebook, photo, other" },
                "mature": { "type": "boolean", "description": "library_add: flag the library as mature (hidden by default)" },
                "rtl": { "type": ["boolean", "null"], "description": "library_add: force reading direction" },
                "confirm": { "type": "boolean", "description": "library_remove: must be true, removes index rows only" },
                "limit": { "type": "integer", "description": "continue: how many entries (default 12)" },
                "mature_visible": { "type": "boolean", "description": "settings_set: show mature libraries" },
                "imgdir_min_pages": { "type": "integer", "description": "settings_set: images needed for a page folder to be a volume" },
                "theme": { "type": "string", "description": "settings_set: legacy, maps dark/light to the default skin of that mode" },
                "skin": { "type": "string", "description": "settings_set: magpie-ink, paper-library, neon-shelf, forest-stacks, sepia-desk or midnight-archive (wins over theme)" },
                "zoom": { "type": "number", "description": "settings_set: UI zoom 0.6 to 2.5" }
            }),
            &["operation"]
        ),
        tool(
            "media_search",
            "Search the whole catalogue. Operation search: every word must match; mode auto tries FTS5 prefix \
             matching first and falls back to substring matching. Operation item: full record for one key, \
             including page count, series, volume and reading progress.",
            json!({
                "operation": ops(&["search", "item"]),
                "query": { "type": "string", "description": "search: words to find" },
                "limit": { "type": "integer", "description": "search: result cap (default from settings, 300)" },
                "mode": { "type": "string", "description": "search: auto, fts or substring" },
                "key": key
            }),
            &["operation"]
        ),
        tool(
            "media_read",
            "Reading surface. Operation volume: page count, readable flag and the first page names for one item \
             (CBR/RAR and PDF pages come through 7-Zip and PdfCraft). page_offset: remember the difference \
             between printed page numbers and scan order. rtl: set reading direction for one item. \
             progress_get and progress_set: reading position. page_export: write one page to a file. \
             open: hand a film, track, book or photo to the user's player or OS handler, resuming where it \
             left off. open runs a program on the user's machine; it never decodes media itself.",
            json!({
                "operation": ops(&["volume", "page_offset", "rtl", "progress_get", "progress_set", "page_export", "open"]),
                "key": key,
                "offset": { "type": "integer", "description": "page_offset: scan page minus printed page" },
                "rtl": { "type": ["boolean", "null"], "description": "rtl: true, false, or null to clear" },
                "page": { "type": "integer", "description": "progress_set: current page; page_export: page to write" },
                "total": { "type": "integer", "description": "progress_set: page count if unknown" },
                "finished": { "type": "boolean", "description": "progress_set: mark as finished" },
                "out": { "type": "string", "description": "page_export: file to write the page to" }
            }),
            &["operation"]
        ),
        tool(
            "media_transform",
            "Converting and subtitle work, all of it writing new files. presets: the named ffmpeg recipes \
             (remux, MP4 compatibility, loudness normalisation, strip metadata) and whether ffmpeg is present. \
             convert_plan: a plan for one video or track with a preset; apply it with media_curate, undo deletes \
             the output. subs: sidecar subtitles and embedded tracks for one item. subs_shift: move every cue by \
             seconds. subs_convert: SRT to WebVTT or back. subs_extract: pull an embedded track out with ffmpeg. \
             Nothing here edits a source file.",
            json!({
                "operation": ops(&["presets", "convert_plan", "subs", "subs_shift", "subs_convert", "subs_extract"]),
                "key": key,
                "preset": { "type": "string", "description": "convert_plan: remux-mkv, mp4-compat, loudnorm or strip-metadata" },
                "out": { "type": "string", "description": "output file; defaults to a suffixed name beside the source" },
                "path": { "type": "string", "description": "subs_shift/subs_convert: the subtitle file to read" },
                "by": { "type": "number", "description": "subs_shift: seconds to move the cues, negative to advance" },
                "to": { "type": "string", "description": "subs_convert: srt or vtt" },
                "stream": { "type": "integer", "description": "subs_extract: subtitle stream index, 0 is the first" }
            }),
            &["operation"]
        ),
        tool(
            "media_health",
            "Integrity. duplicates: exact duplicates by size then BLAKE3. hash: BLAKE3 of one item. manifest_write \
             and manifest_verify: checksum manifests for bitrot detection. verify_status: which libraries are \
             due for a re-check and what the last run found. verify_run: write the baseline on the first run, \
             compare afterwards, and record every run. inspect: ffprobe technical details for one file. Files \
             are only ever read.",
            json!({
                "operation": ops(&["duplicates", "hash", "manifest_write", "manifest_verify", "inspect", "verify_status", "verify_run", "backup_status", "backup_plan", "near_duplicates"]),
                "library": library,
                "limit": { "type": "integer", "description": "duplicates: group cap (default 200)" },
                "key": key,
                "out": { "type": "string", "description": "manifest_write: manifest path (default under the data home)" },
                "manifest": { "type": "string", "description": "manifest_verify: manifest path" },
                "path": { "type": "string", "description": "inspect: absolute path of a media file" },
                "every_days": { "type": "integer", "description": "verify: re-check interval in days (default 30)" },
                "force": { "type": "boolean", "description": "verify_run: verify even when not due" },
                "to": { "type": "string", "description": "backup_plan: the backup root, e.g. E:/Media" },
                "verify": { "type": "boolean", "description": "backup_plan: checksum every copy (default true)" }
            }),
            &["operation"]
        ),
        tool(
            "media_curate",
            "Organise the collection without touching a file until you confirm. review lists items whose \
             filename parse was uncertain. plan_rename builds a rename plan (nothing changes), plans lists \
             saved plans, apply applies one with confirm=true, journals lists applied plans, undo reverses \
             one with confirm=true. plan_quarantine moves an item into quarantine, never deletes it. \
             plan_cleanup proposes empty folders, orphan sidecars, sample files and junk for quarantine. \
             Every applied change is journalled and reversible.",
            json!({
                "operation": ops(&["review", "plan_rename", "plan_quarantine", "plan_cleanup", "plans", "apply", "journals", "undo"]),
                "library": library,
                "limit": { "type": "integer", "description": "review: how many entries (default 200); plan_rename: 0 for all" },
                "preset": { "type": "string", "description": "plan_rename: auto (per item kind), comics, manga or video" },
                "key": key,
                "id": { "type": "string", "description": "apply/undo: plan or journal id" },
                "confirm": { "type": "boolean", "description": "apply/undo: must be true; without it the plan is refused" }
            }),
            &["operation"]
        ),
        tool(
            "magpie_skill_read",
            "Read the bundled skill texts. Without name it lists the skills with their descriptions; \
             with name it returns the full SKILL.md body. Skills name only tools this server really has.",
            json!({
                "name": { "type": "string", "description": "skill folder name, for example magpiecraft-video" }
            }),
            &[]
        )
    ])
}

fn req_str(args: &Value, name: &str) -> Result<String, String> {
    args.get(name)
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("missing required string argument: {name}"))
}

fn opt_str(args: &Value, name: &str) -> Option<String> {
    args.get(name)
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn opt_bool(args: &Value, name: &str) -> Option<bool> {
    args.get(name).and_then(Value::as_bool)
}

fn opt_usize(args: &Value, name: &str) -> Option<usize> {
    args.get(name)
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
}

fn opt_i32(args: &Value, name: &str) -> Option<i32> {
    args.get(name)
        .and_then(Value::as_i64)
        .and_then(|n| i32::try_from(n).ok())
}

/// Translate one tool call into a command. Unknown operations are an error the
/// caller sees as `isError`, never a silent no-op.
pub fn to_command(name: &str, args: &Value) -> Result<Command, String> {
    let normalised = name.replace('.', "_");
    if normalised == "magpie_skill_read" {
        return Ok(Command::SkillRead {
            name: opt_str(args, "name"),
        });
    }
    let operation = req_str(args, "operation")?;
    let op = operation.to_ascii_lowercase();
    let cmd = match normalised.as_str() {
        "media_catalog" => match op.as_str() {
            "status" => Command::Status,
            "tools" => Command::Tools,
            "sources" => Command::Sources,
            "libraries" => Command::Libraries,
            "browse" => Command::Browse {
                key: opt_str(args, "key"),
            },
            "stats" => Command::Stats,
            "scan" => Command::Scan {
                library: opt_str(args, "library"),
                full: opt_bool(args, "full").unwrap_or(false),
            },
            "series" => Command::Series {
                library: opt_str(args, "library"),
            },
            "continue" => Command::Continue {
                limit: opt_usize(args, "limit").unwrap_or(12),
            },
            "library_add" | "library.add" => Command::LibraryAdd {
                name: req_str(args, "name")?,
                root: req_str(args, "root")?,
                kind: opt_str(args, "kind").unwrap_or_else(|| "comics".to_string()),
                mature: opt_bool(args, "mature").unwrap_or(false),
                rtl_default: opt_bool(args, "rtl"),
            },
            "library_remove" | "library.remove" => Command::LibraryRemove {
                name: req_str(args, "name")?,
                confirm: opt_bool(args, "confirm").unwrap_or(false),
            },
            "settings" => Command::Settings,
            "settings_set" | "settings.set" => Command::SettingsSet {
                mature_visible: opt_bool(args, "mature_visible"),
                imgdir_min_pages: opt_usize(args, "imgdir_min_pages"),
                theme: opt_str(args, "theme"),
                skin: opt_str(args, "skin"),
                zoom: args.get("zoom").and_then(Value::as_f64).map(|z| z as f32),
            },
            other => return Err(format!("unknown media_catalog operation: {other}")),
        },
        "media_search" => match op.as_str() {
            "search" => Command::Search {
                query: req_str(args, "query")?,
                limit: opt_usize(args, "limit").unwrap_or(0),
                mode: parse_search_mode(
                    &opt_str(args, "mode").unwrap_or_else(|| "auto".to_string()),
                )
                .map_err(|e| e.to_string())?,
            },
            "item" => Command::Item {
                key: req_str(args, "key")?,
            },
            other => return Err(format!("unknown media_search operation: {other}")),
        },
        "media_read" => match op.as_str() {
            "volume" | "pages" => Command::Pages {
                key: req_str(args, "key")?,
            },
            "page_export" | "page.export" => Command::PageExport {
                key: req_str(args, "key")?,
                page: opt_usize(args, "page")
                    .ok_or_else(|| "missing required integer argument: page".to_string())?,
                out: req_str(args, "out")?,
            },
            // Launches a program on the user's own desktop. Not destructive, but it
            // is the one read tool with a visible side effect, so it says so.
            "open" => Command::OpenItem {
                key: req_str(args, "key")?,
            },
            "page_offset" | "offset" => Command::PageOffset {
                key: req_str(args, "key")?,
                offset: opt_i32(args, "offset")
                    .ok_or_else(|| "missing required integer argument: offset".to_string())?,
            },
            "rtl" => Command::Rtl {
                key: req_str(args, "key")?,
                rtl: opt_bool(args, "rtl"),
            },
            "progress_get" | "progress.get" => Command::ProgressGet {
                key: req_str(args, "key")?,
            },
            "progress_set" | "progress.set" => Command::ProgressSet {
                key: req_str(args, "key")?,
                page: opt_usize(args, "page")
                    .ok_or_else(|| "missing required integer argument: page".to_string())?,
                total: opt_usize(args, "total"),
                finished: opt_bool(args, "finished").unwrap_or(false),
            },
            other => return Err(format!("unknown media_read operation: {other}")),
        },
        "media_health" => match op.as_str() {
            "duplicates" | "dupes" => Command::Duplicates {
                library: opt_str(args, "library"),
                limit: opt_usize(args, "limit").unwrap_or(200),
            },
            "hash" => Command::Hash {
                key: req_str(args, "key")?,
            },
            "manifest_write" | "manifest.write" => Command::ManifestWrite {
                library: req_str(args, "library")?,
                out: opt_str(args, "out"),
            },
            "manifest_verify" | "manifest.verify" => Command::ManifestVerify {
                library: req_str(args, "library")?,
                manifest: opt_str(args, "manifest"),
            },
            "inspect" => Command::Inspect {
                path: req_str(args, "path")?,
            },
            "verify_status" | "verify.status" => Command::VerifyStatus {
                every_days: opt_i32(args, "every_days").map(i64::from).unwrap_or(30),
            },
            "backup_status" | "backup.status" => Command::BackupStatus,
            "backup_plan" | "backup.plan" => Command::PlanBackup {
                library: opt_str(args, "library"),
                to: req_str(args, "to")?,
                verify: opt_bool(args, "verify").unwrap_or(true),
                limit: opt_usize(args, "limit").unwrap_or(0),
            },
            "near_duplicates" | "near.duplicates" | "near_dupes" => Command::NearDuplicates {
                limit: opt_usize(args, "limit").unwrap_or(200),
            },
            "verify_run" | "verify.run" => Command::VerifyRun {
                library: opt_str(args, "library"),
                every_days: opt_i32(args, "every_days").map(i64::from).unwrap_or(30),
                force: opt_bool(args, "force").unwrap_or(false),
            },
            other => return Err(format!("unknown media_health operation: {other}")),
        },
        "media_transform" => match op.as_str() {
            "presets" => Command::Presets,
            "convert_plan" | "convert.plan" => Command::PlanConvert {
                key: req_str(args, "key")?,
                preset: req_str(args, "preset")?,
                out: opt_str(args, "out"),
            },
            "subs" => Command::Subs {
                key: req_str(args, "key")?,
            },
            "subs_shift" | "subs.shift" => Command::SubsShift {
                path: req_str(args, "path")?,
                seconds: args.get("by").and_then(Value::as_f64).unwrap_or(0.0),
                out: opt_str(args, "out"),
            },
            "subs_convert" | "subs.convert" => Command::SubsConvert {
                path: req_str(args, "path")?,
                to: req_str(args, "to")?,
                out: opt_str(args, "out"),
            },
            "subs_extract" | "subs.extract" => Command::SubsExtract {
                key: req_str(args, "key")?,
                subtitle_index: opt_usize(args, "stream").unwrap_or(0) as u32,
                out: req_str(args, "out")?,
            },
            other => return Err(format!("unknown media_transform operation: {other}")),
        },
        "media_curate" => match op.as_str() {
            "review" => Command::Review {
                library: opt_str(args, "library"),
                limit: opt_usize(args, "limit").unwrap_or(200),
            },
            "plan_rename" | "plan.rename" => Command::PlanRename {
                library: opt_str(args, "library"),
                preset: opt_str(args, "preset").unwrap_or_else(|| "auto".to_string()),
                limit: opt_usize(args, "limit").unwrap_or(0),
            },
            "plan_quarantine" | "plan.quarantine" => Command::PlanQuarantine {
                key: req_str(args, "key")?,
            },
            "plan_cleanup" | "plan.cleanup" => Command::PlanCleanup {
                library: opt_str(args, "library"),
            },
            "plans" => Command::Plans,
            "apply" => Command::ApplyPlan {
                id: req_str(args, "id")?,
                confirm: opt_bool(args, "confirm").unwrap_or(false),
            },
            "journals" => Command::Journals,
            "undo" => Command::Undo {
                id: req_str(args, "id")?,
                confirm: opt_bool(args, "confirm").unwrap_or(false),
            },
            other => return Err(format!("unknown media_curate operation: {other}")),
        },
        other => return Err(format!("unknown tool: {other}")),
    };
    Ok(cmd)
}

/// Handle one `tools/call` params object.
pub fn call_tool(catalog: &mut Catalog, params: &Value) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let empty = json!({});
    let args = params.get("arguments").unwrap_or(&empty);
    match to_command(name, args) {
        Ok(cmd) => {
            let result = dispatch(catalog, cmd);
            let is_error = result.get("ok").and_then(Value::as_bool) == Some(false);
            json!({
                "content": [{ "type": "text", "text": result.to_string() }],
                "isError": is_error,
            })
        }
        Err(message) => json!({
            "content": [{ "type": "text", "text": json!({
                "ok": false,
                "error": message,
                "error_type": "invalid",
            }).to_string() }],
            "isError": true,
        }),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use magpiecraft_core::Paths;

    #[test]
    fn schemas_are_well_formed_and_enumerate_operations() {
        let list = tools();
        let arr = list.as_array().unwrap();
        assert_eq!(
            arr.len(),
            7,
            "media_catalog, media_search, media_read, media_health, media_curate, media_transform, magpie_skill_read"
        );
        for t in arr {
            let name = t["name"].as_str().unwrap();
            assert!(
                name.starts_with("media_") || name == "magpie_skill_read",
                "{name}"
            );
            assert!(
                name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "MCP names must be strict: {name}"
            );
            assert!(!t["description"].as_str().unwrap().is_empty());
            if name != "magpie_skill_read" {
                assert!(t["inputSchema"]["properties"]["operation"]["enum"].is_array());
            }
        }
    }

    #[test]
    fn tool_names_match_the_core_skill_allowlist() {
        let list = tools();
        let mut names: Vec<&str> = list
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        names.sort_unstable();
        let mut known: Vec<&str> = magpiecraft_core::skills::KNOWN_TOOLS.to_vec();
        known.sort_unstable();
        assert_eq!(names, known);
    }

    #[test]
    fn operations_map_to_commands() {
        let cmd = to_command(
            "media_search",
            &json!({"operation":"search","query":"saga"}),
        )
        .unwrap();
        assert_eq!(cmd.id(), "search");
        let dotted = to_command("media.catalog", &json!({"operation":"libraries"})).unwrap();
        assert_eq!(dotted.id(), "libraries");
        let offset = to_command(
            "media_read",
            &json!({"operation":"page_offset","key":"lib:1|a.cbz","offset":2}),
        )
        .unwrap();
        assert_eq!(offset.id(), "page_offset");
        assert!(to_command("media_read", &json!({"operation":"volume"})).is_err());
        let convert = to_command(
            "media_transform",
            &json!({"operation":"convert_plan","key":"lib:1|a.mkv","preset":"remux-mkv"}),
        )
        .unwrap();
        assert_eq!(convert.id(), "convert.plan");
        assert!(to_command("media_transform", &json!({"operation":"nope"})).is_err());
        assert!(to_command("media_catalog", &json!({"operation":"download_everything"})).is_err());
        assert!(to_command("media_catalog", &json!({})).is_err());
    }

    #[test]
    fn a_failing_command_is_flagged_as_an_mcp_error() {
        let home = std::env::temp_dir().join(format!("magpie-mcp-tool-{}", std::process::id()));
        let mut catalog = Catalog::open_in_memory(Paths::at(home)).unwrap();
        let bad = call_tool(
            &mut catalog,
            &json!({"name":"media_read","arguments":{"operation":"volume","key":"nonsense"}}),
        );
        assert_eq!(bad["isError"], json!(true));
        let good = call_tool(
            &mut catalog,
            &json!({"name":"media_catalog","arguments":{"operation":"status"}}),
        );
        assert_eq!(good["isError"], json!(false));
        let text = good["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"app\":\"magpiecraft\""), "{text}");
    }
}
