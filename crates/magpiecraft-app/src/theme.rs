//! Visuals driven by the active skin.
//!
//! Every colour comes from the skin palette, so switching skins can never leave a
//! hard-coded grey on white behind. No glyph outside the safe set: egui's default
//! font has no geometric shapes, so a check mark or an arrow would render as a tofu
//! box. Icons are painted as vector shapes or written in ASCII.

use crate::skins::{BackdropStyle, CardDecorStyle, SkinPalette};
use egui::{Color32, CornerRadius, FontId, Shadow, Stroke, TextStyle};

/// User-tuned decor on top of a skin. Stored in Settings, so it survives a
/// restart and a skin switch; the skin stays the curated base.
#[derive(Debug, Clone, Copy)]
pub struct DecorOpts {
    /// Card border width in points, 1.0 to 3.0.
    pub border_width: f32,
    /// Backdrop pattern strength, 0.0 (solid) to 2.0 (double).
    pub backdrop_amount: f32,
}

impl DecorOpts {
    pub fn amount(self) -> f32 {
        self.backdrop_amount
    }
}

/// Full-window visuals for one palette.
pub fn visuals(palette: &SkinPalette) -> egui::Visuals {
    let mut v = if palette.is_light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    v.panel_fill = palette.canvas;
    v.window_fill = palette.card;
    v.extreme_bg_color = palette.canvas;
    v.faint_bg_color = palette.card;
    v.override_text_color = Some(palette.text);
    v.hyperlink_color = palette.accent_dim;
    v.selection.bg_fill = Color32::from_rgba_unmultiplied(
        palette.accent.r(),
        palette.accent.g(),
        palette.accent.b(),
        110,
    );
    v.selection.stroke = Stroke::new(1.0, palette.accent);
    v.widgets.noninteractive.bg_fill = palette.canvas;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.border);
    v.widgets.inactive.bg_fill = palette.card;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, palette.border);
    v.widgets.hovered.bg_fill = hover_fill(palette);
    v.widgets.hovered.bg_stroke = Stroke::new(1.5, palette.accent);
    v.widgets.active.bg_fill = hover_fill(palette);
    v.widgets.active.bg_stroke = Stroke::new(1.5, palette.accent);
    v.window_stroke = Stroke::new(1.0, palette.border);
    v
}

fn hover_fill(palette: &SkinPalette) -> Color32 {
    if palette.is_light {
        lighten(palette.card, 6)
    } else {
        lighten(palette.card, 12)
    }
}

fn lighten(c: Color32, by: u8) -> Color32 {
    Color32::from_rgb(
        c.r().saturating_add(by),
        c.g().saturating_add(by),
        c.b().saturating_add(by),
    )
}

/// Bump the type ramp so the app is readable on a 1440p screen without zooming.
pub fn apply_text_styles(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::proportional(23.0));
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(16.0));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(16.0));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::proportional(13.0));
        style
            .text_styles
            .insert(TextStyle::Monospace, FontId::monospace(14.0));
    });
}

/// A card: filled, decor-styled shadow, accent stripe on the left when requested.
/// The border width comes from the user's decor options, not the skin.
pub fn card(
    ui: &mut egui::Ui,
    palette: &SkinPalette,
    accent: Option<Color32>,
    decor: &DecorOpts,
    add: impl FnOnce(&mut egui::Ui),
) {
    let corner = CornerRadius::same(8);
    let shadow = match palette.decor {
        CardDecorStyle::DropShadow => Shadow {
            offset: [3, 4],
            blur: 14,
            spread: 1,
            color: Color32::from_rgba_unmultiplied(
                0,
                0,
                0,
                if palette.is_light { 70 } else { 150 },
            ),
        },
        CardDecorStyle::NeonGlow => Shadow {
            offset: [0, 0],
            blur: 18,
            spread: 2,
            color: Color32::from_rgba_unmultiplied(
                palette.accent.r(),
                palette.accent.g(),
                palette.accent.b(),
                if palette.is_light { 85 } else { 140 },
            ),
        },
    };
    egui::Frame::NONE
        .fill(palette.card)
        .stroke(Stroke::new(decor.border_width, palette.border))
        .corner_radius(corner)
        .shadow(shadow)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            if let Some(colour) = accent {
                let rect = ui.max_rect();
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())),
                    0.0,
                    colour,
                );
            }
            add(ui);
        });
}

