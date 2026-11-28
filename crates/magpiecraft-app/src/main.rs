//! MagpieCraft desktop app (L3, egui).
//!
//! Calls the L1 engine in-process: no sidecar, no localhost port, no JSON-RPC
//! between the window and the catalogue. `--smoke` runs the same code headless so
//! CI can prove the app opens a catalogue and browses it without a GPU.

mod app;
mod chat;
mod covers;
mod library;
mod onboard;
mod reader;
mod skins;
mod theme;
mod tree;

use clap::Parser;
use magpiecraft_core::{Catalog, Paths};

#[derive(Parser, Debug)]
#[command(
    name = "magpiecraft",
    version,
    about = "MagpieCraft: read and curate a collection you own"
)]
struct Cli {
    /// Open a catalogue at this home instead of the default.
    #[arg(long)]
    home: Option<String>,
    /// Start at one library or folder key.
    #[arg(long)]
    key: Option<String>,
    /// Start on a page: dashboard, browse, organise, stats, settings, help or onboarding.
    #[arg(long, default_value = "dashboard")]
    page: String,
    /// Headless check: open the catalogue, browse, read one page list, exit.
    #[arg(long)]
    smoke: bool,
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let paths = match cli.home.as_deref() {
        Some(home) => Paths::at(home),
        None => Paths::discover(),
    };
    if cli.smoke {
        return match smoke(paths, cli.key.as_deref()) {
            Ok(report) => {
                println!("{report}");
                std::process::ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("[magpiecraft] smoke failed: {e}");
                std::process::ExitCode::FAILURE
            }
        };
    }
    match app::run(paths, cli.key, &cli.page) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("[magpiecraft] {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// Headless proof that the engine wiring the window uses actually works.
fn smoke(paths: Paths, key: Option<&str>) -> Result<serde_json::Value, String> {
    let catalog = Catalog::open(paths).map_err(|e| e.to_string())?;
    let libraries = catalog.libraries().map_err(|e| e.to_string())?;
    let browse = catalog.browse(key).map_err(|e| e.to_string())?;
    let first_item = browse.items.first().map(|i| i.key.clone());
    let pages = match &first_item {
        Some(k) => catalog.pages(k).ok(),
        None => None,
    };
    Ok(serde_json::json!({
        "ok": true,
        "smoke": "app",
        "version": magpiecraft_core::version(),
        "libraries": libraries.len(),
        "library_names": libraries.iter().map(|l| l.name.clone()).collect::<Vec<_>>(),
        "folders_on_screen": browse.total_folders,
        "items_on_screen": browse.total_items,
        "first_item": first_item,
        "first_item_pages": pages.map(|p| p.count),
        "crates": "core, store, scan, meta, health, tools, sources, comics, adapters, mcp, app",
    }))
}
