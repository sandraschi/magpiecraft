//! First-run inventory: which media apps are installed, what MagpieCraft can
//! harvest from each, and how.
//!
//! Read-only throughout. Plex, Jellyfin and Calibre keep local databases and
//! poster files that MagpieCraft can copy into its own cover cache; Immich,
//! Komga and Kavita are servers, so detection is a fast TCP probe plus an
//! identity request that needs no credentials. Every check reports what it
//! found, nothing is ever echoed (tokens stay presence-only), and an absent
//! app is a normal result, not an error.
//!
//! Environment overrides (all optional, all verified when set):
//! `MAGPIECRAFT_PLEX_HOME`, `MAGPIECRAFT_JELLYFIN_HOME`,
//! `MAGPIECRAFT_JELLYFIN_URL`, `MAGPIECRAFT_CALIBRE`,
//! `MAGPIECRAFT_CALIBRE_LIBRARY`, `MAGPIECRAFT_IMMICH_URL`,
//! `MAGPIECRAFT_KOMGA_URL`, `MAGPIECRAFT_KAVITA_URL`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::{find_tool, run_timeout, which, ToolError, ToolResult};

/// One check inside an app report: what was looked for and what was found.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppCheck {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

/// How useful an app is to MagpieCraft right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppState {
    /// The stash is harvestable now.
    Ready,
    /// Installed or running, but something is missing before harvest works.
    Partial,
    /// No trace found. Normal, not an error.
    Absent,
}

impl AppState {
    pub fn as_str(self) -> &'static str {
        match self {
            AppState::Ready => "ready",
            AppState::Partial => "partial",
            AppState::Absent => "absent",
        }
    }
}

/// One media app: what is there, what MagpieCraft can use, and how.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaApp {
    pub id: String,
    pub label: String,
    pub state: AppState,
    pub version: Option<String>,
    pub checks: Vec<AppCheck>,
    pub usable_for: Vec<String>,
    pub needs: String,
    pub data_path: Option<String>,
}

impl MediaApp {
    fn absent(id: &str, label: &str, needs: &str) -> Self {
        MediaApp {
            id: id.to_string(),
            label: label.to_string(),
            state: AppState::Absent,
            version: None,
            checks: Vec::new(),
            usable_for: Vec::new(),
            needs: needs.to_string(),
            data_path: None,
        }
    }
}

/// Fast TCP probe. Closed ports refuse immediately; a short timeout caps
/// filtered ones. No bytes are sent, so nothing can authenticate or break.
pub fn tcp_open(port: u16, timeout: Duration) -> bool {
    let addr: std::net::SocketAddr = match format!("127.0.0.1:{port}").parse() {
        Ok(a) => a,
        Err(_) => return false,
    };
    std::net::TcpStream::connect_timeout(&addr, timeout).is_ok()
}

/// Plain HTTP GET through curl (invoked, never linked). Returns the body on
/// HTTP 200, an error otherwise. Identity endpoints need no credentials.
fn http_get(url: &str, secs: u64) -> ToolResult<String> {
    let curl = which("curl").ok_or_else(|| ToolError::Missing("curl".to_string()))?;
    run_timeout(
        &curl,
        &["--silent", "--fail", "--max-time", &secs.to_string(), url],
        secs.saturating_add(5),
    )
}

fn check(name: &str, ok: bool, detail: impl Into<String>) -> AppCheck {
    AppCheck {
        name: name.to_string(),
        ok,
        detail: detail.into(),
    }
}

fn local_appdata() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
}

fn program_data() -> Option<PathBuf> {
    std::env::var_os("PROGRAMDATA").map(PathBuf::from)
}

fn user_profile() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

fn dir_size_gb(path: &Path) -> Option<f64> {
    let len = std::fs::metadata(path).ok()?.len();
    Some(len as f64 / 1_073_741_824.0)
}

fn file_count_parts(db: &Path) -> Option<i64> {
    let conn =
        rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .ok()?;
    conn.query_row("SELECT COUNT(*) FROM media_parts", [], |r| r.get(0))
        .ok()
}

