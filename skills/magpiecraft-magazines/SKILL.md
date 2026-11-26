---
name: magpiecraft-magazines
description: Magazine issues: dates, numbers, loose-issue folders and archiving.
---

# Magazines in MagpieCraft

Magazine libraries hold issues, not volumes. Filenames carry a date or an
issue number; the parser feeds the same `series` grouping as comics, so a
run of issues orders itself.

## Issues

- Archives and PDFs open like comic volumes (`media_read` operation
  `volume`); loose-issue image folders follow the `imgdir_min_pages` rule.
- `media_read` operation `page_offset` aligns printed issue pages with scan
  order for exports.
- Dates beat numbers when both parse: an issue dated 2024-03 sorts after a
  bare number from the same run.

## Archiving

- Full runs belong in `media_health` operation `manifest_write`: one checksum
  manifest per run, verified later with `manifest_verify` for bitrot.
- Back up finished runs with `media_health` operation `backup_plan` to a
  backup root; `backup_status` reports what is covered.

## Safety

Magazines are often mature-flagged at the library level (`library_add`
argument `mature`); flagged libraries stay out of the grid, search and
exports until Settings shows them.
