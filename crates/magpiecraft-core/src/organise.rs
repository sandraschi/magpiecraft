//! Plans, diffs, the undo journal and quarantine.
//!
//! Nothing here touches a file until a plan is applied, and a plan is applied only
//! with `confirm=true`. Every applied operation is written to a journal, so `undo`
//! can put the collection back exactly as it was. A delete is a move into
//! quarantine, never `remove_file`.
//!
//! The naming rules are pure functions over `NamingInput`, so a golden test can pin
//! the exact target path for a given item without touching a disk.

use crate::catalog::Catalog;
use crate::config::Paths;
use crate::error::{MagpieError, MagpieResult};
use chrono::{SecondsFormat, Utc};
use magpiecraft_store::ItemRow;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// One planned change. `Move` covers rename, move and quarantine (a move into the
/// app's quarantine folder); `Write` is a sidecar text file such as an NFO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PlanOp {
    Move { from: String, to: String },
    Write { to: String, content: String },
}

impl PlanOp {
    pub fn describe(&self) -> String {
        match self {
            PlanOp::Move { from, to } => format!("- {from}\n+ {to}"),
            PlanOp::Write { to, content } => {
                format!("+ {to} ({} bytes)", content.len())
            }
        }
    }

    pub fn source(&self) -> Option<&str> {
        match self {
            PlanOp::Move { from, .. } => Some(from),
            PlanOp::Write { .. } => None,
        }
    }

    pub fn target(&self) -> &str {
        match self {
            PlanOp::Move { to, .. } => to,
            PlanOp::Write { to, .. } => to,
        }
    }
}

/// What a plan is for. Quarantine plans delete nothing: they move files aside.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanKind {
    Rename,
    Quarantine,
    Nfo,
}

impl PlanKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PlanKind::Rename => "rename",
            PlanKind::Quarantine => "quarantine",
            PlanKind::Nfo => "nfo",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "rename" => Some(PlanKind::Rename),
            "quarantine" => Some(PlanKind::Quarantine),
            "nfo" => Some(PlanKind::Nfo),
            _ => None,
        }
    }
}

/// A proposed set of changes, saved as JSON so `apply` can run in another process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub created_at: String,
    pub kind: String,
    pub library: String,
    pub library_id: i64,
    pub root: String,
    pub ops: Vec<PlanOp>,
    /// Items that were not planned, with the reason. Shown alongside the plan.
    pub skipped: Vec<String>,
    pub bytes: u64,
}

impl Plan {
    pub fn diff(&self) -> String {
        let mut lines: Vec<String> = self.ops.iter().map(PlanOp::describe).collect();
        if !self.skipped.is_empty() {
            lines.push(String::new());
            lines.push("skipped:".to_string());
            lines.extend(self.skipped.iter().map(|s| format!("  {s}")));
        }
        lines.join("\n")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalOp {
    pub op: String,
    pub from: Option<String>,
    pub to: String,
    pub ok: bool,
    pub error: Option<String>,
}

/// The record of one applied plan, and the thing `undo` works from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Journal {
    pub id: String,
    pub plan_id: String,
    pub created_at: String,
    pub kind: String,
    pub library: String,
    pub library_id: i64,
    pub root: String,
    pub ops: Vec<JournalOp>,
    pub undone: bool,
    pub undone_at: Option<String>,
}

impl Journal {
    pub fn applied(&self) -> usize {
        self.ops.iter().filter(|o| o.ok).count()
    }

    pub fn failed(&self) -> usize {
        self.ops.iter().filter(|o| !o.ok).count()
    }
}

// ------------------------------------------------------------------ naming

/// The facts a naming rule may use. Kept separate from `ItemRow` so the rules can
/// be tested without a database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamingInput {
    pub rel: String,
    pub title: String,
    pub series: Option<String>,
    pub volume: Option<String>,
    /// Text after the volume marker, e.g. "L'enfant Tronc". Kept, not dropped.
    pub subtitle: Option<String>,
    pub year: Option<i32>,
    /// Language or edition tag, for example `en` or `de`.
    pub edition: Option<String>,
    /// True when the item is a folder of pages (no extension to preserve).
    pub is_folder: bool,
}