/// Plex Media Server: local database plus poster bundles, both readable while
/// the server runs. File paths match MagpieCraft libraries, so posters join
/// by exact path.
pub fn detect_plex() -> MediaApp {
    let home = std::env::var_os("MAGPIECRAFT_PLEX_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| local_appdata().map(|p| p.join("Plex Media Server")));
    let mut checks = Vec::new();
    let mut usable = Vec::new();
    let mut version = None;
    let mut db_parts = None;
    let mut db_gb = None;
    let data_ok = home.as_ref().is_some_and(|h| h.is_dir());
    checks.push(check(
        "data directory",
        data_ok,
        home.as_ref()
            .map(|h| h.display().to_string())
            .unwrap_or_else(|| "not found under %LOCALAPPDATA%".to_string()),
    ));
    if let Some(h) = home.as_ref().filter(|h| h.is_dir()) {
        let db = h.join("Plug-in Support/Databases/com.plexapp.plugins.library.db");
        if db.is_file() {
            db_parts = file_count_parts(&db);
            db_gb = dir_size_gb(&db);
            let detail = match (db_parts, db_gb) {
                (Some(n), Some(g)) => format!("{n} files indexed, {g:.1} GiB database"),
                _ => "database present".to_string(),
            };
            checks.push(check("library database", true, detail));
            usable.push("video posters (exact file match)".to_string());
            usable.push("episode thumbnails".to_string());
            usable.push("titles and years".to_string());
        } else {
            checks.push(check("library database", false, "no library database yet"));
        }
        let prefs = h.join("Preferences.xml");
        if prefs.is_file() {
            let has_token = std::fs::read_to_string(&prefs)
                .map(|t| t.contains("PlexOnlineToken"))
                .unwrap_or(false);
            checks.push(check(
                "server token",
                has_token,
                if has_token {
                    "present (never shown)".to_string()
                } else {
                    "no token stored".to_string()
                },
            ));
        }
    }
    let port_open = tcp_open(32400, Duration::from_millis(400));
    checks.push(check(
        "server on :32400",
        port_open,
        if port_open {
            "listening".to_string()
        } else {
            "not running".to_string()
        },
    ));
    if port_open {
        if let Ok(body) = http_get("http://127.0.0.1:32400/identity", 3) {
            if let Some(v) = body
                .split("version=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
            {
                version = Some(v.to_string());
                checks.push(check("server version", true, v.to_string()));
            }
        }
    }
    let ready = db_parts.is_some();
    let partial = !ready && (data_ok || port_open);
    let needs = if ready {
        "nothing: read-only, automatic".to_string()
    } else if port_open {
        "Plex Media Server runs; if the data directory was missed, point MAGPIECRAFT_PLEX_HOME at it".to_string()
    } else {
        "install Plex Media Server and let it scan the same folders".to_string()
    };
    MediaApp {
        id: "plex".to_string(),
        label: "Plex Media Server".to_string(),
        state: if ready {
            AppState::Ready
        } else if partial {
            AppState::Partial
        } else {
            AppState::Absent
        },
        version,
        checks,
        usable_for: usable,
        needs,
        data_path: home
            .as_ref()
            .map(|h| h.display().to_string())
            .filter(|_| data_ok),
    }
}

/// Jellyfin: open API with a public identity endpoint. Posters and metadata
/// come through the server, so it must be running; an API key unlocks more.
pub fn detect_jellyfin() -> MediaApp {
    let base = std::env::var_os("MAGPIECRAFT_JELLYFIN_URL")
        .and_then(|v| {
            let s = v.to_string_lossy().trim().to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        })
        .unwrap_or_else(|| "http://127.0.0.1:8096".to_string());
    let mut checks = Vec::new();
    let mut usable = Vec::new();
    let mut version = None;
    let port: u16 = base
        .rsplit(':')
        .next()
        .and_then(|p| p.trim_end_matches('/').parse().ok())
        .unwrap_or(8096);
    let port_open = tcp_open(port, Duration::from_millis(400));
    checks.push(check(
        "server port",
        port_open,
        if port_open {
            format!("{base} answers")
        } else {
            format!("{base} not listening (override with MAGPIECRAFT_JELLYFIN_URL)")
        },
    ));
    let mut identified = false;
    if port_open {
        let url = format!("{base}/System/Info/Public");
        if let Ok(body) = http_get(&url, 3) {
            if let Ok(info) = serde_json::from_str::<serde_json::Value>(&body) {
                let name = info
                    .get("ProductName")
                    .or_else(|| info.get("ServerName"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Jellyfin")
                    .to_string();
                version = info
                    .get("Version")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                checks.push(check(
                    "server identity",
                    true,
                    format!(
                        "{} {}",
                        name,
                        version.as_deref().unwrap_or("unknown version")
                    ),
                ));
                identified = true;
                usable.push("video posters (API, no key needed)".to_string());
                usable.push("titles, years and people".to_string());
            }
        }
        if !identified {
            checks.push(check(
                "server identity",
                false,
                "port is open but the public endpoint did not answer".to_string(),
            ));
        }
    }
    let data_candidates: Vec<PathBuf> = std::env::var_os("MAGPIECRAFT_JELLYFIN_HOME")
        .map(PathBuf::from)
        .into_iter()
        .chain(program_data().map(|p| p.join("Jellyfin/Server")))
        .chain(local_appdata().map(|p| p.join("jellyfin")))
        .collect();
    let data_path = data_candidates.iter().find(|p| p.is_dir()).cloned();
    checks.push(check(
        "data directory",
        data_path.is_some(),
        data_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "not found; server API still works".to_string()),
    ));
    MediaApp {
        id: "jellyfin".to_string(),
        label: "Jellyfin".to_string(),
        state: if identified {
            AppState::Ready
        } else if port_open {
            AppState::Partial
        } else {
            AppState::Absent
        },
        version,
        checks,
        usable_for: usable,
        needs: if identified {
            "nothing for posters; an API key unlocks watch state later".to_string()
        } else {
            "run Jellyfin (default http://127.0.0.1:8096) or set MAGPIECRAFT_JELLYFIN_URL"
                .to_string()
        },
        data_path: data_path.map(|p| p.display().to_string()),
    }
}

/// Calibre: desktop binary plus content libraries (metadata.db). Covers and
/// book metadata read straight from the library folders.
pub fn detect_calibre() -> MediaApp {
    let mut checks = Vec::new();
    let mut usable = Vec::new();
    let mut version = None;
    let exe = std::env::var_os("MAGPIECRAFT_CALIBRE")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| find_tool("MAGPIECRAFT_CALIBRE_UNSET", &["calibre"], &[]))
        .or_else(|| {
            let p = PathBuf::from("C:/Program Files/Calibre2/calibre.exe");
            p.is_file().then_some(p)
        })
        .or_else(|| {
            let p = PathBuf::from("C:/Program Files (x86)/Calibre2/calibre.exe");
            p.is_file().then_some(p)
        });
    checks.push(check(
        "calibre binary",
        exe.is_some(),
        exe.as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "not on PATH".to_string()),
    ));
    if let Some(p) = exe.as_ref() {
        if let Ok(out) = run_timeout(p, &["--version"], 10) {
            let v = out.trim().to_string();
            if !v.is_empty() {
                checks.push(check("calibre version", true, v.clone()));
                version = Some(v);
            }
        }
    }
    let lib_candidates: Vec<PathBuf> = std::env::var_os("MAGPIECRAFT_CALIBRE_LIBRARY")
        .map(PathBuf::from)
        .into_iter()
        .chain(user_profile().map(|p| p.join("Calibre Library")))
        .collect();
    let lib_db = lib_candidates
        .iter()
        .map(|p| p.join("metadata.db"))
        .find(|p| p.is_file());
    checks.push(check(
        "content library",
        lib_db.is_some(),
        lib_db
            .as_ref()
            .and_then(|p| p.parent())
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| {
                "no Calibre Library found (set MAGPIECRAFT_CALIBRE_LIBRARY)".to_string()
            }),
    ));
    if lib_db.is_some() {
        usable.push("ebook covers (cover.jpg beside each book)".to_string());
        usable.push("titles, authors, series, tags".to_string());
    }
    MediaApp {
        id: "calibre".to_string(),
        label: "Calibre".to_string(),
        state: if lib_db.is_some() {
            AppState::Ready
        } else if exe.is_some() {
            AppState::Partial
        } else {
            AppState::Absent
        },
        version,
        checks,
        usable_for: usable,
        needs: if lib_db.is_some() {
            "nothing: read-only, automatic".to_string()
        } else {
            "install Calibre or point MAGPIECRAFT_CALIBRE_LIBRARY at the library folder".to_string()
        },
        data_path: lib_db
            .as_ref()
            .and_then(|p| p.parent())
            .map(|p| p.display().to_string()),
    }
}

