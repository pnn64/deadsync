pub const SONG_LUA_TIMING_WINDOW_NAMES: [&str; 5] = [
    "TimingWindow_W1",
    "TimingWindow_W2",
    "TimingWindow_W3",
    "TimingWindow_W4",
    "TimingWindow_W5",
];

#[must_use]
pub fn timing_window_seconds(index: i32, mode: &str, tenms: bool) -> f32 {
    if mode.eq_ignore_ascii_case("FA+") && tenms && index == 1 {
        return 0.0085 + 0.0015;
    }
    let windows = if mode.eq_ignore_ascii_case("FA+") {
        [0.0135, 0.0215, 0.043, 0.135, 0.18]
    } else {
        [0.0215, 0.043, 0.102, 0.135, 0.18]
    };
    let index = index.clamp(1, windows.len() as i32) as usize - 1;
    windows[index] + 0.0015
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timing_window_seconds_matches_itg_and_fa_plus() {
        assert!((timing_window_seconds(1, "", false) - 0.023).abs() <= 1e-6);
        assert!((timing_window_seconds(2, "", false) - 0.0445).abs() <= 1e-6);
        assert!((timing_window_seconds(1, "FA+", false) - 0.015).abs() <= 1e-6);
        assert!((timing_window_seconds(1, "FA+", true) - 0.010).abs() <= 1e-6);
        assert!((timing_window_seconds(99, "", false) - 0.1815).abs() <= 1e-6);
    }
}
