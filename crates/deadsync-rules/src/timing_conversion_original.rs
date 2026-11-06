//! Frozen production conversions from 9b94bc272 for differential checks.
use super::TimingProfile;
use super::TimingProfileNs;

pub fn delta_seconds(lhs: i64, rhs: i64) -> f32 {
    ((i128::from(lhs) - i128::from(rhs)) as f64 * 1.0e-9) as f32
}

pub fn judgment_ns(ms: f32, rate: f32) -> i64 {
    let rate = if rate.is_finite() && rate > 0.0 {
        rate
    } else {
        1.0
    };
    (f64::from(ms) * f64::from(rate) * 1_000_000.0)
        .round()
        .clamp((i64::MIN + 1) as f64, i64::MAX as f64) as i64
}

pub fn profile_ns(profile: &TimingProfile, rate: f32) -> TimingProfileNs {
    fn scale(seconds: f32, rate: f32) -> i64 {
        let rate = if rate.is_finite() && rate > 0.0 {
            rate
        } else {
            1.0
        };
        let nanos = f64::from(seconds) * f64::from(rate) * 1_000_000_000.0;
        nanos.round().clamp((i64::MIN + 1) as f64, i64::MAX as f64) as i64
    }
    TimingProfileNs {
        windows_ns: profile.windows_s.map(|s| scale(s, rate)),
        fa_plus_window_ns: profile.fa_plus_window_s.map(|s| scale(s, rate)),
        mine_window_ns: scale(profile.mine_window_s, rate),
    }
}
