//! MagpieCraft SQL store.
//!
//! One SQLite file holds the catalogue: libraries, directories, items, series,
//! reading progress and the FTS5 index. Every write goes through a transaction.
//! Paths are stored relative to their library root so a library survives a drive
//! letter change or a moved folder (the root is the only absolute path stored).

mod error;
mod schema;

pub use error::{StoreError, StoreResult};
pub use schema::SCHEMA_VERSION;

use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

/// One configured library root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryRow {
    pub id: i64,
    pub name: String,
    pub root: String,
    pub kind: String,
    pub mature: bool,
    pub rtl_default: Option<bool>,
    pub created_at: String,
}

/// A directory inside a library. `volume` marks a folder that is itself readable
/// as a volume (a numbered page folder), so it is shown as an item, not a folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirRow {
    pub library_id: i64,
    pub rel: String,
    pub parent: String,
    pub name: String,
    pub volume: bool,
}

/// One readable item (archive, PDF or page folder).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemRow {
    pub id: i64,
    pub library_id: i64,
    pub rel: String,
    pub parent: String,
    pub name: String,
    pub title: String,
    pub container: String,
    pub size: i64,
    pub mtime: i64,
    pub page_count: Option<i64>,
    pub series_id: Option<i64>,
    pub series_title: Option<String>,
    /// Series-level reading direction override, if the user set one.
    pub series_rtl: Option<bool>,
    pub volume: Option<String>,
    pub rtl: Option<bool>,
    /// Scan page minus printed page, remembered per item.
    pub page_offset: i32,
    /// How confident the filename parse was, 0.0 to 1.0.
    pub confidence: f64,
    /// True when the parse was below the review threshold.
    pub needs_review: bool,
    /// What this item is: video, audio, ebook, comics, manga, magazines, photo, other.
    pub media_kind: String,
    pub comic_info: bool,
    pub hash: Option<String>,
    pub missing: bool,
    pub updated_at: String,
}

impl ItemRow {
    /// Stable item key: `lib:<id>|<relative path>`. Opaque to callers.
    pub fn key(&self) -> String {
        format!("lib:{}|{}", self.library_id, self.rel)
    }
}

/// What the scanner knows about an item before it is written.
#[derive(Debug, Clone)]
pub struct NewItem {
    pub library_id: i64,
    pub rel: String,
    pub parent: String,
    pub name: String,
    pub title: String,
    pub container: String,
    pub size: i64,
    pub mtime: i64,
    pub page_count: Option<i64>,
    pub series_rel: Option<String>,
    pub series_title: Option<String>,
    pub volume: Option<String>,
    pub rtl: Option<bool>,
    pub comic_info: bool,
    /// Filename parse confidence, 0.0 to 1.0.
    pub confidence: f64,
    /// Whether the item belongs in the review queue.
    pub needs_review: bool,
    /// What this item is, from its own format or its library's kind.
    pub media_kind: String,
}

/// Result of an upsert: whether the row was created or actually changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Upsert {
    Added,
    Updated,
    Unchanged,
}

