//! PDF pages, through PdfCraft (`pdfcraft-cli`).
//!
//! Page count comes from `pdfcraft-cli info`; a page comes from
//! `pdfcraft-cli render --page N --dpi 200 --out x.png`, cached as a PNG under the
//! app cache. No PDF library is linked: PdfCraft is the fleet's PDF engine and a
//! separate process, which keeps MagpieCraft MIT/Apache and free of a native PDF
//! dependency.

use crate::helpers::{enforce_cap, Helpers};
use crate::{ComicError, ComicResult};
use std::path::{Path, PathBuf};
use std::process::Command;

fn require_pdfcraft(helpers: &Helpers) -> ComicResult<&Path> {
    match helpers.pdfcraft.as_deref() {
        Some(exe) if exe.is_file() => Ok(exe),
        _ => Err(ComicError::NotConfigured(
            "PdfCraft not found: install it or set MAGPIECRAFT_PDFCRAFT; PDF pages need pdfcraft-cli",
        )),
    }
}

fn run(exe: &Path, args: &[&str]) -> ComicResult<String> {
    let mut cmd = Command::new(exe);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let output = cmd
        .output()
        .map_err(|e| ComicError::Archive(format!("{}: {e}", exe.display())))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ComicError::Archive(format!(
            "pdfcraft-cli failed: {}",
            stderr.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Parse the JSON printed by `pdfcraft-cli info`.
pub fn parse_count(json: &str) -> ComicResult<usize> {
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| ComicError::Parse(format!("pdfcraft-cli info: {e}")))?;
    let pages = value
        .get("pages")
        .and_then(|v| v.as_u64())
        .or_else(|| value.get("page_count").and_then(|v| v.as_u64()))
        .ok_or_else(|| ComicError::Parse("pdfcraft-cli info returned no page count".to_string()))?;
    Ok(usize::try_from(pages).unwrap_or(usize::MAX))
}

/// Page count of a PDF.
pub fn count(path: &Path, helpers: &Helpers) -> ComicResult<usize> {
    let exe = require_pdfcraft(helpers)?;
    let out = run(exe, &["info", &path.display().to_string()])?;
    parse_count(&out)
}

/// Path of a rendered page PNG, rendering it first if needed. `page` is 1-based.
pub fn render(path: &Path, page: usize, helpers: &Helpers) -> ComicResult<PathBuf> {
    let exe = require_pdfcraft(helpers)?;
    let key = helpers.key(path);
    let dir = helpers.render_dir(&key);
    let target = dir.join(format!("{page:04}.png"));
    if target.is_file() {
        return Ok(target);
    }
    std::fs::create_dir_all(&dir).map_err(|e| ComicError::io(&dir, e))?;
    run(
        exe,
        &[
            "render",
            &path.display().to_string(),
            "--page",
            &page.to_string(),
            "--dpi",
            &helpers.pdf_dpi.to_string(),
            "--out",
            &target.display().to_string(),
        ],
    )?;
    if !target.is_file() {
        return Err(ComicError::Archive(format!(
            "pdfcraft-cli reported success but {} was not written",
            target.display()
        )));
    }
    let _ = enforce_cap(&helpers.render_root(), helpers.render_cap_bytes);
    Ok(target)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn page_counts_are_parsed_leniently() {
        assert_eq!(parse_count(r#"{"pages": 226}"#).unwrap(), 226);
        assert_eq!(parse_count(r#"{"page_count": "12"}"#).is_err(), true);
        assert_eq!(parse_count(r#"{"page_count": 12}"#).unwrap(), 12);
        assert!(parse_count("not json").is_err());
        assert!(parse_count(r#"{"nope": 1}"#).is_err());
    }

    #[test]
    fn missing_pdfcraft_is_not_configured() {
        let helpers = Helpers::new(None, None, std::env::temp_dir().join("magpie-none-pdf"));
        let err = count(Path::new("x.pdf"), &helpers).unwrap_err();
        assert_eq!(err.kind(), "not_configured");
        assert!(err.to_string().contains("PdfCraft"));
    }
}
