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

## Tier 3, second slice: near duplicates (2026-11-25)

`near-dupes` reports two different things and says which one it used:

- **Images** by perceptual hash: 64-bit dHash over a 9x8 grayscale, Hamming distance <= 10. A
  resized or recompressed copy matches; a mirrored picture does not. Capped at 5,000 photos.
- **Videos** by identity key: title, year, episode marker and a `Part`/`CD`/`Disc`/`Vol` suffix.

The first run of the real collection was wrong in two instructive ways, both now fixed and tested:
every episode of a series collapsed into one "duplicate" group (the episode marker was not part of
the key), and Shoah Part 1 grouped with Part 2. After the fix, 17,381 videos produce exactly one
possible duplicate: La Traviata 1982, kept twice with and without its cast note. Since then a second
false positive appeared (S02E00 vs S03E00, because episode 0 was rejected by the parser) and was
fixed in the same pass.

164 tests, clippy and fmt clean.

Remaining in Tier 3: the verified backup copy, ffmpeg presets, subtitle tools, SMART drive health.

## Tier 3, third slice: verified backup copy

- `backup-plan --to <root>`: only what the backup is missing; refuses an unmounted root or one inside
  the library; never overwrites a different size.
- Apply copies then hashes both sides with BLAKE3; a mismatched copy is deleted and reported.
  `--fast` = size check only. Undo removes copies, never the library.
- Live: 55 files / 993 MB copied and verified in 10 s, 0 failures; status "complete, 55 of 55 items";
  undo removed all 55.

166 tests. Remaining: ffmpeg presets, subtitle tools, SMART drive health.

## Tier 3, fifth slice: ffmpeg presets (2026-11-25)

- `presets` lists four named recipes with their exact ffmpeg args: `remux-mkv` (copy to Matroska,
  fast, lossless), `mp4-compat` (H.264 CRF 20 + AAC 192k, faststart), `loudnorm` (copy video, EBU R128
  to -16 LUFS), `strip-metadata` (drop every tag).
- `plan-convert <key> --preset <id>` plans one video or track. Refuses comics/books/photos, a missing
  source, an existing target and a missing ffmpeg. Output goes beside the source as
  `<stem>.<preset>.<ext>` so two presets never collide.
- Apply encodes with ffmpeg (invoked, never linked); the output must probe as media with a positive
  duration or it is deleted and counted as a failure. Undo (`Encode` op) deletes the output, never the
  source. No rescan, since a conversion changes nothing in the library.
- `media_transform` is now the sixth MCP tool: `presets`, `convert_plan`, `subs`, `subs_shift`,
  `subs_convert`, `subs_extract`.
- Live: La Traviata 1982.avi (1.4 GB) remuxed to MKV in 31 s, ffprobe 6617 s, then undone with the
  library byte-identical.

180 tests, clippy and fmt clean. Remaining in Tier 3: SMART drive health, local subtitle generation.

## Tier 3, sixth slice: skins (2026-11-25)

Sandra: "make it more beautiful. look at djcraft". Built the equivalent of djcraft's skin system,
fitted to a collector: three curated skins (Magpie Ink, Paper Library, Neon Shelf), per-kind accent
colours, card shadows, procedural backdrops and a reader progress bar.

- Skin is authoritative: one id in settings, the theme string follows it, old dark/light names map
  to the default skins. Picker in Settings with painted swatches, `View > Skin`, command palette,
  `--skin` flag, MCP `settings_set skin`.
- Contrast locked per skin in tests: primary 7:1, secondary 4.5:1 on canvas and card; kind accents
  distinct and readable on cards.
- Two real bugs found on camera: the backdrop painted the whole window and covered the menu bar
  (now scoped to the page rect), and eframe's default clear colour framed the light skin in black
  (now overridden from the palette).

184 tests, clippy and fmt clean. Remaining in Tier 3: SMART drive health, local subtitle generation
(Rust engine only).

## Tier 3, fourth slice: subtitles

- `subs`, `subs-shift`, `subs-convert`, `subs-extract`: sidecar discovery, signed cue shifting, SRT to
  WebVTT and back, and embedded-track extraction through ffmpeg (now part of the toolchain).
- Pure text, tolerant parsing (BOM, CRLF, VTT headers, NOTE, ids, settings), shift truncates at zero
  instead of stretching, ASS read-only with a reason, and every operation writes a new file rather than
  editing the original.
- A test file was briefly copied into the real collection to prove `subs`; it was removed at once and
  the directory verified. Recorded in the session log, because that rule binds me too.

173 tests. Remaining: ffmpeg presets, SMART drive health, local subtitle generation.

## Tier 3, later slices (2026-11-26)

