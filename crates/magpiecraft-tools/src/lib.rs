//! External binaries, invoked as subprocesses and never linked.
//!
//! ffprobe (media inspection), 7-Zip (CBR/RAR extraction, Tier 2) and PdfCraft
//! (`pdfcraft-cli`, page count and page render) are separate programs. MagpieCraft
//! stays MIT/Apache licensed, works without any of them, and reports each one's
//! absence in Health instead of failing silently.
//!
//! Discovery order: explicit environment variable, then PATH, then the documented
//! install location.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

pub type ToolResult<T> = Result<T, ToolError>;

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("tool not found: {0}")]
    Missing(String),
    #[error("tool failed: {0}: {1}")]
    Failed(String, String),
    #[error("parse: {0}")]
    Parse(String),
    #[error("io: {0}")]
    Io(String),
}

impl ToolError {
    pub fn kind(&self) -> &'static str {
        match self {
            ToolError::Missing(_) => "not_configured",
            ToolError::Failed(_, _) => "tool",
            ToolError::Parse(_) => "parse",
            ToolError::Io(_) => "io",
        }
    }
}

/// Env vars that override discovery, checked first.
pub const ENV_FFPROBE: &str = "MAGPIECRAFT_FFPROBE";
pub const ENV_SEVEN_ZIP: &str = "MAGPIECRAFT_7Z";
pub const ENV_PDFCRAFT: &str = "MAGPIECRAFT_PDFCRAFT";

