# Tools: CLI and MCP reference

One binary does both: `magpiecraft-mcp.exe <command>` is the CLI, `magpiecraft-mcp.exe mcp` is the MCP
stdio server. Every CLI command accepts `--json` (global flag, before or after the subcommand) and
`--home <dir>`.

## CLI

| Command | What it does |
|---|---|
| `status` | Version, home, counts, external tool status |
| `tools` | ffprobe, 7-Zip, PdfCraft: found, path, role, what needs it |
| `sources` | Federated source registry (all remote sources are `not_implemented` for now) |
| `libraries` | Configured libraries with id, kind and root |
| `library-add <name> <root> --kind <kind> [--mature] [--rtl\|--ltr]` | Add a library. The root must exist |
| `library-remove <name> --confirm` | Drop a library's index rows. Files are never touched |
| `scan [library] [--full]` | Incremental scan; `--full` re-reads unchanged files |
| `browse [key]` | No key: the libraries overview. With a key: one folder |
| `search <words...> [--limit N] [--mode auto\|fts\|substring]` | Whole-collection search |
| `item <key>` | One item with page count, series, volume, direction, progress |
| `pages <key>` | Page count, readable flag, first page names, offset, direction |
| `offset <key> <n>` | Set the printed-page offset (scan page minus printed page) |
| `rtl <key> [--on\|--off]` | Set or clear the reading direction of one item |
| `progress <key>` / `progress <key> --set N [--of N] [--finished]` | Read or write reading progress |
| `continue [--limit N]` | Recently read, unfinished items |
| `stats` | Counts, pages, bytes, per-library rows, drives and whether they are online |
| `dupes [library] [--limit N]` | Exact duplicates: size first, then BLAKE3 |
| `hash <key>` | BLAKE3 of one item, stored on the row |
| `manifest-write <library> [--out path]` | Write a checksum manifest (default under the data home) |
| `manifest-verify <library> [--manifest path]` | Re-hash and report missing, changed and new files |
| `series [library]` | Series with their direction overrides |
| `series-rtl <library> <series> [--on\|--off]` | Set the reading direction of a series |
| `inspect <path>` | ffprobe technical details for one media file |
| `settings` / `set-settings [--mature true] [--imgdir-min-pages N] [--theme dark] [--zoom 1.2]` | Read or change settings |
| `bench-scan [--files N] [--dir path]` | Synthetic scan benchmark, writes only under the data home |
| `mcp` | MCP stdio server |

Exit codes: `0` success, `1` the command reported an error, `2` the arguments did not parse.

Examples:

```powershell
magpiecraft-mcp library-add French "L:\Multimedia Files\Comics - French" --kind comics
magpiecraft-mcp scan French
magpiecraft-mcp browse "lib:1|Tardi"
magpiecraft-mcp pages "lib:1|Loustal/Loustal v01 Jolie Mer De Chine.cbz"
magpiecraft-mcp progress "lib:1|Loustal/Loustal v01 Jolie Mer De Chine.cbz" --set 5
magpiecraft-mcp search tardi --limit 5 --json
magpiecraft-mcp manifest-write French
magpiecraft-mcp manifest-verify French
magpiecraft-mcp inspect "D:\video\example.mkv"
```

## MCP

Line-delimited JSON-RPC 2.0 over stdio, MCP `2024-11-05`. stdout carries protocol messages only; logs
go to stderr. Four compound tools, all read-first.

### `media_catalog`

| Operation | Arguments | Returns |
|---|---|---|
| `status` | - | Version, home, counts, tool status |
| `tools` | - | The three optional binaries |
| `sources` | - | Source registry |
| `libraries` | - | Library list |
| `browse` | `key` (optional) | Crumbs, folders, items |
| `stats` | - | Totals, per library, drives |
| `scan` | `library` (optional), `full` | Per-library added/changed/missing/ms |
| `series` | `library` (optional) | Series rows |
| `continue` | `limit` | Recently read items |
| `library_add` | `name`, `root`, `kind`, `mature`, `rtl` | The new library |
| `library_remove` | `name`, `confirm=true` | Rows dropped; files untouched |
| `settings` | - | Current settings |
| `settings_set` | `mature_visible`, `imgdir_min_pages`, `theme`, `zoom` | Saved settings |

### `media_search`

| Operation | Arguments | Returns |
|---|---|---|
| `search` | `query`, `limit`, `mode` | Hits with key, title, library, folder |
| `item` | `key` | Full item record |

### `media_read`

| Operation | Arguments | Returns |
|---|---|---|
| `volume` | `key` | Page count, readable flag, reason, first pages, offset, direction |
| `page_offset` | `key`, `offset` | Stored offset |
| `rtl` | `key`, `rtl` (true/false/null) | Stored direction |
| `progress_get` | `key` | Last page and total |
| `progress_set` | `key`, `page`, `total`, `finished` | Saved progress |

### `media_health`

| Operation | Arguments | Returns |
|---|---|---|
| `duplicates` | `library`, `limit` | Groups of identical files |
| `hash` | `key` | BLAKE3 and size |
| `manifest_write` | `library`, `out` | Manifest path, files, bytes |
| `manifest_verify` | `library`, `manifest` | Checked, ok, missing, changed, clean |
| `inspect` | `path` | ffprobe format and stream details |

Tool names are accepted with underscores (`media_catalog`) or dots (`media.catalog`), because some MCP
hosts validate tool names strictly. Results are JSON text in `content[0].text`; `isError` mirrors
`ok: false`.

Example call:

```json
{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{
  "name":"media_search","arguments":{"operation":"search","query":"yotsuba","limit":10}}}
```

## What requires confirmation

Only `library_remove` (with `confirm=true`) and the progress/offset/direction writes, all of which are
index state, never files. There is no command in Tier 1 that renames, moves or deletes a file.
