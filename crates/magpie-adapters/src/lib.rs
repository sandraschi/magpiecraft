//! Read-only adapters for self-hosted media servers.
//!
//! This crate is deliberately standalone: no `path = "../sibling"` dependencies, so
//! HomeServeCraft can pin it as a git dependency (decision D2). It performs no I/O
//! of its own; a `Transport` supplied by the caller does the requests, which keeps
//! every parse testable offline.
//!
//! Nothing here writes to a remote server. Adapters list and read, nothing else.

use serde::{Deserialize, Serialize};

pub type AdapterResult<T> = Result<T, AdapterError>;

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("transport: {0}")]
    Transport(String),
    #[error("parse: {0}")]
    Parse(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
}

impl AdapterError {
    pub fn kind(&self) -> &'static str {
        match self {
            AdapterError::Transport(_) => "network",
            AdapterError::Parse(_) => "parse",
            AdapterError::Unsupported(_) => "unsupported",
        }
    }
}

/// Anything that can fetch a URL as text. Implemented by the app with its HTTP
/// client, and by tests with a canned map.
pub trait Transport {
    fn get_text(&self, url: &str) -> AdapterResult<String>;
}

/// A canned transport for tests and for `--smoke`, where no network may be touched.
#[derive(Debug, Default, Clone)]
pub struct MapTransport {
    pages: std::collections::HashMap<String, String>,
}

impl MapTransport {
    pub fn new() -> Self {
        MapTransport::default()
    }

    pub fn with(mut self, url: &str, body: &str) -> Self {
        self.pages.insert(url.to_string(), body.to_string());
        self
    }
}

impl Transport for MapTransport {
    fn get_text(&self, url: &str) -> AdapterResult<String> {
        self.pages
            .get(url)
            .cloned()
            .ok_or_else(|| AdapterError::Transport(format!("no canned response for {url}")))
    }
}

/// What one remote entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteKind {
    Book,
    Comic,
    Audio,
    Video,
    Folder,
    Unknown,
}

/// One entry from a remote catalogue, normalised.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteItem {
    pub id: String,
    pub title: String,
    pub series: Option<String>,
    pub year: Option<i32>,
    pub kind: RemoteKind,
    /// Where to fetch the file or the next feed page.
    pub href: Option<String>,
    pub summary: Option<String>,
}

// ------------------------------------------------------------------- OPDS
/// An OPDS 1.2 navigation or acquisition feed.
#[derive(Debug, Clone)]
pub struct OpdsCatalog {
    pub base_url: String,
}

