//! Drive roots and the global intelligent scan.
//!
//! The convention: every media drive holds a level-1 `Multimedia Files`
//! folder, and level 2 holds named collections (`Manga - Nihongo`,
//! `Anime - Series`, `Music - Jazz`, `Video`). `propose_libraries` walks
//! those level-2 folders and proposes one library each, with a kind from the
//! folder name and sample files. Anything the patterns cannot read comes back
//! unkinded for the AI pass or the user's own correction. Nothing here writes
//! anything: proposals are data until the user confirms them into libraries.

use crate::MediaKind;
use serde::{Deserialize, Serialize};

/// One proposed library: a level-2 folder plus a kind guess.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveProposal {
    pub path: String,
    pub name: String,
    pub kind: Option<String>,
    pub confidence: f32,
    pub reason: String,
    pub samples: Vec<String>,
}

/// The scan result: proposals plus the roots that could not be read.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DriveScan {
    pub proposals: Vec<DriveProposal>,
    pub warnings: Vec<String>,
}

/// Kind from a level-2 folder name. Prefix rules first (`Manga - Nihongo`
/// is manga no matter what follows), then contains-fallbacks at lower
/// confidence, then nothing: unknown is honest, never a guess.
pub fn classify_folder(name: &str) -> (Option<MediaKind>, f32, String) {
    let lower = name.trim().to_ascii_lowercase();
    let head = lower.split(" - ").next().unwrap_or(&lower);
    let prefix = |kinds: &[&str]| kinds.iter().any(|k| head.starts_with(k));
    let contains = |kinds: &[&str]| kinds.iter().any(|k| lower.contains(k));
    if prefix(&["comic"]) {
        return (
            Some(MediaKind::Comics),
            0.9,
            "prefix rule: comics".to_string(),
        );
    }
    if prefix(&["manga"]) || contains(&["manga", "nihongo"]) {
        return (
            Some(MediaKind::Manga),
            0.9,
            "prefix rule: manga".to_string(),
        );
    }
    if prefix(&["magazine"]) {
        return (
            Some(MediaKind::Magazines),
            0.9,
            "prefix rule: magazines".to_string(),
        );
    }
    if prefix(&[
        "anime",
        "video",
        "movie",
        "film",
        "tv",
        "series",
        "show",
        "animation",
    ]) {
        return (
            Some(MediaKind::Video),
            0.9,
            "prefix rule: video".to_string(),
        );
    }
    if prefix(&["music", "audio"]) {
        return (
            Some(MediaKind::Audio),
            0.9,
            "prefix rule: audio".to_string(),
        );
    }
    if prefix(&["book", "ebook"]) {
        return (
            Some(MediaKind::Ebook),
            0.9,
            "prefix rule: ebooks".to_string(),
        );
    }
    if prefix(&["photo", "picture", "image"]) {
        return (
            Some(MediaKind::Photo),
            0.9,
            "prefix rule: photos".to_string(),
        );
    }
    if contains(&["anime", "series", "movie", "film", "episode"]) {
        return (
            Some(MediaKind::Video),
            0.6,
            "contains rule: video".to_string(),
        );
    }
    if contains(&["music", "album", "track", "jazz", "composer", "artist"]) {
        return (
            Some(MediaKind::Audio),
            0.6,
            "contains rule: audio".to_string(),
        );
    }
    if contains(&["comic", "manga", "magazine", "book"]) {
        return (
            Some(MediaKind::Comics),
            0.5,
            "contains rule: reading".to_string(),
        );
    }
    (
        None,
        0.0,
        "no pattern matched; AI or manual choice needed".to_string(),
    )
}

/// Up to `max_samples` file names from a folder, one level deep, sorted.
/// Directories are skipped; unreadable folders come back empty, never fatal.
fn sample_files(dir: &std::path::Path, max_samples: usize) -> Vec<String> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return out,
    };
    let mut names: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if out.len() >= max_samples {
                break;
            }
            if let Ok(inner) = std::fs::read_dir(&path) {
                for file in inner.flatten().take(max_samples - out.len()) {
                    if file.path().is_file() {
                        if let Some(name) = file.file_name().to_str() {
                            names.push(name.to_string());
                        }
                    }
                }
            }
        } else if let Some(name) = entry.file_name().to_str() {
            names.push(name.to_string());
        }
        if out.len() + names.len() >= max_samples {
            break;
        }
    }
    out.extend(names.into_iter().take(max_samples));
    out.sort();
    out.truncate(max_samples);
    out
}

