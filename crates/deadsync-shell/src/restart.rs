use deadsync_chart::SongData;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_profile::{PlayStyle, PlayerSide, player_side_index};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_screens::ScoreInfo;
use std::sync::Arc;

pub struct RestartPayload {
    pub song: Arc<SongData>,
    pub chart_hashes: [String; MAX_PLAYERS],
    pub music_rate: f32,
    pub scroll_speed: [ScrollSpeedSetting; MAX_PLAYERS],
}

pub fn restart_payload_from_eval(
    score_info: &[Option<ScoreInfo>; MAX_PLAYERS],
) -> Option<RestartPayload> {
    let mut song = None;
    let mut chart_hashes = std::array::from_fn(|_| String::new());
    let mut scroll_speed = [ScrollSpeedSetting::default(); MAX_PLAYERS];
    let mut music_rate = None;

    for entry in score_info.iter().flatten() {
        song.get_or_insert_with(|| entry.song.clone());
        let side = player_side_index(entry.side);
        chart_hashes[side] = entry.chart.short_hash.clone();
        scroll_speed[side] = entry.speed_mod;
        if music_rate.is_none() && entry.music_rate.is_finite() && entry.music_rate > 0.0 {
            music_rate = Some(entry.music_rate);
        }
    }

    song.map(|song| RestartPayload {
        song,
        chart_hashes,
        music_rate: music_rate.unwrap_or(1.0),
        scroll_speed,
    })
}

#[inline(always)]
fn select_restart_steps(
    resolved_steps: [usize; MAX_PLAYERS],
    play_style: PlayStyle,
    player_side: PlayerSide,
) -> [usize; MAX_PLAYERS] {
    match play_style {
        PlayStyle::Versus => resolved_steps,
        PlayStyle::Single | PlayStyle::Double => {
            [resolved_steps[player_side_index(player_side)]; MAX_PLAYERS]
        }
    }
}

pub fn restart_chart_steps(
    song: &SongData,
    chart_hashes: [&str; MAX_PLAYERS],
    chart_type: &str,
    fallback_steps: usize,
    play_style: PlayStyle,
    player_side: PlayerSide,
) -> [usize; MAX_PLAYERS] {
    let resolved_steps = std::array::from_fn(|idx| {
        song.steps_index_for_chart_hash(chart_type, chart_hashes[idx])
            .unwrap_or(fallback_steps)
    });
    select_restart_steps(resolved_steps, play_style, player_side)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_steps_preserve_versus_and_collapse_single_side() {
        assert_eq!(
            select_restart_steps([2, 4], PlayStyle::Versus, PlayerSide::P1),
            [2, 4],
        );
        assert_eq!(
            select_restart_steps([2, 4], PlayStyle::Single, PlayerSide::P2),
            [4, 4],
        );
        assert_eq!(
            select_restart_steps([2, 4], PlayStyle::Double, PlayerSide::P1),
            [2, 2],
        );
    }
}
