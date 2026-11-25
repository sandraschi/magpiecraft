# MagpieCraft help

## What it is

A native app, a command line and an MCP server for the comics, manga and media files you already have
on disk. It indexes them, shows them as a library with covers, and reads them.

It does not download anything, does not bypass copy protection, and never changes a file in your
collection.

## First run

1. `File > Add library...` and point it at a folder. Name it, pick a kind (comics or manga matters: it
   sets the reading direction), and flag it mature if you want it hidden.
2. `File > Scan all libraries`. The status bar shows "scanning..." and the grid fills in.
3. Click a folder to walk into it, click a volume to read it.

## Keys

| Key | Action |
|---|---|
| Right / Left arrow | Page forward / back (swapped in right-to-left mode) |
| Space / Shift + Space | Page forward / back |
| Down / Up arrow | Page forward / back |
| H | Half-page mode: top half, then bottom half with 12 percent overlap |
| T | Two-page spread, with the cover alone on the first screen |
| R | Right to left for this item (remembered) |
| Home / End | First / last page |
| Escape | Back to the library |
| / | Jump to the search box |
| Ctrl + mouse wheel | Zoom the whole window, 60 to 250 percent |

## Toolbar

- **Prev / Next** page, with `7 / 220` or `8-9 / 220` between them.
- **Two pages**, **Half page**, **RTL**: the three reading modes. Turning one on turns the other off
  where they conflict.
- **Fit: page / width**: fit the whole page, or fill the width and clip the tall part. Use width when
  the lettering is small.
- **Offset - / +**: the difference between the printed page numbers and the order of the pages in the
  file. If printed page 1 is the fifth scan, set the offset to 4. It is remembered per item.
- **Printed page**: type a number and press Enter to jump there, offset applied.

## The library grid

- **Breadcrumbs** across the top: click any part to go back up.
- **Filter this folder** narrows the grid as you type, like the prototype.
- **Search whole collection** (or `/`) matches every word you type against titles, series and paths.
  Prefix search first, substring search if that finds nothing.
- **Continue reading** appears at the top when you have unfinished items.
- A folder card shows its first readable item as its cover and how many volumes it holds.

## Organise

`Window > Organise` shows three things:

- **Needs a human**: names the parser was unsure about, with the confidence. Rename the file or add a
  ComicInfo.xml and the entry clears on the next scan.
- **Plans waiting**: a saved plan with its full diff (old path, new path) and an Apply button. Nothing
  has changed yet. Applying runs the moves and logs them.
- **Applied plans**: every journal with an Undo button, until it has been undone once.

The same flow from the CLI: `plan-rename`, read the diff, `apply <id> --confirm`, and `undo <journal>
--confirm` if you do not like the result.

## What is not built yet

The app is honest about this rather than showing an empty page:

- CBR, RAR, CB7 and 7Z read through 7-Zip; PDF pages render through PdfCraft.
- AVIF images are indexed, and the decoder is a pending decision.
- Identify from metadata, NFO and artwork, and tag writing are the next Tier 2 remainder; renames,
  quarantine and verified backups already work as plans with diffs and undo.

## Settings

`Window > Settings` or `Edit > Settings`:

- Show mature-flagged libraries (off by default; they are also excluded from search while off).
- Images needed for a folder to count as a volume (default 3).
- Skin: Magpie Ink, Paper Library or Neon Shelf, each with a painted preview. Everything repaints
  at once: palettes, kind colours, card shadows and the backdrop.
- Zoom (or Ctrl + mouse wheel anywhere, 60 to 250 percent).

Storage paths are listed there too. Deleting the cache folder is always safe.

## Skins

`View > Skin`, the command palette ("Skin: ..."), `set-settings --skin <id>` or MCP
`media_catalog settings_set skin`: `magpie-ink` (high-contrast dark, the default), `paper-library`
(warm paper light) and `neon-shelf` (black glass with neon glow). The old `dark`/`light` theme names
still work everywhere and map to the default skin of that mode. Every skin is contrast-checked:
primary text 7:1, secondary text 4.5:1, on canvas and on cards.

## Where things live

`%LOCALAPPDATA%\magpiecraft` unless `MAGPIECRAFT_HOME` says otherwise: `catalogue.db`,
`settings.json`, `cache/covers`, `manifests`. Your libraries are never written to.

## Help menu

- Command Palette... - type a command, no mouse needed.
- Keyboard Shortcuts... - the table above.
- Project Files and Docs... - which document answers which question.
- Licenses and Notices... - licences of this app and of the tools it invokes.
- About MagpieCraft - version and where the catalogue is.

## Common problems

| Symptom | Cause and fix |
|---|---|
| Grid stays empty after Add library | You have not scanned yet: `File > Scan all libraries` |
| "no cover" on a card | The first page cannot be decoded: CBR/RAR, PDF, AVIF, or a damaged page |
| A library disappeared | It is flagged mature and mature content is off in Settings |
| A library shows as unreachable | The drive is unplugged. The index and your progress are intact |
| Text is small | Ctrl + wheel, or `View > Zoom 100%` |
| Scan seems stuck | It is single-threaded for now; a 100,000-file tree takes under a second to walk but first-time page counts open every archive once |
