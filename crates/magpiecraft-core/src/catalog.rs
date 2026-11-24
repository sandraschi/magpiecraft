//! The catalogue engine: the only place that talks to the database, the file
//! system and the external tools together.
//!
//! Everything the CLI, the MCP server and the desktop app can do is a method here,
//! and the command bus in `command.rs` is a thin typed wrapper over these methods.
//!
//! Rules that the code must keep:
//! - scanning never writes into a library;
//! - an unchanged file is not opened again (incremental by size and mtime);
//! - an item that vanished is marked missing, never deleted;
//! - a container this build cannot read reports `not_implemented`, not an empty list.

use crate::config::{load_settings, save_settings, Paths, Settings};
use crate::cover::{cache_file_for, CoverSource};
use crate::error::{MagpieError, MagpieResult};
use crate::images;
use crate::model::*;
use chrono::{SecondsFormat, Utc};
use magpiecraft_comics::{
    self, container_for_name, display_title, parse_volume_name, zip_comic_info, ComicInfo, PageSet,
};
use magpiecraft_health::{self as health, DupGroup};
use magpiecraft_scan::{self as scan, WalkOptions};
use magpiecraft_store::{ItemRow, LibraryRow, NewItem, Store, Upsert};
use magpiecraft_tools::{self, Toolchain};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// How a search should match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    /// FTS5 prefix matching only.
    Fts,
    /// Substring matching over title, series and path.
    Substring,
    /// FTS5 first, substring if that finds nothing.
    Auto,
}

impl SearchMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "fts" => Some(SearchMode::Fts),
            "substring" | "like" => Some(SearchMode::Substring),
            "auto" | "" => Some(SearchMode::Auto),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            SearchMode::Fts => "fts",
            SearchMode::Substring => "substring",
            SearchMode::Auto => "auto",
        }
    }
}

/// One open catalogue. Cheap to construct, holds one SQLite connection.
pub struct Catalog {
    paths: Paths,
    settings: Settings,
    store: Store,
    tools: Toolchain,
}

impl Catalog {
    pub fn open(paths: Paths) -> MagpieResult<Self> {
        paths.ensure()?;
        let store = Store::open(&paths.db_file())?;
        let settings = load_settings(&paths);
        Ok(Catalog {
            paths,
            settings,
            store,
            tools: Toolchain::detect(),
        })
    }

    pub fn open_default() -> MagpieResult<Self> {
        Catalog::open(Paths::discover())
    }

    /// In-memory catalogue for tests and `--smoke`. Covers still go to a temp
    /// directory under the given home so nothing is written to a real library.
    pub fn open_in_memory(home: Paths) -> MagpieResult<Self> {
        Ok(Catalog {
            paths: home,
            settings: Settings::default(),
            store: Store::open_in_memory()?,
            tools: Toolchain::detect(),
        })
    }

    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn tools(&self) -> &Toolchain {
        &self.tools
    }

    /// Replace the detected toolchain. Tests use it to prove both states: with the
    /// tool (real extraction) and without it (`not_configured`).
    pub fn set_tools(&mut self, tools: Toolchain) {
        self.tools = tools;
    }

    pub(crate) fn store(&self) -> &Store {
        &self.store
    }

    pub(crate) fn store_mut(&mut self) -> &mut Store {
        &mut self.store
    }

    /// One library by name or id, or every library when `library` is `None`.
    pub fn target_libraries(&self, library: Option<&str>) -> MagpieResult<Vec<LibraryRow>> {
        match library {
            Some(key) => Ok(vec![self.library_row(key)?]),
            None => Ok(self.store.libraries()?),
        }
    }

    /// Live items of one library, in path order, capped.
    pub fn store_items_in_library(
        &self,
        library_id: i64,
        limit: usize,
    ) -> MagpieResult<Vec<ItemRow>> {
        Ok(self.store.items_in_library(library_id, limit)?)
    }

    /// Resolve an item key to its library row and item row.
    pub fn resolve_item(&self, key: &str) -> MagpieResult<(LibraryRow, ItemRow)> {
        let ik = ItemKey::parse(key)
            .ok_or_else(|| MagpieError::invalid(format!("not an item key: {key}")))?;
        let lib = self.library_row(&ik.library_id.to_string())?;
        let row = self
            .store
            .item_by_rel(lib.id, &ik.rel)?
            .ok_or_else(|| MagpieError::not_found(format!("no item at {key}")))?;
        Ok((lib, row))
    }

    /// Tool paths and cache root for containers that go through an external tool.
    pub fn helpers(&self) -> magpiecraft_comics::Helpers {
        let mut helpers = magpiecraft_comics::Helpers::new(
            self.tools.seven_zip.clone(),
            self.tools.pdfcraft.clone(),
            self.paths.cache_dir(),
        );
        helpers.pdf_dpi = self.settings.pdf_dpi;
        helpers.extract_cap_bytes = self.settings.extract_cache_mb.saturating_mul(1024 * 1024);
        helpers.render_cap_bytes = self.settings.render_cache_mb.saturating_mul(1024 * 1024);
        helpers
    }

    /// Whether this build can read the container *with the tools present right
    /// now*. A CBR without 7-Zip is not readable, and says so.
    pub fn container_readable(&self, container: ContainerKind) -> bool {
        match container {
            ContainerKind::Rar => self.helpers().has_seven_zip(),
            ContainerKind::Pdf => self.helpers().has_pdfcraft(),
            other => other.readable_now(),
        }
    }

    pub fn set_settings(&mut self, settings: Settings) -> MagpieResult<Settings> {
        self.settings = settings.sanitised();
        save_settings(&self.paths, &self.settings)?;
        Ok(self.settings.clone())
    }

