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

## First scan of the real collection

The reference collection (`L:\Multimedia Files`, 13 libraries, 661 GiB), scanned once with the release
build on 2026-11-24:

| Library | Items | Pages | Size | First scan |
|---|---:|---:|---:|---:|
| Comics | 930 | 65,005 | 34.2 GiB | 243 s |
| Comics 3 | 1,111 | 13,881 | 49.3 GiB | 52 s |
| Comics - Superhero | 420 | 4,552 | 6.9 GiB | 18 s |
| Comics - French | 55 | 1,178 | 993 MiB | 4 s |
| Comics - German | 86 | 4,326 | 1.6 GiB | 36 s |
| Comics - SP & IT | 131 | 6,071 | 2.4 GiB | 56 s |
| Comics - Newspaper | 805 | 1,543 | 14.5 GiB | 9 s |
| Manga | 2,874 | 337,878 | 151.9 GiB | 996 s |
| Manga - Nihongo | 5,709 | 1,050,808 | 375.1 GiB | 2,661 s |
| Manga - TG | 393 | 29,823 | 6.2 GiB | 45 s |
| Manga - TG Nihongo | 181 | 31,284 | 11.7 GiB | 79 s |
| Manga - Sukebe (mature) | 54 | 8,057 | 6.0 GiB | 38 s |
| Comics - Porn (mature) | 12 | 549 | 313 MiB | 2 s |
| **Total** | **12,761** | **1,554,955** | **661 GiB** | **70.6 min** |

Where the time goes: the two manga libraries alone hold about 1.4 million loose page images in folders,
and every new item is opened once for its page list or its PDF page count. A second scan of the same
collection skips every unchanged item, so it is a tree walk plus one index lookup per volume.

Cover thumbnails are generated lazily, only for cards on screen, at three per frame.

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
