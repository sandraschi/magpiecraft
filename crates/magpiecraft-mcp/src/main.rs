//! MagpieCraft L2: one binary that is both the CLI and the MCP stdio server.
//!
//! `magpiecraft-mcp <subcommand> [--json]` for humans and scripts.
//! `magpiecraft-mcp mcp` for a client: line-delimited JSON-RPC 2.0 on stdin and
//! stdout, MCP `2024-11-05`, logs on stderr only. A stray `println!` here would
//! corrupt the handshake, so every diagnostic goes through `eprintln!`.
//!
//! Both surfaces build the same `Command` variants and call the same
//! `dispatch`, so the CLI and MCP can never disagree.

mod bench;
mod human;
mod mcp;
mod tool;

use clap::{Parser, Subcommand};
use magpiecraft_core::command::{dispatch, parse_search_mode, Command};
use magpiecraft_core::{Catalog, Paths};
use serde_json::{json, Value};

#[derive(Parser, Debug)]
#[command(
    name = "magpiecraft-mcp",
    version,
    about = "MagpieCraft: inventory and read a comics, manga and media collection you already own",
    long_about = "MagpieCraft indexes libraries you already own and reads them. It never downloads, \
                  never circumvents DRM and never writes into a collection. MCP clients start it with \
                  the `mcp` subcommand."
)]
struct Cli {
    /// Print machine-readable JSON instead of a human summary.
    #[arg(long, global = true)]
    json: bool,

