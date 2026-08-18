use crate::act;
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::color;
use deadsync_core::input::MAX_PLAYERS;

/// Color zones keyed by percentage of the player's maximum heart rate:
/// 0 grey (<50%), 1 white (50-59%), 2 blue (60-69%), 3 green (70-79%),
/// 4 yellow (80-89%), 5 red (90%+).
const ZONE_RGBA: [[f32; 4]; 6] = [
    color::rgba_hex("#9AA0A6"),
    color::rgba_hex("#FFFFFF"),
    color::rgba_hex("#4EA3FF"),
    color::rgba_hex("#5CE087"),
    color::rgba_hex("#FFD23F"),
    color::rgba_hex("#FF3030"),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeartRatePlayerView {
    pub configured: bool,
    pub connected: bool,
    pub bpm: Option<u16>,
    /// Player's configured maximum heart rate (bpm). Zones are computed as a
    /// percentage of this value. Zero falls back to the profile default.
    pub max_heart_rate: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeartRateView {
    pub players: [HeartRatePlayerView; MAX_PLAYERS],
}

/// Fixed two-player display text updated only when a monitor reading changes.
///
/// The gameplay thread owns this song-lifetime plan. Its two inline values use
/// no heap, cache, synchronization, growth, eviction, or destruction work.
/// Stable frames only clone one bounded `TextContent` value per visible player.
pub(crate) struct HeartRateTextPlan {
    bpm: [Option<u16>; MAX_PLAYERS],
    text: [TextContent; MAX_PLAYERS],
}

impl Default for HeartRateTextPlan {
    fn default() -> Self {
        Self {
            bpm: [None; MAX_PLAYERS],
            text: std::array::from_fn(|_| TextContent::Static("--")),
        }
    }
}

impl HeartRateTextPlan {
    pub(crate) fn sync(&mut self, view: HeartRateView) {
        for (player, reading) in view.players.into_iter().enumerate() {
            if self.bpm[player] != reading.bpm {
                self.bpm[player] = reading.bpm;
                self.text[player] = text(reading.bpm);
            }
        }
    }

    #[inline(always)]
    pub(crate) fn get(&self, player: usize) -> TextContent {
        self.text
            .get(player)
            .cloned()
            .unwrap_or(TextContent::Static("--"))
    }
}

#[inline(always)]
pub(crate) fn text(bpm: Option<u16>) -> TextContent {
    bpm.map_or(TextContent::Static("--"), TextContent::inline_u16)
}

pub(crate) fn pulse_scale(elapsed: f32, bpm: u16) -> f32 {
    if bpm == 0 || !elapsed.is_finite() {
        return 1.0;
    }
    let period = 60.0 / f32::from(bpm);
    let phase = elapsed.rem_euclid(period) / period;
    if phase < 0.12 {
        1.0 + 0.20 * (1.0 - phase / 0.12)
    } else if (0.18..0.30).contains(&phase) {
        1.0 + 0.09 * (1.0 - (phase - 0.18) / 0.12)
    } else {
        1.0
    }
}

fn zone_color(bpm: u16, max_heart_rate: u16) -> [f32; 4] {
    let max = u32::from(if max_heart_rate == 0 {
        deadsync_profile::MAX_HEART_RATE_DEFAULT
    } else {
        max_heart_rate
    });
    // Compare equivalent scaled integer ratios so this per-frame path avoids
    // integer division. For example, floor(bpm * 100 / max) < 50 exactly when
    // bpm * 10 < max * 5.
    let scaled_bpm = u32::from(bpm) * 10;
    ZONE_RGBA[match scaled_bpm {
        value if value < max * 5 => 0, // grey (<50%)
        value if value < max * 6 => 1, // white (50-59%)
        value if value < max * 7 => 2, // blue (60-69%)
        value if value < max * 8 => 3, // green (70-79%)
        value if value < max * 9 => 4, // yellow (80-89%)
        _ => 5,                        // red (90%+)
    }]
}

pub fn push(
    actors: &mut Vec<Actor>,
    reading: HeartRatePlayerView,
    display_text: TextContent,
    elapsed: f32,
    x: f32,
    y: f32,
    zoom: f32,
    z: i16,
) {
    let alpha = if reading.connected { 1.0 } else { 0.45 };
    let bpm = reading.bpm.unwrap_or(0);
    let pulse = pulse_scale(elapsed, bpm);
    let heart_width = 24.0 * zoom * pulse;
    let heart_height = 20.4 * zoom * pulse;
    let zone_rgba = reading
        .bpm
        .map(|bpm| zone_color(bpm, reading.max_heart_rate))
        .unwrap_or(color::JUDGMENT_FA_PLUS_WHITE_RGBA);
    let heart_rgba = zone_rgba;
    let text_rgba = zone_rgba;
    actors.push(act!(sprite("heart.png"):
        align(0.5, 0.5): xy(x, y): zoomto(heart_width, heart_height):
        diffuse(heart_rgba[0], heart_rgba[1], heart_rgba[2], alpha): z(z)
    ));
    actors.push(act!(text:
        font("miso"): settext(display_text): align(0.0, 0.5): horizalign(left):
        xy(x + 16.0 * zoom, y): zoom(2.0 * zoom):
        diffuse(text_rgba[0], text_rgba[1], text_rgba[2], alpha): z(z)
    ));
}

#[cfg(test)]
mod tests {
    use super::{
        HeartRatePlayerView, HeartRateTextPlan, HeartRateView, ZONE_RGBA, pulse_scale, text,
        zone_color,
    };

    #[test]
    fn heart_pulse_repeats_at_the_reported_rate() {
        let period = 60.0 / 120.0;
        assert!((pulse_scale(0.0, 120) - pulse_scale(period, 120)).abs() < 0.0001);
        assert!(pulse_scale(0.0, 120) > pulse_scale(0.10, 120));
    }

    #[test]
    fn missing_rate_keeps_the_heart_still() {
        assert_eq!(pulse_scale(10.0, 0), 1.0);
        assert_eq!(pulse_scale(f32::NAN, 120), 1.0);
    }

    #[test]
    fn colors_scale_with_max_heart_rate() {
        // Zones are percentages of the player's max HR (here 200 bpm).
        assert_eq!(zone_color(80, 200), ZONE_RGBA[0]); // 40% -> grey
        assert_eq!(zone_color(100, 200), ZONE_RGBA[1]); // 50% -> white
        assert_eq!(zone_color(120, 200), ZONE_RGBA[2]); // 60% -> blue
        assert_eq!(zone_color(140, 200), ZONE_RGBA[3]); // 70% -> green
        assert_eq!(zone_color(160, 200), ZONE_RGBA[4]); // 80% -> yellow
        assert_eq!(zone_color(180, 200), ZONE_RGBA[5]); // 90% -> red
        assert_eq!(zone_color(200, 200), ZONE_RGBA[5]); // 100% -> red
        // A lower max shifts the same bpm into a higher zone.
        assert_eq!(zone_color(120, 160), ZONE_RGBA[3]); // 75% -> green
        // Zero max falls back to the default so we never divide by zero.
        assert_eq!(zone_color(95, 0), zone_color(95, 190));
    }

    #[test]
    fn scaled_zone_comparisons_match_integer_percentage_behavior() {
        fn reference_zone_color(bpm: u16, max_heart_rate: u16) -> [f32; 4] {
            let max = if max_heart_rate == 0 {
                deadsync_profile::MAX_HEART_RATE_DEFAULT
            } else {
                max_heart_rate
            };
            let percent = u32::from(bpm) * 100 / u32::from(max);
            ZONE_RGBA[match percent {
                0..=49 => 0,
                50..=59 => 1,
                60..=69 => 2,
                70..=79 => 3,
                80..=89 => 4,
                _ => 5,
            }]
        }

        for max in [0, 1, 159, 160, 161, 190, 219, 220, 221, u16::MAX] {
            for bpm in 0..=u16::MAX {
                assert_eq!(
                    zone_color(bpm, max),
                    reference_zone_color(bpm, max),
                    "bpm={bpm}, max={max}"
                );
            }
        }
    }

    #[test]
    fn heart_rate_text_plan_preserves_missing_and_numeric_readings() {
        let mut plan = HeartRateTextPlan::default();
        assert_eq!(plan.get(0).as_str(), "--");
        plan.sync(HeartRateView {
            players: [
                HeartRatePlayerView {
                    configured: true,
                    connected: true,
                    bpm: Some(147),
                    max_heart_rate: 190,
                },
                HeartRatePlayerView {
                    configured: true,
                    connected: true,
                    bpm: Some(u16::MAX),
                    max_heart_rate: 190,
                },
            ],
        });
        assert_eq!(plan.get(0).as_str(), text(Some(147)).as_str());
        assert_eq!(plan.get(1).as_str(), "65535");
    }
}
