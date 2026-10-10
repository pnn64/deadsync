// Frozen from main 4e59125883b7cf5331779a0a95627fea47930fd9 for paired regression checks and benchmarks.
pub(super) fn sync_heat_value_range(
    values: &[f64],
    clim_pct: Option<(f64, f64)>,
) -> Option<(f64, f64)> {
    if values.is_empty() {
        return None;
    }
    if let Some((lo_pct, hi_pct)) = clim_pct {
        let (lo, hi) = sync_percentile_pair(values, lo_pct, hi_pct);
        if hi > lo {
            return Some((lo, hi));
        }
    }
    let lo = values.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if !lo.is_finite() || !hi.is_finite() {
        None
    } else if hi > lo {
        Some((lo, hi))
    } else {
        Some((lo - 1.0, hi + 1.0))
    }
}
