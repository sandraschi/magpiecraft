//! CBR / RAR / CB7 / 7Z page source, through the 7-Zip subprocess.
//!
//! Listing pages uses `7z l -slt -ba`, which reads the archive index without
//! unpacking anything. Reading a page extracts the archive once into the app cache
//! (solid RAR cannot give you page 200 without unpacking what is in front of it),
//! then serves pages as plain files. A cross-process lock file stops two readers
//! from extracting the same archive at the same time, and a size cap evicts the
//! oldest extractions so the cache cannot grow without bound.

use crate::helpers::{enforce_cap, Helpers};
use crate::{ComicError, ComicResult};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

/// How long to wait for another process that is already extracting the same file.
const LOCK_WAIT_MS: u64 = 180_000;

fn run(exe: &Path, args: &[&str]) -> ComicResult<String> {
    let mut cmd = Command::new(exe);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let output = cmd
        .output()
        .map_err(|e| ComicError::Archive(format!("{}: {e}", exe.display())))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ComicError::Archive(format!(
            "7-Zip failed on {}: {}",
            args.last().copied().unwrap_or(""),
            stderr.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn require_seven_zip(helpers: &Helpers) -> ComicResult<&Path> {
    match helpers.seven_zip.as_deref() {
        Some(exe) if exe.is_file() => Ok(exe),
        _ => Err(ComicError::NotConfigured(
            "7-Zip not found: install it or set MAGPIECRAFT_7Z; CBR/RAR pages need it",
        )),
    }
}

/// Page entries of an archive, in natural order. An index read, not an extraction.
pub fn list(archive: &Path, helpers: &Helpers) -> ComicResult<Vec<String>> {
    let exe = require_seven_zip(helpers)?;
    let text = run(exe, &["l", "-slt", "-ba", &archive.display().to_string()])?;
    let mut pages: Vec<String> = Vec::new();
    let mut current_path: Option<String> = None;
    let mut current_is_dir = false;
    for line in text.lines() {
        let line = line.trim_end();
        if let Some(value) = line.strip_prefix("Path = ") {
            if let Some(path) = current_path.take() {
                if !current_is_dir && is_page(&path) {
                    pages.push(path);
                }
            }
            current_path = Some(value.replace('\\', "/"));
            current_is_dir = false;
        } else if let Some(value) = line.strip_prefix("Attributes = ") {
            current_is_dir = value.contains('D');
        } else if line.starts_with("----------") {
            if let Some(path) = current_path.take() {
                if !current_is_dir && is_page(&path) {
                    pages.push(path);
                }
            }
            current_is_dir = false;
        }
    }
    if let Some(path) = current_path.take() {
        if !current_is_dir && is_page(&path) {
            pages.push(path);
        }
    }
    pages.sort_by(|a, b| magpiecraft_scan::natural_key(a).cmp(&magpiecraft_scan::natural_key(b)));
    Ok(pages)
}

fn is_page(name: &str) -> bool {
    if name.contains("__MACOSX") {
        return false;
    }
    let base = name.rsplit('/').next().unwrap_or(name);
    if base.starts_with('.') {
        return false;
    }
    magpiecraft_scan::is_image_name(base)
}

/// Extract the archive into the cache (once) and return the directory.
pub fn extract(archive: &Path, helpers: &Helpers) -> ComicResult<PathBuf> {
    let exe = require_seven_zip(helpers)?;
    let key = helpers.key(archive);
    let out = helpers.extract_dir(&key);
    let done = out.join(".done");
    if done.is_file() {
        return Ok(out);
    }
    std::fs::create_dir_all(&out)
        .map_err(|e| ComicError::Io(out.display().to_string(), e.to_string()))?;

    let lock_path = out.join(".lock");
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
    {
        Ok(mut lock) => {
            let _ = writeln!(lock, "{}", std::process::id());
            let result = run(
                exe,
                &[
                    "x",
                    "-y",
                    "-bso0",
                    "-bsp0",
                    &format!("-o{}", out.display()),
                    &archive.display().to_string(),
                ],
            );
            match result {
                Ok(_) => {
                    if let Ok(mut f) = std::fs::File::create(&done) {
                        let _ = writeln!(f, "ok");
                    }
                    let _ = std::fs::remove_file(&lock_path);
                    let _ = enforce_cap(&helpers.extract_root(), helpers.extract_cap_bytes);
                    Ok(out)
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&lock_path);
                    let _ = std::fs::remove_dir_all(&out);
                    Err(e)
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // Another process is extracting this archive: wait for its marker.
            let deadline =
                std::time::Instant::now() + std::time::Duration::from_millis(LOCK_WAIT_MS);
            while std::time::Instant::now() < deadline {
                if done.is_file() {
                    return Ok(out);
                }
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            Err(ComicError::Archive(format!(
                "timed out waiting for another process to extract {}",
                archive.display()
            )))
        }
        Err(e) => Err(ComicError::Io(out.display().to_string(), e.to_string())),
    }
}

/// Bytes of one page, extracted on demand.
pub fn read(archive: &Path, name: &str, helpers: &Helpers) -> ComicResult<Vec<u8>> {
    let dir = extract(archive, helpers)?;
    let mut target = dir.clone();
    for part in name.split('/').filter(|p| !p.is_empty()) {
        target.push(part);
    }
    let meta = std::fs::metadata(&target).map_err(|e| ComicError::io(&target, e))?;
    if meta.len() > crate::pageset::MAX_PAGE_BYTES {
        return Err(ComicError::Archive(format!(
            "{name}: page is {} bytes, over the ceiling",
            meta.len()
        )));
    }
    std::fs::read(&target).map_err(|e| ComicError::io(&target, e))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::helpers::Helpers;
    use magpiecraft_tools_find::find_seven_zip;

    mod magpiecraft_tools_find {
        // The comics crate does not depend on the tools crate; a tiny local search
        // keeps the test hermetic and skips when 7-Zip is not installed.
        use std::path::PathBuf;
        pub fn find_seven_zip() -> Option<PathBuf> {
            if let Ok(explicit) = std::env::var("MAGPIECRAFT_7Z") {
                let p = PathBuf::from(explicit);
                if p.is_file() {
                    return Some(p);
                }
            }
            for name in ["7z.exe", "7za.exe"] {
                if let Some(path) = std::env::var_os("PATH") {
                    for dir in std::env::split_paths(&path) {
                        let candidate = dir.join(name);
                        if candidate.is_file() {
                            return Some(candidate);
                        }
                    }
                }
            }
            let fallback = PathBuf::from("C:/Program Files/7-Zip/7z.exe");
            if fallback.is_file() {
                return Some(fallback);
            }
            None
        }
    }

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "magpie-rar-{}-{}-{}",
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

    /// 7-Zip reads ZIP as happily as RAR, so a ZIP renamed to .cbr exercises the
    /// same subprocess path without needing a RAR encoder in the build.
    fn write_cbr(path: &Path, pages: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        for (name, bytes) in pages {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn missing_seven_zip_is_not_configured_not_empty() {
        let helpers = Helpers::new(None, None, std::env::temp_dir().join("magpie-none"));
        match list(Path::new("x.cbr"), &helpers) {
            Err(e) => assert_eq!(e.kind(), "not_configured"),
            Ok(_) => panic!("must not pretend to list without a tool"),
        }
    }

    #[test]
    fn lists_and_reads_pages_through_seven_zip() {
        let Some(seven) = find_seven_zip() else {
            eprintln!("7-Zip not installed: skipping the extraction test");
            return;
        };
        let dir = tmp("read");
        let archive = dir.join("vol.cbr");
        write_cbr(
            &archive,
            &[
                ("p10.jpg", b"ten"),
                ("p2.jpg", b"two"),
                ("p1.jpg", b"one"),
                ("notes.txt", b"skip"),
            ],
        );
        let helpers = Helpers::new(Some(seven), None, dir.join("cache"));
        let pages = list(&archive, &helpers).unwrap();
        assert_eq!(pages, vec!["p1.jpg", "p2.jpg", "p10.jpg"]);
        assert_eq!(read(&archive, "p2.jpg", &helpers).unwrap(), b"two");
        // Second read reuses the extraction.
        assert_eq!(read(&archive, "p10.jpg", &helpers).unwrap(), b"ten");
        let extracted = helpers.extract_dir(&helpers.key(&archive));
        assert!(extracted.join(".done").is_file());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