    /// Override the data home (default: %LOCALAPPDATA%\magpiecraft).
    #[arg(long, global = true)]
    home: Option<String>,

    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Version, home, counts and external tool status.
    Status,
    /// External tools: ffprobe, 7-Zip, PdfCraft.
    Tools,
    /// Federated source registry (pillar E).
    Sources,
    /// Configured libraries.
    Libraries,
    /// Add a library root.
    LibraryAdd {
        name: String,
        root: String,
        #[arg(long, default_value = "comics")]
        kind: String,
        #[arg(long)]
        mature: bool,
        /// Force right-to-left reading for this library.
        #[arg(long)]
        rtl: bool,
        /// Force left-to-right reading for this library.
        #[arg(long, conflicts_with = "rtl")]
        ltr: bool,
    },
    /// Remove a library from the index. The files are never touched.
    LibraryRemove {
        name: String,
        /// Required: confirms removing the index rows.
        #[arg(long)]
        confirm: bool,
    },
    /// Scan libraries incrementally by path, size and mtime.
    Scan {
        /// Library name or id. All libraries when omitted.
        library: Option<String>,
        /// Re-read every item, including unchanged files.
        #[arg(long)]
        full: bool,
    },
    /// Browse one folder. No key lists the libraries.
    Browse {
        /// Item key such as `lib:1|Manga/Yotsuba`.
        key: Option<String>,
    },
    /// Whole-collection search.
    Search {
        /// Words to find. Every word must match.
        query: Vec<String>,
        #[arg(long, default_value_t = 0)]
        limit: usize,
        /// auto, fts or substring.
        #[arg(long, default_value = "auto")]
        mode: String,
    },
    /// One item by key.
    Item { key: String },
    /// Page list of one volume.
    Pages { key: String },
    /// Write one page to a file (read-only on the collection).
    Page {
        key: String,
        page: usize,
        #[arg(long)]
        out: String,
    },
    /// Open an item in the player (video, audio) or the OS handler (books, photos).
    Open { key: String },
    /// Set the printed-page offset of one item.
    Offset { key: String, value: i32 },
    /// Force or clear the reading direction of one item.
    Rtl {
        key: String,
        #[arg(long)]
        on: bool,
        #[arg(long, conflicts_with = "on")]
        off: bool,
    },
    /// Reading progress for one item.
    Progress {
        key: String,
        /// Record this page instead of reading progress.
        #[arg(long)]
        set: Option<usize>,
        #[arg(long)]
        of: Option<usize>,
        #[arg(long)]
        finished: bool,
    },
    /// Recently read items.
    Continue {
        #[arg(long, default_value_t = 12)]
        limit: usize,
    },
    /// Counts, sizes, pages and drives.
    Stats,
    /// Near duplicates: perceptual images, videos with the same title and year.
    NearDupes {
        #[arg(long, default_value_t = 200)]
        limit: usize,
    },
    /// Exact duplicates by BLAKE3.
    Dupes {
        library: Option<String>,
        #[arg(long, default_value_t = 200)]
        limit: usize,
    },
    /// BLAKE3 of one item.
    Hash { key: String },
    /// Write a checksum manifest for a library.
    ManifestWrite {
        library: String,
        #[arg(long)]
        out: Option<String>,
    },
    /// Verify a checksum manifest.
    ManifestVerify {
        library: String,
        #[arg(long)]
        manifest: Option<String>,
    },
    /// Series in a library.
    Series { library: Option<String> },
    /// Set the reading direction of a series.
    SeriesRtl {
        library: String,
        series: String,
        #[arg(long)]
        on: bool,
        #[arg(long, conflicts_with = "on")]
        off: bool,
    },
    /// Technical inspection of one media file with ffprobe.
    Inspect { path: String },
    /// Current settings.
    Settings,
    /// Change settings.
    SetSettings {
        #[arg(long)]
        mature: Option<bool>,
        #[arg(long)]
        imgdir_min_pages: Option<usize>,
        #[arg(long)]
        theme: Option<String>,
        /// Skin id: magpie-ink, paper-library, neon-shelf, forest-stacks, sepia-desk or midnight-archive. Wins over --theme.
        #[arg(long)]
        skin: Option<String>,
        #[arg(long)]
        zoom: Option<f32>,
    },
    /// Synthetic scan benchmark. Writes only under the data home.
    BenchScan {
        #[arg(long, default_value_t = 100_000)]
        files: usize,
        /// Reuse a tree at this path instead of creating one.
        #[arg(long)]
        dir: Option<String>,
    },
    /// MCP stdio server (this is what a client starts).
    Mcp,
    /// What checksum verification is due.
    Verify {
        #[arg(long, default_value_t = 30)]
        every_days: i64,
    },
    /// Verify the libraries that are due (the first run writes the baseline).
    VerifyRun {
        library: Option<String>,
        #[arg(long, default_value_t = 30)]
        every_days: i64,
        /// Verify even when the library is not due.
        #[arg(long)]
        force: bool,
    },
    /// The ffmpeg preset catalogue.
    Presets,
    /// Build a conversion plan for one item (nothing encoded until applied).
    PlanConvert {
        key: String,
        #[arg(long)]
        preset: String,
        #[arg(long)]
        out: Option<String>,
    },
    /// Subtitle sidecars and embedded tracks for one item.
    Subs { key: String },
    /// Shift a subtitle file by seconds into a new file.
    SubsShift {
        path: String,
        #[arg(long, allow_hyphen_values = true)]
        by: f64,
        #[arg(long)]
        out: Option<String>,
    },
    /// Convert a subtitle file to srt or vtt into a new file.
    SubsConvert {
        path: String,
        #[arg(long)]
        to: String,
        #[arg(long)]
        out: Option<String>,
    },
    /// Extract an embedded subtitle stream with ffmpeg.
    SubsExtract {
        key: String,
        /// 0 is the first subtitle stream.
        #[arg(long, default_value_t = 0)]
        stream: u32,
        #[arg(long)]
        out: String,
    },
    /// Generate an SRT sidecar with whisperx/whisper on this machine (minutes, not seconds).
    SubsGenerate {
        key: String,
        /// Whisper model name, for example large-v3. Default is the tool default.
        #[arg(long)]
        model: Option<String>,
        /// Language code, for example en. Default is auto-detect.
        #[arg(long)]
        lang: Option<String>,
        /// Output path. Default is <stem>.<lang>.srt beside the source.
        #[arg(long)]
        out: Option<String>,
    },
    /// Where each library stands against the configured backup roots.
    Backup,
    /// Physical disks behind the libraries, with health warnings.
    Drives,
    /// Build a verified backup copy plan (nothing is copied until applied).
    BackupPlan {
        /// Where the backup lives, for example E:\Media
        #[arg(long)]
        to: String,
        library: Option<String>,
        /// Size check only; skip the checksum of every copy.
        #[arg(long)]
        fast: bool,
        /// 0 means every item.
        #[arg(long, default_value_t = 0)]
        limit: usize,
    },
    /// Set (or clear) the backup roots.
    BackupRoots {
        roots: Vec<String>,
        #[arg(long)]
        clear: bool,
    },
    /// Items whose filename parse was uncertain.
    Review {
        library: Option<String>,
        #[arg(long, default_value_t = 200)]
        limit: usize,
    },
    /// Build a rename plan. Nothing changes until the plan is applied.
    PlanRename {
        library: Option<String>,
        /// auto (per item kind), comics, manga or video.
        #[arg(long, default_value = "auto")]
        preset: String,
        /// 0 means every item in the library.
        #[arg(long, default_value_t = 0)]
        limit: usize,
    },
    /// Build a quarantine plan for one item (a move, never a delete).
    PlanQuarantine { key: String },
    /// Build a cleanup plan: empty folders, orphan sidecars, samples, junk.
    PlanCleanup { library: Option<String> },
    /// Saved plans.
    Plans,
    /// Apply a saved plan. Requires --confirm.
    Apply {
        id: String,
        #[arg(long)]
        confirm: bool,
    },
    /// Applied plans and their journals.
    Journals,
    /// Reverse an applied plan. Requires --confirm.
    Undo {
        id: String,
        #[arg(long)]
        confirm: bool,
    },
    /// Bundled skill texts for agents. Lists, shows or exports them.
    Skills {
        #[command(subcommand)]
        op: Option<SkillsOp>,
    },
    /// LLM provider: status, models, selection, keys, VRAM, live test.
    Llm {
        #[command(subcommand)]
        op: Option<LlmOp>,
    },
}

