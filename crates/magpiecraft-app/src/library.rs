//! Browse view: breadcrumbs, folders, items, continue-reading row and search
//! results, laid out like the prototype (folders first, then volumes).
//!
//! Cover thumbnails are requested lazily for cards that are actually drawn, so a
//! library with 12,000 items opens as fast as one with twelve.

use crate::covers::CoverCache;
use crate::skins::SkinPalette;
use egui::Color32;
use magpiecraft_core::{BrowseView, Catalog, SearchView};
use serde_json::Value;

const CARD_WIDTH: f32 = 158.0;
const CARD_GAP: f32 = 12.0;
/// Card height fits the content exactly: cover + gap + title box + gap + subtitle.
/// Nothing overflows, so no row layout can ever stagger the cards.
const CARD_HEIGHT: f32 = 237.0 + 3.0 + 34.0 + 4.0 + 16.0 + 6.0;

pub enum Nav {
    /// Open a folder (None goes to the libraries overview).
    Open(Option<String>),
    /// Open the reader for one item (comics, manga, magazines).
    Read(String),
    /// Hand an item to the player or the OS handler (video, audio, book, photo).
    Launch(String),
    /// Select a card without opening it. Single click selects; double-click
    /// and Enter open. The inspector shows whatever is selected.
    Select(Selection),
    None,
}

/// How a selection opens: decided where the kind is known so Enter and
/// double-click behave exactly like the card's own open action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelOpen {
    Folder,
    Read,
    Launch,
}

#[derive(Debug, Clone)]
pub struct Selection {
    pub open: SelOpen,
    pub key: String,
    pub title: String,
    pub subtitle: String,
    pub library: Option<String>,
}

pub struct BrowseState {
    pub browse: BrowseView,
    pub filter: String,
    pub search: Option<SearchView>,
    pub continue_row: Vec<Value>,
    pub load_error: Option<String>,
}