/// Servers identified by port plus credential-free endpoints. Komga and
/// Kavita need a login for their APIs, so an open port alone is partial.
fn detect_server(
    id: &str,
    label: &str,
    default_url: &str,
    env_name: &str,
    ping_path: &str,
    ping_marker: &str,
    version_path: Option<&str>,
    usable_ready: &[&str],
    needs_missing: &str,
) -> MediaApp {
    let base = std::env::var(env_name)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_url.to_string());
    let mut checks = Vec::new();
    let mut usable = Vec::new();
    let mut version = None;
    let port: u16 = base
        .rsplit(':')
        .next()
        .and_then(|p| p.trim_end_matches('/').parse().ok())
        .unwrap_or(0);
    let port_open = port > 0 && tcp_open(port, Duration::from_millis(400));
    checks.push(check(
        "server port",
        port_open,
        if port_open {
            format!("{base} answers")
        } else {
            format!("{base} not listening (override with {env_name})")
        },
    ));
    let mut identified = false;
    if port_open {
        if let Ok(body) = http_get(&format!("{base}{ping_path}"), 3) {
            if body.contains(ping_marker) {
                identified = true;
                checks.push(check("server identity", true, "ping answered".to_string()));
                if let Some(vp) = version_path {
                    if let Ok(vbody) = http_get(&format!("{base}{vp}"), 3) {
                        if let Ok(info) = serde_json::from_str::<serde_json::Value>(&vbody) {
                            let v = info
                                .get("Version")
                                .and_then(|x| x.as_str())
                                .map(str::to_string)
                                .or_else(|| {
                                    let parts = ["major", "minor", "patch"]
                                        .iter()
                                        .filter_map(|k| info.get(*k).and_then(|x| x.as_u64()));
                                    let parts: Vec<u64> = parts.collect();
                                    (!parts.is_empty()).then(|| {
                                        parts
                                            .iter()
                                            .map(|n| n.to_string())
                                            .collect::<Vec<_>>()
                                            .join(".")
                                    })
                                });
                            if let Some(v) = v {
                                checks.push(check("server version", true, v.clone()));
                                version = Some(v);
                            }
                        }
                    }
                }
                for u in usable_ready {
                    usable.push((*u).to_string());
                }
            }
        }
        if !identified {
            checks.push(check(
                "server identity",
                false,
                "port is open but the public endpoint did not confirm it".to_string(),
            ));
        }
    }
    MediaApp {
        id: id.to_string(),
        label: label.to_string(),
        state: if identified {
            AppState::Ready
        } else if port_open {
            AppState::Partial
        } else {
            AppState::Absent
        },
        version,
        checks,
        usable_for: usable,
        needs: needs_missing.to_string(),
        data_path: None,
    }
}

