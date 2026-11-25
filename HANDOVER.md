---
updated: 2026-11-24T14:40:00+01:00
agent: opencode-deepseek
status: in-progress
tier: "Tier 1 built, Tier 1 exit review pending"
---

# MagpieCraft handover

## What is happening

Session A (opencode, DeepSeek V4 Flash) built Tier 0 and Tier 1 of MagpieCraft per
`mcp-central-docs/projects/craft-handover-2026-11-24/HANDOVER_AGY.md` (tasks A0 and A1) and
`WEEK_PLAN_OPENCODE.md`. The comics reader was built first, then inventory and search, as the week plan
asked.

Tier 2 (A2: identify, tag, rename, CBR/RAR and PDF pages) has **not** been started. Per the week plan it
waits for the other session to review this Tier 1 exit.

## Last concrete action

Tier 2 started on Sandra's instruction (`par ordre de mufti`), overriding the wait for session B's
review. Two increments landed:

**Containers.** CBR/RAR/CB7/7Z pages now come from 7-Zip and PDF pages from PdfCraft, so the reader
handles the whole collection. Page lists are an index read (no extraction); a page extracts once into
the app cache behind a cross-process lock and a size cap. A missing tool is `not_configured` with the
variable to set. `page <key> <n> --out path` exports one page byte for byte.

**Organise.** Plans with diffs, apply on confirm, an undo journal, quarantine instead of delete, and a
review queue for uncertain filenames. Reading progress follows a rename because the progress key is
rewritten with the path. Verified end to end from the CLI on a real folder tree: 5 operations planned,
refused without confirm, applied, browsed with the new names and the old page counts, undone back to
the original names, and a second undo refused.

Measured:

- `cargo test --workspace`: **146 passed**, 0 failed; clippy and fmt clean.
- Real CBR: page list ~210 ms, first page 628 ms (index read + extraction + read), next page ~190 ms.
- Page counts for archives that need a tool: 2,105 items in 17 minutes, once.
- MCP stdio probe: 5 tools, `media_curate` included.
- The Organise page renders the review queue, plan diffs and journals (screenshot in
  `docs/screenshots/organise.png`), and `--page organise` starts the app there for screenshots.

Two parser improvements came out of running the review queue against the real collection: a volume
folder whose name starts with the marker ("Vol 1 Moss and Rain") now keeps its volume, and a number in
the middle of a name ("Alef Thau - 01 - L'enfant Tronc") is a volume with the rest kept as a subtitle,
so a rename no longer throws a subtitle away.

Still open in A2: identify from metadata (local dumps first: GCD, MangaBaka, the user's own Heavy Metal
catalogue; then Open Library and MusicBrainz with per-provider limiters and provenance), NFO and
artwork writing, tag writing, the Jellyfin/OPDS clients, the OPDS server.

## Tier 2, third block: every kind, not just comics

Sandra corrected the concept ("magpiecraft is not just for comics"), and she was right: the scanner
only turned comic containers into items, so 17,328 video files and 14,051 books were walked and
dropped. Fixed properly:

- `scan::family_of` classifies archives, PDFs, images, video, audio, ebooks and subtitles;
  `core::classify_item` combines that with the library kind. Schema 4 adds `items.media_kind` with a
  data migration that backfills old rows from their library kind.
