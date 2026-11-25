//! Schema and migrations.
//!
//! Migration rule: never rewrite an old statement's meaning. Add a new numbered
//! block and bump `SCHEMA_VERSION`. Every statement here is `IF NOT EXISTS`, so a
//! fresh file and an existing file both end up valid.

pub const SCHEMA_VERSION: &str = "5";

/// Additive step from schema 1 to 2: the review queue needs to know how confident
/// the filename parse was, and whether a human should look at it.
pub const MIGRATE_1_TO_2: &str = r#"
ALTER TABLE items ADD COLUMN confidence REAL NOT NULL DEFAULT 0;
ALTER TABLE items ADD COLUMN needs_review INTEGER NOT NULL DEFAULT 0;
"#;

/// Additive step from schema 2 to 3: an item carries its own media kind, because a
/// video in a comics library is still a video.
pub const MIGRATE_2_TO_3: &str = r#"
ALTER TABLE items ADD COLUMN media_kind TEXT NOT NULL DEFAULT '';
"#;

/// Data step from schema 3 to 4: rows written before the kind column existed take
/// their kind from the library they live in. That is exactly right for everything
/// the old scanner indexed (comic archives, page folders, PDFs in book libraries);
/// the next scan refines anything else.
pub const MIGRATE_3_TO_4: &str = r#"
UPDATE items
   SET media_kind = COALESCE(
       (SELECT kind FROM libraries WHERE libraries.id = items.library_id),
       'other')
 WHERE media_kind = '';
"#;

/// Additive step from schema 4 to 5: films and tracks resume in seconds, not pages.
pub const MIGRATE_4_TO_5: &str = r#"
ALTER TABLE items ADD COLUMN duration_secs INTEGER;
ALTER TABLE progress ADD COLUMN position_secs INTEGER;
ALTER TABLE progress ADD COLUMN duration_secs INTEGER;
"#;

pub const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS libraries (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL UNIQUE,
    root        TEXT NOT NULL,
    kind        TEXT NOT NULL,
    mature      INTEGER NOT NULL DEFAULT 0,
    rtl_default INTEGER,
    created_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS dirs (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    library_id INTEGER NOT NULL,
    rel        TEXT NOT NULL,
    parent     TEXT NOT NULL,
    name       TEXT NOT NULL,
    volume     INTEGER NOT NULL DEFAULT 0,
    UNIQUE(library_id, rel)
);
CREATE INDEX IF NOT EXISTS idx_dirs_parent ON dirs(library_id, parent);

CREATE TABLE IF NOT EXISTS series (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    library_id INTEGER NOT NULL,
    rel        TEXT NOT NULL,
    title      TEXT NOT NULL,
    rtl        INTEGER,
    UNIQUE(library_id, rel)
);

CREATE TABLE IF NOT EXISTS items (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    library_id INTEGER NOT NULL,
    rel        TEXT NOT NULL,
    parent     TEXT NOT NULL,
    name       TEXT NOT NULL,
    title      TEXT NOT NULL,
    container  TEXT NOT NULL,
    size       INTEGER NOT NULL DEFAULT 0,
    mtime      INTEGER NOT NULL DEFAULT 0,
    page_count INTEGER,
    series_id  INTEGER,
    volume     TEXT,
    rtl        INTEGER,
    comic_info INTEGER NOT NULL DEFAULT 0,
    hash       TEXT,
    page_offset INTEGER NOT NULL DEFAULT 0,
    missing    INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    UNIQUE(library_id, rel)
);
CREATE INDEX IF NOT EXISTS idx_items_parent ON items(library_id, parent, missing);
CREATE INDEX IF NOT EXISTS idx_items_size ON items(size);
CREATE INDEX IF NOT EXISTS idx_items_series ON items(series_id);

CREATE TABLE IF NOT EXISTS progress (
    key        TEXT PRIMARY KEY,
    page       INTEGER NOT NULL,
    total      INTEGER NOT NULL,
    finished   INTEGER NOT NULL DEFAULT 0,
    title      TEXT,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS scans (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    library_id  INTEGER,
    started_at  TEXT NOT NULL,
    finished_at TEXT,
    added       INTEGER NOT NULL DEFAULT 0,
    updated     INTEGER NOT NULL DEFAULT 0,
    missing     INTEGER NOT NULL DEFAULT 0,
    total       INTEGER NOT NULL DEFAULT 0,
    ms          INTEGER NOT NULL DEFAULT 0,
    full        INTEGER NOT NULL DEFAULT 0
);

CREATE VIRTUAL TABLE IF NOT EXISTS items_fts USING fts5(
    title,
    series,
    path,
    library,
    tokenize = 'unicode61 remove_diacritics 2'
);
"#;
