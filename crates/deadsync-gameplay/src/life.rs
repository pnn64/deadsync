#[inline(always)]
pub const fn player_life_is_dead(life: f32, is_failing: bool) -> bool {
    is_failing || life <= 0.0
}

const SURVIVAL_FULL_LIFE_SECONDS: f32 = 90.0;
const SURVIVAL_MIN_GAIN_SECONDS: f32 = 15.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum CourseLifeConfig {
    #[default]
    Bar,
    Battery {
        total_lives: u32,
        reward_lives: u32,
    },
    Survival {
        gain_seconds: f32,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum CourseLifeState {
    #[default]
    Bar,
    Battery {
        lives: u32,
        total_lives: u32,
        reward_lives: u32,
    },
    Survival {
        remaining_seconds: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CourseLifeEvent {
    Tap(JudgeGrade),
    Mine,
    CheckpointHit,
    CheckpointMiss,
    HoldHeld,
    HoldLetGo,
    ForceFail,
}

#[inline(always)]
fn survival_life(seconds: f32) -> f32 {
    (seconds / SURVIVAL_FULL_LIFE_SECONDS).clamp(0.0, 1.0)
}

pub fn init_course_life(
    config: CourseLifeConfig,
    carry: Option<CourseLifeState>,
) -> (CourseLifeState, f32) {
    match config {
        CourseLifeConfig::Bar => (CourseLifeState::Bar, 0.5),
        CourseLifeConfig::Battery {
            total_lives,
            reward_lives,
        } => {
            let total_lives = total_lives.max(1);
            let lives = match carry {
                Some(CourseLifeState::Battery { lives, .. }) => lives.min(total_lives),
                _ => total_lives,
            };
            (
                CourseLifeState::Battery {
                    lives,
                    total_lives,
                    reward_lives,
                },
                lives as f32 / total_lives as f32,
            )
        }
        CourseLifeConfig::Survival { gain_seconds } => {
            let carried_seconds = match carry {
                Some(CourseLifeState::Survival {
                    remaining_seconds, ..
                }) => remaining_seconds.max(0.0),
                _ => 0.0,
            };
            let start_seconds = carried_seconds
                + if gain_seconds.is_finite() {
                    gain_seconds.max(SURVIVAL_MIN_GAIN_SECONDS)
                } else {
                    SURVIVAL_MIN_GAIN_SECONDS
                };
            (
                CourseLifeState::Survival {
                    remaining_seconds: start_seconds,
                },
                survival_life(start_seconds),
            )
        }
    }
}

pub fn course_life_carry(state: CourseLifeState) -> CourseLifeState {
    match state {
        CourseLifeState::Battery {
            lives,
            total_lives,
            reward_lives,
        } => CourseLifeState::Battery {
            lives: if lives == 0 {
                0
            } else {
                lives.saturating_add(reward_lives).min(total_lives)
            },
            total_lives,
            reward_lives: 0,
        },
        state => state,
    }
}

#[inline(always)]
const fn battery_lives_lost(event: CourseLifeEvent) -> u32 {
    match event {
        CourseLifeEvent::Tap(
            JudgeGrade::Fantastic | JudgeGrade::Excellent | JudgeGrade::Great,
        )
        | CourseLifeEvent::CheckpointHit
        | CourseLifeEvent::CheckpointMiss
        | CourseLifeEvent::HoldHeld => 0,
        CourseLifeEvent::Tap(JudgeGrade::Decent | JudgeGrade::WayOff | JudgeGrade::Miss)
        | CourseLifeEvent::Mine
        | CourseLifeEvent::HoldLetGo => 1,
        CourseLifeEvent::ForceFail => u32::MAX,
    }
}

#[inline(always)]
const fn survival_seconds_change(event: CourseLifeEvent) -> f32 {
    match event {
        CourseLifeEvent::Tap(JudgeGrade::Fantastic) => 0.2,
        CourseLifeEvent::Tap(JudgeGrade::Excellent)
        | CourseLifeEvent::CheckpointHit
        | CourseLifeEvent::CheckpointMiss
        | CourseLifeEvent::HoldHeld => 0.0,
        CourseLifeEvent::Tap(JudgeGrade::Great) => -0.5,
        CourseLifeEvent::Tap(JudgeGrade::Decent) => -1.0,
        CourseLifeEvent::Tap(JudgeGrade::WayOff) | CourseLifeEvent::Mine => -2.0,
        CourseLifeEvent::Tap(JudgeGrade::Miss) | CourseLifeEvent::HoldLetGo => -4.0,
        CourseLifeEvent::ForceFail => f32::MIN,
    }
}

fn record_player_life_change(
    player: &mut PlayerRuntime,
    current_music_time: f32,
    old_life: f32,
) {
    if (player.life - old_life).abs() <= 0.000_001_f32 {
        return;
    }
    deadsync_rules::life::record_life_history(
        &mut player.life_history,
        current_music_time,
        old_life,
    );
    deadsync_rules::life::record_life_history(
        &mut player.life_history,
        current_music_time,
        player.life,
    );
}

fn fail_course_life(player: &mut PlayerRuntime, current_music_time: f32) {
    if player.life > 0.0 {
        return;
    }
    player.life = 0.0;
    player.is_failing = true;
    player.fail_time.get_or_insert(current_music_time);
}

pub fn update_course_life_time(
    player: &mut PlayerRuntime,
    current_music_time: f32,
    delta_time: f32,
) {
    let CourseLifeState::Survival { remaining_seconds } = &mut player.course_life
    else {
        return;
    };
    if player.is_failing {
        return;
    }
    let old_life = player.life;
    if delta_time.is_finite() {
        *remaining_seconds = (*remaining_seconds - delta_time.max(0.0)).max(0.0);
    }
    player.life = survival_life(*remaining_seconds);
    record_player_life_change(player, current_music_time, old_life);
    fail_course_life(player, current_music_time);
}

pub fn apply_course_life_event(
    player: &mut PlayerRuntime,
    current_music_time: f32,
    normal_delta: f32,
    event: CourseLifeEvent,
) {
    if matches!(player.course_life, CourseLifeState::Bar) {
        apply_life_change(player, current_music_time, normal_delta);
        return;
    }

    if let Some(submit_life) = player.course_submit_life.as_mut() {
        let _ = deadsync_rules::life::apply_life_delta(
            submit_life,
            current_music_time,
            normal_delta,
        );
    }
    if player.is_failing {
        return;
    }

    let old_life = player.life;
    match &mut player.course_life {
        CourseLifeState::Bar => unreachable!(),
        CourseLifeState::Battery {
            lives, total_lives, ..
        } => {
            *lives = lives.saturating_sub(battery_lives_lost(event));
            player.life = *lives as f32 / (*total_lives).max(1) as f32;
        }
        CourseLifeState::Survival { remaining_seconds } => {
            let change = survival_seconds_change(event);
            if change == f32::MIN {
                player.life = 0.0;
            } else {
                *remaining_seconds = (*remaining_seconds + change).max(0.0);
                player.life = survival_life(*remaining_seconds);
            }
        }
    }
    record_player_life_change(player, current_music_time, old_life);
    fail_course_life(player, current_music_time);
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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct IndividualSongOutcome {
    pub song_completed_naturally: bool,
    pub is_failing: bool,
    pub life: f32,
    pub fail_time: Option<f32>,
}

pub fn individual_song_outcome(
    player: &PlayerRuntime,
    song_completed_naturally: bool,
    include_post_fail_passes: bool,
) -> IndividualSongOutcome {
    let post_fail_life = include_post_fail_passes
        .then_some(player.course_submit_life.as_ref())
        .flatten()
        .filter(|life| {
            player_runtime_is_dead(player)
                && !life.is_failing
                && life.fail_time.is_none()
                && life.life > 0.0
        });
    let (life, is_failing, fail_time) = post_fail_life.map_or(
        (player.life, player.is_failing, player.fail_time),
        |life| (life.life, life.is_failing, life.fail_time),
    );
    IndividualSongOutcome {
        song_completed_naturally,
        is_failing,
        life,
        fail_time,
    }
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
    if let Some(meter) = course_submit_life {
        let _ = deadsync_rules::life::apply_life_delta(meter, current_music_time, delta);
    }
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
