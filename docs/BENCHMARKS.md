# Benchmarks

Measured on Goliath (Ryzen 9 5900X, 64 GB RAM, NVMe), Windows 11, release build, 2026-11-24.
Reproduce with `just bench` or:

```powershell
magpiecraft-mcp bench-scan --files 100000 --json
```

## Synthetic scan: 100,000 files

The benchmark creates empty `.jpg` files in 500 page folders under the app data home, adds the tree
as a library, and scans it. Empty files isolate what is measured: the walker, the page-folder decision
and the SQLite writes. Image decoding is not part of a scan.

| Metric | Value |
|---|---|
| Files created | 100,000 in 15,172 ms (6,592 files/s, first run only) |
| Directories | 502 |
| Volumes indexed | 500 (200 pages each) |
| Scan wall time | **478 ms** |
| Throughput | **209,205 files/s** |
| Inserted items | 500 |
| Missing | 0 |

Second scan of the same tree: nothing is reopened (size and mtime match), so the cost is the walk plus
one row check per volume.

This is the number the PRD's "100,000-file synthetic scan benchmark" asks for.

## First scan of a real library

`L:\Multimedia Files\Comics - French`, 55 files (34 CBR, 15 PDF, 6 CBZ), 993 MiB:

| Metric | Value |
|---|---|
| Items | 55 |
| Pages | 1,178 |
| Directories | 13 |
| Scan time | 7,279 ms |

Why it is slower per file than the synthetic run: every new item is opened once. The six CBZ archives
are read for their page list and ComicInfo.xml; the fifteen PDFs ask `pdfcraft-cli info` for a page
count, and process start-up dominates that call. Re-scanning the same library skips all of them.

## Known costs and where they will go

| Cost | Today | Plan |
|---|---|---|
| PDF page count | one `pdfcraft-cli` process per new PDF | batch or cache by (path, size, mtime); Tier 2 |
| Cover rendering | one archive open and JPEG encode per cover, on a worker | already lazy and capped; add a bounded pool if first paint lags |
| Scanning | single thread | a bounded worker pool at Tier 2 if a 5,000-archive library is slow |
| Search | FTS5 prefix, `LIKE` fallback | unchanged; measure at 100,000 items first |

## How to re-measure

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cargo build --release -p magpiecraft-mcp
$env:MAGPIECRAFT_HOME = "<a scratch directory>"
.\target\release\magpiecraft-mcp.exe bench-scan --files 100000 --json
```

Nothing is written outside that scratch home. Delete it afterwards; it holds 100,000 empty files.
