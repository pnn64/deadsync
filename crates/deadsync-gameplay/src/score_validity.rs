#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScoreValidityOptions {
    pub chart_effects: ChartAttackEffects,
    pub attack_mode: GameplayAttackMode,
    pub music_rate: f32,
}

impl Default for ScoreValidityOptions {
    fn default() -> Self {
        Self {
            chart_effects: ChartAttackEffects::default(),
            attack_mode: GameplayAttackMode::default(),
            music_rate: 1.0,
        }
    }
}

pub fn score_invalid_reason_lines_for_options(
    chart: &ChartData,
    options: ScoreValidityOptions,
) -> Vec<&'static str> {
    let mut reasons = Vec::with_capacity(6);
    let rate = normalized_song_rate(options.music_rate);
    if rate < 1.0 {
        reasons.push("music rate is below 1.0x");
    }

    let remove_mask = options.chart_effects.remove_mask;
    if (remove_mask & REMOVE_MASK_BIT_NO_HOLDS) != 0 && chart.stats.holds > 0 {
        reasons.push("No Holds is enabled on a chart with holds");
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_MINES) != 0 && chart.mines_nonfake > 0 {
        reasons.push("No Mines is enabled on a chart with mines");
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_JUMPS) != 0 && chart.stats.jumps > 0 {
        reasons.push("No Jumps is enabled on a chart with jumps");
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_HANDS) != 0 && chart.stats.hands > 0 {
        reasons.push("No Hands is enabled on a chart with hands");
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_QUADS) != 0 && chart.stats.hands > 0 {
        reasons.push("No Quads is enabled on a chart with quads");
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_LIFTS) != 0 && chart.stats.lifts > 0 {
        reasons.push("No Lifts is enabled on a chart with lifts");
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_FAKES) != 0 && chart.stats.fakes > 0 {
        reasons.push("No Fakes is enabled on a chart with fakes");
    }

    let holds_mask = options.chart_effects.holds_mask;
    if (holds_mask & HOLDS_MASK_BIT_NO_ROLLS) != 0 && chart.stats.rolls > 0 {
        reasons.push("No Rolls is enabled on a chart with rolls");
    }
    if (remove_mask & REMOVE_MASK_BIT_LITTLE) != 0 {
        reasons.push("Little is enabled");
    }

    let insert_mask = options.chart_effects.insert_mask;
    if (insert_mask & INSERT_MASK_BIT_ECHO) != 0 {
        reasons.push("Echo is enabled");
    }
    if (holds_mask & HOLDS_MASK_BIT_PLANTED) != 0 {
        reasons.push("Planted is enabled");
    }
    if (holds_mask & HOLDS_MASK_BIT_FLOORED) != 0 {
        reasons.push("Floored is enabled");
    }
    if (holds_mask & HOLDS_MASK_BIT_TWISTER) != 0 {
        reasons.push("Twister is enabled");
    }

    match options.attack_mode {
        GameplayAttackMode::Off => {
            if chart.has_chart_attacks {
                reasons.push("AttackMode=Off is enabled on a chart with attacks");
            }
        }
        GameplayAttackMode::On => {}
        GameplayAttackMode::Random => reasons.push("AttackMode=Random is enabled"),
    }

    reasons
}

pub fn score_invalid_reason_lines_for_chart<Profile: GameplayProfileData>(
    chart: &ChartData,
    profile: &Profile,
    _scroll_speed: ScrollSpeedSetting,
    music_rate: f32,
) -> Vec<&'static str> {
    score_invalid_reason_lines_for_options(
        chart,
        ScoreValidityOptions {
            chart_effects: profile.chart_effects(),
            attack_mode: profile.attack_mode(),
            music_rate,
        },
    )
}

