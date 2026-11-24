//! Metadata foundations for Tier 2.
//!
//! This crate holds the parts that must be right before any provider call is
//! written: who each provider is, what it costs, what its terms demand, and a rate
//! limiter that can be tested without waiting in real time.
//!
//! No network code lives here yet. MusicBrainz (1 request per second, real user
//! agent) and TMDB (attribution, per-user key, no key shipped) are the first two
//! adapters and both depend on `Throttle`.

use serde::{Deserialize, Serialize};

/// The user agent every provider request must carry. MusicBrainz rejects generic
/// or absent user agents, and a contact URL is part of its terms.
pub fn user_agent() -> String {
    format!(
        "MagpieCraft/{} (+https://github.com/sandraschi/magpiecraft)",
        env!("CARGO_PKG_VERSION")
    )
}

/// Who a metadata call goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    /// MusicBrainz, music releases and artists. Open data, rate limited.
    MusicBrainz,
    /// AcoustID, audio fingerprints resolved to MusicBrainz recordings.
    AcoustId,
    /// TMDB, film and TV metadata. Attribution required, per-user key.
    Tmdb,
    /// Open Library, books.
    OpenLibrary,
    /// ComicVine, comics issues and credits. Per-user key.
    ComicVine,
    /// MangaBaka, local SQLite dump preferred over the API.
    MangaBaka,
    /// Grand Comics Database, local dump only.
    Gcd,
    /// LRCLIB, opt-in lyrics provider, one request at a time.
    Lrclib,
    /// OpenSubtitles, opt-in with the user's own key.
    OpenSubtitles,
}

impl Provider {
    pub fn id(self) -> &'static str {
        match self {
            Provider::MusicBrainz => "musicbrainz",
            Provider::AcoustId => "acoustid",
            Provider::Tmdb => "tmdb",
            Provider::OpenLibrary => "openlibrary",
            Provider::ComicVine => "comicvine",
            Provider::MangaBaka => "mangabaka",
            Provider::Gcd => "gcd",
            Provider::Lrclib => "lrclib",
            Provider::OpenSubtitles => "opensubtitles",
        }
    }

    /// Minimum milliseconds between two requests. Local dumps are unlimited.
    pub fn min_interval_ms(self) -> u64 {
        match self {
            Provider::MusicBrainz => 1_000,
            Provider::AcoustId => 350,
            Provider::Tmdb => 260,
            Provider::OpenLibrary => 1_000,
            Provider::ComicVine => 1_000,
            Provider::Lrclib => 1_000,
            Provider::OpenSubtitles => 1_000,
            // Local SQLite dumps: no network, no limit.
            Provider::MangaBaka | Provider::Gcd => 0,
        }
    }

    /// True when the provider needs an API key the user supplies.
    pub fn needs_key(self) -> bool {
        matches!(
            self,
            Provider::Tmdb | Provider::ComicVine | Provider::OpenSubtitles | Provider::AcoustId
        )
    }

    /// True when the data can come from a local dump, which is always preferred.
    pub fn prefers_local_dump(self) -> bool {
        matches!(self, Provider::MangaBaka | Provider::Gcd)
    }

    /// One line of terms the UI must show next to results from this provider.
    pub fn attribution(self) -> &'static str {
        match self {
            Provider::MusicBrainz => {
                "Metadata from MusicBrainz (CC0). Requests are limited to one per second."
            }
            Provider::AcoustId => "Fingerprint matches from AcoustID.",
            Provider::Tmdb => {
                "This product uses the TMDB API but is not endorsed or certified by TMDB."
            }
            Provider::OpenLibrary => "Metadata from Open Library (CC0).",
            Provider::ComicVine => "Metadata from Comic Vine. Per-user key.",
            Provider::MangaBaka => "Manga metadata from a local MangaBaka dump.",
            Provider::Gcd => "Comic credits from a local Grand Comics Database dump.",
            Provider::Lrclib => "Lyrics from LRCLIB, opt-in, cached as sidecar files.",
            Provider::OpenSubtitles => {
                "Subtitles from OpenSubtitles, opt-in with your own API key."
            }
        }
    }
}

