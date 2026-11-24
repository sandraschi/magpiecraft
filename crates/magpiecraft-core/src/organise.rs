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
    /// The folder the item sits in, when it may name a series (TV episodes).
    pub series_hint: Option<String>,
}

/// A preset decides the shape of the target name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamingPreset {
    /// `Series v05 (2007).cbz`
    Comics,
    /// `Series v05 (2007) [de].cbz`
    Manga,
    /// `Title (Year).mkv`
    Video,
}

impl NamingPreset {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "comics" | "" => Some(NamingPreset::Comics),
            "manga" => Some(NamingPreset::Manga),
            "video" | "movie" | "film" => Some(NamingPreset::Video),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            NamingPreset::Comics => "comics",
            NamingPreset::Manga => "manga",
            NamingPreset::Video => "video",
        }
    }
}

/// Release tags that are not part of a film's title.
const QUALITY_TAGS: &[&str] = &[
    "1080p", "720p", "2160p", "480p", "4k", "uhd", "hdr", "x264", "x265", "h264", "h265", "hevc",
    "avc", "webrip", "web-dl", "webdl", "web", "bluray", "blu-ray", "brrip", "bdrip", "dvdrip",
    "hdtv", "remux", "aac", "ac3", "dts", "ddp", "xvid", "divx", "multi", "proper", "repack",
    "internal", "dubbed", "subbed", "sub", "german", "ger", "eng", "english", "dl",
];

/// Title, year and any remaining text of a film file, or `None` when there is no
/// year to trust.
///
/// `Shaun.The.Sheep.The.Beast.Of.Mossy.Bottom.2026.mp4` becomes
/// `("Shaun The Sheep The Beast Of Mossy Bottom", 2026, "")`, and
/// `Shoah (1985) Part 2.mkv` becomes `("Shoah", 1985, "Part 2")` so a two-part film
/// does not collapse onto itself. Release tags after the year are dropped, and a
/// name without a year is not guessed at.
pub fn video_title_and_year(stem: &str) -> Option<(String, i32, String)> {
    let normalised = stem.replace(['.', '_'], " ");
    let tokens: Vec<&str> = normalised.split_whitespace().collect();
    let mut year: Option<i32> = None;
    let mut cut = tokens.len();
    for (i, token) in tokens.iter().enumerate().rev() {
        let digits: String = token.chars().filter(char::is_ascii_digit).collect();
        if digits.len() == 4 {
            if let Ok(value) = digits.parse::<i32>() {
                if (1900..=2099).contains(&value) {
                    year = Some(value);
                    cut = i;
                    break;
                }
            }
        }
    }
    let year = year?;
    let mut words: Vec<&str> = tokens.get(..cut)?.to_vec();
    while let Some(last) = words.last() {
        let lower = last.to_ascii_lowercase();
        let is_tag = QUALITY_TAGS.contains(&lower.as_str());
        let is_bracket = lower.starts_with('[') || lower.starts_with('(');
        if is_tag || is_bracket {
            words.pop();
        } else {
            break;
        }
    }
    let title = words
        .join(" ")
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_string();
    if title.is_empty() {
        return None;
    }
    let tail: Vec<&str> = tokens
        .iter()
        .skip(cut + 1)
        .filter(|t| !QUALITY_TAGS.contains(&t.to_ascii_lowercase().as_str()))
        .copied()
        .collect();
    let tail = tail
        .join(" ")
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_string();
    Some((title, year, tail))
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

/// Season and episode from a file name: `S01E02`, `s1e2`, `1x02`.
///
/// Returns `(season, episode)` and the byte offset just past the marker, so the
/// caller can keep the episode title that follows it.
pub fn episode_of(stem: &str) -> Option<(u32, u32, usize)> {
    let chars: Vec<char> = stem.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars.get(i).copied().unwrap_or(' ').to_ascii_lowercase();
        // s01e02 and s1e2
        if c == 's' {
            let mut j = i + 1;
            let start_season = j;
            while j < chars.len() && chars.get(j).is_some_and(|c| c.is_ascii_digit()) {
                j += 1;
            }
            if j > start_season && chars.get(j).is_some_and(|c| c.eq_ignore_ascii_case(&'e')) {
                let start_episode = j + 1;
                let mut k = start_episode;
                while k < chars.len() && chars.get(k).is_some_and(|c| c.is_ascii_digit()) {
                    k += 1;
                }
                if k > start_episode {
                    let season = collect_u32(&chars[start_season..j]);
                    let episode = collect_u32(&chars[start_episode..k]);
                    // Season 0 is legitimate: specials live there.
                    if episode > 0 {
                        return Some((season, episode, k));
                    }
                }
            }
        }
        // 1x02
        if c.is_ascii_digit() {
            let mut j = i;
            while j < chars.len() && chars.get(j).is_some_and(|c| c.is_ascii_digit()) {
                j += 1;
            }
            if chars.get(j).is_some_and(|c| c.eq_ignore_ascii_case(&'x')) {
                let start_episode = j + 1;
                let mut k = start_episode;
                while k < chars.len() && chars.get(k).is_some_and(|c| c.is_ascii_digit()) {
                    k += 1;
                }
                if k > start_episode {
                    let season = collect_u32(&chars[i..j]);
                    let episode = collect_u32(&chars[start_episode..k]);
                    // Only when it is a whole token, so a film title like "2001x" is safe.
                    let before_ok =
                        i == 0 || !chars.get(i - 1).is_some_and(|c| c.is_alphanumeric());
                    if episode > 0 && before_ok {
                        return Some((season, episode, k));
                    }
                }
            }
        }
        i += 1;
    }
    None
}

