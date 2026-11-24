//! Browse view: breadcrumbs, folders, items, continue-reading row and search
//! results, laid out like the prototype (folders first, then volumes).
//!
//! Cover thumbnails are requested lazily for cards that are actually drawn, so a
//! library with 12,000 items opens as fast as one with twelve.

use crate::covers::CoverCache;
use crate::theme;
use magpiecraft_core::{BrowseView, Catalog, SearchView};
use serde_json::Value;

const CARD_WIDTH: f32 = 158.0;
const CARD_GAP: f32 = 12.0;

pub enum Nav {
    /// Open a folder (None goes to the libraries overview).
    Open(Option<String>),
    /// Open the reader for one item.
    Read(String),
    None,
}

pub struct BrowseState {
    pub browse: BrowseView,
    pub filter: String,
    pub search: Option<SearchView>,
    pub continue_row: Vec<Value>,
}

impl BrowseState {
    pub fn load(catalog: &mut Catalog, key: Option<String>) -> (Self, Option<String>) {
        let mut error = None;
        let browse = match catalog.browse(key.as_deref()) {
            Ok(b) => b,
            Err(e) => {
                error = Some(e.to_string());
                catalog.browse(None).unwrap_or_else(|_| BrowseView {
                    kind: None,
                    library: None,
                    mature: false,
                    crumbs: Vec::new(),
                    folders: Vec::new(),
                    items: Vec::new(),
                    total_folders: 0,
                    total_items: 0,
                })
            }
        };
        let continue_row = catalog
            .continue_reading(12)
            .ok()
            .and_then(|v| v.get("entries").and_then(Value::as_array).cloned())
            .unwrap_or_default();
        (
            BrowseState {
                browse,
                filter: String::new(),
                search: None,
                continue_row,
            },
            error,
        )
    }

    fn matches(&self, name: &str) -> bool {
        let needle = self.filter.trim().to_lowercase();
        needle.is_empty() || name.to_lowercase().contains(&needle)
    }
}

pub fn show(
    ui: &mut egui::Ui,
    state: &mut BrowseState,
    covers: &mut CoverCache,
    catalog: &mut Catalog,
) -> Nav {
    let mut nav = Nav::None;
    crumbs(ui, state, &mut nav);
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label("Filter this folder");
        ui.add(
            egui::TextEdit::singleline(&mut state.filter)
                .desired_width(260.0)
                .hint_text("type to narrow the grid"),
        );
        if !state.filter.is_empty() && ui.small_button("clear").clicked() {
            state.filter.clear();
        }
        if state.search.is_some() && ui.small_button("Back to browsing").clicked() {
            state.search = None;
        }
    });
    ui.add_space(6.0);

    if let Some(search) = state.search.clone() {
        search_results(ui, &search, covers, catalog, &mut nav);
        return nav;
    }

    if state.browse.crumbs.len() <= 1 && !state.continue_row.is_empty() {
        section(ui, "Continue reading");
        ui.horizontal_wrapped(|ui| {
            let entries = state.continue_row.clone();
            for entry in &entries {
                with_card(ui, |ui, width| {
                    let key = entry.get("key").and_then(Value::as_str).unwrap_or("");
                    let title = entry.get("title").and_then(Value::as_str).unwrap_or("");
                    let page = entry.get("page").and_then(Value::as_i64).unwrap_or(0);
                    let total = entry.get("total").and_then(Value::as_i64).unwrap_or(0);
                    cover(ui, width, key, covers, catalog);
                    let label = if total > 0 {
                        format!("{title}  p.{page}/{total}")
                    } else {
                        title.to_string()
                    };
                    if card_label(ui, &label, key).clicked() {
                        nav = Nav::Read(key.to_string());
                    }
                });
            }
        });
        ui.add_space(10.0);
    }

    if !state.browse.folders.is_empty() {
        let visible: Vec<_> = state
            .browse
            .folders
            .iter()
            .filter(|f| state.matches(&f.name))
            .cloned()
            .collect();
        section(ui, &format!("Folders ({})", state.browse.folders.len()));
        ui.horizontal_wrapped(|ui| {
            for folder in &visible {
                with_card(ui, |ui, width| {
                    match &folder.cover_key {
                        Some(cover_key) => cover(ui, width, cover_key, covers, catalog),
                        None => placeholder(ui, width),
                    }
                    let label = folder.name.clone();
                    let sub = if folder.items == 1 {
                        "1 volume".to_string()
                    } else {
                        format!("{} volumes", folder.items)
                    };
                    if card_label(ui, &label, &sub).clicked() {
                        nav = Nav::Open(Some(folder.key.clone()));
                    }
                });
            }
        });
    }

    if !state.browse.items.is_empty() {
        let visible: Vec<_> = state
            .browse
            .items
            .iter()
            .filter(|i| state.matches(&i.title))
            .cloned()
            .collect();
        section(
            ui,
            &format!("Volumes and issues ({})", state.browse.items.len()),
        );
        ui.horizontal_wrapped(|ui| {
            for item in &visible {
                with_card(ui, |ui, width| {
                    cover(ui, width, &item.key, covers, catalog);
                    let sub = item_subtitle(item);
                    if card_label(ui, &item.title, &sub).clicked() {
                        nav = Nav::Read(item.key.clone());
                    }
                });
            }
        });
    }

    if state.browse.folders.is_empty() && state.browse.items.is_empty() {
        ui.add_space(16.0);
        ui.label(egui::RichText::new("Nothing readable here.").color(theme::dim_text(ui)));
        if state.browse.crumbs.len() <= 1 {
            ui.label(
                egui::RichText::new(
                    "Add a library folder in File > Add library..., then run File > Scan all libraries.",
                )
                .color(theme::dim_text(ui)),
            );
        }
    }
    nav
}

