//! Where everything sits, ported from the ITGmania Content Browser's own
//! `23 layout.lua` so the two look like the same program.
//!
//! The virtual screen is height-fixed at 480 and 854 wide at 16:9 or wider.
//! DeadSync's `metrics_for_aspect` says "Match SM/SL exactly: 854 units at
//! 16:9 or wider", which is the same basis the Lua was written against, so
//! these constants port across unchanged.
//!
//! The original refuses to draw below 700 wide (`LO.MIN_WIDTH`) and shows a
//! "needs a widescreen display" panel instead. That gate is kept: the tab strip
//! alone needs 851px, and there is no reflow anywhere in the original.

use deadlib_present::space::{screen_height, screen_width};
use std::time::{SystemTime, UNIX_EPOCH};

/// Below this the browser will not lay out. Nine tab pills need 851px and the
/// original has no wrap, clip or reflow for them.
pub(super) const MIN_WIDTH: f32 = 700.0;

pub(super) const CONTENT_BOT: f32 = 440.0;
pub(super) const LIST_X: f32 = 16.0;
pub(super) const ROW_H: f32 = 35.0;
/// Rows per page, and the catalogue page size the original requests.
pub(super) const ROWS: usize = 7;
/// How far below the last row the "more" mark sits, as the original has it.
pub(super) const MORE_MARK_DY: f32 = 9.0;
pub(super) const SCROLL_W: f32 = 4.0;
pub(super) const GUTTER: f32 = 8.0;
pub(super) const HEADER_RULE_Y: f32 = 42.0;

// --- tab strip --------------------------------------------------------------
pub(super) const TABS_X: f32 = 196.0;
pub(super) const TABS_Y: f32 = 22.0;
pub(super) const TABS_PITCH: f32 = 72.0;
pub(super) const TAB_W: f32 = TABS_PITCH - 6.0;
pub(super) const TAB_H: f32 = 20.0;
pub(super) const TAB_LABEL_ZOOM: f32 = 0.48;
/// The icon sits between the pill's left edge and the label, which is why the
/// label starts 15px in rather than hard against the plate.
pub(super) const TAB_ICON_INSET: f32 = 7.0;
pub(super) const TAB_ICON_PX: f32 = 12.0;
pub(super) const TAB_LABEL_INSET: f32 = 15.0;

// --- featured grid ----------------------------------------------------------
pub(super) const FEAT_COLS: usize = 6;
pub(super) const FEAT_ROWS: usize = 2;
pub(super) const FEAT_LABEL_Y: f32 = HEADER_RULE_Y + 14.0; // 56
pub(super) const FEAT_PANEL_Y: f32 = FEAT_LABEL_Y + GUTTER; // 64
pub(super) const FEAT_PAD: f32 = 4.0;
pub(super) const FEAT_PAD_TOP: f32 = 8.0;
pub(super) const FEAT_TOP: f32 = FEAT_PANEL_Y + FEAT_PAD_TOP; // 72
pub(super) const FEAT_CARD_H: f32 = 41.0;
pub(super) const FEAT_DOT: f32 = 8.0;
pub(super) const FEAT_GAP: f32 = 4.0;
pub(super) const FEAT_ROW_GAP: f32 = 4.0;
/// 72 + 2*41 + 4 = 158
pub(super) const FEAT_BOT: f32 =
    FEAT_TOP + FEAT_ROWS as f32 * FEAT_CARD_H + (FEAT_ROWS as f32 - 1.0) * FEAT_ROW_GAP;
/// 158 + 4 + 4 = 166
pub(super) const FEAT_RULE_Y: f32 = FEAT_BOT + FEAT_PAD + FEAT_DOT / 2.0;
/// 174 - 64 = 110
pub(super) const FEAT_PANEL_H: f32 = (FEAT_RULE_Y + FEAT_DOT / 2.0 + FEAT_PAD) - FEAT_PANEL_Y;

