use crate::act;
use deadlib_present::actors::Actor;
use deadlib_present::cache::{TextCache, cached_text, text_cache_with_capacity};
use deadlib_present::color;
use deadsync_core::input::MAX_PLAYERS;
use std::cell::RefCell;
use std::sync::{Arc, LazyLock};

const ZONE_RGBA: [[f32; 4]; 5] = [
    color::rgba_hex("#5CE087"),
    color::rgba_hex("#FFFF00"),
    color::rgba_hex("#FF9F1C"),
    color::rgba_hex("#FF6B6B"),
    color::rgba_hex("#FF3030"),
];

thread_local! {
    static TEXT_CACHE: RefCell<TextCache<u16>> = RefCell::new(text_cache_with_capacity(512));
}

static UNKNOWN_TEXT: LazyLock<Arc<str>> = LazyLock::new(|| Arc::<str>::from("--"));

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeartRatePlayerView {
    pub configured: bool,
    pub connected: bool,
    pub bpm: Option<u16>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeartRateView {
    pub players: [HeartRatePlayerView; MAX_PLAYERS],
}

fn cached_text_for_bpm(bpm: u16) -> Arc<str> {
    // Owner: render thread. Lifetime: session. Capacity: 512 readings. Misses
    // format one small integer; the cache saturates without pruning.
    cached_text(&TEXT_CACHE, bpm, 512, || bpm.to_string())
}

pub(crate) fn text(bpm: Option<u16>) -> Arc<str> {
    bpm.map(cached_text_for_bpm)
        .unwrap_or_else(|| Arc::clone(&UNKNOWN_TEXT))
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

fn zone_color(bpm: u16) -> [f32; 4] {
    ZONE_RGBA[match bpm {
        ..=119 => 0,
        120..=139 => 1,
        140..=159 => 2,
        160..=179 => 3,
        180.. => 4,
    }]
}

pub fn push(
    actors: &mut Vec<Actor>,
    reading: HeartRatePlayerView,
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
    let heart_rgba = ZONE_RGBA[4];
    let text_rgba = reading
        .bpm
        .map(zone_color)
        .unwrap_or(color::JUDGMENT_FA_PLUS_WHITE_RGBA);
    actors.push(act!(sprite("heart.png"):
        align(0.5, 0.5): xy(x, y): zoomto(heart_width, heart_height):
        diffuse(heart_rgba[0], heart_rgba[1], heart_rgba[2], alpha): z(z)
    ));
    actors.push(act!(text:
        font("miso"): settext(text(reading.bpm)): align(0.0, 0.5): horizalign(left):
        xy(x + 16.0 * zoom, y): zoom(2.0 * zoom):
        diffuse(text_rgba[0], text_rgba[1], text_rgba[2], alpha): z(z)
    ));
}

#[cfg(test)]
mod tests {
    use super::{ZONE_RGBA, pulse_scale, zone_color};

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
    fn colors_cover_twenty_bpm_zones() {
        assert_eq!(zone_color(100), ZONE_RGBA[0]);
        assert_eq!(zone_color(120), ZONE_RGBA[1]);
        assert_eq!(zone_color(140), ZONE_RGBA[2]);
        assert_eq!(zone_color(160), ZONE_RGBA[3]);
        assert_eq!(zone_color(180), ZONE_RGBA[4]);
        assert_eq!(zone_color(200), ZONE_RGBA[4]);
    }
}