/// A preset decides the shape of the target path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamingPreset {
    /// `Series/Series v05 (2007).cbz`
    Comics,
    /// `Series/Series v05 (2007) [de].cbz`
    Manga,
}

impl NamingPreset {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "comics" | "" => Some(NamingPreset::Comics),
            "manga" => Some(NamingPreset::Manga),
            _ => None,
        }
    }
}

/// Replace characters Windows refuses, collapse whitespace, trim trailing dots and
/// spaces, and bound the length. Also strips path separators, so a metadata field
/// can never escape its folder.
pub fn sanitise_segment(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_space = false;
    for ch in input.chars() {
        let mapped = match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if (c as u32) < 0x20 => ' ',
            c => c,
        };
        if mapped == ' ' {
            if last_space || out.is_empty() {
                continue;
            }
            last_space = true;
            out.push(' ');
        } else {
            last_space = false;
            out.push(mapped);
        }
    }
    let trimmed = out.trim().trim_end_matches(['.', ' ']).to_string();
    // A run of dots is a traversal hazard in a derived name (`..`), so collapse it.
    let mut no_dots = trimmed;
    while no_dots.contains("..") {
        no_dots = no_dots.replace("..", "_");
    }
    let bounded: String = no_dots.chars().take(120).collect();
    if bounded.is_empty() {
        "untitled".to_string()
    } else {
        bounded
    }
}

fn extension_of(rel: &str) -> Option<String> {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && ext.len() <= 6 => Some(ext.to_ascii_lowercase()),
        _ => None,
    }
}

/// Volume label: digits become `v05`, anything else is kept word by word.
pub fn volume_label(volume: &str) -> Option<String> {
    let trimmed = volume.trim();
    if trimmed.is_empty() {
        return None;
    }
    let digits: String = trimmed.chars().take_while(char::is_ascii_digit).collect();
    if !digits.is_empty() && digits.len() == trimmed.len() {
        let n = digits.parse::<u32>().unwrap_or(0);
        return Some(if n < 100 {
            format!("v{n:02}")
        } else {
            format!("v{n}")
        });
    }
    Some(format!("v{}", sanitise_segment(trimmed)))
}

/// Target relative path for one item, or `None` when it is already named correctly.
pub fn target_for(input: &NamingInput, preset: NamingPreset) -> Option<String> {
    let series = input
        .series
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| input.title.clone());
    let series_segment = sanitise_segment(&series);
    let mut stem = series_segment.clone();
    if let Some(volume) = input.volume.as_deref().and_then(volume_label) {
        stem.push(' ');
        stem.push_str(&volume);
    } else if preset == NamingPreset::Comics {
        // No volume: keep the issue title so two books in a series do not collide.
        let title = sanitise_segment(&input.title);
        if !title.eq_ignore_ascii_case(&series_segment) {
            stem.push_str(" - ");
            stem.push_str(&title);
        }
    }
    if let Some(subtitle) = input
        .subtitle
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        stem.push_str(" - ");
        stem.push_str(&sanitise_segment(subtitle));
    }
    if let Some(year) = input.year {
        if (1900..=2099).contains(&year) {
            stem.push_str(&format!(" ({year})"));
        }
    }
    if preset == NamingPreset::Manga {
        if let Some(edition) = input.edition.as_deref().filter(|e| !e.trim().is_empty()) {
            stem.push_str(&format!(" [{}]", sanitise_segment(edition)));
        }
    }
    let file_name = if input.is_folder {
        stem
    } else {
        match extension_of(&input.rel) {
            Some(ext) => format!("{stem}.{ext}"),
            None => stem,
        }
    };
    let target = format!("{series_segment}/{file_name}");
    if normalise(&target) == normalise(&input.rel) {
        None
    } else {
        Some(target)
    }
}

