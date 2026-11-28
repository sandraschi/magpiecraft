//! Left sidebar: drive and folder hierarchy.
//!
//! Libraries group under their drive letter (parsed from the root path, so
//! `L:\Multimedia Files\Comics` hangs under `L:`). Folders load lazily:
//! expanding a node browses exactly that key and caches the children until
//! the next refresh. Single click navigates and selects; the inspector shows
//! whatever was selected last.

use crate::library::{SelOpen, Selection};
use egui::Ui;
use magpiecraft_core::{BrowseView, Catalog, FolderView, ItemView, Library};
use std::collections::HashMap;

/// At most this many rows render under one tree node per page. An anime
/// folder with 10,000 subfolders pages through 100 at a time instead of
/// freezing the sidebar; the filter narrows it further.
const TREE_PAGE_SIZE: usize = 100;

/// All per-node sidebar state in one place: expansion, cached browse views,
/// errors, page index, filter text and sort direction, keyed by node key.
#[derive(Default)]
pub struct TreeUi {
    pub expanded: HashMap<String, bool>,
    pub kids: HashMap<String, BrowseView>,
    pub errors: HashMap<String, String>,
    pub page: HashMap<String, usize>,
    pub filter: HashMap<String, String>,
    pub sort_desc: HashMap<String, bool>,
}

/// What the tree wants the app to do after a click.
pub enum TreeOut {
    None,
    Open {
        key: Option<String>,
        select: Selection,
    },
    /// Select without navigating: leaf items have no children, so the grid
    /// stays where it is and the inspector shows the item.
    Select(Selection),
}

fn drive_of(root: &str) -> String {
    let mut chars = root.chars();
    match (chars.next(), chars.next()) {
        (Some(letter), Some(':')) if letter.is_ascii_alphabetic() => {
            format!("{}:", letter.to_ascii_uppercase())
        }
        _ => "Other drives".to_string(),
    }
}

/// Draw the tree. Returns navigation/selection for clicks. The `tree`
/// state persists across frames; `kids`/`errors` are cleared on every
/// refresh. Whole browse views are cached so a node shows both subfolders
/// and leaf items (volumes are items, and a comics library is almost all
/// volumes). Long nodes page (100 rows), filter by name and sort A-Z/Z-A.
pub fn show_tree(
    ui: &mut Ui,
    palette: &crate::skins::SkinPalette,
    catalog: &mut Catalog,
    libraries: &[Library],
    tree: &mut TreeUi,
    browse_key: &Option<String>,
    selected_key: Option<&str>,
) -> TreeOut {
    let mut out = TreeOut::None;
    let TreeUi {
        expanded,
        kids,
        errors,
        page,
        filter,
        sort_desc,
    } = &mut *tree;

    // Group libraries under their drive letter first, so the header buttons
    // can expand or collapse every known drive and library in one click.
    let mut groups: HashMap<String, Vec<&Library>> = HashMap::new();
    for lib in libraries {
        groups.entry(drive_of(&lib.root)).or_default().push(lib);
    }
    let mut drives: Vec<String> = groups.keys().cloned().collect();
    drives.sort();
    for drive in &mut drives {
        if let Some(libs) = groups.get_mut(drive) {
            libs.sort_by(|a, b| a.name.cmp(&b.name));
        }
    }

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Library").strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Collapse all").clicked() {
                expanded.clear();
                for drive in &drives {
                    expanded.insert(format!("drive:{drive}"), false);
                }
                kids.clear();
            }
            if ui.small_button("Expand all").clicked() {
                for drive in &drives {
                    expanded.insert(format!("drive:{drive}"), true);
                    if let Some(libs) = groups.get(drive) {
                        for lib in libs {
                            expanded.insert(lib.key.clone(), true);
                        }
                    }
                }
            }
        });
    });
    ui.label(
        egui::RichText::new(format!(
            "{} librar{} on {} drive{}",
            libraries.len(),
            if libraries.len() == 1 { "y" } else { "ies" },
            drives.len(),
            if drives.len() == 1 { "" } else { "s" },
        ))
        .small()
        .color(palette.text_dim),
    );
    ui.add_space(2.0);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for drive in &drives {
                let libs = match groups.get(drive) {
                    Some(l) => l,
                    None => continue,
                };
                let dkey = format!("drive:{drive}");
                let dexpanded = *expanded.get(&dkey).unwrap_or(&true);
                ui.horizontal(|ui| {
                    let glyph = if dexpanded { "v" } else { ">" };
                    if ui.small_button(glyph).clicked() {
                        expanded.insert(dkey.clone(), !dexpanded);
                    }
                    ui.label(
                        egui::RichText::new(format!(
                            "{drive}  ({} librar{})",
                            libs.len(),
                            if libs.len() == 1 { "y" } else { "ies" }
                        ))
                        .color(palette.text_dim),
                    );
                });
                if !dexpanded {
                    continue;
                }
                ui.indent(format!("indent-{drive}"), |ui| {
                    for lib in libs {
                        let lexpanded = *expanded.get(&lib.key).unwrap_or(&false);
                        let is_current = browse_key.as_deref() == Some(lib.key.as_str());
                        let online = std::path::Path::new(&lib.root).is_dir();
                        ui.horizontal(|ui| {
                            let glyph = if lexpanded { "v" } else { ">" };
                            if ui.small_button(glyph).clicked() {
                                let now = !*expanded.get(&lib.key).unwrap_or(&false);
                                expanded.insert(lib.key.clone(), now);
                            }
                            let mut label = egui::RichText::new(&lib.name);
                            if is_current {
                                label = label.strong().color(palette.accent);
                            }
                            if ui
                                .selectable_label(selected_key == Some(lib.key.as_str()), label)
                                .clicked()
                            {
                                out = TreeOut::Open {
                                    key: Some(lib.key.clone()),
                                    select: Selection {
                                        open: SelOpen::Folder,
                                        key: lib.key.clone(),
                                        title: lib.name.clone(),
                                        subtitle: lib.root.clone(),
                                        library: Some(lib.name.clone()),
                                    },
                                };
                            }
                            if !online {
                                ui.label(
                                    egui::RichText::new("offline")
                                        .size(11.0)
                                        .color(palette.accent),
                                );
                            }
                        });
                        if *expanded.get(&lib.key).unwrap_or(&false) {
                            if !kids.contains_key(&lib.key) {
                                load_kids(catalog, &lib.key, kids, errors);
                            }
                            // One indent per node: the error (if any) sits above
                            // the children inside the same block, so egui never
                            // sees two identical indent ids side by side.
                            ui.indent(format!("indent-{}", lib.key), |ui| {
                                if let Some(err) = errors.get(&lib.key).cloned() {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "cannot list {}: {err}",
                                            lib.root
                                        ))
                                        .size(12.0)
                                        .color(palette.accent),
                                    );
                                }
                                match show_kids(
                                    ui,
                                    palette,
                                    catalog,
                                    &lib.key,
                                    &mut *expanded,
                                    &mut *kids,
                                    &mut *errors,
                                    &mut *page,
                                    &mut *filter,
                                    &mut *sort_desc,
                                    browse_key,
                                    selected_key,
                                ) {
                                    TreeOut::None => {}
                                    other => out = other,
                                }
                            });
                        }
                    }
                });
            }
        });
    out
}

