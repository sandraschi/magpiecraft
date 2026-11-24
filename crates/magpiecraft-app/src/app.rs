//! Desktop shell: menu bar, search box, page routing, status bar, settings,
//! statistics, help and the add-library dialog.
//!
//! Pattern 2 of the fleet UI standard (viewport-centric craft workstation): the
//! top menu bar is File, Edit, View, Reading, Go, Window, Help; the middle is the
//! page; the bottom is a status bar. Every menu item calls the same engine the CLI
//! and MCP use, in process.

use crate::covers::CoverCache;
use crate::library::{self, BrowseState, Nav};
use crate::reader::{Action, ReaderView};
use crate::theme;
use magpiecraft_core::{Catalog, MediaKind, Paths, Settings, StatsView};
use std::sync::mpsc::{channel, Receiver};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Browse,
    Reader,
    Organise,
    Stats,
    Settings,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dialog {
    None,
    AddLibrary,
    About,
    Shortcuts,
    Docs,
    Licences,
    Palette,
}

#[derive(Debug, Clone)]
enum Act {
    None,
    AddLibrary,
    ScanAll,
    ScanThis,
    ScanFull,
    Theme,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    TwoPages(bool),
    HalfPage(bool),
    Rtl,
    Offset(i32),
    SaveSettings,
    GoLibraries,
    GoBack,
    GoStats,
    GoSettings,
    GoHelp,
    GoOrganise,
    PlanRename,
    ApplyPlan(String),
    UndoJournal(String),
    Dialog(Dialog),
    Refresh,
    Quit,
}

pub struct MagpieApp {
    catalog: Catalog,
    settings: Settings,
    page: Page,
    browse: BrowseState,
    browse_key: Option<String>,
    covers: CoverCache,
    reader: Option<ReaderView>,
    stats: Option<StatsView>,
    status: String,
    dark: bool,
    zoom: f32,
    applied_theme: Option<bool>,
    dialog: Dialog,
    palette_filter: String,
    new_name: String,
    new_root: String,
    new_kind: String,
    new_mature: bool,
    search_query: String,
    scan_rx: Option<Receiver<String>>,
    focus_search_next: bool,
    organise: Option<OrganiseView>,
}

/// The Organise page: what is uncertain, what is planned, what was applied.
#[derive(Default, Clone)]
struct OrganiseView {
    review: Vec<serde_json::Value>,
    plans: Vec<serde_json::Value>,
    journals: Vec<serde_json::Value>,
    message: String,
}

pub fn run(paths: Paths, start_key: Option<String>, start_page: &str) -> Result<(), String> {
    let catalog = Catalog::open(paths).map_err(|e| e.to_string())?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 860.0])
            .with_min_inner_size([900.0, 600.0])
            .with_title("MagpieCraft"),
        ..Default::default()
    };
    eframe::run_native(
        "MagpieCraft",
        options,
        Box::new(move |cc| {
            Ok(Box::new(MagpieApp::new(
                cc,
                catalog,
                start_key,
                start_page.to_string(),
            )))
        }),
    )
    .map_err(|e| e.to_string())
}

