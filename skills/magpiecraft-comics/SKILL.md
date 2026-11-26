---
name: magpiecraft-comics
description: Comic volumes: CBZ, CBR, PDF pages, ComicInfo, spreads and reading order.
---

# Comics in MagpieCraft

Comic volumes are archives (CBZ/ZIP), RAR archives (CBR, read through 7-Zip)
or PDFs (pages rendered through PdfCraft). The app links no archive or PDF
library; both tools are invoked, never linked.

## Pages and metadata

- `media_read` operation `volume`: page count, readable flag, first page names.
- ComicInfo.xml is read when present: series, volume number, writer and artist
  feed the `series` grouping in `media_catalog`.
- `media_read` operation `page_offset`: the difference between printed page
  numbers and scan order. `media_read` operation `page_export` writes one page
  to a file (for covers, never for ripping whole volumes).

## Reading order

- Two-page spreads leave the cover alone; `media_read` operation `rtl`
  switches a right-to-left volume. Half-page mode is a reader view concern and
  needs no tool call.
- Position is kept with `progress_get` and `progress_set`; `continue` in
  `media_catalog` surfaces the volume again on the next visit.

## Safety

CBR needs 7-Zip on PATH, PDF needs PdfCraft. `media_catalog` operation
`status` reports which external tools were found. Missing tools make pages
unreadable, never corrupt: the index still lists the volume.
