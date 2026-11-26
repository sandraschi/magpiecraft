---
name: magpiecraft-ebooks
description: Ebooks and documents: catalogue entries, covers, external readers.
---

# Ebooks in MagpieCraft

Ebook items are catalogue entries with a cover, a page count when known, and
a launch into the reader the user already has. The app renders no document
pages itself.

## Catalogue

- `media_catalog` operation `browse` lists titles; `media_search` operation
  `item` returns the full record for one key.
- Covers come from embedded art or first-page rendering where the format
  allows; missing covers show a kind card, never a broken image.
- PDF documents with real pages belong to the comics-family reader path
  (`media_read` operation `volume`, pages through PdfCraft); reflowable
  ebooks launch externally.

## Launch

`media_read` operation `open` hands the file to the OS handler with the
reading position where supported. The catalogue records the position with
`progress_get` and `progress_set`.

## Safety

No DRM removal exists anywhere in this product. A protected file opens in
whatever reader the user authorised, or not at all.
