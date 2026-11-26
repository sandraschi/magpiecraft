---
name: magpiecraft-video
description: Films and series: playback handoff, subtitles, technical identity.
---

# Video in MagpieCraft

Video items are never decoded in the app. Playback is a handoff: VLC first
(`--start-time`), mpv second (`--start`), then the OS handler. The position
is recorded and resumes on the next launch.

## Identity and inspection

- `media_health` operation `inspect` runs ffprobe and reports container,
  codecs, resolution and duration. ffprobe missing means no identity, never a
  guess.
- `media_search` operation `search` finds titles; `media_catalog` operation
  `browse` walks season folders.

## Subtitles

All subtitle work goes through `media_transform` and only ever writes new
sidecar files:

- `subs`: list sidecars and embedded tracks for one item.
- `subs_shift`: move every cue by seconds (negative advances).
- `subs_convert`: SRT to WebVTT or back (`to: srt` or `vtt`).
- `subs_extract`: pull an embedded track out with ffmpeg (`stream` index,
  0 is the first).
- `subs_generate`: transcribe speech to SRT on this machine with whisperx or
  whisper (`model`, `lang`); takes minutes, needs the binary on PATH, never
  any cloud service.

## Conversion

`media_transform` operation `presets` lists the named ffmpeg recipes
(remux-mkv, mp4-compat, loudnorm, strip-metadata) and whether ffmpeg exists.
`convert_plan` builds the plan for one item; `media_curate` applies it, and
undo deletes the output. Sources are never edited.
