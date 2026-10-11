// Frozen from main 16a06a2619a6cdcc34c647603d5dfa186e2f3e73; test visibility only.

pub(super) fn min_chart_bpm(bpms: &[(f32, f32)]) -> f64 {
    bpms.iter()
        .map(|&(_, bpm)| f64::from(bpm))
        .filter(|bpm| bpm.is_finite() && *bpm > 0.0)
        .fold(f64::INFINITY, f64::min)
        .min(f64::MAX)
}

pub(super) fn max_chart_bpm(bpms: &[(f32, f32)]) -> f64 {
    bpms.iter()
        .map(|&(_, bpm)| f64::from(bpm))
        .filter(|bpm| bpm.is_finite() && *bpm > 0.0)
        .fold(0.0_f64, f64::max)
}
