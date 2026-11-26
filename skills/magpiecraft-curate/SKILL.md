---
name: magpiecraft-curate
description: Organise plans: review queue, renames, quarantine, journals, undo.
---

# Curating the collection

Everything that changes the collection goes through `media_curate` as a
plan. Nothing is applied without `confirm: true`, every apply writes a
journal, and every journal reverses with `undo`.

## Review first

`media_curate` operation `review` lists items whose filename parse was
uncertain. Curate the queue before planning: a plan built on a misparse
renames the wrong thing, and the journal will faithfully record it.

## Plans

- `plan_rename`: builds a rename plan (`preset` auto, comics, manga or
  video; `limit` 0 means all). `plans` lists saved plans.
- `apply`: applies one plan with `confirm: true`. Without the flag the call
  is refused.
- `journals` lists applied plans; `undo` reverses one with `confirm: true`.
- `plan_quarantine`: moves an item into quarantine. This is the delete: files
  are never removed.
- `plan_cleanup`: proposes empty folders, orphan sidecars, sample files and
  junk for quarantine.

## Backups

`media_health` operation `backup_plan` copies a library to a backup root,
checksumming every copy when asked. `backup_status` reports coverage. Redo a
failed backup rather than editing around it.