fn normalise(rel: &str) -> String {
    rel.replace('\\', "/").to_lowercase()
}

// ------------------------------------------------------------------- plans

fn plan_id() -> String {
    let stamp = Utc::now().format("%Y%m%d%H%M%S").to_string();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{stamp}-{nanos:09}")
}

fn naming_input(row: &ItemRow) -> NamingInput {
    // The subtitle lives in the file name, not in the catalogue, so parse it here.
    let parsed = magpiecraft_comics::parse_volume_name(&row.name);
    NamingInput {
        rel: row.rel.clone(),
        title: row.title.clone(),
        series: row.series_title.clone(),
        volume: row.volume.clone(),
        subtitle: parsed.subtitle,
        year: None,
        edition: None,
        is_folder: row.container == "image_folder",
    }
}

impl Catalog {
    pub fn plans_dir(&self) -> PathBuf {
        self.paths().home().join("plans")
    }

    pub fn journals_dir(&self) -> PathBuf {
        self.paths().home().join("journal")
    }

    pub fn quarantine_root(&self) -> PathBuf {
        self.paths().home().join("quarantine")
    }

    fn save_plan(&self, plan: &Plan) -> MagpieResult<PathBuf> {
        let dir = self.plans_dir();
        std::fs::create_dir_all(&dir).map_err(|e| MagpieError::io(&dir, e))?;
        let file = dir.join(format!("{}.json", plan.id));
        let body = serde_json::to_string_pretty(plan)
            .map_err(|e| MagpieError::invalid(format!("plan: {e}")))?;
        std::fs::write(&file, body).map_err(|e| MagpieError::io(&file, e))?;
        Ok(file)
    }

    pub fn load_plan(&self, id: &str) -> MagpieResult<Plan> {
        let file = self.plans_dir().join(format!("{id}.json"));
        let body = std::fs::read_to_string(&file).map_err(|e| MagpieError::io(&file, e))?;
        serde_json::from_str(&body).map_err(|e| MagpieError::invalid(format!("plan {id}: {e}")))
    }

    /// Build a rename plan for one library or all of them. Writes nothing.
    pub fn plan_rename(
        &mut self,
        library: Option<&str>,
        preset: &str,
        limit: usize,
    ) -> MagpieResult<Value> {
        let preset = NamingPreset::parse(preset)
            .ok_or_else(|| MagpieError::invalid(format!("unknown naming preset {preset}")))?;
        let rows = self.target_libraries(library)?;
        let limit = if limit == 0 { 5000 } else { limit };
        let mut plans: Vec<Plan> = Vec::new();
        for lib in rows {
            let items = self.store_items_in_library(lib.id, limit)?;
            let mut ops: Vec<PlanOp> = Vec::new();
            let mut skipped: Vec<String> = Vec::new();
            let mut targets: HashSet<String> = HashSet::new();
            let mut bytes = 0u64;
            for row in &items {
                let input = naming_input(row);
                let Some(target_rel) = target_for(&input, preset) else {
                    continue;
                };
                let from = root_join(&lib.root, &row.rel);
                let to = root_join(&lib.root, &target_rel);
                if !Path::new(&from).exists() {
                    skipped.push(format!("{}: source is gone", row.rel));
                    continue;
                }
                if Path::new(&to).exists() {
                    skipped.push(format!("{}: {} already exists", row.rel, target_rel));
                    continue;
                }
                if !targets.insert(normalise(&target_rel)) {
                    skipped.push(format!("{}: two items map to {}", row.rel, target_rel));
                    continue;
                }
                bytes = bytes.saturating_add(u64::try_from(row.size).unwrap_or(0));
                ops.push(PlanOp::Move { from, to });
            }
            if ops.is_empty() && skipped.is_empty() {
                continue;
            }
            let plan = Plan {
                id: plan_id(),
                created_at: now(),
                kind: PlanKind::Rename.as_str().to_string(),
                library: lib.name.clone(),
                library_id: lib.id,
                root: lib.root.clone(),
                ops,
                skipped,
                bytes,
            };
            self.save_plan(&plan)?;
            plans.push(plan);
        }
        Ok(json!({
            "ok": true,
            "kind": "rename",
            "plans": plans.iter().map(plan_summary).collect::<Vec<_>>(),
            "operations": plans.iter().map(|p| p.ops.len()).sum::<usize>(),
            "note": "nothing has changed on disk; apply each plan by id with confirm=true",
        }))
    }

