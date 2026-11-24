# MagpieCraft

**Inventory and read the comics, manga and media collection you already own.**
One pure-Rust desktop app, one CLI and one MCP server. No downloads, no DRM circumvention, no accounts,
no telemetry, no open ports.

MagpieCraft is part of the Storytold craft family and follows the fleet's Pure-Rust ArtCraft
standard: an L1 engine crate, an L2 CLI + MCP binary and an L3 egui desktop app, all calling the same
command bus in process.

> **This project is not for piracy.** It contains no and will accept no features, advice or examples
> for obtaining copyrighted works from unauthorised sources (piracy sites, torrent or Usenet
> indexers, trackers, shadow libraries, scene sources), automating such downloads, breaking DRM, or
> scraping metadata from grey sources. Metadata comes from embedded data, your own files, openly
> licensed dumps and official APIs under their terms. Issues and pull requests that add any of the
> above are rejected.

## What it does today

- Scans libraries of CBZ/ZIP archives and numbered page folders, incrementally by path, size and mtime.
  A second scan of 100,000 files takes under a second and opens nothing it already knows.
- Reads ComicInfo.xml, groups volumes into series, and defaults reading direction per library, series
  or item.
- Reads in a native reader: fit page or fit width, two-page spreads with the cover alone, half-page
  mode with a 12 percent overlap, right to left, printed-page offset, progress, prefetch, keyboard.
- Searches the whole collection with FTS5, falling back to substring matching when a word is inside
  another word.
- Verifies integrity: BLAKE3 on demand, exact duplicates by size then hash, and checksum manifests that
  detect bitrot later.
- Shows what the collection is: items, pages, bytes, series, and which drive each library lives on,
  including drives that are currently unplugged.

The behavioural oracle for the grid and the reader is the browser prototype Sandra uses
(`heavymetal-site/magpie_proto.py`). `docs/READER_PARITY.md` lists every scenario and whether the Rust
app matches it.

## Feature matrix

Status is `Implemented`, `Partial` or `Planned`. Nothing is implied by omission.

### A. Inventory and catalogue

| Feature | Status | Notes |
|---|---|---|
| Library roots, multiple kinds, mature flag | Implemented | Mature libraries are hidden from the grid, from search and from the CLI until enabled |
| Incremental scan by path, size and mtime | Implemented | Unchanged archives are not reopened |
| CBZ / ZIP page listing and page count | Implemented | Natural order, `__MACOSX` and dotfiles skipped |
| Numbered image folders as volumes | Implemented | Leaf-most folder with at least N images (default 3, configurable) |
| CBR / RAR / CB7 / 7Z | Partial | Indexed with size and title; page reading is `not_implemented` until Tier 2 |
| PDF | Partial | Page count via `pdfcraft-cli info`; page rendering in the reader is Tier 2 |
| AVIF | Partial | Indexed; the decoder is deferred with the AVIF decision |
| ComicInfo.xml | Implemented | Series, number, year, page count, credits, right-to-left flag |
| Series grouping from folders and file names | Partial | Parsing with confidence; the review queue UI is Tier 2 |
| BLAKE3 on demand | Implemented | `hash`, or `media_health hash` |
| Exact duplicates | Implemented | Size first, then hash |
| Checksum manifests and verification | Implemented | Tab separated, written outside the library |
| FTS5 search | Implemented | Prefix tokens, diacritics folded, substring fallback |
| Offline drive catalogue | Partial | Per-volume item counts and online state; free space and growth need a Tier 3 pass |
| Technical inspection (ffprobe) | Implemented | Video and audio files; ffprobe is optional |
| Broken-file detection | Partial | A corrupt archive reports an unreadable page list; decode checks are Tier 3 |
| Statistics and reports | Partial | Sizes, pages, kinds, drives; decades, languages and gaps are Tier 3+ |
| Physical items (shelf catalogue) | Planned | Tier 5 |

### B to F (later tiers)

| Pillar | Status |
|---|---|
| B. Identify, tag, NFO and artwork, rename with plan and undo | Planned (Tier 2) |
| C. Near duplicates, backup planner, SMART | Planned (Tier 3) |
| D. Convert, subtitle generation, CD ripping, print scanning | Planned (Tier 3 and 5) |
| E. Knowledge graph and federated sources | Planned (Tier 4; the source registry exists) |
| F. Gaps, wishlist and the curated source guide | Planned (Tier 5) |