/// Walk every root's media folder and propose one library per level-2 folder.
/// Missing roots and unreadable folders become warnings, never errors.
pub fn propose_libraries(roots: &[String], media_folder: &str) -> DriveScan {
    let mut proposals = Vec::new();
    let mut warnings = Vec::new();
    let media_folder = media_folder.trim();
    let media_folder = if media_folder.is_empty() {
        "Multimedia Files"
    } else {
        media_folder
    };
    for root in roots {
        let base = std::path::Path::new(root.trim()).join(media_folder);
        let entries = match std::fs::read_dir(&base) {
            Ok(entries) => entries,
            Err(_) => {
                warnings.push(format!(
                    "cannot read {} (missing drive or folder?)",
                    base.display()
                ));
                continue;
            }
        };
        let mut folders: Vec<std::path::PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        folders.sort();
        if folders.is_empty() {
            warnings.push(format!("{} holds no collection folders", base.display()));
        }
        for folder in folders {
            let name = folder
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string();
            let (kind, confidence, reason) = classify_folder(&name);
            proposals.push(DriveProposal {
                path: folder.display().to_string(),
                name,
                kind: kind.map(|k| k.as_str().to_string()),
                confidence,
                reason,
                samples: sample_files(&folder, 20),
            });
        }
    }
    proposals.sort_by(|a, b| a.path.cmp(&b.path));
    DriveScan {
        proposals,
        warnings,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn sandras_layout_classifies() {
        let cases = [
            ("Manga - Nihongo", Some(MediaKind::Manga)),
            ("Anime - Series", Some(MediaKind::Video)),
            ("Anime - Movies", Some(MediaKind::Video)),
            ("Music - Jazz", Some(MediaKind::Audio)),
            ("Music - Classical", Some(MediaKind::Audio)),
            ("Video", Some(MediaKind::Video)),
            ("TV Series - Thriller", Some(MediaKind::Video)),
            ("Movies - Animation", Some(MediaKind::Video)),
            ("Comics - Franco-Belge", Some(MediaKind::Comics)),
            ("Magazines - Tech", Some(MediaKind::Magazines)),
            ("Books - SciFi", Some(MediaKind::Ebook)),
            ("Photos - 2024", Some(MediaKind::Photo)),
            ("Random Stuff", None),
        ];
        for (name, want) in cases {
            let (kind, confidence, reason) = classify_folder(name);
            assert_eq!(kind, want, "{name}: {reason}");
            if want.is_some() {
                assert!(confidence >= 0.6, "{name}");
                assert!(!reason.is_empty());
            } else {
                assert_eq!(confidence, 0.0);
            }
        }
    }

    #[test]
    fn missing_roots_warn_instead_of_failing() {
        let scan = propose_libraries(&["Z:/Nope".to_string()], "Multimedia Files");
        assert!(scan.proposals.is_empty());
        assert_eq!(scan.warnings.len(), 1);
    }

    #[test]
    fn level_two_folders_propose_with_samples() {
        let home = std::env::temp_dir().join(format!("magpie-drives-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let base = home.join("Multimedia Files");
        std::fs::create_dir_all(base.join("Manga - Nihongo")).unwrap();
        std::fs::create_dir_all(base.join("Anime - Series")).unwrap();
        std::fs::write(base.join("Manga - Nihongo").join("v01.cbz"), b"x").unwrap();
        std::fs::write(base.join("notes.txt"), b"level one file").unwrap();
        let scan = propose_libraries(&[home.display().to_string()], "Multimedia Files");
        assert_eq!(scan.proposals.len(), 2);
        let manga = scan
            .proposals
            .iter()
            .find(|p| p.name == "Manga - Nihongo")
            .unwrap();
        assert_eq!(manga.kind.as_deref(), Some("manga"));
        assert_eq!(manga.samples, vec!["v01.cbz".to_string()]);
        let _ = std::fs::remove_dir_all(&home);
    }
}
