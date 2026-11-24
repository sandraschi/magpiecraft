//! ComicInfo.xml, the de-facto metadata file inside comic and manga archives.
//!
//! The parser is deliberately small and tolerant: it extracts a fixed set of
//! simple elements, ignores namespaces and unknown tags, and never fails on a
//! malformed document. A wrong or partial file leaves fields empty rather than
//! aborting a scan of ten thousand archives.

use serde::{Deserialize, Serialize};

/// The fields MagpieCraft reads from ComicInfo.xml.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComicInfo {
    pub series: Option<String>,
    pub number: Option<String>,
    pub volume: Option<String>,
    pub title: Option<String>,
    pub writer: Option<String>,
    pub penciller: Option<String>,
    pub publisher: Option<String>,
    pub year: Option<i32>,
    pub summary: Option<String>,
    pub page_count: Option<u32>,
    pub language: Option<String>,
    pub genre: Option<String>,
    /// `YesAndRightToLeft`, `Yes`, `No` or `Unknown`.
    pub manga: Option<String>,
    pub notes: Option<String>,
}

impl ComicInfo {
    pub fn is_empty(&self) -> bool {
        *self == ComicInfo::default()
    }

    /// Right-to-left reading, only when the file says so explicitly.
    pub fn rtl(&self) -> Option<bool> {
        match self.manga.as_deref() {
            Some(v) if v.eq_ignore_ascii_case("yesandrighttoleft") => Some(true),
            Some(v) if v.eq_ignore_ascii_case("no") => Some(false),
            _ => None,
        }
    }

    /// A human line for the item detail view: "Series 3 (2004), Writer".
    pub fn credit_line(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        let series = self.series.clone().unwrap_or_default();
        let number = self.number.clone().unwrap_or_default();
        if !series.is_empty() {
            let with_number = match (series.is_empty(), number.is_empty()) {
                (false, false) => format!("{series} {number}"),
                _ => series,
            };
            parts.push(with_number);
        }
        if let Some(year) = self.year {
            parts.push(format!("({year})"));
        }
        if let Some(w) = self.writer.as_ref().filter(|w| !w.trim().is_empty()) {
            parts.push(w.clone());
        }
        parts.join(" ")
    }
}

/// Parse a ComicInfo.xml document. Never fails; unknown or broken content is
/// simply absent.
pub fn parse_comic_info(xml: &str) -> ComicInfo {
    let page_count = tag_text(xml, "PageCount").and_then(|v| v.parse::<u32>().ok());
    ComicInfo {
        series: tag_text(xml, "Series"),
        number: tag_text(xml, "Number"),
        volume: tag_text(xml, "Volume"),
        title: tag_text(xml, "Title"),
        writer: tag_text(xml, "Writer"),
        penciller: tag_text(xml, "Penciller"),
        publisher: tag_text(xml, "Publisher"),
        year: tag_text(xml, "Year").and_then(|v| v.parse::<i32>().ok()),
        summary: tag_text(xml, "Summary"),
        page_count,
        language: tag_text(xml, "LanguageISO"),
        genre: tag_text(xml, "Genre"),
        manga: tag_text(xml, "Manga"),
        notes: tag_text(xml, "Notes"),
    }
}

