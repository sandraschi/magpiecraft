---
name: magpiecraft-workflow
description: Everyday MagpieCraft flows: libraries, scanning, browsing, search, reading progress.
---

# MagpieCraft workflow

MagpieCraft inventories media the user already owns. It never downloads,
never writes into a library, and every rename, move or convert is a
plan-first diff applied on confirm.

## Model

A library is a named folder with a kind: comics, manga, magazines, video,
audio, ebook, photo or other. The catalogue (SQLite FTS5) indexes folders and
items; covers, caches and manifests live under the app data home, never in the
library.

## Everyday flows

- Add a folder: `media_catalog` operation `library_add` (name, root, kind,
  mature flag). List them with operation `libraries`.
- Scan: `media_catalog` operation `scan`. Incremental by default; `full`
  re-reads everything. Removing a library (`library_remove` with
  `confirm: true`) drops index rows only.
- Browse: `media_catalog` operation `browse` (one folder by key, or all
  libraries when the key is omitted). Series grouping via operation `series`.
- Search the whole collection: `media_search` operation `search` (every word
  must match; `mode` auto, fts or substring). One record: operation `item`.
- Resume: `media_catalog` operation `continue` lists recently read items.
- Progress: `media_read` operations `progress_get` and `progress_set`.
- Totals: `media_catalog` operation `stats`; engine state: operation `status`.

## Rules

Reading, scanning and search never change a file. Writes only happen through
`media_curate` plans with `confirm: true`, and deletes never happen at all:
unwanted items go to quarantine and journals reverse applies.