    // ------------------------------------------------------------ libraries
    pub fn libraries(&self) -> MagpieResult<Vec<Library>> {
        Ok(self
            .store
            .libraries()?
            .into_iter()
            .map(library_view)
            .collect())
    }

    fn library_row(&self, name_or_id: &str) -> MagpieResult<LibraryRow> {
        let trimmed = name_or_id.trim();
        if trimmed.is_empty() {
            return Err(MagpieError::invalid("library name is empty"));
        }
        if let Ok(id) = trimmed.parse::<i64>() {
            if let Some(row) = self.store.library(id)? {
                return Ok(row);
            }
        }
        self.store
            .library_by_name(trimmed)?
            .ok_or_else(|| MagpieError::not_found(format!("library {trimmed} is not configured")))
    }

    pub fn add_library(
        &mut self,
        name: &str,
        root: &str,
        kind: &str,
        mature: bool,
        rtl_default: Option<bool>,
    ) -> MagpieResult<Library> {
        let name = name.trim();
        if name.is_empty() {
            return Err(MagpieError::invalid("library name is empty"));
        }
        let root_path = PathBuf::from(root.trim());
        if root.trim().is_empty() {
            return Err(MagpieError::invalid("library root is empty"));
        }
        if !root_path.is_dir() {
            return Err(MagpieError::invalid(format!(
                "library root {} is not a directory",
                root_path.display()
            )));
        }
        let media_kind = MediaKind::parse(kind)
            .ok_or_else(|| MagpieError::invalid(format!("unknown library kind {kind}")))?;
        let absolute = if root_path.is_absolute() {
            root_path
        } else {
            let cwd =
                std::env::current_dir().map_err(|e| MagpieError::Io(".".into(), e.to_string()))?;
            cwd.join(root_path)
        };
        let root_text = absolute.display().to_string();
        if self.store.library_by_name(name)?.is_some() {
            return Err(MagpieError::invalid(format!(
                "a library named {name} already exists"
            )));
        }
        let id =
            self.store
                .add_library(name, &root_text, media_kind.as_str(), mature, rtl_default)?;
        let row = self.store.library(id)?.ok_or_else(|| {
            MagpieError::not_found(format!("library {name} vanished after insert"))
        })?;
        Ok(library_view(row))
    }

    pub fn remove_library(&mut self, name_or_id: &str, confirm: bool) -> MagpieResult<Value> {
        if !confirm {
            return Err(MagpieError::ConfirmRequired(format!(
                "removing library {name_or_id} deletes its index (the files are untouched); pass confirm=true"
            )));
        }
        let row = self.library_row(name_or_id)?;
        let items = self.store.count_items_under(row.id, "")?;
        self.store.remove_library(row.id)?;
        Ok(json!({
            "removed": row.name,
            "library_id": row.id,
            "items_dropped_from_index": items,
            "files_deleted": 0,
        }))
    }

    // ---------------------------------------------------------------- scan
    pub fn scan(&mut self, library: Option<&str>, full: bool) -> MagpieResult<ScanReportView> {
        let rows: Vec<LibraryRow> = match library {
            Some(key) => vec![self.library_row(key)?],
            None => self.store.libraries()?,
        };
        let started = Instant::now();
        let mut report = ScanReportView {
            full,
            libraries: Vec::new(),
            added: 0,
            updated: 0,
            unchanged: 0,
            missing: 0,
            ms: 0,
        };
        for row in rows {
            let entry = self.scan_library(&row, full)?;
            report.added += entry.added;
            report.updated += entry.updated;
            report.unchanged += entry.unchanged;
            report.missing += entry.missing;
            report.libraries.push(entry);
        }
        report.ms = started.elapsed().as_millis();
        Ok(report)
    }

