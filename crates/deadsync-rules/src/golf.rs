//! GolfScoring.lua's curved scoring. Totals are raw integer strokes; 1,000 raw strokes
//! make one displayed stroke. This is independent of ITG/EX grade scoring.
pub const RAW_PER_STROKE: u64 = 1000;
pub const MISS_STROKES: u32 = 100;
pub const SCORING_VERSION: &str = "golf-curved-1";

/// Score the engine's timing error without reducing its millisecond precision.
#[must_use]
pub fn tap_strokes(offset_ms: Option<f32>) -> u32 {
    strokes_from_ms(offset_ms.map(|ms| f64::from(ms).abs()))
}

#[must_use]
pub fn tap_strokes_seconds(offset: Option<f64>) -> u32 {
    strokes_from_ms(offset.map(|offset| offset.abs() * 1000.0))
}

// The curve floors absolute milliseconds before scoring, so its finite domain
// has only 104 results. Keep the 103.5 ms cutoff before truncating to an index.
const STROKES: [u8; 104] = [
    0, 0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 12, 12, 13,
    14, 14, 15, 16, 17, 17, 18, 19, 19, 20, 21, 21, 22, 23, 24, 24, 25, 26, 27, 28, 29, 30, 31, 32,
    32, 33, 34, 35, 36, 37, 37, 38, 39, 40, 41, 41, 42, 43, 44, 44, 45, 46, 47, 47, 48, 49, 49, 50,
    51, 51, 52, 52, 53, 54, 54, 55, 56, 56, 57, 57, 58, 58, 59, 60, 60, 61, 61, 62, 62, 63, 63, 64,
    64, 65, 65,
];

fn strokes_from_ms(ms: Option<f64>) -> u32 {
    match ms {
        Some(ms) if (0.0..=103.5).contains(&ms) => u32::from(STROKES[ms as usize]),
        _ => MISS_STROKES,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GolfScore {
    pub raw_strokes: u64,
    pub judged_rows: u32,
}

impl GolfScore {
    pub fn record_tap(&mut self, offset_ms: Option<f32>) -> u32 {
        let strokes = tap_strokes(offset_ms);
        self.raw_strokes = self.raw_strokes.saturating_add(u64::from(strokes));
        self.judged_rows = self.judged_rows.saturating_add(1);
        strokes
    }

    #[must_use]
    pub fn with_penalties(self, dropped_holds: u32, dropped_rolls: u32, mines_hit: u32) -> Self {
        let penalties = u64::from(dropped_holds) + u64::from(dropped_rolls) + u64::from(mines_hit);
        Self {
            raw_strokes: self
                .raw_strokes
                .saturating_add(penalties * u64::from(MISS_STROKES)),
            ..self
        }
    }

    #[must_use]
    pub fn displayed(self) -> f64 {
        self.raw_strokes as f64 / RAW_PER_STROKE as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_supplied_lua_curve_and_boundaries() {
        for (ms, expected) in [
            (0.0, 0),
            (4.999, 0),
            (5.0, 1),
            (23.0, 10),
            (45.0, 25),
            (50.0, 30),
            (103.5, 65),
            (103.6, 100),
            (-50.0, 30),
        ] {
            assert_eq!(tap_strokes_seconds(Some(ms / 1000.0)), expected, "{ms}");
        }
        assert_eq!(tap_strokes(None), 100);
        assert_eq!(tap_strokes(Some(f32::NAN)), 100);
    }

    #[test]
    fn golf_rounds_each_row_before_summing() {
        let mut score = GolfScore::default();
        score.record_tap(Some(5.0));
        score.record_tap(Some(5.0));
        assert_eq!(score.raw_strokes, 2);
    }

    #[test]
    fn sums_rows_and_separate_hold_roll_mine_penalties() {
        let mut score = GolfScore::default();
        score.record_tap(Some(0.0));
        score.record_tap(Some(50.0));
        score.record_tap(None);
        let result = score.with_penalties(1, 1, 1);
        assert_eq!(result.raw_strokes, 430);
        assert_eq!(result.judged_rows, 3);
        assert_eq!(result.displayed(), 0.430);
    }
}
