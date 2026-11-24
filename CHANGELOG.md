# Changelog

All notable changes to MagpieCraft. Newest first. Dates are the day the work landed.

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
- 116 tests, clippy clean with the deny gates, fmt clean, MCP probe verified.

Not in this version (see the README matrix): CBR/RAR and PDF page reading, identify/tag/rename,
near-duplicates, backup planner, conversions, federated sources, the collection guide, installer.

## Unreleased

Nothing yet.