#[derive(Subcommand, Debug)]
enum SkillsOp {
    /// List the bundled skills.
    List,
    /// Read one skill body, for example magpiecraft-video.
    Show { name: String },
    /// Write every SKILL.md to a directory (for example ~/.claude/skills).
    Export {
        /// Target directory.
        #[arg(long)]
        to: String,
        /// Overwrite existing files.
        #[arg(long)]
        force: bool,
    },
}

/// `llm` verbs, mirroring the magpie_llm_* MCP tools one to one.
#[derive(Subcommand, Debug)]
enum LlmOp {
    /// Redacted config: provider, model, endpoint, key presence.
    Status,
    /// Live model list for one provider, or the configured provider.
    List {
        #[arg(long)]
        provider: Option<String>,
    },
    /// Persist the provider and model (switches Ollama VRAM to the pick).
    Use { provider: String, model: String },
    /// Store one provider key locally. Empty keeps the stored value.
    SetKey {
        provider: String,
        #[arg(long)]
        key: Option<String>,
        /// Forget the stored key.
        #[arg(long)]
        clear: bool,
    },
    /// Evict resident Ollama models, optionally keeping one warm.
    Evict {
        #[arg(long)]
        keep: Option<String>,
    },
    /// One live completion through the selected provider.
    Test {
        #[arg(long)]
        prompt: Option<String>,
    },
}

fn print_value(json_mode: bool, command_id: &str, value: &Value) -> i32 {
    if json_mode {
        match serde_json::to_string_pretty(value) {
            Ok(text) => println!("{text}"),
            Err(e) => {
                eprintln!("[magpiecraft] could not serialise output: {e}");
                return 1;
            }
        }
    } else {
        human::print(command_id, value);
    }
    if value.get("ok").and_then(Value::as_bool) == Some(false) {
        return 1;
    }
    0
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let paths = match cli.home.as_deref() {
        Some(home) => Paths::at(home),
        None => Paths::discover(),
    };

    if matches!(cli.command, Cmd::Mcp) {
        return match mcp::serve(paths) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("[magpiecraft-mcp] fatal: {e}");
                std::process::ExitCode::FAILURE
            }
        };
    }

    if let Cmd::BenchScan { files, dir } = &cli.command {
        return match bench::run(paths, *files, dir.as_deref()) {
            Ok(value) => {
                let code = print_value(cli.json, "bench-scan", &value);
                std::process::ExitCode::from(u8::try_from(code).unwrap_or(1))
            }
            Err(e) => {
                eprintln!("[magpiecraft] bench-scan failed: {e}");
                std::process::ExitCode::FAILURE
            }
        };
    }

    let mut catalog = match Catalog::open(paths) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[magpiecraft] cannot open the catalogue: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };

    let (id, command) = match build(&cli.command, cli.json) {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("[magpiecraft] {e}");
            return std::process::ExitCode::from(2);
        }
    };
    let value = dispatch(&mut catalog, command);
    let code = print_value(cli.json, id, &value);
    std::process::ExitCode::from(u8::try_from(code).unwrap_or(1))
}

