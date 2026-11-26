//! Where MagpieCraft keeps its own state, and the small settings file.
//!
//! The catalogue database, the cover cache and the manifests live in one home
//! directory: `%LOCALAPPDATA%\magpiecraft` on Windows, `$XDG_DATA_HOME/magpiecraft`
//! or `~/.local/share/magpiecraft` elsewhere, overridable with `MAGPIECRAFT_HOME`
//! (tests and `--smoke` use that so they never touch a real library).
//!
//! Settings are JSON, separate from the database, and are written atomically.
//! A corrupt settings file falls back to defaults instead of blocking startup.

use crate::error::{MagpieError, MagpieResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Directories for one installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    home: PathBuf,
}

impl Paths {
    /// Environment first, then the platform data directory, then the working
    /// directory. Never a hardcoded machine path.
    pub fn discover() -> Self {
        if let Some(home) = std::env::var_os("MAGPIECRAFT_HOME") {
            let home = PathBuf::from(home);
            if !home.as_os_str().is_empty() {
                return Paths { home };
            }
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            return Paths {
                home: PathBuf::from(local).join("magpiecraft"),
            };
        }
        if let Some(data) = std::env::var_os("XDG_DATA_HOME") {
            return Paths {
                home: PathBuf::from(data).join("magpiecraft"),
            };
        }
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            return Paths {
                home: PathBuf::from(home).join(".magpiecraft"),
            };
        }
        Paths {
            home: PathBuf::from(".magpiecraft"),
        }
    }

    pub fn at(home: impl Into<PathBuf>) -> Self {
        Paths { home: home.into() }
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn settings_file(&self) -> PathBuf {
        self.home.join("settings.json")
    }

    pub fn db_file(&self) -> PathBuf {
        self.home.join("catalogue.db")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.home.join("cache")
    }

    pub fn covers_dir(&self) -> PathBuf {
        self.cache_dir().join("covers")
    }

    pub fn pages_dir(&self) -> PathBuf {
        self.cache_dir().join("pages")
    }

    pub fn manifests_dir(&self) -> PathBuf {
        self.home.join("manifests")
    }

    pub fn ensure(&self) -> MagpieResult<()> {
        for dir in [
            self.home.clone(),
            self.cache_dir(),
            self.covers_dir(),
            self.pages_dir(),
            self.manifests_dir(),
        ] {
            std::fs::create_dir_all(&dir).map_err(|e| MagpieError::io(&dir, e))?;
        }
        Ok(())
    }
}

/// The look of the app. One id, six curated skins; the palettes themselves live
/// in the app crate, but the id and its lightness live here so settings, the CLI
/// and MCP all agree on what names are legal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkinId {
    MagpieInk,
    PaperLibrary,
    NeonShelf,
    ForestStacks,
    SepiaDesk,
    MidnightArchive,
}

impl SkinId {
    pub const ALL: [Self; 6] = [
        Self::MagpieInk,
        Self::PaperLibrary,
        Self::NeonShelf,
        Self::ForestStacks,
        Self::SepiaDesk,
        Self::MidnightArchive,
    ];

    pub fn id(self) -> &'static str {
        match self {
            SkinId::MagpieInk => "magpie-ink",
            SkinId::PaperLibrary => "paper-library",
            SkinId::NeonShelf => "neon-shelf",
            SkinId::ForestStacks => "forest-stacks",
            SkinId::SepiaDesk => "sepia-desk",
            SkinId::MidnightArchive => "midnight-archive",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "magpie-ink" | "ink" | "dark" => Some(SkinId::MagpieInk),
            "paper-library" | "paper" | "light" => Some(SkinId::PaperLibrary),
            "neon-shelf" | "neon" => Some(SkinId::NeonShelf),
            "forest-stacks" | "forest" => Some(SkinId::ForestStacks),
            "sepia-desk" | "sepia" => Some(SkinId::SepiaDesk),
            "midnight-archive" | "midnight" | "archive" => Some(SkinId::MidnightArchive),
            _ => None,
        }
    }

    pub fn is_light(self) -> bool {
        matches!(self, SkinId::PaperLibrary | SkinId::SepiaDesk)
    }

    pub fn fallback() -> Self {
        SkinId::MagpieInk
    }
}