    fn scan_library(&mut self, lib: &LibraryRow, full: bool) -> MagpieResult<ScanItemReport> {
        let started = Instant::now();
        let root = PathBuf::from(&lib.root);
        let mut entry = ScanItemReport {
            library: lib.name.clone(),
            library_id: lib.id,
            root: lib.root.clone(),
            added: 0,
            updated: 0,
            unchanged: 0,
            missing: 0,
            dirs: 0,
            items: 0,
            ms: 0,
            skipped: false,
            note: None,
        };
        if !root.is_dir() {
            entry.skipped = true;
            entry.note = Some(format!(
                "root is not reachable (an unplugged drive keeps its index): {}",
                lib.root
            ));
            entry.ms = started.elapsed().as_millis();
            return Ok(entry);
        }
        let walk = scan::walk(
            &root,
            &WalkOptions {
                min_pages: self.settings.imgdir_min_pages,
                follow_links: false,
                limit: 0,
            },
        )?;
        self.store.clear_dirs(lib.id)?;
        for dir in &walk.dirs {
            self.store
                .upsert_dir(lib.id, &dir.rel, &dir.parent, &dir.name, dir.volume)?;
        }
        entry.dirs = walk.dirs.len();

        let mut seen: HashSet<String> = HashSet::new();
        let mut added = 0usize;
        let mut updated = 0usize;
        let mut unchanged = 0usize;

        for file in &walk.files {
            let container = match container_for_name(&file.name) {
                Some(c) => c,
                None => continue,
            };
            let existing = self.store.item_by_rel(lib.id, &file.rel)?;
            let needs_open = full
                || match &existing {
                    None => true,
                    Some(row) => {
                        row.size != i64::try_from(file.size).unwrap_or(i64::MAX)
                            || row.mtime != file.mtime
                            || row.page_count.is_none()
                    }
                };
            let (page_count, comic) = if needs_open {
                self.probe_container(&container, &root.join(rel_to_path(&file.rel)))
            } else {
                (existing.as_ref().and_then(|r| r.page_count), None)
            };
            let new_item = {
                let mut item = build_file_item(lib, file, container, page_count, comic.as_ref());
                // When the file was not reopened, ComicInfo is not in hand, so keep
                // what the first scan learned instead of downgrading it.
                if !needs_open {
                    if let Some(row) = existing.as_ref() {
                        if row.comic_info {
                            item.confidence = row.confidence;
                            item.needs_review = row.needs_review;
                            item.volume = row.volume.clone().or(item.volume);
                        }
                    }
                }
                item
            };
            let (_, state) = self.store.upsert_item(&new_item)?;
            match state {
                Upsert::Added => added += 1,
                Upsert::Updated => updated += 1,
                Upsert::Unchanged => unchanged += 1,
            }
            seen.insert(file.rel.clone());
        }

        for dir in walk.dirs.iter().filter(|d| d.volume) {
            let parsed = parse_volume_name(&dir.name);
            let series_title = series_title_for(&dir.parent, &dir.name, None, &parsed);
            let new_item = NewItem {
                library_id: lib.id,
                rel: dir.rel.clone(),
                parent: dir.parent.clone(),
                name: dir.name.clone(),
                title: dir.name.clone(),
                container: ContainerKind::ImageFolder.as_str().to_string(),
                size: 0,
                mtime: 0,
                page_count: Some(i64::try_from(dir.images).unwrap_or(i64::MAX)),
                series_rel: Some(series_rel(&dir.parent, &series_title)),
                series_title: Some(series_title),
                volume: parsed.volume.clone(),
                rtl: None,
                comic_info: false,
                confidence: f64::from(parsed.confidence.clamp(0.0, 1.0)),
                needs_review: parsed.needs_review,
            };
            let (_, state) = self.store.upsert_item(&new_item)?;
            match state {
                Upsert::Added => added += 1,
                Upsert::Updated => updated += 1,
                Upsert::Unchanged => unchanged += 1,
            }
            seen.insert(dir.rel.clone());
        }

        entry.missing = self.store.mark_missing_except(lib.id, &seen)?;
        entry.added = added;
        entry.updated = updated;
        entry.unchanged = unchanged;
        entry.items = added + updated + unchanged;
        entry.ms = started.elapsed().as_millis();
        let _ = self.store.record_scan(&magpiecraft_store::ScanRecord {
            id: 0,
            library_id: Some(lib.id),
            started_at: now(),
            finished_at: Some(now()),
            added,
            updated,
            missing: entry.missing,
            total: entry.items,
            ms: usize::try_from(entry.ms).unwrap_or(usize::MAX),
            full,
        });
        Ok(entry)
    }

    /// Page count and ComicInfo for a container, without failing the scan when the
    /// file is broken: a corrupt archive is still indexed so the user can see it.
    fn probe_container(
        &self,
        container: &ContainerKind,
        path: &Path,
    ) -> (Option<i64>, Option<ComicInfo>) {
        let helpers = self.helpers();
        let to_count = |n: usize| i64::try_from(n).unwrap_or(i64::MAX);
        match container {
            ContainerKind::Zip => {
                let pages = PageSet::open(path, ContainerKind::Zip, &helpers)
                    .ok()
                    .map(|s| to_count(s.len()));
                let comic = zip_comic_info(path).ok().flatten();
                (pages, comic)
            }
            ContainerKind::Rar => {
                // An index read through 7-Zip, no extraction. Costs about 150 ms per
                // archive, once, when the item is new.
                let pages = magpiecraft_comics::rar::list(path, &helpers)
                    .ok()
                    .map(|p| to_count(p.len()));
                (pages, None)
            }
            ContainerKind::Pdf => {
                let pages = magpiecraft_comics::pdf::count(path, &helpers)
                    .ok()
                    .map(to_count);
                (pages, None)
            }
            _ => (None, None),
        }
    }

