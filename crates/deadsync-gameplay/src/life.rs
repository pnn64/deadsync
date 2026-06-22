#[inline(always)]
pub const fn player_life_is_dead(life: f32, is_failing: bool) -> bool {
    is_failing || life <= 0.0
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerLifeStatus {
    pub life: f32,
    pub is_failing: bool,
}

pub fn all_joined_players_failed(
    players: &[PlayerLifeStatus; MAX_PLAYERS],
    num_players: usize,
) -> bool {
    let active_players = num_players.min(MAX_PLAYERS);
    active_players > 0
        && players
            .iter()
            .take(active_players)
            .all(|player| player_life_is_dead(player.life, player.is_failing))
}

#[inline(always)]
pub const fn player_life_status(player: &PlayerRuntime) -> PlayerLifeStatus {
    PlayerLifeStatus {
        life: player.life,
        is_failing: player.is_failing,
    }
}

pub fn all_joined_player_runtimes_failed(
    players: &[PlayerRuntime; MAX_PLAYERS],
    num_players: usize,
) -> bool {
    let statuses = std::array::from_fn(|player| player_life_status(&players[player]));
    all_joined_players_failed(&statuses, num_players)
}

#[inline(always)]
pub fn course_submit_life_eligible(life: Option<&deadsync_rules::life::LifeMeter>) -> bool {
    life.is_none_or(|life| !life.is_failing && life.fail_time.is_none() && life.life > 0.0)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GameplayLifeDeltaUpdate {
    pub failed_now: bool,
    pub was_dead: bool,
}

pub fn apply_gameplay_life_delta(
    meter: &mut deadsync_rules::life::LifeMeter,
    life_history: &mut Vec<(f32, f32)>,
    course_submit_life: Option<&mut deadsync_rules::life::LifeMeter>,
    current_music_time: f32,
    delta: f32,
) -> GameplayLifeDeltaUpdate {
    if player_life_is_dead(meter.life, meter.is_failing) {
        meter.life = 0.0;
        meter.is_failing = true;
        return GameplayLifeDeltaUpdate {
            failed_now: false,
            was_dead: true,
        };
    }

    let result = deadsync_rules::life::apply_life_delta(meter, current_music_time, delta);
    if (result.new_life - result.old_life).abs() > 0.000_001_f32 {
        deadsync_rules::life::record_life_history(
            life_history,
            current_music_time,
            result.old_life,
        );
        deadsync_rules::life::record_life_history(
            life_history,
            current_music_time,
            result.new_life,
        );
    }
    if let Some(meter) = course_submit_life {
        let _ = deadsync_rules::life::apply_life_delta(meter, current_music_time, delta);
    }

    GameplayLifeDeltaUpdate {
        failed_now: result.failed_now,
        was_dead: false,
    }
}

#[inline(always)]
fn player_runtime_life_meter(player: &PlayerRuntime) -> deadsync_rules::life::LifeMeter {
    deadsync_rules::life::LifeMeter {
        life: player.life,
        combo_after_miss: player.combo_after_miss,
        is_failing: player.is_failing,
        fail_time: player.fail_time,
    }
}

#[inline(always)]
fn write_player_runtime_life_meter(
    player: &mut PlayerRuntime,
    meter: deadsync_rules::life::LifeMeter,
) {
    player.life = meter.life;
    player.combo_after_miss = meter.combo_after_miss;
    player.is_failing = meter.is_failing;
    player.fail_time = meter.fail_time;
}

pub fn apply_player_runtime_life_delta(
    player: &mut PlayerRuntime,
    current_music_time: f32,
    delta: f32,
) -> GameplayLifeDeltaUpdate {
    let mut meter = player_runtime_life_meter(player);
    let update = apply_gameplay_life_delta(
        &mut meter,
        &mut player.life_history,
        player.course_submit_life.as_mut(),
        current_music_time,
        delta,
    );
    write_player_runtime_life_meter(player, meter);
    update
}

#[inline(always)]
pub fn apply_life_change(player: &mut PlayerRuntime, current_music_time: f32, delta: f32) {
    let result = apply_player_runtime_life_delta(player, current_music_time, delta);
    if result.failed_now {
        log::debug!("Player has failed!");
    }
}

#[inline(always)]
pub fn player_runtime_is_dead(player: &PlayerRuntime) -> bool {
    player_life_is_dead(player.life, player.is_failing)
}

