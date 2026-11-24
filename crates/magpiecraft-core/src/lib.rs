//! MagpieCraft core (L1).
//!
//! Holds the domain model, the catalogue engine over SQLite, cover rendering and
//! the command bus shared by the CLI, the MCP server and the desktop app. No UI,
//! no CLI parser, no MCP framing: those live in the L2 and L3 crates.

pub mod catalog;
pub mod command;
pub mod config;
pub mod cover;
pub mod error;
pub mod images;
pub mod model;
pub mod organise;

pub use catalog::{Catalog, SearchMode};
pub use command::{dispatch, Command};
pub use config::{load_settings, save_settings, Paths, Settings};
pub use error::{MagpieError, MagpieResult};
pub use model::{
    BrowseView, Crumb, DriveView, DupGroupView, DuplicatesView, FolderView, ItemKey, ItemView,
    Library, LibraryStatsView, MediaKind, PageSetInfo, ProgressView, ScanItemReport,
    ScanReportView, SearchHit, SearchView, StatsView,
};

/// Re-exports so L2 and L3 do not need to depend on every leaf crate.
pub use magpiecraft_comics::{
    ContainerKind, Crop, FitMode, Helpers, PageSet, Reader, ReaderSettings, Slot, Step,
};
pub use magpiecraft_tools::Toolchain;

/// Version string used in status output and user agents.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