/// Immich: photo server with credential-free ping and version endpoints.
pub fn detect_immich() -> MediaApp {
    detect_server(
        "immich",
        "Immich",
        "http://127.0.0.1:2283",
        "MAGPIECRAFT_IMMICH_URL",
        "/api/server/ping",
        "pong",
        Some("/api/server/version"),
        &["photo thumbnails (API)", "albums and people"],
        "nothing for thumbnails; the API key unlocks uploads later",
    )
}

/// Komga: comics server. Its API needs a login, so an open port is partial
/// until credentials are configured.
pub fn detect_komga() -> MediaApp {
    let mut app = detect_server(
        "komga",
        "Komga",
        "http://127.0.0.1:25600",
        "MAGPIECRAFT_KOMGA_URL",
        "/api/v2/server/status",
        "UP",
        None,
        &[],
        "needs a Komga login (user and password) before the API opens",
    );
    if app.state == AppState::Ready {
        app.state = AppState::Partial;
        app.usable_for = vec!["comic covers and series metadata (after login)".to_string()];
    }
    app
}

/// Kavita: reading server. Its API needs a login, so an open port is partial
/// until credentials are configured.
pub fn detect_kavita() -> MediaApp {
    let mut app = detect_server(
        "kavita",
        "Kavita",
        "http://127.0.0.1:5000",
        "MAGPIECRAFT_KAVITA_URL",
        "/api/health",
        "alive",
        None,
        &[],
        "needs a Kavita login (user and password) before the API opens",
    );
    if app.state == AppState::Ready {
        app.state = AppState::Partial;
        app.usable_for = vec!["comic covers and series metadata (after login)".to_string()];
    }
    app
}