/// Resolve an executable: absolute path from `env`, else PATH, else the first
/// existing candidate in `fallbacks`.
pub fn find_tool(env: &str, names: &[&str], fallbacks: &[&str]) -> Option<PathBuf> {
    if let Ok(value) = std::env::var(env) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            let p = PathBuf::from(trimmed);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    for name in names {
        if let Some(p) = which(name) {
            return Some(p);
        }
    }
    for candidate in fallbacks {
        let p = PathBuf::from(candidate);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Minimal `which`: PATH directories, Windows executable extensions.
pub fn which(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) {
        let pathext =
            std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".to_string());
        let mut v: Vec<String> = pathext
            .split(';')
            .map(|e| e.trim().to_ascii_lowercase())
            .filter(|e| !e.is_empty())
            .collect();
        v.insert(0, String::new());
        v
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&path_var) {
        for ext in &exts {
            let candidate = if ext.is_empty() {
                dir.join(name)
            } else {
                dir.join(format!("{name}{ext}"))
            };
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// The three binaries MagpieCraft can use, with where they were found.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Toolchain {
    pub ffprobe: Option<PathBuf>,
    pub seven_zip: Option<PathBuf>,
    pub pdfcraft: Option<PathBuf>,
}

impl Toolchain {
    pub fn detect() -> Self {
        Toolchain {
            ffprobe: find_tool(ENV_FFPROBE, &["ffprobe"], &[]),
            seven_zip: find_tool(
                ENV_SEVEN_ZIP,
                &["7z", "7za", "7zr"],
                &["C:/Program Files/7-Zip/7z.exe"],
            ),
            pdfcraft: find_tool(
                ENV_PDFCRAFT,
                &["pdfcraft-cli"],
                &["C:/Program Files/PdfCraft/pdfcraft-cli.exe"],
            ),
        }
    }

    /// JSON-friendly report for `status` and the Health page.
    pub fn report(&self) -> serde_json::Value {
        let entry = |path: &Option<PathBuf>, role: &str, needed_for: &str| match path {
            Some(p) => serde_json::json!({
                "found": true,
                "path": p.display().to_string(),
                "role": role,
                "needed_for": needed_for,
            }),
            None => serde_json::json!({
                "found": false,
                "path": null,
                "role": role,
                "needed_for": needed_for,
                "note": "optional; the feature that needs it reports not_configured",
            }),
        };
        serde_json::json!({
            "ffprobe": entry(&self.ffprobe, "technical inspection", "codec, resolution and stream details for video and audio"),
            "seven_zip": entry(&self.seven_zip, "CBR/RAR extraction", "reading CBR, RAR, CB7 and 7Z archives (Tier 2)"),
            "pdfcraft": entry(&self.pdfcraft, "PDF pages", "PDF page count and page rendering, via pdfcraft-cli (Tier 2)"),
        })
    }

    pub fn missing(&self) -> Vec<&'static str> {
        let mut v = Vec::new();
        if self.ffprobe.is_none() {
            v.push("ffprobe");
        }
        if self.seven_zip.is_none() {
            v.push("seven_zip");
        }
        if self.pdfcraft.is_none() {
            v.push("pdfcraft");
        }
        v
    }
}

fn command_for(path: &Path) -> Command {
    let mut cmd = Command::new(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

fn run(path: &Path, args: &[&str]) -> ToolResult<String> {
    let label = path.display().to_string();
    let output = command_for(path)
        .args(args)
        .output()
        .map_err(|e| ToolError::Failed(label.clone(), e.to_string()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ToolError::Failed(
            label,
            format!("exit {:?}: {}", output.status.code(), stderr.trim()),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// ---------------------------------------------------------------- ffprobe
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StreamInfo {
    pub index: u32,
    pub codec_type: String,
    pub codec_name: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub channels: Option<u32>,
    pub sample_rate: Option<u32>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MediaInfo {
    pub format_name: String,
    pub duration_seconds: Option<f64>,
    pub size_bytes: Option<u64>,
    pub bit_rate: Option<u64>,
    pub streams: Vec<StreamInfo>,
}

/// Parse `ffprobe -print_format json -show_format -show_streams` output.
pub fn parse_ffprobe(json: &str) -> ToolResult<MediaInfo> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| ToolError::Parse(e.to_string()))?;
    let format = value
        .get("format")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let mut info = MediaInfo {
        format_name: format
            .get("format_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        duration_seconds: format
            .get("duration")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok()),
        size_bytes: format
            .get("size")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok()),
        bit_rate: format
            .get("bit_rate")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok()),
        streams: Vec::new(),
    };
    if let Some(streams) = value.get("streams").and_then(|v| v.as_array()) {
        for s in streams {
            info.streams.push(StreamInfo {
                index: s.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                codec_type: s
                    .get("codec_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                codec_name: s
                    .get("codec_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                width: s.get("width").and_then(|v| v.as_u64()).map(|v| v as u32),
                height: s.get("height").and_then(|v| v.as_u64()).map(|v| v as u32),
                channels: s.get("channels").and_then(|v| v.as_u64()).map(|v| v as u32),
                sample_rate: s
                    .get("sample_rate")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<u32>().ok()),
                language: s
                    .get("tags")
                    .and_then(|t| t.get("language"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
            });
        }
    }
    Ok(info)
}

pub fn ffprobe(tool: &Toolchain, path: &Path) -> ToolResult<MediaInfo> {
    let exe = tool
        .ffprobe
        .as_ref()
        .ok_or_else(|| ToolError::Missing("ffprobe".to_string()))?;
    let out = run(
        exe,
        &[
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            &path.display().to_string(),
        ],
    )?;
    parse_ffprobe(&out)
}

// ---------------------------------------------------------------- PdfCraft
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PdfInfo {
    pub pages: u32,
    pub path: String,
}

/// Parse the JSON printed by `pdfcraft-cli info <file>`.
pub fn parse_pdf_info(json: &str) -> ToolResult<PdfInfo> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| ToolError::Parse(e.to_string()))?;
    let pages = value
        .get("pages")
        .and_then(|v| v.as_u64())
        .or_else(|| value.get("page_count").and_then(|v| v.as_u64()))
        .ok_or_else(|| {
            ToolError::Parse("no pages field in pdfcraft-cli info output".to_string())
        })?;
    Ok(PdfInfo {
        pages: u32::try_from(pages).unwrap_or(u32::MAX),
        path: value
            .get("path")
            .or_else(|| value.get("file"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

pub fn pdf_info(tool: &Toolchain, path: &Path) -> ToolResult<PdfInfo> {
    let exe = tool
        .pdfcraft
        .as_ref()
        .ok_or_else(|| ToolError::Missing("pdfcraft-cli".to_string()))?;
    let out = run(exe, &["info", &path.display().to_string()])?;
    parse_pdf_info(&out)
}

/// Render one PDF page to a PNG with PdfCraft. `page` is 1-based.
pub fn pdf_render(
    tool: &Toolchain,
    path: &Path,
    page: u32,
    dpi: u32,
    out: &Path,
) -> ToolResult<PathBuf> {
    let exe = tool
        .pdfcraft
        .as_ref()
        .ok_or_else(|| ToolError::Missing("pdfcraft-cli".to_string()))?;
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| ToolError::Io(e.to_string()))?;
    }
    run(
        exe,
        &[
            "render",
            &path.display().to_string(),
            "--page",
            &page.to_string(),
            "--dpi",
            &dpi.to_string(),
            "--out",
            &out.display().to_string(),
        ],
    )?;
    if out.is_file() {
        Ok(out.to_path_buf())
    } else {
        Err(ToolError::Failed(
            "pdfcraft-cli render".to_string(),
            format!("reported success but {} does not exist", out.display()),
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    const FFPROBE: &str = r#"{
      "streams": [
        {"index": 0, "codec_type": "video", "codec_name": "hevc", "width": 1920, "height": 1080},
        {"index": 1, "codec_type": "audio", "codec_name": "aac", "channels": 6, "sample_rate": "48000", "tags": {"language": "eng"}}
      ],
      "format": {"format_name": "matroska,webm", "duration": "5421.250000", "size": "3221225472", "bit_rate": "4750000"}
    }"#;

    #[test]
    fn ffprobe_json_becomes_media_info() {
        let info = parse_ffprobe(FFPROBE).unwrap();
        assert_eq!(info.format_name, "matroska,webm");
        assert_eq!(info.duration_seconds, Some(5421.25));
        assert_eq!(info.size_bytes, Some(3_221_225_472));
        assert_eq!(info.streams.len(), 2);
        let video = info.streams.first().unwrap();
        assert_eq!(video.codec_name, "hevc");
        assert_eq!(video.width, Some(1920));
        let audio = info.streams.get(1).unwrap();
        assert_eq!(audio.language.as_deref(), Some("eng"));
        assert_eq!(audio.channels, Some(6));
    }

    #[test]
    fn ffprobe_json_without_streams_is_not_an_error() {
        let info = parse_ffprobe(r#"{"format":{"format_name":"mp3"}}"#).unwrap();
        assert_eq!(info.format_name, "mp3");
        assert!(info.streams.is_empty());
        assert!(parse_ffprobe("not json").is_err());
    }

    #[test]
    fn pdfcraft_info_is_read() {
        let info = parse_pdf_info(r#"{"pages": 226, "path": "x.pdf"}"#).unwrap();
        assert_eq!(info.pages, 226);
        assert!(parse_pdf_info(r#"{"nope": 1}"#).is_err());
    }

    #[test]
    fn missing_tool_is_reported_not_guessed() {
        let empty = Toolchain::default();
        assert_eq!(
            ffprobe(&empty, Path::new("x.mkv")).unwrap_err().kind(),
            "not_configured"
        );
        assert_eq!(empty.missing().len(), 3);
        let report = empty.report();
        assert_eq!(report["ffprobe"]["found"], serde_json::json!(false));
    }

    #[test]
    fn which_does_not_invent_paths() {
        assert!(which("magpiecraft-surely-not-a-real-binary").is_none());
    }
}
