//! Federated source registry (pillar E).
//!
//! A registry, not a scraper. Each entry says what the source is, how it is
//! reached, what it costs to use and whether it is enabled by default. Nothing in
//! this crate opens a socket: `probe` needs a transport, and no transport is
//! shipped until Tier 4. An unbuilt source reports `not_configured`, never a fake
//! empty result.
//!
//! Sources are the user's own servers and open catalogues only. No indexers, no
//! trackers, no third-party scan sites.

use serde::{Deserialize, Serialize};

/// How a source is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    /// Local HTTP API (Plex, Jellyfin, Kavita, Komga, Immich, Photoprism).
    Http,
    /// OPDS catalogue (Kavita, Komga, Calibre-Web, Audiobookshelf).
    Opds,
    /// A SQLite file on disk (Calibre metadata.db, Audiobookshelf).
    Sqlite,
    /// A local MCP server, called in-process or over its tool surface.
    Mcp,
}

/// One source the collection can be browsed through.
#[derive(Debug, Clone, Serialize)]
pub struct SourceSpec {
    pub id: &'static str,
    pub name: &'static str,
    /// Which of the user's media kinds it holds.
    pub kinds: &'static [&'static str],
    pub transport: Transport,
    /// True when it serves the user's own files, false for an open catalogue.
    pub self_hosted: bool,
    /// True when it needs a URL and usually a token before it works.
    pub needs_credentials: bool,
    /// Whether Tier 1 turns it on. Only local files are on by default.
    pub enabled_by_default: bool,
    /// One line for the UI.
    pub note: &'static str,
}

/// Every source MagpieCraft intends to read, in display order.
pub const SOURCES: &[SourceSpec] = &[
    SourceSpec {
        id: "local",
        name: "Local folders",
        kinds: &[
            "video", "audio", "ebook", "comic", "manga", "magazine", "photo",
        ],
        transport: Transport::Sqlite,
        self_hosted: true,
        needs_credentials: false,
        enabled_by_default: true,
        note: "The catalogue itself: libraries on this machine or an attached drive.",
    },
    SourceSpec {
        id: "jellyfin",
        name: "Jellyfin",
        kinds: &["video", "audio"],
        transport: Transport::Http,
        self_hosted: true,
        needs_credentials: true,
        enabled_by_default: false,
        note: "Read-only library listing and watch state.",
    },
    SourceSpec {
        id: "plex",
        name: "Plex",
        kinds: &["video", "audio"],
        transport: Transport::Http,
        self_hosted: true,
        needs_credentials: true,
        enabled_by_default: false,
        note: "Read-only library listing.",
    },
    SourceSpec {
        id: "kavita",
        name: "Kavita",
        kinds: &["comic", "manga", "ebook"],
        transport: Transport::Opds,
        self_hosted: true,
        needs_credentials: true,
        enabled_by_default: false,
        note: "OPDS catalogue, read-only.",
    },
    SourceSpec {
        id: "komga",
        name: "Komga",
        kinds: &["comic", "manga", "magazine"],
        transport: Transport::Opds,
        self_hosted: true,
        needs_credentials: true,
        enabled_by_default: false,
        note: "OPDS catalogue, read-only.",
    },
    SourceSpec {
        id: "calibre",
        name: "Calibre library",
        kinds: &["ebook"],
        transport: Transport::Sqlite,
        self_hosted: true,
        needs_credentials: false,
        enabled_by_default: false,
        note: "Reads metadata.db directly; book management stays in BookCraft.",
    },
    SourceSpec {
        id: "audiobookshelf",
        name: "Audiobookshelf",
        kinds: &["audio"],
        transport: Transport::Opds,
        self_hosted: true,
        needs_credentials: true,
        enabled_by_default: false,
        note: "Read-only progress and library listing.",
    },
    SourceSpec {
        id: "immich",
        name: "Immich",
        kinds: &["photo"],
        transport: Transport::Http,
        self_hosted: true,
        needs_credentials: true,
        enabled_by_default: false,
        note: "Read-only album and asset listing.",
    },
    SourceSpec {
        id: "openlibrary",
        name: "Open Library",
        kinds: &["ebook"],
        transport: Transport::Http,
        self_hosted: false,
        needs_credentials: false,
        enabled_by_default: false,
        note: "Open metadata catalogue (CC0), rate limited, attribution shown.",
    },
];

pub fn get(id: &str) -> Option<&'static SourceSpec> {
    SOURCES.iter().find(|s| s.id == id)
}

pub fn enabled_by_default() -> Vec<&'static SourceSpec> {
    SOURCES.iter().filter(|s| s.enabled_by_default).collect()
}

/// Probe result. Kept explicit so callers cannot mistake "not wired yet" for
/// "empty library".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Probe {
    /// No transport is implemented in this build.
    NotImplemented,
    /// A transport exists but nothing is configured for this source.
    NotConfigured,
    /// Reachable, with a human note.
    Online(String),
    /// Configured but unreachable.
    Offline(String),
}

impl Probe {
    pub fn is_online(&self) -> bool {
        matches!(self, Probe::Online(_))
    }
}

/// Ask a source whether it can serve. Without a transport this is honest about it.
pub fn probe(spec: &SourceSpec, transport_available: bool, configured: bool) -> Probe {
    if !transport_available {
        return Probe::NotImplemented;
    }
    if !configured {
        return Probe::NotConfigured;
    }
    Probe::Offline(format!("{} configured, no response", spec.name))
}

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("unknown source: {0}")]
    Unknown(String),
    #[error("source not available: {0}")]
    NotAvailable(&'static str),
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_consistent() {
        assert!(!SOURCES.is_empty());
        let mut ids: Vec<&str> = SOURCES.iter().map(|s| s.id).collect();
        ids.sort_unstable();
        let unique = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), unique, "duplicate source ids");
        for s in SOURCES {
            assert!(!s.note.is_empty(), "{} has no note", s.id);
            assert!(!s.kinds.is_empty(), "{} has no kinds", s.id);
        }
    }

    #[test]
    fn only_local_files_are_on_by_default() {
        let enabled = enabled_by_default();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].id, "local");
        assert!(get("jellyfin").is_some());
        assert!(get("nope").is_none());
    }

    #[test]
    fn nothing_claims_to_be_online_without_a_transport() {
        let jellyfin = get("jellyfin").unwrap();
        assert_eq!(probe(jellyfin, false, true), Probe::NotImplemented);
        assert_eq!(probe(jellyfin, true, false), Probe::NotConfigured);
        assert!(!probe(jellyfin, false, true).is_online());
    }
}
