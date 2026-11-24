//! Integrity: checksum manifests, verification and exact duplicates.
//!
//! Bitrot is silent, so a manifest is only useful if it was written when the
//! files were known good and can be re-checked later without the original tree.
//! The format is three tab separated fields, one file per line:
//!
//! ```text
//! <blake3 hex>  <size in bytes>  <path relative to the manifest root>
//! ```
//!
//! Tabs, not spaces, are the field separator, so file names with spaces need no
//! quoting or escaping.

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};

pub type HealthResult<T> = Result<T, HealthError>;

#[derive(Debug, thiserror::Error)]
pub enum HealthError {
    #[error("io: {0}: {1}")]
    Io(String, String),
    #[error("manifest: {0}")]
    Manifest(String),
}

impl HealthError {
    pub fn kind(&self) -> &'static str {
        match self {
            HealthError::Io(_, _) => "io",
            HealthError::Manifest(_) => "parse",
        }
    }

    pub fn io(path: &Path, e: std::io::Error) -> Self {
        HealthError::Io(path.display().to_string(), e.to_string())
    }
}

/// BLAKE3 of a byte slice, hex encoded, with the length that was hashed.
pub fn hash_bytes(bytes: &[u8]) -> (String, u64) {
    let mut hasher = blake3::Hasher::new();
    hasher.update(bytes);
    (hasher.finalize().to_hex().to_string(), bytes.len() as u64)
}

