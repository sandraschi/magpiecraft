# MagpieCraft agent rules

Pure-Rust ArtCraft app: L1 core, L2 CLI + MCP, L3 egui. Standard:
`D:\Dev\repos\mcp-central-docs\standards\RUST_ARTCRAFT_STANDARD.md`.
Python, FastMCP, Starlette, webapp and port-claiming rules do not apply here.

## Line endings and characters

LF only, enforced by `.gitattributes` (`* text=auto eol=lf`). The Windows host has
`core.autocrlf` set, so tools that write files must emit LF. After bulk writes check `git ls-files --eol`
shows `w/lf`.

No em dash anywhere: code, docs, commit messages, chat. Use a hyphen.

## Layout

| Crate | Tier | Role |
|---|---|---|
| `magpiecraft-core` | L1 | Model, catalogue engine, command bus, cover rendering, image helpers |
| `magpiecraft-store` | L1 | SQLite schema, migrations, queries, FTS5, progress |
| `magpiecraft-scan` | L1 | Read-only walker: dirs, files, natural sort, page-folder detection |
| `magpiecraft-comics` | L1 | Containers, page sources, ComicInfo.xml, name parsing, reader layout maths |
| `magpiecraft-health` | L1 | BLAKE3, manifests, duplicate grouping |
| `magpiecraft-tools` | L1 | ffprobe, 7-Zip, PdfCraft discovery and subprocess calls |
| `magpiecraft-meta` | L1 | Provider facts, rate limiter, provenance (Tier 2 foundation) |
| `magpiecraft-sources` | L1 | Federated source registry (pillar E, transport not built) |
| `magpie-adapters` | shared | OPDS and Jellyfin read-only adapters; standalone on purpose (decision D2) |
| `magpiecraft-mcp` | L2 | CLI (`--json`) and MCP stdio server in one binary |
| `magpiecraft-app` | L3 | egui desktop app, calls the engine in process |

Dependency direction: `core` -> `store`, `scan`, `comics`, `health`, `tools`, `meta`, `sources`.
`mcp` and `app` -> `core`. `magpie-adapters` depends on nothing in this workspace so HomeServeCraft can
pin it as a git dependency: never add a `path = "../..."` dependency to it.

## Gates (all green before commit)

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Plus, for anything touching the surfaces: an MCP stdio probe (`initialize`, `tools/list`, one
`tools/call`) and `magpiecraft.exe --smoke`. Use `just ci` for the first three.

Workspace lints deny `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented`. Test modules opt
out explicitly with `#[allow(...)]`; production code does not. Every bug fix ships with a regression
test. Commit `Cargo.lock`. No `target/`, `*.exe`, `*.bak`, `data/` or `dist/` in `git status`.

Report test counts as measured from `cargo test --workspace`, never estimated.

## Product hard rules

- **No acquisition features of any kind.** No downloaders, torrent, Usenet, indexers, trackers, arr
  integration, stream rippers or "find missing episode" search. Gap reports may only link the curated
  legitimate-source allowlist.
- **No DRM circumvention.** Audio-CD ripping is allowed (Tier 5); decrypting DVD or Blu-ray is not.
  Never integrate, launch or automate Libation, OpenAudible or any DRM-removal tool.
- **Non-destructive.** Every rename, move, retag or delete is a plan, shown as a diff, applied on
  confirm, recorded in an undo journal; deletes go to quarantine. Tier 1 has no such feature yet, and
  `library-remove` only drops index rows.
- **Never write into a library.** Covers, caches, manifests and the database live under the app data
  home. Scanning is read-only. There is a test for this.
- **Mature libraries** are flagged, hidden by default, excluded from search and exports, and never used
  as screenshots.
- **Metadata** only from embedded data, the user's files, openly licensed dumps and official APIs
  under their terms: MusicBrainz at most 1 request per second with a real user agent, TMDB with
  attribution and per-user keys, no key shipped. Synthetic fixtures only, never copyrighted media.
- **External tools are invoked, never linked** (ffprobe, 7-Zip, PdfCraft). PDF pages come from PdfCraft
  (`pdfcraft-cli info` / `render`), not from a PDF library.
- **No stubs presented as features.** An unbuilt path returns `not_implemented` with a reason. The
  README matrix says Partial or Planned where that is the truth.

## Code rules

- `stdout` of `magpiecraft-mcp mcp` is 100% JSON-RPC. All logs go to `eprintln!`.
- Errors at the boundary are `{ok: false, error, error_type}`. `error_type` is stable; callers never
  parse the message.
- Treat arguments as hostile: validate, bound, no direct indexing of untrusted slices, no string
  slicing by byte index.
- One command bus: CLI and MCP both build `Command` variants and call `dispatch`. Do not add a second
  path to the engine.
- The reader's layout maths lives in `magpiecraft-comics::reader` with no UI types, so it stays
  testable. The egui layer only paints what the maths returns.
- SQL is parameterised. Migrations are additive; bump `SCHEMA_VERSION`.
- Files above roughly 600 lines should be split. `catalog.rs` is the current largest and is the one to
  watch.

## Git and coordination

- Single trunk: work on `main`. A short-lived branch only if merged and deleted the same session.
  Never leave agent branches.
- Claim the repo before touching it (`devcraft_claim`, or a claim file
  `mcp-central-docs/operations/session-log/craft-claim-<agent>-<date>-magpiecraft.md` when DevCraft is
  down). One agent per repo. Release the claim when done.
- Max 10 files per commit; 3 or more file edits need a timestamped `.bak` per file first.
- Never force-push, rewrite history, or delete branches, tags, refs or releases without Sandra's
  explicit go in the session log.
- End every work block with a `HANDOVER.md` update, a session-log entry in `mcp-central-docs`
  (`operations/session-log/craft-<agent>-<date>.md`) and a commit. Leave the repo resumable from those
  files alone.
- Releases: plain executables as assets, no zip-only. Below the test floor (T1 20, T2 50) or with a
  reachable `todo!()` the release is a pre-release. Versions stay below 1.0 until T2 plus a week of
  real use.

## Fixtures and tests

- Every fixture is generated in the test: PNG pages from `image`, CBZ archives from `zip`, image
  folders from `std::fs`. No copyrighted page is ever committed.
- Tests never hit the network. `magpie-adapters` takes a `Transport` so parsing is tested with canned
  response bodies.
- Tests use `MAGPIECRAFT_HOME` or `Paths::at(tmp)` so they never touch a real library or the real
  catalogue.
- On Windows, drop the `Catalog` before removing its temp directory: an open SQLite handle keeps the
  file locked.

## Content preservation

Applying a standard or template restructures; it never deletes. Never remove what the project is, what
it is based on, its status (what works and what does not) or warnings to users without Sandra asking in
the session. See `mcp-central-docs/standards/CONTENT_PRESERVATION_STANDARD.md`.