- Kinds are first-class: statistics per kind, kind-aware page reasons ("video is played in mpv,
  Jellyfin or the OS"), kind-aware headings and cards ("VIDEOS (7)", a film card says "video").
- Presets follow the kind: films by title and year (release tags dropped, text after the year kept),
  TV episodes by series folder and SxxExx (season 0 specials included), comics and manga as before.
  Books and music are skipped with a reason instead of guessed.
- Book PDFs no longer get a page count probed, so a book library scans in seconds.

Measured: 44,493 items across 17 libraries, 5.0 TiB, 1,696,651 pages. Video library 7.8 s for 17,399
items; the Calibre book library 11.1 s for 14,320. The video planner proposes 545 in-place renames and
reports 16,785 skips.

Also fixed: `magpiecraft-mcp libraries` printed "no libraries yet" because `dispatch` wraps list
results; the printer now looks inside `value`. 146 tests, clippy and fmt clean.

## Playback, D8: hand-off, not a decoder (2026-11-25)

Sandra challenged "playback stays out": correct challenge, because the PRD's hand-off line had been
implemented as a dead end. Now:

- `open <key>` (CLI, MCP `media_read open`, and a click on a film card) launches the configured player
  with the file and the resume position: VLC (`--start-time`), then mpv (`--start`), then the OS
  handler; `MAGPIECRAFT_PLAYER` overrides. Detached, own process group.
- Schema 5: playback position and duration live in `progress` (seconds), duration cached on the item
  after one ffprobe. Continue lists films and comics in one row with the right units.
- Comics are refused with a reason. Still out, per D8: decoder, subtitle rendering, transcoding,
  streaming server (LGPL/GPL linkage, and HomeServeCraft owns remote consumption).
- Optional later: mpv `--wid` embedding plus JSON IPC, no decoder added.

152 tests, clippy and fmt clean. Live check launched VLC on a real film and continue showed
`0:00 / 1:44:28`.

## Tier 3, first slice: integrity

- **Verification schedule:** `verify` says which libraries are due and what the last run found;
  `verify-run` writes the baseline first, compares afterwards, and records each run (schema 6,
  `verifications`). Live: 55 files / 1 GB baselined in 8 s.
- **Cleanup planner:** empty folders, orphan sidecars, samples and junk as quarantines with reasons;
  kind-aware (an ebook library is never touched automatically); apply on confirm, undo restores.
- **Backup status:** per library, complete / partial / not backed up against the configured roots; a
  count, not a checksum. Unconfigured is the honest default and is the current state.

Next in Tier 3: perceptual near-duplicates, SMART drive health, ffmpeg presets, subtitle tools
(extract, convert, shift, local generation) and the verified backup copy.

## Next step

1. The remaining A2 work, in this order: identify from metadata (local dumps first: GCD, MangaBaka,
   the user's own Heavy Metal catalogue; then Open Library and MusicBrainz with per-provider limiters
   and provenance), NFO and artwork writing in Jellyfin/Kodi layout, then the Jellyfin and OPDS
   read-only clients in `magpie-adapters`. The OPDS server is the last item and is opt-in.
2. Then the Tier 2 exit review (fresh clone, tests, clippy, fmt, the organise round trip, README
   matrix) by an agent that did not write it.
3. Tier 3 onwards needs a new handover.

## Verification (for the reviewer)

```powershell
git clone https://github.com/sandraschi/magpiecraft
cd magpiecraft
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace                      # expect 146 passed
cargo run -q -p magpiecraft-app -- --smoke  # headless app wiring
magpiecraft-mcp.exe status --json
```

Then the organise round trip, on a copy of any folder with a few archives:

```powershell
$env:MAGPIECRAFT_HOME = "<scratch>"
magpiecraft-mcp.exe library-add Demo "<copy of a folder>" --kind comics
magpiecraft-mcp.exe scan
magpiecraft-mcp.exe review
magpiecraft-mcp.exe plan-rename            # note the plan id, read the diff
magpiecraft-mcp.exe apply <plan-id>        # must refuse without --confirm
magpiecraft-mcp.exe apply <plan-id> --confirm
magpiecraft-mcp.exe browse lib:1           # new names, same page counts
magpiecraft-mcp.exe undo <journal-id> --confirm
```

And the two behavioural claims against the prototype at `http://127.0.0.1:8766/`:

- `docs/READER_PARITY.md` lists every scenario with the test that locks it. Open a real volume in both
  and compare spreads, half page, RTL and the page offset.
- Covers: the grid should look like the prototype's, folders using their first item.

## Blockers

None. Two known limits are deliberate and documented rather than blockers:

- CBR/RAR and PDF pages are indexed but not readable (Tier 2), and say so.
- The scan is single-threaded. 100,000 files walk in 478 ms; the slow part is the first-time page count
  of every archive and every PDF (one `pdfcraft-cli` process each).

## Context the next agent needs

- Item identity is `lib:<library id>|<path relative to root>`. Progress survives a drive letter change
  because of that, so do not switch to absolute paths.
- A missing file is marked, never deleted. Browse and search exclude missing rows; statistics count
  them separately. Keep that when adding the rename journal.
- `magpiecraft-store` holds plain row types; `magpiecraft-core` holds the model and the engine. The
  engine is the only place that touches DB, filesystem and external tools together. Keep the direction
  `core -> leaves`; do not add a cycle.
- `magpie-adapters` must stay dependency-free of this workspace (decision D2, HomeServeCraft will pin
  it as a git dependency).
- The reader's layout maths are pure functions in `magpiecraft-comics::reader`. Any behaviour change
  goes there with a test, not in the egui code.
- The UI glyph rule is enforced by a test: ASCII plus `° × · …` only, everything else is painted as a
  vector or written in words. Adding an arrow character will fail `cargo test`.
- On Windows, drop the `Catalog` before deleting a temp directory in a test, or SQLite keeps the file
  locked (error 32).
- The real data home is `%LOCALAPPDATA%\magpiecraft`; `MAGPIECRAFT_HOME` overrides it and the smoke
  tests use it. Sandra's 13 libraries are registered there and indexed (12,761 items).
- Do not trust the UI because it compiles. `scripts/capture-window.ps1` captures the real window to a
  PNG, and the script must mark itself DPI aware or the bitmap is clipped and the layout looks broken
  when it is not. That trap cost an hour; the capture script documents it.
- `App::ui` gives an unbounded `Ui`. Never use `available_size()` or `Panel` inside it; take rects from
  `ctx.content_rect()`.

## Deliberately not done

- **Tier 2 of any kind.** Waiting for the Tier 1 review, per the week plan.
- **Installer, .mcpb, release assets.** Release tier work; the repo has no release and no version above
  0.1.0.
- **egui_kittest golden snapshots.** Tier 4 in the fleet standard; the reader maths are unit-tested
  instead and `--smoke` proves the headless wiring.
- **OPDS listener.** Tier 2 decision, and it would be the only port in the product.
- **Physical shelf catalogue, conversions, knowledge graph, source guide.** Pillars B to F, later tiers.
- **Threading the scan.** Measured first; 478 ms for 100,000 files did not justify a pool yet.

## Where things are

| Path | What |
|---|---|
| `docs/ARCHITECTURE.md` | Crates, command bus, schema, scanning contract |
| `docs/TOOLS.md` | CLI and MCP reference |
| `docs/READER_PARITY.md` | Prototype scenarios, test mapping, deliberate differences |
| `docs/BENCHMARKS.md` | Measured numbers |
| `docs/DEVELOPMENT.md` | Gates, fixtures, known debt |
| `mcp-central-docs/projects/mediacraft/PRD.md` | The binding PRD (v2) |
| `mcp-central-docs/projects/comiccraft/PRD.md` | Comics module scope |
| `mcp-central-docs/operations/session-log/craft-opencode-2026-11-24.md` | This session's log |
