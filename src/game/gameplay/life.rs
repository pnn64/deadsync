use log::debug;

use super::{
    HOT_LIFE_MIN_NEGATIVE_DELTA, MAX_REGEN_COMBO_AFTER_MISS, PlayerRuntime, REGEN_COMBO_AFTER_MISS,
    State,
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

pub(super) fn apply_life_change(p: &mut PlayerRuntime, current_music_time: f32, delta: f32) {
    if is_player_dead(p) {
        p.life = 0.0;
        p.is_failing = true;
        return;
    }

    let old_life = p.life;

    let mut final_delta = delta;
    if old_life >= 1.0 && final_delta < 0.0 {
        final_delta = final_delta.min(HOT_LIFE_MIN_NEGATIVE_DELTA);
    }
    if final_delta >= 0.0 {
        if p.combo_after_miss > 0 {
            p.combo_after_miss -= 1;
            if p.combo_after_miss > 0 {
                final_delta = 0.0;
            }
        }
    } else if final_delta < 0.0 {
        let stacked_lock = p.combo_after_miss.saturating_add(REGEN_COMBO_AFTER_MISS);
        p.combo_after_miss = p
            .combo_after_miss
            .max(stacked_lock.min(MAX_REGEN_COMBO_AFTER_MISS));
    }

    let mut new_life = (p.life + final_delta).clamp(0.0, 1.0);

    if new_life <= 0.0 {
        if !p.is_failing {
            p.fail_time = Some(current_music_time);
        }
        new_life = 0.0;
        p.is_failing = true;
        debug!("Player has failed!");
    }

    if (new_life - old_life).abs() > 0.000_001_f32 {
        record_life(p, current_music_time, old_life);
        record_life(p, current_music_time, new_life);
    }
    p.life = new_life;
}