/// Paint the skin backdrop over one rect. The caller passes the page area, never
/// the whole window: the menu bar and search row are painted before this runs and
/// must not be covered.
pub fn paint_backdrop(ui: &egui::Ui, palette: &SkinPalette, rect: egui::Rect, decor: &DecorOpts) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, palette.canvas);
    let amount = decor.amount();
    if amount <= 0.01 {
        return;
    }
    match palette.backdrop {
        BackdropStyle::Solid => {}
        BackdropStyle::Weave => {
            // Faint diagonal weave, like book cloth.
            let color = pattern_line(palette, amount);
            let step = 26.0;
            let mut x = rect.left() - rect.height();
            while x < rect.right() {
                painter.line_segment(
                    [
                        egui::pos2(x, rect.bottom()),
                        egui::pos2(x + rect.height(), rect.top()),
                    ],
                    egui::Stroke::new(1.0, color),
                );
                x += step;
            }
        }
        BackdropStyle::Dots => {
            // Sparse dots, like dust in a projector beam.
            let color = pattern_line(palette, amount);
            let step = 34.0;
            let mut y = rect.top() + 12.0;
            let mut row = 0;
            while y < rect.bottom() {
                let mut x = rect.left() + 12.0 + if row % 2 == 0 { 0.0 } else { step / 2.0 };
                while x < rect.right() {
                    painter.circle_filled(egui::pos2(x, y), 1.4, color);
                    x += step;
                }
                y += step;
                row += 1;
            }
        }
    }
}

fn pattern_line(palette: &SkinPalette, amount: f32) -> Color32 {
    // Barely-there lines: visible on close look, invisible at a glance.
    // The user's backdrop slider scales the alpha; 0 paints solid.
    let (r, g, b) = if palette.is_light {
        (120, 110, 95)
    } else {
        (150, 160, 190)
    };
    let base = if palette.is_light { 16 } else { 20 };
    let alpha = ((base as f32 * amount).round() as i32).clamp(0, 255) as u8;
    Color32::from_rgba_unmultiplied(r, g, b, alpha)
}

/// Dimmed secondary text colour for the current theme.
pub fn dim_text(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(186, 194, 206)
    } else {
        Color32::from_rgb(92, 85, 74)
    }
}

/// Ctrl + mouse wheel zoom, 0.6 to 2.5. Returns the new factor when it changed.
pub fn handle_ctrl_wheel(ctx: &egui::Context, current: f32) -> Option<f32> {
    let scroll = ctx.input(|i| {
        if i.modifiers.ctrl || i.modifiers.command {
            i.smooth_scroll_delta.y
        } else {
            0.0
        }
    });
    if scroll.abs() < 0.5 {
        return None;
    }
    let step = if scroll > 0.0 { 1.1 } else { 1.0 / 1.1 };
    let next = (current * step).clamp(0.6, 2.5);
    if (next - current).abs() < 0.0001 {
        None
    } else {
        Some(next)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::skins::SkinId;

    #[test]
    fn visuals_follow_the_palette() {
        for skin in SkinId::ALL {
            let p = skin.palette();
            let v = visuals(&p);
            assert_eq!(v.panel_fill, p.canvas);
            assert_eq!(v.override_text_color, Some(p.text));
            assert_eq!(v.dark_mode, !p.is_light);
        }
    }

    #[test]
    fn every_ui_source_file_is_ascii_safe() {
        // egui's bundled font has no geometric shapes or emoji: anything outside
        // this allowlist renders as a tofu box. Keep the UI to ASCII.
        let allowed = ['\u{00B0}', '\u{00D7}', '\u{00B7}', '\u{2026}'];
        for (name, src) in [
            ("app.rs", include_str!("app.rs")),
            ("theme.rs", include_str!("theme.rs")),
            ("skins.rs", include_str!("skins.rs")),
            ("covers.rs", include_str!("covers.rs")),
            ("library.rs", include_str!("library.rs")),
            ("reader.rs", include_str!("reader.rs")),
        ] {
            for (idx, ch) in src.chars().enumerate() {
                if ch.is_ascii() || allowed.contains(&ch) {
                    continue;
                }
                panic!(
                    "non-ASCII glyph {ch:?} (U+{:04X}) at byte {idx} in {name}",
                    ch as u32
                );
            }
        }
    }
}