fn build(cmd: &Cmd, json_mode: bool) -> Result<(&'static str, Command), String> {
    let mode = |s: &str| parse_search_mode(s).map_err(|e| e.to_string());
    let direction = |on: bool, off: bool| -> Option<bool> {
        if on {
            Some(true)
        } else if off {
            Some(false)
        } else {
            None
        }
    };
    let pair = match cmd {
        Cmd::Status => (Command::Status.id(), Command::Status),
        Cmd::Tools => (Command::Tools.id(), Command::Tools),
        Cmd::Sources => (Command::Sources.id(), Command::Sources),
        Cmd::Libraries => (Command::Libraries.id(), Command::Libraries),
        Cmd::LibraryAdd {
            name,
            root,
            kind,
            mature,
            rtl,
            ltr,
        } => {
            let c = Command::LibraryAdd {
                name: name.clone(),
                root: root.clone(),
                kind: kind.clone(),
                mature: *mature,
                rtl_default: direction(*rtl, *ltr),
            };
            (c.id(), c)
        }
        Cmd::LibraryRemove { name, confirm } => {
            let c = Command::LibraryRemove {
                name: name.clone(),
                confirm: *confirm,
            };
            (c.id(), c)
        }
        Cmd::Scan { library, full } => {
            let c = Command::Scan {
                library: library.clone(),
                full: *full,
            };
            (c.id(), c)
        }
        Cmd::Browse { key } => {
            let c = Command::Browse { key: key.clone() };
            (c.id(), c)
        }
        Cmd::Search {
            query,
            limit,
            mode: m,
        } => {
            let c = Command::Search {
                query: query.join(" "),
                limit: *limit,
                mode: mode(m)?,
            };
            (c.id(), c)
        }
        Cmd::Item { key } => {
            let c = Command::Item { key: key.clone() };
            (c.id(), c)
        }
        Cmd::Pages { key } => {
            let c = Command::Pages { key: key.clone() };
            (c.id(), c)
        }
        Cmd::Page { key, page, out } => {
            let c = Command::PageExport {
                key: key.clone(),
                page: *page,
                out: out.clone(),
            };
            (c.id(), c)
        }
        Cmd::Open { key } => {
            let c = Command::OpenItem { key: key.clone() };
            (c.id(), c)
        }
        Cmd::Offset { key, value } => {
            let c = Command::PageOffset {
                key: key.clone(),
                offset: *value,
            };
            (c.id(), c)
        }
        Cmd::Rtl { key, on, off } => {
            let c = Command::Rtl {
                key: key.clone(),
                rtl: direction(*on, *off),
            };
            (c.id(), c)
        }
        Cmd::Progress {
            key,
            set,
            of,
            finished,
        } => match set {
            Some(page) => {
                let c = Command::ProgressSet {
                    key: key.clone(),
                    page: *page,
                    total: *of,
                    finished: *finished,
                };
                (c.id(), c)
            }
            None => {
                let c = Command::ProgressGet { key: key.clone() };
                (c.id(), c)
            }
        },
        Cmd::Continue { limit } => {
            let c = Command::Continue { limit: *limit };
            (c.id(), c)
        }
        Cmd::Stats => (Command::Stats.id(), Command::Stats),
        Cmd::Dupes { library, limit } => {
            let c = Command::Duplicates {
                library: library.clone(),
                limit: *limit,
            };
            (c.id(), c)
        }
        Cmd::NearDupes { limit } => {
            let c = Command::NearDuplicates { limit: *limit };
            (c.id(), c)
        }
        Cmd::Hash { key } => {
            let c = Command::Hash { key: key.clone() };
            (c.id(), c)
        }
        Cmd::ManifestWrite { library, out } => {
            let c = Command::ManifestWrite {
                library: library.clone(),
                out: out.clone(),
            };
            (c.id(), c)
        }
        Cmd::ManifestVerify { library, manifest } => {
            let c = Command::ManifestVerify {
                library: library.clone(),
                manifest: manifest.clone(),
            };
            (c.id(), c)
        }
        Cmd::Series { library } => {
            let c = Command::Series {
                library: library.clone(),
            };
            (c.id(), c)
        }
        Cmd::SeriesRtl {
            library,
            series,
            on,
            off,
        } => {
            let c = Command::SeriesRtl {
                library: library.clone(),
                series: series.clone(),
                rtl: direction(*on, *off),
            };
            (c.id(), c)
        }
        Cmd::Inspect { path } => {
            let c = Command::Inspect { path: path.clone() };
            (c.id(), c)
        }
        Cmd::Settings => (Command::Settings.id(), Command::Settings),
        Cmd::SetSettings {
            mature,
            imgdir_min_pages,
            theme,
            skin,
            zoom,
        } => {
            let c = Command::SettingsSet {
                mature_visible: *mature,
                imgdir_min_pages: *imgdir_min_pages,
                theme: theme.clone(),
                skin: skin.clone(),
                zoom: *zoom,
            };
            (c.id(), c)
        }
        Cmd::BenchScan { .. } | Cmd::Mcp => {
            return Err("internal: command handled before the catalogue was opened".to_string())
        }
        Cmd::Presets => (Command::Presets.id(), Command::Presets),
        Cmd::PlanConvert { key, preset, out } => {
            let c = Command::PlanConvert {
                key: key.clone(),
                preset: preset.clone(),
                out: out.clone(),
            };
            (c.id(), c)
        }
        Cmd::Subs { key } => {
            let c = Command::Subs { key: key.clone() };
            (c.id(), c)
        }
        Cmd::SubsShift { path, by, out } => {
            let c = Command::SubsShift {
                path: path.clone(),
                seconds: *by,
                out: out.clone(),
            };
            (c.id(), c)
        }
        Cmd::SubsConvert { path, to, out } => {
            let c = Command::SubsConvert {
                path: path.clone(),
                to: to.clone(),
                out: out.clone(),
            };
            (c.id(), c)
        }
        Cmd::SubsExtract { key, stream, out } => {
            let c = Command::SubsExtract {
                key: key.clone(),
                subtitle_index: *stream,
                out: out.clone(),
            };
            (c.id(), c)
        }
        Cmd::SubsGenerate {
            key,
            model,
            lang,
            out,
        } => {
            let c = Command::SubsGenerate {
                key: key.clone(),
                model: model.clone(),
                lang: lang.clone(),
                out: out.clone(),
            };
            (c.id(), c)
        }
        Cmd::Backup => (Command::BackupStatus.id(), Command::BackupStatus),
        Cmd::Drives => (Command::DriveHealth.id(), Command::DriveHealth),
        Cmd::BackupPlan {
            to,
            library,
            fast,
            limit,
        } => {
            let c = Command::PlanBackup {
                library: library.clone(),
                to: to.clone(),
                verify: !*fast,
                limit: *limit,
            };
            (c.id(), c)
        }
        Cmd::BackupRoots { roots, clear } => {
            let c = Command::BackupRoots {
                roots: roots.clone(),
                clear: *clear,
            };
            (c.id(), c)
        }
        Cmd::Verify { every_days } => {
            let c = Command::VerifyStatus {
                every_days: *every_days,
            };
            (c.id(), c)
        }
        Cmd::VerifyRun {
            library,
            every_days,
            force,
        } => {
            let c = Command::VerifyRun {
                library: library.clone(),
                every_days: *every_days,
                force: *force,
            };
            (c.id(), c)
        }
        Cmd::Review { library, limit } => {
            let c = Command::Review {
                library: library.clone(),
                limit: *limit,
            };
            (c.id(), c)
        }
        Cmd::PlanRename {
            library,
            preset,
            limit,
        } => {
            let c = Command::PlanRename {
                library: library.clone(),
                preset: preset.clone(),
                limit: *limit,
            };
            (c.id(), c)
        }
        Cmd::PlanQuarantine { key } => {
            let c = Command::PlanQuarantine { key: key.clone() };
            (c.id(), c)
        }
        Cmd::PlanCleanup { library } => {
            let c = Command::PlanCleanup {
                library: library.clone(),
            };
            (c.id(), c)
        }
        Cmd::Plans => (Command::Plans.id(), Command::Plans),
        Cmd::Apply { id, confirm } => {
            let c = Command::ApplyPlan {
                id: id.clone(),
                confirm: *confirm,
            };
            (c.id(), c)
        }
        Cmd::Journals => (Command::Journals.id(), Command::Journals),
        Cmd::Undo { id, confirm } => {
            let c = Command::Undo {
                id: id.clone(),
                confirm: *confirm,
            };
            (c.id(), c)
        }
        Cmd::Skills { op } => match op {
            None | Some(SkillsOp::List) => {
                let c = Command::SkillRead { name: None };
                (c.id(), c)
            }
            Some(SkillsOp::Show { name }) => {
                let c = Command::SkillRead {
                    name: Some(name.clone()),
                };
                (c.id(), c)
            }
            Some(SkillsOp::Export { to, force }) => {
                return export_skills(to, *force, json_mode);
            }
        },
        Cmd::Llm { op } => match op {
            None | Some(LlmOp::Status) => (Command::LlmStatus.id(), Command::LlmStatus),
            Some(LlmOp::List { provider }) => {
                let c = Command::LlmListModels {
                    provider: provider.clone(),
                };
                (c.id(), c)
            }
            Some(LlmOp::Use { provider, model }) => {
                let c = Command::LlmSelect {
                    provider: provider.clone(),
                    model: model.clone(),
                };
                (c.id(), c)
            }
            Some(LlmOp::SetKey {
                provider,
                key,
                clear,
            }) => {
                let c = Command::LlmSetKey {
                    provider: provider.clone(),
                    api_key: key.clone().unwrap_or_default(),
                    clear: *clear,
                };
                (c.id(), c)
            }
            Some(LlmOp::Evict { keep }) => {
                let c = Command::LlmEvict { keep: keep.clone() };
                (c.id(), c)
            }
            Some(LlmOp::Test { prompt }) => {
                let c = Command::LlmTest {
                    prompt: prompt.clone(),
                };
                (c.id(), c)
            }
        },
    };
    Ok(pair)
}

