# Changelog

All notable changes to MagpieCraft. Newest first. Dates are the day the work landed.

## 0.9.0 - 2026-11-25 (Tier 3: ffmpeg presets)

Added:

- **`presets`**: the four named ffmpeg recipes with what each one runs: `remux-mkv` (copy to
  Matroska, fast, lossless), `mp4-compat` (H.264 CRF 20 + AAC 192k, faststart), `loudnorm` (copy
  video, EBU R128 to -16 LUFS), `strip-metadata` (drop every tag).
- **`plan-convert --preset`**: a conversion plan for one video or track. Refuses comics, books and
  photos ("presets apply to video and audio"), refuses a missing source and an existing target, and
  plans into `<stem>.<preset>.<ext>` beside the source so two presets never collide.
- **Apply encodes with ffmpeg, invoked not linked.** The output must probe as media with a positive
  duration or it is deleted and counted as a failure. Undo (`Encode` op) deletes the output, never
  the source. A backup-style plan does not trigger a pointless rescan.
- Live on the real collection: La Traviata 1982.avi (1.4 GB) remuxed to MKV in 31 s, output probed
  at 6617 s, then undone with the library byte-identical.

180 tests. Still planned in Tier 3: SMART drive health, local subtitle generation (Rust engine only).

Still planned in Tier 3: SMART drive health, local subtitle generation (Rust engine only).

## 0.8.0 - 2026-11-25 (Tier 3: subtitles)

Added:

- **`subs <key>`**: sidecar subtitles for an item (with their cue count) and the embedded subtitle
  tracks ffprobe reports (codec, language, subtitle index).
- **`subs-shift <path> --by <seconds>`**: moves every cue by a signed offset. A cue pushed before zero is
  truncated at zero (never stretched, or the text would appear after it finished); a cue pushed entirely
  before zero is dropped and counted. Writes `<stem>.shifted.srt` and refuses to overwrite.
- **`subs-convert <path> --to srt|vtt`**: SRT to WebVTT and back, with a proper `WEBVTT` header and
  `.` millisecond separators. ASS/SSA is read-only with a reason (it carries styling sections worth
  more than anything this app could regenerate); convert it with ffmpeg.
- **`subs-extract <key> --stream N --out file`**: pulls an embedded track out with
  `ffmpeg -map 0:s:N`, invoked, never linked. ffmpeg is now part of the detected toolchain.
- Parsing is tolerant on purpose: BOM, CRLF, a missing SRT index, WebVTT headers, `NOTE` blocks, cue ids
  and settings after the end time all parse. Non-UTF-8 subtitle files are refused by name rather than
  mangled.

173 tests. Still planned in Tier 3: ffmpeg presets (remux/normalise) and SMART drive health; local
subtitle generation is planned with the other transcription work.

## 0.7.0 - 2026-11-25 (Tier 3: verified backup copy)

Added:

- **`backup-plan --to <root>`** (MCP `media_health backup_plan`) plans the copies a backup still
  needs: only files missing at the target, never a file whose size differs (reported, not clobbered),
  and it refuses a root that is not mounted or that sits inside the library.
- **Verified copies.** Applying a backup plan copies each file and hashes both sides with BLAKE3. A
  copy that does not match is deleted again and reported as a failure. `--fast` keeps the size check
  only.
- **`Copy` as a plan operation**, next to `Move` and `Write`, with undo that removes the copies and
  never touches the library. A backup run no longer triggers a pointless rescan.
- Measured on the real collection: 55 files / 993 MB copied and verified in 10 s, 0 failures; backup
  status then reported "complete, 55 files, 55 items"; undo removed all 55 and left the library
  untouched.

166 tests. Still planned in Tier 3: ffmpeg presets, subtitle tools (extract, convert, shift, local
generation), SMART drive health.

## 0.6.0 - 2026-11-25 (Tier 3: near duplicates)

Added:

- **`near-dupes`** (MCP `media_health near_duplicates`), report only:
  - Images by perceptual hash: 64-bit dHash over a 9x8 grayscale, Hamming distance <= 10. A
    resized or recompressed copy matches; a mirrored picture does not. Capped at 5,000 photos
    per report.
  - Videos by identity key: title, year, episode marker (`S01E02`, `1x02`, season 0 specials and
    episode 0 both accepted) and a `Part`/`CD`/`Disc`/`Vol` suffix. Cast notes in brackets are not
    part of it, so the same recording kept under two names is found.
  - The note says which kind of comparison produced the group.
- Verified against the real collection: 17,381 videos produce exactly one possible duplicate
  (La Traviata 1982, kept twice with and without its cast note), and no false positives from
  series episodes or the two-part Shoah. The first version did group every episode of a series
  and Shoah Part 1 with Part 2; the identity key now carries the episode marker and the part
  suffix, and there is a test for each.

164 tests. Still planned in Tier 3: the verified backup copy, ffmpeg presets, subtitle tools
(extract, convert, shift, local generation), SMART drive health.

## 0.5.0 - 2026-11-25 (Tier 3: integrity)

Pillar C, the part after "find duplicates".

Added:

- **Verification schedule.** `verify` reports which libraries are due (default 30 days) with what
  the last run found; `verify-run` writes the manifest as a baseline the first time, compares
  afterwards, and records every run in a new `verifications` table (schema 6). A corrupt file is
  named by the next run ("changed 1").
- **Cleanup planner.** `plan-cleanup` proposes empty folders, orphan sidecars (`srt`, `sub`, `ass`,
  `ssa`, `vtt`, `idx`, `sup`, `nfo` whose stem matches nothing), sample files and junk
  (`Thumbs.db`, `.DS_Store`, `desktop.ini`) as quarantines, with a reason per file. Kind-aware: an
  ebook library is never cleaned automatically because Calibre keeps `.opf`, covers and
  `metadata.db` on purpose. Apply on confirm, undo restores.
- **Backup status.** `backup` compares each library with the copies under the configured backup
  roots (`backup-roots`) and says complete, partial or not backed up per library. A count, not a
  checksum: `verify_run` is what proves integrity. Verified copy plans are next.
- The scan walker reports files per folder, which is what makes "empty folder" and "orphan
  sidecar" answerable without a second pass over the disk.

Measured: a baseline of 55 files (1 GB) in 8 s. 158 tests.

Still planned in Tier 3: near-duplicate detection (perceptual), SMART drive health, ffmpeg presets,
subtitle tools (extract, convert, shift, local generation), and the verified backup copy.

## 0.4.0 - 2026-11-25 (Open with player, decision D8)

Playback is a hand-off, not a dead end.

Added:

- `open <key>` (CLI, MCP `media_read open`, and a click on a film card in the app) launches the
  configured player with the file and the position it left off at. VLC first (`--start-time`), then mpv
  (`--start`), then the OS handler; `MAGPIECRAFT_PLAYER` overrides discovery. The player is spawned
  detached in its own process group, so it outlives MagpieCraft.
- Films and tracks resume in seconds: schema 5 adds `progress.position_secs` and `duration_secs` and an
  `items.duration_secs` cache filled once by ffprobe. "Continue" lists a film and a comic in one row,
  in the right units (`1:30 / 1:44:28` versus `p.7/220`), and `open` records the attempt before
  launching so a missing player does not lose the film from the queue.
- Comics are refused with a reason ("comics are read in the reader, not handed to a player").
- Pure, tested pieces: `choose_player` precedence, `player_args` per player (including "zero means no
  flag"), and `format_secs`.

Still out, by the same decision: a built-in decoder, subtitle rendering, transcoding, and any streaming
server. 152 tests.

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