impl BrowseState {
    pub fn load(catalog: &mut Catalog, key: Option<String>) -> (Self, Option<String>) {
        let mut error = None;
        let browse = match catalog.browse(key.as_deref()) {
            Ok(b) => b,
            Err(e) => {
                error = Some(format!(
                    "cannot list {}: {e}",
                    key.as_deref().unwrap_or("libraries")
                ));
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
                load_error: error.clone(),
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
    palette: &SkinPalette,
    selected: Option<&str>,
) -> Nav {
    let mut nav = Nav::None;
    crumbs(ui, state, palette, &mut nav);
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
        search_results(ui, &search, covers, catalog, palette, selected, &mut nav);
        return nav;
    }

    if state.browse.crumbs.len() <= 1 && !state.continue_row.is_empty() {
        section(ui, palette, "Continue reading");
        let entries = state.continue_row.clone();
        grid(
            ui,
            &entries,
            palette.accent,
            selected,
            |entry: &Value| entry.get("key").and_then(Value::as_str).unwrap_or(""),
            |ui, entry, width| {
                let key = entry.get("key").and_then(Value::as_str).unwrap_or("");
                let title = entry.get("title").and_then(Value::as_str).unwrap_or("");
                let label_text = entry.get("label").and_then(Value::as_str).unwrap_or("");
                let kind = entry.get("kind").and_then(Value::as_str).unwrap_or("other");
                let comicish = matches!(kind, "comics" | "manga" | "magazines");
                let face = if comicish {
                    CoverFace::Page(Some(palette.kind_color_by_name(kind)))
                } else {
                    CoverFace::Label(kind)
                };
                cover(ui, width, key, covers, catalog, palette, face);
                let label = format!("{title}  {label_text}");
                let sub_kind = magpiecraft_core::MediaKind::parse(kind);
                let response = card_label(ui, &label, kind, sub_kind, palette);
                let open = if comicish {
                    SelOpen::Read
                } else {
                    SelOpen::Launch
                };
                if response.double_clicked() {
                    nav = if comicish {
                        Nav::Read(key.to_string())
                    } else {
                        Nav::Launch(key.to_string())
                    };
                } else if response.clicked() {
                    nav = Nav::Select(Selection {
                        open,
                        key: key.to_string(),
                        title: title.to_string(),
                        subtitle: label_text.to_string(),
                        library: entry
                            .get("library")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    });
                }
            },
        );
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
        section(
            ui,
            palette,
            &format!("Folders ({})", state.browse.folders.len()),
        );
        grid(
            ui,
            &visible,
            palette.accent,
            selected,
            |folder: &magpiecraft_core::FolderView| folder.key.as_str(),
            |ui, folder, width| {
                match &folder.cover_key {
                    Some(cover_key) => cover(
                        ui,
                        width,
                        cover_key,
                        covers,
                        catalog,
                        palette,
                        CoverFace::Page(None),
                    ),
                    None => placeholder(ui, width, palette),
                }
                let label = folder.name.clone();
                let sub = if folder.items == 1 {
                    "1 volume".to_string()
                } else {
                    format!("{} volumes", folder.items)
                };
                let response = card_label(ui, &label, &sub, None, palette);
                if response.double_clicked() {
                    nav = Nav::Open(Some(folder.key.clone()));
                } else if response.clicked() {
                    nav = Nav::Select(Selection {
                        open: SelOpen::Folder,
                        key: folder.key.clone(),
                        title: folder.name.clone(),
                        subtitle: sub,
                        library: state.browse.library.clone(),
                    });
                }
            },
        );
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
            palette,
            &items_heading(state.browse.kind, state.browse.items.len()),
        );
        grid(
            ui,
            &visible,
            palette.accent,
            selected,
            |item: &magpiecraft_core::ItemView| item.key.as_str(),
            |ui, item, width| {
                let comicish = matches!(
                    item.kind,
                    magpiecraft_core::MediaKind::Comics
                        | magpiecraft_core::MediaKind::Manga
                        | magpiecraft_core::MediaKind::Magazines
                );
                if comicish {
                    cover(
                        ui,
                        width,
                        &item.key,
                        covers,
                        catalog,
                        palette,
                        CoverFace::Page(Some(palette.kind_color(item.kind))),
                    );
                } else {
                    // A film or a book has no page to use as a cover, so the card
                    // says what it is instead of asking for an image that cannot exist.
                    cover(
                        ui,
                        width,
                        &item.key,
                        covers,
                        catalog,
                        palette,
                        CoverFace::Label(item.kind.as_str()),
                    );
                }
                let sub = item_subtitle(item);
                let response = card_label(ui, &item.title, &sub, Some(item.kind), palette);
                let open = if item.kind.is_comicish() {
                    SelOpen::Read
                } else {
                    SelOpen::Launch
                };
                if response.double_clicked() {
                    nav = if item.kind.is_comicish() {
                        Nav::Read(item.key.clone())
                    } else {
                        Nav::Launch(item.key.clone())
                    };
                } else if response.clicked() {
                    nav = Nav::Select(Selection {
                        open,
                        key: item.key.clone(),
                        title: item.title.clone(),
                        subtitle: sub,
                        library: Some(item.library.clone()),
                    });
                }
            },
        );
    }

    if state.browse.folders.is_empty() && state.browse.items.is_empty() {
        ui.add_space(16.0);
        if let Some(err) = state.load_error.clone() {
            ui.label(egui::RichText::new(err).color(palette.accent));
            ui.label(
                egui::RichText::new(
                    "The drive may be unplugged or the folder moved. Plug it back and run File > Scan all libraries.",
                )
                .color(palette.text_dim),
            );
        } else {
            ui.label(egui::RichText::new("Nothing readable here.").color(palette.text_dim));
            if state.browse.crumbs.len() <= 1 {
                ui.label(
                    egui::RichText::new(
                        "Add a library folder in File > Add library..., then run File > Scan all libraries.",
                    )
                    .color(palette.text_dim),
                );
            } else {
                ui.label(
                    egui::RichText::new(
                        "This folder holds no indexed items. If files are there, run File > Scan all libraries.",
                    )
                    .color(palette.text_dim),
                );
            }
        }
    }
    nav
}

fn item_subtitle(item: &magpiecraft_core::ItemView) -> String {
    let mut parts: Vec<String> = Vec::new();
    match item.kind {
        magpiecraft_core::MediaKind::Comics
        | magpiecraft_core::MediaKind::Manga
        | magpiecraft_core::MediaKind::Magazines => {
            parts.push(match item.page_count {
                Some(p) => format!("{p} p"),
                None => "unknown pages".to_string(),
            });
            if let Some(series) = &item.series {
                if !series.is_empty() {
                    parts.push(series.clone());
                }
            }
            if !item.readable {
                parts.push(format!("{} (not readable yet)", item.container.as_str()));
            }
        }
        other => {
            parts.push(other.as_str().to_string());
            parts.push(crate::app::human_bytes(item.size));
        }
    }
    if let Some(progress) = &item.progress {
        parts.push(format!("p.{}", progress.page));
    }
    parts.join("  ")
}

fn section(ui: &mut egui::Ui, palette: &SkinPalette, title: &str) {
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(title.to_uppercase())
            .size(14.0)
            .strong()
            .color(palette.text_dim),
    );
    let width = ui.available_width().max(40.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 3.0), egui::Sense::hover());
    ui.painter().rect_filled(
        egui::Rect::from_min_size(rect.min, egui::vec2(rect.width().min(220.0), 1.0)),
        0.0,
        palette.border,
    );
    ui.add_space(2.0);
}

