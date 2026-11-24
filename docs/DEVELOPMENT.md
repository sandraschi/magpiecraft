# Development

## Environment

- Rust 1.82 or newer, `cargo` on `PATH` (`$env:USERPROFILE\.cargo\bin`).
- Windows 11 is the primary target; the workspace is platform-neutral (std paths, no Win32 calls in
  the engine, `CREATE_NO_WINDOW` behind `cfg(windows)`).
- Optional: `ffprobe`, `7z`, `pdfcraft-cli` for the features that need them. Tests never require them.

Useful variables:

| Variable | Effect |
|---|---|
| `MAGPIECRAFT_HOME` | Catalogue, settings and caches live here instead of the default |
| `MAGPIECRAFT_FFPROBE` / `MAGPIECRAFT_7Z` / `MAGPIECRAFT_PDFCRAFT` | Explicit tool paths |
| `CARGO_TARGET_DIR` | Shared build directory, recommended across fleet Rust repos |

## Gates

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo run -q -p magpiecraft-app -- --smoke
```

Expected: 116 tests pass (2026-11-24), clippy clean with the deny gates, fmt clean.

Probe the MCP surface after any change to tools or the command bus:

```powershell
$exe = ".\target\debug\magpiecraft-mcp.exe"
@(
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}',
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}',
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"media_catalog","arguments":{"operation":"status"}}}'
) | & $exe mcp
```

Three responses, no log text on stdout.

## Tests

- Unit tests sit next to the code they cover (`#[cfg(test)] mod tests`), integration tests in
  `crates/magpiecraft-core/tests/`.
- Test modules opt out of the panic lints with an explicit attribute:
  `#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`. Production code never does.
- Fixtures are generated. PNG pages come from `image`, CBZ archives from `zip`, folders from `std::fs`.
  Never commit a real page or cover.
- Tests never touch a real library: they use `Paths::at(temp)` or `MAGPIECRAFT_HOME`.
- On Windows, drop the `Catalog` before deleting its temp directory, or SQLite keeps the file locked
  and `remove_dir_all` fails with error 32.
- The reader's behaviour is locked by tests in `magpiecraft-comics/src/reader.rs`; see
  `docs/READER_PARITY.md` for the mapping from prototype scenario to test.

## Adding a capability

1. Add a `Command` variant in `magpiecraft-core/src/command.rs` with a stable id.
2. Implement it on `Catalog` in `catalog.rs`, returning typed views from `model.rs`.
3. Add the arm to `run()` in `command.rs`.
4. Expose it in the CLI (`magpiecraft-mcp/src/main.rs`) and, if it is an agent-facing read, in
   `tool.rs` with an `operation` value and a schema.
5. Add the button or menu item in the app if it belongs there.
6. Test it in `catalog_tests.rs`, and update `docs/TOOLS.md` and the README matrix.

Never add a second path into the engine: the CLI, MCP and app must share the command.

## Style

- No `unwrap`, `expect`, `panic`, `todo!`, `unimplemented!` in production code.
- Treat all input as hostile: `get()` instead of indexing, `checked_*`/`saturating_*` for arithmetic,
  no byte slicing of strings.
- Errors carry a stable `error_type`. Messages are for humans and may change.
- UI strings are ASCII plus `° × · …`; the glyph test enforces it.
- Files stay under roughly 600 lines; split by concern rather than by line count.
- Comments explain why, not what. British spelling in prose, US spelling in identifiers.

## Known debt

| Item | Impact |
|---|---|
| `catalog.rs` is large (about 1,100 lines) | Split into `scan`, `browse`, `health` modules before it grows |
| Scan is single-threaded | Fine for 55 files; measure before a pool at Tier 2 |
| `Reader::step` half-page edge at the last page stays on the bottom half | Intentional; documented in the test |
| No `egui_kittest` snapshots | Tier 4; the reader maths are unit-tested instead |
| Covers are JPEG, not WebP | No WebP encoder in the stack; the difference is invisible at card size |