    /// Build a quarantine plan for one item: a move into the app's quarantine folder,
    /// never a delete.
    pub fn plan_quarantine(&mut self, key: &str) -> MagpieResult<Value> {
        let (lib, row) = self.resolve_item(key)?;
        let from = root_join(&lib.root, &row.rel);
        if !Path::new(&from).exists() {
            return Err(MagpieError::not_found(format!("{key} is not on disk")));
        }
        let stamp = Utc::now().format("%Y%m%d-%H%M%S").to_string();
        let target = self
            .quarantine_root()
            .join(&stamp)
            .join(sanitise_segment(&lib.name))
            .join(row.rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        let plan = Plan {
            id: plan_id(),
            created_at: now(),
            kind: PlanKind::Quarantine.as_str().to_string(),
            library: lib.name.clone(),
            library_id: lib.id,
            root: lib.root.clone(),
            ops: vec![PlanOp::Move {
                from,
                to: target.display().to_string(),
            }],
            skipped: Vec::new(),
            bytes: u64::try_from(row.size).unwrap_or(0),
        };
        self.save_plan(&plan)?;
        Ok(json!({
            "ok": true,
            "plans": vec![plan_summary(&plan)],
            "note": "the file is moved into quarantine, not deleted; undo restores it",
        }))
    }

    pub fn plans(&self) -> MagpieResult<Value> {
        let mut out: Vec<Value> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(self.plans_dir()) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(body) = std::fs::read_to_string(&path) {
                    if let Ok(plan) = serde_json::from_str::<Plan>(&body) {
                        out.push(plan_summary(&plan));
                    }
                }
            }
        }
        out.sort_by(|a, b| b["created_at"].as_str().cmp(&a["created_at"].as_str()));
        Ok(json!({ "count": out.len(), "plans": out }))
    }

    /// Apply one saved plan. Refuses without `confirm`.
    pub fn apply_plan(&mut self, id: &str, confirm: bool) -> MagpieResult<Value> {
        let plan = self.load_plan(id)?;
        if !confirm {
            return Err(MagpieError::ConfirmRequired(format!(
                "plan {id} would change {} path(s); pass confirm=true to apply it",
                plan.ops.len()
            )));
        }
        if plan.ops.is_empty() {
            return Err(MagpieError::invalid(format!("plan {id} is empty")));
        }
        let mut journal_ops: Vec<JournalOp> = Vec::new();
        let mut moved_items: Vec<(String, String)> = Vec::new();
        for op in &plan.ops {
            match op {
                PlanOp::Move { from, to } => {
                    let result = move_file(Path::new(from), Path::new(to));
                    journal_ops.push(JournalOp {
                        op: "move".to_string(),
                        from: Some(from.clone()),
                        to: to.clone(),
                        ok: result.is_ok(),
                        error: result.err(),
                    });
                    if journal_ops.last().map(|o| o.ok) == Some(true) {
                        if let Some(old_rel) = relative_to_root(&plan.root, from) {
                            if let Some(new_rel) = relative_to_root(&plan.root, to) {
                                moved_items.push((old_rel, new_rel));
                            }
                        }
                    }
                }
                PlanOp::Write { to, content } => {
                    let result = write_sidecar(Path::new(to), content);
                    journal_ops.push(JournalOp {
                        op: "write".to_string(),
                        from: None,
                        to: to.clone(),
                        ok: result.is_ok(),
                        error: result.err(),
                    });
                }
            }
        }
        // Keep the catalogue in step: paths and reading progress.
        for (old_rel, new_rel) in &moved_items {
            let rows = self.store().items_under_path(plan.library_id, old_rel)?;
            for (id, rel) in rows {
                let target = if rel == *old_rel {
                    new_rel.clone()
                } else {
                    format!("{new_rel}{}", rel.get(old_rel.len()..).unwrap_or(""))
                };
                self.store_mut().move_item_path(id, &rel, &target)?;
            }
        }
        let journal = Journal {
            id: plan_id(),
            plan_id: plan.id.clone(),
            created_at: now(),
            kind: plan.kind.clone(),
            library: plan.library.clone(),
            library_id: plan.library_id,
            root: plan.root.clone(),
            ops: journal_ops,
            undone: false,
            undone_at: None,
        };
        self.save_journal(&journal)?;
        // A scan rebuilds folders and series for the new layout. It is incremental,
        // so it does not reopen the archives it already knows.
        let scan = self.scan(Some(&plan.library), false).ok();
        Ok(json!({
            "ok": true,
            "journal": journal.id,
            "plan": plan.id,
            "applied": journal.applied(),
            "failed": journal.failed(),
            "errors": journal.ops.iter().filter(|o| !o.ok).map(|o| o.error.clone().unwrap_or_default()).collect::<Vec<_>>(),
            "scan_ms": scan.as_ref().map(|s| s.ms),
            "undo": format!("undo {}", journal.id),
        }))
    }

    fn save_journal(&self, journal: &Journal) -> MagpieResult<PathBuf> {
        let dir = self.journals_dir();
        std::fs::create_dir_all(&dir).map_err(|e| MagpieError::io(&dir, e))?;
        let file = dir.join(format!("{}.json", journal.id));
        let body = serde_json::to_string_pretty(journal)
            .map_err(|e| MagpieError::invalid(format!("journal: {e}")))?;
        std::fs::write(&file, body).map_err(|e| MagpieError::io(&file, e))?;
        Ok(file)
    }

    pub fn load_journal(&self, id: &str) -> MagpieResult<Journal> {
        let file = self.journals_dir().join(format!("{id}.json"));
        let body = std::fs::read_to_string(&file).map_err(|e| MagpieError::io(&file, e))?;
        serde_json::from_str(&body).map_err(|e| MagpieError::invalid(format!("journal {id}: {e}")))
    }

    pub fn journals(&self) -> MagpieResult<Value> {
        let mut out: Vec<Value> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(self.journals_dir()) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(body) = std::fs::read_to_string(&path) {
                    if let Ok(journal) = serde_json::from_str::<Journal>(&body) {
                        out.push(json!({
                            "id": journal.id,
                            "plan": journal.plan_id,
                            "created_at": journal.created_at,
                            "kind": journal.kind,
                            "library": journal.library,
                            "applied": journal.applied(),
                            "failed": journal.failed(),
                            "undone": journal.undone,
                        }));
                    }
                }
            }
        }
        out.sort_by(|a, b| b["created_at"].as_str().cmp(&a["created_at"].as_str()));
        Ok(json!({ "count": out.len(), "journals": out }))
    }

    /// Reverse an applied plan. Refuses without `confirm`, and refuses twice.
    pub fn undo(&mut self, journal_id: &str, confirm: bool) -> MagpieResult<Value> {
        let mut journal = self.load_journal(journal_id)?;
        if journal.undone {
            return Err(MagpieError::invalid(format!(
                "journal {journal_id} was already undone at {}",
                journal.undone_at.clone().unwrap_or_default()
            )));
        }
        if !confirm {
            return Err(MagpieError::ConfirmRequired(format!(
                "undoing {journal_id} would move {} path(s) back; pass confirm=true",
                journal.applied()
            )));
        }
        let mut restored = 0usize;
        let mut problems: Vec<String> = Vec::new();
        let mut moved_back: Vec<(String, String)> = Vec::new();
        for op in journal.ops.iter().filter(|o| o.ok) {
            match op.op.as_str() {
                "move" => {
                    let (Some(from), to) = (op.from.as_deref(), op.to.as_str()) else {
                        continue;
                    };
                    if !Path::new(to).exists() {
                        problems.push(format!("{to} is missing, cannot restore {from}"));
                        continue;
                    }
                    match move_file(Path::new(to), Path::new(from)) {
                        Ok(()) => {
                            restored += 1;
                            if let Some(old_rel) = relative_to_root(&journal.root, to) {
                                if let Some(new_rel) = relative_to_root(&journal.root, from) {
                                    moved_back.push((old_rel, new_rel));
                                }
                            }
                        }
                        Err(e) => problems.push(format!("{from}: {e}")),
                    }
                }
                "write" => match std::fs::remove_file(&op.to) {
                    Ok(()) => restored += 1,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => problems.push(format!("{}: {e}", op.to)),
                },
                _ => {}
            }
        }
        for (old_rel, new_rel) in &moved_back {
            let rows = self.store().items_under_path(journal.library_id, old_rel)?;
            for (id, rel) in rows {
                let target = if rel == *old_rel {
                    new_rel.clone()
                } else {
                    format!("{new_rel}{}", rel.get(old_rel.len()..).unwrap_or(""))
                };
                self.store_mut().move_item_path(id, &rel, &target)?;
            }
        }
        journal.undone = true;
        journal.undone_at = Some(now());
        self.save_journal(&journal)?;
        let scan = self.scan(Some(&journal.library), false).ok();
        Ok(json!({
            "ok": true,
            "journal": journal.id,
            "restored": restored,
            "problems": problems,
            "scan_ms": scan.as_ref().map(|s| s.ms),
        }))
    }
}

