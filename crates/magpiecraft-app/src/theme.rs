//! Visuals: a high-contrast dark theme and a crisp light theme.
//!
//! Fleet rule: high contrast in both themes, never light grey on white in light
//! mode, and Ctrl + wheel zoom everywhere. No glyph outside the safe set: egui's
//! default font has no geometric shapes, so a check mark or an arrow would render
//! as a tofu box. Icons here are painted as vector shapes or written in ASCII.

use egui::{Color32, FontId, Stroke, TextStyle};

pub const ACCENT: Color32 = Color32::from_rgb(255, 122, 26);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(160, 82, 24);
pub const DARK_CANVAS: Color32 = Color32::from_rgb(10, 13, 18);
pub const DARK_CARD: Color32 = Color32::from_rgb(16, 26, 46);
pub const DARK_LINE: Color32 = Color32::from_rgb(38, 52, 82);
pub const DARK_TEXT: Color32 = Color32::from_rgb(245, 247, 250);
pub const DARK_TEXT_DIM: Color32 = Color32::from_rgb(186, 194, 206);
pub const LIGHT_CANVAS: Color32 = Color32::from_rgb(248, 250, 252);
pub const LIGHT_CARD: Color32 = Color32::from_rgb(255, 255, 255);
pub const LIGHT_LINE: Color32 = Color32::from_rgb(203, 213, 225);
pub const LIGHT_TEXT: Color32 = Color32::from_rgb(15, 23, 42);
pub const LIGHT_TEXT_DIM: Color32 = Color32::from_rgb(51, 65, 85);

pub fn harsh_visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.panel_fill = DARK_CANVAS;
    v.window_fill = DARK_CARD;
    v.extreme_bg_color = Color32::from_rgb(6, 8, 12);
    v.faint_bg_color = DARK_CARD;
    v.override_text_color = Some(DARK_TEXT);
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = Color32::from_rgba_unmultiplied(255, 122, 26, 110);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.widgets.noninteractive.bg_fill = DARK_CANVAS;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, DARK_LINE);
    v.widgets.inactive.bg_fill = DARK_CARD;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(48, 66, 102));
    v.widgets.hovered.bg_fill = Color32::from_rgb(24, 38, 64);
    v.widgets.hovered.bg_stroke = Stroke::new(1.5, ACCENT);
    v.widgets.active.bg_fill = Color32::from_rgb(30, 46, 76);
    v.widgets.active.bg_stroke = Stroke::new(1.5, ACCENT);
    v.window_stroke = Stroke::new(1.0, DARK_LINE);
    v
}

pub fn crisp_light_visuals() -> egui::Visuals {
    let mut v = egui::Visuals::light();
    v.panel_fill = LIGHT_CANVAS;
    v.window_fill = LIGHT_CARD;
    v.extreme_bg_color = LIGHT_CARD;
    v.faint_bg_color = LIGHT_CANVAS;
    v.override_text_color = Some(LIGHT_TEXT);
    v.hyperlink_color = ACCENT_DIM;
    v.selection.bg_fill = Color32::from_rgba_unmultiplied(255, 122, 26, 80);
    v.selection.stroke = Stroke::new(1.0, ACCENT_DIM);
    v.widgets.noninteractive.bg_fill = LIGHT_CANVAS;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LIGHT_LINE);
    v.widgets.inactive.bg_fill = LIGHT_CARD;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, LIGHT_LINE);
    v.widgets.hovered.bg_fill = Color32::from_rgb(241, 245, 249);
    v.widgets.hovered.bg_stroke = Stroke::new(1.5, ACCENT_DIM);
    v.widgets.active.bg_fill = Color32::from_rgb(226, 232, 240);
    v.widgets.active.bg_stroke = Stroke::new(1.5, ACCENT_DIM);
    v.window_stroke = Stroke::new(1.0, LIGHT_LINE);
    v
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

pub fn visuals(dark: bool) -> egui::Visuals {
    if dark {
        harsh_visuals()
    } else {
        crisp_light_visuals()
    }
}

/// A card: filled, hairline border, accent stripe on the left when requested.
pub fn card(ui: &mut egui::Ui, accent: Option<Color32>, add: impl FnOnce(&mut egui::Ui)) {
    let dark = ui.visuals().dark_mode;
    let fill = if dark { DARK_CARD } else { LIGHT_CARD };
    let line = if dark { DARK_LINE } else { LIGHT_LINE };
    egui::Frame::NONE
        .fill(fill)
        .stroke(Stroke::new(1.0, line))
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

/// Dimmed secondary text colour for the current theme.
pub fn dim_text(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        DARK_TEXT_DIM
    } else {
        LIGHT_TEXT_DIM
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

    #[test]
    fn both_themes_have_strong_contrast() {
        let dark = harsh_visuals();
        assert_eq!(dark.panel_fill, DARK_CANVAS);
        assert_eq!(dark.override_text_color, Some(DARK_TEXT));
        let light = crisp_light_visuals();
        assert_eq!(light.panel_fill, LIGHT_CANVAS);
        assert_eq!(light.override_text_color, Some(LIGHT_TEXT));
        // Light mode text must be near-black, not grey.
        let text = LIGHT_TEXT;
        assert!(text.r() < 40 && text.g() < 40 && text.b() < 60);
    }

    #[test]
    fn every_ui_source_file_is_ascii_safe() {
        // egui's bundled font has no geometric shapes or emoji: anything outside
        // this allowlist renders as a tofu box. Keep the UI to ASCII.
        let allowed = ['\u{00B0}', '\u{00D7}', '\u{00B7}', '\u{2026}'];
        for (name, src) in [
            ("app.rs", include_str!("app.rs")),
            ("theme.rs", include_str!("theme.rs")),
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