/// User settings. Small on purpose: everything else belongs in the catalogue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// A folder with at least this many page images is a volume.
    pub imgdir_min_pages: usize,
    /// Mature-flagged libraries are hidden until this is true.
    pub mature_visible: bool,
    /// `dark` or `light`. Kept in sync with the skin: setting a skin sets this.
    pub theme: String,
    /// Which curated skin paints the app: `magpie-ink`, `paper-library` or `neon-shelf`.
    pub skin: String,
    /// UI zoom factor, persisted so Ctrl+wheel survives a restart.
    pub zoom: f32,
    /// Cover cache size in pixels. The long edge drives an aspect box.
    pub cover_width: u32,
    pub cover_height: u32,
    /// Default result cap for a whole-collection search.
    pub search_limit: usize,
    /// Cover JPEG quality, 1 to 100.
    pub cover_quality: u8,
    /// Prefetch depth in the reader.
    pub prefetch_pages: usize,
    /// PDF render resolution for pages that come from PdfCraft.
    pub pdf_dpi: u32,
    /// Ceiling for extracted CBR/RAR copies, in MiB. Oldest extraction goes first.
    pub extract_cache_mb: u64,
    /// Ceiling for rendered PDF pages, in MiB.
    pub render_cache_mb: u64,
    /// Folders that hold backups, checked by ackup_status.
    pub backup_roots: Vec<String>,
    /// Card border width in points, 1.0 to 3.0. Tuned in Settings, Appearance.
    pub border_width: f32,
    /// Backdrop pattern strength, 0.0 (solid) to 2.0 (double). Tuned in Settings.
    pub backdrop_amount: f32,
    /// Drive roots holding a level-1 media folder each, for example E:\ plus
    /// F:\ plus K:\ plus L:\. The global scan proposes one library per level-2 folder.
    pub drive_roots: Vec<String>,
    /// Level-1 folder name on every media drive. Default: Multimedia Files.
    pub media_folder: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            imgdir_min_pages: 3,
            mature_visible: false,
            theme: "dark".to_string(),
            skin: SkinId::fallback().id().to_string(),
            zoom: 1.0,
            cover_width: 360,
            cover_height: 520,
            search_limit: 300,
            cover_quality: 78,
            prefetch_pages: 3,
            pdf_dpi: 200,
            extract_cache_mb: 8192,
            render_cache_mb: 2048,
            backup_roots: Vec::new(),
            border_width: 1.0,
            backdrop_amount: 1.0,
            drive_roots: Vec::new(),
            media_folder: "Multimedia Files".to_string(),
        }
    }
}

impl Settings {
    /// Clamp values that would break the UI if edited by hand.
    pub fn sanitised(mut self) -> Self {
        self.imgdir_min_pages = self.imgdir_min_pages.clamp(1, 50);
        self.zoom = self.zoom.clamp(0.6, 2.5);
        self.cover_width = self.cover_width.clamp(80, 900);
        self.cover_height = self.cover_height.clamp(80, 1300);
        self.search_limit = self.search_limit.clamp(10, 5000);
        self.cover_quality = self.cover_quality.clamp(30, 95);
        self.prefetch_pages = self.prefetch_pages.min(12);
        self.pdf_dpi = self.pdf_dpi.clamp(96, 400);
        self.extract_cache_mb = self.extract_cache_mb.clamp(256, 200_000);
        self.render_cache_mb = self.render_cache_mb.clamp(64, 50_000);
        self.border_width = self.border_width.clamp(1.0, 3.0);
        self.backdrop_amount = self.backdrop_amount.clamp(0.0, 2.0);
        self.drive_roots = self
            .drive_roots
            .iter()
            .map(|r| r.trim().to_string())
            .filter(|r| !r.is_empty())
            .collect();
        if self.media_folder.trim().is_empty() {
            self.media_folder = "Multimedia Files".to_string();
        } else {
            self.media_folder = self.media_folder.trim().to_string();
        }
        if SkinId::parse(&self.skin).is_none() {
            self.skin = SkinId::fallback().id().to_string();
        }
        // The skin is authoritative for looks; the theme string follows it so old
        // readers of the settings (and the --theme flag) keep working.
        self.theme = if SkinId::parse(&self.skin).is_some_and(SkinId::is_light) {
            "light".to_string()
        } else {
            "dark".to_string()
        };
        self
    }
}

