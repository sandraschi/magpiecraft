---
name: magpiecraft-audio
description: Music and audiobooks: track order, playback handoff, conversion.
---

# Audio in MagpieCraft

Audio items are tracks and audiobooks. Like video, they are never decoded in
the app: the same VLC, mpv, OS-handler handoff plays them, and the position
resumes where it stopped.

## Order and identity

- Track order follows natural sort inside album folders; leading zeros are
  not required.
- `media_health` operation `inspect` reports codec, duration and bitrate via
  ffprobe for one file.
- `media_search` operation `search` matches artist, album and title words;
  every word must match.

## Conversion

- `media_transform` operation `presets` names the same ffmpeg recipes as
  video; loudness normalisation (`loudnorm`) fits compilations.
- `convert_plan` plus `media_curate` apply: plan first, confirm second, undo
  deletes the output. The source track is never rewritten.

## Safety

Ripping an audio CD you own is allowed. Decrypting a protected disc is not,
and no tool here does it. Anything that would need circumvention reports
`not_implemented` with a reason.
