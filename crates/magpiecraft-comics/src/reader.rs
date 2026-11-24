//! Reader layout: pure logic, no UI, no files.
//!
//! This mirrors the browser prototype (`proto/read.html`) because that is what
//! Sandra uses today. Every rule here is a test: cover alone in two-page mode,
//! spread pairing, right-to-left order, half-page mode stepping top then bottom
//! then next page, and the printed-page offset.
//!
//! Half-page geometry: the top half shows 0 to 56 percent of the page height, the
//! bottom half shows 44 to 100 percent, so the two views overlap by 12 percent and
//! nothing at the cut is lost.

use serde::{Deserialize, Serialize};

/// Fraction of the page height shown in half-page mode.
pub const HALF_FRACTION: f32 = 0.56;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FitMode {
    Width,
    Height,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Crop {
    Full,
    Top,
    Bottom,
}

impl Crop {
    /// Vertical source range as fractions of the page height: (top, bottom).
    pub fn source_range(self) -> (f32, f32) {
        match self {
            Crop::Full => (0.0, 1.0),
            Crop::Top => (0.0, HALF_FRACTION),
            Crop::Bottom => (1.0 - HALF_FRACTION, 1.0),
        }
    }
}

/// One image to draw: which page, and which part of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    pub page: usize,
    pub crop: Crop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Forward,
    Backward,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReaderSettings {
    /// Two pages side by side, the cover alone.
    pub two_page: bool,
    /// Top and bottom halves enlarged.
    pub half_mode: bool,
    /// Right-to-left reading, default for manga libraries.
    pub rtl: bool,
    pub fit: FitMode,
    /// scan page minus printed page: how many pages of front matter the scan has
    /// in front of printed page 1. Printed 5 with offset 2 is scan page 7.
    pub page_offset: i32,
}

impl Default for ReaderSettings {
    fn default() -> Self {
        ReaderSettings {
            two_page: false,
            half_mode: false,
            rtl: false,
            fit: FitMode::Width,
            page_offset: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Reader {
    page: usize,
    count: usize,
    settings: ReaderSettings,
    half_bottom: bool,
}

impl Reader {
    pub fn new(count: usize, settings: ReaderSettings) -> Self {
        Reader {
            page: 1,
            count,
            settings,
            half_bottom: false,
        }
    }

    /// Restore a saved position, clamped into the volume.
    pub fn with_page(count: usize, settings: ReaderSettings, page: usize) -> Self {
        let mut r = Reader::new(count, settings);
        r.set_page(page);
        r
    }

    pub fn count(&self) -> usize {
        self.count
    }

    pub fn page(&self) -> usize {
        self.page
    }

    pub fn settings(&self) -> &ReaderSettings {
        &self.settings
    }

    /// Change settings. Layout-only changes keep the page; a new page count
    /// re-clamps the position.
    pub fn set_settings(&mut self, settings: ReaderSettings) {
        self.settings = settings;
        self.half_bottom = false;
        self.clamp();
    }

    pub fn set_page(&mut self, page: usize) {
        self.page = page.max(1);
        self.half_bottom = false;
        self.clamp();
    }

    fn clamp(&mut self) {
        if self.count == 0 {
            self.page = 1;
        } else if self.page > self.count {
            self.page = self.count;
        }
    }

    /// What to draw, in display order (left to right on screen).
    pub fn slots(&self) -> Vec<Slot> {
        if self.count == 0 {
            return Vec::new();
        }
        if self.settings.half_mode {
            return vec![Slot {
                page: self.page,
                crop: if self.half_bottom {
                    Crop::Bottom
                } else {
                    Crop::Top
                },
            }];
        }
        let mut pages = if self.settings.two_page && self.page > 1 {
            vec![self.page, self.page + 1]
        } else {
            vec![self.page]
        };
        pages.retain(|p| *p >= 1 && *p <= self.count);
        if self.settings.rtl {
            pages.reverse();
        }
        pages
            .into_iter()
            .map(|page| Slot {
                page,
                crop: Crop::Full,
            })
            .collect()
    }

    /// True when the current position shows the bottom half of a page.
    pub fn half_bottom(&self) -> bool {
        self.half_bottom
    }

    /// Advance or retreat by one "screen". In half mode a step is top then bottom;
    /// in two-page mode a step is a spread, except that page one is the cover
    /// alone, so the first forward step lands on page two.
    pub fn step(&mut self, direction: Step) {
        if self.count == 0 {
            return;
        }
        if self.settings.half_mode {
            match direction {
                Step::Forward => {
                    if self.half_bottom {
                        if self.page < self.count {
                            self.page += 1;
                            self.half_bottom = false;
                        }
                        // On the bottom half of the last page there is nothing left,
                        // so the position stays put instead of bouncing to the top.
                    } else {
                        self.half_bottom = true;
                    }
                }
                Step::Backward => {
                    if self.half_bottom {
                        self.half_bottom = false;
                    } else if self.page > 1 {
                        self.page -= 1;
                        self.half_bottom = true;
                    }
                }
            }
            return;
        }
        let stride = if self.settings.two_page {
            if self.page <= 1 {
                1
            } else {
                2
            }
        } else {
            1
        };
        match direction {
            Step::Forward => {
                self.page = (self.page + stride).min(self.count);
            }
            Step::Backward => {
                let back = if self.settings.two_page && self.page <= 2 {
                    1
                } else {
                    stride
                };
                self.page = self.page.saturating_sub(back).max(1);
            }
        }
    }

    /// "7 / 220", "8-9 / 220" for a spread, "7 / 220 bottom" in half mode.
    pub fn position_label(&self) -> String {
        if self.count == 0 {
            return "0 / 0".to_string();
        }
        if self.settings.half_mode {
            return format!(
                "{} / {} {}",
                self.page,
                self.count,
                if self.half_bottom { "bottom" } else { "top" }
            );
        }
        if self.settings.two_page && self.page > 1 && self.page < self.count {
            return format!("{}-{} / {}", self.page, self.page + 1, self.count);
        }
        format!("{} / {}", self.page, self.count)
    }

    /// Printed page number for the current scan page.
    pub fn printed_page(&self) -> i32 {
        let page = i32::try_from(self.page).unwrap_or(i32::MAX);
        page.saturating_sub(self.settings.page_offset)
    }

    /// Jump to a printed page number (what is written on the page).
    pub fn goto_printed(&mut self, printed: i32) {
        let scan = printed.saturating_add(self.settings.page_offset);
        let page = usize::try_from(scan.max(1)).unwrap_or(1);
        self.set_page(page);
    }

    pub fn at_end(&self) -> bool {
        self.count > 0 && self.page >= self.count
    }

    pub fn progress_fraction(&self) -> f32 {
        if self.count == 0 {
            return 0.0;
        }
        (self.page as f32 / self.count as f32).clamp(0.0, 1.0)
    }

    /// Pages worth prefetching, nearest first, excluding what is on screen.
    pub fn prefetch_pages(&self, ahead: usize) -> Vec<usize> {
        let shown: Vec<usize> = self.slots().iter().map(|s| s.page).collect();
        let mut out = Vec::new();
        let mut p = self.page + 1;
        while out.len() < ahead && p <= self.count {
            if !shown.contains(&p) {
                out.push(p);
            }
            p += 1;
        }
        out
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn two_page(rtl: bool, count: usize) -> Reader {
        Reader::new(
            count,
            ReaderSettings {
                two_page: true,
                rtl,
                ..ReaderSettings::default()
            },
        )
    }

    #[test]
    fn cover_is_alone_then_spreads_of_two() {
        let mut r = two_page(false, 10);
        assert_eq!(
            r.slots(),
            vec![Slot {
                page: 1,
                crop: Crop::Full
            }]
        );
        r.step(Step::Forward);
        assert_eq!(r.page(), 2);
        assert_eq!(
            r.slots(),
            vec![
                Slot {
                    page: 2,
                    crop: Crop::Full
                },
                Slot {
                    page: 3,
                    crop: Crop::Full
                }
            ]
        );
        r.step(Step::Forward);
        assert_eq!(r.page(), 4);
        assert_eq!(r.position_label(), "4-5 / 10");
    }

    #[test]
    fn right_to_left_reverses_display_order_only() {
        let mut r = two_page(true, 10);
        r.set_page(2);
        let pages: Vec<usize> = r.slots().iter().map(|s| s.page).collect();
        assert_eq!(pages, vec![3, 2], "page 3 is drawn left of page 2");
        assert_eq!(r.position_label(), "2-3 / 10", "the label stays ascending");
    }

    #[test]
    fn backward_from_the_first_spread_returns_to_the_cover() {
        let mut r = two_page(false, 10);
        r.set_page(2);
        r.step(Step::Backward);
        assert_eq!(r.page(), 1);
        r.step(Step::Backward);
        assert_eq!(r.page(), 1, "never before the cover");
    }

    #[test]
    fn last_spread_clamps_and_does_not_overrun() {
        let mut r = two_page(false, 10);
        r.set_page(8);
        r.step(Step::Forward);
        assert_eq!(r.page(), 10);
        assert_eq!(r.slots().len(), 1, "page 11 does not exist");
        r.step(Step::Forward);
        assert_eq!(r.page(), 10);
        assert!(r.at_end());
    }

    #[test]
    fn single_page_mode_steps_by_one() {
        let mut r = Reader::new(5, ReaderSettings::default());
        r.step(Step::Forward);
        assert_eq!(r.page(), 2);
        assert_eq!(r.slots().len(), 1);
        r.step(Step::Backward);
        assert_eq!(r.page(), 1);
    }

    #[test]
    fn half_page_walks_top_then_bottom_then_next_page() {
        let mut r = Reader::new(
            3,
            ReaderSettings {
                half_mode: true,
                ..ReaderSettings::default()
            },
        );
        assert_eq!(r.slots()[0].crop, Crop::Top);
        r.step(Step::Forward);
        assert_eq!(r.slots()[0].crop, Crop::Bottom, "bottom of the same page");
        assert_eq!(r.page(), 1);
        r.step(Step::Forward);
        assert_eq!(r.page(), 2);
        assert_eq!(r.slots()[0].crop, Crop::Top);
        assert_eq!(r.position_label(), "2 / 3 top");
    }

    #[test]
    fn half_page_backwards_goes_top_then_previous_bottom() {
        let mut r = Reader::new(
            3,
            ReaderSettings {
                half_mode: true,
                ..ReaderSettings::default()
            },
        );
        r.set_page(2);
        r.step(Step::Backward);
        assert_eq!(r.page(), 1);
        assert_eq!(r.slots()[0].crop, Crop::Bottom);
        r.step(Step::Backward);
        assert_eq!(r.page(), 1);
        assert_eq!(r.slots()[0].crop, Crop::Top);
    }

    #[test]
    fn half_page_crops_overlap_by_twelve_percent() {
        let (t0, t1) = Crop::Top.source_range();
        let (b0, b1) = Crop::Bottom.source_range();
        assert!((t0 - 0.0).abs() < f32::EPSILON);
        assert!((t1 - 0.56).abs() < 0.001);
        assert!((b0 - 0.44).abs() < 0.001);
        assert!((b1 - 1.0).abs() < f32::EPSILON);
        assert!(b0 < t1, "the halves must overlap, not leave a gap");
    }

    #[test]
    fn printed_page_offset_maps_both_ways() {
        let mut r = Reader::new(
            100,
            ReaderSettings {
                page_offset: 2,
                ..ReaderSettings::default()
            },
        );
        r.goto_printed(5);
        assert_eq!(r.page(), 7);
        assert_eq!(r.printed_page(), 5);
        let mut negative = Reader::new(
            100,
            ReaderSettings {
                page_offset: -3,
                ..ReaderSettings::default()
            },
        );
        negative.goto_printed(10);
        assert_eq!(negative.page(), 7);
        assert_eq!(negative.printed_page(), 10);
    }

    #[test]
    fn jump_is_clamped_into_the_volume() {
        let mut r = Reader::new(20, ReaderSettings::default());
        r.set_page(999);
        assert_eq!(r.page(), 20);
        r.set_page(0);
        assert_eq!(r.page(), 1);
    }

    #[test]
    fn empty_volume_is_not_a_panic() {
        let mut r = Reader::new(0, ReaderSettings::default());
        assert!(r.slots().is_empty());
        assert_eq!(r.position_label(), "0 / 0");
        r.step(Step::Forward);
        r.step(Step::Backward);
        assert_eq!(r.progress_fraction(), 0.0);
        assert!(!r.at_end());
    }

    #[test]
    fn prefetch_skips_what_is_on_screen() {
        let mut r = two_page(false, 10);
        r.set_page(2);
        assert_eq!(r.prefetch_pages(3), vec![4, 5, 6]);
        assert_eq!(r.progress_fraction(), 0.2);
    }

    #[test]
    fn toggling_half_mode_resets_the_sub_position() {
        let mut r = Reader::new(4, ReaderSettings::default());
        r.step(Step::Forward);
        r.set_settings(ReaderSettings {
            half_mode: true,
            ..ReaderSettings::default()
        });
        assert_eq!(r.slots()[0].crop, Crop::Top);
        assert_eq!(r.page(), 2, "switching modes keeps the page");
    }
}
