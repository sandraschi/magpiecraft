//! Comics reader view.
//!
//! Behaviour matched against the browser prototype: spreads with the cover alone,
//! half-page mode with a 12 percent overlap, right to left per item or series,
//! printed-page offset, prefetch, and the keyboard the prototype uses (arrows,
//! space, H, T, R, Home, End, Escape).
//!
//! Decoding happens on a worker thread. The UI thread only uploads textures and
//! paints: a 400-page archive costs nothing until a page is on screen.

use crate::skins::SkinPalette;
use magpiecraft_core::images;
use magpiecraft_core::{Catalog, FitMode, PageSet, Reader, ReaderSettings};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{channel, Receiver, Sender};

pub enum Action {
    Back,
    None,
}

struct PageRequest {
    page: usize,
    max_width: u32,
}

struct PageDone {
    page: usize,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

struct PageLoader {
    tx: Option<Sender<PageRequest>>,
    rx: Receiver<PageDone>,
    textures: HashMap<usize, egui::TextureHandle>,
    pending: HashSet<usize>,
    failed: HashSet<usize>,
}

impl PageLoader {
    fn open(set: PageSet) -> Self {
        let (req_tx, req_rx) = channel::<PageRequest>();
        let (done_tx, done_rx) = channel::<PageDone>();
        std::thread::Builder::new()
            .name("magpie-pages".to_string())
            .spawn(move || {
                while let Ok(req) = req_rx.recv() {
                    let done = match set.read(req.page) {
                        Ok(bytes) => match images::decode_fitted(&bytes, req.max_width, 5000) {
                            Ok(img) => {
                                let (w, h) = img.dimensions();
                                PageDone {
                                    page: req.page,
                                    width: w,
                                    height: h,
                                    rgba: img.into_raw(),
                                }
                            }
                            Err(_) => PageDone {
                                page: req.page,
                                width: 0,
                                height: 0,
                                rgba: Vec::new(),
                            },
                        },
                        Err(_) => PageDone {
                            page: req.page,
                            width: 0,
                            height: 0,
                            rgba: Vec::new(),
                        },
                    };
                    if done_tx.send(done).is_err() {
                        break;
                    }
                }
            })
            .ok();
        PageLoader {
            tx: Some(req_tx),
            rx: done_rx,
            textures: HashMap::new(),
            pending: HashSet::new(),
            failed: HashSet::new(),
        }
    }

    fn request(&mut self, page: usize, max_width: u32) {
        if self.textures.contains_key(&page)
            || self.pending.contains(&page)
            || self.failed.contains(&page)
        {
            return;
        }
        if let Some(tx) = &self.tx {
            if tx.send(PageRequest { page, max_width }).is_ok() {
                self.pending.insert(page);
            }
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(done) = self.rx.try_recv() {
            self.pending.remove(&done.page);
            if done.width == 0 || done.height == 0 {
                self.failed.insert(done.page);
                continue;
            }
            let colour = egui::ColorImage::from_rgba_unmultiplied(
                [done.width as usize, done.height as usize],
                &done.rgba,
            );
            let handle = ctx.load_texture(
                format!("page-{}", done.page),
                colour,
                egui::TextureOptions::LINEAR,
            );
            self.textures.insert(done.page, handle);
        }
        // Keep the texture cache small: the current spread plus a few prefetched.
        if self.textures.len() > 24 {
            self.textures.clear();
            self.pending.clear();
        }
    }

    fn texture(&self, page: usize) -> Option<&egui::TextureHandle> {
        self.textures.get(&page)
    }

    fn failed(&self, page: usize) -> bool {
        self.failed.contains(&page)
    }
}

pub struct ReaderView {
    key: String,
    title: String,
    count: usize,
    error: Option<String>,
    reader: Reader,
    loader: Option<PageLoader>,
    jump: String,
    last_saved: usize,
    prefetch: usize,
    status: String,
    escape_requested: bool,
}

impl ReaderView {
    /// Open an item for reading. Unreadable containers keep their explanation:
    /// the view never shows an empty page without saying why.
    pub fn open(catalog: &mut Catalog, key: &str) -> Self {
        let info = match catalog.pages(key) {
            Ok(info) => info,
            Err(e) => {
                return ReaderView {
                    key: key.to_string(),
                    title: String::new(),
                    count: 0,
                    error: Some(format!("cannot open {key}: {e}")),
                    reader: Reader::new(0, ReaderSettings::default()),
                    loader: None,
                    jump: String::new(),
                    last_saved: 0,
                    prefetch: catalog.settings().prefetch_pages,
                    status: String::new(),
                    escape_requested: false,
                };
            }
        };
        let set = if info.readable {
            catalog.page_set(key).ok()
        } else {
            None
        };
        let loader = set.map(PageLoader::open);
        let saved_page = info
            .progress
            .as_ref()
            .map(|p| usize::try_from(p.page).unwrap_or(1))
            .unwrap_or(1);
        let settings = ReaderSettings {
            two_page: false,
            half_mode: false,
            rtl: info.rtl,
            fit: FitMode::Height,
            page_offset: info.page_offset,
        };
        let reader = Reader::with_page(info.count, settings, saved_page);
        ReaderView {
            key: key.to_string(),
            title: info.title,
            count: info.count,
            error: info.reason,
            reader,
            loader,
            jump: String::new(),
            last_saved: 0,
            prefetch: catalog.settings().prefetch_pages,
            status: String::new(),
            escape_requested: false,
        }
    }

