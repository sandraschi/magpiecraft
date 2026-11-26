# MagpieCraft Help

Inventory, reader and curator for media you already own. MIT OR Apache-2.0.
No telemetry, no cloud account. This file mirrors the in-app Help Center.

## Get started

1. Open Settings and add a library: a name, the folder on your disk, its kind.
2. Scan it. File menu, Scan all libraries, or the Scan button on the library row.
3. Browse: folders and volumes on the Libraries page, whole-collection search with `/`.
4. Read: Enter opens a comic, manga or magazine in the reader. Anything else
   offers Launch, which hands the file to VLC, mpv or your system handler.
   Nothing plays inside this app.
5. Curate: the Organise page plans renames, verifies checksums and tracks
   backups. Every plan is a diff first, applied on confirm, recorded in a
   journal you can undo.

## Functions

- Catalogue: libraries, incremental scan, whole-collection search, series and
  reading progress.
- Reader: CBZ and ZIP pages, CBR through 7-Zip, PDF through PdfCraft,
  ComicInfo metadata, spreads, half-page and right-to-left.
- Playback handoff: films, tracks, books and photos launch in VLC, mpv or the
  system handler with resume positions. No decoder lives here.
- Organise: film and TV naming presets, rename plans with diff, journal and
  undo, quarantine instead of delete, review queue.
- Verify: BLAKE3 manifests, duplicate and near-duplicate grouping,
  verification schedule, backup status and verified copies.
- Subtitles: list, shift, convert and extract sidecar files next to your films.
- Convert plans: FFmpeg presets turned into reviewable plans, applied through
  the same journal as renames.

## Usage

In this window: Libraries page (folders, volumes, Continue reading, `/`
search), Reader (arrows, Space, H, T, R, Home, End), Organise (Review, Plans,
Journals), Settings (libraries, reading preferences, skins, zoom, LLM
provider and chat), Statistics (totals plus per-library items, pages, bytes).

Command line (same engine, `--json` for scripts): `magpiecraft status`,
`libraries`, `scan`, `browse`, `search`, `stats`, `review`, `plan-rename`,
`apply-plan`, `journals`, `undo`, `duplicates`, `manifest-write`,
`manifest-verify`, `verify-status`, `verify-run`, `backup-status`.

MCP server: `magpiecraft mcp` speaks MCP over stdio (`tools/list`, then
`tools/call`). Portmanteau tools: `media_catalogue`, `media_search`,
`media_curate`, `media_verify`, `media_transform`, plus `library_add`,
`library_remove`, `settings`, `settings_set`, `sources` and `tools`.

## Tips and tricks

- The search row searches the whole collection. The filter above the grid
  narrows the folder you are in.
- Scan is incremental: unchanged files are skipped by path, size and time.
  Full rescan re-reads everything and takes longer.
- Removed or unplugged files are kept as missing, so progress is never lost.
  Plug the drive back and scan again.
- Keyboard: arrows, Space, Shift+Space turn pages. H half page, T two pages,
  R right to left. Home and End jump. Escape goes back. `/` jumps to search.
  Ctrl+J opens the chat dock. Ctrl plus mouse wheel zooms the window.

## Common pitfalls and safety

- Mature-flagged libraries are hidden from the grid, search and exports until
  Settings shows them. That is a filter, not a lock.
- Removing a library drops its index rows. The files stay on disk, but rescan
  the folder to get them back.
- Rename, move and convert plans show a diff first and write a journal.
  Deletes go to quarantine, and Undo reverses the journal.
- CBR needs 7-Zip and PDF needs PdfCraft on PATH; video identity needs
  ffprobe. The status command reports what was found.
- Playback is a handoff: positions resume where you left off, but chapters,
  subtitle rendering and transcoding belong to the player.
- A chat answer is grounded in this catalogue, not the internet. With a cloud
  provider selected, the app says so before anything is sent.

## Planned extensions

- SMART drive health: read the disks your libraries sit on and warn before
  they fail.
- Local subtitle generation with a Rust engine, no cloud service.
- Undecided: an OPDS listener for e-readers. It would be the only network port
  in the product, so it stays undecided until the rest is solid.

## Other crafts

- PdfCraft renders the PDF pages (`pdfcraft-cli info` and `render`). Its repo
  is upstream and read-only from here.
- 7-Zip extracts CBR and RAR (`7z` on PATH, Igor Pavlov's tool, invoked only).
- FFmpeg probes (`ffprobe`) and converts (`ffmpeg`) video and audio. Invoked
  only, never linked.
- VLC first, mpv second, then the OS handler for playback. No decoder,
  subtitle renderer or transcoder lives in this repo.
