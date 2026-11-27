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
| `propose-libraries [roots...] [--media-folder name]` | Suggest one library per level-2 drive folder (pattern pass; AI fills unknowns from Settings) |
| `scan [library] [--full]` | Incremental scan; `--full` re-reads unchanged files |
| `browse [key]` | No key: the libraries overview. With a key: one folder |
| `search <words...> [--limit N] [--mode auto\|fts\|substring]` | Whole-collection search |
| `item <key>` | One item with page count, series, volume, direction, progress |
| `pages <key>` | Page count, readable flag, first page names, offset, direction |
| `page <key> <n> --out path` | Write one page to a file, byte for byte; never into the library |
| `open <key>` | Open a film, track, book or photo in the player or OS handler, resuming where it left off |
| `offset <key> <n>` | Set the printed-page offset (scan page minus printed page) |
| `rtl <key> [--on\|--off]` | Set or clear the reading direction of one item |
| `progress <key>` / `progress <key> --set N [--of N] [--finished]` | Read or write reading progress |
| `continue [--limit N]` | Recently read, unfinished items |
| `stats` | Counts, pages, bytes, per-library rows, drives and whether they are online |
| `dupes [library] [--limit N]` | Exact duplicates: size first, then BLAKE3 |
| `near-dupes [--limit N]` | Perceptual image groups and possible video duplicates (same title, year and episode) |
| `hash <key>` | BLAKE3 of one item, stored on the row |
| `manifest-write <library> [--out path]` | Write a checksum manifest (default under the data home) |
| `manifest-verify <library> [--manifest path]` | Re-hash and report missing, changed and new files |
| `verify [--every-days N]` | Which libraries are due a verification, and what the last run found |
| `verify-run [library] [--every-days N] [--force]` | Write the baseline on the first run, compare afterwards, record every run |
| `backup` | Where each library stands against the configured backup roots |
| `backup-roots [paths...] [--clear]` | Set or clear the backup roots |
| `backup-plan --to <root> [library] [--fast] [--limit N]` | Plan the copies a backup still needs: size check by default, `--fast` skips the checksum |
| `apply <plan-id> --confirm` | Applies a backup plan too: copies, then verifies each copy against its source |
| `undo <journal-id> --confirm` | For a backup journal, removes the copies and leaves the library alone |
| `series [library]` | Series with their direction overrides |
| `series-rtl <library> <series> [--on\|--off]` | Set the reading direction of a series |
| `inspect <path>` | ffprobe technical details for one media file |
| `plan-convert <key> --preset <id> [--out path]` | Plan a conversion; apply encodes with ffmpeg, undo deletes the output |
| `presets` | The four named ffmpeg recipes and whether ffmpeg is present |
| `subs <key>` | Subtitle sidecars (with cue counts) and embedded subtitle tracks |
| `subs-shift <path> --by <seconds> [--out file]` | Move every cue; writes a new file and never overwrites |
| `subs-convert <path> --to srt\|vtt [--out file]` | Convert between SRT and WebVTT |
| `subs-extract <key> --stream N --out file` | Pull an embedded subtitle track out with ffmpeg |
| `subs-generate <key> [--model name] [--lang en] [--out file]` | Transcribe speech to an SRT sidecar with whisperx/whisper on this machine; takes minutes |
| `drives` | Physical disks behind the libraries with health warnings |
| `llm status` | Redacted provider config: provider, model, endpoint, key presence |
| `llm list [--provider id]` | Live model list from the provider |
| `llm use <provider> <model> [--for chat\|classify]` | Persist the pair; `classify` writes the classifier override (default: chat) |
| `llm set-key <provider> --key ... [--clear]` | Store or forget one cloud key; never echoed |
| `review [library] [--limit N]` | Items whose filename parse was uncertain, with the confidence |
| `plan-rename [library] [--preset auto\|comics\|manga\|video] [--limit N]` | Build a rename plan. `auto` picks the preset per item kind; nothing changes on disk |
| `plan-quarantine <key>` | Build a plan that moves one item into quarantine (never deletes) |
| `plans` | Saved plans with their diff |
| `apply <plan-id> --confirm` | Apply a plan; refused without `--confirm` |
| `journals` | Applied plans and their outcome |
| `undo <journal-id> --confirm` | Reverse an applied plan, including reading progress |
| `settings` / `set-settings [--mature true] [--imgdir-min-pages N] [--skin neon-shelf] [--zoom 1.2] [--drive-roots E: F:] [--media-folder name]` | Read or change settings (`--theme dark|light` still works and maps to the default skin) |
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
go to stderr. Five compound tools, all read-first except the explicitly confirmed curate operations.

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
| `settings_set` | `mature_visible`, `imgdir_min_pages`, `skin`, `theme`, `zoom`, `drive_roots`, `media_folder` | Saved settings (skin wins over theme) |
| `propose_libraries` | `roots`, `media_folder` | Proposal list plus warnings; nothing is created |

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
| `page_export` | `key`, `page`, `out` | Writes one page to a file and returns its path and size |
| `open` | `key` | Launches the player or OS handler with the file and the resume position. The one read tool with a visible side effect: it starts a program on the user's own desktop |