- SMART drive health (`drive.health`, `drives`): Windows Storage API probe, per-library drive mapping, warnings for unhealthy/high-wear/hot/erroring/full volumes.
- Local subtitle generation (`subs.generate`): whisperx preferred, openai-whisper fallback, SRT sidecar beside the source; no model downloads, no cloud.
- Drive proposal scan (`propose_libraries`): one library per level-2 folder of Multimedia Files on each media drive, pattern kind guess plus sample files; unknown kinds stay unknown for the AI pass or manual correction.
- Chat dock, in-app LLM settings (provider/model/discovery/keys/test), and `magpie_llm_*` MCP tools.
- LLM classifier override (2026-11-27): `Settings.llm.classify_provider/classify_model`, empty follows the chat pair. Wired through `magpie_llm_select --for classify`, the MCP `slot` param, and a Settings "Classification" section showing the effective pair with a "Follow chat" reset. `classify_folder_ai` asks the effective model for strict JSON `{kind, confidence, reason}`; `AI-classify unknowns` fills proposal rows in place (user overrides win, 50/folder cap per run, cancel supported).

221 tests, clippy clean, fmt clean. Verified: CLI `llm use --for classify` + `llm status` round-trip (temp home for the write path), full workspace suite, screenshot of the Settings LLM card with Classification section.

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

## 2026-11-26: grid stagger and dialog keys

- **Stair-step cards fixed.** `horizontal_wrapped` centers each card (`Align::Center`),
  so any height difference staged the tops. The library grid is now row-major: columns
  from available width, one `horizontal` strip per row, every card a fixed
  `CARD_HEIGHT` that fits its content exactly. Verified with a screenshot, old Tier-1
  shot confirms the stagger predates the skins work.
- **Dialog keys.** Global shortcuts now use `egui_wants_keyboard_input()` instead of
  `memory.focused().is_some()`, so typing in any field (search, filter, add-library
  path) never triggers app keys, and clicking a button no longer silently disables
  `/` and Ctrl+F. Escape closes a dialog only when not typing. Enter submits the
  Add-library form.
- Gates: `cargo fmt --check`, clippy `-D warnings`, 184 workspace tests, all green.

## 2026-11-26 follow-up: the stair was a stale staged binary

- The grid fix above was correct (screenshot pixels: both card bodies start at the
  same y), but double-clicking `start.bat` still showed the stair. Root cause:
  `start.ps1` only rebuilt when the exes were missing, and session builds go to
  `D:\Dev\cargo-target`, so `target\release\magpiecraft.exe` was the 11/25 binary.
- `start.ps1` now rebuilds when any `crates/**/*.rs` or `Cargo.toml` is newer than
  the staged exe. Staged exe refreshed by hand this time; verified end to end with
  `start.ps1 -Smoke` (0.9.0, 17 libraries, ok).

## 2026-11-26: standard shell, slice 1 - libraries in Settings

- Settings opens with a Libraries card: every indexed folder with kind, item
  count, root path, per-library Scan, two-step Remove (confirm inline, index rows
  only, files untouched) and Add library. `GoSettings` now refreshes so counts
  are current. Verified by screenshot on the demo home.

## 2026-11-26: standard shell, slice 2 - Help to the 10-item suite

- Help menu now matches CRAFT_MENU_BAR_STANDARD: Command Palette, Keyboard
  Shortcuts, Getting Started, Usage, Tips and Tricks, Pitfalls and Safety,
  Planned Extensions, Bundled Skills, Project Files and Docs, Licenses and
  Notices, About.
- Help page is a tabbed Help Center (Get started default) with the seven
  standard content sections plus Skills (honest: none bundled yet, MCP surface
  named instead) and Project files. HELP.md rewritten to match. Stale Tier-2
  "not there yet" card is gone.

## 2026-11-26: standard shell, slice 3 - skins djcraft-style

- Six skins now: Forest Stacks (deep green dark), Sepia Desk (aged paper light)
  and Midnight Archive (deep blue dark) join the three. All six pass the locked
  contrast tests (7:1 primary, 4.5:1 secondary, distinct kind accents).
- Appearance tuning in Settings: zoom slider (75-150%), card border width
  (1.0-3.0pt) and backdrop pattern strength (0-200%, 0 paints solid). New
  Settings fields with defaults and clamps, so old settings files keep loading.
- Command palette and --skin help text list all six; core and app SkinId agree.

## 2026-11-26: standard shell, slice 4a - skills for every media type

- Ten skill texts under skills/ (workflow, comics, manga, magazines, video,
  audio, ebooks, photos, curate, verify). Each names only real MCP tools.
- Core skills.rs bundle: include_str!, frontmatter/description check, tool-ref
  check against KNOWN_TOOLS, 40 KB total cap. `Command::SkillRead` serves list
  and body through the normal dispatch bus.
- MCP: resources/list + resources/read (unknown URI is -32002),
  magpie_skill_read tool, core allowlist cross-test. CLI: skills list, show,
  export --to --force. Help Skills tab and HELP.md render the real bundle.

## 2026-11-26: standard shell, slice 4b - LLM core

- magpiecraft-core::llm: 9-provider table with aliases, shared
  ~/.craft/ai_config.json (one selection for every craft app), key resolution
  file-then-env, presence flags only, send-time guard (empty model or missing
  key refuses with a Settings pointer).
- Discovery: live Ollama tags+residents, live OpenAI-compat /v1/models with
  2s timeouts, curated lists for Anthropic and Gemini. Verified against
  Sandra's real Ollama (her models list) and her real shared config.
