# Changelog

All notable changes to MagpieCraft. Newest first. Dates are the day the work landed.

## 0.3.0 - 2026-11-24 (Tier 2: every kind, not just comics)

The catalogue is a collection-wide index. Comics are one module of it.

Added:

- **Every readable file is an item, with its own media kind.** `scan::family_of` classifies the name,
  `core::classify_item` combines it with the library kind: a `.mkv` is a video in any library, a `.jpg`
  is a page in a comics library and a photo in a photo library, a `.cbz` is an item only in a
  comics-like library, a book PDF is a book. Subtitles, `.opf` and other sidecars are never items.
- Schema 4 with an additive migration: `items.media_kind`, backfilled from the library kind for rows
  written before the column existed.
- Statistics per kind, and the app's section headings and cards follow the kind ("VIDEOS (7)", a film
  card says "video", a comic card shows its cover).
- **Kind-aware naming presets**: comics and manga as before, films as `Title (Year).ext` (release tags
  dropped, text after the year kept so a two-part film does not collapse), TV episodes as
  `Series - SxxExx - Title.ext` using the series folder and the `s01e02` / `1x02` marker, specials in
  season 0 included. Books, music and photos are left alone with a reason.
- `plan-rename --preset auto` picks per item; an explicit preset still overrides.
- Page counts are only probed for comic-like containers, so a book library does not pay for a PDF page
  count it never uses.

Measured on the reference collection: 44,493 items across 17 libraries, 5.0 TiB, 1,696,651 pages.
Video library (17,399 items) scanned in 8 s, book library (14,320 items) in 11 s. On the video library
the planner proposes 545 in-place renames and skips 16,785, mostly films whose name has no year.

Fixed:

- `magpiecraft-mcp libraries` printed "no libraries yet" because a list result is wrapped by `dispatch`;
  the printer now looks inside `value`. Regression test added.

## 0.2.0 - 2026-11-24 (Tier 2, first slice)

Reading every container in the collection, and organising it without losing anything.

Added:

- **CBR / RAR / CB7 / 7Z pages through 7-Zip.** Page lists come from the archive index without
  extracting; a page extracts the archive once into the app cache behind a cross-process lock, and the
  cache has a size cap with oldest-first eviction. A missing 7-Zip is `not_configured`, not an empty
  list.
- **PDF pages through PdfCraft.** Page count from `pdfcraft-cli info`, pages rendered at a configurable
  dpi into the cache. No PDF library is linked.
- **Page export**: `page <key> <n> --out path` writes one page byte for byte, and `media_read
  page_export` does the same over MCP.
- **Organise with plans, diffs and an undo journal.** `plan-rename` builds a deterministic plan from
  pure naming rules (golden tested), `apply --confirm` moves the files, updates the catalogue and the
  reading-progress keys, and rescans that library; `undo --confirm` puts everything back. Collisions,
  duplicate targets and vanished sources are reported as skipped, never overwritten. `sanitise_segment`
  stops a metadata field from escaping its folder.
- **Quarantine instead of delete**: `plan-quarantine` moves an item into
  `<home>/quarantine/<timestamp>/<library>/`, and undo brings it back.
- **Review queue**: items whose filename parse was uncertain are stored with a confidence and listed by
  `review`; they clear when the name or the ComicInfo is fixed.
- **`media_curate`** MCP tool (plan, apply, undo, quarantine, review) with `confirm` required for
  anything that touches a file.
- Schema 2 with an additive migration (confidence and review flag on items).
- 146 tests, including golden rename plans, apply/undo round trips with progress migration, quarantine
  round trips, and the CBR end-to-end path.

Not in this version: identify from metadata dumps or APIs, NFO and artwork writing, tag writing, the
Jellyfin/OPDS clients, the OPDS server.

## 0.1.0 - 2026-11-24 (Tier 0 and Tier 1)

First build. Scaffolding plus the inventory engine and the comics reader.

Added:

- Workspace of eleven crates under the ArtCraft containment harness: `magpiecraft-core`, `-store`,
  `-scan`, `-comics`, `-health`, `-tools`, `-meta`, `-sources`, `magpie-adapters`, `-mcp`, `-app`.
  Workspace lints deny `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented`.
- SQLite catalogue (WAL, additive schema, FTS5 index): libraries, dirs, items, series, progress,
  scans. Item identity is `lib:<id>|<relative path>`, so moving a drive does not orphan progress.
- Read-only inventory walker: natural sort, skipped system folders, leaf-most page-folder detection.
- Incremental scan by path, size and mtime. Unchanged archives are not reopened. A vanished file is
  marked missing, never deleted, and keeps its reading progress.
- CBZ/ZIP page source (natural order, `__MACOSX` and dotfiles skipped), numbered page folders,
  explicit `not_implemented` for CBR/RAR and PDF pages.
- ComicInfo.xml parser: series, number, volume, title, writer, penciller, publisher, year, summary,
  page count, language, genre, manga flag.
- Filename parsing for series and volume with a confidence score and a review flag.
- Reader layout maths: cover alone in two-page mode, spread pairing, right-to-left display order,
  half-page top and bottom with a 12 percent overlap, printed-page offset, prefetch list, clamping.
- Cover rendering to a JPEG cache under the app data home, from a worker thread in the app.
- Integrity: BLAKE3 on demand, exact duplicates by size then hash, checksum manifests and verification.
- External tool bridge: ffprobe (JSON parse), 7-Zip and PdfCraft discovery with honest
  `not_configured` results.
- CLI with `--json` for every command, and an MCP stdio server with four compound tools
  (`media_catalog`, `media_search`, `media_read`, `media_health`). A synthetic 100,000-file scan
  benchmark (`bench-scan`).
- egui desktop app: library grid with covers, breadcrumbs, folder filter, continue-reading row,
  whole-collection search, reader with spreads, half page, RTL, page jump, offset, progress, keyboard,
  Ctrl + wheel zoom, dark and light high-contrast themes, menu bar with the five-item Help suite.
- 117 tests, clippy clean with the deny gates, fmt clean, MCP probe verified.

Not in this version (see the README matrix): CBR/RAR and PDF page reading, identify/tag/rename,
near-duplicates, backup planner, conversions, federated sources, the collection guide, installer.

## Unreleased

Nothing yet.