/// 64 + 110 + 8 = 182, with the grid showing.
pub(super) const LIST_TOP: f32 = FEAT_PANEL_Y + FEAT_PANEL_H + GUTTER;
/// 72 + 63 = 135, without it.
pub(super) const LIST_TOP_TIGHT: f32 = FEAT_TOP + 63.0;

// --- one row's children, in row-local coordinates ---------------------------
/// The banner's centre, and the spinner that stands in for it.
pub(super) const ROW_ART_CX: f32 = 42.0;
pub(super) const ROW_ART_CY: f32 = (ROW_H - 3.0) / 2.0; // 16
/// The box a banner is fitted into, aspect preserved.
pub(super) const ROW_ART_W: f32 = 76.0;
pub(super) const ROW_ART_H: f32 = ROW_H - 9.0; // 26
pub(super) const ROW_TEXT_X: f32 = 88.0;
pub(super) const ROW_NAME_Y: f32 = 11.0;
pub(super) const ROW_NAME_ZOOM: f32 = 0.72;
pub(super) const ROW_META_Y: f32 = 25.0;
pub(super) const ROW_META_ZOOM: f32 = 0.5;
/// Room reserved on the right of a row for the two badges.
pub(super) const ROW_TEXT_RESERVE: f32 = 176.0;
pub(super) const ROW_BADGE_Y: f32 = 16.0;
pub(super) const ROW_TYPE_BADGE_INSET: f32 = 84.0;
pub(super) const ROW_STATUS_BADGE_INSET: f32 = 8.0;
/// The visible gap between one row background and the next.
pub(super) const ROW_FOCUS_H: f32 = ROW_H - 3.0; // 32

// --- context band (search results, level views) -----------------------------
/// The band sits where the featured grid's first card row would be, and its
/// subtitle deliberately overhangs the bottom of it -- which is the whole
/// reason `LIST_TOP_TIGHT` is `FEAT_TOP + 63` rather than the band's own height.
pub(super) const BAND_Y: f32 = FEAT_TOP; // 72
pub(super) const BAND_H: f32 = FEAT_CARD_H; // 41
pub(super) const BAND_TITLE_X: f32 = LIST_X + 18.0; // 34
pub(super) const BAND_TITLE_Y: f32 = FEAT_TOP + 26.0; // 98
pub(super) const BAND_SUB_Y: f32 = FEAT_TOP + 52.0; // 124

// --- year strip -------------------------------------------------------------
/// The first year the strip goes back to. Everything older is one bucket.
pub(super) const YEAR_FLOOR: u16 = 2019;
pub(super) const YEAR_CHIP_H: f32 = 34.0;
pub(super) const YEAR_CHIP_Y: f32 = FEAT_TOP + 18.0; // 90
pub(super) const YEAR_CHIP_GAP: f32 = 10.0;
/// The index readout beside the header, while the walk is still running.
pub(super) const YEAR_PROGRESS_W: f32 = 140.0;
pub(super) const YEAR_PROGRESS_H: f32 = 4.0;

// --- doubles: two columns ---------------------------------------------------
pub(super) const DBL_GAP: f32 = 20.0;
/// The column heading and the rule under it.
pub(super) const DBL_HEAD: f32 = 22.0;
pub(super) const DBL_ROWS: usize = 7;
pub(super) const DBL_ART_CX: f32 = 38.0;
pub(super) const DBL_ART_W: f32 = 68.0;
pub(super) const DBL_TEXT_X: f32 = 80.0;

// --- the installed view: a two-column grid, not a list ----------------------
pub(super) const INST_TOP: f32 = FEAT_TOP; // 72
pub(super) const INST_ROW_H: f32 = 32.0;
pub(super) const INST_COLS: usize = 2;
/// Cells on one page: two columns of eleven.
pub(super) const INST_PER_COL: usize = 11;
pub(super) const INST_ROWS: usize = INST_COLS * INST_PER_COL;
pub(super) const INST_GAP: f32 = 12.0;
pub(super) const INST_ART_CX: f32 = 36.0;
pub(super) const INST_ART_W: f32 = 64.0;
pub(super) const INST_TEXT_X: f32 = 76.0;