    // -------------------------------------------------------------- browse
    pub fn browse(&self, key: Option<&str>) -> MagpieResult<BrowseView> {
        let mature_visible = self.settings.mature_visible;
        match key {
            None => {
                let libs = self.store.libraries()?;
                let mut folders = Vec::new();
                for lib in libs.iter().filter(|l| mature_visible || !l.mature) {
                    let items = self.store.count_items_under(lib.id, "")?;
                    let cover = self.store.first_item_under(lib.id, "")?.map(|i| i.key());
                    folders.push(FolderView {
                        key: ItemKey::root(lib.id).as_string(),
                        library_id: lib.id,
                        rel: String::new(),
                        name: lib.name.clone(),
                        cover_key: cover,
                        items,
                    });
                }
                Ok(BrowseView {
                    kind: None,
                    library: None,
                    mature: mature_visible,
                    crumbs: vec![Crumb {
                        key: None,
                        name: "Libraries".to_string(),
                    }],
                    total_folders: folders.len(),
                    total_items: 0,
                    folders,
                    items: Vec::new(),
                })
            }
            Some(raw) => {
                let key = ItemKey::parse(raw)
                    .ok_or_else(|| MagpieError::invalid(format!("not an item key: {raw}")))?;
                let lib = self.library_row(&key.library_id.to_string())?;
                if lib.mature && !mature_visible {
                    return Err(MagpieError::not_found(
                        "this library is flagged mature; enable mature content in Settings to browse it",
                    ));
                }
                let mut crumbs = vec![
                    Crumb {
                        key: None,
                        name: "Libraries".to_string(),
                    },
                    Crumb {
                        key: Some(ItemKey::root(lib.id).as_string()),
                        name: lib.name.clone(),
                    },
                ];
                let mut acc = String::new();
                for part in key.rel.split('/').filter(|p| !p.is_empty()) {
                    acc = if acc.is_empty() {
                        part.to_string()
                    } else {
                        format!("{acc}/{part}")
                    };
                    crumbs.push(Crumb {
                        key: Some(ItemKey::new(lib.id, &acc).as_string()),
                        name: part.to_string(),
                    });
                }
                let dir_rows = self.store.dirs_under(lib.id, &key.rel)?;
                let mut folders = Vec::with_capacity(dir_rows.len());
                for dir in &dir_rows {
                    folders.push(FolderView {
                        key: ItemKey::new(lib.id, &dir.rel).as_string(),
                        library_id: lib.id,
                        rel: dir.rel.clone(),
                        name: dir.name.clone(),
                        cover_key: self
                            .store
                            .first_item_under(lib.id, &dir.rel)?
                            .map(|i| i.key()),
                        items: self.store.count_items_under(lib.id, &dir.rel)?,
                    });
                }
                let item_rows = self.store.items_under(lib.id, &key.rel)?;
                let items = item_rows
                    .iter()
                    .map(|row| self.item_view(&lib, row))
                    .collect::<Vec<_>>();
                Ok(BrowseView {
                    kind: MediaKind::parse(&lib.kind),
                    library: Some(lib.name.clone()),
                    mature: lib.mature,
                    crumbs,
                    total_folders: folders.len(),
                    total_items: items.len(),
                    folders,
                    items,
                })
            }
        }
    }

    // -------------------------------------------------------------- search
    pub fn search(&self, query: &str, limit: usize, mode: SearchMode) -> MagpieResult<SearchView> {
        let query = query.trim().to_string();
        let limit = if limit == 0 {
            self.settings.search_limit
        } else {
            limit
        };
        let mut used = mode;
        let mut rows: Vec<ItemRow> = Vec::new();
        if !query.is_empty() {
            if mode != SearchMode::Substring {
                rows = self.store.search_fts(&query, limit)?;
            }
            if rows.is_empty() && mode != SearchMode::Fts {
                rows = self.store.search_like(&query, limit)?;
                used = SearchMode::Substring;
            } else if mode == SearchMode::Auto {
                used = SearchMode::Fts;
            }
        }
        let libs = self.store.libraries()?;
        let hits = rows
            .iter()
            .map(|row| {
                let library = libs
                    .iter()
                    .find(|l| l.id == row.library_id)
                    .map(|l| l.name.clone())
                    .unwrap_or_default();
                SearchHit {
                    key: row.key(),
                    title: row.title.clone(),
                    library,
                    where_: row.parent.clone(),
                    container: ContainerKind::parse(&row.container).unwrap_or(ContainerKind::File),
                    page_count: row.page_count,
                }
            })
            .collect::<Vec<_>>();
        Ok(SearchView {
            count: hits.len(),
            truncated: hits.len() >= limit,
            mode: used.as_str().to_string(),
            query,
            hits,
        })
    }

    // ---------------------------------------------------------------- item
    pub fn item(&self, key: &str) -> MagpieResult<ItemView> {
        let ik = ItemKey::parse(key)
            .ok_or_else(|| MagpieError::invalid(format!("not an item key: {key}")))?;
        let lib = self.library_row(&ik.library_id.to_string())?;
        let row = self
            .store
            .item_by_rel(lib.id, &ik.rel)?
            .ok_or_else(|| MagpieError::not_found(format!("no item at {key}")))?;
        Ok(self.item_view(&lib, &row))
    }

    fn item_view(&self, lib: &LibraryRow, row: &ItemRow) -> ItemView {
        let container = ContainerKind::parse(&row.container).unwrap_or(ContainerKind::File);
        ItemView {
            key: row.key(),
            library_id: row.library_id,
            library: lib.name.clone(),
            rel: row.rel.clone(),
            parent: row.parent.clone(),
            name: row.name.clone(),
            title: row.title.clone(),
            container,
            readable: self.container_readable(container),
            size: row.size,
            mtime: row.mtime,
            page_count: row.page_count,
            series: row.series_title.clone(),
            series_id: row.series_id,
            volume: row.volume.clone(),
            rtl: effective_rtl(lib, row),
            page_offset: row.page_offset,
            confidence: row.confidence,
            needs_review: row.needs_review,
            comic_info: row.comic_info,
            progress: self.progress_view(&row.key()),
        }
    }

    // --------------------------------------------------------------- pages
    pub fn pages(&self, key: &str) -> MagpieResult<PageSetInfo> {
        let ik = ItemKey::parse(key)
            .ok_or_else(|| MagpieError::invalid(format!("not an item key: {key}")))?;
        let lib = self.library_row(&ik.library_id.to_string())?;
        let row = self
            .store
            .item_by_rel(lib.id, &ik.rel)?
            .ok_or_else(|| MagpieError::not_found(format!("no item at {key}")))?;
        let container = ContainerKind::parse(&row.container).unwrap_or(ContainerKind::File);
        let path = library_path(&lib, &row.rel);
        let (readable, count, preview, reason) =
            match PageSet::open(&path, container, &self.helpers()) {
                Ok(set) => {
                    let preview: Vec<String> =
                        set.pages().iter().take(3).cloned().collect::<Vec<_>>();
                    (true, set.len(), preview, None)
                }
                Err(e) => (
                    false,
                    usize::try_from(row.page_count.unwrap_or(0)).unwrap_or(0),
                    Vec::new(),
                    Some(e.to_string()),
                ),
            };
        let back = ItemKey::new(lib.id, &row.parent).as_string();
        Ok(PageSetInfo {
            key: row.key(),
            title: row.title.clone(),
            container,
            readable,
            reason,
            count,
            rtl: effective_rtl(&lib, &row),
            page_offset: row.page_offset,
            fit: "width".to_string(),
            preview,
            progress: self.progress_view(&row.key()),
            back_key: back,
        })
    }