/// Where one field's value came from. Tier 2 stores provenance per field so a
/// later provider cannot silently overwrite a user's correction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub field: String,
    /// Provider id, `embedded` for the file's own tags, or `manual` for the user.
    pub source: String,
    /// ISO 8601 timestamp of when the value was recorded.
    pub at: String,
    /// 0.0 to 1.0.
    pub confidence: u8,
    /// Locked fields are never overwritten by an automatic update.
    pub locked: bool,
}

impl Provenance {
    pub fn manual(field: &str, at: &str) -> Self {
        Provenance {
            field: field.to_string(),
            source: "manual".to_string(),
            at: at.to_string(),
            confidence: 100,
            locked: true,
        }
    }

    /// Whether an incoming value may replace this one.
    pub fn may_be_replaced_by(&self, incoming: &Provenance) -> bool {
        !self.locked && incoming.confidence >= self.confidence
    }
}

/// Token bucket expressed as "earliest next call", in milliseconds on a monotonic
/// clock the caller supplies. Pure, so tests do not sleep.
#[derive(Debug, Clone)]
pub struct Throttle {
    min_interval_ms: u64,
    next_allowed_ms: u64,
}

impl Throttle {
    pub fn new(min_interval_ms: u64) -> Self {
        Throttle {
            min_interval_ms,
            next_allowed_ms: 0,
        }
    }

    pub fn for_provider(provider: Provider) -> Self {
        Throttle::new(provider.min_interval_ms())
    }

    pub fn min_interval_ms(&self) -> u64 {
        self.min_interval_ms
    }

    /// Ask for a slot at `now_ms`. Returns how long to wait (0 = go now) and
    /// records the slot when the wait is zero.
    pub fn request(&mut self, now_ms: u64) -> u64 {
        if now_ms < self.next_allowed_ms {
            return self.next_allowed_ms - now_ms;
        }
        self.next_allowed_ms = now_ms.saturating_add(self.min_interval_ms);
        0
    }

    /// Blocking variant for real use. Returns the wait that was applied.
    pub fn acquire(&mut self) -> u64 {
        let now = now_ms();
        let wait = self.request(now);
        if wait > 0 {
            std::thread::sleep(std::time::Duration::from_millis(wait));
            // Re-record the slot after sleeping so the next call is also spaced.
            let after = now_ms();
            let _ = self.request(after);
        }
        wait
    }
}

/// Milliseconds from a process-local monotonic origin.
pub fn now_ms() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    let origin = ORIGIN.get_or_init(Instant::now);
    u64::try_from(origin.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn musicbrainz_is_limited_to_one_per_second() {
        assert_eq!(Provider::MusicBrainz.min_interval_ms(), 1_000);
        let mut t = Throttle::for_provider(Provider::MusicBrainz);
        assert_eq!(t.request(0), 0, "first call is free");
        assert_eq!(t.request(0), 1_000, "second call waits a second");
        assert_eq!(
            t.request(400),
            600,
            "partial wait is reported as a remainder"
        );
        assert_eq!(t.request(1_000), 0);
        assert_eq!(t.request(1_500), 500);
    }

    #[test]
    fn local_dumps_are_not_throttled() {
        let mut t = Throttle::for_provider(Provider::Gcd);
        assert_eq!(t.request(0), 0);
        assert_eq!(t.request(0), 0);
        assert_eq!(t.request(0), 0);
    }

    #[test]
    fn keys_and_attribution_are_declared() {
        assert!(Provider::Tmdb.needs_key());
        assert!(!Provider::MusicBrainz.needs_key());
        assert!(Provider::Gcd.prefers_local_dump());
        assert!(Provider::Tmdb.attribution().contains("not endorsed"));
        assert!(user_agent().starts_with("MagpieCraft/"));
        assert!(user_agent().contains("http"));
    }

    #[test]
    fn manual_edits_are_locked_against_automatic_overwrites() {
        let manual = Provenance::manual("title", "2026-11-24T12:00:00Z");
        let incoming = Provenance {
            field: "title".to_string(),
            source: "tmdb".to_string(),
            at: "2026-11-25T12:00:00Z".to_string(),
            confidence: 99,
            locked: false,
        };
        assert!(!manual.may_be_replaced_by(&incoming));
        let weak = Provenance {
            confidence: 40,
            ..incoming.clone()
        };
        let strong = Provenance {
            confidence: 90,
            locked: false,
            ..incoming
        };
        assert!(weak.may_be_replaced_by(&strong));
    }
}
