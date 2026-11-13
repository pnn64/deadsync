//! Stage and player statistics, with explicit reset/feed phases and script writes.
use super::catalog::{arg, integer, raw, text};
use super::fields::{object, set};
use super::scores::{empty, method};
use super::*;
use crate::{
    Clock,
    statistics::{Phase, State},
};
use std::sync::{Arc, Mutex, atomic::Ordering};

type Shared = Arc<Mutex<State>>;
#[derive(Clone)]
struct Binding {
    op: Op,
    state: Shared,
    clock: Arc<Clock>,
}
#[derive(Clone, Copy)]
enum Op {
    Integer(&'static str),
    Grade,
    Failed,
    Fail,
    SetScore(&'static str),
    Limits,
    Percent,
    Life,
    Combo(&'static str),
    Taps,
    Record,
    Radar(f32),
    GameplaySeconds,
    Host,
}
struct Setup {
    context: crate::SongLuaCompileContext,
    state: Shared,
    clock: Arc<Clock>,
}

pub(crate) fn install(
    vm: &mut Vm,
    catalog: Value,
    context: &crate::SongLuaCompileContext,
    state: Shared,
    clock: Arc<Clock>,
) -> Result<(), Error> {
    let captures = [vm.get_global(b"_G")?, vm.get_global(b"tonumber")?, catalog];
    let initialize = vm.new_host_function_with_data(
        initialize,
        &captures,
        Setup {
            context: context.clone(),
            state,
            clock,
        },
    )?;
    vm.call(initialize, &[]).map(|_| ())
}
fn initialize(call: &mut HostCall<'_>) -> Result<(), Error> {
    let setup = call.function_data::<Setup>().unwrap();
    let context = setup.context.clone();
    let binding = Binding {
        op: Op::Host,
        state: setup.state.clone(),
        clock: setup.clock.clone(),
    };
    let catalog = table(call, call.captures()[2])?;
    let rate = raw_table(call, catalog, "current_song_options")?;
    let stage = object(call, |call, stage| {
        let players = object(call, |call, players| {
            for index in 0..crate::LUA_PLAYERS {
                let stats = player(call, &binding, &context, index, "Player", rate)?;
                set(stats, "__songlua_player_index", index, call)?;
                set(players, index + 1, stats, call)?;
            }
            Ok(())
        })?;
        set(stage, "__songlua_player_stats", players, call)?;
        let p1 = call.raw_table_get(players, Value::Number(1.))?;
        let p2 = call.raw_table_get(players, Value::Number(2.))?;
        // LiveStats owns the original players even if authored Lua replaces
        // the public stage array. Keep that ownership in a private traced table.
        let bound_players = object(call, |call, bound| {
            set(bound, 1_i32, p1, call)?;
            set(bound, 2_i32, p2, call)
        })?;
        set(catalog, "statistics_players", bound_players, call)?;
        let query = call.new_host_function(player_query, &[p1, p2])?;
        set(stage, "GetPlayerStageStats", query, call)?;
        let multi = player(call, &binding, &context, 0, "MultiPlayer", rate)?;
        method(call, stage, "GetMultiPlayerStageStats", multi)?;
        let song = super::catalog::song(call, &context, rate)?;
        call.with_roots(&[Value::Table(song)], |call| {
            for name in ["GetPlayedSongs", "GetPossibleSongs"] {
                let query = call.new_host_function(single_array, &[Value::Table(song)])?;
                set(stage, name, query, call)?;
            }
            Ok(())
        })?;
        set(
            stage,
            "__songlua_total_steps_seconds",
            context.music_length_seconds.max(0.),
            call,
        )?;
        bound(
            call,
            stage,
            "GetGameplaySeconds",
            &binding,
            Op::GameplaySeconds,
        )?;
        for name in ["GetTotalPossibleStepsSeconds", "GetStepsSeconds"] {
            method(call, stage, name, context.music_length_seconds.max(0.))?;
        }
        method(call, stage, "GetStageIndex", 0_i32)?;
        method(call, stage, "GetStage", "Stage_1st")?;
        for name in [
            "AllFailed",
            "PlayerHasHighScore",
            "GetEarnedExtraStage",
            "GaveUp",
        ] {
            method(call, stage, name, false)?;
        }
        method(call, stage, "OnePassed", true)?;
        // Host feed retains the original players; reset reads the current raw stage array.
        let slots = raw_table(call, catalog, "game_state_slots")?;
        let selected = raw(call, slots, "steps")?;
        let captures = [
            Value::Table(stage),
            call.captures()[0],
            call.captures()[1],
            p1,
            p2,
            selected,
        ];
        let host = call.new_host_function_with_data(callback, &captures, binding.clone())?;
        set(catalog, "statistics_phase", host, call)?;
        binding.state.lock().unwrap().pending = Some(Phase::Reset(Box::new(context.clone())));
        call.call(host, &[])?;
        Ok(())
    })?;
    call.with_roots(&[Value::Table(stage)], |call| {
        let manager = object(call, |call, manager| {
            for name in [
                "GetCurStageStats",
                "GetAccumPlayedStageStats",
                "GetFinalEvalStageStats",
            ] {
                method(call, manager, name, stage)?;
            }
            let query = call.new_host_function(played_stage, &[Value::Table(stage)])?;
            set(manager, "GetPlayedStageStats", query, call)?;
            let reset = call.new_host_function(noop, &[])?;
            set(manager, "Reset", reset, call)?;
            method(call, manager, "GetStagesPlayed", 1_i32)?;
            for name in [
                "GetFinalGrade",
                "GetBestGrade",
                "GetWorstGrade",
                "GetBestFinalGrade",
            ] {
                method(call, manager, name, "Grade_Tier07")?;
            }
            Ok(())
        })?;
        set(table(call, call.captures()[0])?, "STATSMAN", manager, call)
    })
}
fn noop(_: &mut HostCall<'_>) -> Result<(), Error> {
    Ok(())
}
fn player_query(call: &mut HostCall<'_>) -> Result<(), Error> {
    let index = super::game_state::player(call, arg(call, 0)).unwrap_or(0);
    call.push(call.captures()[index])
}
fn single_array(call: &mut HostCall<'_>) -> Result<(), Error> {
    let values = object(call, |call, values| {
        set(values, 1_i32, call.captures()[0], call)
    })?;
    call.push(Value::Table(values))
}
fn played_stage(call: &mut HostCall<'_>) -> Result<(), Error> {
    let result = if integer(call, arg(call, 0)).unwrap_or(1) == 1 {
        call.captures()[0]
    } else {
        Value::Nil
    };
    call.push(result)
}
fn bound(
    call: &mut HostCall<'_>,
    owner: TableId,
    name: &str,
    binding: &Binding,
    op: Op,
) -> Result<(), Error> {
    let retained = if matches!(
        op,
        Op::Life | Op::Combo(_) | Op::Taps | Op::Record | Op::Radar(_)
    ) {
        Value::Nil
    } else {
        Value::Table(owner)
    };
    let captures = [retained, call.captures()[0], call.captures()[1]];
    let function = call.new_host_function_with_data(
        callback,
        &captures,
        Binding {
            op,
            ..binding.clone()
        },
    )?;
    set(owner, name, function, call)
}
fn player(
    call: &mut HostCall<'_>,
    binding: &Binding,
    context: &crate::SongLuaCompileContext,
    index: usize,
    score_name: &str,
    rate: TableId,
) -> Result<TableId, Error> {
    object(call, |call, owner| {
        set(owner, "__songlua_failed", false, call)?;
        set(owner, "__songlua_life", crate::SONG_LUA_INITIAL_LIFE, call)?;
        for key in [
            "__songlua_score",
            "__songlua_cur_max_score",
            "__songlua_actual_dance_points",
            "__songlua_possible_dance_points",
            "__songlua_current_possible_dance_points",
            "__songlua_combo",
            "__songlua_miss_combo",
        ] {
            set(owner, key, 0_i32, call)?;
        }
        let counts = object(call, |call, counts| {
            for name in crate::live::TAP_SCORES {
                set(counts, name, 0_i32, call)?;
            }
            Ok(())
        })?;
        set(owner, "__songlua_tap_scores", counts, call)?;
        let score = super::scores::score(call, score_name)?;
        method(call, owner, "GetHighScore", score)?;
        let steps = super::catalog::statistics_steps(call, context, index, rate)?;
        call.with_roots(&[Value::Table(steps)], |call| {
            for name in ["GetPlayedSteps", "GetPossibleSteps"] {
                let array = object(call, |call, array| set(array, 1_i32, steps, call))?;
                method(call, owner, name, array)?;
            }
            Ok(())
        })?;
        for (name, op) in [
            ("GetCurMaxScore", Op::Integer("__songlua_cur_max_score")),
            ("GetScore", Op::Integer("__songlua_score")),
            (
                "GetActualDancePoints",
                Op::Integer("__songlua_actual_dance_points"),
            ),
            (
                "GetPossibleDancePoints",
                Op::Integer("__songlua_possible_dance_points"),
            ),
            (
                "GetCurrentPossibleDancePoints",
                Op::Integer("__songlua_current_possible_dance_points"),
            ),
            ("GetGrade", Op::Grade),
            ("GetFailed", Op::Failed),
            ("FailPlayer", Op::Fail),
            ("SetScore", Op::SetScore("__songlua_score")),
            ("SetCurMaxScore", Op::SetScore("__songlua_cur_max_score")),
            ("SetDancePointLimits", Op::Limits),
            ("GetPercentDancePoints", Op::Percent),
            ("GetCurrentLife", Op::Life),
            ("GetCurrentCombo", Op::Combo("__songlua_combo")),
            ("GetCurrentMissCombo", Op::Combo("__songlua_miss_combo")),
            ("GetTapNoteScores", Op::Taps),
            ("GetLifeRecord", Op::Record),
            ("GetRadarActual", Op::Radar(0.)),
            ("GetRadarPossible", Op::Radar(1.)),
            ("GetRadarValues", Op::Radar(0.)),
        ] {
            bound(call, owner, name, binding, op)?;
        }
        for name in [
            "GetCaloriesBurned",
            "GetNumControllerSteps",
            "GetSurvivalSeconds",
            "GetLifeRemainingSeconds",
            "GetAliveSeconds",
            "GetHoldNoteScores",
            "MaxCombo",
            "GetLessonScoreActual",
            "GetLessonScoreNeeded",
            "GetPercentageOfTaps",
            "GetSongsPassed",
            "GetSongsPlayed",
        ] {
            method(call, owner, name, 0_i32)?;
        }
        method(call, owner, "GetCurrentScoreMultiplier", 1_i32)?;
        for name in ["FullCombo", "IsDisqualified", "FullComboOfScore"] {
            method(call, owner, name, false)?;
        }
        for name in ["GetMachineHighScoreIndex", "GetPersonalHighScoreIndex"] {
            method(call, owner, name, -1_i32)?;
        }
        for (name, value) in [
            ("GetStageAward", "StageAward_None"),
            ("GetPeakComboAward", "PeakComboAward_None"),
            ("GetBestFullComboTapNoteScore", "TapNoteScore_None"),
        ] {
            method(call, owner, name, value)?;
        }
        let list = call.new_host_function(empty, &[])?;
        set(owner, "GetComboList", list, call)
    })
}

fn raw_table(call: &mut HostCall<'_>, owner: TableId, key: &str) -> Result<TableId, Error> {
    let value = raw(call, owner, key)?;
    table(call, value)
}
fn get(call: &mut HostCall<'_>, owner: Value, key: &str) -> Result<Value, Error> {
    super::super::field(call, owner, key)
}
fn write(call: &mut HostCall<'_>, owner: Value, key: &str, value: Value) -> Result<(), Error> {
    super::super::set(call, owner, key, value)
}
fn number(call: &mut HostCall<'_>, value: Value) -> Result<f64, Error> {
    super::song_options::number(call, value, call.captures()[2])
}
fn limit_arg(call: &mut HostCall<'_>, value: Value) -> Result<i64, Error> {
    let value = if matches!(value, Value::Number(_)) {
        value
    } else {
        call.call(call.captures()[2], &[value])?
            .first()
            .copied()
            .unwrap_or(Value::Nil)
    };
    match value {
        Value::Number(n) => Ok(n as i32 as i64),
        _ => Err(call.error(b"SetDancePointLimits requires two numbers")),
    }
}
fn bounded_integer(
    call: &mut HostCall<'_>,
    value: Value,
    low: f64,
    high: f64,
) -> Result<f64, Error> {
    let n = number(call, value)?.trunc();
    if !n.is_finite() || n < low || n >= high {
        return Err(call.error(b"integer out of range"));
    }
    Ok(n)
}
fn points(call: &mut HostCall<'_>, owner: Value, key: &str) -> Result<i64, Error> {
    let value = get(call, owner, key)?;
    if value == Value::Nil {
        return Ok(0);
    }
    Ok(bounded_integer(call, value, i64::MIN as f64, 9223372036854775808.)? as i64)
}
fn truthy(value: Value) -> bool {
    !matches!(value, Value::Nil | Value::Boolean(false))
}
fn percent(actual: i64, possible: i64) -> f32 {
    if possible == 0 {
        return 0.;
    }
    if actual == possible {
        return 1.;
    }
    const INTERVAL: f32 = {
        let tenth = 0.1_f32 as f64;
        (tenth * tenth * tenth * tenth) as f32
    };
    let percent = (actual as f32 / possible as f32).max(0.) + 0.000001;
    (percent / INTERVAL) as i32 as f32 * INTERVAL
}
fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
    let binding = call.function_data::<Binding>().unwrap().clone();
    if matches!(binding.op, Op::Host) {
        let phase = binding.state.lock().unwrap().pending.take();
        return match phase {
            Some(Phase::Reset(context)) => reset(call, &context),
            Some(Phase::Feed { seconds, players }) => feed(call, seconds, players),
            None => Err(call.error(b"missing statistics phase")),
        };
    }
    let owner = call.captures()[0];
    let result = match binding.op {
        Op::Integer(key) => Value::Number(points(call, owner, key)? as f64),
        Op::Grade | Op::Failed => {
            let failed = truthy(get(call, owner, "__songlua_failed")?);
            if matches!(binding.op, Op::Failed) {
                Value::Boolean(failed)
            } else {
                call.try_intern(if failed {
                    b"Grade_Failed"
                } else {
                    b"Grade_Tier17"
                })?
            }
        }
        Op::Fail => {
            write(call, owner, "__songlua_failed", Value::Boolean(true))?;
            let value = raw(call, table(call, owner)?, "__songlua_player_index")?;
            if value != Value::Nil {
                let player = bounded_integer(call, value, 0., (usize::MAX as f64) + 1.)? as usize;
                let mut state = binding.state.lock().unwrap();
                if let Some(players) = &mut state.players
                    && let Some(value) = players.get_mut(player)
                {
                    value.failed = true;
                }
                if binding.clock.live.load(Ordering::Relaxed) {
                    state
                        .writes
                        .push(crate::live::SongLuaLivePlayerWrite::Fail { player });
                }
            }
            super::side_effect(call, call.captures()[1], call.captures()[2])?;
            owner
        }
        Op::SetScore(key) => {
            if let Some(score) = integer(call, arg(call, 0))
                && score >= 0
            {
                write(call, owner, key, Value::Number(score as f64))?;
            }
            super::side_effect(call, call.captures()[1], call.captures()[2])?;
            owner
        }
        Op::Limits => {
            let actual = limit_arg(call, arg(call, 0))?;
            let possible = limit_arg(call, arg(call, 1))?;
            if actual < 0 || possible <= 0 {
                owner
            } else {
                write(
                    call,
                    owner,
                    "__songlua_possible_dance_points",
                    Value::Number(possible as f64),
                )?;
                write(
                    call,
                    owner,
                    "__songlua_actual_dance_points",
                    Value::Number(actual.min(possible) as f64),
                )?;
                super::side_effect(call, call.captures()[1], call.captures()[2])?;
                call.arguments().last().copied().unwrap_or(Value::Nil)
            }
        }
        Op::Percent => {
            let actual = points(call, owner, "__songlua_actual_dance_points")?;
            let possible = points(call, owner, "__songlua_possible_dance_points")?;
            Value::Number(percent(actual, possible) as f64)
        }
        Op::Life | Op::Combo(_) | Op::Taps => {
            let receiver = table(
                call,
                call.arguments().first().copied().unwrap_or(Value::Nil),
            )?;
            let value = match binding.op {
                Op::Life => raw(call, receiver, "__songlua_life")?,
                Op::Combo(key) => raw(call, receiver, key)?,
                Op::Taps => {
                    let value = call.arguments().get(1).copied().unwrap_or(Value::Nil);
                    let id = call
                        .coerce_string(value)?
                        .ok_or_else(|| call.error(b"TapNoteScore string expected"))?;
                    let name = text(call, Value::String(id))
                        .ok_or_else(|| call.error(b"invalid UTF-8 string"))?;
                    let name = name.strip_prefix("TapNoteScore_").unwrap_or(&name);
                    if !crate::live::TAP_SCORES.contains(&name) {
                        return Err(call.error(format!("invalid TapNoteScore '{name}'").as_bytes()));
                    }
                    let counts = raw_table(call, receiver, "__songlua_tap_scores")?;
                    raw(call, counts, name)?
                }
                _ => unreachable!(),
            };
            Value::Number(if matches!(binding.op, Op::Life) {
                number(call, value)? as f32 as f64
            } else {
                bounded_integer(call, value, 0., 4294967296.)? as u32 as f64
            })
        }
        Op::Record => {
            let samples = integer(call, arg(call, 1))
                .unwrap_or(crate::GRAPH_DISPLAY_VALUE_RESOLUTION as i32)
                .max(1) as usize;
            let record = object(call, |call, record| {
                for i in 1..=samples {
                    set(record, i, crate::SONG_LUA_INITIAL_LIFE.clamp(0., 1.), call)?;
                }
                Ok(())
            })?;
            Value::Table(record)
        }
        Op::Radar(fallback) => Value::Table(super::scores::radar(call, fallback)?),
        Op::GameplaySeconds => {
            let value = raw(call, table(call, owner)?, "__songlua_gameplay_seconds")?;
            Value::Number(number(call, value)?)
        }
        Op::Host => unreachable!(),
    };
    call.push(result)
}

fn reset(call: &mut HostCall<'_>, context: &crate::SongLuaCompileContext) -> Result<(), Error> {
    let stage = table(call, call.captures()[0])?;
    set(stage, "__songlua_gameplay_seconds", 0_f64, call)?;
    let players = raw_table(call, stage, "__songlua_player_stats")?;
    let selected = table(call, call.captures()[5])?;
    call.with_roots(&[Value::Table(players)], |call| {
        for player in 0..crate::LUA_PLAYERS {
            let stats = table(
                call,
                call.raw_table_get(players, Value::Number((player + 1) as f64))?,
            )?;
            call.with_roots(&[Value::Table(stats)], |call| {
                let possible = context.player_step_indices[player]
                    .and_then(|i| context.song_steps.get(i))
                    .map_or(0, |s| s.possible_grade_points);
                set(stats, "__songlua_possible_dance_points", possible, call)?;
                for key in [
                    "__songlua_actual_dance_points",
                    "__songlua_current_possible_dance_points",
                    "__songlua_score",
                    "__songlua_cur_max_score",
                ] {
                    set(stats, key, 0_i32, call)?;
                }
                set(stats, "__songlua_failed", false, call)?;
                set(stats, "__songlua_life", crate::SONG_LUA_INITIAL_LIFE, call)?;
                set(stats, "__songlua_combo", 0_i32, call)?;
                set(stats, "__songlua_miss_combo", 0_i32, call)?;
                let counts = raw_table(call, stats, "__songlua_tap_scores")?;
                for name in crate::live::TAP_SCORES {
                    set(counts, name, 0_i32, call)?;
                }
                let steps = call.raw_table_get(selected, Value::Number((player + 1) as f64))?;
                call.with_roots(&[steps], |call| {
                    for name in ["GetPlayedSteps", "GetPossibleSteps"] {
                        let getter = get(call, Value::Table(stats), name)?;
                        if !matches!(getter, Value::Function(_) | Value::Native(_)) {
                            return Err(call.error(b"steps getter: function expected"));
                        }
                        let array = call
                            .call(getter, &[Value::Table(stats)])?
                            .first()
                            .copied()
                            .unwrap_or(Value::Nil);
                        let array = table(call, array)?;
                        call.raw_table_set(array, Value::Number(1.), steps)?;
                    }
                    Ok(())
                })
            })?;
        }
        Ok(())
    })
}
fn feed(
    call: &mut HostCall<'_>,
    seconds: f64,
    values: [crate::SongLuaLivePlayer; crate::LUA_PLAYERS],
) -> Result<(), Error> {
    set(
        table(call, call.captures()[0])?,
        "__songlua_gameplay_seconds",
        seconds,
        call,
    )?;
    for (index, value) in values.into_iter().enumerate() {
        let stats = table(call, call.captures()[index + 3])?;
        set(
            stats,
            "__songlua_possible_dance_points",
            value.possible_points,
            call,
        )?;
        set(
            stats,
            "__songlua_actual_dance_points",
            value.actual_points,
            call,
        )?;
        set(
            stats,
            "__songlua_current_possible_dance_points",
            value.current_possible_points,
            call,
        )?;
        set(stats, "__songlua_failed", value.failed, call)?;
        set(
            stats,
            "__songlua_life",
            value.life_record_latest.unwrap_or(value.life),
            call,
        )?;
        let meter = raw(call, stats, "__songlua_life_meter")?;
        if meter != Value::Nil {
            set(table(call, meter)?, "__songlua_life", value.life, call)?;
        }
        set(stats, "__songlua_combo", value.combo as f64, call)?;
        set(stats, "__songlua_miss_combo", value.miss_combo as f64, call)?;
        let counts = raw_table(call, stats, "__songlua_tap_scores")?;
        for (name, count) in crate::live::TAP_SCORES.into_iter().zip(value.tap_scores) {
            set(counts, name, count as f64, call)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn dance_percent_native_values() {
        let native: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/itgmania-song-lua-micro/native-dance-percent.json"
        )))
        .expect("complete native percent capture");
        for case in native["cases"].as_array().expect("native cases") {
            let actual = super::percent(
                case["actual"].as_i64().expect("native points"),
                case["possible"].as_i64().expect("native total"),
            );
            assert_eq!(
                f64::from(actual),
                case["percent"].as_f64().expect("native percentage"),
                "native case {}",
                case["index"]
            );
        }
        assert_eq!(super::percent(0, 0), 0.0, "native zero total branch");
    }
}