    /// Open a page set for the desktop reader. Decoding happens in the app.
    pub fn page_set(&self, key: &str) -> MagpieResult<PageSet> {
        let ik = ItemKey::parse(key)
            .ok_or_else(|| MagpieError::invalid(format!("not an item key: {key}")))?;
        let lib = self.library_row(&ik.library_id.to_string())?;
        let row = self
            .store
            .item_by_rel(lib.id, &ik.rel)?
            .ok_or_else(|| MagpieError::not_found(format!("no item at {key}")))?;
        let container = ContainerKind::parse(&row.container).unwrap_or(ContainerKind::File);
        Ok(PageSet::open(
            &library_path(&lib, &row.rel),
            container,
            &self.helpers(),
        )?)
    }

    /// Resolve the cover for an item or folder without touching the image.
    pub fn cover_source(&self, key: &str) -> MagpieResult<CoverSource> {
        let ik = ItemKey::parse(key)
            .ok_or_else(|| MagpieError::invalid(format!("not an item key: {key}")))?;
        let lib = self.library_row(&ik.library_id.to_string())?;
        let row = match self.store.item_by_rel(lib.id, &ik.rel)? {
            Some(row) => row,
            None => self
                .store
                .first_item_under(lib.id, &ik.rel)?
                .ok_or_else(|| MagpieError::not_found(format!("no readable item at {key}")))?,
        };
        let container = ContainerKind::parse(&row.container).unwrap_or(ContainerKind::File);
        Ok(CoverSource {
            cache_file: cache_file_for(&self.paths.covers_dir(), &row.key()),
            item_path: library_path(&lib, &row.rel),
            container,
            helpers: self.helpers(),
        })
    }

    /// Render a cover now (CLI, MCP, tests). The app uses `cover_source` plus a
    /// worker thread instead.
    pub fn cover_path(&self, key: &str) -> MagpieResult<PathBuf> {
        let source = self.cover_source(key)?;
        crate::cover::render_cover(
            &source,
            self.settings.cover_width,
            self.settings.cover_height,
            self.settings.cover_quality,
        )
    }

    /// Decode one page to RGBA at a bound, for callers that do not have a GPU.
    pub fn page_image(
        &self,
        key: &str,
        page: usize,
        max_w: u32,
        max_h: u32,
    ) -> MagpieResult<image::RgbaImage> {
        let set = self.page_set(key)?;
        let bytes = set.read(page)?;
        images::decode_fitted(&bytes, max_w, max_h)
    }

    /// Write one page to a file. The bytes are copied, never re-encoded, so an
    /// exported page is exactly the page in the archive.
    pub fn export_page(&self, key: &str, page: usize, out: &str) -> MagpieResult<Value> {
        if out.trim().is_empty() {
            return Err(MagpieError::invalid("output path is empty"));
        }
        let set = self.page_set(key)?;
        let bytes = set.read(page)?;
        let page_name = set.name(page).unwrap_or("").to_string();
        let target = PathBuf::from(out);
        if let Some(dir) = target.parent() {
            if !dir.as_os_str().is_empty() {
                std::fs::create_dir_all(dir).map_err(|e| MagpieError::io(dir, e))?;
            }
        }
        std::fs::write(&target, &bytes).map_err(|e| MagpieError::io(&target, e))?;
        Ok(json!({
            "ok": true,
            "key": key,
            "page": page,
            "page_name": page_name,
            "out": target.display().to_string(),
            "bytes": bytes.len(),
        }))
    }

    // --------------------------------------------------------------- review
    /// The review queue: items whose filename parse was uncertain. Nothing here was
    /// guessed silently; each one says what it thinks and how sure it is.
    pub fn review(&self, library: Option<&str>, limit: usize) -> MagpieResult<Value> {
        let library_id = match library {
            Some(key) => Some(self.library_row(key)?.id),
            None => None,
        };
        let limit = if limit == 0 { 200 } else { limit };
        let rows = self.store.review_items(library_id, limit)?;
        let libs = self.store.libraries()?;
        let entries: Vec<Value> = rows
            .iter()
            .map(|row| {
                let library = libs
                    .iter()
                    .find(|l| l.id == row.library_id)
                    .map(|l| l.name.clone())
                    .unwrap_or_default();
                json!({
                    "key": row.key(),
                    "title": row.title,
                    "rel": row.rel,
                    "library": library,
                    "series": row.series_title,
                    "volume": row.volume,
                    "confidence": row.confidence,
                    "container": row.container,
                })
            })
            .collect();
        Ok(json!({
            "count": entries.len(),
            "total_in_queue": self.store.review_count().unwrap_or(0),
            "entries": entries,
            "note": "rename the file yourself, or add ComicInfo.xml; the next scan clears the entry",
        }))
    }

    // ------------------------------------------------------------ progress
    pub fn progress_get(&self, key: &str) -> MagpieResult<Option<ProgressView>> {
        Ok(self.progress_view(key))
    }