fn collect_u32(chars: &[char]) -> u32 {
    let text: String = chars.iter().collect();
    text.parse::<u32>().unwrap_or(0)
}

/// Folder names that are structural, not a series name.
fn is_structural_folder(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("season ")
        || lower.starts_with("series ")
        || lower == "specials"
        || lower.starts_with("disc ")
        || lower.starts_with("cd ")
}

/// A folder name as a series title: a trailing `(1984)` year is dropped, as are
/// separators left behind by it.
pub fn series_from_folder(folder: &str) -> String {
    let trimmed = folder.trim();
    let without_year = match trimmed.rfind('(') {
        Some(pos)
            if trimmed.ends_with(')')
                && trimmed
                    .get(pos + 1..trimmed.len().saturating_sub(1))
                    .map(|y| y.len() == 4 && y.chars().all(|c| c.is_ascii_digit()))
                    .unwrap_or(false) =>
        {
            trimmed.get(..pos).unwrap_or(trimmed)
        }
        _ => trimmed,
    };
    // "Dexter's Laboratory S02" is a season folder, not a series name.
    let mut words: Vec<&str> = without_year
        .trim_end_matches(['-', ' ', '.'])
        .split_whitespace()
        .collect();
    while let Some(last) = words.last() {
        let lower = last.to_ascii_lowercase();
        let season_token = lower
            .strip_prefix('s')
            .map(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
            .unwrap_or(false);
        if season_token || is_structural_folder(last) {
            words.pop();
        } else {
            break;
        }
    }
    sanitise_segment(&words.join(" "))
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

/// Target file name for one item, or `None` when it is already named correctly.
///
/// Only the name changes: an item is renamed inside the folder it already lives in.
/// A rename must never move a book out of an author or publisher folder the user
/// built, so the target has no directory part.
pub fn target_for(input: &NamingInput, preset: NamingPreset) -> Option<String> {
    if preset == NamingPreset::Video {
        // A season and episode marker is a stronger signal than a year, so it is
        // tried first: "Show S02E43 Title 2000" is an episode, not a film from 2000.
        if let Some((season, episode, tail)) = episode_of(&input.title) {
            let series = input
                .series_hint
                .as_deref()
                .map(series_from_folder)
                .filter(|s| !s.is_empty())?;
            let rest: String = input.title.chars().skip(tail).collect();
            let rest = rest.trim_matches(|c: char| !c.is_alphanumeric()).trim();
            let mut stem = format!("{series} - S{season:02}E{episode:02}");
            if !rest.is_empty() {
                stem.push_str(" - ");
                stem.push_str(&sanitise_segment(rest));
            }
            let file_name = match extension_of(&input.rel) {
                Some(ext) => format!("{stem}.{ext}"),
                None => stem,
            };
            return changed_or_none(input, &file_name);
        }
        if let Some((title, year, tail)) = video_title_and_year(&input.title) {
            let mut stem = format!("{} ({year})", sanitise_segment(&title));
            if !tail.is_empty() {
                stem.push(' ');
                stem.push_str(&sanitise_segment(&tail));
            }
            let file_name = match extension_of(&input.rel) {
                Some(ext) => format!("{stem}.{ext}"),
                None => stem,
            };
            return changed_or_none(input, &file_name);
        }
        return None;
    }
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

    changed_or_none(input, &file_name)
}

/// `None` when the name is already right, so a plan does not list no-op renames.
fn changed_or_none(input: &NamingInput, file_name: &str) -> Option<String> {
    let current = input.rel.rsplit('/').next().unwrap_or(&input.rel);
    if normalise(current) == normalise(file_name) {
        None
    } else {
        Some(file_name.to_string())
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
    let kind =
        crate::model::MediaKind::parse(&row.media_kind).unwrap_or(crate::model::MediaKind::Other);
    let series_hint = match kind {
        crate::model::MediaKind::Video => series_folder_hint(&row.parent),
        _ => None,
    };
    NamingInput {
        rel: row.rel.clone(),
        title: row.title.clone(),
        series: row.series_title.clone(),
        volume: row.volume.clone(),
        subtitle: parsed.subtitle,
        year: None,
        edition: None,
        is_folder: row.container == "image_folder",
        series_hint,
    }
}

/// The nearest folder above a video that looks like a show name, skipping
/// "Season 1" style folders.
fn series_folder_hint(parent: &str) -> Option<String> {
    let mut parts: Vec<&str> = parent.split('/').filter(|p| !p.is_empty()).collect();
    while let Some(last) = parts.last() {
        if is_structural_folder(last) {
            parts.pop();
        } else {
            break;
        }
    }
    parts.last().map(|s| s.to_string())
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
        // "auto" picks the preset per item from its media kind. An explicit preset
        // overrides that for every item.
        let forced =
            if preset.trim().is_empty() || preset == "auto" {
                None
            } else {
                Some(NamingPreset::parse(preset).ok_or_else(|| {
                    MagpieError::invalid(format!("unknown naming preset {preset}"))
                })?)
            };
        let rows = self.target_libraries(library)?;
        // 0 means every item: a plan is a proposal, so being able to see the whole
        // library at once is the point.
        let limit = if limit == 0 { usize::MAX } else { limit };
        let mut plans: Vec<Plan> = Vec::new();
        for lib in rows {
            let items = self.store_items_in_library(lib.id, limit)?;
            let mut ops: Vec<PlanOp> = Vec::new();
            let mut skipped: Vec<String> = Vec::new();
            let mut targets: HashSet<String> = HashSet::new();
            let mut bytes = 0u64;
            for row in &items {
                let kind = crate::model::MediaKind::parse(&row.media_kind).unwrap_or_else(|| {
                    crate::model::MediaKind::parse(&lib.kind)
                        .unwrap_or(crate::model::MediaKind::Other)
                });
                let chosen = match forced {
                    Some(p) => Some(p),
                    None => match kind {
                        crate::model::MediaKind::Comics
                        | crate::model::MediaKind::Manga
                        | crate::model::MediaKind::Magazines => Some(NamingPreset::Comics),
                        crate::model::MediaKind::Video => Some(NamingPreset::Video),
                        _ => None,
                    },
                };
                let Some(chosen) = chosen else {
                    skipped.push(format!(
                        "{}: {} naming is not automatic (tags or the owning app decide it)",
                        row.rel,
                        kind.as_str()
                    ));
                    continue;
                };
                // A name we do not understand is left alone: it is in the review
                // queue, not on the rename list.
                if chosen != NamingPreset::Video && row.confidence < 0.5 && !row.comic_info {
                    skipped.push(format!(
                        "{}: uncertain name ({:.2}), see review",
                        row.rel, row.confidence
                    ));
                    continue;
                }
                let input = naming_input(row);
                if chosen == NamingPreset::Video && video_title_and_year(&input.title).is_none() {
                    skipped.push(format!("{}: no year in the name", row.rel));
                    continue;
                }
                let Some(file_name) = target_for(&input, chosen) else {
                    // Already named correctly: not a skip worth reporting.
                    continue;
                };
                // Rename in place: never move an item out of its folder.
                let target_rel = match row.rel.rsplit_once('/') {
                    Some((parent, _)) => format!("{parent}/{file_name}"),
                    None => file_name.clone(),
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
            series_hint: None,
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
                Some("Berserk v01 (1989).cbz"),
            ),
            (
                input(
                    "Saga/Saga 054.cbz",
                    "Saga 054",
                    Some("Saga"),
                    Some("54"),
                    Some(2018),
                ),
                Some("Saga v54 (2018).cbz"),
            ),
            (
                input(
                    "Yotsuba/Yotsuba v05",
                    "Yotsuba v05",
                    Some("Yotsuba"),
                    Some("5"),
                    Some(2007),
                ),
                Some("Yotsuba v05 (2007)"),
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
                Some("Loose Issue 003 v03.cbz"),
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
            Some("Berserk v01 (1989) [de].cbz")
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
    fn video_names_become_title_and_year() {
        assert_eq!(
            video_title_and_year("Shaun.The.Sheep.The.Beast.Of.Mossy.Bottom.2026")
                .map(|(t, y, _)| (t, y)),
            Some((
                "Shaun The Sheep The Beast Of Mossy Bottom".to_string(),
                2026
            ))
        );
        assert_eq!(
            video_title_and_year("Zootopia .2 2025").map(|(t, y, _)| (t, y)),
            Some(("Zootopia 2".to_string(), 2025))
        );
        assert_eq!(
            video_title_and_year("Despicable Me Minion Madness 2010").map(|(t, y, _)| (t, y)),
            Some(("Despicable Me Minion Madness".to_string(), 2010))
        );
        // A release tag after the year is not part of the title.
        assert_eq!(
            video_title_and_year("Alien 1979 1080p BluRay x265").map(|(t, y, _)| (t, y)),
            Some(("Alien".to_string(), 1979))
        );
        // Text after the year is kept, so a two-part film does not collapse.
        assert_eq!(
            video_title_and_year("Shoah (1985) Part 2"),
            Some(("Shoah".to_string(), 1985, "Part 2".to_string()))
        );
        // No year: not guessed at.
        assert_eq!(video_title_and_year("Some Home Video"), None);
        assert_eq!(video_title_and_year("2025"), None);
    }

    #[test]
    fn a_film_is_renamed_in_place_from_its_year() {
        let case = NamingInput {
            rel: "Video/Shaun.The.Sheep.The.Beast.Of.Mossy.Bottom.2026.mp4".to_string(),
            title: "Shaun.The.Sheep.The.Beast.Of.Mossy.Bottom.2026".to_string(),
            series: None,
            volume: None,
            subtitle: None,
            year: None,
            edition: None,
            is_folder: false,
            series_hint: None,
        };
        assert_eq!(
            target_for(&case, NamingPreset::Video).as_deref(),
            Some("Shaun The Sheep The Beast Of Mossy Bottom (2026).mp4")
        );
        let already = NamingInput {
            rel: "Video/Alien (1979).mkv".to_string(),
            title: "Alien (1979)".to_string(),
            ..case
        };
        assert_eq!(target_for(&already, NamingPreset::Video), None);
    }

    #[test]
    fn an_episode_is_named_from_its_marker_and_folder() {
        assert_eq!(
            episode_of("Alfred J  Kwak s01e01 Alfred comes to life Part 1").map(|(s, e, _)| (s, e)),
            Some((1, 1))
        );
        assert_eq!(
            episode_of("Friends - 1x02 - The One with the Sonogram").map(|(s, e, _)| (s, e)),
            Some((1, 2))
        );
        assert_eq!(
            episode_of("Show S2E05").map(|(s, e, _)| (s, e)),
            Some((2, 5))
        );
        let (_, _, tail) = episode_of("Show S2E05 - Pilot").unwrap();
        assert_eq!(
            "Show S2E05 - Pilot".chars().skip(tail).collect::<String>(),
            " - Pilot"
        );
        assert_eq!(episode_of("A Film 2019"), None);
        assert_eq!(episode_of("S01 without an episode"), None);

        let case = NamingInput {
            rel: "TV/Friends (1994)/Friends - 1x02 - The One with the Sonogram.mkv".to_string(),
            title: "Friends - 1x02 - The One with the Sonogram".to_string(),
            series: None,
            volume: None,
            subtitle: None,
            year: None,
            edition: None,
            is_folder: false,
            series_hint: Some("Friends (1994)".to_string()),
        };
        assert_eq!(
            target_for(&case, NamingPreset::Video).as_deref(),
            Some("Friends - S01E02 - The One with the Sonogram.mkv"),
            "the year leaves the series name and the marker is normalised"
        );

        let no_title = NamingInput {
            rel: "TV/Show/Season 1/Show s02e05.mkv".to_string(),
            title: "Show s02e05".to_string(),
            series_hint: Some("Show".to_string()),
            ..case.clone()
        };
        assert_eq!(
            target_for(&no_title, NamingPreset::Video).as_deref(),
            Some("Show - S02E05.mkv")
        );
        // "Season 1" is structural, so the show comes from the folder above it.
        assert_eq!(
            series_folder_hint("TV/Show/Season 1"),
            Some("Show".to_string())
        );
        assert_eq!(
            series_folder_hint("Video/Murder, She Wrote (1984)"),
            Some("Murder, She Wrote (1984)".to_string())
        );
        assert_eq!(
            series_from_folder("Murder, She Wrote (1984)"),
            "Murder, She Wrote"
        );
    }

    #[test]
    fn a_rename_cannot_escape_its_folder() {
        let case = input("x/a.cbz", "../../evil", Some("../../etc"), Some("1"), None);
        let target = target_for(&case, NamingPreset::Comics).unwrap();
        assert!(!target.contains(".."), "{target}");
        assert!(
            !target.contains('/') && !target.contains('\\'),
            "no separators: {target}"
        );
        assert!(target.starts_with("__"), "dots collapsed: {target}");
    }
}
