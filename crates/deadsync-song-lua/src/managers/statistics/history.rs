//! Played StageStats own copies; current-stage feeds must not rewrite history.
use super::*;

const PLAYER_OBJECT_GETTERS: [&str; 3] = ["GetPlayedSteps", "GetPossibleSteps", "GetHighScore"];

pub(super) fn retain_player_sources(
    call: &mut HostCall<'_>, sources: TableId, index: usize, owner: TableId,
) -> Result<(), Error> {
    let entry = object(call, |call, entry| {
        set(entry, 1_i32, owner, call)?;
        for (offset, name) in PLAYER_OBJECT_GETTERS.into_iter().enumerate() {
            let getter = raw(call, owner, name)?;
            set(entry, offset + 2, getter, call)?;
        }
        Ok(())
    })?;
    set(sources, index, entry, call)
}

pub(super) fn install_commit(
    call: &mut HostCall<'_>, catalog: TableId, stage: TableId, sources: TableId,
    history: TableId, binding: &Binding,
) -> Result<(), Error> {
    // Preserve native owners rather than resolving authored replacements of
    // STATSMAN, GetPlayerStageStats, or a player's array getters at commitment.
    let commit = call.new_host_function_with_data(commit, &[
        call.captures()[0], call.captures()[1], Value::Table(stage),
        Value::Table(sources), Value::Table(history),
    ], binding.clone())?;
    set(catalog, "statistics_commit", commit, call)
}

pub(super) fn install_queries(call: &mut HostCall<'_>, manager: TableId, history: TableId) -> Result<(), Error> {
    let query = call.new_host_function(played_stage, &[
        Value::Table(history), call.captures()[0], call.captures()[1],
    ])?;
    set(manager, "GetPlayedStageStats", query, call)?;
    let count = call.new_host_function(|call| {
        let history = table(call, call.captures()[0])?;
        let count = call.raw_table_len(history)?;
        call.push(Value::Number(count as f64))
    }, &[Value::Table(history)])?;
    set(manager, "GetStagesPlayed", count, call)
}

fn played_stage(call: &mut HostCall<'_>) -> Result<(), Error> {
    // Native luaL_checkint truncates a numeric argument; 1 is newest, not the
    // first stage. Missing/non-numeric arguments remain errors before a stage.
    let ago = number(call, arg(call, 0))? as i32;
    let history = table(call, call.captures()[0])?;
    let count = call.raw_table_len(history)?;
    let index = if ago > 0 { count.checked_sub(ago as usize) } else { None };
    if let Some(index) = index {
        let result = call.raw_table_get(history, Value::Number((index + 1) as f64))?;
        call.push(result)?;
    }
    // Native returns no values when iAgo is outside the saved-stage range.
    Ok(())
}

fn copy_table(call: &mut HostCall<'_>, original: TableId) -> Result<TableId, Error> {
    object(call, |call, copy| {
        let mut key = Value::Nil;
        while let Some((next, value)) = call.raw_table_next(original, key)? {
            key = next;
            call.raw_table_set(copy, key, value)?;
        }
        Ok(())
    })
}

fn copy_player(call: &mut HostCall<'_>, source: TableId, binding: &Binding) -> Result<TableId, Error> {
    let original = call.raw_table_get(source, Value::Number(1.))?;
    let copy = copy_table(call, table(call, original)?)?;
    call.with_roots(&[Value::Table(copy)], |call| {
        let counts = raw_table(call, copy, "__songlua_tap_scores")?;
        let counts = copy_table(call, counts)?;
        set(copy, "__songlua_tap_scores", counts, call)?;
        // PlayerStageStats does not own its screen's life-meter actor.
        set(copy, "__songlua_life_meter", Value::Nil, call)?;
        bind_player_operations(call, copy, binding)?;
        for (offset, name) in PLAYER_OBJECT_GETTERS.into_iter().enumerate() {
            let getter = call.raw_table_get(source, Value::Number((offset + 2) as f64))?;
            let value = call.call(getter, &[original])?.first().copied().unwrap_or(Value::Nil);
            call.with_roots(&[value], |call| {
                let copied = copy_table(call, table(call, value)?)?;
                method(call, copy, name, copied)
            })?;
        }
        Ok(copy)
    })
}

fn commit(call: &mut HostCall<'_>) -> Result<(), Error> {
    let current = call.function_data::<Binding>().unwrap().clone();
    let snapshot = {
        let state = current.state.lock().unwrap();
        State {
            players: state.players, routine: state.routine,
            life_records: state.life_records.clone(),
            routine_life_record: state.routine_life_record.clone(),
            ..Default::default()
        }
    };
    let binding = Binding {
        state: Arc::new(Mutex::new(snapshot)), write_through: false, ..current
    };
    let original = table(call, call.captures()[2])?;
    let sources = table(call, call.captures()[3])?;
    let history = table(call, call.captures()[4])?;
    let stage = copy_table(call, original)?;
    call.with_roots(&[Value::Table(stage)], |call| {
        let players = object(call, |call, players| {
            for index in 1..=crate::LUA_PLAYERS + 2 {
                let source = call.raw_table_get(sources, Value::Number(index as f64))?;
                let player = copy_player(call, table(call, source)?, &binding)?;
                set(players, index, player, call)?;
            }
            Ok(())
        })?;
        call.with_roots(&[Value::Table(players)], |call| {
            let p1 = call.raw_table_get(players, Value::Number(1.))?;
            let p2 = call.raw_table_get(players, Value::Number(2.))?;
            let pair = object(call, |call, pair| {
                set(pair, 1_i32, p1, call)?;
                set(pair, 2_i32, p2, call)
            })?;
            set(stage, "__songlua_player_stats", pair, call)?;
            let query = call.new_host_function(player_query, &[p1, p2])?;
            set(stage, "GetPlayerStageStats", query, call)?;
            for (offset, name) in ["GetRoutineStageStats", "GetMultiPlayerStageStats"].into_iter().enumerate() {
                let owner = call.raw_table_get(players, Value::Number((crate::LUA_PLAYERS + offset + 1) as f64))?;
                method(call, stage, name, owner)?;
            }
            bound(call, stage, "GetGameplaySeconds", &binding, Op::Number("__songlua_gameplay_seconds"))?;
            bound(call, stage, "GetStepsSeconds", &binding, Op::Number("__songlua_steps_seconds"))?;
            let index = call.raw_table_len(history)? + 1;
            set(history, index, stage, call)
        })
    })
}
