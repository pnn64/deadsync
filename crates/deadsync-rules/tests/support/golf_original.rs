// Frozen from 241a16adc for differential checks and paired benchmarks.
#![allow(dead_code)]
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
