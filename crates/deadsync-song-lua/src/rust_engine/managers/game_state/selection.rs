//! Authored chart identity and synchronous CurrentSteps change delivery.
use super::*;
const CURRENT: usize = 4;
const STYLE: usize = 5;
const MESSAGES: usize = 6;
const OWNER: usize = 7;
const KNOWN: usize = 8;
#[derive(Clone)]
struct Selection {
    write: bool,
    enabled: [bool; crate::LUA_PLAYERS],
    metadata: Arc<[crate::SongLuaStep]>,
    #[cfg(feature = "test-support")]
    observations: crate::rust_engine::observations::Observations,
}
pub(super) fn install(
    call: &mut HostCall<'_>,
    class: TableId,
    owner: TableId,
    catalog: TableId,
    slots: TableId,
    context: &crate::SongLuaCompileContext,
    messages: Value,
) -> Result<(), Error> {
    #[cfg(feature = "test-support")]
    let observations = call
        .function_data::<super::Setup>()
        .expect("game state setup")
        .observations
        .clone();
    let song = raw(call, catalog, "song")?;
    let song = table(call, song)?;
    let steps = raw(call, song, "__songlua_steps")?;
    let steps = table(call, steps)?;
    let mut known = Vec::new();
    let mut metadata = context.song_steps.clone();
    for i in 1..=metadata.len() {
        known.push(call.raw_table_get(steps, Value::Number(i as f64))?);
    }
    let initial = raw(call, catalog, "steps")?;
    let initial = table(call, initial)?;
    for (i, player) in context.players.iter().enumerate() {
        let value = call.raw_table_get(initial, Value::Number((i + 1) as f64))?;
        if !known.contains(&value) {
            known.push(value);
            metadata.push(crate::SongLuaStep {
                steps_type: crate::song_lua_style_info(&context.style_name)
                    .steps_type
                    .into(),
                difficulty: player.difficulty,
                description: String::new(),
                chart_name: String::new(),
                credit: String::new(),
                meter: player.difficulty.meter().max(0) as u32,
                music_file: String::new(),
                possible_grade_points: 0,
            });
        }
    }
    let retained = array(call, known.iter().copied())?;
    set(slots, "known_steps", retained, call)?;
    let metadata: Arc<[crate::SongLuaStep]> = metadata.into();
    let current = raw(call, slots, "steps")?;
    let style = raw(call, slots, "style")?;
    super::scene::install(call, catalog, current, &known, metadata.clone())?;
    for (name, write) in [
        ("SetCurrentSteps", true),
        ("GetEasiestStepsDifficulty", false),
    ] {
        let mut captures = vec![
            Value::Nil,
            call.captures()[1],
            call.captures()[2],
            call.captures()[3],
            current,
            if write { style } else { Value::Nil },
            messages,
            if write {
                Value::Table(owner)
            } else {
                Value::Nil
            },
        ];
        captures.extend_from_slice(&known);
        let f = call.new_host_function_with_data(
            binding,
            &captures,
            Selection {
                write,
                enabled: context.players.each_ref().map(|p| p.enabled),
                metadata: metadata.clone(),
                #[cfg(feature = "test-support")]
                observations: observations.clone(),
            },
        )?;
        set(class, name, f, call)?;
    }
    Ok(())
}
fn metadata<'a>(
    call: &HostCall<'_>,
    data: &'a Selection,
    value: Value,
) -> Option<&'a crate::SongLuaStep> {
    call.captures()[KNOWN..]
        .iter()
        .position(|v| *v == value)
        .map(|i| &data.metadata[i])
}
fn notify(call: &mut HostCall<'_>, player: usize) -> Result<(), Error> {
    let name = ["CurrentStepsP1Changed", "CurrentStepsP2Changed"][player];
    let globals = call.captures()[1];
    let history = get(call, globals, crate::SONG_LUA_BROADCASTS_KEY)?;
    if history != Value::Nil {
        let history = table(call, history)?;
        call.with_roots(&[Value::Table(history)], |call| {
            let entry = object(call, |call, entry| {
                set(entry, "message", name, call)?;
                set(entry, "has_params", false, call)
            })?;
            let next = call.raw_table_len(history)? + 1;
            call.raw_table_set(history, Value::Number(next as f64), Value::Table(entry))
        })?;
    }
    let globals = table(call, globals)?;
    let suppressed = raw(call, globals, crate::SONG_LUA_SUPPRESS_BROADCAST_KEY)?;
    if !truthy(suppressed) {
        let name = call.try_intern(name.as_bytes())?;
        call.call(
            call.captures()[MESSAGES],
            &[
                name,
                Value::Boolean(false),
                Value::Nil,
                Value::Boolean(true),
            ],
        )?;
    }
    Ok(())
}
fn write(call: &mut HostCall<'_>, player: usize, value: Value) -> Result<(), Error> {
    let current = table(call, call.captures()[CURRENT])?;
    call.raw_table_set(current, Value::Number((player + 1) as f64), value)?;
    notify(call, player)
}
fn binding(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call
        .function_data::<Selection>()
        .expect("selection binding")
        .clone();
    if !data.write {
        let mut easiest: Option<crate::SongLuaDifficulty> = None;
        for (index, enabled) in data.enabled.iter().enumerate() {
            if !enabled {
                continue;
            }
            let value = selected(call, call.captures()[CURRENT], index)?;
            if value == Value::Nil {
                let message = call.try_intern(
                    format!(
                        "GetEasiestStepsDifficulty called but p{} hasn't chosen notes",
                        index + 1
                    )
                    .as_bytes(),
                )?;
                call.call(
                    call.captures()[MESSAGES],
                    &[
                        message,
                        Value::Nil,
                        Value::Nil,
                        Value::Nil,
                        Value::Boolean(true),
                    ],
                )?;
                continue;
            }
            let step = metadata(call, &data, value)
                .ok_or_else(|| call.error(b"Unknown selected Steps"))?;
            if easiest.is_none_or(|old| step.difficulty.sort_key() < old.sort_key()) {
                easiest = Some(step.difficulty);
            }
        }
        let value = if let Some(difficulty) = easiest {
            call.try_intern(difficulty.sm_name().as_bytes())?
        } else {
            Value::Nil
        };
        return call.push(value);
    }
    let player = required_player(call, arg(call, 0))?;
    let value = arg(call, 1);
    if value == Value::Nil {
        write(call, player, Value::Nil)?;
        #[cfg(feature = "test-support")]
        data.observations.state_call(
            call,
            call.captures()[1],
            "GameState.SetCurrentSteps",
            vec![serde_json::json!(player), serde_json::Value::Null],
        )?;
        effect(call)?;
        return call.push(call.captures()[OWNER]);
    }
    let step = metadata(call, &data, value).ok_or_else(|| call.error(b"Expected Steps"))?;
    let style = selected(call, call.captures()[STYLE], 0)?;
    table(call, style)?;
    call.with_roots(&[style], |call| {
        let method = get(call, style, "GetStepsType")?;
        let method = function(call, method)?;
        let kind = call
            .call(method, &[style])?
            .first()
            .copied()
            .unwrap_or(Value::Nil);
        let kind = string(call, kind)?;
        if step.steps_type != kind {
            return Err(call.error(b"No compatible style for steps/trail."));
        }
        Ok(())
    })?;
    write(call, player, value)?;
    let main = selected(call, call.captures()[CURRENT], player)?;
    if main != Value::Nil {
        let main_data =
            metadata(call, &data, main).ok_or_else(|| call.error(b"Unknown selected Steps"))?;
        for (other, enabled) in data.enabled.iter().enumerate() {
            if !enabled {
                continue;
            }
            let selected = selected(call, call.captures()[CURRENT], other)?;
            if selected == main {
                continue;
            }
            let matches = metadata(call, &data, selected).is_some_and(|other| {
                other.music_file == main_data.music_file
                    && !matches!(
                        main_data.steps_type.as_str(),
                        "StepsType_Dance_Routine" | "StepsType_Pump_Routine"
                    )
                    && !matches!(
                        other.steps_type.as_str(),
                        "StepsType_Dance_Routine" | "StepsType_Pump_Routine"
                    )
            });
            if !matches {
                write(call, other, main)?;
            }
        }
    }
    #[cfg(feature = "test-support")]
    data.observations.state_call(
        call,
        call.captures()[1],
        "GameState.SetCurrentSteps",
        vec![
            serde_json::json!(player),
            serde_json::json!(step.steps_type),
            serde_json::json!(step.difficulty.sm_name()),
            serde_json::json!(step.description),
        ],
    )?;
    effect(call)?;
    call.push(call.captures()[OWNER])
}
