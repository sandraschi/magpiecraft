//! Filename parsing for series, volume and issue numbers.
//!
//! Guessing is allowed here, silently rewriting files is not. Every parse carries a
//! confidence and anything below the review threshold is flagged so the UI can put
//! it in a review list instead of trusting it.

use serde::{Deserialize, Serialize};

/// Volume markers we accept, longest first so `volume` wins over `vol`.
const MARKER_WORDS: &[&str] = &["volume", "vol.", "vol", "tome", "book", "bd", "v"];

/// Parsed view of one file or folder name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedName {
    /// Series title as far as it can be trusted.
    pub series: String,
    /// Volume or issue marker, as written ("05", "12", "54").
    pub volume: Option<String>,
    pub year: Option<i32>,
    /// 0.0 to 1.0. Below `REVIEW_THRESHOLD` the name needs a human.
    pub confidence: f32,
    pub needs_review: bool,
}

/// Parses below this confidence appear in the review queue.
pub const REVIEW_THRESHOLD: f32 = 0.5;

/// Parse `Berserk Vol. 12 (1990)`, `Saga #54`, `Yotsuba v05`, `Batman 003` and
/// `Some Folder Name`. Never fails; an unparseable name becomes the series with
/// low confidence.
pub fn parse_volume_name(name: &str) -> ParsedName {
    let stem = match name.rsplit_once('.') {
        Some((s, ext)) if !s.is_empty() && ext.len() <= 5 => s,
        _ => name,
    };
    let (mut work, year) = strip_year(stem);
    work = strip_bracket_groups(&work);
    let tokens = tokenise(&work);

    if let Some((idx, volume, confidence)) = find_marker(&tokens) {
        let series = clean_series(&tokens[..idx]);
        if !series.is_empty() {
            let confidence = if confidence >= REVIEW_THRESHOLD {
                confidence
            } else if year.is_some() {
                confidence + 0.05
            } else {
                confidence
            };
            return ParsedName {
                series,
                volume: Some(volume),
                year,
                confidence,
                needs_review: confidence < REVIEW_THRESHOLD,
            };
        }
    }

    let series = clean_series(&tokens);
    let confidence = if series.is_empty() { 0.0 } else { 0.25 };
    ParsedName {
        series: if series.is_empty() {
            stem.trim().to_string()
        } else {
            series
        },
        volume: None,
        year,
        confidence,
        needs_review: true,
    }
}

/// Pull a four-digit year in parentheses out of the name.
fn strip_year(stem: &str) -> (String, Option<i32>) {
    let bytes: Vec<char> = stem.chars().collect();
    let mut year = None;
    let mut out = String::with_capacity(stem.len());
    let mut i = 0usize;
    while i < bytes.len() {
        let ch = bytes.get(i).copied().unwrap_or(' ');
        if ch == '(' || ch == '[' {
            // Look for (19xx) or (20xx) closing within 6 chars.
            let close = if ch == '(' { ')' } else { ']' };
            let mut j = i + 1;
            let mut digits = String::new();
            while j < bytes.len() && j <= i + 6 {
                let c = bytes.get(j).copied().unwrap_or(' ');
                if c == close {
                    break;
                }
                if c.is_ascii_digit() {
                    digits.push(c);
                } else {
                    break;
                }
                j += 1;
            }
            let closed = bytes.get(j).copied() == Some(close);
            if closed && digits.len() == 4 {
                let value = digits.parse::<i32>().unwrap_or(0);
                if (1900..=2099).contains(&value) {
                    year = Some(value);
                    i = j + 1;
                    continue;
                }
            }
        }
        out.push(ch);
        i += 1;
    }
    (collapse_spaces(&out), year)
}