impl OpdsCatalog {
    pub fn new(base_url: &str) -> Self {
        OpdsCatalog {
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Fetch and parse the root feed.
    pub fn entries(&self, transport: &dyn Transport) -> AdapterResult<Vec<RemoteItem>> {
        let url = format!("{}/opds/v1.2/catalog", self.base_url);
        let body = transport.get_text(&url)?;
        Ok(parse_opds_feed(&body))
    }

    /// Parse a feed body. Split out so a caller can page through feeds itself.
    pub fn parse(&self, body: &str) -> Vec<RemoteItem> {
        parse_opds_feed(body)
    }
}

/// Parse an Atom/OPDS feed into entries. Tolerant of namespaces, missing fields
/// and HTML in summaries; a malformed feed yields fewer entries, never an error.
pub fn parse_opds_feed(xml: &str) -> Vec<RemoteItem> {
    let mut out = Vec::new();
    for block in blocks(xml, "entry") {
        let title = text_of(&block, "title").unwrap_or_default();
        if title.trim().is_empty() {
            continue;
        }
        let id = text_of(&block, "id").unwrap_or_else(|| title.clone());
        let summary = text_of(&block, "summary").or_else(|| text_of(&block, "content"));
        let links = links_of(&block);
        let acquisition = links
            .iter()
            .find(|l| l.rel.contains("acquisition"))
            .or_else(|| links.iter().find(|l| l.rel == "alternate"));
        let href = acquisition.map(|l| l.href.clone());
        let kind = match acquisition.and_then(|l| l.media_type.as_deref()) {
            Some(t) if t.contains("epub") || t.contains("pdf") => RemoteKind::Book,
            Some(t) if t.contains("cbz") || t.contains("comic") => RemoteKind::Comic,
            Some(t) if t.contains("audio") || t.contains("mp3") => RemoteKind::Audio,
            Some(t) if t.contains("video") => RemoteKind::Video,
            _ if href.is_none() => RemoteKind::Folder,
            _ => RemoteKind::Unknown,
        };
        out.push(RemoteItem {
            id,
            title,
            series: text_of(&block, "series"),
            year: text_of(&block, "issued")
                .or_else(|| text_of(&block, "published"))
                .and_then(|s| extract_year(&s)),
            kind,
            href,
            summary: summary.map(|s| strip_tags(&s)),
        });
    }
    out
}

fn extract_year(s: &str) -> Option<i32> {
    let digits: Vec<char> = s.chars().collect();
    for window in digits.windows(4) {
        if window.iter().all(char::is_ascii_digit) {
            let text: String = window.iter().collect();
            if let Ok(y) = text.parse::<i32>() {
                if (1400..=2100).contains(&y) {
                    return Some(y);
                }
            }
        }
    }
    None
}

// --------------------------------------------------------------- Jellyfin
/// A Jellyfin library, read-only.
#[derive(Debug, Clone)]
pub struct JellyfinLibrary {
    pub base_url: String,
    pub user_id: String,
}

impl JellyfinLibrary {
    pub fn new(base_url: &str, user_id: &str) -> Self {
        JellyfinLibrary {
            base_url: base_url.trim_end_matches('/').to_string(),
            user_id: user_id.to_string(),
        }
    }

    /// URL of the items listing. The API key is passed by the caller's transport,
    /// never stored here, so it cannot leak into a log or an export.
    pub fn items_url(&self, parent_id: Option<&str>) -> String {
        let parent = parent_id.unwrap_or("");
        format!(
            "{}/Users/{}/Items?Recursive=true&IncludeItemTypes=Movie,Series,Episode,Audio,Book&Fields=ProductionYear,SeriesName,Overview&ParentId={}",
            self.base_url, self.user_id, parent
        )
    }

    pub fn entries(&self, transport: &dyn Transport) -> AdapterResult<Vec<RemoteItem>> {
        let body = transport.get_text(&self.items_url(None))?;
        parse_jellyfin_items(&body)
    }
}

/// Parse a Jellyfin `Items` response.
pub fn parse_jellyfin_items(json: &str) -> AdapterResult<Vec<RemoteItem>> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;
    let items = value
        .get("Items")
        .and_then(|v| v.as_array())
        .ok_or_else(|| AdapterError::Parse("no Items array in Jellyfin response".to_string()))?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let title = item
            .get("Name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if title.is_empty() {
            continue;
        }
        let kind = match item.get("Type").and_then(|v| v.as_str()).unwrap_or("") {
            "Movie" | "Episode" => RemoteKind::Video,
            "Series" => RemoteKind::Folder,
            "Audio" => RemoteKind::Audio,
            "Book" => RemoteKind::Book,
            _ => RemoteKind::Unknown,
        };
        out.push(RemoteItem {
            id: item
                .get("Id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            title,
            series: item
                .get("SeriesName")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            year: item
                .get("ProductionYear")
                .and_then(|v| v.as_i64())
                .and_then(|y| i32::try_from(y).ok()),
            kind,
            href: None,
            summary: item
                .get("Overview")
                .and_then(|v| v.as_str())
                .map(str::to_string),
        });
    }
    Ok(out)
}

// ------------------------------------------------------------ tiny XML bits
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub rel: String,
    pub href: String,
    pub media_type: Option<String>,
}

/// Inner text of every `<name>...</name>` element, byte offsets preserved.
fn blocks(xml: &str, name: &str) -> Vec<String> {
    let lower = xml.to_ascii_lowercase();
    let open = format!("<{name}");
    let close = format!("</{name}>");
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = lower.get(from..).and_then(|s| s.find(&open)) {
        let start = from + rel;
        let open_end = match lower.get(start..).and_then(|s| s.find('>')) {
            Some(e) => start + e,
            None => break,
        };
        let close_at = match lower.get(open_end..).and_then(|s| s.find(&close)) {
            Some(c) => open_end + c,
            None => break,
        };
        if let Some(body) = xml.get(open_end + 1..close_at) {
            out.push(body.to_string());
        }
        from = close_at + close.len();
    }
    out
}

fn text_of(xml: &str, name: &str) -> Option<String> {
    let lower = xml.to_ascii_lowercase();
    let open = format!("<{name}");
    let close = format!("</{name}>");
    let start = lower.find(&open)?;
    let open_end = start + lower.get(start..)?.find('>')?;
    if lower.get(open_end.saturating_sub(1)..open_end)? == "/" {
        return None;
    }
    let close_at = open_end + lower.get(open_end..)?.find(&close)?;
    let raw = xml.get(open_end + 1..close_at)?.trim();
    if raw.is_empty() {
        None
    } else {
        Some(decode_entities(raw))
    }
}

fn links_of(xml: &str) -> Vec<Link> {
    let lower = xml.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = lower.get(from..).and_then(|s| s.find("<link")) {
        let start = from + rel;
        let end = match lower.get(start..).and_then(|s| s.find('>')) {
            Some(e) => start + e,
            None => break,
        };
        let tag = xml.get(start..end).unwrap_or("");
        let attrs = attributes(tag);
        let href = attrs
            .iter()
            .find(|(k, _)| k == "href")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        if !href.is_empty() {
            out.push(Link {
                rel: attrs
                    .iter()
                    .find(|(k, _)| k == "rel")
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default(),
                href,
                media_type: attrs
                    .iter()
                    .find(|(k, _)| k == "type")
                    .map(|(_, v)| v.clone()),
            });
        }
        from = end + 1;
    }
    out
}

