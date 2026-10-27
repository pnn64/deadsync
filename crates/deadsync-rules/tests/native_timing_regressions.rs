use deadsync_rules::timing::{
    TimingProfile, TimingProfileNs, classify_offset_ns_with_disabled_windows,
};

#[test]
fn classification_matches_wide_reference_at_every_window_boundary() {
    use deadsync_rules::judgment::{JudgeGrade::*, TimingWindow::*};
    let reference = |offset: i64, profile: &TimingProfileNs, disabled: &[bool; 5]| {
        let abs = i128::from(offset).abs();
        [
            (disabled[0], profile.fa_plus_window_ns, Fantastic, W0),
            (disabled[0], Some(profile.windows_ns[0]), Fantastic, W1),
            (disabled[1], Some(profile.windows_ns[1]), Excellent, W2),
            (disabled[2], Some(profile.windows_ns[2]), Great, W3),
            (disabled[3], Some(profile.windows_ns[3]), Decent, W4),
            (disabled[4], Some(profile.windows_ns[4]), WayOff, W5),
        ]
        .into_iter()
        .find_map(|(disabled, limit, grade, window)| {
            (!disabled && limit.is_some_and(|limit| abs <= i128::from(limit)))
                .then_some((grade, window))
        })
    };
    let default =
        TimingProfileNs::from_profile_scaled(&TimingProfile::default_itg_with_fa_plus(), 1.0);
    for windows in [
        default.windows_ns,
        [0; 5],
        [i64::MIN, -1, 0, 1, i64::MAX],
        [i64::MAX; 5],
        [50, 10, 30, -1, 20],
    ] {
        for fa_plus_window_ns in [
            None,
            Some(0),
            Some(-1),
            Some(15_000_000),
            Some(i64::MAX),
            Some(i64::MIN),
        ] {
            let profile = TimingProfileNs {
                windows_ns: windows,
                fa_plus_window_ns,
                mine_window_ns: 0,
            };
            let mut offsets = vec![i64::MIN, i64::MIN + 1, i64::MAX, -1, 0, 1];
            for limit in windows.into_iter().chain(fa_plus_window_ns) {
                for adjustment in [-1, 0, 1] {
                    let offset = limit.saturating_add(adjustment);
                    offsets.push(offset);
                    offsets.push(offset.saturating_neg());
                }
            }
            for mask in 0..32 {
                let disabled = std::array::from_fn(|i| mask & (1 << i) != 0);
                for &offset in &offsets {
                    assert_eq!(
                        classify_offset_ns_with_disabled_windows(offset, &profile, &disabled),
                        reference(offset, &profile, &disabled),
                        "offset={offset}, profile={profile:?}, disabled={disabled:?}"
                    );
                }
            }
        }
    }
}
