---
name: magpiecraft-verify
description: Integrity: duplicates, manifests, verification schedule, missing files.
---

# Verifying the collection

All verify work goes through `media_health` and only ever reads the
collection, except `manifest_write` and `backup_plan`, which write new files
beside it or onto a backup root.

## Duplicates

- `duplicates`: exact duplicate groups by size, then BLAKE3.
- `near_duplicates`: perceptual groups (image similarity hash) for covers and
  photos that were re-encoded.
- Keep one copy, quarantine the rest with `media_curate` operation
  `plan_quarantine`. Never delete.

## Bitrot and schedule

- `hash`: BLAKE3 of one item. `manifest_write`: one checksum manifest per
  folder (default under the data home). `manifest_verify`: re-check it.
- `verify_status`: which libraries are due and what the last run found.
  `verify_run`: writes the baseline on the first run, compares afterwards.
  `force` verifies even when nothing is due; `every_days` sets the interval.

## Missing files

Unplugged drives and moved files show as missing, never as deleted: progress
and journals survive. Plug the drive back and scan again; `backup_status`
shows whether a second copy exists.
