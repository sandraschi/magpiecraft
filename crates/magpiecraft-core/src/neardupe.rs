//! Near duplicates (pillar C).
//!
//! Two different questions, answered separately and labelled honestly:
//!
//! - **Perceptual image duplicates**: a 64-bit difference hash (dHash) of the image,
//!   compared by Hamming distance. Recompressed, resized or slightly cropped copies
//!   land within a handful of bits; two different photographs do not.
//! - **Possible video duplicates**: the same title and year in more than one file.
//!   That is a name comparison, not a perceptual one: it catches the common case of
//!   the same film kept twice in different qualities, and it says so. When ffprobe is
//!   available the durations are reported next to it so a human can decide.
//!
//! Report only. Nothing here quarantines anything; a later plan can, with the usual
//! diff and journal.

use crate::catalog::Catalog;
use crate::error::MagpieResult;
use serde_json::{json, Value};
use std::collections::HashMap;

/// Bits that may differ and still be called the same picture, out of 64.
pub const IMAGE_DISTANCE: u32 = 10;

/// How many photos are hashed in one report. Beyond this the pairwise comparison
/// stops being instant, and a report is not worth a minute of decoding.
pub const MAX_PHOTOS: usize = 5000;

/// 64-bit difference hash: 8x9 grayscale, one bit per horizontal neighbour pair.
pub fn dhash(image: &image::DynamicImage) -> u64 {
    let luma = image.to_luma8();
    let small = image::imageops::resize(&luma, 9, 8, image::imageops::FilterType::Triangle);
    let mut hash = 0u64;
    let mut bit = 0u32;
    for y in 0..8u32 {
        for x in 0..8u32 {
            let a = small.get_pixel(x, y)[0];
            let b = small.get_pixel(x + 1, y)[0];
            if a > b {
                hash |= 1u64 << bit;
            }
            bit += 1;
        }
    }
    hash
}

pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// A title reduced to what actually distinguishes two files: letters, digits and
/// single spaces, lower case, no year and no release tags.
///
/// An episode marker is part of the identity, so every episode of a series is its
/// own key rather than one group per series. So is a `Part 1` / `CD 2` suffix, so a
/// two-part film does not look like a duplicate of itself. Cast notes in brackets are
/// not part of it: the same recording with a cast annotation is still the same
/// recording.
pub fn title_key(title: &str) -> Option<String> {
    let (name, year, tail) = crate::organise::video_title_and_year(title)?;
    let mut key = format!("{}|{year}", normalise_words(&name));
    if let Some((season, episode, _)) = crate::organise::episode_of(title) {
        key.push_str(&format!("|s{season:02}e{episode:02}"));
    } else if let Some(suffix) = part_suffix(&tail) {
        key.push('|');
        key.push_str(&suffix);
    }
    Some(key)
}

/// "Part 1", "CD 2", "Disc 3", "Vol 4" from a tail, if that is what the tail is.
fn part_suffix(tail: &str) -> Option<String> {
    const WORDS: &[&str] = &["part", "pt", "cd", "disc", "disk", "vol", "volume"];
    let tokens: Vec<String> = tail
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_ascii_lowercase())
        .collect();
    for (i, token) in tokens.iter().enumerate() {
        if WORDS.contains(&token.as_str()) {
            if let Some(next) = tokens.get(i + 1) {
                if next.chars().all(|c| c.is_ascii_digit()) {
                    return Some(format!("{token}{next}"));
                }
            }
        }
    }
    None
}

fn normalise_words(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_space = false;
    for ch in name.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
            last_space = false;
        } else if !last_space && !out.is_empty() {
            out.push(' ');
            last_space = true;
        }
    }
    out.trim().to_string()
}

