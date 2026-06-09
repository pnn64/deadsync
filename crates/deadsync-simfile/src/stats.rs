use deadsync_chart::StaminaCounts;
use rssp::patterns::{PatternCounts, PatternVariant, compute_box_counts, count_pattern};

pub fn build_stamina_counts(chart: &rssp::report::ChartSummary) -> StaminaCounts {
    build_stamina_counts_from_parts(
        &chart.detected_patterns,
        [
            chart.anchor_left,
            chart.anchor_down,
            chart.anchor_up,
            chart.anchor_right,
        ],
        chart.candle_total,
        chart.candle_percent,
        chart.mono_total,
        chart.mono_percent,
    )
}

fn build_stamina_counts_from_parts(
    patterns: &PatternCounts,
    anchors: [u32; 4],
    candle_total: u32,
    candle_percent: f64,
    mono_total: u32,
    mono_percent: f64,
) -> StaminaCounts {
    StaminaCounts {
        anchors: anchors.into_iter().sum(),
        triangles: sum_patterns(
            patterns,
            &[
                PatternVariant::TriangleLDL,
                PatternVariant::TriangleLUL,
                PatternVariant::TriangleRDR,
                PatternVariant::TriangleRUR,
            ],
        ),
        boxes: compute_box_counts(patterns).total_boxes,
        towers: sum_patterns(
            patterns,
            &[
                PatternVariant::TowerLR,
                PatternVariant::TowerUD,
                PatternVariant::TowerCornerLD,
                PatternVariant::TowerCornerLU,
                PatternVariant::TowerCornerRD,
                PatternVariant::TowerCornerRU,
            ],
        ),
        doritos: sum_patterns(
            patterns,
            &[
                PatternVariant::DoritoLeft,
                PatternVariant::DoritoRight,
                PatternVariant::DoritoInvLeft,
                PatternVariant::DoritoInvRight,
            ],
        ),
        hip_breakers: sum_patterns(
            patterns,
            &[
                PatternVariant::HipBreakerLeft,
                PatternVariant::HipBreakerRight,
                PatternVariant::HipBreakerInvLeft,
                PatternVariant::HipBreakerInvRight,
            ],
        ),
        copters: sum_patterns(
            patterns,
            &[
                PatternVariant::CopterLeft,
                PatternVariant::CopterRight,
                PatternVariant::CopterInvLeft,
                PatternVariant::CopterInvRight,
            ],
        ),
        spirals: sum_patterns(
            patterns,
            &[
                PatternVariant::SpiralLeft,
                PatternVariant::SpiralRight,
                PatternVariant::SpiralInvLeft,
                PatternVariant::SpiralInvRight,
            ],
        ),
        candles: candle_total,
        candle_percent,
        staircases: sum_patterns(
            patterns,
            &[
                PatternVariant::StaircaseLeft,
                PatternVariant::StaircaseRight,
                PatternVariant::StaircaseInvLeft,
                PatternVariant::StaircaseInvRight,
            ],
        ),
        mono: mono_total,
        mono_percent,
        sweeps: sum_patterns(
            patterns,
            &[
                PatternVariant::SweepLeft,
                PatternVariant::SweepRight,
                PatternVariant::SweepInvLeft,
                PatternVariant::SweepInvRight,
            ],
        ),
    }
}

fn sum_patterns(patterns: &PatternCounts, variants: &[PatternVariant]) -> u32 {
    variants
        .iter()
        .copied()
        .map(|variant| count_pattern(patterns, variant))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rssp::patterns::PATTERN_COUNT;

    #[test]
    fn stamina_counts_sum_pattern_groups_and_direct_fields() {
        let mut patterns = [0u32; PATTERN_COUNT];
        patterns[PatternVariant::BoxLR as usize] = 2;
        patterns[PatternVariant::BoxCornerLU as usize] = 3;
        patterns[PatternVariant::TowerLR as usize] = 4;
        patterns[PatternVariant::TowerCornerRU as usize] = 5;
        patterns[PatternVariant::TriangleLDL as usize] = 6;
        patterns[PatternVariant::DoritoRight as usize] = 7;
        patterns[PatternVariant::HipBreakerInvLeft as usize] = 8;
        patterns[PatternVariant::CopterLeft as usize] = 9;
        patterns[PatternVariant::SpiralInvRight as usize] = 10;
        patterns[PatternVariant::StaircaseRight as usize] = 11;
        patterns[PatternVariant::SweepInvLeft as usize] = 12;

        let counts = build_stamina_counts_from_parts(&patterns, [1, 2, 3, 4], 13, 0.25, 14, 0.5);

        assert_eq!(counts.anchors, 10);
        assert_eq!(counts.boxes, 5);
        assert_eq!(counts.towers, 9);
        assert_eq!(counts.triangles, 6);
        assert_eq!(counts.doritos, 7);
        assert_eq!(counts.hip_breakers, 8);
        assert_eq!(counts.copters, 9);
        assert_eq!(counts.spirals, 10);
        assert_eq!(counts.staircases, 11);
        assert_eq!(counts.sweeps, 12);
        assert_eq!(counts.candles, 13);
        assert_eq!(counts.candle_percent, 0.25);
        assert_eq!(counts.mono, 14);
        assert_eq!(counts.mono_percent, 0.5);
    }
}