fn plan_summary(plan: &Plan) -> Value {
    json!({
        "id": plan.id,
        "created_at": plan.created_at,
        "kind": plan.kind,
        "library": plan.library,
        "operations": plan.ops.len(),
        "skipped": plan.skipped.len(),
        "bytes": plan.bytes,
        "diff": plan.diff(),
    })
}

fn root_join(root: &str, rel: &str) -> String {
    let mut path = PathBuf::from(root);
    for part in rel.split('/').filter(|p| !p.is_empty()) {
        path.push(part);
    }
    path.display().to_string()
}

fn relative_to_root(root: &str, path: &str) -> Option<String> {
    let root_path = Path::new(root);
    let path = Path::new(path);
    let rel = path.strip_prefix(root_path).ok()?;
    let mut parts: Vec<String> = Vec::new();
    for component in rel.components() {
        parts.push(component.as_os_str().to_string_lossy().to_string());
    }
    Some(parts.join("/"))
}

fn move_file(from: &Path, to: &Path) -> Result<(), String> {
    if !from.exists() {
        return Err(format!("{} does not exist", from.display()));
    }
    if to.exists() {
        return Err(format!(
            "{} already exists, refusing to overwrite",
            to.display()
        ));
    }
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::rename(from, to).map_err(|e| format!("{}: {e}", from.display()))
}

