//! Desktop shell: menu bar, search box, page routing, status bar, settings,
//! statistics, help and the add-library dialog.
//!
//! Pattern 2 of the fleet UI standard (viewport-centric craft workstation): the
//! top menu bar is File, Edit, View, Reading, Go, Window, Help; the middle is the
//! page; the bottom is a status bar. Every menu item calls the same engine the CLI
//! and MCP use, in process.

use crate::chat::{ChatDock, ChatLine};
use crate::covers::CoverCache;
use crate::library::{self, BrowseState, Nav, Selection};
use crate::reader::{Action, ReaderView};
use crate::skins::{SkinId, SkinPalette};
use crate::theme;
use magpiecraft_core::llm::{DiscoveredModel, LlmConfig, LlmProvider};
use magpiecraft_core::{Catalog, MediaKind, Paths, Settings, StatsView};
use serde_json::{json, Value};
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

/// Help Center tabs. The Help menu opens each one directly; the order matches
/// the menu and the fleet Help content standard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HelpTab {
    GettingStarted,
    Functions,
    Usage,
    Tips,
    Pitfalls,
    Planned,
    Skills,
    Docs,
}

impl HelpTab {
    const ALL: [Self; 8] = [
        Self::GettingStarted,
        Self::Functions,
        Self::Usage,
        Self::Tips,
        Self::Pitfalls,
        Self::Planned,
        Self::Skills,
        Self::Docs,
    ];

    fn label(self) -> &'static str {
        match self {
            HelpTab::GettingStarted => "Get started",
            HelpTab::Functions => "Functions",
            HelpTab::Usage => "Usage",
            HelpTab::Tips => "Tips and tricks",
            HelpTab::Pitfalls => "Pitfalls and safety",
            HelpTab::Planned => "Planned",
            HelpTab::Skills => "Skills",
            HelpTab::Docs => "Project files",
        }
    }
}

#[derive(Debug, Clone)]
enum Act {
    None,
    AddLibrary,
    ScanAll,
    ScanThis,
    ScanFull,
    Skin(SkinId),
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
    ScanLibrary(String),
    RemoveLibrary(String),
    HelpTab(HelpTab),
    LlmDiscover,
    LlmProvider(LlmProvider),
    LlmUseModel(String),
    LlmSaveKey,
    LlmForgetKey,
    LlmTest,
    LlmSave,
    LlmClassifierProvider(LlmProvider),
    LlmClassifierClear,
    ToggleChat,
    ChatSend,
    ChatClear,
    ChatPrompt,
    CheckDrives,
    ScanDrives,
    ProposeAdd {
        name: String,
        root: String,
        kind: String,
    },
    ProposeAddAll,
    ClassifyUnknowns,
    CancelClassify,
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
    /// Single-click selection shown in the inspector. Never opened
    /// implicitly: only double-click, Enter or an Open button follows it.
    selection: Option<Selection>,
    covers: CoverCache,
    reader: Option<ReaderView>,
    stats: Option<StatsView>,
    status: String,
    skin: SkinId,
    zoom: f32,
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
    /// Library name armed for removal. The row shows Confirm/Keep instead of
    /// removing on the first click.
    remove_armed: Option<String>,
    /// Last drive proposal scan (JSON value, never cached in the catalogue).
    drive_proposals: Option<Value>,
    drive_propose_busy: bool,
    drive_propose_rx: Option<Receiver<Value>>,
    /// Editable drive roots buffer (one per line). Saved explicitly.
    drive_roots_text: String,
    /// Per-row kind overrides when confirming proposals, by folder path.
    propose_kinds: std::collections::HashMap<String, String>,
    /// AI classification run over unknown proposals.
    classify_busy: bool,
    classify_rx: Option<Receiver<Vec<ClassifyHit>>>,
    classify_cancel: Option<magpiecraft_core::llm::Cancel>,
    /// Selected Help Center tab.
    help_tab: HelpTab,
    /// LLM provider state. The file is shared across craft apps; this is the
    /// in-memory copy, saved on every change.
    llm: LlmConfig,
    /// Last discovery result for the model picker.
    llm_models: Vec<DiscoveredModel>,
    llm_scan_error: Option<String>,
    llm_scanning: bool,
    llm_scan_rx: Option<Receiver<(Vec<DiscoveredModel>, Option<String>)>>,
    llm_testing: bool,
    llm_test_rx: Option<Receiver<Result<Value, String>>>,
    llm_test_out: Option<String>,
    /// BYOK field. Starts empty every launch; only non-empty entries are stored.
    llm_key_input: String,
    /// Chat dock state. The worker owns its catalogue handle.
    chat: ChatDock,
    /// Last drive health report (live probe, never cached in the catalogue).
    drive_report: Option<Value>,
    drive_busy: bool,
    drive_rx: Option<Receiver<Value>>,
}

/// The Organise page: what is uncertain, what is planned, what was applied.
#[derive(Default, Clone)]
struct OrganiseView {
    review: Vec<serde_json::Value>,
    plans: Vec<serde_json::Value>,
    journals: Vec<serde_json::Value>,
    message: String,
}

