# Reader parity with the prototype

The oracle is the browser prototype Sandra uses daily: `D:\Dev\repos\heavymetal-site\magpie_proto.py`
with `proto/browse.html` and `proto/read.html`, served at `http://127.0.0.1:8766/`. The Rust app must
match its behaviour, not its Python.

Each row is a scenario from the prototype. "Where" says which test locks it.

## Library grid

| # | Scenario | Expected | Status | Where |
|---|---|---|---|---|
| G1 | Root view | Breadcrumb "Libraries", one card per library with its kind | Matches | `catalog_tests::scan_indexes_archives_and_page_folders` |
| G2 | Folder cards | A folder card shows the first readable item below it as the cover and its volume count | Matches | `catalog_tests::covers_and_page_images_are_rendered_into_the_cache_only` |
| G3 | A page folder is an item, not a folder | A folder with N images appears under "Volumes and issues" | Matches | `catalog_tests::page_counts_come_from_the_archive_and_the_folder` |
| G4 | Breadcrumbs | Every path segment is clickable, the last is plain text | Matches | `BrowseView.crumbs` from `browse` |
| G5 | Folder filter | Typing narrows the grid by name, case-insensitively | Matches | `library::tests::filter_is_case_insensitive_and_trims` |
| G6 | Whole-collection search | Header box, Enter searches all libraries, shows library and folder | Matches | `catalog_tests::search_finds_words_across_series_and_paths` |
| G7 | Search is "every word must match" | Multi-word queries intersect | Matches | same |
| G8 | Search matches inside a word | The prototype uses substrings; we use FTS5 first, then substring | Matches (two modes) | same test, `search_like` |
| G9 | Continue reading row | Up to 12 unfinished items, most recent first | Matches | `catalog_tests::progress_roundtrip_and_continue_row` |
| G10 | Mature libraries hidden | Hidden unless `--mature` (prototype) / Settings (app) | Matches | `catalog_tests::mature_libraries_stay_hidden_until_enabled` |
| G11 | Empty folder | "Nothing readable here." | Matches | `library::show` empty branch |
| G12 | Folder cover for an empty folder | Placeholder box | Matches | `placeholder` in `library.rs` |

## Reader

| # | Scenario | Expected | Status | Where |
|---|---|---|---|---|
| R1 | Open an item | First page, or the saved page | Matches | `ReaderView::open`, `progress_roundtrip_and_continue_row` |
| R2 | Single page forward | Page + 1, clamped at the last page | Matches | `reader::tests::single_page_mode_steps_by_one` |
| R3 | Two-page mode | Cover alone, then spreads of two | Matches | `reader::tests::cover_is_alone_then_spreads_of_two` |
| R4 | Two-page stride | Page 1 -> 2 -> 4 -> 6 | Matches | same |
| R5 | Back from the first spread | Returns to the cover | Matches | `reader::tests::backward_from_the_first_spread_returns_to_the_cover` |
| R6 | Last spread | Clamps, never shows a page past the end | Matches | `reader::tests::last_spread_clamps_and_does_not_overrun` |
| R7 | Half page, forward | Top half, then bottom half, then next page top | Matches | `reader::tests::half_page_walks_top_then_bottom_then_next_page` |
| R8 | Half page, backward | Bottom half of the previous page | Matches | `reader::tests::half_page_backwards_goes_top_then_previous_bottom` |
| R9 | Half-page overlap | Top shows 0 to 56 percent, bottom 44 to 100 percent: 12 percent overlap | Matches | `reader::tests::half_page_crops_overlap_by_twelve_percent` |
| R10 | Right to left | Display order reversed, page numbers still ascending | Matches | `reader::tests::right_to_left_reverses_display_order_only` |
| R11 | RTL default for manga | Manga libraries read right to left without any toggle | Matches | `comicinfo_supplies_series_credits_and_direction` |
| R12 | RTL per item, remembered | Toolbar toggle or `R` writes the item override | Matches | `page_offset_and_item_direction_are_remembered` |
| R13 | RTL per series | Series override beats the library default | Matches | `series_direction_override_beats_the_library_default` |
| R14 | Printed-page offset | Printed 5 with offset 2 opens scan page 7, both directions | Matches | `reader::tests::printed_page_offset_maps_both_ways` |
| R15 | Offset remembered | Stored per item in the catalogue | Matches | `page_offset_and_item_direction_are_remembered` |
| R16 | Page jump | Type a page number, Enter, jump (offset applied) | Matches | `goto_printed` |
| R17 | Keyboard: arrows | Left/right page, swapped under RTL | Matches | `ReaderView::keyboard` |
| R18 | Keyboard: space and shift+space | Forward and back | Matches | same |
| R19 | Keyboard: H, T, R | Half page, two pages, RTL | Matches | same |
| R20 | Keyboard: Home, End | First and last page | Matches | same |
| R21 | Keyboard: Escape | Back to the library | Matches | `ReaderView::keyboard` returns the back action |
| R22 | Prefetch | Next three pages requested before they are shown | Matches | `reader::tests::prefetch_skips_what_is_on_screen` |
| R23 | Progress saved per page change | Closing and reopening resumes | Matches | `progress_roundtrip_and_continue_row` |
| R24 | Position label | "7 / 220", "8-9 / 220", "7 / 220 bottom" | Matches | `position_label` and its tests |
| R25 | Ctrl + wheel zoom | Zooms the whole window, persisted | Matches | `theme::handle_ctrl_wheel`, `Settings.zoom` |

## Deliberate differences

| # | Prototype | MagpieCraft | Why |
|---|---|---|---|
| D1 | Any folder with 2+ images is a volume, even if it has volume subfolders | A folder is a volume only if it has N images (default 3) and no volume subfolder | Stops a series folder with a few scans from being indexed on top of its own volumes; the threshold matches the PRD |
| D2 | Fits any container with `object-fit: contain` only | Adds "Fit: width" to fill the width and clip the height | Small lettering in scans is a real need; the default is still contain |
| D3 | Covers cached as WebP under `data/proto` | JPEG under the app data home | No WebP encoder in the blessed stack; the path is outside the collection either way |
| D4 | Search is a linear scan over an in-memory index | FTS5 first, `LIKE` fallback | Same results for the common case, faster at 100,000 items |
| D5 | Libraries are hardcoded in the Python file | Libraries are configured (`library-add`, `File > Add library...`) | The app ships to other machines |

## Gaps to close in Tier 2

| # | Gap |
|---|---|
| T2-1 | CBR/RAR page reading via the 7-Zip subprocess |
| T2-2 | PDF pages via `pdfcraft-cli render`, cached like other pages |
| T2-3 | Vertical drag when "Fit: width" overflows the window |
| T2-4 | Two-page spread for landscape pages that were scanned as one image |
