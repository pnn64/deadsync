use log::debug;

use deadsync_rules::life::LifeMeter;

use super::{
    MAX_PLAYERS, PlayerLifeStatus, PlayerRuntime, State, all_joined_players_failed_for_statuses,
    apply_gameplay_life_delta, course_submit_life_eligible, player_life_is_dead,
};

#[inline(always)]
pub(super) fn is_player_dead(p: &PlayerRuntime) -> bool {
    player_life_is_dead(p.life, p.is_failing)
}

#[inline(always)]
pub(super) fn is_state_dead(state: &State, player: usize) -> bool {
    is_player_dead(&state.players[player])
}

#[inline(always)]
pub(super) fn all_joined_players_failed(state: &State) -> bool {
    let players = std::array::from_fn(|player| PlayerLifeStatus {
        life: state.players[player].life,
        is_failing: state.players[player].is_failing,
    });
    all_joined_players_failed_for_statuses(&players, state.num_players)
}

#[inline(always)]
pub fn course_stage_life_submit_eligible(state: &State, player_idx: usize) -> bool {
    if player_idx >= state.num_players.min(MAX_PLAYERS) {
        return true;
    }
    course_submit_life_eligible(state.players[player_idx].course_submit_life.as_ref())
}

#[inline(always)]
fn player_life_meter(p: &PlayerRuntime) -> LifeMeter {
    LifeMeter {
        life: p.life,
        combo_after_miss: p.combo_after_miss,
        is_failing: p.is_failing,
        fail_time: p.fail_time,
    }
}

#[inline(always)]
fn write_player_life_meter(p: &mut PlayerRuntime, meter: LifeMeter) {
    p.life = meter.life;
    p.combo_after_miss = meter.combo_after_miss;
    p.is_failing = meter.is_failing;
    p.fail_time = meter.fail_time;
}

pub(super) fn apply_life_change(p: &mut PlayerRuntime, current_music_time: f32, delta: f32) {
    let mut meter = player_life_meter(p);
    let result = apply_gameplay_life_delta(
        &mut meter,
        &mut p.life_history,
        p.course_submit_life.as_mut(),
        current_music_time,
        delta,
    );
    write_player_life_meter(p, meter);
    if result.failed_now {
        debug!("Player has failed!");
    }
}