/// Write every bundled SKILL.md under `<dir>/<name>/SKILL.md` for clients that
/// read skill folders. Refuses to overwrite without --force. Needs no catalogue.
fn export_skills(
    dir: &str,
    force: bool,
    json_mode: bool,
) -> Result<(&'static str, Command), String> {
    let root = std::path::Path::new(dir.trim());
    if dir.trim().is_empty() {
        return Err("export directory is empty".to_string());
    }
    let mut written = Vec::new();
    for skill in magpiecraft_core::skills::SKILLS {
        let folder = root.join(skill.name);
        let file = folder.join("SKILL.md");
        if file.exists() && !force {
            return Err(format!(
                "{} exists: re-run with --force to overwrite",
                file.display()
            ));
        }
        std::fs::create_dir_all(&folder)
            .map_err(|e| format!("cannot create {}: {e}", folder.display()))?;
        std::fs::write(&file, skill.body)
            .map_err(|e| format!("cannot write {}: {e}", file.display()))?;
        written.push(file.display().to_string());
    }
    for path in &written {
        if json_mode {
            println!("{path}");
        } else {
            println!("wrote {path}");
        }
    }
    Ok(("skill.read", Command::SkillRead { name: None }))
}

/// The JSON-RPC error envelope used by the MCP loop for unhandled methods.
pub fn method_not_found(id: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": -32601, "message": "Method not found" },
    })
}