fn item_subtitle(item: &magpiecraft_core::ItemView) -> String {
    let pages = match item.page_count {
        Some(p) => format!("{p} p"),
        None => "unknown pages".to_string(),
    };
    let mut parts = vec![pages];
    if let Some(series) = &item.series {
        if !series.is_empty() {
            parts.push(series.clone());
        }
    }
    if !item.readable {
        parts.push(format!("{} (not readable yet)", item.container.as_str()));
    }
    if let Some(progress) = &item.progress {
        parts.push(format!("p.{}", progress.page));
    }
    parts.join("  ")
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(title.to_uppercase())
            .size(13.0)
            .color(theme::dim_text(ui)),
    );
    ui.add_space(2.0);
}

fn crumbs(ui: &mut egui::Ui, state: &mut BrowseState, nav: &mut Nav) {
    let crumbs = state.browse.crumbs.clone();
    ui.horizontal_wrapped(|ui| {
        for (i, crumb) in crumbs.iter().enumerate() {
            let last = i + 1 == crumbs.len();
            if last {
                ui.label(egui::RichText::new(&crumb.name).strong());
            } else if ui.link(&crumb.name).clicked() {
                *nav = Nav::Open(crumb.key.clone());
            }
            if !last {
                ui.label(egui::RichText::new("/").color(theme::dim_text(ui)));
            }
        }
        if let Some(kind) = state.browse.kind {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(kind.as_str())
                    .size(12.0)
                    .color(theme::ACCENT),
            );
        }
        if state.browse.mature {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("mature")
                    .size(12.0)
                    .color(theme::ACCENT),
            );
        }
    });
}

fn with_card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui, f32)) {
    ui.allocate_ui_with_layout(
        egui::vec2(CARD_WIDTH, 236.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            add(ui, CARD_WIDTH);
        },
    );
    ui.add_space(CARD_GAP - 12.0);
}

