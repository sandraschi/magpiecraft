//! Cover thumbnails: rendered on a worker thread, uploaded to GPU textures on the
//! UI thread.
//!
//! Covers are requested lazily, only for cards that are on screen, at most a few
//! per frame. Rendering writes a JPEG under the app cache directory; the file is
//! decoded here at card size so 200 cards do not cost 150 MB of texture memory.

use magpiecraft_core::cover::{render_cover, CoverSource};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};

const MAX_TEXTURES: usize = 400;
const TEXTURE_WIDTH: u32 = 220;
const TEXTURE_HEIGHT: u32 = 320;
const LOADS_PER_FRAME: usize = 3;

struct Job {
    key: String,
    source: CoverSource,
}

enum Done {
    Rendered(String, PathBuf),
    Failed(String),
}

pub struct CoverCache {
    tx: Sender<Job>,
    rx: Receiver<Done>,
    textures: HashMap<String, egui::TextureHandle>,
    requested: HashSet<String>,
    failed: HashSet<String>,
}

impl Default for CoverCache {
    fn default() -> Self {
        Self::new()
    }
}

impl CoverCache {
    pub fn new() -> Self {
        let (job_tx, job_rx) = channel::<Job>();
        let (done_tx, done_rx) = channel::<Done>();
        std::thread::Builder::new()
            .name("magpie-covers".to_string())
            .spawn(move || {
                while let Ok(job) = job_rx.recv() {
                    match render_cover(&job.source, 360, 520, 78) {
                        Ok(path) => {
                            if done_tx.send(Done::Rendered(job.key, path)).is_err() {
                                break;
                            }
                        }
                        Err(_) => {
                            if done_tx.send(Done::Failed(job.key)).is_err() {
                                break;
                            }
                        }
                    }
                }
            })
            .ok();
        CoverCache {
            tx: job_tx,
            rx: done_rx,
            textures: HashMap::new(),
            requested: HashSet::new(),
            failed: HashSet::new(),
        }
    }

    /// Ask for a cover once. `source` is resolved by the caller, which is the only
    /// place that touches the database.
    pub fn request(&mut self, key: &str, source: CoverSource) {
        if self.textures.contains_key(key)
            || self.requested.contains(key)
            || self.failed.contains(key)
        {
            return;
        }
        if self
            .tx
            .send(Job {
                key: key.to_string(),
                source,
            })
            .is_ok()
        {
            self.requested.insert(key.to_string());
        }
    }

    pub fn texture(&self, key: &str) -> Option<&egui::TextureHandle> {
        self.textures.get(key)
    }

    pub fn failed(&self, key: &str) -> bool {
        self.failed.contains(key)
    }

    /// Upload finished covers. Call once per frame.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let mut loads = 0usize;
        while loads < LOADS_PER_FRAME {
            match self.rx.try_recv() {
                Ok(Done::Rendered(key, path)) => {
                    loads += 1;
                    if let Some(handle) = load_texture(ctx, &key, &path) {
                        self.textures.insert(key, handle);
                    } else {
                        self.failed.insert(key);
                    }
                }
                Ok(Done::Failed(key)) => {
                    self.failed.insert(key);
                }
                Err(_) => break,
            }
        }
        if self.textures.len() > MAX_TEXTURES {
            // Bounded memory beats a perfect cache: drop everything and let the
            // visible cards request again (the JPEGs are already on disk).
            self.textures.clear();
            self.requested.clear();
            self.failed.clear();
        }
    }
}

fn load_texture(ctx: &egui::Context, key: &str, path: &PathBuf) -> Option<egui::TextureHandle> {
    let img = image::open(path).ok()?;
    let w = img.width().max(1);
    let h = img.height().max(1);
    let scale = (TEXTURE_WIDTH as f32 / w as f32)
        .min(TEXTURE_HEIGHT as f32 / h as f32)
        .min(1.0);
    let tw = ((w as f32 * scale).round() as u32).max(1);
    let th = ((h as f32 * scale).round() as u32).max(1);
    let rgba = image::imageops::thumbnail(&img.to_rgba8(), tw, th);
    let colour =
        egui::ColorImage::from_rgba_unmultiplied([tw as usize, th as usize], rgba.as_raw());
    Some(ctx.load_texture(key, colour, egui::TextureOptions::LINEAR))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_request_is_only_made_once() {
        let mut cache = CoverCache::new();
        let source = CoverSource {
            cache_file: std::env::temp_dir().join("magpie-cover-test/none.jpg"),
            item_path: std::env::temp_dir().join("magpie-cover-test/none.cbz"),
            container: magpiecraft_core::ContainerKind::Zip,
        };
        cache.request("k", source.clone());
        cache.request("k", source);
        assert_eq!(cache.requested.len(), 1);
        assert!(cache.texture("k").is_none());
        assert!(
            !cache.failed("k"),
            "a failed render reports failure only after it fails"
        );
    }
}