// --- detail page ------------------------------------------------------------
pub(super) const DET_TAB_X: f32 = 30.0;
pub(super) const DET_TAB_VX: f32 = DET_TAB_X + 82.0; // 112
pub(super) const DET_TAB_Y: f32 = 194.0;
pub(super) const DET_TAB_H: f32 = 15.0;
pub(super) const SONG_TOP: f32 = 66.0;
pub(super) const SONG_ROW_PITCH: f32 = 46.0;
pub(super) const SONG_ROW_H: f32 = SONG_ROW_PITCH - 4.0; // 42
pub(super) const SONG_ROWS: usize = 8;
pub(super) const SONG_ART_W: f32 = 104.0;
pub(super) const SONG_ART_H: f32 = SONG_ROW_H - 8.0; // 34
pub(super) const DL_BTN_W: f32 = 208.0;
pub(super) const DL_BTN_H: f32 = 21.0;
pub(super) const DL_BTN_Y: f32 = SONG_TOP - 14.0; // 52
pub(super) const BHIST_H: f32 = 80.0;
pub(super) const BHIST_Y: f32 = 420.0;

// --- footer -----------------------------------------------------------------
pub(super) const VERSION_Y: f32 = CONTENT_BOT + 15.0; // 455
pub(super) const VERSION_ZOOM: f32 = 0.4;

/// Screen width, which everything horizontal is derived from.
pub(super) fn width() -> f32 {
    screen_width()
}

pub(super) fn height() -> f32 {
    screen_height()
}

/// Too narrow to lay out. The original shows a widescreen notice here rather
/// than a squashed browser, and so does this.
pub(super) fn too_narrow() -> bool {
    width() < MIN_WIDTH
}

/// `math.floor(LO.W * 0.53)` -- 452 at 854, 339 at 640.
pub(super) fn list_w() -> f32 {
    (width() * 0.53).floor()
}

pub(super) fn pane_x() -> f32 {
    LIST_X + list_w() + 12.0
}

pub(super) fn pane_w() -> f32 {
    width() - pane_x() - 16.0
}

/// Where the list starts, which depends on whether the featured grid is above
/// it.
pub(super) fn list_top(grid_showing: bool) -> f32 {
    if grid_showing {
        LIST_TOP
    } else {
        LIST_TOP_TIGHT
    }
}

pub(super) fn feat_panel_w() -> f32 {
    width() - 2.0 * LIST_X
}

pub(super) fn feat_span() -> f32 {
    feat_panel_w() - 2.0 * FEAT_PAD
}

pub(super) fn feat_card_w() -> f32 {
    ((feat_span() - (FEAT_COLS as f32 - 1.0) * FEAT_GAP) / FEAT_COLS as f32).floor()
}

/// The x a tab pill is anchored at. Index is zero-based here; the Lua is
/// one-based and subtracts.
pub(super) fn tab_x(index: usize) -> f32 {
    TABS_X + index as f32 * TABS_PITCH
}

/// The year the strip counts back from.
///
/// Derived from the wall clock rather than baked in, because the original's
/// strip grows by one chip every January and a port that stopped at the year
/// it was written would quietly lose a year of packs.
pub(super) fn current_year() -> u16 {
    const SECONDS_PER_DAY: u64 = 86_400;
    let Ok(since_epoch) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return YEAR_FLOOR;
    };
    let days = (since_epoch.as_secs() / SECONDS_PER_DAY) as i64;
    // Howard Hinnant's civil-from-days, shifted to a 1 March epoch so the leap
    // day falls at the end of the cycle and no month-length table is needed.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let year = yoe + era * 400 + i64::from(mp >= 10);
    u16::try_from(year).unwrap_or(YEAR_FLOOR).max(YEAR_FLOOR)
}

/// Chips in the year strip: one per year back to the floor, plus OLDER.
pub(super) fn year_slots() -> usize {
    (current_year() - YEAR_FLOOR) as usize + 2
}

