# Contributing to MagpieCraft

Read [AGENTS.md](AGENTS.md) first: it carries the rules this project is held to (no acquisition
features, no DRM circumvention, plan-first mutations, never write into a library, generated fixtures
only). A pull request that breaks one of them is rejected regardless of quality.

## Getting set up

```powershell
git clone https://github.com/sandraschi/magpiecraft
cd magpiecraft
cargo build --workspace
cargo test --workspace
```

Rust 1.82 or newer. No Python, Node, database server or service to install.

## Before you open a pull request

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
magpiecraft-mcp.exe status --json
magpiecraft.exe --smoke
```

All five must pass. The workspace denies `unwrap`, `expect`, `panic`, `todo` and `unimplemented` in
production code: test modules opt out with an explicit `#[allow(...)]` attribute, nothing else does.

## What a good change looks like

- One concern per commit, 10 files at most, message in the imperative ("add page offset memory").
- A new capability is one `Command` variant plus one arm in `dispatch`, and it is exposed by the CLI,
  MCP and the app from that single place. Do not add a second path into the engine.
- Unbuilt work returns `not_implemented` with a reason. Do not merge a stub that looks finished.
- A bug fix comes with a test that fails before it and passes after.
- Reader behaviour changes are argued against `docs/READER_PARITY.md`, because the browser prototype is
  what the user actually works with today.
- Never add a fixture that contains real comic pages, real cover art or anything copyrighted. Generate
  the image in the test.

## Areas that need help

| Area | What |
|---|---|
| CBR/RAR page reading | 7-Zip subprocess extraction behind the licence check in `docs/ATTRIBUTION.md` |
| PDF pages | Wire PdfCraft `render` into the reader's page source |
| AVIF | Pick a decoder that does not break the zero-runtime-dependency release |
| Series parsing | More filename patterns, with tests, without guessing silently |
| Performance | Cover prefetch on a bounded pool, first-paint time on 12,000-item libraries |

## Reporting a problem

Include: what you did, what you expected, what happened, and the output of
`magpiecraft-mcp status --json` (it contains no personal data beyond paths and counts, so trim the
paths if you prefer). Never attach a copyrighted page.

## Licence

Contributions are dual licensed MIT OR Apache-2.0, matching the project. By opening a pull request you
agree to that.