/// Read settings, falling back to defaults when the file is missing or broken.
pub fn load_settings(paths: &Paths) -> Settings {
    let file = paths.settings_file();
    let text = match std::fs::read_to_string(&file) {
        Ok(t) => t,
        Err(_) => return Settings::default(),
    };
    serde_json::from_str::<Settings>(&text)
        .map(Settings::sanitised)
        .unwrap_or_default()
}

/// Write settings atomically: a crash mid-write must not leave half a file.
pub fn save_settings(paths: &Paths, settings: &Settings) -> MagpieResult<()> {
    paths.ensure()?;
    let file = paths.settings_file();
    let body = serde_json::to_string_pretty(&settings.clone().sanitised())
        .map_err(|e| MagpieError::Config(e.to_string()))?;
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, format!("{body}\n")).map_err(|e| MagpieError::io(&tmp, e))?;
    std::fs::rename(&tmp, &file).map_err(|e| MagpieError::io(&file, e))?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn tmp_home(name: &str) -> Paths {
        let p = std::env::temp_dir().join(format!(
            "magpie-config-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        Paths::at(p)
    }

    #[test]
    fn paths_are_under_one_home() {
        let paths = Paths::at("C:/tmp/magpie");
        assert!(paths.db_file().starts_with(paths.home()));
        assert!(paths.covers_dir().starts_with(paths.cache_dir()));
        assert!(paths.manifests_dir().starts_with(paths.home()));
        paths.ensure().unwrap();
        assert!(paths.covers_dir().is_dir());
        std::fs::remove_dir_all(paths.home()).unwrap();
    }

    #[test]
    fn settings_roundtrip_and_fallbacks() {
        let paths = tmp_home("settings");
        assert_eq!(load_settings(&paths), Settings::default(), "missing file");
        let s = Settings {
            skin: "paper-library".to_string(),
            zoom: 1.5,
            ..Settings::default()
        };
        save_settings(&paths, &s).unwrap();
        assert_eq!(load_settings(&paths).skin, "paper-library");
        assert_eq!(load_settings(&paths).theme, "light");
        assert!((load_settings(&paths).zoom - 1.5).abs() < f32::EPSILON);
        std::fs::write(paths.settings_file(), "{not json").unwrap();
        assert_eq!(load_settings(&paths), Settings::default(), "broken file");
        std::fs::remove_dir_all(paths.home()).unwrap();
    }

    #[test]
    fn hand_edited_values_are_clamped() {
        let s = Settings {
            zoom: 99.0,
            cover_width: 5,
            theme: "purple".to_string(),
            skin: "neon".to_string(),
            imgdir_min_pages: 0,
            ..Settings::default()
        }
        .sanitised();
        assert_eq!(s.zoom, 2.5);
        assert_eq!(s.cover_width, 80);
        assert_eq!(s.skin, "neon", "a known alias survives sanitising");
        assert_eq!(s.theme, "dark", "the theme follows the skin");
        assert_eq!(s.imgdir_min_pages, 1);
    }

    #[test]
    fn drive_roots_trim_and_media_folder_defaults() {
        let s = Settings {
            drive_roots: vec![" E:/ ".to_string(), String::new(), "F:/".to_string()],
            media_folder: "  ".to_string(),
            ..Settings::default()
        }
        .sanitised();
        assert_eq!(s.drive_roots, vec!["E:/".to_string(), "F:/".to_string()]);
        assert_eq!(s.media_folder, "Multimedia Files");
    }

    #[test]
    fn an_unknown_skin_falls_back_to_the_default() {
        let s = Settings {
            skin: "chartreuse".to_string(),
            ..Settings::default()
        }
        .sanitised();
        assert_eq!(s.skin, SkinId::fallback().id());
    }

    #[test]
    fn env_home_wins() {
        std::env::set_var("MAGPIECRAFT_HOME", "C:/tmp/magpie-env");
        assert_eq!(Paths::discover(), Paths::at("C:/tmp/magpie-env"));
        std::env::remove_var("MAGPIECRAFT_HOME");
    }
}