/// Drop `[Group]` and `(Digital)` style groups that are not years.
fn strip_bracket_groups(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    for ch in s.chars() {
        match ch {
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => depth = depth.saturating_sub(1),
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    collapse_spaces(&out)
}

fn tokenise(s: &str) -> Vec<String> {
    s.split_whitespace()
        .map(|t| {
            t.trim_matches(|c: char| c == '-' || c == '_' || c == ',' || c == ':')
                .to_string()
        })
        .filter(|t| !t.is_empty())
        .collect()
}

/// Find the first volume marker. Returns (index, volume text, confidence).
fn find_marker(tokens: &[String]) -> Option<(usize, String, f32)> {
    for (i, token) in tokens.iter().enumerate() {
        let lower = token.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix('#') {
            let digits = leading_digits(rest);
            if !digits.is_empty() {
                return Some((i, digits, 0.85));
            }
        }
        // vol12, vol.12, v05
        for word in MARKER_WORDS {
            let glued = word.replace('.', "");
            for prefix in [word.to_string(), glued] {
                if let Some(rest) = lower.strip_prefix(&prefix) {
                    let digits = leading_digits(rest);
                    if !digits.is_empty() {
                        return Some((i, digits, 0.9));
                    }
                    if rest.is_empty() || rest == "." {
                        // "Vol. 12": number is the next token.
                        if let Some(next) = tokens.get(i + 1) {
                            let digits = leading_digits(&next.to_ascii_lowercase());
                            if !digits.is_empty()
                                && digits.len()
                                    == next.trim_matches(|c: char| !c.is_ascii_digit()).len()
                            {
                                return Some((i, digits, 0.9));
                            }
                        }
                    }
                }
            }
        }
    }
    // A bare number as the last token: "Batman 003". Reject years and long runs.
    if let Some(last) = tokens.last() {
        let digits = leading_digits(last);
        if !digits.is_empty() && digits.len() == last.len() && digits.len() <= 4 {
            let value = digits.parse::<i32>().unwrap_or(0);
            if !(1900..=2099).contains(&value) && tokens.len() >= 2 {
                return Some((tokens.len() - 1, digits, 0.6));
            }
        }
    }
    None
}

fn leading_digits(s: &str) -> String {
    s.chars().take_while(char::is_ascii_digit).collect()
}

fn clean_series(tokens: &[String]) -> String {
    let joined = tokens.join(" ");
    let trimmed = joined.trim().trim_end_matches(|c: char| {
        c == '-' || c == '_' || c == ',' || c == ':' || c.is_whitespace()
    });
    collapse_spaces(trimmed)
}

fn collapse_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_was_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !last_was_space && !out.is_empty() {
                out.push(' ');
            }
            last_was_space = true;
        } else {
            out.push(ch);
            last_was_space = false;
        }
    }
    out.trim_end().to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn explicit_volume_markers() {
        let p = parse_volume_name("Berserk Vol. 12 (1990).cbz");
        assert_eq!(p.series, "Berserk");
        assert_eq!(p.volume.as_deref(), Some("12"));
        assert_eq!(p.year, Some(1990));
        assert!(p.confidence >= 0.9);
        assert!(!p.needs_review);

        let p = parse_volume_name("Yotsuba v05");
        assert_eq!(p.series, "Yotsuba");
        assert_eq!(p.volume.as_deref(), Some("05"));

        let p = parse_volume_name("Akira Volume 3");
        assert_eq!(p.series, "Akira");
        assert_eq!(p.volume.as_deref(), Some("3"));

        let p = parse_volume_name("Le Chat du Rabbin Tome 4");
        assert_eq!(p.series, "Le Chat du Rabbin");
        assert_eq!(p.volume.as_deref(), Some("4"));
    }

    #[test]
    fn issue_numbers() {
        let p = parse_volume_name("Saga #54 (2018).cbz");
        assert_eq!(p.series, "Saga");
        assert_eq!(p.volume.as_deref(), Some("54"));
        assert_eq!(p.year, Some(2018));
    }

    #[test]
    fn trailing_bare_number_is_lower_confidence() {
        let p = parse_volume_name("Batman 003");
        assert_eq!(p.series, "Batman");
        assert_eq!(p.volume.as_deref(), Some("003"));
        assert!(p.confidence < 0.9);
        assert!(!p.needs_review);
    }

    #[test]
    fn release_group_and_quality_tags_are_stripped() {
        let p = parse_volume_name("[Group] Bleach 012 [Digital] [cbr]");
        assert_eq!(p.series, "Bleach");
        assert_eq!(p.volume.as_deref(), Some("012"));
    }

    #[test]
    fn names_without_a_marker_need_review() {
        let p = parse_volume_name("Heavy Metal 198512");
        assert_eq!(
            p.volume, None,
            "a six digit run is not trusted as an issue number"
        );
        assert!(p.needs_review);
        let p = parse_volume_name("Some Folder Name");
        assert_eq!(p.series, "Some Folder Name");
        assert_eq!(p.volume, None);
        assert!(p.needs_review);
        assert_eq!(p.confidence, 0.25);
    }

    #[test]
    fn years_in_parens_are_not_volumes() {
        let p = parse_volume_name("Moebius (1987) - Collected");
        assert_eq!(p.year, Some(1987));
        assert_eq!(p.volume, None);
        assert!(p.needs_review, "nothing identifies a volume");
    }
}