### Reader and interfaces

| Feature | Status |
|---|---|
| egui library grid with covers, breadcrumbs, folder filter | Implemented |
| Continue-reading row | Implemented |
| Whole-collection search box | Implemented |
| Reader: single page, spread with cover alone, half page, RTL, page jump, offset | Implemented |
| Reader: prefetch and decode off the UI thread | Implemented |
| Ctrl + wheel zoom, dark and light high-contrast themes | Implemented |
| CLI with `--json` | Implemented |
| MCP stdio server: `media_catalog`, `media_search`, `media_read`, `media_health` | Implemented |
| OPDS server, static site export | Planned (Tier 2/3) |
| Installer, .mcpb bundle, release assets | Planned (release tier) |

## Install

Requires Rust 1.82 or newer. No Python, no Node, no runtime services.

```powershell
git clone https://github.com/sandraschi/magpiecraft
cd magpiecraft
cargo build --release --workspace
```

Two binaries:

- `target\release\magpiecraft.exe` - the desktop app
- `target\release\magpiecraft-mcp.exe` - the CLI and the MCP stdio server

CLI first:

```powershell
magpiecraft-mcp library-add Comics "L:\Multimedia Files\Comics" --kind comics
magpiecraft-mcp scan
magpiecraft-mcp browse
magpiecraft-mcp search saga --limit 10
magpiecraft-mcp status --json
```

MCP client configuration (Claude Desktop or any stdio client):

```json
{
  "mcpServers": {
    "magpiecraft": {
      "command": "D:/Dev/repos/magpiecraft/target/release/magpiecraft-mcp.exe",
      "args": ["mcp"]
    }
  }
}
```

Optional external tools, detected at runtime and never linked: `ffprobe` (technical inspection),
7-Zip (CBR/RAR, Tier 2), `pdfcraft-cli` (PDF pages, Tier 2). Absence is reported in `status` and in
Health, never hidden.

Full instructions, including the data directory and environment variables: [INSTALL.md](INSTALL.md).

## Where your data lives

`%LOCALAPPDATA%\magpiecraft` (or `$XDG_DATA_HOME/magpiecraft`, or `MAGPIECRAFT_HOME`):

| Path | What |
|---|---|
| `catalogue.db` | Libraries, items, series, progress, FTS index |
| `settings.json` | Theme, zoom, mature visibility, page-folder threshold |
| `cache/covers` | Cover thumbnails, rendered from your files |
| `manifests` | Checksum manifests |

Nothing is ever written into a library. Covers, caches and manifests live here.

## Documentation

| File | Contents |
|---|---|
| [INSTALL.md](INSTALL.md) | Build, register, first library, external tools |
| [docs/TOOLS.md](docs/TOOLS.md) | Every CLI command and MCP tool with examples |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Crates, command bus, schema, scanning contract |
| [docs/READER_PARITY.md](docs/READER_PARITY.md) | Prototype scenarios and the reader's behaviour |
| [docs/BENCHMARKS.md](docs/BENCHMARKS.md) | The 100,000-file scan measurement |
| [docs/ATTRIBUTION.md](docs/ATTRIBUTION.md) | Provider terms, external tools, licences |
| [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) | Tests, fixtures, environment, style |
| [HELP.md](HELP.md) | User help: keys, what works, what does not |
| [HANDOVER.md](HANDOVER.md) | What the next agent must know |

## Quality gates

Measured 2026-11-24 on the Tier 1 commit:

- `cargo test --workspace`: **116 passed**, 0 failed
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean
  (`unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented` denied workspace-wide)
- `cargo fmt --all --check`: clean
- MCP stdio probe: `initialize` -> `tools/list` -> `tools/call` verified, stdout pure JSON-RPC
- Synthetic scan: 100,000 files in 478 ms (see `docs/BENCHMARKS.md`)

There is no `todo!()` or `unimplemented!()` reachable from any entry point. Unbuilt paths return
`not_implemented` with a reason.

## Licence and credits

MIT OR Apache-2.0. No copyrighted media is in this repository: every test fixture is generated in the
test. Third-party tools are invoked as subprocesses and never linked (see `docs/ATTRIBUTION.md`).