fn cover(ui: &mut egui::Ui, width: f32, key: &str, covers: &mut CoverCache, catalog: &mut Catalog) {
    let height = width * 1.5;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let dark = ui.visuals().dark_mode;
    let bg = if dark {
        theme::DARK_CARD
    } else {
        theme::LIGHT_CARD
    };
    ui.painter().rect_filled(rect, 3.0, bg);
    if let Some(tex) = covers.texture(key) {
        let size = tex.size_vec2();
        if size.x > 0.0 && size.y > 0.0 {
            let scale = (rect.width() / size.x).max(rect.height() / size.y);
            let draw = egui::vec2(size.x * scale, size.y * scale);
            let target = egui::Rect::from_center_size(rect.center(), draw);
            ui.painter_at(rect).image(
                tex.id(),
                target,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            return;
        }
    }
    if covers.failed(key) {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "no cover",
            egui::FontId::proportional(12.0),
            theme::dim_text(ui),
        );
        return;
    }
    if let Ok(source) = catalog.cover_source(key) {
        covers.request(key, source);
    } else {
        placeholder_paint(ui, rect);
        return;
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "...",
        egui::FontId::proportional(18.0),
        theme::dim_text(ui),
    );
}

fn placeholder(ui: &mut egui::Ui, width: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, width * 1.5), egui::Sense::hover());
    placeholder_paint(ui, rect);
}

fn placeholder_paint(ui: &egui::Ui, rect: egui::Rect) {
    let dark = ui.visuals().dark_mode;
    let line = if dark {
        theme::DARK_LINE
    } else {
        theme::LIGHT_LINE
    };
    ui.painter().rect_stroke(
        rect,
        3.0,
        egui::Stroke::new(1.0, line),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "folder",
        egui::FontId::proportional(12.0),
        theme::dim_text(ui),
    );
}

fn card_label(ui: &mut egui::Ui, title: &str, sub: &str) -> egui::Response {
    ui.add_space(3.0);
    // Two lines are reserved for the title so the subtitle lines up across a row.
    let width = ui.available_width().max(40.0);
    let title_response = ui
        .allocate_ui_with_layout(
            egui::vec2(width, 34.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new(title).size(13.5))
                        .wrap()
                        .sense(egui::Sense::click()),
                )
            },
        )
        .inner;
    ui.label(
        egui::RichText::new(sub)
            .size(11.5)
            .color(theme::dim_text(ui)),
    );
    title_response
}

fn search_results(
    ui: &mut egui::Ui,
    search: &SearchView,
    covers: &mut CoverCache,
    catalog: &mut Catalog,
    nav: &mut Nav,
) {
    section(
        ui,
        &format!(
            "{} result{}{} via {}",
            search.count,
            if search.count == 1 { "" } else { "s" },
            if search.truncated { "+" } else { "" },
            search.mode
        ),
    );
    ui.horizontal_wrapped(|ui| {
        for hit in &search.hits {
            with_card(ui, |ui, width| {
                cover(ui, width, &hit.key, covers, catalog);
                let library = if hit.where_.is_empty() {
                    hit.library.clone()
                } else {
                    format!("{} / {}", hit.library, hit.where_)
                };
                if card_label(ui, &hit.title, &library).clicked() {
                    *nav = Nav::Read(hit.key.clone());
                }
            });
        }
    });
    if search.count == 0 {
        ui.label(
            egui::RichText::new("No matches. Try fewer words, or the substring mode.")
                .color(theme::dim_text(ui)),
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn card_geometry_keeps_a_book_aspect() {
        assert!((CARD_WIDTH * 1.5 - 237.0).abs() < 1.0);
    }

    #[test]
    fn filter_is_case_insensitive_and_trims() {
        let home = std::env::temp_dir().join(format!("magpie-browse-{}", std::process::id()));
        let mut catalog = Catalog::open_in_memory(magpiecraft_core::Paths::at(home)).unwrap();
        let (mut state, error) = BrowseState::load(&mut catalog, None);
        assert!(error.is_none());
        state.filter = "  YOTSU ".to_string();
        assert!(state.matches("Yotsuba v01"));
        assert!(!state.matches("Berserk v01"));
        state.filter.clear();
        assert!(state.matches("anything"));
    }
}