    fn progress_view(&self, key: &str) -> Option<ProgressView> {
        self.store.progress_get(key).ok().flatten().map(|p| {
            let percent = if p.total > 0 {
                (p.page as f32 / p.total as f32).clamp(0.0, 1.0) * 100.0
            } else {
                0.0
            };
            ProgressView {
                page: p.page,
                total: p.total,
                finished: p.finished,
                percent,
                updated_at: p.updated_at,
            }
        })
    }

    pub fn progress_set(
        &mut self,
        key: &str,
        page: usize,
        total: Option<usize>,
        finished: bool,
    ) -> MagpieResult<ProgressView> {
        let ik = ItemKey::parse(key)
            .ok_or_else(|| MagpieError::invalid(format!("not an item key: {key}")))?;
        let lib = self.library_row(&ik.library_id.to_string())?;
        let row = self
            .store
            .item_by_rel(lib.id, &ik.rel)?
            .ok_or_else(|| MagpieError::not_found(format!("no item at {key}")))?;
        let total = total
            .map(|t| i64::try_from(t).unwrap_or(i64::MAX))
            .or(row.page_count)
            .unwrap_or(0);
        let page = i64::try_from(page).unwrap_or(i64::MAX);
        self.store
            .progress_set(&row.key(), page, total, finished, Some(&row.title))?;
        self.progress_view(&row.key())
            .ok_or_else(|| MagpieError::invalid("progress could not be read back"))
    }

    pub fn set_page_offset(&mut self, key: &str, offset: i32) -> MagpieResult<Value> {
        let ik = ItemKey::parse(key)
            .ok_or_else(|| MagpieError::invalid(format!("not an item key: {key}")))?;
        let lib = self.library_row(&ik.library_id.to_string())?;
        let row = self
            .store
            .item_by_rel(lib.id, &ik.rel)?
            .ok_or_else(|| MagpieError::not_found(format!("no item at {key}")))?;
        let offset = offset.clamp(-500, 500);
        self.store.set_page_offset(row.id, offset)?;
        Ok(json!({ "key": row.key(), "page_offset": offset }))
    }

    pub fn set_item_rtl(&mut self, key: &str, rtl: Option<bool>) -> MagpieResult<Value> {
        let ik = ItemKey::parse(key)
            .ok_or_else(|| MagpieError::invalid(format!("not an item key: {key}")))?;
        let lib = self.library_row(&ik.library_id.to_string())?;
        let row = self
            .store
            .item_by_rel(lib.id, &ik.rel)?
            .ok_or_else(|| MagpieError::not_found(format!("no item at {key}")))?;
        self.store.set_rtl(row.id, rtl)?;
        Ok(json!({ "key": row.key(), "rtl": rtl }))
    }

    pub fn continue_reading(&self, limit: usize) -> MagpieResult<Value> {
        let rows = self
            .store
            .continue_reading(if limit == 0 { 12 } else { limit })?;
        let entries: Vec<Value> = rows
            .iter()
            .map(|p| {
                let title = p.title.clone().unwrap_or_default();
                json!({
                    "key": p.key,
                    "title": title,
                    "page": p.page,
                    "total": p.total,
                    "percent": if p.total > 0 { (p.page as f32 / p.total as f32 * 100.0).clamp(0.0, 100.0) } else { 0.0 },
                    "updated_at": p.updated_at,
                })
            })
            .collect();
        Ok(json!({ "count": entries.len(), "entries": entries }))
    }

    // --------------------------------------------------------------- stats
    pub fn stats(&self) -> MagpieResult<StatsView> {
        let raw = self.store.catalogue_stats()?;
        let libs = self.store.libraries()?;
        let mut drives: Vec<DriveView> = Vec::new();
        for lib in &libs {
            let volume = volume_of(&lib.root);
            let online = Path::new(&lib.root).is_dir();
            let idx = match drives.iter().position(|d| d.volume == volume) {
                Some(i) => i,
                None => {
                    drives.push(DriveView {
                        volume: volume.clone(),
                        online,
                        libraries: Vec::new(),
                        items: 0,
                        bytes: 0,
                    });
                    drives.len().saturating_sub(1)
                }
            };
            let per = raw.per_library.iter().find(|p| p.library_id == lib.id);
            if let Some(bucket) = drives.get_mut(idx) {
                bucket.online = bucket.online || online;
                bucket.libraries.push(lib.name.clone());
                if let Some(per) = per {
                    bucket.items += per.items;
                    bucket.bytes += per.bytes;
                }
            }
        }
        let last = self.store.last_scan(None)?;
        Ok(StatsView {
            libraries: raw.libraries,
            items: raw.items,
            missing: raw.missing,
            series: raw.series,
            dirs: raw.dirs,
            pages: raw.pages,
            bytes: raw.bytes,
            per_library: raw
                .per_library
                .iter()
                .map(|p| LibraryStatsView {
                    library_id: p.library_id,
                    name: p.name.clone(),
                    kind: MediaKind::parse(&p.kind).unwrap_or(MediaKind::Other),
                    mature: p.mature,
                    items: p.items,
                    missing: p.missing,
                    dirs: p.dirs,
                    pages: p.pages,
                    bytes: p.bytes,
                })
                .collect(),
            drives,
            last_scan_ms: last.as_ref().map(|s| s.ms),
            last_scan_at: last.and_then(|s| s.finished_at),
        })
    }