impl Catalog {
    /// Report near duplicates: perceptual image groups and possible video duplicates.
    pub fn near_duplicates(&self, limit: usize) -> MagpieResult<Value> {
        let limit = if limit == 0 { 200 } else { limit };
        let libs = self.store().libraries()?;
        let lib_name = |id: i64| {
            libs.iter()
                .find(|l| l.id == id)
                .map(|l| l.name.clone())
                .unwrap_or_default()
        };

        // --- images, by perceptual hash
        let photos = self.store().items_of_kind("photo", MAX_PHOTOS)?;
        let scanned = photos.len();
        let mut hashed: Vec<(u64, String, String, i64, String)> = Vec::new();
        for row in &photos {
            let lib = libs.iter().find(|l| l.id == row.library_id);
            let Some(lib) = lib else { continue };
            let path = crate::catalog::library_path(lib, &row.rel);
            if let Ok(img) = image::open(&path) {
                hashed.push((
                    dhash(&img),
                    row.key(),
                    row.title.clone(),
                    row.size,
                    lib.name.clone(),
                ));
            }
        }
        let mut image_groups: Vec<Value> = Vec::new();
        let mut used = vec![false; hashed.len()];
        for i in 0..hashed.len() {
            if used.get(i).copied().unwrap_or(true) {
                continue;
            }
            let (hash, _, _, _, _) = match hashed.get(i) {
                Some(h) => h,
                None => continue,
            };
            let mut members: Vec<Value> = Vec::new();
            for j in (i + 1)..hashed.len() {
                if used.get(j).copied().unwrap_or(true) {
                    continue;
                }
                if let Some((other, key, title, size, library)) = hashed.get(j) {
                    let distance = hamming(*hash, *other);
                    if distance <= IMAGE_DISTANCE {
                        if let Some(slot) = used.get_mut(j) {
                            *slot = true;
                        }
                        members.push(json!({
                            "key": key,
                            "title": title,
                            "library": library,
                            "size": size,
                            "distance": distance,
                        }));
                    }
                }
            }
            if !members.is_empty() {
                if let Some((_, key, title, size, library)) = hashed.get(i) {
                    if let Some(slot) = used.get_mut(i) {
                        *slot = true;
                    }
                    let mut group = vec![json!({
                        "key": key,
                        "title": title,
                        "library": library,
                        "size": size,
                        "distance": 0,
                    })];
                    group.extend(members);
                    image_groups.push(json!({ "kind": "image", "items": group }));
                }
            }
            if image_groups.len() >= limit {
                break;
            }
        }

        // --- video, by title and year
        let videos = self.store().items_of_kind("video", 200_000)?;
        let mut by_title: HashMap<String, Vec<&magpiecraft_store::ItemRow>> = HashMap::new();
        for row in &videos {
            if let Some(key) = title_key(&row.title) {
                by_title.entry(key).or_default().push(row);
            }
        }
        let mut video_groups: Vec<Value> = Vec::new();
        for (key, rows) in by_title.iter().filter(|(_, rows)| rows.len() > 1) {
            let mut items: Vec<Value> = Vec::new();
            for row in rows {
                items.push(json!({
                    "key": row.key(),
                    "title": row.title,
                    "library": lib_name(row.library_id),
                    "size": row.size,
                    "page_count": row.page_count,
                }));
            }
            video_groups.push(json!({
                "kind": "video",
                "title_key": key,
                "items": items,
                "note": "same title and year: compare the files before keeping one",
            }));
            if video_groups.len() >= limit {
                break;
            }
        }
        video_groups.sort_by(|a, b| {
            let count = |v: &Value| v["items"].as_array().map(|i| i.len()).unwrap_or(0);
            count(b).cmp(&count(a))
        });

        let mut exact = 0usize;
        for group in image_groups.iter().chain(video_groups.iter()) {
            exact += group["items"]
                .as_array()
                .map(|i| i.len())
                .unwrap_or(0)
                .saturating_sub(1);
        }
        Ok(json!({
            "ok": true,
            "image_groups": image_groups.len(),
            "video_groups": video_groups.len(),
            "extra_copies": exact,
            "photos_hashed": scanned,
            "videos_compared": videos.len(),
            "image_distance": IMAGE_DISTANCE,
            "groups": image_groups.into_iter().chain(video_groups.into_iter()).collect::<Vec<_>>(),
            "note": "images are compared by perceptual hash; videos by title and year, which is a name comparison, not a fingerprint",
        }))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32, shift: u8) -> image::DynamicImage {
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_fn(w, h, |x, y| {
            let v = ((x * 255 / w.max(1)) as u8).wrapping_add(shift);
            image::Rgba([v, y as u8, 128, 255])
        }))
    }

    #[test]
    fn a_resized_copy_has_the_same_hash_and_a_different_picture_does_not() {
        let base = gradient(400, 600, 0);
        let resized = base.resize_exact(200, 300, image::imageops::FilterType::Triangle);
        assert_eq!(
            dhash(&base),
            dhash(&resized),
            "a resize keeps the structure"
        );
        let other = gradient(400, 600, 0).fliph();
        let distance = hamming(dhash(&base), dhash(&other));
        assert!(
            distance > IMAGE_DISTANCE,
            "a mirrored gradient must not be called a duplicate ({distance} bits)"
        );
    }

    #[test]
    fn hamming_counts_differing_bits() {
        assert_eq!(hamming(0, 0), 0);
        assert_eq!(hamming(0b1011, 0b1001), 1);
        assert_eq!(hamming(u64::MAX, 0), 64);
    }

    #[test]
    fn a_title_key_ignores_case_punctuation_and_release_tags() {
        let a = title_key("Shaun.The.Sheep.2026.1080p.BluRay.x265").unwrap();
        let b = title_key("Shaun The Sheep (2026)").unwrap();
        assert_eq!(a, b);
        assert_eq!(
            title_key("Zootopia .2 2025").unwrap(),
            "zootopia 2|2025",
            "a number inside the title is part of it"
        );
        // A film with no year is not grouped: nothing to compare on.
        assert!(title_key("Some Home Video").is_none());
        assert_ne!(
            title_key("Alien 1979").unwrap(),
            title_key("Aliens 1986").unwrap()
        );
    }

    #[test]
    fn an_episode_is_its_own_key_and_a_part_suffix_keeps_multi_part_films_apart() {
        // Every episode of a series used to collapse into one "duplicate" group.
        let e1 = title_key("Battlestar Galactica (2003) S01E01 33").unwrap();
        let e2 = title_key("Battlestar Galactica (2003) S01E02 Water").unwrap();
        assert_ne!(e1, e2);
        assert!(e1.ends_with("|s01e01"), "{e1}");
        // Two copies of the same episode still group.
        assert_eq!(
            e1,
            title_key("Battlestar Galactica (2003) S01E01 33 1080p").unwrap()
        );
        // A two-part film is two files, not a duplicate.
        assert_ne!(
            title_key("Shoah (1985) PART 1").unwrap(),
            title_key("Shoah (1985) PART 2").unwrap()
        );
        // A cast note in brackets is not part of the identity.
        assert_eq!(
            title_key("La Traviata 1982 [Zeffirelli, Stratas, Domingo]").unwrap(),
            title_key("La Traviata 1982").unwrap()
        );
    }
}
