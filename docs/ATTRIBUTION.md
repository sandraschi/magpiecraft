# Attribution, terms and licences

## This application

MIT OR Apache-2.0, at your choice. No copyrighted media is in this repository; every fixture is
generated inside a test.

## Metadata providers

No provider is called in Tier 1. The facts below are recorded now so Tier 2 cannot quietly break them.

| Provider | Terms and limits | Key | Where it is used |
|---|---|---|---|
| MusicBrainz | CC0 data. **At most one request per second** with a real `User-Agent` containing a contact URL. | No | Music releases, artists (Tier 2) |
| AcoustID | Fingerprint matching to MusicBrainz recordings. Per-user key. | Yes | Audio identification via `fpcalc` (Tier 2) |
| TMDB | Attribution required: "This product uses the TMDB API but is not endorsed or certified by TMDB." Per-user key, never shipped. | Yes | Film and TV (Tier 2) |
| TVDB / AniDB | Own terms; used only for series and anime where TMDB is weak. | Yes | Series and anime (Tier 2) |
| Open Library | CC0 metadata. Be polite, cache. | No | Books (Tier 2) |
| ComicVine | Own terms. Per-user key. | Yes | Comics issues and credits (Tier 2) |
| Grand Comics Database (GCD) | Open dump, downloaded by the user. | No | Comic contents and credits; preferred over any API |
| MangaBaka | Local SQLite dump. | No | Manga metadata; preferred over any API |
| LRCLIB | Opt-in lyrics provider, one request at a time, results cached as `.lrc`. | No | Lyrics (Tier 3) |
| OpenSubtitles | Opt-in with the user's own API key (decision D7). | Yes | Subtitles (Tier 3) |

Rules that follow from the table:

- Per-user keys only, stored in the OS credential store, never in the repository, never in a log.
- A per-provider rate limiter is mandatory (`magpiecraft-meta::Throttle`), and local dumps are preferred
  because they are free of load and of terms.
- Every result carries its provider and a link to the attribution line the UI must show.
- Subtitle and lyric downloads stay off until the user enables them.

## External tools (invoked, never linked)

| Tool | Licence | How it is used |
|---|---|---|
| ffprobe (FFmpeg project) | LGPL/GPL depending on build | Spawned for technical inspection; output parsed as JSON |
| 7-Zip | LGPL (unRAR restriction noted upstream) | Spawned to list or extract CBR/RAR (Tier 2) |
| PdfCraft (`pdfcraft-cli`) | Craft app by Brandon Thomas / Storytold | `info` for page count, `render` for pages (Tier 2) |

None of these are linked into MagpieCraft, so MagpieCraft stays MIT/Apache and each tool keeps its own
licence. Their absence is reported as `not_configured`, never as an error that blocks the app.

## RAR

CBR/RAR reading is not built. When it is, the decision from the licence report applies: shell out to
7-Zip (a separate program, licence-clean for us) unless a crate with an acceptable licence is approved.
No unrar source is vendored.

## Fonts and UI

egui ships Ubuntu-Light and its own icon font. The UI deliberately uses ASCII and vector shapes instead
of geometric Unicode, because the bundled font has no such glyphs and they would render as tofu boxes.
A test scans the UI sources to enforce that.

## Data the user brings

Reading progress, page offsets, direction overrides and manifests are the user's own data and stay in
the app data directory. Nothing is uploaded anywhere; there is no telemetry, no account and no cloud
component.
