use log::debug;

use super::{
    CourseSubmitLife, HOT_LIFE_MIN_NEGATIVE_DELTA, MAX_PLAYERS, MAX_REGEN_COMBO_AFTER_MISS,
    PlayerRuntime, REGEN_COMBO_AFTER_MISS, State,
};

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
    p.course_submit_life = Some(CourseSubmitLife::new());
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
fn record_life(p: &mut PlayerRuntime, t: f32, life: f32) {
    const SHIFT: f32 = 0.003_906_25_f32; // 1/256, matches ITGmania's PlayerStageStats quirk

    let life = life.clamp(0.0_f32, 1.0_f32);
    let hist = &mut p.life_history;
    let Some(&(last_t, last_life)) = hist.last() else {
        hist.push((t, life));
        return;
    };

    if t > last_t {
        if (life - last_life).abs() > 0.000_001_f32 {
            hist.push((t, life));
        }
        return;
    }

    if (t - last_t).abs() <= 0.000_001_f32 {
        if (life - last_life).abs() <= 0.000_001_f32 {
            return;
        }
        let last_ix = hist.len() - 1;
        hist[last_ix].0 = t - SHIFT;
        hist.push((t, life));
    }
}

#[derive(Clone, Copy, Debug)]
struct LifeDeltaResult {
    old_life: f32,
    new_life: f32,
    failed_now: bool,
}

#[inline(always)]
fn apply_life_delta(
    life: &mut f32,
    combo_after_miss: &mut u32,
    is_failing: &mut bool,
    fail_time: &mut Option<f32>,
    current_music_time: f32,
    delta: f32,
) -> LifeDeltaResult {
    if *is_failing || *life <= 0.0 {
        let old_life = *life;
        *life = 0.0;
        *is_failing = true;
        return LifeDeltaResult {
            old_life,
            new_life: 0.0,
            failed_now: false,
        };
    }

    let old_life = *life;

    let mut final_delta = delta;
    if old_life >= 1.0 && final_delta < 0.0 {
        final_delta = final_delta.min(HOT_LIFE_MIN_NEGATIVE_DELTA);
    }
    if final_delta >= 0.0 {
        if *combo_after_miss > 0 {
            *combo_after_miss -= 1;
            if *combo_after_miss > 0 {
                final_delta = 0.0;
            }
        }
    } else if final_delta < 0.0 {
        let stacked_lock = combo_after_miss.saturating_add(REGEN_COMBO_AFTER_MISS);
        *combo_after_miss = (*combo_after_miss).max(stacked_lock.min(MAX_REGEN_COMBO_AFTER_MISS));
    }

    let mut new_life = (*life + final_delta).clamp(0.0, 1.0);
    let failed_now = new_life <= 0.0 && !*is_failing;

    if new_life <= 0.0 {
        if failed_now {
            *fail_time = Some(current_music_time);
        }
        new_life = 0.0;
        *is_failing = true;
    }

    *life = new_life;
    LifeDeltaResult {
        old_life,
        new_life,
        failed_now,
    }
}

#[inline(always)]
fn apply_course_submit_life_change(
    meter: &mut CourseSubmitLife,
    current_music_time: f32,
    delta: f32,
) {
    let _ = apply_life_delta(
        &mut meter.life,
        &mut meter.combo_after_miss,
        &mut meter.is_failing,
        &mut meter.fail_time,
        current_music_time,
        delta,
    );
}

pub(super) fn apply_life_change(p: &mut PlayerRuntime, current_music_time: f32, delta: f32) {
    if is_player_dead(p) {
        p.life = 0.0;
        p.is_failing = true;
        return;
    }

    let result = apply_life_delta(
        &mut p.life,
        &mut p.combo_after_miss,
        &mut p.is_failing,
        &mut p.fail_time,
        current_music_time,
        delta,
    );
    if result.failed_now {
        debug!("Player has failed!");
    }

    if (result.new_life - result.old_life).abs() > 0.000_001_f32 {
        record_life(p, current_music_time, result.old_life);
        record_life(p, current_music_time, result.new_life);
    }
    if let Some(meter) = &mut p.course_submit_life {
        apply_course_submit_life_change(meter, current_music_time, delta);
    }
}