impl MagpieApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        catalog: Catalog,
        start_key: Option<String>,
        start_page: String,
    ) -> Self {
        theme::apply_text_styles(&cc.egui_ctx);
        let settings = catalog.settings().clone();
        let dark = settings.theme != "light";
        let zoom = settings.zoom.clamp(0.6, 2.5);
        cc.egui_ctx.set_visuals(theme::visuals(dark));
        cc.egui_ctx.set_zoom_factor(zoom);
        let mut app = MagpieApp {
            settings,
            page: Page::Browse,
            browse: BrowseState {
                browse: empty_browse(),
                filter: String::new(),
                search: None,
                continue_row: Vec::new(),
            },
            browse_key: start_key.clone(),
            covers: CoverCache::new(),
            reader: None,
            stats: None,
            status: String::new(),
            dark,
            zoom,
            applied_theme: Some(dark),
            dialog: Dialog::None,
            palette_filter: String::new(),
            new_name: String::new(),
            new_root: String::new(),
            new_kind: "comics".to_string(),
            new_mature: false,
            search_query: String::new(),
            scan_rx: None,
            focus_search_next: false,
            organise: None,
            catalog,
        };
        app.refresh();
        match start_page.as_str() {
            "organise" => {
                app.refresh_organise();
                app.page = Page::Organise;
            }
            "stats" => {
                app.stats = app.catalog.stats().ok();
                app.page = Page::Stats;
            }
            "settings" => app.page = Page::Settings,
            "help" => app.page = Page::Help,
            _ => {}
        }
        if let Some(key) = start_key {
            if key.starts_with("lib:") {
                app.reader = Some(ReaderView::open(&mut app.catalog, &key));
                if app.reader.as_ref().map(|r| r.count()).unwrap_or(0) > 0 {
                    app.page = Page::Reader;
                }
            }
        }
        app
    }

    fn refresh(&mut self) {
        let (browse, error) = BrowseState::load(&mut self.catalog, self.browse_key.clone());
        self.browse = browse;
        if let Some(e) = error {
            self.status = e;
        }
        self.stats = self.catalog.stats().ok();
    }

    fn open_reader(&mut self, key: String) {
        self.reader = Some(ReaderView::open(&mut self.catalog, &key));
        self.page = Page::Reader;
    }

    fn start_scan(&mut self, library: Option<String>, full: bool) {
        let paths = self.catalog.paths().clone();
        let (tx, rx) = channel::<String>();
        let label = library
            .clone()
            .unwrap_or_else(|| "all libraries".to_string());
        std::thread::Builder::new()
            .name("magpie-scan".to_string())
            .spawn(move || {
                let message = match Catalog::open(paths) {
                    Ok(mut catalog) => match catalog.scan(library.as_deref(), full) {
                        Ok(report) => format!(
                            "scan of {}: +{} new, {} changed, {} missing in {} ms",
                            label, report.added, report.updated, report.missing, report.ms
                        ),
                        Err(e) => format!("scan failed: {e}"),
                    },
                    Err(e) => format!("scan could not open the catalogue: {e}"),
                };
                let _ = tx.send(message);
            })
            .ok();
        self.scan_rx = Some(rx);
        self.status = "scanning...".to_string();
    }

    fn apply_actions(&mut self, actions: Vec<Act>, ctx: &egui::Context) {
        for action in actions {
            match action {
                Act::None => {}
                Act::AddLibrary => {
                    self.dialog = Dialog::AddLibrary;
                    self.new_name.clear();
                    self.new_root.clear();
                    self.new_kind = "comics".to_string();
                    self.new_mature = false;
                }
                Act::ScanAll => self.start_scan(None, false),
                Act::ScanThis => self.start_scan(self.browse.browse.library.clone(), false),
                Act::ScanFull => self.start_scan(None, true),
                Act::Theme => {
                    self.dark = !self.dark;
                    self.settings.theme = if self.dark { "dark" } else { "light" }.to_string();
                    self.applied_theme = None;
                    self.save_settings();
                }
                Act::ZoomIn => self.set_zoom(ctx, self.zoom * 1.1),
                Act::ZoomOut => self.set_zoom(ctx, self.zoom / 1.1),
                Act::ZoomReset => self.set_zoom(ctx, 1.0),
                Act::TwoPages(on) => {
                    if let Some(reader) = self.reader.as_mut() {
                        reader.set_two_page(on);
                    }
                }
                Act::HalfPage(on) => {
                    if let Some(reader) = self.reader.as_mut() {
                        reader.set_half(on);
                    }
                }
                Act::Rtl => {
                    if let Some(reader) = self.reader.as_mut() {
                        reader.toggle_rtl();
                    }
                }
                Act::Offset(delta) => {
                    if let Some(reader) = self.reader.as_ref() {
                        let key = reader.key().to_string();
                        let current = self.catalog.pages(&key).map(|p| p.page_offset).unwrap_or(0);
                        let _ = self
                            .catalog
                            .set_page_offset(&key, (current + delta).clamp(-500, 500));
                    }
                }
                Act::SaveSettings => self.save_settings(),
                Act::GoLibraries => {
                    self.browse_key = None;
                    self.page = Page::Browse;
                    self.browse.search = None;
                    self.refresh();
                }
                Act::GoBack => {
                    let mut key = self.browse_key.clone();
                    if let Some(current) = key.clone() {
                        if let Some(ik) = magpiecraft_core::ItemKey::parse(&current) {
                            key = ik.parent().map(|p| p.as_string());
                        }
                    }
                    self.browse_key = key;
                    self.page = Page::Browse;
                    self.browse.search = None;
                    self.refresh();
                }
                Act::GoStats => {
                    self.stats = self.catalog.stats().ok();
                    self.page = Page::Stats;
                }
                Act::GoOrganise => {
                    self.refresh_organise();
                    self.page = Page::Organise;
                }
                Act::PlanRename => {
                    let preset = self
                        .browse
                        .browse
                        .kind
                        .map(|_| "comics")
                        .unwrap_or("comics");
                    match self.catalog.plan_rename(None, preset, 5000) {
                        Ok(value) => {
                            let count = value["operations"].as_u64().unwrap_or(0);
                            let mut view = self.organise.take().unwrap_or_default();
                            view.message = format!(
                                "{count} operation(s) planned. Read the diff, then apply the plan."
                            );
                            self.organise = Some(view);
                            self.refresh_organise();
                        }
                        Err(e) => self.status = format!("plan failed: {e}"),
                    }
                }
                Act::ApplyPlan(id) => match self.catalog.apply_plan(&id, true) {
                    Ok(value) => {
                        self.status = format!(
                            "applied {} operation(s), {} failed",
                            value["applied"].as_u64().unwrap_or(0),
                            value["failed"].as_u64().unwrap_or(0)
                        );
                        self.refresh_organise();
                        self.refresh();
                    }
                    Err(e) => self.status = format!("apply failed: {e}"),
                },
                Act::UndoJournal(id) => match self.catalog.undo(&id, true) {
                    Ok(value) => {
                        self.status = format!(
                            "restored {} operation(s)",
                            value["restored"].as_u64().unwrap_or(0)
                        );
                        self.refresh_organise();
                        self.refresh();
                    }
                    Err(e) => self.status = format!("undo failed: {e}"),
                },
                Act::GoSettings => self.page = Page::Settings,
                Act::GoHelp => self.page = Page::Help,
                Act::Dialog(d) => self.dialog = d,
                Act::Refresh => self.refresh(),
                Act::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            }
        }
    }

    fn set_zoom(&mut self, ctx: &egui::Context, zoom: f32) {
        self.zoom = zoom.clamp(0.6, 2.5);
        ctx.set_zoom_factor(self.zoom);
        self.settings.zoom = self.zoom;
        self.save_settings();
    }

    fn save_settings(&mut self) {
        let settings = self.settings.clone();
        match self.catalog.set_settings(settings) {
            Ok(saved) => self.settings = saved,
            Err(e) => self.status = format!("settings not saved: {e}"),
        }
    }

    fn run_search(&mut self) {
        let query = self.search_query.trim().to_string();
        if query.is_empty() {
            return;
        }
        match self
            .catalog
            .search(&query, 0, magpiecraft_core::SearchMode::Auto)
        {
            Ok(view) => {
                self.status = format!("{} results for {query}", view.count);
                self.browse.search = Some(view);
                self.page = Page::Browse;
            }
            Err(e) => self.status = format!("search failed: {e}"),
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Act>) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Add library...").clicked() {
                    actions.push(Act::AddLibrary);
                    ui.close();
                }
                if ui.button("Scan all libraries").clicked() {
                    actions.push(Act::ScanAll);
                    ui.close();
                }
                if ui.button("Scan this library").clicked() {
                    actions.push(Act::ScanThis);
                    ui.close();
                }
                if ui.button("Full rescan (re-read unchanged files)").clicked() {
                    actions.push(Act::ScanFull);
                    ui.close();
                }
                ui.separator();
                if ui.button("Refresh view").clicked() {
                    actions.push(Act::Refresh);
                    ui.close();
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    actions.push(Act::Quit);
                    ui.close();
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui.button("Settings").clicked() {
                    actions.push(Act::GoSettings);
                    ui.close();
                }
                if ui.button("Save settings now").clicked() {
                    actions.push(Act::SaveSettings);
                    ui.close();
                }
            });
            ui.menu_button("View", |ui| {
                if ui.button("Toggle light / dark").clicked() {
                    actions.push(Act::Theme);
                    ui.close();
                }
                ui.separator();
                if ui.button("Zoom in (Ctrl +)").clicked() {
                    actions.push(Act::ZoomIn);
                    ui.close();
                }
                if ui.button("Zoom out (Ctrl -)").clicked() {
                    actions.push(Act::ZoomOut);
                    ui.close();
                }
                if ui.button("Zoom 100%").clicked() {
                    actions.push(Act::ZoomReset);
                    ui.close();
                }
                ui.separator();
                if ui.button("Statistics").clicked() {
                    actions.push(Act::GoStats);
                    ui.close();
                }
            });
            ui.menu_button("Reading", |ui| {
                let (two, half) = self
                    .reader
                    .as_ref()
                    .map(|r| (r.two_pages(), r.half()))
                    .unwrap_or((false, false));
                if ui.checkbox(&mut { two }, "Two pages (T)").clicked() {
                    actions.push(Act::TwoPages(!two));
                    ui.close();
                }
                if ui.checkbox(&mut { half }, "Half page (H)").clicked() {
                    actions.push(Act::HalfPage(!half));
                    ui.close();
                }
                if ui.button("Right to left (R)").clicked() {
                    actions.push(Act::Rtl);
                    ui.close();
                }
                ui.separator();
                if ui.button("Page offset -1").clicked() {
                    actions.push(Act::Offset(-1));
                    ui.close();
                }
                if ui.button("Page offset +1").clicked() {
                    actions.push(Act::Offset(1));
                    ui.close();
                }
            });
            ui.menu_button("Go", |ui| {
                if ui.button("Libraries").clicked() {
                    actions.push(Act::GoLibraries);
                    ui.close();
                }
                if ui.button("Up one folder").clicked() {
                    actions.push(Act::GoBack);
                    ui.close();
                }
                ui.separator();
                if ui.button("Statistics").clicked() {
                    actions.push(Act::GoStats);
                    ui.close();
                }
                if ui.button("Organise").clicked() {
                    actions.push(Act::GoOrganise);
                    ui.close();
                }
            });
            ui.menu_button("Window", |ui| {
                if ui.button("Browse").clicked() {
                    actions.push(Act::GoLibraries);
                    ui.close();
                }
                if ui.button("Organise").clicked() {
                    actions.push(Act::GoOrganise);
                    ui.close();
                }
                if ui.button("Settings").clicked() {
                    actions.push(Act::GoSettings);
                    ui.close();
                }
                if ui.button("Statistics").clicked() {
                    actions.push(Act::GoStats);
                    ui.close();
                }
                if ui.button("Help").clicked() {
                    actions.push(Act::GoHelp);
                    ui.close();
                }
            });
            ui.menu_button("Help", |ui| {
                if ui.button("Command Palette...").clicked() {
                    actions.push(Act::Dialog(Dialog::Palette));
                    ui.close();
                }
                if ui.button("Keyboard Shortcuts...").clicked() {
                    actions.push(Act::Dialog(Dialog::Shortcuts));
                    ui.close();
                }
                if ui.button("Project Files and Docs...").clicked() {
                    actions.push(Act::Dialog(Dialog::Docs));
                    ui.close();
                }
                if ui.button("Licenses and Notices...").clicked() {
                    actions.push(Act::Dialog(Dialog::Licences));
                    ui.close();
                }
                if ui.button("About MagpieCraft").clicked() {
                    actions.push(Act::Dialog(Dialog::About));
                    ui.close();
                }
            });
        });
    }

    fn search_row(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Search whole collection");
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.search_query)
                    .id_salt("global-search")
                    .desired_width(360.0)
                    .hint_text("words, Enter to search (/)"),
            );
            if self.focus_search_next {
                response.request_focus();
                self.focus_search_next = false;
            }
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.run_search();
            }
            if ui.button("Search").clicked() {
                self.run_search();
            }
            if self.browse.search.is_some() && ui.button("Clear results").clicked() {
                self.browse.search = None;
            }
            if !self.status.is_empty() {
                ui.separator();
                ui.label(egui::RichText::new(self.status.clone()).color(theme::ACCENT));
            }
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let items = self.stats.as_ref().map(|s| s.items).unwrap_or(0);
            let libraries = self.stats.as_ref().map(|s| s.libraries).unwrap_or(0);
            ui.label(format!("{libraries} libraries  |  {items} items"));
            ui.separator();
            ui.label(match self.page {
                Page::Browse => "Browse".to_string(),
                Page::Reader => match self.reader.as_ref() {
                    Some(reader) => format!("Reader  p.{} / {}", reader.page(), reader.count()),
                    None => "Reader".to_string(),
                },
                Page::Stats => "Statistics".to_string(),
                Page::Organise => "Organise".to_string(),
                Page::Settings => "Settings".to_string(),
                Page::Help => "Help".to_string(),
            });
            ui.separator();
            ui.label(format!("zoom {:.0}%", self.zoom * 100.0));
            if self.scan_rx.is_some() {
                ui.separator();
                ui.label(egui::RichText::new("scanning...").color(theme::ACCENT));
            }
            if let Some(browse) = self.browse.browse.library.clone() {
                ui.separator();
                ui.label(browse);
            }
        });
    }

    fn refresh_organise(&mut self) {
        let review = self
            .catalog
            .review(None, 200)
            .ok()
            .and_then(|v| {
                v.get("entries")
                    .and_then(serde_json::Value::as_array)
                    .cloned()
            })
            .unwrap_or_default();
        let plans = self
            .catalog
            .plans()
            .ok()
            .and_then(|v| {
                v.get("plans")
                    .and_then(serde_json::Value::as_array)
                    .cloned()
            })
            .unwrap_or_default();
        let journals = self
            .catalog
            .journals()
            .ok()
            .and_then(|v| {
                v.get("journals")
                    .and_then(serde_json::Value::as_array)
                    .cloned()
            })
            .unwrap_or_default();
        let message = self
            .organise
            .as_ref()
            .map(|v| v.message.clone())
            .unwrap_or_default();
        self.organise = Some(OrganiseView {
            review,
            plans,
            journals,
            message,
        });
    }

    fn organise_page(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Act>) {
        let view = self.organise.clone().unwrap_or_default();
        theme::card(ui, Some(theme::ACCENT), |ui| {
            ui.label(egui::RichText::new("Organise").size(19.0).strong());
            ui.label(
                "Renaming is a plan: you read the diff, then apply it. Every applied plan is journalled \
                 and can be undone. Deleting is a move into quarantine.",
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Plan renames").clicked() {
                    actions.push(Act::PlanRename);
                }
                if ui.button("Refresh").clicked() {
                    actions.push(Act::GoOrganise);
                }
                ui.label(
                    egui::RichText::new(format!(
                        "{} to review, {} plan(s), {} journal(s)",
                        view.review.len(),
                        view.plans.len(),
                        view.journals.len()
                    ))
                    .color(theme::dim_text(ui)),
                );
            });
            if !view.message.is_empty() {
                ui.label(egui::RichText::new(&view.message).color(theme::ACCENT));
            }
        });
        ui.add_space(8.0);

        if !view.review.is_empty() {
            theme::card(ui, None, |ui| {
                ui.label(egui::RichText::new("Needs a human").strong());
                ui.label(
                    egui::RichText::new(
                        "Names the parser is unsure about. Rename the file, or add ComicInfo.xml; the \
                         entry clears on the next scan.",
                    )
                    .color(theme::dim_text(ui)),
                );
                for entry in view.review.iter().take(60) {
                    let confidence = entry["confidence"].as_f64().unwrap_or(0.0);
                    let rel = entry["rel"].as_str().unwrap_or("");
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{confidence:.2}"))
                                .monospace()
                                .color(theme::ACCENT),
                        );
                        ui.label(rel);
                    });
                }
            });
            ui.add_space(8.0);
        }

        if !view.plans.is_empty() {
            theme::card(ui, None, |ui| {
                ui.label(egui::RichText::new("Plans waiting").strong());
                for plan in view.plans.iter().take(10) {
                    let id = plan["id"].as_str().unwrap_or("");
                    let kind = plan["kind"].as_str().unwrap_or("");
                    let ops = plan["operations"].as_u64().unwrap_or(0);
                    ui.horizontal(|ui| {
                        ui.label(format!("{kind}  {ops} operation(s)  {id}"));
                        if ui.button("Apply").clicked() && !id.is_empty() {
                            actions.push(Act::ApplyPlan(id.to_string()));
                        }
                    });
                    if let Some(diff) = plan["diff"].as_str() {
                        ui.label(
                            egui::RichText::new(
                                diff.lines().take(12).collect::<Vec<_>>().join("\n"),
                            )
                            .monospace()
                            .size(11.5)
                            .color(theme::dim_text(ui)),
                        );
                    }
                    ui.separator();
                }
            });
            ui.add_space(8.0);
        }

        if !view.journals.is_empty() {
            theme::card(ui, None, |ui| {
                ui.label(egui::RichText::new("Applied plans").strong());
                for journal in view.journals.iter().take(10) {
                    let id = journal["id"].as_str().unwrap_or("");
                    let kind = journal["kind"].as_str().unwrap_or("");
                    let applied = journal["applied"].as_u64().unwrap_or(0);
                    let undone = journal["undone"].as_bool().unwrap_or(false);
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "{kind}  {applied} operation(s)  {}{}",
                            id,
                            if undone { "  (undone)" } else { "" }
                        ));
                        if !undone && ui.button("Undo").clicked() && !id.is_empty() {
                            actions.push(Act::UndoJournal(id.to_string()));
                        }
                    });
                }
            });
        }
    }

    fn settings_page(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Act>) {
        theme::card(ui, Some(theme::ACCENT), |ui| {
            ui.label(egui::RichText::new("Settings").size(19.0).strong());
            ui.add_space(4.0);
            let mut mature = self.settings.mature_visible;
            if ui
                .checkbox(&mut mature, "Show mature-flagged libraries")
                .changed()
            {
                self.settings.mature_visible = mature;
                self.save_settings();
                actions.push(Act::Refresh);
            }
            ui.label(
                egui::RichText::new(
                    "Mature libraries are hidden from the grid, from search and from exports while this is off.",
                )
                .color(theme::dim_text(ui)),
            );
            ui.add_space(6.0);
            let mut min_pages = self.settings.imgdir_min_pages;
            if ui
                .add(
                    egui::Slider::new(&mut min_pages, 1..=20)
                        .text("images needed for a page folder"),
                )
                .changed()
            {
                self.settings.imgdir_min_pages = min_pages;
                self.save_settings();
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Theme");
                if ui.selectable_label(self.dark, "Dark").clicked() && !self.dark {
                    actions.push(Act::Theme);
                }
                if ui.selectable_label(!self.dark, "Light").clicked() && self.dark {
                    actions.push(Act::Theme);
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(format!("Zoom {:.0}%", self.zoom * 100.0));
                if ui.small_button("100%").clicked() {
                    actions.push(Act::ZoomReset);
                }
                ui.label("Ctrl + mouse wheel zooms anywhere");
            });
        });
        ui.add_space(8.0);
        theme::card(ui, None, |ui| {
            ui.label(egui::RichText::new("Storage").strong());
            ui.label(format!("Home: {}", self.catalog.paths().home().display()));
            ui.label(format!(
                "Catalogue: {}",
                self.catalog.paths().db_file().display()
            ));
            ui.label(format!(
                "Cover cache: {}",
                self.catalog.paths().covers_dir().display()
            ));
            ui.label(
                egui::RichText::new(
                    "MagpieCraft never writes into a library. Covers, caches and manifests live here.",
                )
                .color(theme::dim_text(ui)),
            );
        });
    }

    fn stats_page(&mut self, ui: &mut egui::Ui) {
        let stats = self.stats.clone().unwrap_or_default();
        theme::card(ui, Some(theme::ACCENT), |ui| {
            ui.label(egui::RichText::new("Statistics").size(19.0).strong());
            ui.label(format!(
                "{} libraries, {} items, {} pages, {} series",
                stats.libraries, stats.items, stats.pages, stats.series
            ));
            if stats.missing > 0 {
                ui.label(
                    egui::RichText::new(format!(
                        "{} items are missing (unplugged drive or deleted file); they are kept so progress is not lost",
                        stats.missing
                    ))
                    .color(theme::ACCENT),
                );
            }
            if let Some(ms) = stats.last_scan_ms {
                ui.label(format!("last scan: {ms} ms"));
            }
        });
        ui.add_space(8.0);
        for lib in &stats.per_library {
            theme::card(ui, None, |ui| {
                ui.label(egui::RichText::new(&lib.name).strong());
                ui.label(format!(
                    "{} items  {} pages  {}  {} dirs  ({})",
                    lib.items,
                    lib.pages,
                    human_bytes(lib.bytes),
                    lib.dirs,
                    lib.kind.as_str()
                ));
                if lib.mature {
                    ui.label(egui::RichText::new("mature").color(theme::ACCENT));
                }
            });
            ui.add_space(4.0);
        }
        for drive in &stats.drives {
            theme::card(ui, None, |ui| {
                ui.label(format!(
                    "Drive {} ({})",
                    drive.volume,
                    if drive.online { "online" } else { "offline" }
                ));
                ui.label(format!(
                    "{} items  {}  {}",
                    drive.items,
                    human_bytes(drive.bytes),
                    drive.libraries.join(", ")
                ));
            });
            ui.add_space(4.0);
        }
        theme::card(ui, None, |ui| {
            ui.label(egui::RichText::new("External tools").strong());
            let report = self.catalog.tools_report();
            for (name, label) in [
                ("ffprobe", "ffprobe (technical inspection)"),
                ("seven_zip", "7-Zip (CBR/RAR, Tier 2)"),
                ("pdfcraft", "PdfCraft (PDF pages, Tier 2)"),
            ] {
                let found = report
                    .get(name)
                    .and_then(|t| t.get("found"))
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                let text = if found {
                    format!("{label}: found")
                } else {
                    format!("{label}: not found (optional)")
                };
                ui.label(text);
            }
        });
    }

    fn help_page(&self, ui: &mut egui::Ui) {
        theme::card(ui, Some(theme::ACCENT), |ui| {
            ui.label(egui::RichText::new("MagpieCraft").size(19.0).strong());
            ui.label(format!(
                "Version {} - inventory and read a collection you already own.",
                magpiecraft_core::version()
            ));
            ui.add_space(6.0);
            ui.label(
                "It works on files on your disk. It has no downloaders, no indexer or tracker \
                 integration, no stream rippers and no DRM circumvention. Every rename or move \
                 is a plan shown before it is applied.",
            );
        });
        ui.add_space(8.0);
        theme::card(ui, None, |ui| {
            ui.label(egui::RichText::new("Keyboard").strong());
            for line in [
                "Arrows / Space / Shift+Space: page forward and back",
                "H: half-page mode, top then bottom with 12 percent overlap",
                "T: two-page spread, the cover alone",
                "R: right-to-left reading for this item",
                "Home / End: first and last page",
                "Escape: back to the library",
                "/: jump to the search box",
                "Ctrl + mouse wheel: zoom the whole window",
            ] {
                ui.label(line);
            }
        });
        ui.add_space(8.0);
        theme::card(ui, None, |ui| {
            ui.label(egui::RichText::new("Not there yet").strong());
            for line in [
                "CBR/RAR and PDF pages (Tier 2; 7-Zip and PdfCraft)",
                "Identify and tag from TMDB, MusicBrainz, Open Library (Tier 2)",
                "Renames, NFO and artwork, review queue (Tier 2)",
                "Federated sources and the knowledge graph (Tier 4)",
            ] {
                ui.label(line);
            }
            ui.label(
                egui::RichText::new(
                    "Those paths report not_implemented; none of them pretend to work.",
                )
                .color(theme::dim_text(ui)),
            );
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        match self.dialog {
            Dialog::None => {}
            Dialog::AddLibrary => {
                let mut open = true;
                egui::Window::new("Add library")
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.label("Name");
                        ui.add(egui::TextEdit::singleline(&mut self.new_name).desired_width(320.0));
                        ui.label("Folder");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.new_root)
                                .desired_width(420.0)
                                .hint_text("absolute path, for example D:/Comics"),
                        );
                        ui.horizontal(|ui| {
                            ui.label("Kind");
                            for kind in [
                                MediaKind::Comics,
                                MediaKind::Manga,
                                MediaKind::Magazines,
                                MediaKind::Ebook,
                                MediaKind::Other,
                            ] {
                                if ui
                                    .selectable_label(self.new_kind == kind.as_str(), kind.as_str())
                                    .clicked()
                                {
                                    self.new_kind = kind.as_str().to_string();
                                }
                            }
                        });
                        ui.checkbox(&mut self.new_mature, "Flag as mature (hidden by default)");
                        ui.add_space(4.0);
                        if ui.button("Add library").clicked() {
                            let name = self.new_name.trim().to_string();
                            let root = self.new_root.trim().to_string();
                            let kind = self.new_kind.clone();
                            let mature = self.new_mature;
                            match self.catalog.add_library(&name, &root, &kind, mature, None) {
                                Ok(lib) => {
                                    self.status =
                                        format!("added library {} at {}", lib.name, lib.root);
                                    self.dialog = Dialog::None;
                                    self.refresh();
                                }
                                Err(e) => self.status = e.to_string(),
                            }
                        }
                    });
                if !open {
                    self.dialog = Dialog::None;
                }
            }
            Dialog::Palette => {
                let mut open = true;
                egui::Window::new("Command palette")
                    .open(&mut open)
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.palette_filter)
                                .desired_width(320.0)
                                .hint_text("type a command"),
                        );
                        let needle = self.palette_filter.to_lowercase();
                        let commands = [
                            ("Add library", Act::AddLibrary),
                            ("Scan all libraries", Act::ScanAll),
                            ("Full rescan", Act::ScanFull),
                            ("Toggle theme", Act::Theme),
                            ("Statistics", Act::GoStats),
                            ("Organise", Act::GoOrganise),
                            ("Plan renames", Act::PlanRename),
                            ("Settings", Act::GoSettings),
                            ("Libraries", Act::GoLibraries),
                            ("Help", Act::GoHelp),
                            ("Zoom 100%", Act::ZoomReset),
                            ("Two pages", Act::TwoPages(true)),
                            ("Half page", Act::HalfPage(true)),
                        ];
                        let mut chosen: Option<Act> = None;
                        for (label, act) in commands {
                            if (needle.is_empty() || label.to_lowercase().contains(&needle))
                                && ui.button(label).clicked()
                            {
                                chosen = Some(act);
                            }
                        }
                        if let Some(act) = chosen {
                            let mut batch = vec![act];
                            if self.palette_filter.is_empty() {
                                batch.push(Act::None);
                            }
                            self.palette_filter.clear();
                            self.dialog = Dialog::None;
                            self.apply_actions(batch, ctx);
                        }
                    });
                if !open {
                    self.dialog = Dialog::None;
                }
            }
            Dialog::About | Dialog::Shortcuts | Dialog::Docs | Dialog::Licences => {
                let mut open = true;
                let title = match self.dialog {
                    Dialog::About => "About MagpieCraft",
                    Dialog::Shortcuts => "Keyboard shortcuts",
                    Dialog::Docs => "Project files and docs",
                    _ => "Licenses and notices",
                };
                egui::Window::new(title)
                    .open(&mut open)
                    .collapsible(false)
                    .show(ctx, |ui| match self.dialog {
                        Dialog::About => {
                            ui.label(format!("MagpieCraft {}", magpiecraft_core::version()));
                            ui.label("Inventory, reader and curator for media you already own.");
                            ui.label("MIT OR Apache-2.0. No telemetry, no cloud account.");
                            ui.label(format!(
                                "Catalogue: {}",
                                self.catalog.paths().db_file().display()
                            ));
                        }
                        Dialog::Shortcuts => {
                            for line in [
                                "Arrows, Space, Shift+Space: page",
                                "H, T, R: half page, two pages, right to left",
                                "Home, End: first and last page",
                                "Escape: back",
                                "/: search",
                                "Ctrl + mouse wheel: zoom",
                            ] {
                                ui.label(line);
                            }
                        }
                        Dialog::Docs => {
                            ui.label("README.md: what this is and what is not built yet");
                            ui.label("docs/TOOLS.md: CLI and MCP tool reference");
                            ui.label("docs/READER_PARITY.md: how the reader matches the prototype");
                            ui.label("docs/ARCHITECTURE.md: crates and the command bus");
                            ui.label("HANDOVER.md: what the next agent must know");
                        }
                        _ => {
                            ui.label("MagpieCraft is MIT OR Apache-2.0.");
                            ui.label("External tools are invoked, never linked:");
                            ui.label("ffprobe (FFmpeg project), 7-Zip (Igor Pavlov), PdfCraft.");
                            ui.label(
                                "Metadata providers keep their own terms; see docs/ATTRIBUTION.md.",
                            );
                        }
                    });
                if !open {
                    self.dialog = Dialog::None;
                }
            }
        }
    }

    fn poll_scan(&mut self) {
        let message = match self.scan_rx.as_ref().map(|rx| rx.try_recv()) {
            Some(Ok(message)) => Some(message),
            Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                Some("scan thread stopped unexpectedly".to_string())
            }
            _ => None,
        };
        if let Some(message) = message {
            self.status = message;
            self.scan_rx = None;
            self.refresh();
        }
    }

    fn handle_app_keys(&mut self, ctx: &egui::Context) {
        let typing = ctx.memory(|m| m.focused().is_some());
        let (slash, escape, ctrl_f, zoomin, zoomout, zero) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Slash),
                i.key_pressed(egui::Key::Escape),
                i.key_pressed(egui::Key::F) && (i.modifiers.ctrl || i.modifiers.command),
                i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals),
                i.key_pressed(egui::Key::Minus),
                i.key_pressed(egui::Key::Num0),
            )
        });
        let ctrl = ctx.input(|i| i.modifiers.ctrl || i.modifiers.command);
        if ctrl && zoomin {
            self.set_zoom(ctx, self.zoom * 1.1);
        }
        if ctrl && zoomout {
            self.set_zoom(ctx, self.zoom / 1.1);
        }
        if ctrl && zero {
            self.set_zoom(ctx, 1.0);
        }
        if !typing && (slash || ctrl_f) {
            self.focus_search_next = true;
        }
        if escape && self.dialog != Dialog::None {
            self.dialog = Dialog::None;
        }
    }
}

