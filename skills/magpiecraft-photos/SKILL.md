---
name: magpiecraft-photos
description: Photo folders: grid browsing, dates, handoff to viewers.
---

# Photos in MagpieCraft

Photo libraries are browsed as grids, grouped by folder and date. The app
edits nothing: no rotation writes, no metadata writes, no deletes.

## Browsing

- `media_catalog` operation `browse` walks year and event folders; the grid
  shows kind cards where no cover exists yet.
- `media_search` operation `search` matches folder and file names; dates in
  folder names order the result.
- Image folders with enough pictures are detected as page volumes for the
  reader path; ordinary photo folders stay plain folders.

## Viewing

`media_read` operation `open` hands a photo to the system viewer. Originals
are never modified; an edited copy is a new file the user makes in their own
editor.

## Safety

RAW formats the system cannot thumbnail still catalogue and launch. Missing
thumbnails are a display gap, reported honestly, never a reason to touch the
file.