fn write_sidecar(to: &Path, content: &str) -> Result<(), String> {
    if to.exists() {
        return Err(format!(
            "{} already exists, refusing to overwrite",
            to.display()
        ));
    }
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(to, content).map_err(|e| format!("{}: {e}", to.display()))
}

/// Paths used by the journal and quarantine, exposed for tests and for the CLI
/// `paths` command.
pub fn plan_paths(paths: &Paths) -> Value {
    json!({
        "plans": paths.home().join("plans").display().to_string(),
        "journal": paths.home().join("journal").display().to_string(),
        "quarantine": paths.home().join("quarantine").display().to_string(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn input(
        rel: &str,
        title: &str,
        series: Option<&str>,
        volume: Option<&str>,
        year: Option<i32>,
    ) -> NamingInput {
        NamingInput {
            rel: rel.to_string(),
            title: title.to_string(),
            series: series.map(str::to_string),
            volume: volume.map(str::to_string),
            subtitle: None,
            year,
            edition: None,
            is_folder: rel.ends_with('/') || !rel.contains('.'),
        }
    }

    #[test]
    fn golden_comics_targets() {
        // (input, expected target or None when the name is already right)
        let cases: Vec<(NamingInput, Option<&str>)> = vec![
            (
                input(
                    "berserk/berserk v01.cbz",
                    "berserk v01",
                    Some("Berserk"),
                    Some("1"),
                    Some(1989),
                ),
                Some("Berserk/Berserk v01 (1989).cbz"),
            ),
            (
                input(
                    "Saga/Saga 054.cbz",
                    "Saga 054",
                    Some("Saga"),
                    Some("54"),
                    Some(2018),
                ),
                Some("Saga/Saga v54 (2018).cbz"),
            ),
            (
                input(
                    "Yotsuba/Yotsuba v05",
                    "Yotsuba v05",
                    Some("Yotsuba"),
                    Some("5"),
                    Some(2007),
                ),
                Some("Yotsuba/Yotsuba v05 (2007)"),
            ),
            (
                input(
                    "Berserk/Berserk v01 (1989).cbz",
                    "Berserk v01",
                    Some("Berserk"),
                    Some("1"),
                    Some(1989),
                ),
                None,
            ),
            (
                input(
                    "Loose/Loose Issue 003.cbz",
                    "Loose Issue 003",
                    None,
                    Some("3"),
                    None,
                ),
                Some("Loose Issue 003/Loose Issue 003 v03.cbz"),
            ),
        ];
        for (case, expected) in cases {
            assert_eq!(
                target_for(&case, NamingPreset::Comics).as_deref(),
                expected,
                "{case:?}"
            );
        }
    }

    #[test]
    fn manga_preset_appends_the_edition() {
        let mut case = input(
            "Berserk/Berserk v01.cbz",
            "Berserk v01",
            Some("Berserk"),
            Some("1"),
            Some(1989),
        );
        case.edition = Some("de".to_string());
        assert_eq!(
            target_for(&case, NamingPreset::Manga).as_deref(),
            Some("Berserk/Berserk v01 (1989) [de].cbz")
        );
    }

    #[test]
    fn sanitising_strips_windows_hostile_characters_and_escapes() {
        assert_eq!(sanitise_segment("A/B: C? D*"), "A_B_ C_ D_");
        assert_eq!(sanitise_segment("  spaced   out  "), "spaced out");
        assert_eq!(sanitise_segment("trailing... "), "trailing");
        assert_eq!(sanitise_segment(""), "untitled");
        let long = "x".repeat(500);
        assert_eq!(sanitise_segment(&long).chars().count(), 120);
    }

    #[test]
    fn volume_labels_are_stable() {
        assert_eq!(volume_label("5").as_deref(), Some("v05"));
        assert_eq!(volume_label("12").as_deref(), Some("v12"));
        assert_eq!(volume_label("123").as_deref(), Some("v123"));
        assert_eq!(volume_label("3a").as_deref(), Some("v3a"));
        assert_eq!(volume_label("  "), None);
    }

    #[test]
    fn a_rename_cannot_escape_its_folder() {
        let case = input("x/a.cbz", "../../evil", Some("../../etc"), Some("1"), None);
        let target = target_for(&case, NamingPreset::Comics).unwrap();
        assert!(!target.contains(".."), "{target}");
        assert!(target.starts_with("__"), "dots collapsed: {target}");
        assert_eq!(
            target.matches('/').count(),
            1,
            "one folder, one file: {target}"
        );
    }
}