### `media_transform`

Converting and subtitle work; everything here writes new files, never edits a source.

| Operation | Arguments | Returns |
|---|---|---|
| `presets` | - | The named ffmpeg recipes and whether ffmpeg is present |
| `convert_plan` | `key`, `preset`, `out` | One encode op; apply runs ffmpeg, undo deletes the output |
| `subs` | `key` | Sidecars with cue counts, and embedded subtitle tracks |
| `subs_shift` | `path`, `by`, `out` | Shift the cues by seconds into a new file |
| `subs_convert` | `path`, `to`, `out` | Convert SRT to WebVTT or back into a new file |
| `subs_extract` | `key`, `stream`, `out` | Extract an embedded subtitle track with ffmpeg |
| `subs_generate` | `key`, `model`, `lang`, `out` | Transcribe speech to an SRT sidecar with whisperx/whisper; takes minutes |

### `media_curate`

Plan first, apply on confirm, undo by journal. No operation deletes a file: a "delete" is a move into
quarantine.

| Operation | Arguments | Returns |
|---|---|---|
| `review` | `library`, `limit` | Items with an uncertain filename parse and their confidence |
| `plan_rename` | `library`, `preset` (`auto`/`comics`/`manga`/`video`), `limit` | Plan id, diff, skipped items. Nothing changes |
| `plan_quarantine` | `key` | Plan id that moves the item into quarantine |
| `plan_cleanup` | `library` | Plan of quarantines: empty folders, orphan sidecars, samples, junk |
| `plans` | - | Saved plans with their diff |
| `apply` | `id`, `confirm=true` | Applied and failed counts, journal id |
| `journals` | - | Applied plans, with `undone` state |
| `undo` | `id`, `confirm=true` | Restored count, problems |
An agent that calls `apply` or `undo` without `confirm=true` gets an `confirm_required` error and
nothing happens.

### `media_health`

| Operation | Arguments | Returns |
|---|---|---|
| `duplicates` | `library`, `limit` | Groups of identical files |
| `hash` | `key` | BLAKE3 and size |
| `manifest_write` | `library`, `out` | Manifest path, files, bytes |
| `manifest_verify` | `library`, `manifest` | Checked, ok, missing, changed, clean |
| `inspect` | `path` | ffprobe format and stream details |
| `verify_status` | `every_days` | Per-library due state and what the last run found |
| `verify_run` | `library`, `every_days`, `force` | Baseline or comparison, recorded per run |
| `backup_status` | - | Each library against the configured backup roots |
| `backup_plan` | `library`, `to`, `verify`, `limit` | A copy plan: what the backup still needs. `verify` checksums every copy on apply |
| `near_duplicates` | `limit` | Perceptual image groups and possible video duplicates |

Tool names are accepted with underscores (`media_catalog`) or dots (`media.catalog`), because some MCP
hosts validate tool names strictly. Results are JSON text in `content[0].text`; `isError` mirrors
`ok: false`.

Example call:

```json
{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{
  "name":"media_search","arguments":{"operation":"search","query":"yotsuba","limit":10}}}
```

### `magpie_llm_*` and `magpie_skill_read`

| Tool | What it does |
|---|---|
| `magpie_llm_status` | Redacted config: provider, model, endpoint, per-provider key presence |
| `magpie_llm_list_models` | Live model list for one provider (`provider` optional) |
| `magpie_llm_select` | Persist the pair; `slot: "classify"` writes the classifier override instead of the chat pair |
| `magpie_llm_set_key` / `clear: true` | Store or forget one cloud key; the key is never echoed |
| `magpie_llm_evict` | Evict resident Ollama models, optionally keeping one warm |
| `magpie_llm_test` | One live completion; `ok` only on a real reply |
| `magpie_skill_read` | Bundled skill texts (`name` optional); skills are also MCP resources under `skill://` |

## What requires confirmation

`apply` and `undo` (plan ids and journal ids), and `library_remove`. Progress, offset and direction
writes are index state, not files, and need no confirmation. There is no command that renames, moves or
deletes a file outside a plan.