/// One AI-classified proposal, sent back from the worker thread.
struct ClassifyHit {
    path: String,
    kind: String,
    confidence: f32,
    reason: String,
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
        let skin = SkinId::parse(&settings.skin).unwrap_or_default();
        let zoom = settings.zoom.clamp(0.6, 2.5);
        cc.egui_ctx.set_visuals(theme::visuals(&skin.palette()));
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
            selection: None,
            covers: CoverCache::new(),
            reader: None,
            stats: None,
            status: String::new(),
            skin,
            zoom,
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
            remove_armed: None,
            drive_proposals: None,
            drive_propose_busy: false,
            drive_propose_rx: None,
            propose_kinds: std::collections::HashMap::new(),
            classify_busy: false,
            classify_rx: None,
            classify_cancel: None,
            drive_roots_text: String::new(),
            help_tab: HelpTab::GettingStarted,
            llm: magpiecraft_core::llm::load_config(),
            llm_models: Vec::new(),
            llm_scan_error: None,
            llm_scanning: false,
            llm_scan_rx: None,
            llm_testing: false,
            llm_test_rx: None,
            llm_test_out: None,
            llm_key_input: String::new(),
            chat: ChatDock::new(),
            drive_report: None,
            drive_busy: false,
            drive_rx: None,
            catalog,
        };
        app.refresh();
        app.drive_roots_text = app.settings.drive_roots.join("\n");
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

    fn palette(&self) -> SkinPalette {
        self.skin.palette()
    }

    /// User-tuned decor on top of the skin, from Settings.
    fn decor(&self) -> theme::DecorOpts {
        theme::DecorOpts {
            border_width: self.settings.border_width,
            backdrop_amount: self.settings.backdrop_amount,
        }
    }

    fn set_skin(&mut self, skin: SkinId) {
        self.skin = skin;
        self.settings.skin = skin.id().to_string();
        self.save_settings();
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

    /// Follow one navigation action from the grid, the tree, the search
    /// results or the Enter key. Selection never opens on its own; only this
    /// path and double-click do.
    fn follow_nav(&mut self, nav: Nav) {
        match nav {
            Nav::Open(key) => {
                self.browse_key = key;
                self.browse.search = None;
                self.refresh();
            }
            Nav::Read(key) => self.open_reader(key),
            Nav::Launch(key) => match self.catalog.open_item(&key) {
                Ok(report) => {
                    let player = report["player"].as_str().unwrap_or("player");
                    let start = report["start_secs"].as_i64().unwrap_or(0);
                    self.status = if start > 0 {
                        format!(
                            "opened in {player} at {}",
                            magpiecraft_core::format_secs(u64::try_from(start).unwrap_or(0))
                        )
                    } else {
                        format!("opened in {player}")
                    };
                    self.refresh();
                }
                Err(e) => self.status = format!("cannot open: {e}"),
            },
            Nav::Select(sel) => {
                self.selection = Some(sel);
            }
            Nav::None => {}
        }
    }

    /// Open whatever is currently selected (Enter key).
    fn open_selected(&mut self) {
        let nav = match self.selection.clone() {
            Some(sel) => match sel.open {
                library::SelOpen::Folder => Nav::Open(Some(sel.key)),
                library::SelOpen::Read => Nav::Read(sel.key),
                library::SelOpen::Launch => Nav::Launch(sel.key),
            },
            None => Nav::None,
        };
        self.follow_nav(nav);
    }

    /// Poll the LLM worker threads. Blocking provider calls run on spawned
    /// threads; this drains their channels every frame and repaints while busy.
    fn poll_llm(&mut self, ctx: &egui::Context) {
        let mut busy = false;
        if let Some(rx) = &self.llm_scan_rx {
            match rx.try_recv() {
                Ok((models, error)) => {
                    self.llm_models = models;
                    self.llm_scan_error = error;
                    self.llm_scanning = false;
                    self.llm_scan_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => busy = true,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.llm_scan_error = Some("discovery thread stopped".to_string());
                    self.llm_scanning = false;
                    self.llm_scan_rx = None;
                }
            }
        }
        if let Some(rx) = &self.llm_test_rx {
            match rx.try_recv() {
                Ok(result) => {
                    self.llm_test_out = Some(match result {
                        Ok(value) => format!(
                            "ok in {} ms: {}",
                            value.get("ms").and_then(Value::as_u64).unwrap_or(0),
                            value.get("reply").and_then(Value::as_str).unwrap_or("")
                        ),
                        Err(e) => format!("test failed: {e}"),
                    });
                    self.llm_testing = false;
                    self.llm_test_rx = None;
                    self.refresh_llm();
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => busy = true,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.llm_test_out = Some("test thread stopped".to_string());
                    self.llm_testing = false;
                    self.llm_test_rx = None;
                }
            }
        }
        if busy {
            ctx.request_repaint();
        }
    }

    /// Re-read the shared LLM config (CLI/MCP edits show up live).
    fn refresh_llm(&mut self) {
        self.llm = magpiecraft_core::llm::load_config();
    }

    fn persist_llm(&mut self, message: &str) {
        match magpiecraft_core::llm::save_config(&self.llm) {
            Ok(()) => self.status = message.to_string(),
            Err(e) => self.status = format!("LLM settings not saved: {e}"),
        }
    }

    /// Probe the configured provider on a worker thread. Never blocks the UI.
    fn start_discover(&mut self) {
        if self.llm_scanning {
            return;
        }
        let config = self.llm.clone();
        let (tx, rx) = channel::<(Vec<DiscoveredModel>, Option<String>)>();
        self.llm_scan_rx = Some(rx);
        self.llm_scanning = true;
        self.llm_scan_error = None;
        std::thread::Builder::new()
            .name("magpie-llm-scan".to_string())
            .spawn(move || {
                let out = magpiecraft_core::llm::discover_models(&config);
                let _ = tx.send(out);
            })
            .ok();
    }

    /// A provider switch starts with an empty model: no blind defaults.
    fn set_llm_provider(&mut self, provider: LlmProvider) {
        self.llm.provider = provider.id().to_string();
        self.llm.model.clear();
        self.llm_models.clear();
        self.llm_key_input.clear();
        self.llm_test_out = None;
        self.persist_llm("provider switched: pick an installed model");
    }

    /// A classifier override starts with an empty model: no blind defaults.
    /// Clearing both fields returns to following the chat pair.
    fn set_llm_classifier_provider(&mut self, provider: LlmProvider) {
        self.llm.classify_provider = provider.id().to_string();
        self.llm.classify_model.clear();
        self.persist_llm("classifier override set: pick a model for it");
    }

    /// Persist the pick through the bus (Ollama also switches VRAM) and
    /// re-read the shared file.
    fn use_llm_model(&mut self, model: String) {
        use magpiecraft_core::command::{dispatch, Command};
        let out = dispatch(
            &mut self.catalog,
            Command::LlmSelect {
                provider: self.llm.provider.clone(),
                model,
                slot: None,
            },
        );
        if out.get("ok").and_then(Value::as_bool) == Some(true) {
            self.status = format!(
                "LLM is now {} {}",
                out.get("provider").and_then(Value::as_str).unwrap_or(""),
                out.get("model").and_then(Value::as_str).unwrap_or("")
            );
        } else {
            self.status = format!(
                "LLM select failed: {}",
                out.get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
            );
        }
        self.refresh_llm();
    }

    /// Store the BYOK field, or forget the stored key. Empty keeps storage.
    fn save_llm_key(&mut self, forget: bool) {
        use magpiecraft_core::command::{dispatch, Command};
        let key = self.llm_key_input.trim().to_string();
        if key.is_empty() && !forget {
            self.status = "type a key first, or Forget to clear the stored one".to_string();
            return;
        }
        let out = dispatch(
            &mut self.catalog,
            Command::LlmSetKey {
                provider: self.llm.provider.clone(),
                api_key: key,
                clear: forget,
            },
        );
        self.llm_key_input.clear();
        self.status = if out.get("ok").and_then(Value::as_bool) == Some(true) {
            "key stored (presence only from here on)".to_string()
        } else {
            format!(
                "key not stored: {}",
                out.get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
            )
        };
        self.refresh_llm();
    }

    /// One live completion on a worker thread; the UI never waits on it.
    fn start_llm_test(&mut self) {
        if self.llm_testing {
            return;
        }
        let config = self.llm.clone();
        let (tx, rx) = channel::<Result<Value, String>>();
        self.llm_test_rx = Some(rx);
        self.llm_testing = true;
        self.llm_test_out = None;
        std::thread::Builder::new()
            .name("magpie-llm-test".to_string())
            .spawn(move || {
                let out = magpiecraft_core::llm::test_completion(&config, None)
                    .map_err(|e| e.to_string());
                let _ = tx.send(out);
            })
            .ok();
    }

    /// What the chat sees: the open reader position or the browse spot.
    fn chat_selection(&self) -> Value {
        if let Some(reader) = &self.reader {
            json!({
                "view": "reader",
                "key": reader.key(),
                "page": reader.page(),
                "count": reader.count(),
            })
        } else {
            json!({
                "view": "browse",
                "library": self.browse.browse.library,
                "key": self.browse_key,
                "filter": self.browse.filter,
            })
        }
    }

    /// Live catalogue status for the chat preprompt, truncated by the worker.
    fn chat_status(&mut self) -> Value {
        use magpiecraft_core::command::{dispatch, Command};
        dispatch(&mut self.catalog, Command::Status)
    }

    /// Send the chat input on a worker thread. Never blocks the UI.
    fn send_chat(&mut self) {
        if !self.chat.open {
            self.chat.open = true;
        }
        let status = self.chat_status();
        let selection = self.chat_selection();
        let paths = self.catalog.paths().clone();
        let config = self.llm.clone();
        self.chat.send(&config, 10, status, selection, paths);
    }

    /// Probe the physical disks on a worker thread. A sick drive must never
    /// freeze the window.
    fn start_drive_check(&mut self) {
        if self.drive_busy {
            return;
        }
        let paths = self.catalog.paths().clone();
        let (tx, rx) = channel::<Value>();
        self.drive_rx = Some(rx);
        self.drive_busy = true;
        std::thread::Builder::new()
            .name("magpie-drives".to_string())
            .spawn(move || {
                let report = match Catalog::open(paths) {
                    Ok(mut catalog) => {
                        use magpiecraft_core::command::{dispatch, Command};
                        dispatch(&mut catalog, Command::DriveHealth)
                    }
                    Err(e) => json!({"ok": false, "error": e.to_string()}),
                };
                let _ = tx.send(report);
            })
            .ok();
    }

    /// Poll the drive worker. Call every frame; repaints while busy.
    /// Propose libraries from the drive roots on a worker thread. Nothing is
    /// created; confirming a proposal adds a real library.
    fn start_propose(&mut self) {
        if self.drive_propose_busy {
            return;
        }
        let paths = self.catalog.paths().clone();
        let (tx, rx) = channel::<Value>();
        self.drive_propose_rx = Some(rx);
        self.drive_propose_busy = true;
        self.propose_kinds.clear();
        std::thread::Builder::new()
            .name("magpie-propose".to_string())
            .spawn(move || {
                let report = match Catalog::open(paths) {
                    Ok(mut catalog) => {
                        use magpiecraft_core::command::{dispatch, Command};
                        dispatch(
                            &mut catalog,
                            Command::ProposeLibraries {
                                roots: None,
                                media_folder: None,
                            },
                        )
                    }
                    Err(e) => json!({"ok": false, "error": e.to_string()}),
                };
                let _ = tx.send(report);
            })
            .ok();
    }

    /// Poll the proposal worker. Call every frame; repaints while busy.
    fn poll_propose(&mut self, ctx: &egui::Context) {
        let mut busy = false;
        if let Some(rx) = &self.drive_propose_rx {
            match rx.try_recv() {
                Ok(report) => {
                    self.drive_proposals = Some(report);
                    self.drive_propose_busy = false;
                    self.drive_propose_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => busy = true,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.status = "proposal scan stopped unexpectedly".to_string();
                    self.drive_propose_busy = false;
                    self.drive_propose_rx = None;
                }
            }
        }
        if busy {
            ctx.request_repaint();
        }
    }

    /// Classify the unknown proposals with the effective classifier pair on
    /// a worker thread. Caps at 50 folders per run; results merge back into
    /// the proposal list for user review, never auto-confirm.
    fn start_classify(&mut self) {
        if self.classify_busy {
            return;
        }
        let Some(report) = self.drive_proposals.clone() else {
            return;
        };
        let unknowns: Vec<(String, String, Vec<String>)> = report
            .get("proposals")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|p| {
                p.get("kind")
                    .and_then(Value::as_str)
                    .is_none_or(|k| k == "unknown" || k.trim().is_empty())
            })
            .take(50)
            .filter_map(|p| {
                let path = p.get("path")?.as_str()?.to_string();
                if self.propose_kinds.contains_key(&path) {
                    return None;
                }
                let name = p.get("name")?.as_str().unwrap_or("?").to_string();
                let samples = p
                    .get("samples")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect();
                Some((path, name, samples))
            })
            .collect();
        if unknowns.is_empty() {
            self.status = "nothing unknown left to classify".to_string();
            return;
        }
        let config = self.llm.clone();
        let (tx, rx) = channel::<Vec<ClassifyHit>>();
        self.classify_rx = Some(rx);
        self.classify_busy = true;
        let cancel = magpiecraft_core::llm::Cancel::new();
        self.classify_cancel = Some(cancel.clone());
        std::thread::Builder::new()
            .name("magpie-classify".to_string())
            .spawn(move || {
                let mut hits = Vec::new();
                for (path, name, samples) in unknowns {
                    if cancel.is_cancelled() {
                        break;
                    }
                    match magpiecraft_core::llm::classify_folder_ai(
                        &config, &name, &samples, &cancel,
                    ) {
                        Ok((kind, confidence, reason)) => hits.push(ClassifyHit {
                            path,
                            kind,
                            confidence,
                            reason,
                        }),
                        Err(e) => hits.push(ClassifyHit {
                            path,
                            kind: "unknown".to_string(),
                            confidence: 0.0,
                            reason: format!("AI failed: {e}"),
                        }),
                    }
                }
                let _ = tx.send(hits);
            })
            .ok();
    }

    /// Merge classifier results into the proposal list. A hit only fills an
    /// unknown row; user overrides in propose_kinds always win.
    fn poll_classify(&mut self, ctx: &egui::Context) {
        let mut busy = false;
        if let Some(rx) = &self.classify_rx {
            match rx.try_recv() {
                Ok(hits) => {
                    if let Some(report) = self.drive_proposals.as_mut() {
                        if let Some(list) =
                            report.get_mut("proposals").and_then(Value::as_array_mut)
                        {
                            for item in list.iter_mut() {
                                let Some(path) =
                                    item.get("path").and_then(Value::as_str).map(str::to_string)
                                else {
                                    continue;
                                };
                                if self.propose_kinds.contains_key(&path) {
                                    continue;
                                }
                                if let Some(hit) = hits.iter().find(|h| h.path == path) {
                                    if let Some(obj) = item.as_object_mut() {
                                        obj.insert(
                                            "kind".to_string(),
                                            Value::String(hit.kind.clone()),
                                        );
                                        obj.insert(
                                            "confidence".to_string(),
                                            Value::from(hit.confidence),
                                        );
                                        obj.insert(
                                            "reason".to_string(),
                                            Value::String(format!("AI: {}", hit.reason)),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    let n = hits.len();
                    self.status = format!("AI classified {n} folder(s); review and confirm");
                    self.classify_busy = false;
                    self.classify_rx = None;
                    self.classify_cancel = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => busy = true,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.status = "classification stopped unexpectedly".to_string();
                    self.classify_busy = false;
                    self.classify_rx = None;
                    self.classify_cancel = None;
                }
            }
        }
        if busy {
            ctx.request_repaint();
        }
    }

    /// Confirm one proposal as a real library (kind may be overridden in the row).
    fn confirm_proposal(&mut self, name: String, root: String, kind: String) {
        if self
            .catalog
            .add_library(&name, &root, &kind, false, None)
            .is_ok()
        {
            self.status = format!("added library {name} at {root}");
            self.propose_kinds.remove(&root);
            self.refresh();
        } else {
            self.status = "add failed (duplicate name?)".to_string();
        }
    }

    /// Confirm every proposal that already has a kind. Unknowns stay for review.
    fn confirm_all_proposals(&mut self) {
        let proposals = self
            .drive_proposals
            .clone()
            .and_then(|v| v.get("proposals").cloned())
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        let mut added = 0;
        for p in &proposals {
            let Some(root) = p.get("path").and_then(Value::as_str) else {
                continue;
            };
            let name = p
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("?")
                .to_string();
            let kind = self
                .propose_kinds
                .get(root)
                .cloned()
                .or_else(|| p.get("kind").and_then(Value::as_str).map(str::to_string));
            if let Some(kind) = kind {
                if self
                    .catalog
                    .add_library(&name, root, &kind, false, None)
                    .is_ok()
                {
                    added += 1;
                }
            }
        }
        self.propose_kinds.clear();
        self.status = format!("added {added} libraries from the drive scan");
        self.refresh();
    }

    fn poll_drives(&mut self, ctx: &egui::Context) {
        let mut busy = false;
        if let Some(rx) = &self.drive_rx {
            match rx.try_recv() {
                Ok(report) => {
                    self.drive_report = Some(report);
                    self.drive_busy = false;
                    self.drive_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => busy = true,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.status = "drive check stopped unexpectedly".to_string();
                    self.drive_busy = false;
                    self.drive_rx = None;
                }
            }
        }
        if busy {
            ctx.request_repaint();
        }
    }

    /// Removing a library drops its index rows only. The files on disk are
    /// never touched; the engine requires the confirm flag for the same reason.
    fn remove_library(&mut self, name: String) {
        match self.catalog.remove_library(&name, true) {
            Ok(value) => {
                let dropped = value
                    .get("items_dropped_from_index")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                self.status = format!(
                    "removed library {name} ({dropped} index rows dropped, files untouched)"
                );
                self.remove_armed = None;
                self.refresh();
            }
            Err(e) => self.status = format!("remove failed: {e}"),
        }
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
                Act::ScanLibrary(name) => {
                    self.remove_armed = None;
                    self.start_scan(Some(name), false);
                }
                Act::RemoveLibrary(name) => self.remove_library(name),
                Act::HelpTab(tab) => {
                    self.page = Page::Help;
                    self.help_tab = tab;
                }
                Act::LlmDiscover => self.start_discover(),
                Act::LlmProvider(provider) => self.set_llm_provider(provider),
                Act::LlmUseModel(model) => self.use_llm_model(model),
                Act::LlmSaveKey => self.save_llm_key(false),
                Act::LlmForgetKey => self.save_llm_key(true),
                Act::LlmTest => self.start_llm_test(),
                Act::LlmSave => self.persist_llm("LLM settings saved"),
                Act::LlmClassifierProvider(provider) => self.set_llm_classifier_provider(provider),
                Act::LlmClassifierClear => {
                    self.llm.classify_provider.clear();
                    self.llm.classify_model.clear();
                    self.persist_llm("Classifier follows the chat pair");
                }
                Act::ToggleChat => {
                    self.chat.open = !self.chat.open;
                }
                Act::ChatSend => self.send_chat(),
                Act::ChatClear => self.chat.clear(),
                Act::ChatPrompt => {
                    self.chat.show_prompt = !self.chat.show_prompt;
                }
                Act::CheckDrives => self.start_drive_check(),
                Act::ScanDrives => self.start_propose(),
                Act::ProposeAdd { name, root, kind } => self.confirm_proposal(name, root, kind),
                Act::ProposeAddAll => self.confirm_all_proposals(),
                Act::ClassifyUnknowns => self.start_classify(),
                Act::CancelClassify => {
                    if let Some(cancel) = self.classify_cancel.take() {
                        cancel.cancel();
                    }
                }
                Act::Skin(skin) => self.set_skin(skin),
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
                    let preset = "auto";
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
                Act::GoSettings => {
                    self.page = Page::Settings;
                    self.refresh();
                }
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
                ui.menu_button("Skin", |ui| {
                    for skin in SkinId::ALL {
                        let mut checked = self.skin == skin;
                        if ui.checkbox(&mut checked, skin.label()).changed() {
                            actions.push(Act::Skin(skin));
                            ui.close();
                        }
                    }
                });
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
                ui.separator();
                let chat_label = if self.chat.open {
                    "Close chat"
                } else {
                    "Chat (Ctrl+J)"
                };
                if ui.button(chat_label).clicked() {
                    actions.push(Act::ToggleChat);
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
                ui.separator();
                if ui.button("Getting Started...").clicked() {
                    actions.push(Act::HelpTab(HelpTab::GettingStarted));
                    ui.close();
                }
                if ui.button("Usage (GUI, CLI and MCP)...").clicked() {
                    actions.push(Act::HelpTab(HelpTab::Usage));
                    ui.close();
                }
                if ui.button("Tips and Tricks...").clicked() {
                    actions.push(Act::HelpTab(HelpTab::Tips));
                    ui.close();
                }
                if ui.button("Common Pitfalls and Safety...").clicked() {
                    actions.push(Act::HelpTab(HelpTab::Pitfalls));
                    ui.close();
                }
                if ui.button("Planned Extensions...").clicked() {
                    actions.push(Act::HelpTab(HelpTab::Planned));
                    ui.close();
                }
                if ui.button("Bundled Skills...").clicked() {
                    actions.push(Act::HelpTab(HelpTab::Skills));
                    ui.close();
                }
                if ui.button("Project Files and Docs...").clicked() {
                    actions.push(Act::Dialog(Dialog::Docs));
                    ui.close();
                }
                ui.separator();
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
        let palette = self.palette();
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
                ui.label(egui::RichText::new(self.status.clone()).color(palette.accent));
            }
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
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
            if let Some(sel) = &self.selection {
                ui.separator();
                ui.label(format!("Selected: {}", sel.title));
            }
            if self.scan_rx.is_some() {
                ui.separator();
                ui.label(egui::RichText::new("scanning...").color(palette.accent));
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
        let palette = self.palette();
        let view = self.organise.clone().unwrap_or_default();
        theme::card(ui, &palette, Some(palette.accent), &self.decor(), |ui| {
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
                ui.label(egui::RichText::new(&view.message).color(palette.accent));
            }
        });
        ui.add_space(8.0);

        if !view.review.is_empty() {
            theme::card(ui, &palette, None, &self.decor(), |ui| {
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
                                .color(palette.accent),
                        );
                        ui.label(rel);
                    });
                }
            });
            ui.add_space(8.0);
        }

        if !view.plans.is_empty() {
            theme::card(ui, &palette, None, &self.decor(), |ui| {
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
            theme::card(ui, &palette, None, &self.decor(), |ui| {
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

    fn settings_page(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, actions: &mut Vec<Act>) {
        let palette = self.palette();
        theme::card(ui, &palette, Some(palette.accent), &self.decor(), |ui| {
            ui.label(egui::RichText::new("Libraries").size(19.0).strong());
            ui.label(
                egui::RichText::new(
                    "Every folder this app indexes. Removing drops the index rows \
                     only; your files are never touched. To point a library at a new \
                     folder, remove it and add the new folder below.",
                )
                .color(theme::dim_text(ui)),
            );
            ui.add_space(4.0);
            match self.catalog.libraries() {
                Err(e) => {
                    ui.label(egui::RichText::new(format!("libraries: {e}")).color(palette.accent));
                }
                Ok(libraries) => {
                    if libraries.is_empty() {
                        ui.label("No libraries yet. Add the first folder below.");
                    }
                    for lib in &libraries {
                        let items = self
                            .stats
                            .as_ref()
                            .and_then(|s| s.per_library.iter().find(|p| p.library_id == lib.id))
                            .map(|p| p.items)
                            .unwrap_or(0);
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&lib.name).strong());
                            ui.label(
                                egui::RichText::new(lib.kind.as_str())
                                    .size(12.0)
                                    .color(theme::dim_text(ui)),
                            );
                            if lib.mature {
                                ui.label(
                                    egui::RichText::new("mature")
                                        .size(12.0)
                                        .color(palette.accent),
                                );
                            }
                            ui.label(
                                egui::RichText::new(format!("{items} items"))
                                    .size(12.0)
                                    .color(theme::dim_text(ui)),
                            );
                        });
                        ui.label(
                            egui::RichText::new(&lib.root)
                                .size(12.0)
                                .color(theme::dim_text(ui)),
                        );
                        ui.horizontal(|ui| {
                            let armed = self.remove_armed.as_deref() == Some(lib.name.as_str());
                            if armed {
                                ui.label("Remove this library from the index?");
                                if ui.small_button("Confirm remove").clicked() {
                                    actions.push(Act::RemoveLibrary(lib.name.clone()));
                                }
                                if ui.small_button("Keep").clicked() {
                                    self.remove_armed = None;
                                }
                            } else {
                                if ui.small_button("Scan").clicked() {
                                    actions.push(Act::ScanLibrary(lib.name.clone()));
                                }
                                if ui.small_button("Remove...").clicked() {
                                    self.remove_armed = Some(lib.name.clone());
                                }
                            }
                        });
                    }
                    ui.add_space(4.0);
                    if ui.button("Add library...").clicked() {
                        actions.push(Act::AddLibrary);
                    }
                }
            }
        });
        ui.add_space(8.0);
        self.propose_card(ui, &palette, actions);
        ui.add_space(8.0);
        theme::card(ui, &palette, Some(palette.accent), &self.decor(), |ui| {
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
            ui.label(egui::RichText::new("Skin").size(16.0).strong());
            ui.label(
                egui::RichText::new("Six curated palettes. Everything repaints at once.")
                    .color(theme::dim_text(ui)),
            );
            ui.add_space(2.0);
            for skin in SkinId::ALL {
                let selected = self.skin == skin;
                ui.horizontal(|ui| {
                    skin_swatch(ui, &skin.palette(), selected);
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(skin.label()).strong());
                        ui.label(
                            egui::RichText::new(skin.blurb())
                                .size(12.0)
                                .color(theme::dim_text(ui)),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if selected {
                            ui.label(
                                egui::RichText::new("active")
                                    .size(12.0)
                                    .color(self.palette().accent),
                            );
                        } else if ui.small_button("use").clicked() {
                            actions.push(Act::Skin(skin));
                        }
                    });
                });
                ui.add_space(2.0);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(format!("Zoom {:.0}%", self.zoom * 100.0));
                if ui.small_button("100%").clicked() {
                    actions.push(Act::ZoomReset);
                }
                ui.label("Ctrl + mouse wheel zooms anywhere");
            });
            let mut zoom_pct = self.zoom * 100.0;
            if ui
                .add(egui::Slider::new(&mut zoom_pct, 75.0..=150.0).text("zoom %"))
                .changed()
            {
                self.set_zoom(ctx, zoom_pct / 100.0);
            }
            let mut border = self.settings.border_width;
            if ui
                .add(egui::Slider::new(&mut border, 1.0..=3.0).text("card border"))
                .changed()
            {
                self.settings.border_width = border;
                self.save_settings();
            }
            let mut backdrop = self.settings.backdrop_amount * 100.0;
            if ui
                .add(egui::Slider::new(&mut backdrop, 0.0..=200.0).text("backdrop pattern %"))
                .changed()
            {
                self.settings.backdrop_amount = backdrop / 100.0;
                self.save_settings();
            }
        });
        ui.add_space(8.0);
        self.llm_card(ui, &palette, actions);
        ui.add_space(8.0);
        self.agentic_card(ui, &palette);
        ui.add_space(8.0);
        theme::card(ui, &palette, None, &self.decor(), |ui| {
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

    /// LLM provider, model picker, keys and test. The file is shared across
    /// craft apps; discovery and the live test run on worker threads.
    fn llm_card(&mut self, ui: &mut egui::Ui, palette: &SkinPalette, actions: &mut Vec<Act>) {
        theme::card(ui, palette, Some(palette.accent), &self.decor(), |ui| {
            ui.label(egui::RichText::new("LLM and chat").size(19.0).strong());
            ui.label(
                egui::RichText::new(
                    "One provider for every craft app: this page reads and writes the shared file, \
                     so the pick here is the pick everywhere. Local models stay on this machine; \
                     a cloud provider sends catalogue excerpts with the chat.",
                )
                .color(theme::dim_text(ui)),
            );
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Provider").strong());
            ui.horizontal_wrapped(|ui| {
                for provider in LlmProvider::ALL {
                    let selected = self.llm.provider_enum() == provider;
                    if ui
                        .selectable_label(selected, provider.label())
                        .on_hover_text(provider.id())
                        .clicked()
                    {
                        actions.push(Act::LlmProvider(provider));
                    }
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Model");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.llm.model)
                        .desired_width(240.0)
                        .hint_text("pick below, or type a name"),
                );
                if response.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && !self.llm.model.trim().is_empty()
                {
                    actions.push(Act::LlmUseModel(self.llm.model.trim().to_string()));
                }
                if ui
                    .small_button(if self.llm_scanning {
                        "Scanning..."
                    } else {
                        "Discover"
                    })
                    .clicked()
                {
                    actions.push(Act::LlmDiscover);
                }
            });
            if !self.llm_models.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    for found in self.llm_models.clone() {
                        let mut label = found.name.clone();
                        if let Some(gb) = found.size_gb {
                            label.push_str(&format!(" ({gb:.1} GB)"));
                        }
                        if found.resident {
                            label.push_str(" [VRAM]");
                        }
                        if ui.small_button(label).clicked() {
                            actions.push(Act::LlmUseModel(found.name.clone()));
                        }
                    }
                });
            } else if let Some(error) = self.llm_scan_error.clone() {
                ui.label(egui::RichText::new(format!("discovery: {error}")).color(palette.accent));
            }
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Classification").strong());
            ui.label(
                egui::RichText::new(
                    "Which pair the drive scan proposes with. Empty follows the chat pair above.",
                )
                .color(theme::dim_text(ui)),
            );
            {
                let (eff_provider, eff_model) = self.llm.classifier();
                let eff_text = if eff_model.trim().is_empty() {
                    format!("{} | no model", eff_provider.id())
                } else {
                    format!("{} | {}", eff_provider.id(), eff_model.trim())
                };
                ui.horizontal(|ui| {
                    ui.label("Effective:");
                    ui.label(egui::RichText::new(eff_text).strong());
                    let overriding = !self.llm.classify_provider.trim().is_empty()
                        || !self.llm.classify_model.trim().is_empty();
                    if overriding && ui.small_button("Follow chat").clicked() {
                        actions.push(Act::LlmClassifierClear);
                    }
                });
            }
            ui.horizontal_wrapped(|ui| {
                for provider in LlmProvider::ALL {
                    let selected =
                        LlmProvider::parse(&self.llm.classify_provider) == Some(provider);
                    if ui
                        .selectable_label(selected, provider.label())
                        .on_hover_text(provider.id())
                        .clicked()
                    {
                        actions.push(Act::LlmClassifierProvider(provider));
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Model");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.llm.classify_model)
                        .desired_width(240.0)
                        .hint_text("empty = same as chat"),
                );
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.llm.classify_model = self.llm.classify_model.trim().to_string();
                    actions.push(Act::LlmSave);
                }
            });
            ui.add_space(4.0);
            let provider = self.llm.provider_enum();
            if provider.local() {
                ui.label(
                    egui::RichText::new("Local provider: no key needed.")
                        .size(12.0)
                        .color(theme::dim_text(ui)),
                );
                if provider == LlmProvider::Ollama {
                    ui.horizontal(|ui| {
                        ui.label("Endpoint");
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut self.llm.ollama_endpoint)
                                    .desired_width(260.0),
                            )
                            .lost_focus()
                        {
                            actions.push(Act::LlmSave);
                        }
                    });
                }
                if provider == LlmProvider::LmStudio {
                    ui.horizontal(|ui| {
                        ui.label("Endpoint");
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut self.llm.lmstudio_endpoint)
                                    .desired_width(260.0),
                            )
                            .lost_focus()
                        {
                            actions.push(Act::LlmSave);
                        }
                    });
                }
            } else {
                ui.label(egui::RichText::new("API key (BYOK)").strong());
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.llm_key_input)
                            .password(true)
                            .desired_width(260.0)
                            .hint_text("empty every launch; empty keeps the stored one"),
                    );
                    if ui.small_button("Save key").clicked() {
                        actions.push(Act::LlmSaveKey);
                    }
                    if ui.small_button("Forget").clicked() {
                        actions.push(Act::LlmForgetKey);
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    for (id, present) in self.llm.keys_present() {
                        let text = if present {
                            format!("{id}: SET")
                        } else {
                            format!("{id}: -")
                        };
                        ui.label(egui::RichText::new(text).size(12.0).color(if present {
                            palette.accent
                        } else {
                            theme::dim_text(ui)
                        }));
                    }
                });
                if provider == LlmProvider::Custom {
                    ui.horizontal(|ui| {
                        ui.label("Endpoint");
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut self.llm.custom_endpoint)
                                    .desired_width(260.0)
                                    .hint_text("https://host:port"),
                            )
                            .lost_focus()
                        {
                            actions.push(Act::LlmSave);
                        }
                    });
                }
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui
                    .small_button(if self.llm_testing {
                        "Testing..."
                    } else {
                        "Test connection"
                    })
                    .clicked()
                {
                    actions.push(Act::LlmTest);
                }
                if let Some(out) = self.llm_test_out.clone() {
                    ui.label(out);
                }
            });
            ui.add_space(4.0);
            let mut tools = self.llm.tools_enabled;
            if ui
                .checkbox(&mut tools, "Chat may call read-only tools")
                .changed()
            {
                self.llm.tools_enabled = tools;
                actions.push(Act::LlmSave);
            }
            let mut transcripts = self.llm.save_transcripts;
            if ui
                .checkbox(
                    &mut transcripts,
                    "Keep chat transcripts (per-day files, no keys)",
                )
                .changed()
            {
                self.llm.save_transcripts = transcripts;
                actions.push(Act::LlmSave);
            }
            ui.horizontal(|ui| {
                ui.label("Chat instructions");
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut self.llm.user_extra_prompt)
                            .desired_width(320.0)
                            .hint_text("optional extra instructions for the chat"),
                    )
                    .lost_focus()
                {
                    actions.push(Act::LlmSave);
                }
            });
        });
    }

    /// The chat dock: provider pill, context meter, messages, input bar.
    /// Bounded like the fleet guardrail: 280 to 480 wide, wrapping text,
    /// Send on the right of the input row.
    fn show_chat(
        &mut self,
        ui: &mut egui::Ui,
        _rect: egui::Rect,
        palette: &SkinPalette,
        actions: &mut Vec<Act>,
    ) {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        egui::Frame::NONE
            .fill(palette.card)
            .stroke(egui::Stroke::new(1.0, palette.border))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let pill = if self.llm.model.trim().is_empty() {
                        "no model".to_string()
                    } else {
                        // Canonical id, not the raw string: another app may have
                        // stored an alias like "local".
                        format!("{} | {}", self.llm.provider_enum().id(), self.llm.model)
                    };
                    if ui.small_button(pill).clicked() {
                        actions.push(Act::GoSettings);
                    }
                    let estimate = self.chat.last_prompt.len() / 4;
                    ui.label(
                        egui::RichText::new(format!("~{estimate}/16384"))
                            .size(12.0)
                            .color(theme::dim_text(ui)),
                    );
                });
                ui.horizontal(|ui| {
                    if ui.small_button("Prompt").clicked() {
                        actions.push(Act::ChatPrompt);
                    }
                    let mut tools = self.llm.tools_enabled;
                    if ui.checkbox(&mut tools, "Tools").changed() {
                        self.llm.tools_enabled = tools;
                        actions.push(Act::LlmSave);
                    }
                    let max_label = if self.chat.maximized { "Dock" } else { "Max" };
                    if ui.small_button(max_label).clicked() {
                        self.chat.maximized = !self.chat.maximized;
                    }
                    if ui.small_button("Clear").clicked() {
                        actions.push(Act::ChatClear);
                    }
                    if ui.small_button("Close").clicked() {
                        actions.push(Act::ToggleChat);
                    }
                });
                if !self.llm.provider_enum().local() {
                    ui.label(
                        egui::RichText::new(format!(
                            "This chat sends catalogue excerpts to {}",
                            self.llm.provider
                        ))
                        .size(12.0)
                        .color(palette.accent),
                    );
                }
                if self.chat.show_prompt {
                    egui::Window::new("Prompt sent to the model")
                        .resizable(true)
                        .show(ui.ctx(), |ui| {
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                ui.label(format!("~{} tokens", self.chat.last_prompt.len() / 4));
                                ui.monospace(&self.chat.last_prompt);
                            });
                        });
                }
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        if self.chat.lines.is_empty() {
                            ui.label(
                                egui::RichText::new(
                                    "Ask about your collection. Grounded in this catalogue, \
                                     never the internet. Reads run read-only tools; writes live \
                                     in the app, the CLI and MCP.",
                                )
                                .color(theme::dim_text(ui)),
                            );
                        }
                        for line in self.chat.lines.clone() {
                            match line {
                                ChatLine::User(text) => {
                                    ui.label(egui::RichText::new("You").strong());
                                    ui.label(&text);
                                }
                                ChatLine::Assistant(text) => {
                                    ui.label(egui::RichText::new("Magpie").color(palette.accent));
                                    ui.label(&text);
                                }
                                ChatLine::Tool { name, args, result } => {
                                    egui::CollapsingHeader::new(format!("tool: {name}")).show(
                                        ui,
                                        |ui| {
                                            ui.label(args.to_string());
                                            ui.label(crate::chat::truncate_result(&result));
                                        },
                                    );
                                }
                                ChatLine::Error(text) => {
                                    ui.label(egui::RichText::new(text).color(palette.accent));
                                }
                            }
                            ui.add_space(4.0);
                        }
                        if self.chat.busy {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                if ui.small_button("Cancel").clicked() {
                                    self.chat.cancel();
                                }
                            });
                        }
                    });
                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if ui.button("Send").clicked() && !self.chat.busy {
                        actions.push(Act::ChatSend);
                    }
                    let response = ui.add(
                        egui::TextEdit::multiline(&mut self.chat.input)
                            .desired_rows(2)
                            .hint_text(
                                "Ask about your collection (Enter sends, Shift+Enter newline)",
                            ),
                    );
                    if response.has_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift)
                        && !self.chat.busy
                    {
                        actions.push(Act::ChatSend);
                    }
                });
            });
    }

    /// How agents use this app without any LLM key: MCP over stdio.
    fn agentic_card(&self, ui: &mut egui::Ui, palette: &SkinPalette) {
        theme::card(ui, palette, None, &self.decor(), |ui| {
            ui.label(egui::RichText::new("Agentic DE coop (MCP)").strong());
            ui.label(
                egui::RichText::new(
                    "Agents in Antigravity, Claude or Cursor drive this app over MCP stdio: \
                     tools, skills and the catalogue, no LLM key needed.",
                )
                .color(theme::dim_text(ui)),
            );
            ui.label("magpiecraft-mcp mcp");
            ui.label(".\\start.ps1 -Mcp");
        });
    }

    /// Media drives and the proposal scan. Roots are plain drive letters (one
    /// per line, for example E:). Level-2 folders of "Multimedia Files" become
    /// library proposals with a pattern kind guess; unknown rows stay for the AI
    /// pass or manual correction. Confirming adds a real library.
    fn propose_card(&mut self, ui: &mut egui::Ui, palette: &SkinPalette, actions: &mut Vec<Act>) {
        theme::card(ui, palette, None, &self.decor(), |ui| {
            ui.label(egui::RichText::new("Media drives").size(19.0).strong());
            ui.label(egui::RichText::new("One drive letter per line. The scan reads the level-1 media folder on each drive and proposes one library per level-2 folder. Confirming adds the library; nothing is created by the scan itself.").color(theme::dim_text(ui)));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Roots");
                ui.add(
                    egui::TextEdit::multiline(&mut self.drive_roots_text)
                        .desired_rows(2)
                        .desired_width(120.0)
                        .hint_text("E:\nF:"),
                );
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label("Folder");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.settings.media_folder)
                                .desired_width(180.0),
                        );
                    });
                    ui.horizontal(|ui| {
                        if ui.small_button("Save drives").clicked() {
                            self.settings.drive_roots = self
                                .drive_roots_text
                                .lines()
                                .map(|l| l.trim().to_string())
                                .filter(|l| !l.is_empty())
                                .collect();
                            self.save_settings();
                            self.drive_roots_text = self.settings.drive_roots.join("\n");
                        }
                        if ui
                            .small_button(if self.drive_propose_busy {
                                "Scanning..."
                            } else {
                                "Scan drives"
                            })
                            .clicked()
                        {
                            actions.push(Act::ScanDrives);
                        }
                    });
                });
            });
            if let Some(report) = self.drive_proposals.clone() {
                if report.get("ok").and_then(Value::as_bool) != Some(true) {
                    ui.label(
                        egui::RichText::new(format!(
                            "scan failed: {}",
                            report
                                .get("error")
                                .and_then(Value::as_str)
                                .unwrap_or("unknown")
                        ))
                        .color(palette.accent),
                    );
                } else {
                    let proposals = report
                        .get("proposals")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    let known = proposals
                        .iter()
                        .filter(|p| p.get("kind").and_then(Value::as_str).is_some())
                        .count();
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "{} proposals, {} with a kind guess",
                            proposals.len(),
                            known
                        ));
                        if !proposals.is_empty() && ui.small_button("Add all known").clicked() {
                            actions.push(Act::ProposeAddAll);
                        }
                    });
                    ui.horizontal(|ui| {
                        let unknown = proposals
                            .iter()
                            .filter(|p| {
                                let overridden = p
                                    .get("path")
                                    .and_then(Value::as_str)
                                    .is_some_and(|path| self.propose_kinds.contains_key(path));
                                !overridden
                                    && p.get("kind")
                                        .and_then(Value::as_str)
                                        .is_none_or(|k| k == "unknown" || k.trim().is_empty())
                            })
                            .count();
                        if self.classify_busy {
                            ui.label("Classifying with the AI pair...");
                            if ui.small_button("Cancel").clicked() {
                                actions.push(Act::CancelClassify);
                            }
                        } else if unknown > 0
                            && ui
                                .small_button(format!("AI-classify {unknown} unknown"))
                                .clicked()
                        {
                            actions.push(Act::ClassifyUnknowns);
                        }
                    });
                    for p in &proposals {
                        let Some(root) = p.get("path").and_then(Value::as_str) else {
                            continue;
                        };
                        let name = p.get("name").and_then(Value::as_str).unwrap_or("?");
                        let guessed = p.get("kind").and_then(Value::as_str).map(str::to_string);
                        let chosen = self
                            .propose_kinds
                            .get(root)
                            .cloned()
                            .or_else(|| guessed.clone())
                            .unwrap_or_else(|| "unknown".to_string());
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(name).strong());
                            ui.label(
                                egui::RichText::new(root)
                                    .size(12.0)
                                    .color(theme::dim_text(ui)),
                            );
                        });
                        ui.horizontal_wrapped(|ui| {
                            for kind in [
                                "comics",
                                "manga",
                                "magazines",
                                "video",
                                "audio",
                                "ebook",
                                "photo",
                                "other",
                            ] {
                                if ui.selectable_label(chosen == kind, kind).clicked() {
                                    self.propose_kinds
                                        .insert(root.to_string(), kind.to_string());
                                }
                            }
                            let reason = p.get("reason").and_then(Value::as_str).unwrap_or("");
                            let conf = p.get("confidence").and_then(Value::as_f64).unwrap_or(0.0);
                            ui.label(
                                egui::RichText::new(format!("{reason} ({:.0}%)", conf * 100.0))
                                    .size(12.0)
                                    .color(theme::dim_text(ui)),
                            );
                        });
                        if let Some(samples) = p.get("samples").and_then(Value::as_array) {
                            let shown: Vec<&str> =
                                samples.iter().filter_map(Value::as_str).take(3).collect();
                            if !shown.is_empty() {
                                ui.label(
                                    egui::RichText::new(shown.join(", "))
                                        .size(12.0)
                                        .color(theme::dim_text(ui)),
                                );
                            }
                        }
                        ui.horizontal(|ui| {
                            let ready = chosen != "unknown";
                            if ui
                                .add_enabled(
                                    ready,
                                    egui::Button::new(if ready {
                                        "Add library"
                                    } else {
                                        "Pick a kind first"
                                    }),
                                )
                                .clicked()
                            {
                                actions.push(Act::ProposeAdd {
                                    name: name.to_string(),
                                    root: root.to_string(),
                                    kind: chosen.clone(),
                                });
                            }
                        });
                    }
                    if let Some(warnings) = report.get("warnings").and_then(Value::as_array) {
                        for w in warnings.iter().filter_map(Value::as_str) {
                            ui.label(egui::RichText::new(format!("! {w}")).color(palette.accent));
                        }
                    }
                }
            } else if self.drive_propose_busy {
                ui.label("Scanning drives...");
            }
        });
    }

    fn stats_page(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Act>) {
        let palette = self.palette();
        let stats = self.stats.clone().unwrap_or_default();
        theme::card(ui, &palette, Some(palette.accent), &self.decor(), |ui| {
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
                    .color(palette.accent),
                );
            }
            if let Some(ms) = stats.last_scan_ms {
                ui.label(format!("last scan: {ms} ms"));
            }
        });
        ui.add_space(8.0);
        for lib in &stats.per_library {
            theme::card(ui, &palette, None, &self.decor(), |ui| {
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
                    ui.label(egui::RichText::new("mature").color(palette.accent));
                }
            });
            ui.add_space(4.0);
        }
        for drive in &stats.drives {
            theme::card(ui, &palette, None, &self.decor(), |ui| {
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
        theme::card(ui, &palette, None, &self.decor(), |ui| {
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
        ui.add_space(8.0);
        self.drives_card(ui, &palette, actions);
    }

    /// SMART and space report for the disks behind the libraries. The probe
    /// runs on a worker thread; this card shows the last report.
    fn drives_card(&mut self, ui: &mut egui::Ui, palette: &SkinPalette, actions: &mut Vec<Act>) {
        theme::card(ui, palette, None, &self.decor(), |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Drive health").strong());
                if ui
                    .small_button(if self.drive_busy {
                        "Checking..."
                    } else {
                        "Check drives now"
                    })
                    .clicked()
                {
                    actions.push(Act::CheckDrives);
                }
            });
            ui.label(
                egui::RichText::new(
                    "Windows Storage API; USB enclosures report no counters, shown as unknown. \
                     smartctl is used when present.",
                )
                .size(12.0)
                .color(theme::dim_text(ui)),
            );
            let Some(report) = self.drive_report.clone() else {
                if !self.drive_busy {
                    ui.label("No check yet. The probe takes a few seconds.");
                }
                return;
            };
            if report.get("ok").and_then(Value::as_bool) == Some(false) {
                ui.label(
                    egui::RichText::new(format!(
                        "drive check failed: {}",
                        report
                            .get("error")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown")
                    ))
                    .color(palette.accent),
                );
                return;
            }
            if let Some(drives) = report.get("drives").and_then(Value::as_array) {
                for d in drives {
                    let name = d.get("name").and_then(Value::as_str).unwrap_or("?");
                    let health = d.get("health").and_then(Value::as_str).unwrap_or("?");
                    let detail = if d.get("counters_known").and_then(Value::as_bool) == Some(true) {
                        format!(
                            " wear {}% temp {}C hours {}",
                            d.get("wear_percent")
                                .and_then(Value::as_u64)
                                .map(|w| w.to_string())
                                .unwrap_or("?".to_string()),
                            d.get("temp_c")
                                .and_then(Value::as_i64)
                                .map(|t| t.to_string())
                                .unwrap_or("?".to_string()),
                            d.get("power_on_hours")
                                .and_then(Value::as_u64)
                                .map(|h| h.to_string())
                                .unwrap_or("?".to_string())
                        )
                    } else {
                        " counters unknown".to_string()
                    };
                    let line = format!("{name}: {health}{detail}");
                    if health.eq_ignore_ascii_case("healthy") {
                        ui.label(&line);
                    } else {
                        ui.label(egui::RichText::new(&line).color(palette.accent));
                    }
                }
            }
            if let Some(warnings) = report.get("warnings").and_then(Value::as_array) {
                if warnings.is_empty() {
                    ui.label("No warnings.");
                }
                for w in warnings.iter().filter_map(Value::as_str) {
                    ui.label(egui::RichText::new(format!("! {w}")).color(palette.accent));
                }
            }
            if let Some(libraries) = report.get("libraries").and_then(Value::as_array) {
                for lib in libraries {
                    let name = lib.get("library").and_then(Value::as_str).unwrap_or("?");
                    let drive = lib
                        .get("drive")
                        .and_then(Value::as_str)
                        .unwrap_or("outside known drives");
                    ui.label(
                        egui::RichText::new(format!("{name} -> {drive}"))
                            .size(12.0)
                            .color(theme::dim_text(ui)),
                    );
                }
            }
        });
    }

    fn help_page(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        ui.horizontal_wrapped(|ui| {
            for tab in HelpTab::ALL {
                if ui
                    .selectable_label(self.help_tab == tab, tab.label())
                    .clicked()
                {
                    self.help_tab = tab;
                }
            }
        });
        ui.add_space(6.0);
        match self.help_tab {
            HelpTab::GettingStarted => self.help_started(ui, &palette),
            HelpTab::Functions => self.help_functions(ui, &palette),
            HelpTab::Usage => self.help_usage(ui, &palette),
            HelpTab::Tips => self.help_tips(ui, &palette),
            HelpTab::Pitfalls => self.help_pitfalls(ui, &palette),
            HelpTab::Planned => self.help_planned(ui, &palette),
            HelpTab::Skills => self.help_skills(ui, &palette),
            HelpTab::Docs => self.help_docs(ui, &palette),
        }
    }

    fn help_card(&self, ui: &mut egui::Ui, palette: &SkinPalette, title: &str, lines: &[&str]) {
        theme::card(ui, palette, None, &self.decor(), |ui| {
            ui.label(egui::RichText::new(title).strong());
            for line in lines {
                ui.label(*line);
            }
        });
        ui.add_space(8.0);
    }

    fn help_started(&self, ui: &mut egui::Ui, palette: &SkinPalette) {
        self.help_card(
            ui,
            palette,
            "Get started in five steps",
            &[
                "1. Open Settings and add a library: a name, the folder on your disk, its kind.",
                "2. Scan it. File menu, Scan all libraries, or the Scan button on the library row.",
                "3. Browse: folders and volumes on the Libraries page, whole-collection search with /.",
                "4. Read: Enter opens a comic, manga or magazine in the reader. Anything else offers Launch, which hands the file to VLC, mpv or your system handler. Nothing plays inside this app.",
                "5. Curate: the Organise page plans renames, verifies checksums and tracks backups. Every plan is a diff first, applied on confirm, recorded in a journal you can undo.",
            ],
        );
        self.help_card(
            ui,
            palette,
            "What this app is",
            &[
                "MagpieCraft inventories and reads media you already own. It has no downloaders, no tracker or indexer integration, no stream rippers and no DRM circumvention.",
                "It never writes into your libraries. Covers, caches, manifests and the catalogue live under its own data home.",
            ],
        );
    }

    fn help_functions(&self, ui: &mut egui::Ui, palette: &SkinPalette) {
        self.help_card(
            ui,
            palette,
            "Functions",
            &[
                "Catalogue: libraries, incremental scan, whole-collection search, series and reading progress.",
                "Reader: CBZ and ZIP pages, CBR through 7-Zip, PDF through PdfCraft, ComicInfo metadata, spreads, half-page and right-to-left.",
                "Playback handoff: films, tracks, books and photos launch in VLC, mpv or the system handler with resume positions. No decoder lives here.",
                "Organise: film and TV naming presets, rename plans with diff, journal and undo, quarantine instead of delete, review queue.",
                "Verify: BLAKE3 manifests, duplicate and near-duplicate grouping, verification schedule, backup status and verified copies.",
                "Drives: physical disks behind the libraries with health warnings (Stats page).",
                "Drive scan: one proposed library per level-2 folder (Settings).",
                "Subtitles: list, shift, convert, extract and locally generate (whisperx/whisper) sidecar files next to your films.",
                "Convert plans: FFmpeg presets turned into reviewable plans, applied through the same journal as renames.",
            ],
        );
    }

    fn help_usage(&self, ui: &mut egui::Ui, palette: &SkinPalette) {
        self.help_card(
            ui,
            palette,
            "In this window",
            &[
                "Libraries page: folders, volumes, Continue reading, search with / or the search row.",
                "Reader: arrows, Space and Shift+Space turn pages. H half page, T two pages, R right to left, Home and End jump.",
                "Organise: Review lists what is uncertain, Plans holds diffs to apply, Journals records what was applied.",
                "Settings: libraries, media drives with AI-assisted proposals, reading preferences, skins, zoom, LLM provider and chat (plus a separate classifier pair that defaults to chat).",
                "Statistics: totals plus per-library items, pages and bytes.",
            ],
        );
        self.help_card(
            ui,
            palette,
            "Command line (same engine, --json for scripts)",
            &[
                "magpiecraft status --json: version, home, counts, external tools.",
                "magpiecraft libraries / scan / browse / search / stats: the catalogue verbs.",
                "magpiecraft review / plan-rename / apply-plan / journals / undo: the organise verbs.",
                "magpiecraft duplicates / manifest-write / manifest-verify / verify-status / verify-run / backup-status: the verify verbs.",
            ],
        );
        self.help_card(
            ui,
            palette,
            "MCP server (agents and other apps)",
            &[
                "magpiecraft mcp speaks MCP over stdio: tools/list, then tools/call.",
                "Portmanteau tools: media_catalog, media_search, media_read, media_transform, media_health, media_curate, plus magpie_skill_read.",
                "library_remove needs confirm=true and drops index rows only. The files are untouched.",
                "Skill texts are MCP resources too: resources/list shows every skill:// URI, resources/read returns the body.",
            ],
        );
    }

    fn help_tips(&self, ui: &mut egui::Ui, palette: &SkinPalette) {
        self.help_card(
            ui,
            palette,
            "Keyboard",
            &[
                "Arrows / Space / Shift+Space: page forward and back",
                "H: half-page mode, top then bottom with 12 percent overlap",
                "T: two-page spread, the cover alone",
                "R: right-to-left reading for this item",
                "Home / End: first and last page",
                "Escape: back to the library",
                "/: jump to the search box",
                "Ctrl+J: chat dock",
                "Ctrl + mouse wheel: zoom the whole window",
            ],
        );
        self.help_card(
            ui,
            palette,
            "Search and scan",
            &[
                "The search row searches the whole collection. The filter above the grid narrows the folder you are in.",
                "Scan is incremental: unchanged files are skipped by path, size and time. Full rescan re-reads everything and takes longer.",
                "Removed or unplugged files are kept as missing, so progress is never lost. Plug the drive back and scan again.",
            ],
        );
    }

    fn help_pitfalls(&self, ui: &mut egui::Ui, palette: &SkinPalette) {
        self.help_card(
            ui,
            palette,
            "Common pitfalls and safety",
            &[
                "Mature-flagged libraries are hidden from the grid, search and exports until Settings shows them. That is a filter, not a lock.",
                "Removing a library drops its index rows. The files stay on disk, but rescan the folder to get them back.",
                "Rename, move and convert plans show a diff first and write a journal. Deletes go to quarantine, and Undo reverses the journal.",
                "CBR needs 7-Zip and PDF needs PdfCraft on PATH; video identity needs ffprobe. The status command reports what was found.",
                "Playback is a handoff: positions resume where you left off, but chapters, subtitle rendering and transcoding belong to the player.",
                "A chat answer is grounded in this catalogue, not the internet. With a cloud provider selected, the app says so before anything is sent.",
            ],
        );
    }

    fn help_planned(&self, ui: &mut egui::Ui, palette: &SkinPalette) {
        self.help_card(
            ui,
            palette,
            "Planned extensions",
            &[
                "SMART drive health: read the disks your libraries sit on and warn before they fail.",
                "Local subtitle generation with a Rust engine, no cloud service.",
                "Undecided: an OPDS listener for e-readers. It would be the only network port in the product, so it stays undecided until the rest is solid.",
            ],
        );
    }

    fn help_skills(&self, ui: &mut egui::Ui, palette: &SkinPalette) {
        use magpiecraft_core::skills::SKILLS;
        let mut lines: Vec<String> = Vec::new();
        lines.push(format!(
            "{} skills ship inside the app and double as the chat preprompt: one source for agents, the chat model and you.",
            SKILLS.len()
        ));
        for skill in SKILLS {
            lines.push(format!("{}: {}", skill.name, skill.description()));
        }
        lines.push(
            "Agents read them over MCP (resources/list, resources/read, magpie_skill_read) or from the command line: magpiecraft skills list, magpiecraft skills show <name>, magpiecraft skills export --to <dir>."
                .to_string(),
        );
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        self.help_card(ui, palette, "Bundled skills", &refs);
    }

    fn help_docs(&self, ui: &mut egui::Ui, palette: &SkinPalette) {
        self.help_card(
            ui,
            palette,
            "Project files and docs",
            &[
                "README.md: what this is and what is not built yet.",
                "docs/ARCHITECTURE.md: crates, command bus, schema, scanning contract.",
                "docs/TOOLS.md: CLI and MCP reference.",
                "docs/READER_PARITY.md: reader behaviour and deliberate differences.",
                "docs/CONFIGURATION.md: settings and environment.",
                "HELP.md: this Help Center as a file.",
                "HANDOVER.md: what the next agent must know.",
            ],
        );
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
                        let mut submit = ui.button("Add library").clicked();
                        // Enter submits the form, so a path can be finished from the keyboard.
                        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            submit = true;
                        }
                        if submit {
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
                            ("Skin: Magpie Ink", Act::Skin(SkinId::MagpieInk)),
                            ("Skin: Paper Library", Act::Skin(SkinId::PaperLibrary)),
                            ("Skin: Neon Shelf", Act::Skin(SkinId::NeonShelf)),
                            ("Skin: Forest Stacks", Act::Skin(SkinId::ForestStacks)),
                            ("Skin: Sepia Desk", Act::Skin(SkinId::SepiaDesk)),
                            ("Skin: Midnight Archive", Act::Skin(SkinId::MidnightArchive)),
                            ("Statistics", Act::GoStats),
                            ("Organise", Act::GoOrganise),
                            ("Chat", Act::ToggleChat),
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
        // A text field holds keyboard focus: global shortcuts stay out of the way.
        let typing = ctx.egui_wants_keyboard_input();
        let (slash, escape, ctrl_f, ctrl_j, zoomin, zoomout, zero, enter) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Slash),
                i.key_pressed(egui::Key::Escape),
                i.key_pressed(egui::Key::F) && (i.modifiers.ctrl || i.modifiers.command),
                i.key_pressed(egui::Key::J) && (i.modifiers.ctrl || i.modifiers.command),
                i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals),
                i.key_pressed(egui::Key::Minus),
                i.key_pressed(egui::Key::Num0),
                i.key_pressed(egui::Key::Enter),
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
        if !typing && ctrl_j {
            self.chat.open = !self.chat.open;
        }
        // Enter opens the current selection on the Browse page. Text fields
        // keep Enter for themselves, guarded by `typing` above.
        if !typing && enter && self.dialog == Dialog::None && matches!(self.page, Page::Browse) {
            self.open_selected();
        }
        // Escape closes a dialog, but never while typing in one of its fields:
        // abandoning the add-library form mid-path would silently lose input.
        if escape && !typing && self.dialog != Dialog::None {
            self.dialog = Dialog::None;
        }
    }
}