/// `math.floor((LO.W - 2*LO.LIST_X + 10) / (YEAR_SPAN + 1)) - 10`.
pub(super) fn year_chip_w() -> f32 {
    let slots = year_slots() as f32;
    (((width() - 2.0 * LIST_X + YEAR_CHIP_GAP) / slots).floor() - YEAR_CHIP_GAP).max(24.0)
}

pub(super) fn year_chip_x(slot: usize) -> f32 {
    LIST_X + slot as f32 * (year_chip_w() + YEAR_CHIP_GAP)
}

/// `math.floor((LO.W - 2*LO.LIST_X - LO.DBL_GAP) / 2)` -- 401 at 854.
pub(super) fn dbl_w() -> f32 {
    ((width() - 2.0 * LIST_X - DBL_GAP) / 2.0).floor()
}

pub(super) fn dbl_x(column: usize) -> f32 {
    LIST_X + column as f32 * (dbl_w() + DBL_GAP)
}

/// Where a doubles column's rows start, under its heading and rule.
pub(super) fn dbl_top() -> f32 {
    LIST_TOP_TIGHT + DBL_HEAD
}

/// `math.floor(LO.W * 0.38)` -- 324 at 854.
pub(super) fn det_left_w() -> f32 {
    (width() * 0.38).floor()
}

pub(super) fn det_songs_x() -> f32 {
    det_left_w() + 32.0
}

pub(super) fn det_songs_w() -> f32 {
    width() - det_songs_x() - 20.0
}

/// The centre of the detail page's left column.
pub(super) fn det_left_cx() -> f32 {
    LIST_X + det_left_w() / 2.0
}

/// The vertical centre of one row of the detail page's fact table.
pub(super) fn det_tab_y(row: usize) -> f32 {
    DET_TAB_Y + (row as f32 + 0.5) * DET_TAB_H
}

/// `math.floor((LO.W - 2*LO.LIST_X - (INST_COLS-1)*INST_GAP) / INST_COLS)`
/// -- 405 at 854.
pub(super) fn inst_w() -> f32 {
    ((width() - 2.0 * LIST_X - (INST_COLS as f32 - 1.0) * INST_GAP) / INST_COLS as f32).floor()
}

pub(super) fn inst_x(column: usize) -> f32 {
    LIST_X + column as f32 * (inst_w() + INST_GAP)
}

pub(super) fn inst_y(row: usize) -> f32 {
    INST_TOP + row as f32 * INST_ROW_H
}

