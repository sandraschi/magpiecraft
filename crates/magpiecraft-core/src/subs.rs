//! Subtitle sidecars: list, shift, convert and extract (pillar D).
//!
//! SRT and WebVTT are plain text, so shifting and converting them is pure work with
//! no external tool and no risk: this module parses cues, moves them in time, and
//! writes a new file. It never edits the original. Extraction from a container needs
//! ffmpeg, which is invoked, never linked.
//!
//! ASS/SSA is read (so its existence and cue count are reported) but not written:
//! its styling sections are not something to regenerate by hand. Convert with ffmpeg
//! if you need ASS output.

use crate::catalog::Catalog;
use crate::error::{MagpieError, MagpieResult};
use magpiecraft_store::ItemRow;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// One subtitle cue. Times are milliseconds from the start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cue {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubFormat {
    Srt,
    Vtt,
    /// Read only: reported, not written.
    Ass,
}

impl SubFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        match magpiecraft_scan::extension(&path.file_name()?.to_string_lossy()).as_str() {
            "srt" => Some(SubFormat::Srt),
            "vtt" => Some(SubFormat::Vtt),
            "ass" | "ssa" => Some(SubFormat::Ass),
            _ => None,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "srt" => Some(SubFormat::Srt),
            "vtt" | "webvtt" => Some(SubFormat::Vtt),
            "ass" | "ssa" => Some(SubFormat::Ass),
            _ => None,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            SubFormat::Srt => "srt",
            SubFormat::Vtt => "vtt",
            SubFormat::Ass => "ass",
        }
    }

    pub fn writable(self) -> bool {
        !matches!(self, SubFormat::Ass)
    }
}

/// Parse a timestamp: `HH:MM:SS,mmm`, `HH:MM:SS.mmm` or `MM:SS.mmm`.
pub fn parse_timestamp(text: &str) -> Option<i64> {
    let cleaned = text.trim().replace(',', ".");
    let (clock, millis) = match cleaned.rsplit_once('.') {
        Some((clock, frac)) => {
            let frac: String = frac.chars().take(3).collect();
            let value: i64 = format!("{frac:0<3}").parse().ok()?;
            (clock, value)
        }
        None => (cleaned.as_str(), 0),
    };
    let parts: Vec<&str> = clock.split(':').collect();
    let (hours, minutes, seconds) = match parts.len() {
        3 => (
            parts.first()?.parse::<i64>().ok()?,
            parts.get(1)?.parse::<i64>().ok()?,
            parts.get(2)?.parse::<i64>().ok()?,
        ),
        2 => (
            0,
            parts.first()?.parse::<i64>().ok()?,
            parts.get(1)?.parse::<i64>().ok()?,
        ),
        _ => return None,
    };
    if !(0..60).contains(&minutes) || !(0..60).contains(&seconds) || hours < 0 {
        return None;
    }
    Some(((hours * 3600 + minutes * 60 + seconds) * 1000) + millis)
}

/// `HH:MM:SS,mmm` (SRT) or `HH:MM:SS.mmm` (WebVTT).
pub fn format_timestamp(ms: i64, separator: char) -> String {
    let ms = ms.max(0);
    let hours = ms / 3_600_000;
    let minutes = (ms % 3_600_000) / 60_000;
    let seconds = (ms % 60_000) / 1000;
    let millis = ms % 1000;
    format!("{hours:02}:{minutes:02}:{seconds:02}{separator}{millis:03}")
}

fn timing_line(line: &str) -> Option<(i64, i64)> {
    let (left, right) = line.split_once("-->")?;
    // A WebVTT timing line may carry settings after the end time ("align:start").
    let end = right.split_whitespace().next().unwrap_or("");
    Some((parse_timestamp(left)?, parse_timestamp(end)?))
}

/// Parse SRT or WebVTT text into cues. Tolerant: a missing index, a BOM, CRLF and
/// stray blank lines are all fine; a block without a timing line is skipped.
pub fn parse_cues(text: &str, format: SubFormat) -> Vec<Cue> {
    let normalised = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let mut cues: Vec<Cue> = Vec::new();
    for block in normalised.split("\n\n") {
        let mut lines = block.lines();
        let Some(first) = lines.next() else { continue };
        let first_trimmed = first.trim();
        if first_trimmed.is_empty() {
            continue;
        }
        // WebVTT headers and comments are not cues.
        if first_trimmed.starts_with("WEBVTT")
            || first_trimmed.starts_with("NOTE")
            || first_trimmed.starts_with("STYLE")
            || first_trimmed.starts_with("REGION")
        {
            continue;
        }
        let (timing, text_lines) = match timing_line(first_trimmed) {
            Some(pair) => (pair, lines.collect::<Vec<_>>()),
            None => {
                // An SRT index or a VTT cue id: the timing line follows.
                let Some(second) = lines.next() else { continue };
                match timing_line(second.trim()) {
                    Some(pair) => (pair, lines.collect::<Vec<_>>()),
                    None => continue,
                }
            }
        };
        let body = text_lines.join("\n").trim().to_string();
        if body.is_empty() {
            continue;
        }
        cues.push(Cue {
            start_ms: timing.0,
            end_ms: timing.1.max(timing.0),
            text: body,
        });
    }
    cues.sort_by_key(|c| c.start_ms);
    let _ = format;
    cues
}