/// Text of the first `<name>...</name>` element, case-insensitive, entities
/// decoded. `None` for a missing, empty or self-closing element.
fn tag_text(xml: &str, name: &str) -> Option<String> {
    let lower = xml.to_ascii_lowercase();
    let open_needle = format!("<{}", name.to_ascii_lowercase());
    let close_needle = format!("</{}", name.to_ascii_lowercase());
    let mut from = 0usize;
    loop {
        let open = lower.get(from..)?.find(&open_needle)? + from;
        let after_name = open + open_needle.len();
        // Reject `<SeriesName>` when looking for `<Series>`.
        let next_char = lower.get(after_name..).and_then(|s| s.chars().next());
        match next_char {
            Some(c) if c.is_whitespace() || c == '>' || c == '/' => {}
            _ => {
                from = after_name;
                continue;
            }
        }
        let open_end = lower.get(after_name..)?.find('>')? + after_name;
        if lower
            .get(open_end.saturating_sub(1)..open_end)
            .map(|s| s == "/")
            .unwrap_or(false)
        {
            return None;
        }
        let text_start = open_end + 1;
        let close = lower.get(text_start..)?.find(&close_needle)? + text_start;
        let raw = xml.get(text_start..close).unwrap_or("").trim();
        if raw.is_empty() {
            return None;
        }
        return Some(decode_entities(raw));
    }
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.char_indices();
    while let Some((i, ch)) = chars.next() {
        if ch != '&' {
            out.push(ch);
            continue;
        }
        let rest = match s.get(i..) {
            Some(r) => r,
            None => break,
        };
        let end = match rest.find(';') {
            Some(e) if e <= 10 => e,
            _ => {
                out.push('&');
                continue;
            }
        };
        let entity = rest.get(1..end).unwrap_or("");
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => {
                let hex = entity
                    .strip_prefix("#x")
                    .or_else(|| entity.strip_prefix("#X"));
                let dec = entity.strip_prefix('#');
                if let Some(h) = hex {
                    u32::from_str_radix(h, 16).ok().and_then(char::from_u32)
                } else if let Some(d) = dec {
                    d.parse::<u32>().ok().and_then(char::from_u32)
                } else {
                    None
                }
            }
        };
        match decoded {
            Some(c) => {
                out.push(c);
                for _ in 0..end {
                    chars.next();
                }
            }
            None => out.push('&'),
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<ComicInfo xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <Series>Yotsuba&amp;!</Series>
  <Number>5</Number>
  <Volume>5</Volume>
  <Title>The Whole Wide World</Title>
  <Writer>Kiyohiko Azuma</Writer>
  <Penciller>Kiyohiko Azuma</Penciller>
  <Publisher>ADV Manga</Publisher>
  <Year>2007</Year>
  <PageCount>226</PageCount>
  <LanguageISO>en</LanguageISO>
  <Genre>Comedy</Genre>
  <Manga>YesAndRightToLeft</Manga>
  <Summary>Summer, and the neighbourhood is loud.</Summary>
</ComicInfo>"#;

    #[test]
    fn reads_the_common_fields() {
        let ci = parse_comic_info(SAMPLE);
        assert_eq!(ci.series.as_deref(), Some("Yotsuba&!"));
        assert_eq!(ci.number.as_deref(), Some("5"));
        assert_eq!(ci.year, Some(2007));
        assert_eq!(ci.page_count, Some(226));
        assert_eq!(ci.writer.as_deref(), Some("Kiyohiko Azuma"));
        assert_eq!(ci.language.as_deref(), Some("en"));
        assert!(!ci.is_empty());
    }

    #[test]
    fn right_to_left_only_when_stated() {
        assert_eq!(parse_comic_info(SAMPLE).rtl(), Some(true));
        assert_eq!(
            parse_comic_info("<ComicInfo><Manga>No</Manga></ComicInfo>").rtl(),
            Some(false)
        );
        assert_eq!(
            parse_comic_info("<ComicInfo><Manga>Yes</Manga></ComicInfo>").rtl(),
            None
        );
        assert_eq!(parse_comic_info("<ComicInfo/>").rtl(), None);
    }

    #[test]
    fn tolerates_junk_garbage_and_prefix_collisions() {
        assert!(parse_comic_info("not xml at all").is_empty());
        assert!(parse_comic_info("").is_empty());
        let ci = parse_comic_info(
            "<ComicInfo><SeriesName>Wrong</SeriesName><Series>Right</Series></ComicInfo>",
        );
        assert_eq!(ci.series.as_deref(), Some("Right"));
        let self_closing = parse_comic_info("<ComicInfo><Series/><Number>2</Number></ComicInfo>");
        assert_eq!(self_closing.series, None);
        assert_eq!(self_closing.number.as_deref(), Some("2"));
    }

    #[test]
    fn decodes_entities_and_numeric_escapes() {
        assert_eq!(decode_entities("A &amp; B"), "A & B");
        assert_eq!(decode_entities("caf&#233;"), "caf\u{e9}");
        assert_eq!(decode_entities("&#x41;"), "A");
        assert_eq!(decode_entities("stray & sign"), "stray & sign");
    }

    #[test]
    fn credit_line_is_readable() {
        let ci = parse_comic_info(SAMPLE);
        assert_eq!(ci.credit_line(), "Yotsuba&! 5 (2007) Kiyohiko Azuma");
    }
}
