//! Skins: curated palettes, card decor and backdrops.
//!
//! djcraft ships nine studio skins; MagpieCraft ships three that fit a collector:
//! ink for the night reader, paper for daylight, neon for the shelf at midnight.
//! A skin owns every colour the app paints, so switching skins can never leave a
//! hard-coded grey on white behind. The contrast test below locks that in.

use egui::Color32;
use magpiecraft_core::MediaKind;

/// How a card is dressed. Covers stay square (the texture is square) and get their
/// depth from the shadow; text cards get the full treatment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardDecorStyle {
    DropShadow,
    NeonGlow,
}

/// What is painted behind the content. All procedural, all cheap: a few dozen
/// lines or dots per frame, no textures, no images.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackdropStyle {
    Solid,
    /// Faint diagonal weave, like book cloth.
    Weave,
    /// A sparse field of dots, like dust in a projector beam.
    Dots,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SkinId {
    #[default]
    MagpieInk,
    PaperLibrary,
    NeonShelf,
}

impl SkinId {
    pub const ALL: [Self; 3] = [Self::MagpieInk, Self::PaperLibrary, Self::NeonShelf];

    pub fn id(self) -> &'static str {
        match self {
            SkinId::MagpieInk => "magpie-ink",
            SkinId::PaperLibrary => "paper-library",
            SkinId::NeonShelf => "neon-shelf",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SkinId::MagpieInk => "Magpie Ink",
            SkinId::PaperLibrary => "Paper Library",
            SkinId::NeonShelf => "Neon Shelf",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            SkinId::MagpieInk => "High-contrast dark, the night reader",
            SkinId::PaperLibrary => "Warm paper light, for daylight cataloguing",
            SkinId::NeonShelf => "Black glass with neon, the shelf at midnight",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "magpie-ink" | "ink" | "dark" => Some(SkinId::MagpieInk),
            "paper-library" | "paper" | "light" => Some(SkinId::PaperLibrary),
            "neon-shelf" | "neon" => Some(SkinId::NeonShelf),
            _ => None,
        }
    }

    pub fn palette(self) -> SkinPalette {
        match self {
            SkinId::MagpieInk => SkinPalette {
                is_light: false,
                canvas: Color32::from_rgb(10, 13, 18),
                card: Color32::from_rgb(16, 26, 46),
                border: Color32::from_rgb(38, 52, 82),
                text: Color32::from_rgb(245, 247, 250),
                text_dim: Color32::from_rgb(186, 194, 206),
                accent: Color32::from_rgb(255, 122, 26),
                accent_dim: Color32::from_rgb(160, 82, 24),
                kind_comics: Color32::from_rgb(255, 138, 61),
                kind_manga: Color32::from_rgb(240, 85, 155),
                kind_magazines: Color32::from_rgb(62, 198, 184),
                kind_video: Color32::from_rgb(74, 168, 255),
                kind_audio: Color32::from_rgb(74, 222, 128),
                kind_ebook: Color32::from_rgb(229, 184, 59),
                kind_photo: Color32::from_rgb(180, 140, 255),
                kind_other: Color32::from_rgb(154, 163, 178),
                decor: CardDecorStyle::DropShadow,
                backdrop: BackdropStyle::Weave,
            },
            SkinId::PaperLibrary => SkinPalette {
                is_light: true,
                canvas: Color32::from_rgb(245, 241, 232),
                card: Color32::from_rgb(255, 253, 248),
                border: Color32::from_rgb(216, 207, 190),
                text: Color32::from_rgb(36, 31, 24),
                text_dim: Color32::from_rgb(92, 85, 74),
                accent: Color32::from_rgb(179, 84, 30),
                accent_dim: Color32::from_rgb(138, 63, 20),
                kind_comics: Color32::from_rgb(194, 87, 26),
                kind_manga: Color32::from_rgb(194, 39, 123),
                kind_magazines: Color32::from_rgb(15, 118, 110),
                kind_video: Color32::from_rgb(29, 111, 209),
                kind_audio: Color32::from_rgb(21, 128, 61),
                kind_ebook: Color32::from_rgb(138, 109, 27),
                kind_photo: Color32::from_rgb(124, 92, 214),
                kind_other: Color32::from_rgb(91, 100, 112),
                decor: CardDecorStyle::DropShadow,
                backdrop: BackdropStyle::Solid,
            },
            SkinId::NeonShelf => SkinPalette {
                is_light: false,
                canvas: Color32::from_rgb(7, 7, 13),
                card: Color32::from_rgb(16, 16, 26),
                border: Color32::from_rgb(42, 42, 63),
                text: Color32::from_rgb(242, 242, 250),
                text_dim: Color32::from_rgb(168, 168, 192),
                accent: Color32::from_rgb(255, 46, 136),
                accent_dim: Color32::from_rgb(0, 229, 255),
                kind_comics: Color32::from_rgb(255, 107, 53),
                kind_manga: Color32::from_rgb(255, 62, 181),
                kind_magazines: Color32::from_rgb(170, 255, 70),
                kind_video: Color32::from_rgb(74, 168, 255),
                kind_audio: Color32::from_rgb(0, 229, 160),
                kind_ebook: Color32::from_rgb(255, 209, 63),
                kind_photo: Color32::from_rgb(178, 132, 255),
                kind_other: Color32::from_rgb(150, 158, 178),
                decor: CardDecorStyle::NeonGlow,
                backdrop: BackdropStyle::Dots,
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SkinPalette {
    pub is_light: bool,
    pub canvas: Color32,
    pub card: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub accent: Color32,
    pub accent_dim: Color32,
    pub kind_comics: Color32,
    pub kind_manga: Color32,
    pub kind_magazines: Color32,
    pub kind_video: Color32,
    pub kind_audio: Color32,
    pub kind_ebook: Color32,
    pub kind_photo: Color32,
    pub kind_other: Color32,
    pub decor: CardDecorStyle,
    pub backdrop: BackdropStyle,
}

impl SkinPalette {
    /// Accent colour for one media kind. Used for stripes, dots and glows, never
    /// for body text, so a bright neon stays legal on any card.
    pub fn kind_color(self, kind: MediaKind) -> Color32 {
        match kind {
            MediaKind::Comics => self.kind_comics,
            MediaKind::Manga => self.kind_manga,
            MediaKind::Magazines => self.kind_magazines,
            MediaKind::Video => self.kind_video,
            MediaKind::Audio => self.kind_audio,
            MediaKind::Ebook => self.kind_ebook,
            MediaKind::Photo => self.kind_photo,
            MediaKind::Other => self.kind_other,
        }
    }

    pub fn kind_color_by_name(self, kind: &str) -> Color32 {
        self.kind_color(MediaKind::parse(kind).unwrap_or(MediaKind::Other))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    /// Relative luminance of an sRGB colour, 0 for black and 1 for white.
    fn luminance(c: Color32) -> f32 {
        fn channel(v: u8) -> f32 {
            let c = f32::from(v) / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
    }

    /// WCAG contrast ratio between two colours, 1 to 21.
    fn contrast(a: Color32, b: Color32) -> f32 {
        let (lighter, darker) = if luminance(a) >= luminance(b) {
            (a, b)
        } else {
            (b, a)
        };
        (luminance(lighter) + 0.05) / (luminance(darker) + 0.05)
    }

    #[test]
    fn every_skin_id_roundtrips_through_its_string() {
        for skin in SkinId::ALL {
            assert_eq!(SkinId::parse(skin.id()), Some(skin));
            assert!(!skin.label().is_empty());
            assert!(!skin.blurb().is_empty());
        }
        assert_eq!(SkinId::parse("dark"), Some(SkinId::MagpieInk));
        assert_eq!(SkinId::parse("light"), Some(SkinId::PaperLibrary));
        assert_eq!(SkinId::parse("nope"), None);
        assert_eq!(SkinId::parse(""), None);
    }

    #[test]
    fn every_skin_keeps_text_readable_on_canvas_and_card() {
        // Fleet rule, locked per skin: primary text at 7:1 (AAA), secondary at
        // 4.5:1 (AA), in both themes.
        for skin in SkinId::ALL {
            let p = skin.palette();
            for (surface_name, surface) in [("canvas", p.canvas), ("card", p.card)] {
                let primary = contrast(p.text, surface);
                assert!(
                    primary >= 7.0,
                    "{:?} primary text on {surface_name} is {primary:.1}:1, want 7+",
                    skin
                );
                let secondary = contrast(p.text_dim, surface);
                assert!(
                    secondary >= 4.5,
                    "{:?} secondary text on {surface_name} is {secondary:.1}:1, want 4.5+",
                    skin
                );
            }
        }
    }

    #[test]
    fn every_kind_has_a_distinct_usable_accent() {
        for skin in SkinId::ALL {
            let p = skin.palette();
            let kinds = [
                p.kind_comics,
                p.kind_manga,
                p.kind_magazines,
                p.kind_video,
                p.kind_audio,
                p.kind_ebook,
                p.kind_photo,
            ];
            for (i, a) in kinds.iter().enumerate() {
                let on_card = contrast(*a, p.card);
                assert!(
                    on_card >= 2.2,
                    "{:?} kind {i} is mud on a card ({on_card:.1}:1)",
                    skin
                );
                for (j, b) in kinds.iter().enumerate() {
                    if i != j {
                        let distance = (a.r() as i16 - b.r() as i16).abs()
                            + (a.g() as i16 - b.g() as i16).abs()
                            + (a.b() as i16 - b.b() as i16).abs();
                        assert!(
                            distance > 60,
                            "{:?} kinds {i} and {j} are indistinguishable",
                            skin
                        );
                    }
                }
            }
        }
    }
}
