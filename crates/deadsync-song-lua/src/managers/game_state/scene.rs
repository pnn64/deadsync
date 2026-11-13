//! Host selection reads and CPU-player seeding at a screen load boundary.
use super::*;

const CURRENT: usize = 4;
const KNOWN: usize = 5;

pub(super) fn install(
    call: &mut HostCall<'_>,
    catalog: TableId,
    current: Value,
    known: &[Value],
    metadata: Arc<[crate::SongLuaStep]>,
) -> Result<(), Error> {
    let mut captures = vec![
        Value::Nil,
        call.captures()[1],
        call.captures()[2],
        call.captures()[3],
        current,
    ];
    captures.extend_from_slice(known);
    let read = call.new_host_function_with_data(read, &captures, metadata)?;
    set(catalog, "scene_selection", read, call)?;
    #[cfg(feature = "test-support")]
    {
        let indices = call.new_host_function(read_indices, &captures)?;
        set(catalog, "scene_step_indices", indices, call)?;
    }
    let seed = call.new_host_function(seed, &[current])?;
    set(catalog, "initialize_gameplay_selection", seed, call)
}

fn read(call: &mut HostCall<'_>) -> Result<(), Error> {
    let metadata = call
        .function_data::<Arc<[crate::SongLuaStep]>>()
        .unwrap()
        .clone();
    let current = table(call, call.captures()[CURRENT])?;
    for player in 1..=crate::LUA_PLAYERS {
        let selected = call.raw_table_get(current, Value::Number(player as f64))?;
        if selected != Value::Nil {
            table(call, selected)?;
        }
        let index = call.captures()[KNOWN..].iter().position(|v| *v == selected);
        call.push(index.map_or(Value::Nil, |i| Value::Number(i as f64)))?;
        call.push(index.map_or(Value::Nil, |i| {
            Value::Number(metadata[i].difficulty.sort_key() as f64)
        }))?;
    }
    // Native context projection reads the charts before this reentrant style
    // query. A style getter may change the slots for the *next* projection.
    let style = super::super::player_options::current_style(call);
    let style = call.try_intern(style.as_bytes())?;
    call.push(style)
}

#[cfg(feature = "test-support")]
fn read_indices(call: &mut HostCall<'_>) -> Result<(), Error> {
    let current = table(call, call.captures()[CURRENT])?;
    for player in 1..=crate::LUA_PLAYERS {
        let selected = call.raw_table_get(current, Value::Number(player as f64))?;
        if selected != Value::Nil {
            table(call, selected)?;
        }
        let index = call.captures()[KNOWN..].iter().position(|v| *v == selected);
        call.push(index.map_or(Value::Nil, |i| Value::Number(i as f64)))?;
    }
    Ok(())
}

fn seed(call: &mut HostCall<'_>) -> Result<(), Error> {
    let enabled: [bool; crate::LUA_PLAYERS] =
        std::array::from_fn(|i| call.arguments().get(i) == Some(&Value::Boolean(true)));
    let Some(human) = enabled.iter().position(|enabled| *enabled) else {
        return Ok(());
    };
    let current = table(call, call.captures()[0])?;
    let steps = call.raw_table_get(current, Value::Number((human + 1) as f64))?;
    if steps != Value::Nil {
        table(call, steps)?;
    }
    // This is an engine assignment, without SetCurrentSteps validation,
    // broadcasts or a side-effect increment.
    for (player, enabled) in enabled.into_iter().enumerate() {
        if !enabled {
            call.raw_table_set(current, Value::Number((player + 1) as f64), steps)?;
        }
    }
    Ok(())
}

pub(crate) fn context(
    vm: &mut Vm,
    catalog: TableId,
    context: &crate::SongLuaCompileContext,
) -> Result<crate::SongLuaCompileContext, Error> {
    let key = vm.try_intern(b"scene_selection")?;
    let read = vm.raw_table_get(catalog, key)?;
    let values = vm.call(read, &[])?.to_vec();
    let mut selected = context.clone();
    let difficulties = [
        crate::SongLuaDifficulty::Beginner,
        crate::SongLuaDifficulty::Easy,
        crate::SongLuaDifficulty::Medium,
        crate::SongLuaDifficulty::Hard,
        crate::SongLuaDifficulty::Challenge,
        crate::SongLuaDifficulty::Edit,
    ];
    for player in 0..crate::LUA_PLAYERS {
        let index = match values[player * 2] {
            Value::Number(i) => Some(i as usize),
            _ => None,
        };
        selected.player_step_indices[player] = index.filter(|i| *i < context.song_steps.len());
        if let Value::Number(difficulty) = values[player * 2 + 1] {
            selected.players[player].difficulty = difficulties[difficulty as usize];
        }
        if let Some(index) = selected.player_step_indices[player]
            && let Some(timing) = selected.song_step_timing.get(index)
        {
            selected.player_timing[player] = Some(timing.clone());
        }
    }
    let Value::String(style) = values[crate::LUA_PLAYERS * 2] else {
        unreachable!("host style projection returns a string")
    };
    selected.style_name = String::from_utf8(vm.string_bytes(style).unwrap().to_vec()).unwrap();
    Ok(selected)
}

pub(crate) fn initialize(
    vm: &mut Vm,
    catalog: TableId,
    context: &crate::SongLuaCompileContext,
) -> Result<(), Error> {
    let key = vm.try_intern(b"initialize_gameplay_selection")?;
    let seed = vm.raw_table_get(catalog, key)?;
    let enabled = context
        .players
        .each_ref()
        .map(|p| Value::Boolean(p.enabled));
    vm.call(seed, &enabled).map(|_| ())
}