impl eframe::App for MagpieApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // The menu bar, search row and status bar are painted straight onto the
        // window; eframe's default clear colour is near-black, which would frame
        // a light skin in black. The skin owns the background instead.
        let c = self.palette().canvas;
        egui::Color32::from_rgb(c.r(), c.g(), c.b()).to_normalized_gamma_f32()
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(next) = theme::handle_ctrl_wheel(ctx, self.zoom) {
            self.set_zoom(ctx, next);
        }
        self.poll_scan();
        self.poll_llm(ctx);
        self.poll_drives(ctx);
        self.poll_propose(ctx);
        self.poll_classify(ctx);
        let home = self.catalog.paths().home().to_path_buf();
        for line in self.chat.poll(ctx) {
            if self.llm.save_transcripts {
                crate::chat::append_transcript(&home, std::slice::from_ref(&line));
            }
        }
        self.handle_app_keys(ctx);
        self.covers.poll(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Set every frame, like djcraft: eframe may reset the style (system theme
        // tracking), and a conditional set can silently lose.
        ctx.set_visuals(theme::visuals(&self.palette()));
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
            let palette = self.palette();
            theme::paint_backdrop(ui, &palette, content, &self.decor());
            if self.chat.open && self.chat.maximized {
                self.show_chat(ui, content, &palette, &mut actions);
            } else {
                let page_rect = if self.chat.open && content.width() > 700.0 {
                    let chat_w = 360.0f32.clamp(280.0, 480.0).min(content.width() * 0.45);
                    egui::Rect::from_min_max(
                        content.min,
                        egui::pos2(content.right() - chat_w, content.bottom()),
                    )
                } else {
                    content
                };
                ui.scope_builder(
                    egui::UiBuilder::new().max_rect(page_rect),
                    |ui| match self.page {
                        Page::Browse => {
                            let selected = self.selection.as_ref().map(|s| s.key.as_str());
                            let nav = egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    library::show(
                                        ui,
                                        &mut self.browse,
                                        &mut self.covers,
                                        &mut self.catalog,
                                        &palette,
                                        selected,
                                    )
                                })
                                .inner;
                            self.follow_nav(nav);
                        }
                        Page::Reader => {
                            let mut back = false;
                            if let Some(reader) = self.reader.as_mut() {
                                if let Action::Back =
                                    reader.show(ui, &mut self.catalog, content, &palette)
                                {
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
                                .show(ui, |ui| self.stats_page(ui, &mut actions));
                        }
                        Page::Organise => {
                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .show(ui, |ui| self.organise_page(ui, &mut actions));
                        }
                        Page::Settings => {
                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .show(ui, |ui| self.settings_page(&ctx, ui, &mut actions));
                        }
                        Page::Help => {
                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .show(ui, |ui| self.help_page(ui));
                        }
                    },
                );
                if self.chat.open && !self.chat.maximized && content.width() > 700.0 {
                    let chat_w = 360.0f32.clamp(280.0, 480.0).min(content.width() * 0.45);
                    let chat_rect = egui::Rect::from_min_max(
                        egui::pos2(content.right() - chat_w, content.top()),
                        egui::pos2(content.right(), content.bottom()),
                    );
                    ui.scope_builder(egui::UiBuilder::new().max_rect(chat_rect), |ui| {
                        let palette = self.palette();
                        self.show_chat(ui, chat_rect, &palette, &mut actions);
                    });
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

/// A painted preview of a skin: canvas, card, accent and one kind stripe.
fn skin_swatch(ui: &mut egui::Ui, palette: &SkinPalette, selected: bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(64.0, 44.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, palette.canvas);
    let card = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 10.0, rect.top() + 6.0),
        egui::pos2(rect.right() - 10.0, rect.bottom() - 6.0),
    );
    painter.rect_filled(card, 3.0, palette.card);
    painter.rect_filled(
        egui::Rect::from_min_size(card.min, egui::vec2(3.0, card.height())),
        0.0,
        palette.kind_video,
    );
    painter.circle_filled(
        egui::pos2(card.right() - 10.0, card.center().y),
        4.0,
        palette.accent,
    );
    if selected {
        painter.rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(2.0, palette.accent),
            egui::StrokeKind::Inside,
        );
    } else {
        painter.rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(1.0, palette.border),
            egui::StrokeKind::Inside,
        );
    }
}

pub(crate) fn human_bytes(n: i64) -> String {
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
