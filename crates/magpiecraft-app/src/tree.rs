//! Left sidebar: drive and folder hierarchy.
//!
//! Libraries group under their drive letter (parsed from the root path, so
//! `L:\Multimedia Files\Comics` hangs under `L:`). Folders load lazily:
//! expanding a node browses exactly that key and caches the children until
//! the next refresh. Single click navigates and selects; the inspector shows
//! whatever was selected last.

use crate::library::{SelOpen, Selection};
use egui::Ui;
use magpiecraft_core::{Catalog, FolderView, Library};
use std::collections::HashMap;

/// What the tree wants the app to do after a click.
pub enum TreeOut {
    None,
    Open {
        key: Option<String>,
        select: Selection,
    },
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

/// Draw the tree. Returns navigation/selection for clicks. `expanded` and
/// `kids` persist across frames; `kids` is cleared on every refresh.
#[allow(clippy::too_many_arguments)]
pub fn show_tree(
    ui: &mut Ui,
    palette: &crate::skins::SkinPalette,
    catalog: &mut Catalog,
    libraries: &[Library],
    expanded: &mut HashMap<String, bool>,
    kids: &mut HashMap<String, Vec<FolderView>>,
    browse_key: &Option<String>,
    selected_key: Option<&str>,
) -> TreeOut {
    let mut out = TreeOut::None;

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Library").strong());
    });
    ui.add_space(2.0);

    // Group libraries under their drive letter, drives sorted, libraries
    // sorted by name inside each drive.
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
                        let lkey = format!("lib:{}", lib.id);
                        let lexpanded = *expanded.get(&lkey).unwrap_or(&false);
                        let is_current = browse_key.as_deref() == Some(lib.key.as_str());
                        ui.horizontal(|ui| {
                            let glyph = if lexpanded { "v" } else { ">" };
                            if ui.small_button(glyph).clicked() {
                                let now = !*expanded.get(&lkey).unwrap_or(&false);
                                expanded.insert(lkey.clone(), now);
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
                        });
                        if *expanded.get(&lkey).unwrap_or(&false) {
                            if !kids.contains_key(&lkey) {
                                load_kids(catalog, &lib.key, &lkey, kids);
                            }
                            ui.indent(format!("indent-{lkey}"), |ui| {
                                match show_kids(
                                    ui,
                                    palette,
                                    catalog,
                                    &lib.key,
                                    expanded,
                                    kids,
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
    cache_key: &str,
    kids: &mut HashMap<String, Vec<FolderView>>,
) {
    let folders = catalog
        .browse(Some(key))
        .map(|v| v.folders)
        .unwrap_or_default();
    kids.insert(cache_key.to_string(), folders);
}

#[allow(clippy::too_many_arguments)]
fn show_kids(
    ui: &mut Ui,
    palette: &crate::skins::SkinPalette,
    catalog: &mut Catalog,
    parent_key: &str,
    expanded: &mut HashMap<String, bool>,
    kids: &mut HashMap<String, Vec<FolderView>>,
    browse_key: &Option<String>,
    selected_key: Option<&str>,
) -> TreeOut {
    let mut out = TreeOut::None;
    if !kids.contains_key(parent_key) {
        load_kids(catalog, parent_key, parent_key, kids);
    }
    let cached: Vec<FolderView> = kids.get(parent_key).cloned().unwrap_or_default();
    for folder in &cached {
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
                        subtitle: format!("{} volumes", folder.items),
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
                    expanded,
                    kids,
                    browse_key,
                    selected_key,
                ) {
                    TreeOut::None => {}
                    other => out = other,
                }
            });
        }
    }
    out
}