/// Section heading for the items on screen, in the words of the kind on screen.
fn items_heading(kind: Option<magpiecraft_core::MediaKind>, count: usize) -> String {
    let noun = match kind {
        Some(magpiecraft_core::MediaKind::Comics)
        | Some(magpiecraft_core::MediaKind::Manga)
        | Some(magpiecraft_core::MediaKind::Magazines) => "Volumes and issues",
        Some(magpiecraft_core::MediaKind::Video) => "Videos",
        Some(magpiecraft_core::MediaKind::Audio) => "Tracks",
        Some(magpiecraft_core::MediaKind::Ebook) => "Books",
        Some(magpiecraft_core::MediaKind::Photo) => "Photos",
        _ => "Items",
    };
    format!("{noun} ({count})")
}

fn crumbs(ui: &mut egui::Ui, state: &mut BrowseState, palette: &SkinPalette, nav: &mut Nav) {
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
                ui.label(egui::RichText::new("/").color(palette.text_dim));
            }
        }
        if let Some(kind) = state.browse.kind {
            ui.add_space(8.0);
            kind_chip(ui, palette, kind.as_str(), Some(kind));
        }
        if state.browse.mature {
            ui.add_space(8.0);
            kind_chip(ui, palette, "mature", None);
        }
    });
}

/// A small kind tag: coloured dot plus dim text. The colour carries the meaning,
/// so the text never has to.
fn kind_chip(
    ui: &mut egui::Ui,
    palette: &SkinPalette,
    text: &str,
    kind: Option<magpiecraft_core::MediaKind>,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
        let color = match kind {
            Some(k) => palette.kind_color(k),
            None => palette.accent,
        };
        ui.painter().circle_filled(rect.center(), 4.0, color);
        ui.label(egui::RichText::new(text).size(12.0).color(palette.text_dim));
    });
}

/// A row-major grid with guaranteed top alignment: columns are computed from the
/// available width, each row is its own horizontal strip, and every card has the
/// same fixed height. The old horizontal_wrapped flow staggered card tops.
fn grid<R>(
    ui: &mut egui::Ui,
    items: &[R],
    accent: egui::Color32,
    selected: Option<&str>,
    key_of: impl Fn(&R) -> &str,
    mut add_item: impl FnMut(&mut egui::Ui, &R, f32),
) {
    if items.is_empty() {
        return;
    }
    let avail = ui.available_width().max(CARD_WIDTH + CARD_GAP);
    let columns = ((avail + CARD_GAP) / (CARD_WIDTH + CARD_GAP))
        .floor()
        .max(1.0) as usize;
    for row in items.chunks(columns.max(1)) {
        ui.horizontal(|ui| {
            for item in row {
                with_card(ui, selected == Some(key_of(item)), accent, |ui, width| {
                    add_item(ui, item, width)
                });
            }
        });
        ui.add_space(8.0);
    }
}

fn with_card(
    ui: &mut egui::Ui,
    selected: bool,
    accent: egui::Color32,
    add: impl FnOnce(&mut egui::Ui, f32),
) {
    let response = ui.allocate_ui_with_layout(
        egui::vec2(CARD_WIDTH, CARD_HEIGHT),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            add(ui, CARD_WIDTH);
        },
    );
    if selected {
        ui.painter().rect_stroke(
            response.response.rect,
            3.0,
            egui::Stroke::new(2.0, accent),
            egui::StrokeKind::Outside,
        );
    }
    ui.add_space(CARD_GAP - 12.0);
}

/// What a cover card shows when there is no texture yet. A page container gets an
/// optional kind stripe; anything else gets a text label, because a film has no
/// page to use as a cover.
enum CoverFace<'a> {
    Page(Option<Color32>),
    Label(&'a str),
}