/// All six media apps, slowest (Plex count query) last. Read-only; safe to
/// run on every onboarding open.
pub fn media_apps() -> Vec<MediaApp> {
    vec![
        detect_jellyfin(),
        detect_calibre(),
        detect_immich(),
        detect_komga(),
        detect_kavita(),
        detect_plex(),
    ]
}

/// JSON-friendly report for the CLI, MCP and the onboarding page.
pub fn apps_report() -> serde_json::Value {
    let apps = media_apps();
    let ready = apps.iter().filter(|a| a.state == AppState::Ready).count();
    serde_json::json!({
        "apps": apps,
        "ready": ready,
        "total": apps.len(),
        "note": "absent is normal: it means the app is not installed, not that anything failed",
    })
}

// ------------------------------------------------------------ quick disks

/// One volume, fast: letter, label, size and free space. No SMART, no disk
/// counters, so this answers in well under a second for the onboarding page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickVolume {
    pub letter: String,
    pub label: String,
    pub size: u64,
    pub free: u64,
}

/// Fast volume list for first-run triage. A sick disk must not hang
/// onboarding: the probe is capped well below the full SMART timeout.
pub fn volume_layout() -> ToolResult<Vec<QuickVolume>> {
    const SCRIPT: &str = "Get-Volume | Where-Object { $_.DriveLetter } | ForEach-Object { [pscustomobject]@{ letter = $_.DriveLetter.ToString(); label = $_.FileSystemLabel; size = $_.Size; free = $_.SizeRemaining } } | ConvertTo-Json -Compress";
    let mut child = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", SCRIPT])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| ToolError::Io(e.to_string()))?;
    let mut waited = 0u32;
    let status = loop {
        match child.try_wait().map_err(|e| ToolError::Io(e.to_string()))? {
            Some(status) => break status,
            None => {
                std::thread::sleep(Duration::from_millis(100));
                waited += 1;
                if waited > 150 {
                    let _ = child.kill();
                    return Err(ToolError::Failed(
                        "powershell".to_string(),
                        "volume probe timed out after 15s".to_string(),
                    ));
                }
            }
        }
    };
    let mut stdout = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        use std::io::Read as _;
        let _ = pipe.read_to_string(&mut stdout);
    }
    if !status.success() {
        return Err(ToolError::Failed(
            "powershell".to_string(),
            "volume probe failed".to_string(),
        ));
    }
    let value: serde_json::Value =
        serde_json::from_str(&stdout).map_err(|e| ToolError::Parse(e.to_string()))?;
    let items: Vec<serde_json::Value> = match value {
        serde_json::Value::Array(a) => a,
        single => vec![single],
    };
    let mut out = Vec::new();
    for v in items {
        let letter = v
            .get("letter")
            .and_then(|x| x.as_str())
            .unwrap_or("?")
            .to_string();
        let label = v
            .get("label")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let size = v
            .get("size")
            .and_then(|x| x.as_u64())
            .or_else(|| {
                v.get("size")
                    .and_then(|x| x.as_str())
                    .and_then(|s| s.parse().ok())
            })
            .unwrap_or(0);
        let free = v
            .get("free")
            .and_then(|x| x.as_u64())
            .or_else(|| {
                v.get("free")
                    .and_then(|x| x.as_str())
                    .and_then(|s| s.parse().ok())
            })
            .unwrap_or(0);
        out.push(QuickVolume {
            letter,
            label,
            size,
            free,
        });
    }
    out.sort_by(|a, b| a.letter.cmp(&b.letter));
    Ok(out)
}