/// BLAKE3 of a file, read in 256 KiB blocks so a 4 GB video does not fill RAM.
pub fn hash_file(path: &Path) -> HealthResult<(String, u64)> {
    let mut file = std::fs::File::open(path).map_err(|e| HealthError::io(path, e))?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut total = 0u64;
    loop {
        let n = file.read(&mut buf).map_err(|e| HealthError::io(path, e))?;
        if n == 0 {
            break;
        }
        match buf.get(..n) {
            Some(chunk) => {
                hasher.update(chunk);
            }
            None => {
                return Err(HealthError::Manifest(format!(
                    "short read on {}",
                    path.display()
                )))
            }
        }
        total = total.saturating_add(n as u64);
    }
    Ok((hasher.finalize().to_hex().to_string(), total))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestLine {
    pub hash: String,
    pub size: u64,
    pub rel: String,
}

impl ManifestLine {
    pub fn to_line(&self) -> String {
        format!("{}\t{}\t{}", self.hash, self.size, self.rel)
    }

    pub fn parse(line: &str) -> Option<Self> {
        let mut parts = line.splitn(3, '\t');
        let hash = parts.next()?.trim();
        let size = parts.next()?.trim().parse::<u64>().ok()?;
        let rel = parts.next()?.trim();
        if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) || rel.is_empty() {
            return None;
        }
        Some(ManifestLine {
            hash: hash.to_ascii_lowercase(),
            size,
            rel: rel.replace('\\', "/"),
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManifestReport {
    pub root: String,
    pub manifest: String,
    pub files: usize,
    pub bytes: u64,
}

/// Hash every file under `root` and write the manifest. Read-only on the tree.
pub fn write_manifest(root: &Path, manifest: &Path) -> HealthResult<ManifestReport> {
    let walk = magpiecraft_scan::walk(
        root,
        &magpiecraft_scan::WalkOptions {
            min_pages: 3,
            follow_links: false,
            limit: 0,
        },
    )
    .map_err(|e| HealthError::Manifest(e.to_string()))?;
    let mut lines: Vec<ManifestLine> = Vec::with_capacity(walk.files.len());
    let mut bytes = 0u64;
    for f in &walk.files {
        let full = root.join(f.rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        let (hash, size) = hash_file(&full)?;
        bytes = bytes.saturating_add(size);
        lines.push(ManifestLine {
            hash,
            size,
            rel: f.rel.clone(),
        });
    }
    lines.sort_by(|a, b| a.rel.cmp(&b.rel));
    let mut body = String::with_capacity(lines.len() * 100);
    body.push_str("# magpiecraft manifest v1: blake3, size, relative path (tab separated)\n");
    for line in &lines {
        body.push_str(&line.to_line());
        body.push('\n');
    }
    if let Some(dir) = manifest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| HealthError::io(dir, e))?;
    }
    std::fs::write(manifest, body).map_err(|e| HealthError::io(manifest, e))?;
    Ok(ManifestReport {
        root: root.display().to_string(),
        manifest: manifest.display().to_string(),
        files: lines.len(),
        bytes,
    })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VerifyReport {
    pub checked: usize,
    pub ok: usize,
    pub missing: Vec<String>,
    pub changed: Vec<String>,
    /// Files that appeared after the manifest was written.
    pub unknown: Vec<String>,
    pub manifest_lines: usize,
    pub bad_lines: usize,
}

impl VerifyReport {
    pub fn clean(&self) -> bool {
        self.missing.is_empty() && self.changed.is_empty()
    }
}

/// Re-hash every file named by a manifest and report what changed.
pub fn verify_manifest(root: &Path, manifest: &Path) -> HealthResult<VerifyReport> {
    let body = std::fs::read_to_string(manifest).map_err(|e| HealthError::io(manifest, e))?;
    let mut report = VerifyReport::default();
    let mut listed: Vec<String> = Vec::new();
    for line in body.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let entry = match ManifestLine::parse(line) {
            Some(e) => e,
            None => {
                report.bad_lines += 1;
                continue;
            }
        };
        listed.push(entry.rel.clone());
        report.checked += 1;
        let full = root.join(entry.rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        match std::fs::metadata(&full) {
            Ok(meta) if meta.len() != entry.size => {
                report.changed.push(entry.rel);
            }
            Ok(_) => match hash_file(&full) {
                Ok((hash, _)) if hash == entry.hash => report.ok += 1,
                Ok(_) => report.changed.push(entry.rel),
                Err(_) => report.changed.push(entry.rel),
            },
            Err(_) => report.missing.push(entry.rel),
        }
    }
    report.manifest_lines = report.checked + report.bad_lines;
    if let Ok(walk) = magpiecraft_scan::walk(
        root,
        &magpiecraft_scan::WalkOptions {
            min_pages: 3,
            follow_links: false,
            limit: 0,
        },
    ) {
        for f in walk.files {
            if !listed.contains(&f.rel) {
                report.unknown.push(f.rel);
            }
        }
    }
    Ok(report)
}

/// One group of byte-identical files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DupGroup {
    pub size: u64,
    pub hash: String,
    pub paths: Vec<String>,
}

/// Exact duplicates: group by size first (cheap), hash only the groups with more
/// than one member. Returns groups of two or more files with different paths.
pub fn group_duplicates(root: &Path, candidates: &[(String, u64)]) -> HealthResult<Vec<DupGroup>> {
    use std::collections::HashMap;
    let mut by_size: HashMap<u64, Vec<&str>> = HashMap::new();
    for (rel, size) in candidates {
        if *size > 0 {
            by_size.entry(*size).or_default().push(rel.as_str());
        }
    }
    let mut out: Vec<DupGroup> = Vec::new();
    for (size, rels) in by_size {
        if rels.len() < 2 {
            continue;
        }
        let mut by_hash: HashMap<String, Vec<String>> = HashMap::new();
        for rel in rels {
            let full = root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            if let Ok((hash, _)) = hash_file(&full) {
                by_hash.entry(hash).or_default().push(rel.to_string());
            }
        }
        for (hash, mut paths) in by_hash {
            if paths.len() < 2 {
                continue;
            }
            paths.sort();
            out.push(DupGroup { size, hash, paths });
        }
    }
    out.sort_by(|a, b| {
        b.paths
            .len()
            .cmp(&a.paths.len())
            .then(a.paths.cmp(&b.paths))
    });
    Ok(out)
}

/// Path to a per-library manifest file inside the app data directory.
pub fn manifest_path(data_dir: &Path, library: &str) -> PathBuf {
    let safe: String = library
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    data_dir
        .join("manifests")
        .join(format!("{safe}.blake3.txt"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "magpie-health-{}-{}-{}",
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
    fn manifest_roundtrip_and_verify() {
        let root = tmp("manifest");
        std::fs::write(root.join("a.txt"), b"alpha").unwrap();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/b bin.dat"), b"beta").unwrap();
        let manifest = root.with_extension("blake3.txt");
        let report = write_manifest(&root, &manifest).unwrap();
        assert_eq!(report.files, 2);
        assert_eq!(report.bytes, 9);
        let verify = verify_manifest(&root, &manifest).unwrap();
        assert!(verify.clean());
        assert_eq!(verify.ok, 2);
        assert_eq!(verify.checked, 2);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn verify_detects_change_and_loss() {
        let root = tmp("verify");
        std::fs::write(root.join("a.txt"), b"alpha").unwrap();
        std::fs::write(root.join("b.txt"), b"beta").unwrap();
        let manifest = root.with_extension("blake3.txt");
        write_manifest(&root, &manifest).unwrap();
        std::fs::write(root.join("a.txt"), b"ALPHA").unwrap();
        std::fs::remove_file(root.join("b.txt")).unwrap();
        std::fs::write(root.join("c.txt"), b"new").unwrap();
        let v = verify_manifest(&root, &manifest).unwrap();
        assert!(!v.clean());
        assert_eq!(v.changed, vec!["a.txt"]);
        assert_eq!(v.missing, vec!["b.txt"]);
        assert_eq!(v.unknown, vec!["c.txt"]);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn same_size_is_not_enough_for_a_duplicate() {
        let root = tmp("dupes");
        std::fs::write(root.join("a.cbz"), b"AAAA").unwrap();
        std::fs::write(root.join("b.cbz"), b"BBBB").unwrap();
        std::fs::write(root.join("c.cbz"), b"AAAA").unwrap();
        let candidates = vec![
            ("a.cbz".to_string(), 4u64),
            ("b.cbz".to_string(), 4u64),
            ("c.cbz".to_string(), 4u64),
        ];
        let groups = group_duplicates(&root, &candidates).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].paths, vec!["a.cbz", "c.cbz"]);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn manifest_lines_are_strict() {
        assert!(ManifestLine::parse("not a line").is_none());
        assert!(ManifestLine::parse("zzz\t5\ta").is_none());
        let line = ManifestLine {
            hash: "a".repeat(64),
            size: 3,
            rel: "x/y.cbz".to_string(),
        };
        assert_eq!(ManifestLine::parse(&line.to_line()), Some(line));
    }
}