    // ---------------------------------------------------------- duplicates
    pub fn duplicates(
        &mut self,
        library: Option<&str>,
        limit: usize,
    ) -> MagpieResult<DuplicatesView> {
        let libs: Vec<LibraryRow> = match library {
            Some(key) => vec![self.library_row(key)?],
            None => self.store.libraries()?,
        };
        let limit = if limit == 0 { 200 } else { limit };
        let mut groups: Vec<DupGroupView> = Vec::new();
        let mut total = 0usize;
        let mut hashed = 0usize;
        for lib in &libs {
            let root = PathBuf::from(&lib.root);
            if !root.is_dir() {
                continue;
            }
            for (size, _count) in self.store.duplicate_sizes(Some(lib.id))? {
                let rows = self.store.items_by_size(size)?;
                let candidates: Vec<(String, u64)> = rows
                    .iter()
                    .filter(|r| r.library_id == lib.id)
                    .map(|r| (r.rel.clone(), u64::try_from(r.size).unwrap_or(0)))
                    .collect();
                if candidates.len() < 2 {
                    continue;
                }
                hashed += candidates.len();
                let found: Vec<DupGroup> = health::group_duplicates(&root, &candidates)?;
                for g in found {
                    total += 1;
                    if groups.len() < limit {
                        groups.push(DupGroupView {
                            size: i64::try_from(g.size).unwrap_or(i64::MAX),
                            hash: g.hash,
                            paths: g.paths,
                            libraries: vec![lib.name.clone()],
                        });
                    }
                }
            }
        }
        Ok(DuplicatesView {
            truncated: total > groups.len(),
            groups,
            groups_total: total,
            files_hashed: hashed,
        })
    }

    pub fn hash_item(&mut self, key: &str) -> MagpieResult<Value> {
        let ik = ItemKey::parse(key)
            .ok_or_else(|| MagpieError::invalid(format!("not an item key: {key}")))?;
        let lib = self.library_row(&ik.library_id.to_string())?;
        let row = self
            .store
            .item_by_rel(lib.id, &ik.rel)?
            .ok_or_else(|| MagpieError::not_found(format!("no item at {key}")))?;
        let path = library_path(&lib, &row.rel);
        let started = Instant::now();
        let (hash, size) = health::hash_file(&path)?;
        self.store.set_hash(row.id, &hash)?;
        Ok(json!({
            "key": row.key(),
            "hash": hash,
            "algorithm": "blake3",
            "bytes": size,
            "ms": started.elapsed().as_millis(),
        }))
    }

    // ----------------------------------------------------------- manifests
    pub fn manifest_write(&mut self, library: &str, out: Option<&str>) -> MagpieResult<Value> {
        let lib = self.library_row(library)?;
        let root = PathBuf::from(&lib.root);
        if !root.is_dir() {
            return Err(MagpieError::not_found(format!(
                "library root is not reachable: {}",
                lib.root
            )));
        }
        let target = match out {
            Some(p) if !p.trim().is_empty() => PathBuf::from(p),
            _ => health::manifest_path(&self.paths.manifests_dir(), &lib.name),
        };
        let started = Instant::now();
        let report = health::write_manifest(&root, &target)?;
        Ok(json!({
            "library": lib.name,
            "root": report.root,
            "manifest": report.manifest,
            "files": report.files,
            "bytes": report.bytes,
            "ms": started.elapsed().as_millis(),
        }))
    }

    pub fn manifest_verify(
        &mut self,
        library: &str,
        manifest: Option<&str>,
    ) -> MagpieResult<Value> {
        let lib = self.library_row(library)?;
        let root = PathBuf::from(&lib.root);
        let target = match manifest {
            Some(p) if !p.trim().is_empty() => PathBuf::from(p),
            _ => health::manifest_path(&self.paths.manifests_dir(), &lib.name),
        };
        let started = Instant::now();
        let report = health::verify_manifest(&root, &target)?;
        Ok(json!({
            "library": lib.name,
            "manifest": target.display().to_string(),
            "checked": report.checked,
            "ok": report.ok,
            "missing": report.missing,
            "changed": report.changed,
            "unknown": report.unknown,
            "bad_lines": report.bad_lines,
            "clean": report.clean(),
            "ms": started.elapsed().as_millis(),
        }))
    }

    // -------------------------------------------------------------- series
    pub fn series_list(&self, library: Option<&str>) -> MagpieResult<Vec<Value>> {
        let library_id = match library {
            Some(key) => Some(self.library_row(key)?.id),
            None => None,
        };
        let rows = self.store.series_list(library_id)?;
        Ok(rows
            .into_iter()
            .map(|s| {
                json!({
                    "id": s.id,
                    "library_id": s.library_id,
                    "rel": s.rel,
                    "title": s.title,
                    "rtl": s.rtl,
                })
            })
            .collect())
    }

    pub fn series_set_rtl(
        &mut self,
        library: &str,
        series: &str,
        rtl: Option<bool>,
    ) -> MagpieResult<Value> {
        let lib = self.library_row(library)?;
        let rows = self.store.series_list(Some(lib.id))?;
        let wanted = series.trim();
        let hit = rows
            .iter()
            .find(|s| s.rel == wanted || s.title.eq_ignore_ascii_case(wanted))
            .ok_or_else(|| MagpieError::not_found(format!("no series {series} in {}", lib.name)))?;
        self.store.series_set_rtl(hit.id, rtl)?;
        Ok(json!({ "series": hit.title, "rtl": rtl }))
    }

    // ------------------------------------------------------------- inspect
    pub fn inspect(&self, path: &str) -> MagpieResult<Value> {
        let p = PathBuf::from(path);
        if !p.is_file() {
            return Err(MagpieError::not_found(format!(
                "not a file: {}",
                p.display()
            )));
        }
        let info = magpiecraft_tools::ffprobe(&self.tools, &p)?;
        Ok(json!({
            "path": p.display().to_string(),
            "media": info,
            "tool": "ffprobe",
        }))
    }

