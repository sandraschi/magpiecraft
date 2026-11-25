//! ffmpeg presets (pillar D).
//!
//! ffmpeg is invoked as a subprocess and never linked: the repository stays
//! MIT/Apache and the binary is optional. Each preset is a named, fixed argument
//! list, so a plan can show exactly what will run before anything is encoded.
//!
//! Every preset writes a new file. Nothing overwrites its input, and a re-encode
//! never lands on top of an existing file. Undo deletes the output.

use crate::catalog::Catalog;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// One named ffmpeg recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    /// Output container extension.
    pub extension: &'static str,
    /// True when the streams are copied, not re-encoded: fast and lossless.
    pub lossless: bool,
}

pub const PRESETS: &[Preset] = &[
    Preset {
        id: "remux-mkv",
        label: "Remux to MKV",
        description: "Copy every stream into Matroska. No re-encode, so it is fast and nothing is lost.",
        extension: "mkv",
        lossless: true,
    },
    Preset {
        id: "mp4-compat",
        label: "MP4 for compatibility",
        description: "Re-encode video to H.264 CRF 20 and audio to AAC 192k, faststart. Plays anywhere.",
        extension: "mp4",
        lossless: false,
    },
    Preset {
        id: "loudnorm",
        label: "Normalise loudness",
        description: "Copy video, re-encode audio to AAC 192k through EBU R128 loudness normalisation (-16 LUFS).",
        extension: "mkv",
        lossless: false,
    },
    Preset {
        id: "strip-metadata",
        label: "Strip metadata",
        description: "Copy the streams, drop every metadata tag. Useful before sharing a file.",
        extension: "mkv",
        lossless: true,
    },
];

pub fn get(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id.trim())
}

/// The exact ffmpeg arguments for one preset. `from` and `to` are absolute paths.
pub fn encode_args(preset: &Preset, from: &Path, to: &Path) -> Vec<String> {
    let input = from.display().to_string();
    let output = to.display().to_string();
    let mut args: Vec<String> = vec!["-i".into(), input];
    match preset.id {
        "remux-mkv" => {
            args.extend(["-map", "0", "-c", "copy", "-f", "matroska"].map(String::from));
        }
        "mp4-compat" => {
            args.extend(
                [
                    "-c:v",
                    "libx264",
                    "-crf",
                    "20",
                    "-preset",
                    "medium",
                    "-c:a",
                    "aac",
                    "-b:a",
                    "192k",
                    "-movflags",
                    "+faststart",
                ]
                .map(String::from),
            );
        }
        "loudnorm" => {
            args.extend(
                [
                    "-c:v",
                    "copy",
                    "-af",
                    "loudnorm=I=-16:TP=-1.5:LRA=11",
                    "-c:a",
                    "aac",
                    "-b:a",
                    "192k",
                ]
                .map(String::from),
            );
        }
        "strip-metadata" => {
            args.extend(["-map_metadata", "-1", "-c", "copy", "-f", "matroska"].map(String::from));
        }
        _ => {}
    }
    args.push(output);
    args
}

/// Where a conversion would write by default: next to the source, suffixed with the
/// preset id, so two presets on one film never collide.
pub fn default_output(source: &Path, preset: &Preset) -> PathBuf {
    let parent = source.parent().unwrap_or_else(|| Path::new(""));
    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());
    parent.join(format!("{stem}.{}.{}", preset.id, preset.extension))
}

impl Catalog {
    /// The preset catalogue, with whether ffmpeg is available.
    pub fn presets(&self) -> Value {
        let ffmpeg = self
            .tools()
            .ffmpeg
            .as_ref()
            .map(|p| p.display().to_string());
        json!({
            "ffmpeg": ffmpeg,
            "available": self.tools().ffmpeg.is_some(),
            "presets": PRESETS.iter().map(|p| json!({
                "id": p.id,
                "label": p.label,
                "description": p.description,
                "extension": p.extension,
                "lossless": p.lossless,
            })).collect::<Vec<_>>(),
            "note": "every preset writes a new file; nothing overwrites and undo deletes the output",
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_remux_copies_streams_and_names_the_container() {
        let preset = get("remux-mkv").unwrap();
        assert!(preset.lossless);
        let args = encode_args(
            preset,
            Path::new("D:/in/film.mp4"),
            Path::new("D:/out/film.remux-mkv.mkv"),
        );
        assert_eq!(args[0], "-i");
        assert!(args.contains(&"-c".to_string()));
        assert!(args.contains(&"copy".to_string()));
        assert!(args.contains(&"matroska".to_string()));
        assert!(!args.contains(&"libx264".to_string()));
        assert_eq!(args.last().unwrap(), "D:/out/film.remux-mkv.mkv");
    }

    #[test]
    fn a_compatibility_encode_re_encodes_and_faststarts() {
        let args = encode_args(
            get("mp4-compat").unwrap(),
            Path::new("in.mkv"),
            Path::new("out.mp4"),
        );
        assert!(args.contains(&"libx264".to_string()));
        assert!(args.contains(&"aac".to_string()));
        assert!(args.contains(&"+faststart".to_string()));
        assert!(!get("mp4-compat").unwrap().lossless);
    }

    #[test]
    fn loudness_normalisation_copies_video_and_re_encodes_audio() {
        let args = encode_args(
            get("loudnorm").unwrap(),
            Path::new("in.mkv"),
            Path::new("out.mkv"),
        );
        assert!(args.contains(&"loudnorm=I=-16:TP=-1.5:LRA=11".to_string()));
        assert!(
            args.windows(3).any(|w| w == ["-c:v", "copy", "-af"]),
            "{args:?}"
        );
    }

    #[test]
    fn unknown_presets_are_not_invented() {
        assert!(get("nope").is_none());
        assert!(get("").is_none());
    }

    #[test]
    fn the_default_output_never_collides_with_the_source() {
        let preset = get("remux-mkv").unwrap();
        let out = default_output(Path::new("D:/films/Alien (1979).mp4"), preset);
        assert_eq!(
            out.file_name().unwrap().to_string_lossy(),
            "Alien (1979).remux-mkv.mkv"
        );
        assert_ne!(out, PathBuf::from("D:/films/Alien (1979).mp4"));
    }
}