    fn settings_snapshot(&self) -> ReaderSettings {
        self.reader.settings().clone()
    }

    fn apply(&mut self, settings: ReaderSettings) {
        self.reader.set_settings(settings);
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        catalog: &mut Catalog,
        viewport: egui::Rect,
        palette: &SkinPalette,
    ) -> Action {
        let mut action = Action::None;
        if let Some(loader) = self.loader.as_mut() {
            loader.poll(ui.ctx());
        }
        self.toolbar(ui, catalog, palette, &mut action);
        self.progress_bar(ui, palette);

        if let Some(error) = self.error.clone() {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(egui::RichText::new(&error).size(16.0));
                if self.count > 0 {
                    ui.label(
                        egui::RichText::new(format!("{} pages are indexed", self.count))
                            .color(palette.text_dim),
                    );
                }
            });
            return action;
        }
        if self.count == 0 {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label("No pages found in this item.");
            });
            return action;
        }
        self.keyboard_hook(ui, catalog);

        let max_width = if self.reader.settings().half_mode {
            2200
        } else {
            1400
        };
        let slots = self.reader.slots();
        if let Some(loader) = self.loader.as_mut() {
            for slot in &slots {
                loader.request(slot.page, max_width);
            }
            let ahead = self.prefetch;
            for page in self.reader.prefetch_pages(ahead) {
                loader.request(page, max_width);
            }
        }

        // Every rect below is absolute, derived from the real window rect handed in
        // by the shell. The reader does not use the layout cursor for the page area:
        // a reader paints the page, it does not flow widgets.
        let top = ui.cursor().top();
        let canvas = egui::Rect::from_min_max(
            egui::pos2(viewport.left() + 4.0, top),
            egui::pos2(viewport.right() - 4.0, viewport.bottom() - 4.0),
        );
        let avail = canvas.size();
        let gap = 6.0;
        let n = slots.len().max(1) as f32;
        let column = ((avail.x - gap * (n - 1.0)) / n).max(40.0);
        let mut x = canvas.left();
        for slot in &slots {
            let slot_rect = egui::Rect::from_min_size(
                egui::pos2(x, canvas.top()),
                egui::vec2(column, canvas.height()),
            );
            x += column + gap;
            self.draw_slot(ui, slot, slot_rect, palette);
        }

        // Save progress when the position actually changed.
        let page = self.reader.page();
        if page != self.last_saved {
            self.last_saved = page;
            let total = Some(self.count);
            if let Err(e) = catalog.progress_set(&self.key, page, total, false) {
                self.status = format!("progress not saved: {e}");
            }
        }
        if self.escape_requested {
            self.escape_requested = false;
            return Action::Back;
        }
        action
    }

    /// Thin progress bar under the toolbar: page fraction in the accent colour.
    fn progress_bar(&self, ui: &mut egui::Ui, palette: &SkinPalette) {
        if self.count == 0 {
            return;
        }
        let width = ui.available_width().max(40.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 3.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 0.0, palette.border);
        let fraction = (self.reader.page() as f32 / self.count as f32).clamp(0.0, 1.0);
        if fraction > 0.0 {
            ui.painter().rect_filled(
                egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * fraction, 3.0)),
                0.0,
                palette.accent,
            );
        }
    }

    fn keyboard_hook(&mut self, ui: &egui::Ui, catalog: &mut Catalog) {
        if self.keyboard(ui, catalog) {
            self.escape_requested = true;
        }
    }

    fn draw_slot(
        &self,
        ui: &egui::Ui,
        slot: &magpiecraft_core::Slot,
        rect: egui::Rect,
        palette: &SkinPalette,
    ) {
        ui.painter().rect_filled(rect, 2.0, palette.canvas);
        let loader = match self.loader.as_ref() {
            Some(l) => l,
            None => return,
        };
        let (top, bottom) = slot.crop.source_range();
        match loader.texture(slot.page) {
            Some(tex) => {
                let size = tex.size_vec2();
                if size.x <= 0.0 || size.y <= 0.0 {
                    return;
                }
                let visible_h = size.y * (bottom - top);
                let fit = self.reader.settings().fit;
                let scale = match fit {
                    FitMode::Height => (rect.width() / size.x).min(rect.height() / visible_h),
                    FitMode::Width => rect.width() / size.x,
                };
                let draw_w = size.x * scale;
                let draw_h = visible_h * scale;
                let centre_x = rect.center().x;
                let top_y = rect.top() + ((rect.height() - draw_h) * 0.5).max(0.0);
                let target = egui::Rect::from_min_size(
                    egui::pos2(centre_x - draw_w * 0.5, top_y),
                    egui::vec2(draw_w, draw_h),
                );
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, top), egui::pos2(1.0, bottom));
                ui.painter_at(rect)
                    .image(tex.id(), target, uv, egui::Color32::WHITE);
            }
            None => {
                let message = if loader.failed(slot.page) {
                    format!("page {} could not be decoded", slot.page)
                } else {
                    format!("loading page {}...", slot.page)
                };
                ui.painter_at(rect).text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    message,
                    egui::FontId::proportional(15.0),
                    palette.text_dim,
                );
            }
        }
    }

    fn toolbar(
        &mut self,
        ui: &mut egui::Ui,
        catalog: &mut Catalog,
        palette: &SkinPalette,
        action: &mut Action,
    ) {
        let mut s = self.settings_snapshot();
        let mut changed = false;
        let mut rtl_changed = false;
        let mut offset_changed = false;
        ui.horizontal_wrapped(|ui| {
            if ui.button("< Library").clicked() {
                *action = Action::Back;
            }
            ui.label(
                egui::RichText::new(if self.title.is_empty() {
                    self.key.clone()
                } else {
                    self.title.clone()
                })
                .strong(),
            );
            ui.separator();
            let rtl = s.rtl;
            if ui.button("Prev").clicked() {
                self.reader.step(if rtl {
                    magpiecraft_core::Step::Forward
                } else {
                    magpiecraft_core::Step::Backward
                });
            }
            ui.label(self.reader.position_label());
            if ui.button("Next").clicked() {
                self.reader.step(if rtl {
                    magpiecraft_core::Step::Backward
                } else {
                    magpiecraft_core::Step::Forward
                });
            }
            ui.separator();
            if ui
                .selectable_label(s.two_page, "Two pages")
                .on_hover_text("Two-page spread, cover alone (T)")
                .clicked()
            {
                s.two_page = !s.two_page;
                if s.two_page {
                    s.half_mode = false;
                }
                changed = true;
            }
            if ui
                .selectable_label(s.half_mode, "Half page")
                .on_hover_text("Top and bottom halves enlarged, 12 percent overlap (H)")
                .clicked()
            {
                s.half_mode = !s.half_mode;
                if s.half_mode {
                    s.two_page = false;
                }
                changed = true;
            }
            if ui
                .selectable_label(s.rtl, "RTL")
                .on_hover_text("Right to left reading (R)")
                .clicked()
            {
                s.rtl = !s.rtl;
                changed = true;
                rtl_changed = true;
            }
            let fit_label = match s.fit {
                FitMode::Height => "Fit: page",
                FitMode::Width => "Fit: width",
            };
            if ui
                .selectable_label(s.fit == FitMode::Width, fit_label)
                .on_hover_text("Fit the whole page, or fill the width and clip the height")
                .clicked()
            {
                s.fit = match s.fit {
                    FitMode::Height => FitMode::Width,
                    FitMode::Width => FitMode::Height,
                };
                changed = true;
            }
            ui.separator();
            ui.label("Offset");
            if ui.small_button("-").clicked() {
                s.page_offset = (s.page_offset - 1).clamp(-500, 500);
                changed = true;
                offset_changed = true;
            }
            ui.label(format!("{}", s.page_offset));
            if ui.small_button("+").clicked() {
                s.page_offset = (s.page_offset + 1).clamp(-500, 500);
                changed = true;
                offset_changed = true;
            }
            ui.separator();
            ui.label("Printed page");
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.jump)
                    .desired_width(52.0)
                    .hint_text("no."),
            );
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                if let Ok(printed) = self.jump.trim().parse::<i32>() {
                    self.reader.goto_printed(printed);
                }
                self.jump.clear();
            }
        });
        if changed {
            self.apply(s.clone());
        }
        if rtl_changed {
            if let Err(e) = catalog.set_item_rtl(&self.key, Some(s.rtl)) {
                self.status = format!("reading direction not saved: {e}");
            }
        }
        if offset_changed {
            if let Err(e) = catalog.set_page_offset(&self.key, s.page_offset) {
                self.status = format!("offset not saved: {e}");
            }
        }
        if !self.status.is_empty() {
            ui.label(egui::RichText::new(&self.status).color(palette.accent));
        }
    }

    /// Returns true when Escape asked to leave the reader.
    fn keyboard(&mut self, ui: &egui::Ui, catalog: &mut Catalog) -> bool {
        let typing = ui.ctx().memory(|m| m.focused().is_some());
        if typing {
            return false;
        }
        let (right, left, space, shift, h, t, r, home, end, escape, up, down) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowRight),
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::Space),
                i.modifiers.shift,
                i.key_pressed(egui::Key::H),
                i.key_pressed(egui::Key::T),
                i.key_pressed(egui::Key::R),
                i.key_pressed(egui::Key::Home),
                i.key_pressed(egui::Key::End),
                i.key_pressed(egui::Key::Escape),
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::ArrowUp),
            )
        });
        let rtl = self.reader.settings().rtl;
        if escape {
            return true;
        }
        if h {
            let mut s = self.settings_snapshot();
            s.half_mode = !s.half_mode;
            if s.half_mode {
                s.two_page = false;
            }
            self.apply(s);
            return false;
        }
        if t {
            let mut s = self.settings_snapshot();
            s.two_page = !s.two_page;
            if s.two_page {
                s.half_mode = false;
            }
            self.apply(s);
            return false;
        }
        if r {
            let mut s = self.settings_snapshot();
            s.rtl = !s.rtl;
            let rtl = s.rtl;
            self.apply(s);
            let _ = catalog.set_item_rtl(&self.key, Some(rtl));
            return false;
        }
        if home {
            self.reader.set_page(1);
            return false;
        }
        if end {
            self.reader.set_page(self.count.max(1));
            return false;
        }
        if down {
            self.reader.step(magpiecraft_core::Step::Forward);
            return false;
        }
        if up {
            self.reader.step(magpiecraft_core::Step::Backward);
            return false;
        }
        if space {
            self.reader.step(if shift {
                magpiecraft_core::Step::Backward
            } else {
                magpiecraft_core::Step::Forward
            });
            return false;
        }
        if right {
            self.reader.step(if rtl {
                magpiecraft_core::Step::Backward
            } else {
                magpiecraft_core::Step::Forward
            });
        } else if left {
            self.reader.step(if rtl {
                magpiecraft_core::Step::Forward
            } else {
                magpiecraft_core::Step::Backward
            });
        }
        false
    }

    /// Page the reader would show, for tests and for the status bar.
    pub fn page(&self) -> usize {
        self.reader.page()
    }

    pub fn count(&self) -> usize {
        self.count
    }

    pub fn set_two_page(&mut self, on: bool) {
        let mut s = self.settings_snapshot();
        s.two_page = on;
        if on {
            s.half_mode = false;
        }
        self.apply(s);
    }

    pub fn two_pages(&self) -> bool {
        self.reader.settings().two_page
    }

    pub fn half(&self) -> bool {
        self.reader.settings().half_mode
    }

    pub fn set_half(&mut self, on: bool) {
        let mut s = self.settings_snapshot();
        s.half_mode = on;
        if on {
            s.two_page = false;
        }
        self.apply(s);
    }

    pub fn toggle_rtl(&mut self) {
        let mut s = self.settings_snapshot();
        s.rtl = !s.rtl;
        self.apply(s);
    }

    pub fn key(&self) -> &str {
        &self.key
    }
}