/// Fields read back before an upsert decides whether anything changed:
/// id, size, mtime, container, was_missing, page_count, confidence.
type ExistingItem = (i64, i64, i64, String, bool, Option<i64>, f64);

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanStats {
    pub library_id: i64,
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub missing: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressRow {
    pub key: String,
    pub page: i64,
    pub total: i64,
    /// Playback position in seconds, for films and tracks.
    pub position_secs: Option<i64>,
    /// Duration in seconds, when known.
    pub duration_secs: Option<i64>,
    pub finished: bool,
    pub title: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeriesRow {
    pub id: i64,
    pub library_id: i64,
    pub rel: String,
    pub title: String,
    pub rtl: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibraryStats {
    pub library_id: i64,
    pub name: String,
    pub kind: String,
    pub mature: bool,
    pub items: usize,
    pub missing: usize,
    pub dirs: usize,
    pub pages: i64,
    pub bytes: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CatalogueStats {
    pub libraries: usize,
    pub items: usize,
    pub missing: usize,
    pub series: usize,
    pub dirs: usize,
    pub pages: i64,
    pub bytes: i64,
    pub per_library: Vec<LibraryStats>,
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// The catalogue database.
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open (or create) the database at `path`, running migrations.
    pub fn open(path: &Path) -> StoreResult<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(StoreError::io)?;
        }
        let conn = Connection::open(path).map_err(StoreError::sqlite)?;
        Self::prepare(conn)
    }

    /// In-memory database for tests and `--smoke`.
    pub fn open_in_memory() -> StoreResult<Self> {
        let conn = Connection::open_in_memory().map_err(StoreError::sqlite)?;
        Self::prepare(conn)
    }

    fn prepare(conn: Connection) -> StoreResult<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(StoreError::sqlite)?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(StoreError::sqlite)?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(StoreError::sqlite)?;
        conn.execute_batch(schema::SCHEMA_SQL)
            .map_err(StoreError::sqlite)?;
        let store = Store { conn };
        // Migrations are additive and versioned: a database written by an older
        // build gains the new columns, a fresh one is built then migrated.
        let version = store
            .meta("schema_version")?
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        if version < 2 {
            store
                .conn
                .execute_batch(schema::MIGRATE_1_TO_2)
                .map_err(StoreError::sqlite)?;
        }
        if version < 3 {
            store
                .conn
                .execute_batch(schema::MIGRATE_2_TO_3)
                .map_err(StoreError::sqlite)?;
        }
        if version < 4 {
            store
                .conn
                .execute_batch(schema::MIGRATE_3_TO_4)
                .map_err(StoreError::sqlite)?;
        }
        if version < 5 {
            store
                .conn
                .execute_batch(schema::MIGRATE_4_TO_5)
                .map_err(StoreError::sqlite)?;
        }
        if version < 6 {
            store
                .conn
                .execute_batch(schema::MIGRATE_5_TO_6)
                .map_err(StoreError::sqlite)?;
        }
        store.set_meta("schema_version", SCHEMA_VERSION)?;
        Ok(store)
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    fn tx(&mut self) -> StoreResult<Transaction<'_>> {
        self.conn.transaction().map_err(StoreError::sqlite)
    }

    // ---------------------------------------------------------------- meta
    pub fn set_meta(&self, key: &str, value: &str) -> StoreResult<()> {
        self.conn
            .execute(
                "INSERT INTO meta(key, value) VALUES(?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map_err(StoreError::sqlite)?;
        Ok(())
    }

    pub fn meta(&self, key: &str) -> StoreResult<Option<String>> {
        self.conn
            .query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| {
                r.get(0)
            })
            .optional()
            .map_err(StoreError::sqlite)
    }

    // ----------------------------------------------------------- libraries
    pub fn add_library(
        &mut self,
        name: &str,
        root: &str,
        kind: &str,
        mature: bool,
        rtl_default: Option<bool>,
    ) -> StoreResult<i64> {
        self.conn
            .execute(
                "INSERT INTO libraries(name, root, kind, mature, rtl_default, created_at)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                params![name, root, kind, i32::from(mature), rtl_default, now()],
            )
            .map_err(StoreError::sqlite)?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn libraries(&self) -> StoreResult<Vec<LibraryRow>> {
        let mut st = self
            .conn
            .prepare(
                "SELECT id, name, root, kind, mature, rtl_default, created_at
                 FROM libraries ORDER BY name COLLATE NOCASE",
            )
            .map_err(StoreError::sqlite)?;
        let rows = st
            .query_map([], |r| {
                Ok(LibraryRow {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    root: r.get(2)?,
                    kind: r.get(3)?,
                    mature: r.get::<_, i32>(4)? != 0,
                    rtl_default: r.get(5)?,
                    created_at: r.get(6)?,
                })
            })
            .map_err(StoreError::sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::sqlite)?;
        Ok(rows)
    }

    pub fn library(&self, id: i64) -> StoreResult<Option<LibraryRow>> {
        let mut st = self
            .conn
            .prepare(
                "SELECT id, name, root, kind, mature, rtl_default, created_at
                 FROM libraries WHERE id = ?1",
            )
            .map_err(StoreError::sqlite)?;
        st.query_row(params![id], |r| {
            Ok(LibraryRow {
                id: r.get(0)?,
                name: r.get(1)?,
                root: r.get(2)?,
                kind: r.get(3)?,
                mature: r.get::<_, i32>(4)? != 0,
                rtl_default: r.get(5)?,
                created_at: r.get(6)?,
            })
        })
        .optional()
        .map_err(StoreError::sqlite)
    }

    pub fn library_by_name(&self, name: &str) -> StoreResult<Option<LibraryRow>> {
        let id: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM libraries WHERE name = ?1 COLLATE NOCASE",
                params![name],
                |r| r.get(0),
            )
            .optional()
            .map_err(StoreError::sqlite)?;
        match id {
            Some(id) => self.library(id),
            None => Ok(None),
        }
    }

    pub fn remove_library(&mut self, id: i64) -> StoreResult<()> {
        let tx = self.tx()?;
        tx.execute(
            "DELETE FROM items_fts WHERE rowid IN (SELECT id FROM items WHERE library_id = ?1)",
            params![id],
        )
        .map_err(StoreError::sqlite)?;
        for table in ["items", "dirs", "series", "libraries"] {
            let sql = format!(
                "DELETE FROM {table} WHERE {} = ?1",
                if table == "libraries" {
                    "id"
                } else {
                    "library_id"
                }
            );
            tx.execute(&sql, params![id]).map_err(StoreError::sqlite)?;
        }
        tx.commit().map_err(StoreError::sqlite)?;
        Ok(())
    }

    // --------------------------------------------------------------- dirs
    pub fn upsert_dir(
        &mut self,
        library_id: i64,
        rel: &str,
        parent: &str,
        name: &str,
        volume: bool,
    ) -> StoreResult<()> {
        self.conn
            .execute(
                "INSERT INTO dirs(library_id, rel, parent, name, volume) VALUES(?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(library_id, rel) DO UPDATE SET parent = excluded.parent,
                     name = excluded.name, volume = excluded.volume",
                params![library_id, rel, parent, name, i32::from(volume)],
            )
            .map_err(StoreError::sqlite)?;
        Ok(())
    }

    pub fn clear_dirs(&mut self, library_id: i64) -> StoreResult<()> {
        self.conn
            .execute(
                "DELETE FROM dirs WHERE library_id = ?1",
                params![library_id],
            )
            .map_err(StoreError::sqlite)?;
        Ok(())
    }

    pub fn dirs_under(&self, library_id: i64, parent: &str) -> StoreResult<Vec<DirRow>> {
        let mut st = self
            .conn
            .prepare(
                "SELECT library_id, rel, parent, name, volume FROM dirs
                 WHERE library_id = ?1 AND parent = ?2 AND volume = 0 AND rel <> ''",
            )
            .map_err(StoreError::sqlite)?;
        let rows = st
            .query_map(params![library_id, parent], |r| {
                Ok(DirRow {
                    library_id: r.get(0)?,
                    rel: r.get(1)?,
                    parent: r.get(2)?,
                    name: r.get(3)?,
                    volume: r.get::<_, i32>(4)? != 0,
                })
            })
            .map_err(StoreError::sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::sqlite)?;
        Ok(rows)
    }

    pub fn dir_count(&self, library_id: Option<i64>) -> StoreResult<usize> {
        let n: i64 = match library_id {
            Some(id) => self.conn.query_row(
                "SELECT COUNT(*) FROM dirs WHERE library_id = ?1",
                params![id],
                |r| r.get(0),
            ),
            None => self
                .conn
                .query_row("SELECT COUNT(*) FROM dirs", [], |r| r.get(0)),
        }
        .map_err(StoreError::sqlite)?;
        Ok(usize::try_from(n).unwrap_or(usize::MAX))
    }

    // -------------------------------------------------------------- items
    fn item_select() -> &'static str {
        "SELECT i.id, i.library_id, i.rel, i.parent, i.name, i.title, i.container, i.size,
                i.mtime, i.page_count, i.series_id, s.title, i.volume, i.rtl, i.comic_info,
                i.hash, i.missing, i.updated_at, s.rtl, i.page_offset,
                i.confidence, i.needs_review, i.media_kind
         FROM items i LEFT JOIN series s ON s.id = i.series_id"
    }

    fn map_item(r: &rusqlite::Row<'_>) -> rusqlite::Result<ItemRow> {
        Ok(ItemRow {
            id: r.get(0)?,
            library_id: r.get(1)?,
            rel: r.get(2)?,
            parent: r.get(3)?,
            name: r.get(4)?,
            title: r.get(5)?,
            container: r.get(6)?,
            size: r.get(7)?,
            mtime: r.get(8)?,
            page_count: r.get(9)?,
            series_id: r.get(10)?,
            series_title: r.get(11)?,
            volume: r.get(12)?,
            rtl: r.get(13)?,
            comic_info: r.get::<_, i32>(14)? != 0,
            hash: r.get(15)?,
            missing: r.get::<_, i32>(16)? != 0,
            updated_at: r.get(17)?,
            series_rtl: r.get(18)?,
            page_offset: r.get(19)?,
            confidence: r.get(20)?,
            needs_review: r.get::<_, i32>(21)? != 0,
            media_kind: r.get(22)?,
        })
    }

    /// Insert or update one item, keeping the FTS index in step.
    pub fn upsert_item(&mut self, item: &NewItem) -> StoreResult<(i64, Upsert)> {
        let existing: Option<ExistingItem> = self
            .conn
            .query_row(
                "SELECT id, size, mtime, container, missing, page_count, confidence FROM items
                 WHERE library_id = ?1 AND rel = ?2",
                params![item.library_id, item.rel],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get::<_, i32>(4)? != 0,
                        r.get(5)?,
                        r.get(6)?,
                    ))
                },
            )
            .optional()
            .map_err(StoreError::sqlite)?;

        let series_id = match (&item.series_rel, &item.series_title) {
            (Some(rel), Some(title)) => Some(self.upsert_series(item.library_id, rel, title)?),
            _ => None,
        };

        match existing {
            Some((id, size, mtime, container, was_missing, page_count, confidence)) => {
                let same = size == item.size
                    && mtime == item.mtime
                    && container == item.container
                    && !was_missing
                    && (confidence - item.confidence).abs() < 0.001
                    && match item.page_count {
                        Some(pages) => page_count == Some(pages),
                        None => true,
                    };
                if same {
                    return Ok((id, Upsert::Unchanged));
                }
                self.conn
                    .execute(
                        "UPDATE items SET parent = ?2, name = ?3, title = ?4, container = ?5, size = ?6,
                             mtime = ?7, page_count = COALESCE(?8, page_count), series_id = ?9,
                             volume = COALESCE(?10, volume), rtl = COALESCE(?11, rtl),
                             comic_info = MAX(comic_info, ?12), confidence = ?14,
                             needs_review = ?15, media_kind = ?16, missing = 0, updated_at = ?13
                         WHERE id = ?1",
                        params![
                            id,
                            item.parent,
                            item.name,
                            item.title,
                            item.container,
                            item.size,
                            item.mtime,
                            item.page_count,
                            series_id,
                            item.volume,
                            item.rtl,
                            i32::from(item.comic_info),
                            now(),
                            item.confidence,
                            i32::from(item.needs_review),
                            item.media_kind
                        ],
                    )
                    .map_err(StoreError::sqlite)?;
                self.index_item(id, item)?;
                Ok((id, Upsert::Updated))
            }
            None => {
                self.conn
                    .execute(
                        "INSERT INTO items(library_id, rel, parent, name, title, container, size, mtime,
                             page_count, series_id, volume, rtl, comic_info, missing, updated_at,
                             confidence, needs_review, media_kind)
                         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 0, ?14, ?15, ?16, ?17)",
                        params![
                            item.library_id,
                            item.rel,
                            item.parent,
                            item.name,
                            item.title,
                            item.container,
                            item.size,
                            item.mtime,
                            item.page_count,
                            series_id,
                            item.volume,
                            item.rtl,
                            i32::from(item.comic_info),
                            now(),
                            item.confidence,
                            i32::from(item.needs_review),
                            item.media_kind
                        ],
                    )
                    .map_err(StoreError::sqlite)?;
                let id = self.conn.last_insert_rowid();
                self.index_item(id, item)?;
                Ok((id, Upsert::Added))
            }
        }
    }

    /// Refresh the FTS row for one item. Series title is included so a search for a
    /// series name finds every volume in it.
    pub fn index_item(&mut self, id: i64, item: &NewItem) -> StoreResult<()> {
        self.conn
            .execute("DELETE FROM items_fts WHERE rowid = ?1", params![id])
            .map_err(StoreError::sqlite)?;
        let series = item.series_title.clone().unwrap_or_default();
        self.conn
            .execute(
                "INSERT INTO items_fts(rowid, title, series, path, library) VALUES(?1, ?2, ?3, ?4, ?5)",
                params![id, item.title, series, item.rel, item.library_id.to_string()],
            )
            .map_err(StoreError::sqlite)?;
        Ok(())
    }

    pub fn item_by_rel(&self, library_id: i64, rel: &str) -> StoreResult<Option<ItemRow>> {
        let sql = format!(
            "{} WHERE i.library_id = ?1 AND i.rel = ?2",
            Self::item_select()
        );
        let mut st = self.conn.prepare(&sql).map_err(StoreError::sqlite)?;
        st.query_row(params![library_id, rel], Self::map_item)
            .optional()
            .map_err(StoreError::sqlite)
    }

    pub fn item_by_id(&self, id: i64) -> StoreResult<Option<ItemRow>> {
        let sql = format!("{} WHERE i.id = ?1", Self::item_select());
        let mut st = self.conn.prepare(&sql).map_err(StoreError::sqlite)?;
        st.query_row(params![id], Self::map_item)
            .optional()
            .map_err(StoreError::sqlite)
    }

    /// Items directly inside one folder, missing rows excluded.
    pub fn items_under(&self, library_id: i64, parent: &str) -> StoreResult<Vec<ItemRow>> {
        let sql = format!(
            "{} WHERE i.library_id = ?1 AND i.parent = ?2 AND i.missing = 0",
            Self::item_select()
        );
        let mut st = self.conn.prepare(&sql).map_err(StoreError::sqlite)?;
        let rows = st
            .query_map(params![library_id, parent], Self::map_item)
            .map_err(StoreError::sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::sqlite)?;
        Ok(rows)
    }

    /// Items the filename parser was unsure about, worst first. This is the review
    /// queue: nothing here was guessed silently, it was flagged.
    pub fn review_items(&self, library_id: Option<i64>, limit: usize) -> StoreResult<Vec<ItemRow>> {
        let limit = i64::try_from(limit.max(1)).unwrap_or(i64::MAX);
        let map = Self::map_item;
        match library_id {
            Some(id) => {
                let sql = format!(
                    "{} WHERE i.library_id = ?1 AND i.missing = 0 AND i.needs_review = 1
                     ORDER BY i.confidence, i.rel LIMIT ?2",
                    Self::item_select()
                );
                let mut st = self.conn.prepare(&sql).map_err(StoreError::sqlite)?;
                let rows = st
                    .query_map(params![id, limit], map)
                    .map_err(StoreError::sqlite)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(StoreError::sqlite)?;
                Ok(rows)
            }
            None => {
                let sql = format!(
                    "{} WHERE i.missing = 0 AND i.needs_review = 1
                     ORDER BY i.confidence, i.rel LIMIT ?1",
                    Self::item_select()
                );
                let mut st = self.conn.prepare(&sql).map_err(StoreError::sqlite)?;
                let rows = st
                    .query_map(params![limit], map)
                    .map_err(StoreError::sqlite)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(StoreError::sqlite)?;
                Ok(rows)
            }
        }
    }

    pub fn review_count(&self) -> StoreResult<usize> {
        let n: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM items WHERE missing = 0 AND needs_review = 1",
                [],
                |r| r.get(0),
            )
            .map_err(StoreError::sqlite)?;
        Ok(usize::try_from(n).unwrap_or(usize::MAX))
    }

    /// Totals per media kind: items, bytes and pages. This is what makes the
    /// catalogue more than a comics index.
    pub fn kind_totals(&self) -> StoreResult<Vec<(String, i64, i64, i64)>> {
        let mut st = self
            .conn
            .prepare(
                "SELECT media_kind, COUNT(*), COALESCE(SUM(size), 0),
                        COALESCE(SUM(COALESCE(page_count, 0)), 0)
                 FROM items WHERE missing = 0 GROUP BY media_kind ORDER BY COUNT(*) DESC",
            )
            .map_err(StoreError::sqlite)?;
        let rows = st
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })
            .map_err(StoreError::sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::sqlite)?;
        Ok(rows)
    }

    /// Items of one kind, in path order, capped. Used by kind-aware tooling.
    pub fn items_of_kind(&self, kind: &str, limit: usize) -> StoreResult<Vec<ItemRow>> {
        let sql = format!(
            "{} WHERE i.missing = 0 AND i.media_kind = ?1 ORDER BY i.rel LIMIT ?2",
            Self::item_select()
        );
        let mut st = self.conn.prepare(&sql).map_err(StoreError::sqlite)?;
        let rows = st
            .query_map(
                params![kind, i64::try_from(limit.max(1)).unwrap_or(i64::MAX)],
                Self::map_item,
            )
            .map_err(StoreError::sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::sqlite)?;
        Ok(rows)
    }

    /// Items of one library in path order, capped. Used by the rename planner.
    pub fn items_in_library(&self, library_id: i64, limit: usize) -> StoreResult<Vec<ItemRow>> {
        let sql = format!(
            "{} WHERE i.library_id = ?1 AND i.missing = 0 ORDER BY i.rel LIMIT ?2",
            Self::item_select()
        );
        let mut st = self.conn.prepare(&sql).map_err(StoreError::sqlite)?;
        let rows = st
            .query_map(
                params![library_id, i64::try_from(limit.max(1)).unwrap_or(i64::MAX)],
                Self::map_item,
            )
            .map_err(StoreError::sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::sqlite)?;
        Ok(rows)
    }

    /// First readable item anywhere under a folder, used for folder covers.
    pub fn first_item_under(&self, library_id: i64, rel: &str) -> StoreResult<Option<ItemRow>> {
        let pattern = if rel.is_empty() {
            "%".to_string()
        } else {
            format!("{}/%", rel.replace('%', "\\%"))
        };
        let sql = format!(
            "{} WHERE i.library_id = ?1 AND i.rel LIKE ?2 ESCAPE '\\' AND i.missing = 0
             ORDER BY i.rel LIMIT 1",
            Self::item_select()
        );
        let mut st = self.conn.prepare(&sql).map_err(StoreError::sqlite)?;
        st.query_row(params![library_id, pattern], Self::map_item)
            .optional()
            .map_err(StoreError::sqlite)
    }

    /// Mark rows whose file was not seen in this scan as missing. Rows are kept
    /// (never deleted) so reading progress and overrides survive an unplugged drive.
    pub fn mark_missing_except(
        &mut self,
        library_id: i64,
        seen: &HashSet<String>,
    ) -> StoreResult<usize> {
        let tx = self.tx()?;
        let mut st = tx
            .prepare("SELECT id, rel FROM items WHERE library_id = ?1 AND missing = 0")
            .map_err(StoreError::sqlite)?;
        let rows = st
            .query_map(params![library_id], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(StoreError::sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::sqlite)?;
        drop(st);
        let mut marked = 0usize;
        for (id, rel) in rows {
            if !seen.contains(&rel) {
                tx.execute(
                    "UPDATE items SET missing = 1, updated_at = ?2 WHERE id = ?1",
                    params![id, now()],
                )
                .map_err(StoreError::sqlite)?;
                marked += 1;
            }
        }
        tx.commit().map_err(StoreError::sqlite)?;
        Ok(marked)
    }

    pub fn set_page_count(&mut self, id: i64, pages: i64) -> StoreResult<()> {
        self.conn
            .execute(
                "UPDATE items SET page_count = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, pages, now()],
            )
            .map_err(StoreError::sqlite)?;
        Ok(())
    }

    pub fn set_hash(&mut self, id: i64, hash: &str) -> StoreResult<()> {
        self.conn
            .execute(
                "UPDATE items SET hash = ?2 WHERE id = ?1",
                params![id, hash],
            )
            .map_err(StoreError::sqlite)?;
        Ok(())
    }

    pub fn set_rtl(&mut self, id: i64, rtl: Option<bool>) -> StoreResult<()> {
        self.conn
            .execute("UPDATE items SET rtl = ?2 WHERE id = ?1", params![id, rtl])
            .map_err(StoreError::sqlite)?;
        Ok(())
    }

    pub fn set_page_offset(&mut self, id: i64, offset: i32) -> StoreResult<()> {
        self.conn
            .execute(
                "UPDATE items SET page_offset = ?2 WHERE id = ?1",
                params![id, offset],
            )
            .map_err(StoreError::sqlite)?;
        Ok(())
    }

    // ------------------------------------------------- reorganise support
    /// Every item whose relative path equals `rel` or sits below it.
    pub fn items_under_path(&self, library_id: i64, rel: &str) -> StoreResult<Vec<(i64, String)>> {
        let pattern = format!("{}/%", rel.replace('%', "\\%"));
        let mut st = self
            .conn
            .prepare(
                "SELECT id, rel FROM items
                 WHERE library_id = ?1 AND (rel = ?2 OR rel LIKE ?3 ESCAPE '\\')
                 ORDER BY length(rel)",
            )
            .map_err(StoreError::sqlite)?;
        let rows = st
            .query_map(params![library_id, rel, pattern], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(StoreError::sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::sqlite)?;
        Ok(rows)
    }

    /// Rewrite one item's path after a move, and carry its reading progress to the
    /// new key. Progress is keyed by the item key, so a rename that does not do
    /// this would silently lose the reader's place.
    pub fn move_item_path(&mut self, id: i64, old_rel: &str, new_rel: &str) -> StoreResult<()> {
        let library_id: i64 = self
            .conn
            .query_row(
                "SELECT library_id FROM items WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .map_err(StoreError::sqlite)?;
        let parent = match new_rel.rsplit_once('/') {
            Some((p, _)) => p.to_string(),
            None => String::new(),
        };
        self.conn
            .execute(
                "UPDATE items SET rel = ?2, parent = ?3, missing = 0, updated_at = ?4 WHERE id = ?1",
                params![id, new_rel, parent, now()],
            )
            .map_err(StoreError::sqlite)?;
        let old_key = format!("lib:{library_id}|{old_rel}");
        let new_key = format!("lib:{library_id}|{new_rel}");
        self.conn
            .execute(
                "UPDATE progress SET key = ?2 WHERE key = ?1",
                params![old_key, new_key],
            )
            .map_err(StoreError::sqlite)?;
        // Keep the FTS path column in step with the file system.
        self.conn
            .execute(
                "UPDATE items_fts SET path = ?2 WHERE rowid = ?1",
                params![id, new_rel],
            )
            .map_err(StoreError::sqlite)?;
        Ok(())
    }

    /// Move a directory row and every directory below it.
    pub fn move_dir_path(&mut self, library_id: i64, from: &str, to: &str) -> StoreResult<usize> {
        let pattern = format!("{}/%", from.replace('%', "\\%"));
        let mut st = self
            .conn
            .prepare(
                "SELECT rel FROM dirs WHERE library_id = ?1 AND (rel = ?2 OR rel LIKE ?3 ESCAPE '\\')",
            )
            .map_err(StoreError::sqlite)?;
        let rels = st
            .query_map(params![library_id, from, pattern], |r| {
                r.get::<_, String>(0)
            })
            .map_err(StoreError::sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::sqlite)?;
        drop(st);
        let mut moved = 0usize;
        for rel in rels {
            let new_rel = if rel == from {
                to.to_string()
            } else {
                format!("{to}{}", rel.get(from.len()..).unwrap_or(""))
            };
            let new_parent = match new_rel.rsplit_once('/') {
                Some((p, _)) => p.to_string(),
                None => String::new(),
            };
            let name = new_rel.rsplit('/').next().unwrap_or("").to_string();
            self.conn
                .execute(
                    "UPDATE dirs SET rel = ?3, parent = ?4, name = ?5
                     WHERE library_id = ?1 AND rel = ?2",
                    params![library_id, rel, new_rel, new_parent, name],
                )
                .map_err(StoreError::sqlite)?;
            moved += 1;
        }
        Ok(moved)
    }

    /// Move a series row (and its items' series id stays valid because only the
    /// path label changes).
    pub fn move_series_path(&mut self, library_id: i64, from: &str, to: &str) -> StoreResult<()> {
        self.conn
            .execute(
                "UPDATE series SET rel = ?3 WHERE library_id = ?1 AND rel = ?2",
                params![library_id, from, to],
            )
            .map_err(StoreError::sqlite)?;
        Ok(())
    }

    // -------------------------------------------------------------- series
    pub fn upsert_series(&mut self, library_id: i64, rel: &str, title: &str) -> StoreResult<i64> {
        self.conn
            .execute(
                "INSERT INTO series(library_id, rel, title) VALUES(?1, ?2, ?3)
                 ON CONFLICT(library_id, rel) DO UPDATE SET title = excluded.title",
                params![library_id, rel, title],
            )
            .map_err(StoreError::sqlite)?;
        let id: i64 = self
            .conn
            .query_row(
                "SELECT id FROM series WHERE library_id = ?1 AND rel = ?2",
                params![library_id, rel],
                |r| r.get(0),
            )
            .map_err(StoreError::sqlite)?;
        Ok(id)
    }

    pub fn series_list(&self, library_id: Option<i64>) -> StoreResult<Vec<SeriesRow>> {
        let sql = "SELECT id, library_id, rel, title, rtl FROM series";
        let mut out = Vec::new();
        match library_id {
            Some(id) => {
                let mut st = self
                    .conn
                    .prepare(&format!(
                        "{sql} WHERE library_id = ?1 ORDER BY title COLLATE NOCASE"
                    ))
                    .map_err(StoreError::sqlite)?;
                let rows = st
                    .query_map(params![id], |r| {
                        Ok(SeriesRow {
                            id: r.get(0)?,
                            library_id: r.get(1)?,
                            rel: r.get(2)?,
                            title: r.get(3)?,
                            rtl: r.get(4)?,
                        })
                    })
                    .map_err(StoreError::sqlite)?;
                for row in rows {
                    out.push(row.map_err(StoreError::sqlite)?);
                }
            }
            None => {
                let mut st = self
                    .conn
                    .prepare(&format!("{sql} ORDER BY title COLLATE NOCASE"))
                    .map_err(StoreError::sqlite)?;
                let rows = st
                    .query_map([], |r| {
                        Ok(SeriesRow {
                            id: r.get(0)?,
                            library_id: r.get(1)?,
                            rel: r.get(2)?,
                            title: r.get(3)?,
                            rtl: r.get(4)?,
                        })
                    })
                    .map_err(StoreError::sqlite)?;
                for row in rows {
                    out.push(row.map_err(StoreError::sqlite)?);
                }
            }
        }
        Ok(out)
    }

    pub fn series_set_rtl(&mut self, series_id: i64, rtl: Option<bool>) -> StoreResult<()> {
        self.conn
            .execute(
                "UPDATE series SET rtl = ?2 WHERE id = ?1",
                params![series_id, rtl],
            )
            .map_err(StoreError::sqlite)?;
        Ok(())
    }

    // ------------------------------------------------------------ progress
    pub fn progress_set(
        &mut self,
        key: &str,
        page: i64,
        total: i64,
        finished: bool,
        title: Option<&str>,
    ) -> StoreResult<()> {
        self.conn
            .execute(
                "INSERT INTO progress(key, page, total, finished, title, updated_at)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(key) DO UPDATE SET page = excluded.page, total = excluded.total,
                     finished = excluded.finished, title = COALESCE(excluded.title, progress.title),
                     updated_at = excluded.updated_at",
                params![key, page, total, i32::from(finished), title, now()],
            )
            .map_err(StoreError::sqlite)?;
        Ok(())
    }

    pub fn progress_get(&self, key: &str) -> StoreResult<Option<ProgressRow>> {
        self.conn
            .query_row(
                "SELECT key, page, total, finished, title, updated_at, position_secs, duration_secs
                 FROM progress WHERE key = ?1",
                params![key],
                |r| {
                    Ok(ProgressRow {
                        key: r.get(0)?,
                        page: r.get(1)?,
                        total: r.get(2)?,
                        finished: r.get::<_, i32>(3)? != 0,
                        title: r.get(4)?,
                        updated_at: r.get(5)?,
                        position_secs: r.get(6)?,
                        duration_secs: r.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::sqlite)
    }

    /// Remember where a film or a track was left, in seconds. Page fields stay zero
    /// so a comic's position is never confused with a film's.
    pub fn progress_set_position(
        &mut self,
        key: &str,
        position_secs: i64,
        duration_secs: Option<i64>,
        title: Option<&str>,
    ) -> StoreResult<()> {
        let position = position_secs.max(0);
        let finished = duration_secs
            .map(|d| d > 0 && position * 100 / d >= 95)
            .unwrap_or(false);
        self.conn
            .execute(
                "INSERT INTO progress(key, page, total, finished, title, updated_at,
                     position_secs, duration_secs)
                 VALUES(?1, 0, 0, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(key) DO UPDATE SET
                     finished = excluded.finished,
                     title = COALESCE(excluded.title, progress.title),
                     updated_at = excluded.updated_at,
                     position_secs = excluded.position_secs,
                     duration_secs = COALESCE(excluded.duration_secs, progress.duration_secs)",
                params![
                    key,
                    i32::from(finished),
                    title,
                    now(),
                    position,
                    duration_secs
                ],
            )
            .map_err(StoreError::sqlite)?;
        Ok(())
    }

    /// Cached duration of an item, in seconds, when one has been probed.
    pub fn item_duration(&self, id: i64) -> StoreResult<Option<i64>> {
        self.conn
            .query_row(
                "SELECT duration_secs FROM items WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()
            .map(|v| v.flatten())
            .map_err(StoreError::sqlite)
    }

    pub fn set_item_duration(&mut self, id: i64, secs: i64) -> StoreResult<()> {
        self.conn
            .execute(
                "UPDATE items SET duration_secs = ?2 WHERE id = ?1",
                params![id, secs],
            )
            .map_err(StoreError::sqlite)?;
        Ok(())
    }

    pub fn continue_reading(&self, limit: usize) -> StoreResult<Vec<ProgressRow>> {
        let mut st = self
            .conn
            .prepare(
                "SELECT key, page, total, finished, title, updated_at, position_secs, duration_secs
                 FROM progress
                 WHERE finished = 0 ORDER BY updated_at DESC LIMIT ?1",
            )
            .map_err(StoreError::sqlite)?;
        let rows = st
            .query_map(params![i64::try_from(limit).unwrap_or(i64::MAX)], |r| {
                Ok(ProgressRow {
                    key: r.get(0)?,
                    page: r.get(1)?,
                    total: r.get(2)?,
                    finished: r.get::<_, i32>(3)? != 0,
                    title: r.get(4)?,
                    updated_at: r.get(5)?,
                    position_secs: r.get(6)?,
                    duration_secs: r.get(7)?,
                })
            })
            .map_err(StoreError::sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::sqlite)?;
        Ok(rows)
    }

    // -------------------------------------------------------------- search
    /// FTS5 prefix search. Every whitespace word becomes a prefix token, all of
    /// which must match. Returns item rows (missing excluded).
    pub fn search_fts(&self, query: &str, limit: usize) -> StoreResult<Vec<ItemRow>> {
        let terms: Vec<String> = query
            .split_whitespace()
            .filter(|w| !w.is_empty())
            .map(|w| format!("\"{}\"*", w.replace('"', "")))
            .collect();
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let expr = terms.join(" AND ");
        let sql = format!(
            "{} WHERE i.missing = 0 AND i.id IN (SELECT rowid FROM items_fts WHERE items_fts MATCH ?1)
             ORDER BY i.title COLLATE NOCASE LIMIT ?2",
            Self::item_select()
        );
        let mut st = self.conn.prepare(&sql).map_err(StoreError::sqlite)?;
        let rows = st
            .query_map(
                params![expr, i64::try_from(limit).unwrap_or(i64::MAX)],
                Self::map_item,
            )
            .map_err(StoreError::sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::sqlite)?;
        Ok(rows)
    }

    /// Substring search over title, series and path. Slower than FTS but matches
    /// inside words, which is what the prototype does and what users expect from
    /// a filter box.
    pub fn search_like(&self, query: &str, limit: usize) -> StoreResult<Vec<ItemRow>> {
        let words: Vec<String> = query
            .split_whitespace()
            .filter(|w| !w.is_empty())
            .map(|w| format!("%{}%", w.replace('%', "\\%").replace('_', "\\_")))
            .collect();
        if words.is_empty() {
            return Ok(Vec::new());
        }
        let mut clauses = Vec::new();
        for _ in 0..words.len() {
            clauses.push("(LOWER(i.title || ' ' || i.rel || ' ' || COALESCE(s.title, '')) LIKE ? ESCAPE '\\')".to_string());
        }
        let sql = format!(
            "{} WHERE i.missing = 0 AND {} ORDER BY i.title COLLATE NOCASE LIMIT {}",
            Self::item_select(),
            clauses.join(" AND "),
            limit
        );
        let mut st = self.conn.prepare(&sql).map_err(StoreError::sqlite)?;
        let lowered: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();
        let mut params_vec: Vec<&dyn rusqlite::ToSql> = Vec::new();
        for w in &lowered {
            params_vec.push(w);
        }
        let rows = st
            .query_map(params_vec.as_slice(), Self::map_item)
            .map_err(StoreError::sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::sqlite)?;
        Ok(rows)
    }

    // --------------------------------------------------------------- stats
    pub fn catalogue_stats(&self) -> StoreResult<CatalogueStats> {
        let mut stats = CatalogueStats::default();
        let libs = self.libraries()?;
        stats.libraries = libs.len();
        for lib in &libs {
            let (items, missing, pages, bytes): (i64, i64, i64, i64) = self
                .conn
                .query_row(
                    "SELECT COUNT(*) - COALESCE(SUM(missing), 0), COALESCE(SUM(missing), 0),
                            COALESCE(SUM(CASE WHEN missing = 0 THEN COALESCE(page_count, 0) ELSE 0 END), 0),
                            COALESCE(SUM(CASE WHEN missing = 0 THEN size ELSE 0 END), 0)
                     FROM items WHERE library_id = ?1",
                    params![lib.id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .map_err(StoreError::sqlite)?;
            let dirs = self.dir_count(Some(lib.id))?;
            let ls = LibraryStats {
                library_id: lib.id,
                name: lib.name.clone(),
                kind: lib.kind.clone(),
                mature: lib.mature,
                items: usize::try_from(items).unwrap_or(usize::MAX),
                missing: usize::try_from(missing).unwrap_or(usize::MAX),
                dirs,
                pages,
                bytes,
            };
            stats.items += ls.items;
            stats.missing += ls.missing;
            stats.pages += ls.pages;
            stats.bytes += ls.bytes;
            stats.per_library.push(ls);
        }
        stats.dirs = self.dir_count(None)?;
        let series: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM series", [], |r| r.get(0))
            .map_err(StoreError::sqlite)?;
        stats.series = usize::try_from(series).unwrap_or(usize::MAX);
        Ok(stats)
    }

    /// Sizes that occur more than once, the candidates for exact duplicates.
    pub fn duplicate_sizes(&self, library_id: Option<i64>) -> StoreResult<Vec<(i64, i64)>> {
        let sql = "SELECT size, COUNT(*) FROM items WHERE missing = 0 AND size > 0";
        let mut out = Vec::new();
        match library_id {
            Some(id) => {
                let mut st = self
                    .conn
                    .prepare(&format!(
                        "{sql} AND library_id = ?1 GROUP BY size HAVING COUNT(*) > 1 ORDER BY size"
                    ))
                    .map_err(StoreError::sqlite)?;
                let rows = st
                    .query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?)))
                    .map_err(StoreError::sqlite)?;
                for row in rows {
                    out.push(row.map_err(StoreError::sqlite)?);
                }
            }
            None => {
                let mut st = self
                    .conn
                    .prepare(&format!(
                        "{sql} GROUP BY size HAVING COUNT(*) > 1 ORDER BY size"
                    ))
                    .map_err(StoreError::sqlite)?;
                let rows = st
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                    .map_err(StoreError::sqlite)?;
                for row in rows {
                    out.push(row.map_err(StoreError::sqlite)?);
                }
            }
        }
        Ok(out)
    }

    pub fn items_by_size(&self, size: i64) -> StoreResult<Vec<ItemRow>> {
        let sql = format!(
            "{} WHERE i.missing = 0 AND i.size = ?1 ORDER BY i.rel",
            Self::item_select()
        );
        let mut st = self.conn.prepare(&sql).map_err(StoreError::sqlite)?;
        let rows = st
            .query_map(params![size], Self::map_item)
            .map_err(StoreError::sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::sqlite)?;
        Ok(rows)
    }

    /// Items at or below a folder, for the "N volumes" line on a folder card.
    pub fn count_items_under(&self, library_id: i64, rel: &str) -> StoreResult<usize> {
        // The library root is the empty relative path: every item is below it.
        let pattern = if rel.is_empty() {
            "%".to_string()
        } else {
            format!("{}/%", rel.replace('%', "\\%"))
        };
        let n: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM items
                 WHERE library_id = ?1 AND missing = 0
                   AND (rel = ?2 OR rel LIKE ?3 ESCAPE '\\')",
                params![library_id, rel, pattern],
                |r| r.get(0),
            )
            .map_err(StoreError::sqlite)?;
        Ok(usize::try_from(n).unwrap_or(usize::MAX))
    }

    pub fn item_count(&self) -> StoreResult<usize> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM items WHERE missing = 0", [], |r| {
                r.get(0)
            })
            .map_err(StoreError::sqlite)?;
        Ok(usize::try_from(n).unwrap_or(usize::MAX))
    }

    pub fn last_scan(&self, library_id: Option<i64>) -> StoreResult<Option<ScanRecord>> {
        let mut out: Vec<ScanRecord> = Vec::new();
        let map = |r: &rusqlite::Row<'_>| -> rusqlite::Result<ScanRecord> {
            let to_usize = |v: i64| usize::try_from(v.max(0)).unwrap_or(usize::MAX);
            Ok(ScanRecord {
                id: r.get(0)?,
                library_id: r.get(1)?,
                started_at: r.get(2)?,
                finished_at: r.get(3)?,
                added: to_usize(r.get(4)?),
                updated: to_usize(r.get(5)?),
                missing: to_usize(r.get(6)?),
                total: to_usize(r.get(7)?),
                ms: to_usize(r.get(8)?),
                full: r.get::<_, i32>(9)? != 0,
            })
        };
        match library_id {
            Some(id) => {
                let mut st = self
                    .conn
                    .prepare("SELECT id, library_id, started_at, finished_at, added, updated, missing, total, ms, full FROM scans WHERE library_id = ?1 ORDER BY id DESC LIMIT 1")
                    .map_err(StoreError::sqlite)?;
                let mut rows = st.query(params![id]).map_err(StoreError::sqlite)?;
                if let Some(row) = rows.next().map_err(StoreError::sqlite)? {
                    out.push(map(row).map_err(StoreError::sqlite)?);
                }
            }
            None => {
                let mut st = self
                    .conn
                    .prepare("SELECT id, library_id, started_at, finished_at, added, updated, missing, total, ms, full FROM scans ORDER BY id DESC LIMIT 1")
                    .map_err(StoreError::sqlite)?;
                let mut rows = st.query([]).map_err(StoreError::sqlite)?;
                if let Some(row) = rows.next().map_err(StoreError::sqlite)? {
                    out.push(map(row).map_err(StoreError::sqlite)?);
                }
            }
        }
        Ok(out.into_iter().next())
    }

    /// Record one checksum verification run, so "what is due" becomes a query.
    pub fn record_verification(&mut self, row: &VerificationRow) -> StoreResult<()> {
        self.conn
            .execute(
                "INSERT INTO verifications(library_id, checked_at, checked, ok, missing, changed,
                     baseline, ms)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    row.library_id,
                    row.checked_at,
                    i64::try_from(row.checked).unwrap_or(i64::MAX),
                    i64::try_from(row.ok).unwrap_or(i64::MAX),
                    i64::try_from(row.missing).unwrap_or(i64::MAX),
                    i64::try_from(row.changed).unwrap_or(i64::MAX),
                    i32::from(row.baseline),
                    i64::try_from(row.ms).unwrap_or(i64::MAX)
                ],
            )
            .map_err(StoreError::sqlite)?;
        Ok(())
    }

    pub fn last_verification(&self, library_id: i64) -> StoreResult<Option<VerificationRow>> {
        self.conn
            .query_row(
                "SELECT library_id, checked_at, checked, ok, missing, changed, baseline, ms
                 FROM verifications WHERE library_id = ?1 ORDER BY id DESC LIMIT 1",
                params![library_id],
                |r| {
                    let to_usize = |v: i64| usize::try_from(v.max(0)).unwrap_or(usize::MAX);
                    Ok(VerificationRow {
                        library_id: r.get(0)?,
                        checked_at: r.get(1)?,
                        checked: to_usize(r.get(2)?),
                        ok: to_usize(r.get(3)?),
                        missing: to_usize(r.get(4)?),
                        changed: to_usize(r.get(5)?),
                        baseline: r.get::<_, i32>(6)? != 0,
                        ms: to_usize(r.get(7)?),
                    })
                },
            )
            .optional()
            .map_err(StoreError::sqlite)
    }

    pub fn record_scan(&mut self, rec: &ScanRecord) -> StoreResult<i64> {
        self.conn
            .execute(
                "INSERT INTO scans(library_id, started_at, finished_at, added, updated, missing, total, ms, full)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    rec.library_id,
                    rec.started_at,
                    rec.finished_at,
                    i64::try_from(rec.added).unwrap_or(i64::MAX),
                    i64::try_from(rec.updated).unwrap_or(i64::MAX),
                    i64::try_from(rec.missing).unwrap_or(i64::MAX),
                    i64::try_from(rec.total).unwrap_or(i64::MAX),
                    i64::try_from(rec.ms).unwrap_or(i64::MAX),
                    i32::from(rec.full)
                ],
            )
            .map_err(StoreError::sqlite)?;
        Ok(self.conn.last_insert_rowid())
    }
}

