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

Ran the gates (117 tests, clippy clean with the deny lints, fmt clean), indexed the real collection,
measured the first and second scan, captured and reviewed the UI, fixed two layout bugs the screenshots
exposed, and pushed.

Measured on 2026-11-24 with the release build:

- Full scan of the reference collection (13 libraries, 661 GiB): **12,761 items, 1,554,955 pages,
  70.6 minutes**. Incremental rescan of the same tree: **156 s, 12,761 unchanged, nothing reopened**.
- Synthetic 100,000-file scan: **478 ms**.
- `cargo test --workspace`: **117 passed**, 0 failed.
- MCP stdio probe: initialize, tools/list, tools/call answered; a notification is not answered.
- `magpiecraft.exe --smoke` reports the library it opened.

Two bugs found by looking at the app rather than trusting it:

1. `App::ui` hands over an unbounded `Ui`, so `Panel::top/bottom` placed the status bar off-screen and
   `available_size()` reported about 14,285 points. The page area and status bar now take explicit rects
   from `ctx.content_rect()`, and the reader paints into a rect derived from the real window.
2. Card titles with two lines pushed their subtitles out of line. The title now reserves two lines.

Both are fixed, with the reader geometry now independent of layout-cursor arithmetic. Screenshots in
`docs/screenshots/` are from the synthetic demo library (`scripts/make-demo-library.ps1`), and
`scripts/capture-window.ps1` reproduces them.

## Next step

1. Verify the Tier 1 exit independently (see "Verification" below). Do not fix what you find here: log
   it, then hand it back, per the craft rule that nobody verifies their own work.
2. If the review passes, start A2 in this order: CBR/RAR page reading (7-Zip subprocess) and PDF pages
   (PdfCraft `render`, cached), then identify and tag for video/music/books/comics, then the rename plan
   with diff and undo journal, then `magpie-adapters` Jellyfin and OPDS read-only.

## Verification (for the reviewer)

```powershell
git clone https://github.com/sandraschi/magpiecraft
cd magpiecraft
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace                      # expect 116 passed
cargo run -q -p magpiecraft-app -- --smoke  # headless app wiring
magpiecraft-mcp.exe status --json
```

Then check the two behavioural claims against the prototype at `http://127.0.0.1:8766/`:

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