    pub fn tools_report(&self) -> Value {
        self.tools.report()
    }

    pub fn source_registry(&self) -> Value {
        json!({
            "sources": magpiecraft_sources::SOURCES,
            "enabled_by_default": magpiecraft_sources::enabled_by_default()
                .iter()
                .map(|s| s.id)
                .collect::<Vec<_>>(),
            "note": "Remote sources report not_implemented until their transports land in Tier 4.",
        })
    }

    /// Everything `status` needs, in one call.
    pub fn status(&self) -> MagpieResult<Value> {
        let stats = self.stats()?;
        Ok(json!({
            "app": "magpiecraft",
            "version": crate::version(),
            "home": self.paths.home().display().to_string(),
            "database": self.paths.db_file().display().to_string(),
            "settings": self.settings,
            "libraries": stats.libraries,
            "items": stats.items,
            "missing": stats.missing,
            "pages": stats.pages,
            "bytes": stats.bytes,
            "mature_visible": self.settings.mature_visible,
            "last_scan_ms": stats.last_scan_ms,
            "tools": self.tools.report(),
        }))
    }
}

// ---------------------------------------------------------------- helpers

fn library_view(row: LibraryRow) -> Library {
    Library {
        key: ItemKey::root(row.id).as_string(),
        id: row.id,
        name: row.name,
        root: row.root,
        kind: MediaKind::parse(&row.kind).unwrap_or(MediaKind::Other),
        mature: row.mature,
        rtl_default: row.rtl_default,
        created_at: row.created_at,
    }
}

/// Effective reading direction: item override, then series override, then the
/// library setting, then the kind's default.
fn effective_rtl(lib: &LibraryRow, row: &ItemRow) -> bool {
    if let Some(rtl) = row.rtl {
        return rtl;
    }
    if let Some(rtl) = row.series_rtl {
        return rtl;
    }
    if let Some(rtl) = lib.rtl_default {
        return rtl;
    }
    MediaKind::parse(&lib.kind)
        .unwrap_or(MediaKind::Other)
        .default_rtl()
}

fn series_rel(parent: &str, series_title: &str) -> String {
    if parent.is_empty() {
        format!("~{series_title}")
    } else {
        parent.to_string()
    }
}

fn series_title_for(
    parent: &str,
    name: &str,
    comic: Option<&ComicInfo>,
    parsed: &magpiecraft_comics::ParsedName,
) -> String {
    let from_comic = comic
        .and_then(|c| c.series.clone())
        .filter(|s| !s.trim().is_empty());
    if let Some(s) = from_comic {
        return s;
    }
    if let Some(last) = parent.rsplit('/').next().filter(|s| !s.is_empty()) {
        return last.to_string();
    }
    if !parsed.series.trim().is_empty() {
        return parsed.series.clone();
    }
    display_title(name, ContainerKind::File)
}

fn build_file_item(
    lib: &LibraryRow,
    file: &magpiecraft_scan::FileInfo,
    container: ContainerKind,
    page_count: Option<i64>,
    comic: Option<&ComicInfo>,
) -> NewItem {
    let parsed = parse_volume_name(&file.name);
    let series_title = series_title_for(&file.parent, &file.name, comic, &parsed);
    // ComicInfo is an explicit statement by the file, so it counts as certain.
    let confidence = if comic.is_some() {
        1.0
    } else {
        f64::from(parsed.confidence.clamp(0.0, 1.0))
    };
    NewItem {
        library_id: lib.id,
        rel: file.rel.clone(),
        parent: file.parent.clone(),
        name: file.name.clone(),
        title: display_title(&file.name, container),
        container: container.as_str().to_string(),
        size: i64::try_from(file.size).unwrap_or(i64::MAX),
        mtime: file.mtime,
        page_count,
        series_rel: Some(series_rel(&file.parent, &series_title)),
        series_title: Some(series_title),
        volume: comic
            .and_then(|c| c.number.clone())
            .or_else(|| parsed.volume.clone()),
        rtl: comic.and_then(|c| c.rtl()),
        comic_info: comic.is_some(),
        confidence,
        needs_review: comic.is_none() && parsed.needs_review,
    }
}

/// Absolute path of a relative item path inside its library.
pub fn library_path(lib: &LibraryRow, rel: &str) -> PathBuf {
    let mut p = PathBuf::from(&lib.root);
    for part in rel.split('/').filter(|s| !s.is_empty()) {
        p.push(part);
    }
    p
}

fn rel_to_path(rel: &str) -> PathBuf {
    let mut p = PathBuf::new();
    for part in rel.split('/').filter(|s| !s.is_empty()) {
        p.push(part);
    }
    p
}

/// Volume prefix for the drive view: `L:` on Windows, `/mnt/media` style roots
/// elsewhere. Falls back to the whole root when there is no obvious prefix.
pub fn volume_of(root: &str) -> String {
    let path = Path::new(root);
    let mut comps = path.components();
    match comps.next() {
        Some(std::path::Component::Prefix(prefix)) => {
            prefix.as_os_str().to_string_lossy().to_string()
        }
        Some(std::path::Component::RootDir) => {
            let mut out = String::from("/");
            if let Some(std::path::Component::Normal(n)) = comps.next() {
                out.push_str(&n.to_string_lossy());
            }
            out
        }
        Some(std::path::Component::Normal(n)) => n.to_string_lossy().to_string(),
        _ => root.to_string(),
    }
}