- Chat transport: sync curl, body and auth header through files, stream false,
  think false, num_ctx 16384 on Ollama only, cancel kills the child. Every
  failure is an error; empty replies never become invented text. Recorded-JSON
  parse tests per provider family.

## 2026-11-26: standard shell, slice 4c - LLM bus, MCP, CLI

- Command::LlmStatus/ListModels/Select/SetKey/Evict/Test through dispatch;
  select switches Ollama VRAM (evict+warm), set_key never echoes.
- Six magpie_llm_* MCP tools mirroring the CLI `llm` verbs one to one.
- Fleet lesson: past ~50 subcommands the debug CLI overflows the 1 MB main
  stack at startup (clap parse frames, no user code runs). .cargo/config.toml
  sets /STACK:8388608 for the Windows target. Release builds were unaffected.

## 2026-11-26: standard shell, slice 4d - LLM settings card (part 1)

- Settings has an LLM and chat card: nine provider selectables, model text
  plus Discover with clickable size/VRAM badges, endpoint fields, BYOK with
  presence dots and Forget, Test connection, tool/transcript toggles and chat
  instructions. Discovery and test run on worker threads; the UI never waits.
- Provider switches clear the model (no blind defaults). Agentic DE coop card
  with the MCP stdio command. start.ps1 gains -Mcp.

## 2026-11-26: Tier 3 SMART drive health

- Probe: Windows Storage API via an invoked powershell.exe (never linked),
  disks plus volumes with disk numbers; USB enclosures report null counters,
  rendered as unknown. smartctl is used when present (later slice).
- health::smart: parse + library-root to drive mapping + warnings (unhealthy
  status, >=90% wear, >=60C, nonzero error counters, <5% or <10 GB free with
  getting-full/nearly-full severity, unmapped roots). Tests on recorded live
  output.
- Command::DriveHealth, MCP media_health op drive_health, CLI `drives`,
  human-readable output, Stats page Drive health card with worker thread
  (Check drives now). Live run flagged Sandra's nearly-full volumes (F: and K:
  at ~200 MB).
- Staged exe refresh skipped: Sandra is running the app right now (PID from
  19:08); start.ps1 rebuilds on her next launch.

## 2026-11-26: Tier 3 local subtitle generation (whisperx/whisper)

- tools: whisperx-first detection (MAGPIECRAFT_WHISPER override), transcribe()
  writing <stem>.srt and renaming to the requested out; refusal to overwrite;
  status report + missing entries.
- Core Command::SubsGenerate (key, model?, lang?, out?) defaulting to
  <stem>.<lang>.srt beside the source; MCP op, CLI verb, human output.
- No model downloads, no cloud: missing binary is not_configured, never silent.
  whisperx.exe and whisper.exe both present on Goliath; live run left for real
  media (minutes per file).

## 2026-11-26: standard shell, slice 4d part 2 - chat dock

- Right dock (360px bounded, wrap, Send right of input), Ctrl+J, Window menu,
  palette entry, maximized-to-viewport mode. Header: provider pill (canonical
  id, opens Settings), context meter, View prompt, Tools toggle, Clear/Close.
- Worker thread per send with cancel; read-only tools run through dispatch
  (allowlist enforced by test); anything else refuses with a pointer, never a
  confirm card, because the chat exposes zero writes. Six tool rounds max.
- Preprompt: header rules, all ten skills, tool catalogue, live status,
  selection, user extra; capped parts truncate with a marker. Cloud banner
  when the provider is not local. Transcripts to chat/YYYY-MM-DD.jsonl.
- Verified: 3 chat unit tests (allowlist, truncation, dates), open-dock and
  closed screenshots, --smoke green.

## 2026-11-26: Tier 3 local subtitle generation (whisperx/whisper)

- tools::transcribe prefers whisperx (CUDA, word-level), falls back to openai-whisper; same flags, output <stem>.srt renamed to the requested name, refuses to overwrite. Missing binary is Missing, never silent.
- Command::SubsGenerate (key, model?, lang?, out?) defaults to <stem>.<lang>.srt beside the source. MCP op subs_generate, CLI verb, human output. No downloads, no cloud.
- Verified by mapping tests, invalid-key refusal, live binary presence on Goliath; no full transcription run yet (minutes per file, needs a real item).

## 2026-11-26: drive shell S1 - pattern proposal scan + Settings UI

- Convention locked: level-1 Multimedia Files, level-2 collections (Manga - Nihongo, Anime - Series, Music - Jazz, Video). Defaults stay empty; E:/F:/K:/L: is user config, not code.
- magpiecraft_drives::propose_libraries walks level-2 folders with kind guesses (prefix then contains rules) and 20 sample files; unknown stays unknown for the AI pass. Settings drive_roots + media_folder with sanitise, CLI and MCP surface, tests on recorded + live-built trees.
- Settings Media drives card: editable roots, Save, Scan drives on a worker, per-row kind picker with Add, Add all known. Confirming builds real libraries via LibraryAdd.
- Next: AI classification pass for unknowns (local model), left tree, right tabs, dashboard.