/// One checksum verification run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationRow {
    pub library_id: i64,
    pub checked_at: String,
    pub checked: usize,
    pub ok: usize,
    pub missing: usize,
    pub changed: usize,
    /// True when the run created the manifest instead of comparing against it.
    pub baseline: bool,
    pub ms: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanRecord {
    pub id: i64,
    pub library_id: Option<i64>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub added: usize,
    pub updated: usize,
    pub missing: usize,
    pub total: usize,
    pub ms: usize,
    #[serde(default)]
    pub full: bool,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn store() -> Store {
        Store::open_in_memory().unwrap()
    }

    fn item(rel: &str, title: &str, size: i64) -> NewItem {
        NewItem {
            library_id: 1,
            rel: rel.to_string(),
            parent: rel
                .rsplit_once('/')
                .map(|(p, _)| p.to_string())
                .unwrap_or_default(),
            name: rel.rsplit('/').next().unwrap_or(rel).to_string(),
            title: title.to_string(),
            container: "zip".to_string(),
            size,
            mtime: 7,
            page_count: None,
            series_rel: None,
            series_title: None,
            volume: None,
            rtl: None,
            comic_info: false,
            confidence: 0.5,
            needs_review: false,
            media_kind: "comics".to_string(),
        }
    }

    #[test]
    fn library_roundtrip() {
        let mut s = store();
        let id = s
            .add_library("Comics", "D:/x", "comics", false, None)
            .unwrap();
        assert_eq!(s.libraries().unwrap().len(), 1);
        assert_eq!(s.library(id).unwrap().unwrap().name, "Comics");
        assert!(s.library_by_name("comics").unwrap().is_some());
        s.remove_library(id).unwrap();
        assert!(s.libraries().unwrap().is_empty());
    }

    #[test]
    fn upsert_reports_added_then_unchanged() {
        let mut s = store();
        s.add_library("L", "D:/x", "comics", false, None).unwrap();
        let it = item("a/b.cbz", "B", 10);
        let (id, state) = s.upsert_item(&it).unwrap();
        assert_eq!(state, Upsert::Added);
        let (id2, state) = s.upsert_item(&it).unwrap();
        assert_eq!(state, Upsert::Unchanged);
        assert_eq!(id, id2);
    }

    #[test]
    fn upsert_detects_size_change_and_clears_missing() {
        let mut s = store();
        s.add_library("L", "D:/x", "comics", false, None).unwrap();
        let mut it = item("a/b.cbz", "B", 10);
        let (id, _) = s.upsert_item(&it).unwrap();
        it.size = 11;
        let (_, state) = s.upsert_item(&it).unwrap();
        assert_eq!(state, Upsert::Updated);
        let row = s.item_by_id(id).unwrap().unwrap();
        assert_eq!(row.size, 11);
        assert!(!row.missing);
    }

    #[test]
    fn missing_marking_keeps_row_and_progress() {
        let mut s = store();
        s.add_library("L", "D:/x", "comics", false, None).unwrap();
        let (id, _) = s.upsert_item(&item("a/b.cbz", "B", 10)).unwrap();
        s.progress_set("lib:1|a/b.cbz", 3, 20, false, Some("B"))
            .unwrap();
        let mut seen = HashSet::new();
        seen.insert("other".to_string());
        assert_eq!(s.mark_missing_except(1, &seen).unwrap(), 1);
        assert!(s.item_by_id(id).unwrap().unwrap().missing);
        assert_eq!(s.items_under(1, "a").unwrap().len(), 0);
        assert!(s.progress_get("lib:1|a/b.cbz").unwrap().is_some());
    }

    #[test]
    fn fts_and_like_search_find_the_same_item() {
        let mut s = store();
        s.add_library("L", "D:/x", "manga", false, None).unwrap();
        s.upsert_item(&item("Yotsuba/Yotsuba v01.cbz", "Yotsuba v01", 1))
            .unwrap();
        let fts = s.search_fts("yotsuba", 10).unwrap();
        assert_eq!(fts.len(), 1);
        let like = s.search_like("yotsub", 10).unwrap();
        assert_eq!(like.len(), 1, "substring search matches inside a word");
        assert!(s.search_fts("nothinghere", 10).unwrap().is_empty());
    }

    #[test]
    fn series_is_created_once_and_shared() {
        let mut s = store();
        s.add_library("L", "D:/x", "comics", false, None).unwrap();
        let mut a = item("S/a.cbz", "a", 1);
        a.series_rel = Some("S".into());
        a.series_title = Some("Series".into());
        let mut b = item("S/b.cbz", "b", 1);
        b.series_rel = Some("S".into());
        b.series_title = Some("Series".into());
        let (ia, _) = s.upsert_item(&a).unwrap();
        let (ib, _) = s.upsert_item(&b).unwrap();
        let (ra, rb) = (
            s.item_by_id(ia).unwrap().unwrap(),
            s.item_by_id(ib).unwrap().unwrap(),
        );
        assert_eq!(ra.series_id, rb.series_id);
        assert_eq!(s.series_list(None).unwrap().len(), 1);
    }

    #[test]
    fn duplicate_sizes_only_lists_repeats() {
        let mut s = store();
        s.add_library("L", "D:/x", "comics", false, None).unwrap();
        s.upsert_item(&item("a.cbz", "a", 5)).unwrap();
        s.upsert_item(&item("b.cbz", "b", 5)).unwrap();
        s.upsert_item(&item("c.cbz", "c", 9)).unwrap();
        let sizes = s.duplicate_sizes(None).unwrap();
        assert_eq!(sizes, vec![(5, 2)]);
        assert_eq!(s.items_by_size(5).unwrap().len(), 2);
    }

    #[test]
    fn stats_add_up() {
        let mut s = store();
        s.add_library("L", "D:/x", "comics", false, None).unwrap();
        let mut it = item("a/b.cbz", "B", 10);
        it.page_count = Some(20);
        s.upsert_item(&it).unwrap();
        let st = s.catalogue_stats().unwrap();
        assert_eq!(st.items, 1);
        assert_eq!(st.pages, 20);
        assert_eq!(st.bytes, 10);
        assert_eq!(st.per_library.len(), 1);
    }

    #[test]
    fn dirs_are_listed_but_volume_dirs_are_not_folders() {
        let mut s = store();
        s.add_library("L", "D:/x", "manga", false, None).unwrap();
        s.upsert_dir(1, "Series", "", "Series", false).unwrap();
        s.upsert_dir(1, "Series/Vol 1", "Series", "Vol 1", true)
            .unwrap();
        assert_eq!(s.dirs_under(1, "").unwrap().len(), 1);
        assert!(s.dirs_under(1, "Series").unwrap().is_empty());
    }

    #[test]
    fn key_roundtrip() {
        let mut s = store();
        s.add_library("L", "D:/x", "comics", false, None).unwrap();
        let (_, _) = s.upsert_item(&item("a/b c.cbz", "B", 10)).unwrap();
        let row = s.item_by_rel(1, "a/b c.cbz").unwrap().unwrap();
        assert_eq!(row.key(), "lib:1|a/b c.cbz");
    }

    #[test]
    fn counting_under_the_root_counts_everything() {
        let mut s = store();
        s.add_library("L", "D:/x", "comics", false, None).unwrap();
        s.upsert_item(&item("a/one.cbz", "one", 1)).unwrap();
        s.upsert_item(&item("b/two.cbz", "two", 2)).unwrap();
        s.upsert_item(&item("three.cbz", "three", 3)).unwrap();
        assert_eq!(
            s.count_items_under(1, "").unwrap(),
            3,
            "the root holds everything"
        );
        assert_eq!(s.count_items_under(1, "a").unwrap(), 1);
        assert_eq!(s.count_items_under(1, "three.cbz").unwrap(), 1);
    }
}
