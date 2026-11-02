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

fn strokes_from_ms(ms: Option<f64>) -> u32 {
    let Some(ms) = ms.filter(|v| v.is_finite()) else {
        return MISS_STROKES;
    };
    if ms > 103.5 {
        MISS_STROKES
    } else if ms < 5.0 {
        0
    } else {
        let f = ms.floor();
        let points = if f <= 45.0 {
            1.25 * (f - 4.0) + 0.5 * (f - 23.0).max(0.0)
        } else {
            62.25 + 150.0 * (1.0 + (f - 45.0) / 60.0).ln()
        };
        // Lua math.floor(points * 0.4 + 0.5), rounded per judged row.
        (points * 0.4 + 0.5).floor() as u32
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