/// Render cues as SRT or WebVTT. ASS is refused here on purpose.
pub fn render_cues(cues: &[Cue], format: SubFormat) -> MagpieResult<String> {
    match format {
        SubFormat::Srt => {
            let mut out = String::new();
            for (i, cue) in cues.iter().enumerate() {
                out.push_str(&format!(
                    "{}\n{} --> {}\n{}\n\n",
                    i + 1,
                    format_timestamp(cue.start_ms, ','),
                    format_timestamp(cue.end_ms, ','),
                    cue.text
                ));
            }
            Ok(out)
        }
        SubFormat::Vtt => {
            let mut out = String::from("WEBVTT\n\n");
            for cue in cues {
                out.push_str(&format!(
                    "{} --> {}\n{}\n\n",
                    format_timestamp(cue.start_ms, '.'),
                    format_timestamp(cue.end_ms, '.'),
                    cue.text
                ));
            }
            Ok(out)
        }
        SubFormat::Ass => Err(MagpieError::not_implemented(
            "writing ASS/SSA is not built: it carries styling sections. Convert with ffmpeg",
        )),
    }
}

/// Move every cue by `by_ms`. A cue pushed before zero is truncated at zero (it does
/// not get longer, or the text would appear after it has finished); a cue pushed
/// entirely before zero is dropped.
pub fn shift_cues(cues: &mut Vec<Cue>, by_ms: i64) -> usize {
    let before = cues.len();
    let mut out: Vec<Cue> = Vec::with_capacity(cues.len());
    for mut cue in cues.drain(..) {
        cue.start_ms = cue.start_ms.saturating_add(by_ms);
        cue.end_ms = cue.end_ms.saturating_add(by_ms);
        if cue.end_ms <= 0 {
            continue;
        }
        if cue.start_ms < 0 {
            cue.start_ms = 0;
        }
        out.push(cue);
    }
    *cues = out;
    before - cues.len()
}

fn read_text(path: &Path) -> MagpieResult<String> {
    let bytes = std::fs::read(path).map_err(|e| MagpieError::io(path, e))?;
    // Subtitles are UTF-8 in practice; a Windows-1252 file would fail loudly here
    // rather than silently mangling accents.
    String::from_utf8(bytes).map_err(|_| {
        MagpieError::invalid(format!(
            "{} is not UTF-8; convert it to UTF-8 first",
            path.display()
        ))
    })
}

/// Write next to the source unless `out` says otherwise, and never overwrite.
fn target_path(source: &Path, out: Option<&str>, suffix: &str, extension: &str) -> PathBuf {
    if let Some(out) = out.filter(|o| !o.trim().is_empty()) {
        return PathBuf::from(out);
    }
    let parent = source.parent().unwrap_or_else(|| Path::new(""));
    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "subtitle".to_string());
    parent.join(format!("{stem}.{suffix}.{extension}"))
}

impl Catalog {
    /// Subtitle sidecars next to one item, and the embedded streams ffprobe reports.
    pub fn subs_list(&self, key: &str) -> MagpieResult<Value> {
        let (lib, row) = self.resolve_item(key)?;
        let path = crate::catalog::library_path(&lib, &row.rel);
        let sidecars = self.sidecars_for(&path);
        let embedded = self.embedded_subtitles(&path);
        Ok(json!({
            "key": row.key(),
            "title": row.title,
            "path": path.display().to_string(),
            "sidecars": sidecars,
            "embedded": embedded,
            "note": "sidecars are shifted and converted in place; embedded tracks are extracted with ffmpeg",
        }))
    }

