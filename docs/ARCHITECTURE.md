# Architecture

## Shape

Pure-Rust ArtCraft three-tier workspace. One language, one lockfile, no web stack, no ports.

```text
magpiecraft/
  crates/
    magpiecraft-core      L1  model, catalogue engine, command bus, covers, image helpers
    magpiecraft-store     L1  SQLite schema, migrations, queries, FTS5
    magpiecraft-scan      L1  read-only walker, natural sort, page-folder detection
    magpiecraft-comics    L1  containers, page sources, ComicInfo, name parsing, reader maths
    magpiecraft-health    L1  BLAKE3, manifests, duplicate grouping
    magpiecraft-tools     L1  ffprobe, 7-Zip, PdfCraft subprocess bridge
    magpiecraft-meta      L1  provider facts, rate limiter, provenance
    magpiecraft-sources   L1  federated source registry
    magpie-adapters       shared  OPDS and Jellyfin read-only adapters (standalone)
    magpiecraft-mcp       L2  CLI with --json, and the MCP stdio server
    magpiecraft-app       L3  egui desktop app
```

Dependencies point one way: `core` depends on the eight L1 leaves; `mcp` and `app` depend on `core`.
`magpie-adapters` depends on nothing in this workspace so HomeServeCraft can pin it as a git dependency
(decision D2). Never give it a `path = "../..."` dependency.

## The command bus

Every capability is one `Command` variant:

```rust
pub enum Command {
    Status, Libraries, LibraryAdd { .. }, LibraryRemove { .. },
    Scan { library, full }, Browse { key }, Search { query, limit, mode },
    Item { key }, Pages { key }, PageOffset { .. }, Rtl { .. },
    ProgressGet { key }, ProgressSet { .. }, Continue { .. },
    Stats, Duplicates { .. }, Hash { key },
    ManifestWrite { .. }, ManifestVerify { .. }, Series { .. }, SeriesRtl { .. },
    Inspect { path }, Tools, Sources, Settings, SettingsSet { .. },
}
```

`dispatch(&mut Catalog, Command) -> serde_json::Value` runs it and encodes the outcome:

```json
{"ok": true,  "command": "browse", ...result fields...}
{"ok": false, "command": "browse", "error": "...", "error_type": "invalid"}
```

Object results are merged at the top level so a CLI user reads fields directly. A list result is wrapped
under `value`. `error_type` is stable (`invalid`, `not_found`, `not_implemented`, `confirm_required`,
`io`, `db`, `parse`, `archive`, `range`, `tool`, `not_configured`, `config`); callers never parse the
message.

The CLI, the MCP server and the desktop app all build these variants. The desktop app additionally
calls typed methods (`catalog.browse`, `catalog.pages`, ...) because it wants the structs, not JSON;
both go through the same `Catalog`, so they cannot drift.

## Item identity

```text
lib:<library id>|<path relative to the library root>
lib:3|Manga/Yotsuba/Yotsuba v05.cbz
lib:3                          (or lib:3|) the library root
```

Why not an absolute path: a drive letter changes, and progress must survive it. Why not a base64 blob:
keys appear in CLI commands and JSON, and a human needs to read them. The key is opaque to callers but
greppable in a terminal.

## Database

One SQLite file, WAL, additive migrations (`SCHEMA_VERSION` in `magpiecraft-store/src/schema.rs`).

| Table | Holds |
|---|---|
| `meta` | Schema version and small flags |
| `libraries` | Root, kind, mature flag, direction default |
| `dirs` | Every directory, and whether it is itself a volume |
| `series` | Series per folder (or per parsed name at the root), with a direction override |
| `items` | One row per volume or issue: path, container, size, mtime, page count, series, direction, page offset, hash, missing flag |
| `progress` | Last page, total, finished, title, timestamp, keyed by item key |
| `scans` | One row per scan: counts, duration, full flag |
| `items_fts` | FTS5 index over title, series, path, library (rowid = item id) |

Three deliberate choices:

- A missing file is marked, not deleted, so reading progress and overrides survive an unplugged drive.
  Browse and search exclude missing rows; statistics count them separately.
- Page lists are not stored. A page list is cheap to rebuild from the archive's central directory, and
  storing 20 million page names for 100,000 items would dwarf the catalogue.
- FTS5 for prefix search, `LIKE` for substring search. `mode: auto` tries FTS first and falls back, so
  typing `loustal` and typing `ousta` both find the volume.

## Scanning contract

```text
walk root read-only
  for each directory: rel path, parent, name, image count, subdirs
  for each file: rel path, name, extension, size, mtime
  a directory is a volume if: not the root, at least N images (default 3),
                              and none of its subdirectories is a volume
compare against the catalogue by (library id, rel path)
  new                     -> probe container (page count, ComicInfo), insert
  size or mtime changed   -> probe, update
  unchanged and known     -> skip entirely, no file opened
  in the catalogue, not seen -> mark missing
```

Guarantees:

- No write, rename or delete inside a library. There is an integration test asserting the cache path is
  outside the library root.
- A corrupt archive does not abort the scan; it is indexed and its page list reports the failure.
- A container this build cannot read (CBR/RAR, PDF pages, AVIF) is still indexed, and asking for pages
  returns `not_implemented` with the reason, never an empty list.

## Covers

Page one of the item, or of the first readable item below a folder. Rendered on a worker thread to a
JPEG in `cache/covers`, then decoded at card size and uploaded as a GPU texture. The UI thread never
opens an archive. At most three covers are uploaded per frame, textures are capped at 400, and a card
that fails gets a "no cover" label rather than an empty rectangle.

## The reader

Layout maths live in `magpiecraft-comics::reader` with no UI types:

- `slots()` returns what to draw, in screen order, with a `Crop` per slot.
- `step()` implements one screen forward or back, including the half-page top/bottom alternation and
  the cover-alone exception in two-page mode.
- `position_label()`, `printed_page()`, `goto_printed()`, `prefetch_pages()`.

The egui layer converts slots into textures and rectangles: it decodes on a worker thread at a bounded
width, then paints with a uv rectangle for half-page crops. Because the crop is a uv range, the 12
percent overlap costs nothing extra in memory.

## Why no ports

The desktop app calls the engine in process. The MCP server speaks stdio. There is no localhost HTTP
surface, so nothing has to be registered, firewalled or port-claimed. An opt-in OPDS listener is a Tier
2 decision and would be the only exception.

## Failure behaviour

- Opening the catalogue fails loudly; no silent fallback to memory.
- Settings are written atomically; a corrupt file falls back to defaults instead of blocking startup.
- External tools are optional. Missing ones produce `not_configured`, never a fake success.
- Any unbuilt path says `not_implemented` with a reason a user can act on.