fn attributes(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes: Vec<char> = tag.chars().collect();
    let mut i = 0usize;
    while i < bytes.len() {
        // Find `name="`.
        let mut key = String::new();
        while i < bytes.len() {
            let c = bytes.get(i).copied().unwrap_or(' ');
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ':' {
                key.push(c.to_ascii_lowercase());
                i += 1;
            } else {
                break;
            }
        }
        if i < bytes.len() && bytes.get(i) == Some(&'=') && !key.is_empty() {
            i += 1;
            let quote = bytes.get(i).copied().unwrap_or(' ');
            if quote == '"' || quote == '\'' {
                i += 1;
                let mut value = String::new();
                while i < bytes.len() && bytes.get(i).copied() != Some(quote) {
                    if let Some(c) = bytes.get(i) {
                        value.push(*c);
                    }
                    i += 1;
                }
                i += 1;
                out.push((key, decode_entities(&value)));
            }
        } else {
            i += 1;
        }
    }
    out
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    for ch in s.chars() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    const FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opds="http://opds-spec.org/2010/catalog">
  <title>Library</title>
  <entry>
    <title>Saga #54</title>
    <id>urn:komga:series:54</id>
    <updated>2024-01-02T00:00:00Z</updated>
    <published>2018-05-01T00:00:00Z</published>
    <summary type="text">&lt;b&gt;The war ends.&lt;/b&gt;</summary>
    <link rel="http://opds-spec.org/acquisition" href="/opds/v1.2/books/54/file.cbz" type="application/vnd.comicbook+zip"/>
  </entry>
  <entry>
    <title>Yotsuba&amp;! v05</title>
    <id>urn:komga:series:5</id>
    <link rel="subsection" href="/opds/v1.2/series/5" type="application/atom+xml"/>
  </entry>
  <entry></entry>
</feed>"#;

    #[test]
    fn opds_feed_becomes_items() {
        let items = parse_opds_feed(FEED);
        assert_eq!(items.len(), 2, "the empty entry is skipped");
        let first = &items[0];
        assert_eq!(first.title, "Saga #54");
        assert_eq!(first.kind, RemoteKind::Comic);
        assert_eq!(first.year, Some(2018));
        assert_eq!(first.summary.as_deref(), Some("The war ends."));
        assert_eq!(first.href.as_deref(), Some("/opds/v1.2/books/54/file.cbz"));
        let second = &items[1];
        assert_eq!(second.title, "Yotsuba&! v05");
        assert_eq!(second.kind, RemoteKind::Folder, "no acquisition link");
    }

    #[test]
    fn opds_through_a_transport() {
        let t = MapTransport::new().with("http://komga.local/opds/v1.2/catalog", FEED);
        let catalog = OpdsCatalog::new("http://komga.local/");
        assert_eq!(catalog.entries(&t).unwrap().len(), 2);
        let missing = MapTransport::new();
        assert_eq!(catalog.entries(&missing).unwrap_err().kind(), "network");
    }

    #[test]
    fn jellyfin_items_parse_and_urls_are_built() {
        let json = r#"{"Items":[
            {"Id":"abc","Name":"Blade Runner 2049","Type":"Movie","ProductionYear":2017,"Overview":"A replicant."},
            {"Id":"def","Name":"Dune","Type":"Series","SeriesName":"Dune"},
            {"Name":"","Type":"Movie"}
        ]}"#;
        let items = parse_jellyfin_items(json).unwrap();
        assert_eq!(items.len(), 2, "an item without a name is skipped");
        assert_eq!(items[0].year, Some(2017));
        assert_eq!(items[0].kind, RemoteKind::Video);
        assert_eq!(items[1].kind, RemoteKind::Folder);
        assert_eq!(items[1].series.as_deref(), Some("Dune"));
        let lib = JellyfinLibrary::new("http://jf.local/", "user1");
        let url = lib.items_url(Some("lib9"));
        assert!(url.starts_with("http://jf.local/Users/user1/Items?"));
        assert!(url.contains("ParentId=lib9"));
        assert!(parse_jellyfin_items("{}").is_err());
    }

    #[test]
    fn xml_attributes_handle_quotes_and_entities() {
        let attrs = attributes(
            "rel=\"http://opds-spec.org/acquisition\" href='/a?b=1&amp;c=2' type=\"text/x\"",
        );
        assert!(attrs.iter().any(|(k, v)| k == "href" && v == "/a?b=1&c=2"));
        assert!(attrs
            .iter()
            .any(|(k, v)| k == "rel" && v.contains("acquisition")));
    }
}