fn load_kids(
    catalog: &mut Catalog,
    key: &str,
    kids: &mut HashMap<String, BrowseView>,
    errors: &mut HashMap<String, String>,
) {
    match catalog.browse(Some(key)) {
        Ok(view) => {
            errors.remove(key);
            kids.insert(key.to_string(), view);
        }
        Err(e) => {
            errors.insert(key.to_string(), e.to_string());
            kids.insert(key.to_string(), empty_view());
        }
    }
}

fn empty_view() -> BrowseView {
    BrowseView {
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

#[allow(clippy::too_many_arguments)]
fn show_kids(
    ui: &mut Ui,
    palette: &crate::skins::SkinPalette,
    catalog: &mut Catalog,
    parent_key: &str,
    expanded: &mut HashMap<String, bool>,
    kids: &mut HashMap<String, BrowseView>,
    errors: &mut HashMap<String, String>,
    page: &mut HashMap<String, usize>,
    filter: &mut HashMap<String, String>,
    sort_desc: &mut HashMap<String, bool>,
    browse_key: &Option<String>,
    selected_key: Option<&str>,
) -> TreeOut {
    let mut out = TreeOut::None;
    if !kids.contains_key(parent_key) {
        load_kids(catalog, parent_key, kids, errors);
    }
    if let Some(err) = errors.get(parent_key).cloned() {
        ui.label(
            egui::RichText::new(format!("cannot list: {err}"))
                .size(12.0)
                .color(palette.accent),
        );
    }
    let view = kids.get(parent_key).cloned().unwrap_or_else(empty_view);
    if view.folders.is_empty() && view.items.is_empty() {
        if errors.get(parent_key).is_none() {
            ui.label(egui::RichText::new("empty").small().color(palette.text_dim));
        }
        return out;
    }
    // Filter and sort row. One text field per node, so the widget id
    // carries the node key; changing either resets to the first page.
    let mut reset_page = false;
    ui.horizontal(|ui| {
        let field = filter.entry(parent_key.to_string()).or_default();
        if ui
            .add(
                egui::TextEdit::singleline(field)
                    .desired_width(110.0)
                    .hint_text("filter...")
                    .id_source(format!("tree-filter-{parent_key}")),
            )
            .changed()
        {
            reset_page = true;
        }
        let desc = sort_desc.entry(parent_key.to_string()).or_insert(false);
        if ui.small_button(if *desc { "Z-A" } else { "A-Z" }).clicked() {
            *desc = !*desc;
            reset_page = true;
        }
    });
    if reset_page {
        page.insert(parent_key.to_string(), 0);
    }
    let raw = filter
        .get(parent_key)
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let needle = raw.to_lowercase();
    let desc = *sort_desc.get(parent_key).unwrap_or(&false);
    let mut folders: Vec<&FolderView> = view
        .folders
        .iter()
        .filter(|f| needle.is_empty() || f.name.to_lowercase().contains(&needle))
        .collect();
    folders.sort_by_key(|a| a.name.to_lowercase());
    let mut items: Vec<&ItemView> = view
        .items
        .iter()
        .filter(|i| {
            needle.is_empty()
                || i.title.to_lowercase().contains(&needle)
                || i.name.to_lowercase().contains(&needle)
        })
        .collect();
    items.sort_by_key(|a| a.title.to_lowercase());
    if desc {
        folders.reverse();
        items.reverse();
    }
    let total = folders.len() + items.len();
    if total == 0 {
        ui.label(
            egui::RichText::new(format!("no match for \"{raw}\""))
                .small()
                .color(palette.text_dim),
        );
        return out;
    }
    let pages = total.div_ceil(TREE_PAGE_SIZE);
    let cur = page
        .get(parent_key)
        .copied()
        .unwrap_or(0)
        .min(pages.saturating_sub(1));
    let start = cur * TREE_PAGE_SIZE;
    let end = (start + TREE_PAGE_SIZE).min(total);
    let fcount = folders.len();
    let (f_lo, f_hi) = (start.min(fcount), end.min(fcount));
    let (i_lo, i_hi) = (start.saturating_sub(fcount), end.saturating_sub(fcount));
    for folder in &folders[f_lo..f_hi] {
        let fkey = folder.key.clone();
        let fexpanded = *expanded.get(&fkey).unwrap_or(&false);
        let is_current = browse_key.as_deref() == Some(fkey.as_str());
        ui.horizontal(|ui| {
            let glyph = if fexpanded { "v" } else { ">" };
            if ui.small_button(glyph).clicked() {
                expanded.insert(fkey.clone(), !fexpanded);
            }
            let mut label = egui::RichText::new(&folder.name).small();
            if is_current {
                label = label.strong().color(palette.accent);
            }
            if ui
                .selectable_label(selected_key == Some(fkey.as_str()), label)
                .clicked()
            {
                out = TreeOut::Open {
                    key: Some(fkey.clone()),
                    select: Selection {
                        open: SelOpen::Folder,
                        key: fkey.clone(),
                        title: folder.name.clone(),
                        subtitle: format!("{} items", folder.items),
                        library: None,
                    },
                };
            }
        });
        if *expanded.get(&fkey).unwrap_or(&false) {
            ui.indent(format!("indent-{fkey}"), |ui| {
                match show_kids(
                    ui,
                    palette,
                    catalog,
                    &fkey,
                    &mut *expanded,
                    &mut *kids,
                    &mut *errors,
                    &mut *page,
                    &mut *filter,
                    &mut *sort_desc,
                    browse_key,
                    selected_key,
                ) {
                    TreeOut::None => {}
                    other => out = other,
                }
            });
        }
    }
    for item in &items[i_lo..i_hi] {
        let open = if item.kind.is_comicish() {
            SelOpen::Read
        } else {
            SelOpen::Launch
        };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("-").small().color(palette.text_dim));
            if ui
                .selectable_label(
                    selected_key == Some(item.key.as_str()),
                    egui::RichText::new(&item.title).small(),
                )
                .clicked()
            {
                out = TreeOut::Select(Selection {
                    open,
                    key: item.key.clone(),
                    title: item.title.clone(),
                    subtitle: item.kind.as_str().to_string(),
                    library: None,
                });
            }
        });
    }
    if pages > 1 {
        let mut next = cur;
        ui.horizontal(|ui| {
            if ui.small_button("<").clicked() {
                next = next.saturating_sub(1);
            }
            ui.label(
                egui::RichText::new(format!("{}-{} of {total}", start + 1, end))
                    .small()
                    .color(palette.text_dim),
            );
            if ui.small_button(">").clicked() {
                next = (next + 1).min(pages - 1);
            }
        });
        page.insert(parent_key.to_string(), next);
    } else {
        page.insert(parent_key.to_string(), cur);
    }
    out
}