/// Uniform scale that fits a source into a box, aspect preserved.
///
/// Deliberately not clamped to <= 1: the original upscales a banner smaller
/// than its box rather than leaving a gap.
pub(super) fn fit_zoom(src_w: f32, src_h: f32, max_w: f32, max_h: f32) -> f32 {
    if src_w <= 0.0 || src_h <= 0.0 {
        return 1.0;
    }
    (max_w / src_w).min(max_h / src_h)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The vertical chain is arithmetic on constants, and every one of these
    /// numbers is quoted from the original. If a constant is mistyped the whole
    /// screen shifts, so they are pinned.
    #[test]
    fn the_vertical_chain_matches_the_original() {
        assert_eq!(FEAT_LABEL_Y, 56.0);
        assert_eq!(FEAT_PANEL_Y, 64.0);
        assert_eq!(FEAT_TOP, 72.0);
        assert_eq!(FEAT_BOT, 158.0);
        assert_eq!(FEAT_RULE_Y, 166.0);
        assert_eq!(FEAT_PANEL_H, 110.0);
        assert_eq!(LIST_TOP, 182.0);
        assert_eq!(LIST_TOP_TIGHT, 135.0);
        assert_eq!(ROW_FOCUS_H, 32.0);
        assert_eq!(ROW_ART_CY, 16.0);
        assert_eq!(ROW_ART_H, 26.0);
    }

    /// Seven rows have to clear the bottom of the content area in both modes,
    /// which is the comment against ROW_H in the original.
    #[test]
    fn seven_rows_clear_the_content_bottom() {
        assert!(LIST_TOP + ROWS as f32 * ROW_H <= CONTENT_BOT);
        assert!(LIST_TOP_TIGHT + ROWS as f32 * ROW_H <= CONTENT_BOT);
    }

    /// Nine pills at pitch 72 end at 835, so the strip needs 851px. This is
    /// why the browser refuses to draw narrow rather than reflowing.
    #[test]
    fn the_tab_strip_fits_at_widescreen_and_not_below() {
        let last_right = tab_x(8) - 3.0 + TAB_W;
        assert_eq!(last_right, 835.0);
        assert!(last_right + 16.0 <= 854.0, "fits at 16:9");
        assert!(last_right + 16.0 > MIN_WIDTH, "and not at the gate width");
    }

    /// Nine slots in 2026, and one more every January. The strip has to fit
    /// the screen at every count it can reach for years to come.
    #[test]
    fn the_year_strip_grows_by_a_chip_a_year_and_still_fits() {
        assert!(year_slots() >= 9, "2019..now plus OLDER");
        assert!(current_year() >= 2026);
        for slots in 9..=19 {
            let chip = (((854.0 - 2.0 * LIST_X + YEAR_CHIP_GAP) / slots as f32).floor()
                - YEAR_CHIP_GAP)
                .max(24.0);
            let right = LIST_X + (slots - 1) as f32 * (chip + YEAR_CHIP_GAP) + chip;
            assert!(right <= 854.0 - LIST_X + 2.0, "{slots} chips overflow");
        }
    }

    /// Seven rows under a heading still have to clear the content bottom, the
    /// same invariant the single list has.
    #[test]
    fn both_doubles_columns_fit_the_content_area() {
        assert_eq!(dbl_top(), 157.0);
        assert!(dbl_top() + DBL_ROWS as f32 * ROW_H <= CONTENT_BOT);
    }

    /// Eleven cells of 32 from 72 end at 420, clear of the content bottom, and
    /// two columns of 405 with a 12px gap exactly fill the content width.
    #[test]
    fn the_installed_grid_fills_its_page_without_overflowing() {
        // Metrics are per thread and start as a placeholder until the app
        // sets them, so a width-dependent test has to say which screen.
        deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
            854.0, 480.0,
        ));
        assert_eq!(inst_w(), 405.0);
        assert_eq!(inst_x(0), 16.0);
        assert_eq!(inst_x(1), 433.0);
        assert_eq!(
            inst_x(1) + inst_w(),
            854.0 - LIST_X,
            "flush with the margin"
        );
        assert_eq!(inst_y(0), 72.0);
        assert_eq!(inst_y(INST_PER_COL - 1), 392.0);
        assert!(inst_y(INST_PER_COL - 1) + INST_ROW_H - 4.0 <= CONTENT_BOT);
        assert_eq!(INST_ROWS, 22);
    }

    /// Eight song rows at 46 pitch from 66 end at 430, inside the content area.
    #[test]
    fn the_song_list_fits_the_detail_page() {
        let bottom = SONG_TOP + (SONG_ROWS - 1) as f32 * SONG_ROW_PITCH + SONG_ROW_H;
        assert_eq!(bottom, 430.0);
        assert!(bottom <= CONTENT_BOT);
        assert_eq!(det_tab_y(0), 201.5);
        assert_eq!(det_tab_y(7), 306.5);
    }

    #[test]
    fn a_banner_is_fitted_without_being_clamped() {
        // wider than the box: width-limited
        let z = fit_zoom(800.0, 250.0, ROW_ART_W, ROW_ART_H);
        assert!((z - 76.0 / 800.0).abs() < 1e-6);
        // smaller than the box: upscaled, not left alone
        let z = fit_zoom(38.0, 13.0, ROW_ART_W, ROW_ART_H);
        assert!(z > 1.0);
    }
}
