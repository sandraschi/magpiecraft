//! First-run onboarding: disks, media apps, harvest readiness, libraries.
//!
//! Four stage cards in order. Everything shown comes from read-only probes:
//! fast volume stats, the media-app inventory (Plex, Jellyfin, Calibre,
//! Immich, Komga, Kavita) and the harvest readiness derived from it. An
//! absent app is normal, never an error, and the last stage points at the
//! actions that add the first library.

use crate::app::Act;
use crate::skins::SkinPalette;
use crate::theme::{self, DecorOpts};
use egui::Ui;
use magpiecraft_core::{Catalog, Paths};
use serde_json::Value;
use std::sync::mpsc::{channel, Receiver};

pub struct OnboardState {
    report: Option<Value>,
    busy: bool,
    rx: Option<Receiver<Value>>,
}

impl OnboardState {
    pub fn new() -> Self {
        OnboardState {
            report: None,
            busy: false,
            rx: None,
        }
    }

    /// Run both probes on a worker thread. The Plex count query and the curl
    /// identity checks take seconds; the UI never waits.
    pub fn start(&mut self, paths: Paths) {
        if self.busy {
            return;
        }
        let (tx, rx) = channel::<Value>();
        self.rx = Some(rx);
        self.busy = true;
        std::thread::Builder::new()
            .name("magpie-onboard".to_string())
            .spawn(move || {
                let report = match Catalog::open(paths) {
                    Ok(catalog) => {
                        let apps = catalog.apps_report();
                        let disks = catalog.quick_disks();
                        serde_json::json!({
                            "ok": true,
                            "apps": apps["apps"],
                            "ready": apps["ready"],
                            "disks": disks["volumes"],
                        })
                    }
                    Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }),
                };
                let _ = tx.send(report);
            })
            .ok();
    }

    /// Poll the worker. Call every frame; repaints while busy.
    pub fn poll(&mut self, ctx: &egui::Context) -> bool {
        if let Some(rx) = self.rx.as_ref() {
            match rx.try_recv() {
                Ok(report) => {
                    self.report = Some(report);
                    self.rx = None;
                    self.busy = false;
                    return true;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.report = Some(serde_json::json!({
                        "ok": false, "error": "onboarding probe stopped unexpectedly",
                    }));
                    self.rx = None;
                    self.busy = false;
                    return true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.busy {
            ctx.request_repaint();
        }
        false
    }

    fn apps(&self) -> Vec<Value> {
        self.report
            .as_ref()
            .and_then(|r| r.get("apps"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }

    fn disks(&self) -> Vec<Value> {
        self.report
            .as_ref()
            .and_then(|r| r.get("disks"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }
}

impl Default for OnboardState {
    fn default() -> Self {
        Self::new()
    }
}

/// Harvest readiness derived from the detected state. Only sources verified
/// on this machine claim readiness; everything else names its blocker.
fn harvest_line(app: &Value) -> (String, bool) {
    let id = app.get("id").and_then(Value::as_str).unwrap_or("");
    let state = app.get("state").and_then(Value::as_str).unwrap_or("");
    if state != "ready" {
        let blocker = app
            .get("needs")
            .and_then(Value::as_str)
            .unwrap_or("not available");
        return (blocker.to_string(), false);
    }
    let line = match id {
        "plex" => {
            "Posters plus episode thumbnails: ready to harvest (exact file match, verified here)"
        }
        "jellyfin" => "Video posters plus titles: ready to harvest (server API verified)",
        "calibre" => "Ebook covers plus authors and series: ready to harvest (library found)",
        "immich" => "Photo thumbnails plus albums: ready to harvest (server API verified)",
        _ => "ready",
    };
    (line.to_string(), true)
}

fn state_color(palette: &SkinPalette, state: &str) -> egui::Color32 {
    match state {
        "ready" => palette.text,
        "partial" => palette.accent,
        _ => palette.text_dim,
    }
}

/// The four onboarding stages. `libraries` and `items` come from the stats
/// the app already holds; every probe result comes from the worker report.
pub fn show(
    ui: &mut Ui,
    state: &mut OnboardState,
    palette: &SkinPalette,
    decor: &DecorOpts,
    actions: &mut Vec<Act>,
    libraries: usize,
    items: usize,
) {
    theme::card(ui, palette, Some(palette.accent), decor, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Welcome to MagpieCraft")
                    .size(19.0)
                    .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !state.busy,
                        egui::Button::new(if state.busy {
                            "Checking..."
                        } else {
                            "Check again"
                        }),
                    )
                    .clicked()
                {
                    actions.push(Act::OnboardRefresh);
                }
            });
        });
        ui.label(
            egui::RichText::new(
                "First run in four steps: see the disks, see which media apps are installed, \
                 see what their metadata stashes offer, then add the first library. \
                 Everything here only reads.",
            )
            .color(theme::dim_text(ui)),
        );
    });
    ui.add_space(8.0);

    // Stage 1: disks.
    theme::card(ui, palette, None, decor, |ui| {
        ui.label(egui::RichText::new("1. Disks").size(16.0).strong());
        let disks = state.disks();
        if disks.is_empty() {
            ui.label(
                egui::RichText::new(if state.busy {
                    "Probing volumes..."
                } else {
                    "No volumes reported yet. Press Check again."
                })
                .color(theme::dim_text(ui)),
            );
        }
        for v in &disks {
            let letter = v.get("letter").and_then(Value::as_str).unwrap_or("?");
            let label = v.get("label").and_then(Value::as_str).unwrap_or("");
            let size = v.get("size").and_then(Value::as_u64).unwrap_or(0);
            let free = v.get("free").and_then(Value::as_u64).unwrap_or(0);
            let used = size.saturating_sub(free);
            let frac = if size > 0 {
                (used as f32 / size as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("{letter}:  {label}")).strong());
                ui.add(egui::ProgressBar::new(frac).desired_width(140.0));
                ui.label(
                    egui::RichText::new(format!("{} free of {}", bytes(free), bytes(size)))
                        .size(12.0)
                        .color(theme::dim_text(ui)),
                );
                if frac >= 0.95 {
                    ui.label(egui::RichText::new("full").color(palette.accent));
                }
            });
        }
    });
    ui.add_space(8.0);

    // Stage 2: media apps.
    theme::card(ui, palette, None, decor, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("2. Media apps").size(16.0).strong());
            ui.label(
                egui::RichText::new("what is there, what MagpieCraft can use, and how")
                    .color(theme::dim_text(ui)),
            );
        });
        let apps = state.apps();
        if apps.is_empty() {
            ui.label(
                egui::RichText::new(if state.busy {
                    "Probing Plex, Jellyfin, Calibre, Immich, Komga and Kavita..."
                } else {
                    "Nothing reported yet. Press Check again."
                })
                .color(theme::dim_text(ui)),
            );
        }
        for app in &apps {
            let label = app.get("label").and_then(Value::as_str).unwrap_or("?");
            let app_state = app.get("state").and_then(Value::as_str).unwrap_or("absent");
            let version = app.get("version").and_then(Value::as_str).unwrap_or("");
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(label).strong());
                ui.label(
                    egui::RichText::new(format!("{app_state} {version}"))
                        .color(state_color(palette, app_state)),
                );
            });
            if let Some(uses) = app.get("usable_for").and_then(Value::as_array) {
                for u in uses.iter().filter_map(Value::as_str) {
                    ui.label(
                        egui::RichText::new(format!("can use: {u}"))
                            .size(12.0)
                            .color(theme::dim_text(ui)),
                    );
                }
            }
            let needs = app.get("needs").and_then(Value::as_str).unwrap_or("");
            if !needs.is_empty() {
                ui.label(
                    egui::RichText::new(format!("how: {needs}"))
                        .size(12.0)
                        .color(theme::dim_text(ui)),
                );
            }
        }
    });
    ui.add_space(8.0);

    // Stage 3: harvest readiness, derived from stage 2.
    theme::card(ui, palette, None, decor, |ui| {
        ui.label(
            egui::RichText::new("3. Metadata harvest")
                .size(16.0)
                .strong(),
        );
        ui.label(
            egui::RichText::new(
                "Harvest copies posters and facts into MagpieCraft's own cover cache. \
                 Your libraries and the app databases are never written to.",
            )
            .color(theme::dim_text(ui)),
        );
        let apps = state.apps();
        if apps.is_empty() {
            ui.label(
                egui::RichText::new("Waiting on the media-app probe above.")
                    .color(theme::dim_text(ui)),
            );
        }
        for app in &apps {
            let label = app.get("label").and_then(Value::as_str).unwrap_or("?");
            let (line, ready) = harvest_line(app);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(if ready { "ready" } else { "blocked" }).color(
                        state_color(
                            palette,
                            app.get("state").and_then(Value::as_str).unwrap_or(""),
                        ),
                    ),
                );
                ui.label(egui::RichText::new(format!("{label}: {line}")).size(12.0));
            });
        }
    });
    ui.add_space(8.0);

    // Stage 4: libraries.
    theme::card(ui, palette, None, decor, |ui| {
        ui.label(egui::RichText::new("4. Libraries").size(16.0).strong());
        if libraries == 0 {
            ui.label("No libraries yet. Add the first folder to start indexing.");
        } else {
            ui.label(format!("{libraries} libraries, {items} items indexed."));
        }
        ui.horizontal(|ui| {
            if ui.button("Add library...").clicked() {
                actions.push(Act::AddLibrary);
            }
            if ui.button("Scan drives").clicked() {
                actions.push(Act::ScanDrives);
            }
            if ui.button("Check drives").clicked() {
                actions.push(Act::CheckDrives);
            }
        });
    });
}

fn bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = n as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