/// JSON-friendly quick disk report.
pub fn disks_report() -> serde_json::Value {
    match volume_layout() {
        Ok(volumes) => serde_json::json!({ "ok": true, "volumes": volumes }),
        Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "magpie-apps-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn absent_apps_are_normal_results() {
        // A bogus home means no data and no database whatever else runs, so
        // Plex can never be Ready; whether the port is open decides between
        // Absent and Partial on any machine.
        std::env::set_var("MAGPIECRAFT_PLEX_HOME", "Z:/no-such-plex-home");
        let app = detect_plex();
        assert_ne!(app.state, AppState::Ready);
        let data = app
            .checks
            .iter()
            .find(|c| c.name == "data directory")
            .expect("data check");
        assert!(!data.ok);
        std::env::remove_var("MAGPIECRAFT_PLEX_HOME");
    }

    #[test]
    fn fake_plex_home_reports_partial_without_database() {
        let dir = tmp("plexfake");
        std::env::set_var("MAGPIECRAFT_PLEX_HOME", &dir);
        let app = detect_plex();
        assert_eq!(app.state, AppState::Partial);
        assert!(app
            .checks
            .iter()
            .any(|c| c.name == "data directory" && c.ok));
        assert!(app
            .checks
            .iter()
            .any(|c| c.name == "library database" && !c.ok));
        std::env::remove_var("MAGPIECRAFT_PLEX_HOME");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn fake_plex_token_is_presence_only() {
        let dir = tmp("plextoken");
        let prefs = dir.join("Preferences.xml");
        let mut f = std::fs::File::create(&prefs).unwrap();
        writeln!(
            f,
            "<Preferences PlexOnlineToken=\"SECRET-should-never-appear\"/>"
        )
        .unwrap();
        std::env::set_var("MAGPIECRAFT_PLEX_HOME", &dir);
        let app = detect_plex();
        let token = app
            .checks
            .iter()
            .find(|c| c.name == "server token")
            .expect("token check");
        assert!(token.ok);
        assert!(!token.detail.contains("SECRET"));
        std::env::remove_var("MAGPIECRAFT_PLEX_HOME");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn closed_ports_are_absent_not_errors() {
        assert!(!tcp_open(9, Duration::from_millis(300)));
        std::env::set_var("MAGPIECRAFT_KOMGA_URL", "http://127.0.0.1:9");
        std::env::set_var("MAGPIECRAFT_KAVITA_URL", "http://127.0.0.1:9");
        std::env::set_var("MAGPIECRAFT_IMMICH_URL", "http://127.0.0.1:9");
        std::env::set_var("MAGPIECRAFT_JELLYFIN_URL", "http://127.0.0.1:9");
        for app in [
            detect_komga(),
            detect_kavita(),
            detect_immich(),
            detect_jellyfin(),
        ] {
            assert_eq!(app.state, AppState::Absent, "{}", app.id);
        }
        std::env::remove_var("MAGPIECRAFT_KOMGA_URL");
        std::env::remove_var("MAGPIECRAFT_KAVITA_URL");
        std::env::remove_var("MAGPIECRAFT_IMMICH_URL");
        std::env::remove_var("MAGPIECRAFT_JELLYFIN_URL");
    }

    #[test]
    fn apps_report_counts_ready() {
        let report = apps_report();
        assert_eq!(report["total"], serde_json::json!(6));
        assert!(report["ready"].as_u64().is_some());
    }

    #[test]
    fn quick_disks_list_volumes() {
        let volumes = volume_layout().expect("volume probe works on Windows");
        assert!(!volumes.is_empty());
        assert!(volumes.iter().any(|v| v.letter == "C"));
    }
}