fn cover(
    ui: &mut egui::Ui,
    width: f32,
    key: &str,
    covers: &mut CoverCache,
    catalog: &mut Catalog,
    palette: &SkinPalette,
    face: CoverFace<'_>,
) {
    let height = width * 1.5;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    // Drop shadow: a soft dark rect peeking out below and right.
    let shadow_color = if palette.is_light {
        Color32::from_rgba_unmultiplied(60, 50, 40, 70)
    } else {
        Color32::from_rgba_unmultiplied(0, 0, 0, 150)
    };
    ui.painter().rect_filled(
        egui::Rect::from_min_size(
            egui::pos2(rect.left(), rect.top() + 3.0),
            egui::vec2(rect.width(), rect.height()),
        ),
        3.0,
        shadow_color,
    );
    ui.painter().rect_filled(rect, 3.0, palette.card);
    let (label, stripe) = match face {
        CoverFace::Page(stripe) => (None, stripe),
        CoverFace::Label(label) => (Some(label), None),
    };
    if let Some(label) = label {
        // Not a page container: say what it is rather than asking for a cover that
        // cannot exist (a film, a book, a track).
        ui.painter().rect_stroke(
            rect,
            3.0,
            egui::Stroke::new(1.0, palette.text_dim),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(13.0),
            palette.text_dim,
        );
        return;
    }
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
            if let Some(colour) = stripe {
                ui.painter_at(rect).rect_filled(
                    egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 4.0)),
                    0.0,
                    colour,
                );
            }
            return;
        }
    }
    if covers.failed(key) {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "cover failed",
            egui::FontId::proportional(12.0),
            palette.text_dim,
        );
        return;
    }
    match catalog.cover_source(key) {
        Ok(source) => covers.request(key, source),
        Err(e) => {
            ui.painter().rect_stroke(
                rect,
                3.0,
                egui::Stroke::new(1.0, palette.text_dim),
                egui::StrokeKind::Inside,
            );
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("unavailable: {e}"),
                egui::FontId::proportional(11.0),
                palette.text_dim,
            );
            return;
        }
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "...",
        egui::FontId::proportional(18.0),
        palette.text_dim,
    );
}

fn placeholder(ui: &mut egui::Ui, width: f32, palette: &SkinPalette) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, width * 1.5), egui::Sense::hover());
    placeholder_paint(ui, palette, rect);
}

fn placeholder_paint(ui: &egui::Ui, palette: &SkinPalette, rect: egui::Rect) {
    ui.painter().rect_stroke(
        rect,
        3.0,
        egui::Stroke::new(1.0, palette.border),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "folder",
        egui::FontId::proportional(12.0),
        palette.text_dim,
    );
}

fn card_label(
    ui: &mut egui::Ui,
    title: &str,
    sub: &str,
    kind: Option<magpiecraft_core::MediaKind>,
    palette: &SkinPalette,
) -> egui::Response {
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
    match kind {
        Some(k) => {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let (dot, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                ui.painter()
                    .circle_filled(dot.center(), 3.5, palette.kind_color(k));
                ui.label(egui::RichText::new(sub).size(11.5).color(palette.text_dim));
            });
        }
        None => {
            ui.label(egui::RichText::new(sub).size(11.5).color(palette.text_dim));
        }
    }
    title_response
}

fn search_results(
    ui: &mut egui::Ui,
    search: &SearchView,
    covers: &mut CoverCache,
    catalog: &mut Catalog,
    palette: &SkinPalette,
    selected: Option<&str>,
    nav: &mut Nav,
) {
    section(
        ui,
        palette,
        &format!(
            "{} result{}{} via {}",
            search.count,
            if search.count == 1 { "" } else { "s" },
            if search.truncated { "+" } else { "" },
            search.mode
        ),
    );
    grid(
        ui,
        &search.hits,
        palette.accent,
        selected,
        |hit: &magpiecraft_core::SearchHit| hit.key.as_str(),
        |ui, hit, width| {
            cover(
                ui,
                width,
                &hit.key,
                covers,
                catalog,
                palette,
                CoverFace::Page(None),
            );
            let library = if hit.where_.is_empty() {
                hit.library.clone()
            } else {
                format!("{} / {}", hit.library, hit.where_)
            };
            let response = card_label(ui, &hit.title, &library, None, palette);
            if response.double_clicked() {
                *nav = Nav::Read(hit.key.clone());
            } else if response.clicked() {
                *nav = Nav::Select(Selection {
                    open: SelOpen::Read,
                    key: hit.key.clone(),
                    title: hit.title.clone(),
                    subtitle: library,
                    library: Some(hit.library.clone()),
                });
            }
        },
    );
    if search.count == 0 {
        ui.label(
            egui::RichText::new("No matches. Try fewer words, or the substring mode.")
                .color(palette.text_dim),
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