impl eframe::App for MagpieApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(next) = theme::handle_ctrl_wheel(ctx, self.zoom) {
            self.set_zoom(ctx, next);
        }
        self.poll_scan();
        self.handle_app_keys(ctx);
        self.covers.poll(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.applied_theme != Some(self.dark) {
            ctx.set_visuals(theme::visuals(self.dark));
            self.applied_theme = Some(self.dark);
        }
        let mut actions: Vec<Act> = Vec::new();

        // `App::ui` hands over an unbounded Ui, so panels do not size themselves.
        // Lay the chrome out in flow and give the page and the status bar explicit
        // rects taken from the real window rect.
        self.top_bar(ui, &mut actions);
        ui.separator();
        self.search_row(ui);
        ui.separator();

        let window = ctx.content_rect();
        let status_height = 26.0;
        let content = egui::Rect::from_min_max(
            egui::pos2(window.left(), ui.cursor().top()),
            egui::pos2(
                window.right(),
                (window.bottom() - status_height).max(ui.cursor().top() + 80.0),
            ),
        );
        let status = egui::Rect::from_min_max(
            egui::pos2(window.left(), content.bottom()),
            egui::pos2(window.right(), window.bottom()),
        );

        ui.scope_builder(egui::UiBuilder::new().max_rect(content), |ui| {
            match self.page {
                Page::Browse => {
                    let nav = egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            library::show(ui, &mut self.browse, &mut self.covers, &mut self.catalog)
                        })
                        .inner;
                    match nav {
                        Nav::Open(key) => {
                            self.browse_key = key;
                            self.browse.search = None;
                            self.refresh();
                        }
                        Nav::Read(key) => self.open_reader(key),
                        Nav::None => {}
                    }
                }
                Page::Reader => {
                    let mut back = false;
                    if let Some(reader) = self.reader.as_mut() {
                        if let Action::Back = reader.show(ui, &mut self.catalog, content) {
                            back = true;
                        }
                    } else {
                        ui.label("No item is open.");
                    }
                    if back {
                        self.page = Page::Browse;
                        self.reader = None;
                        self.refresh();
                    }
                }
                Page::Stats => {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| self.stats_page(ui));
                }
                Page::Organise => {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| self.organise_page(ui, &mut actions));
                }
                Page::Settings => {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| self.settings_page(ui, &mut actions));
                }
                Page::Help => {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| self.help_page(ui));
                }
            }
        });

        ui.scope_builder(egui::UiBuilder::new().max_rect(status), |ui| {
            self.status_bar(ui);
        });

        self.apply_actions(actions, &ctx);
        self.dialogs(&ctx);
    }
}

fn empty_browse() -> magpiecraft_core::BrowseView {
    magpiecraft_core::BrowseView {
        kind: None,
        library: None,
        mature: false,
        crumbs: Vec::new(),
        folders: Vec::new(),
        items: Vec::new(),
        total_folders: 0,
        total_items: 0,
    }
}

fn human_bytes(n: i64) -> String {
    let mut value = n as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < 4 {
        value /= 1024.0;
        unit += 1;
    }
    match unit {
        0 => format!("{n} B"),
        1 => format!("{value:.1} KiB"),
        2 => format!("{value:.1} MiB"),
        3 => format!("{value:.1} GiB"),
        _ => format!("{value:.2} TiB"),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn byte_sizes_are_human() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1536), "1.5 KiB");
        assert!(human_bytes(3_221_225_472).ends_with("GiB"));
    }

    #[test]
    fn an_empty_browse_view_is_valid() {
        let view = empty_browse();
        assert!(view.crumbs.is_empty());
        assert_eq!(view.total_items, 0);
    }
}