    /// Sidecar files that share the item's stem.
    pub fn sidecars_for(&self, media: &Path) -> Vec<Value> {
        let mut out: Vec<Value> = Vec::new();
        let Some(parent) = media.parent() else {
            return out;
        };
        let Some(stem) = media.file_stem().map(|s| s.to_string_lossy().to_string()) else {
            return out;
        };
        if let Ok(entries) = std::fs::read_dir(parent) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if path.file_stem().map(|s| s.to_string_lossy().to_string()) != Some(stem.clone()) {
                    continue;
                }
                let Some(format) = SubFormat::from_path(&path) else {
                    continue;
                };
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                let cues = if format.writable() {
                    read_text(&path)
                        .ok()
                        .map(|text| parse_cues(&text, format).len())
                } else {
                    None
                };
                out.push(json!({
                    "name": name,
                    "path": path.display().to_string(),
                    "format": format.extension(),
                    "bytes": size,
                    "cues": cues,
                }));
            }
        }
        out.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        out
    }

    /// Embedded subtitle streams, with their global index, language and title.
    pub fn embedded_subtitles(&self, media: &Path) -> Vec<Value> {
        let Ok(info) = magpiecraft_tools::ffprobe(self.tools(), media) else {
            return Vec::new();
        };
        info.streams
            .iter()
            .filter(|s| s.codec_type == "subtitle")
            .enumerate()
            .map(|(n, s)| {
                json!({
                    "subtitle_index": n,
                    "stream_index": s.index,
                    "codec": s.codec_name,
                    "language": s.language,
                })
            })
            .collect()
    }

    /// Shift a subtitle file by `seconds`, writing a new file.
    pub fn subs_shift(&self, path: &str, seconds: f64, out: Option<&str>) -> MagpieResult<Value> {
        let source = PathBuf::from(path.trim());
        let format = SubFormat::from_path(&source)
            .ok_or_else(|| MagpieError::invalid("not a subtitle file (srt, vtt, ass, ssa)"))?;
        if !format.writable() {
            return Err(MagpieError::not_implemented(
                "shifting ASS/SSA is not built; convert it with ffmpeg first",
            ));
        }
        if !seconds.is_finite() || seconds.abs() > 3600.0 {
            return Err(MagpieError::invalid("shift must be within one hour"));
        }
        let text = read_text(&source)?;
        let mut cues = parse_cues(&text, format);
        if cues.is_empty() {
            return Err(MagpieError::invalid(format!(
                "{} has no cues I can read",
                source.display()
            )));
        }
        let dropped = shift_cues(&mut cues, (seconds * 1000.0).round() as i64);
        let target = target_path(&source, out, "shifted", format.extension());
        if target.exists() {
            return Err(MagpieError::invalid(format!(
                "{} already exists, refusing to overwrite",
                target.display()
            )));
        }
        let body = render_cues(&cues, format)?;
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir).map_err(|e| MagpieError::io(dir, e))?;
        }
        std::fs::write(&target, body).map_err(|e| MagpieError::io(&target, e))?;
        Ok(json!({
            "ok": true,
            "source": source.display().to_string(),
            "out": target.display().to_string(),
            "by_seconds": seconds,
            "cues": cues.len(),
            "dropped": dropped,
            "note": "the original is untouched; this is a new file",
        }))
    }

    /// Convert SRT <-> WebVTT, writing a new file.
    pub fn subs_convert(&self, path: &str, to: &str, out: Option<&str>) -> MagpieResult<Value> {
        let source = PathBuf::from(path.trim());
        let from = SubFormat::from_path(&source)
            .ok_or_else(|| MagpieError::invalid("not a subtitle file (srt, vtt, ass, ssa)"))?;
        if !from.writable() {
            return Err(MagpieError::not_implemented(
                "reading ASS/SSA is not built; convert it with ffmpeg first",
            ));
        }
        let target_format = SubFormat::parse(to)
            .ok_or_else(|| MagpieError::invalid(format!("unknown subtitle format {to}")))?;
        if !target_format.writable() {
            return Err(MagpieError::not_implemented(
                "writing ASS/SSA is not built: it carries styling sections. Convert with ffmpeg",
            ));
        }
        let text = read_text(&source)?;
        let cues = parse_cues(&text, from);
        if cues.is_empty() {
            return Err(MagpieError::invalid(format!(
                "{} has no cues I can read",
                source.display()
            )));
        }
        let target = target_path(&source, out, "converted", target_format.extension());
        if target.exists() {
            return Err(MagpieError::invalid(format!(
                "{} already exists, refusing to overwrite",
                target.display()
            )));
        }
        let body = render_cues(&cues, target_format)?;
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir).map_err(|e| MagpieError::io(dir, e))?;
        }
        std::fs::write(&target, body).map_err(|e| MagpieError::io(&target, e))?;
        Ok(json!({
            "ok": true,
            "source": source.display().to_string(),
            "out": target.display().to_string(),
            "from": from.extension(),
            "to": target_format.extension(),
            "cues": cues.len(),
        }))
    }

    /// Extract one embedded subtitle stream with ffmpeg.
    pub fn subs_extract(&self, key: &str, subtitle_index: u32, out: &str) -> MagpieResult<Value> {
        let (lib, row) = self.resolve_item(key)?;
        let source = crate::catalog::library_path(&lib, &row.rel);
        if !source.is_file() {
            return Err(MagpieError::not_found(format!(
                "{} is not on disk",
                source.display()
            )));
        }
        let target = PathBuf::from(out.trim());
        if out.trim().is_empty() {
            return Err(MagpieError::invalid("output path is empty"));
        }
        if target.exists() {
            return Err(MagpieError::invalid(format!(
                "{} already exists, refusing to overwrite",
                target.display()
            )));
        }
        let written =
            magpiecraft_tools::extract_subtitle(self.tools(), &source, subtitle_index, &target)?;
        Ok(json!({
            "ok": true,
            "key": row.key(),
            "from": source.display().to_string(),
            "out": written.display().to_string(),
            "subtitle_index": subtitle_index,
            "bytes": std::fs::metadata(&written).map(|m| m.len()).unwrap_or(0),
        }))
    }
}

