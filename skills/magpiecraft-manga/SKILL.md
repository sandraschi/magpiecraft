---
name: magpiecraft-manga
description: Manga volumes: right-to-left defaults, series tracking, image folders.
---

# Manga in MagpieCraft

Manga is a first-class kind, not comics with a flag. Libraries of kind manga
default to right-to-left; the default can be forced per library at add time
(`library_add` argument `rtl`) or per item (`media_read` operation `rtl`).

## Volumes and folders

- Archives work exactly like comics (`media_read` operation `volume`).
- Loose image folders become volumes when they hold at least
  `imgdir_min_pages` page images (Settings, `media_catalog` operation
  `settings_set`). Folders below the threshold stay plain folders.
- Natural sort orders pages and volumes; leading zeros are not required.

## Series and progress

- `media_catalog` operation `series` groups by parsed series and volume
  number, so `Vol 12` follows `Vol 2`.
- Track position with `media_read` operations `progress_get` and
  `progress_set`; the Continue row and `media_catalog` operation `continue`
  pick the next unread volume.

## Safety

Same as comics: CBR through 7-Zip, PDF through PdfCraft, both invoked. The
reader reports `not_implemented` with a reason when a tool is missing instead
of pretending.
