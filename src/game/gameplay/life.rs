use log::debug;

use deadsync_rules::life::{self as life_rules, LifeMeter};

use super::{MAX_PLAYERS, PlayerRuntime, State};

#[inline(always)]
pub(super) fn is_player_dead(p: &PlayerRuntime) -> bool {
    p.is_failing || p.life <= 0.0
}

#[inline(always)]
pub(super) fn is_state_dead(state: &State, player: usize) -> bool {
    is_player_dead(&state.players[player])
}

#[inline(always)]
pub(super) fn all_joined_players_failed(state: &State) -> bool {
    if state.num_players == 0 {
        return false;
    }
    for player in 0..state.num_players {
        if !is_state_dead(state, player) {
            return false;
        }
    }
    true
}

#[inline(always)]
pub(super) fn init_course_submit_life(p: &mut PlayerRuntime) {
    p.course_submit_life = Some(LifeMeter::course_submit_start());
}

#[inline(always)]
pub fn course_stage_life_submit_eligible(state: &State, player_idx: usize) -> bool {
    if player_idx >= state.num_players.min(MAX_PLAYERS) {
        return true;
    }
    state.players[player_idx]
        .course_submit_life
        .as_ref()
        .map_or(true, |life| {
            !life.is_failing && life.fail_time.is_none() && life.life > 0.0
        })
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

#[inline(always)]
fn apply_course_submit_life_change(meter: &mut LifeMeter, current_music_time: f32, delta: f32) {
    let _ = life_rules::apply_life_delta(meter, current_music_time, delta);
}

pub(super) fn apply_life_change(p: &mut PlayerRuntime, current_music_time: f32, delta: f32) {
    if is_player_dead(p) {
        p.life = 0.0;
        p.is_failing = true;
        return;
    }

    let mut meter = player_life_meter(p);
    let result = life_rules::apply_life_delta(&mut meter, current_music_time, delta);
    write_player_life_meter(p, meter);
    if result.failed_now {
        debug!("Player has failed!");
    }

    if (result.new_life - result.old_life).abs() > 0.000_001_f32 {
        life_rules::record_life_history(&mut p.life_history, current_music_time, result.old_life);
        life_rules::record_life_history(&mut p.life_history, current_music_time, result.new_life);
    }
    if let Some(meter) = &mut p.course_submit_life {
        apply_course_submit_life_change(meter, current_music_time, delta);
    }
}