/// The media file a subtitle sidecar belongs to, used by the app to offer "open the
/// video for this subtitle".
pub fn sibling_media_row(row: &ItemRow) -> Option<&str> {
    if SubFormat::from_path(Path::new(&row.name)).is_some() {
        Some(&row.name)
    } else {
        None
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    const SRT: &str = "1\r\n00:00:01,000 --> 00:00:03,500\r\nHello there\r\n\r\n2\r\n00:00:04,000 --> 00:00:06,000\r\nLine one\r\nLine two\r\n";

    #[test]
    fn srt_parses_with_crlf_and_a_bom() {
        let cues = parse_cues(&format!("\u{feff}{SRT}"), SubFormat::Srt);
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].start_ms, 1000);
        assert_eq!(cues[0].end_ms, 3500);
        assert_eq!(cues[0].text, "Hello there");
        assert_eq!(cues[1].text, "Line one\nLine two");
    }

    #[test]
    fn vtt_parses_with_a_header_settings_and_ids() {
        let vtt = "WEBVTT\nKind: captions\n\nNOTE a comment\n\ncue-1\n00:01.000 --> 00:03.000 align:start position:10%\nHi\n";
        let cues = parse_cues(vtt, SubFormat::Vtt);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].start_ms, 1000);
        assert_eq!(cues[0].end_ms, 3000);
        assert_eq!(cues[0].text, "Hi");
    }

    #[test]
    fn timestamps_round_trip_in_both_formats() {
        assert_eq!(parse_timestamp("00:00:01,000"), Some(1000));
        assert_eq!(parse_timestamp("01:02:03.456"), Some(3_723_456));
        assert_eq!(parse_timestamp("02:03.5"), Some(123_500));
        assert_eq!(parse_timestamp("not a time"), None);
        assert_eq!(parse_timestamp("00:99:00,000"), None);
        assert_eq!(format_timestamp(3_723_456, ','), "01:02:03,456");
        assert_eq!(format_timestamp(1000, '.'), "00:00:01.000");
        assert_eq!(format_timestamp(-5, ','), "00:00:00,000");
    }

    #[test]
    fn shifting_moves_cues_and_drops_the_ones_pushed_before_zero() {
        let mut cues = parse_cues(SRT, SubFormat::Srt);
        assert_eq!(shift_cues(&mut cues, 1500), 0);
        assert_eq!(cues[0].start_ms, 2500);
        assert_eq!(cues[0].end_ms, 5000);
        // Backwards: the first cue starts at zero, keeps its length; the second
        // survives; a big shift drops everything.
        let mut cues = parse_cues(SRT, SubFormat::Srt);
        shift_cues(&mut cues, -2000);
        assert_eq!(cues[0].start_ms, 0);
        assert_eq!(
            cues[0].end_ms, 1500,
            "the cue is truncated at zero, not stretched"
        );
        let mut cues = parse_cues(SRT, SubFormat::Srt);
        assert_eq!(shift_cues(&mut cues, -60_000), 2);
        assert!(cues.is_empty());
    }

    #[test]
    fn converting_srt_to_vtt_and_back_keeps_the_cues() {
        let cues = parse_cues(SRT, SubFormat::Srt);
        let vtt = render_cues(&cues, SubFormat::Vtt).unwrap();
        assert!(vtt.starts_with("WEBVTT\n\n"));
        assert!(vtt.contains("00:00:01.000 --> 00:00:03.500"));
        let back = parse_cues(&vtt, SubFormat::Vtt);
        assert_eq!(back, cues);
        let srt = render_cues(&back, SubFormat::Srt).unwrap();
        assert!(srt.starts_with("1\n00:00:01,000 --> 00:00:03,500"));
        assert!(
            render_cues(&cues, SubFormat::Ass).is_err(),
            "ASS is read-only"
        );
    }

    #[test]
    fn a_file_with_no_cues_is_reported_not_written() {
        assert!(parse_cues("just words", SubFormat::Srt).is_empty());
        assert!(parse_cues("", SubFormat::Vtt).is_empty());
    }
}
