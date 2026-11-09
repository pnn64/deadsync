use super::*;
use crate::rust_engine::PlayerStateStatus;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct Setup {
    context: crate::SongLuaCompileContext,
    #[cfg(feature = "test-support")]
    observations: observations::Observations,
    status: Arc<Mutex<[PlayerStateStatus; crate::LUA_PLAYERS]>>,
}
pub(in crate::rust_engine) fn install(
    vm: &mut Vm,
    catalog: Value,
    positions: [Value; crate::LUA_PLAYERS],
    context: &crate::SongLuaCompileContext,
    status: Arc<Mutex<[PlayerStateStatus; crate::LUA_PLAYERS]>>,
    #[cfg(feature = "test-support")] observations: observations::Observations,
) -> Result<(), Error> {
    let captures = [
        catalog,
        positions[0],
        positions[1],
        vm.get_global(b"_G")?,
        vm.get_global(b"tonumber")?,
        vm.get_global(b"next")?,
    ];
    let initialize = vm.new_host_function_with_data(
        initialize,
        &captures,
        Setup {
            context: context.clone(),
            #[cfg(feature = "test-support")]
            observations,
            status,
        },
    )?;
    vm.call(initialize, &[]).map(|_| ())
}
#[derive(Clone, Copy)]
enum Operation {
    Select,
    Text,
    Array,
    Assign,
}
#[derive(Clone)]
struct Binding {
    operation: Operation,
    #[cfg(feature = "test-support")]
    observations: observations::Observations,
}
#[derive(Clone)]
struct StatusBinding {
    shared: Arc<Mutex<[PlayerStateStatus; crate::LUA_PLAYERS]>>,
    index: usize,
    health: bool,
    fallback: &'static str,
}
fn status(call: &mut HostCall<'_>) -> Result<(), Error> {
    let binding = call
        .function_data::<StatusBinding>()
        .expect("player status binding");
    let value = binding.shared.lock().expect("player status")[binding.index];
    let text = if binding.health {
        value.health.unwrap_or(binding.fallback)
    } else {
        value.autoplay.map_or(binding.fallback, |autoplay| {
            if autoplay {
                "PlayerController_Autoplay"
            } else {
                "PlayerController_Human"
            }
        })
    };
    let value = call.try_intern(text.as_bytes())?;
    call.push(value)
}
fn fixed(call: &mut HostCall<'_>, owner: TableId, name: &str, value: Value) -> Result<(), Error> {
    let method = call.new_host_function(constant, &[value])?;
    set(owner, name, method, call)
}
fn initialize(call: &mut HostCall<'_>) -> Result<(), Error> {
    let setup = call
        .function_data::<Setup>()
        .expect("player state setup")
        .clone();
    let catalog = table(call, call.captures()[0])?;
    let players = raw(call, catalog, "player_options")?;
    let players = table(call, players)?;
    let states = object(call, |call, states| {
        for (index, player) in setup.context.players.iter().enumerate() {
            let levels = call.raw_table_get(players, Value::Number((index + 1) as f64))?;
            let levels = table(call, levels)?;
            let mut captures = vec![
                Value::Nil,
                call.captures()[3],
                call.captures()[4],
                call.captures()[5],
            ];
            for level in 1..=4 {
                captures.push(call.raw_table_get(levels, Value::Number(level as f64))?);
            }
            let state = object(call, |call, state| {
                for (name, health, fallback) in [
                    (
                        "GetPlayerController",
                        false,
                        if player.enabled && player.autoplay == 0.0 {
                            "PlayerController_Human"
                        } else {
                            "PlayerController_Autoplay"
                        },
                    ),
                    (
                        "GetHealthState",
                        true,
                        if player.enabled {
                            "HealthState_Alive"
                        } else {
                            "HealthState_Dead"
                        },
                    ),
                ] {
                    let method = call.new_host_function_with_data(
                        status,
                        &[],
                        StatusBinding {
                            shared: setup.status.clone(),
                            index,
                            health,
                            fallback,
                        },
                    )?;
                    set(state, name, method, call)?;
                }
                let player_name = call.try_intern(crate::player_number_name(index).as_bytes())?;
                fixed(call, state, "GetPlayerNumber", player_name)?;
                fixed(call, state, "GetCurrentPlayerOptions", captures[7])?;
                fixed(call, state, "GetSongPosition", call.captures()[index + 1])?;
                for (name, operation) in [
                    ("GetPlayerOptions", Operation::Select),
                    ("GetPlayerOptionsString", Operation::Text),
                    ("GetPlayerOptionsArray", Operation::Array),
                    ("SetPlayerOptions", Operation::Assign),
                ] {
                    let method = call.new_host_function_with_data(
                        binding,
                        &captures,
                        Binding {
                            operation,
                            #[cfg(feature = "test-support")]
                            observations: setup.observations.clone(),
                        },
                    )?;
                    set(state, name, method, call)?;
                }
                Ok(())
            })?;
            set(states, index + 1, state, call)?;
        }
        Ok(())
    })?;
    set(catalog, "player_states", states, call)
}
fn binding(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call
        .function_data::<Binding>()
        .expect("player state operation")
        .clone();
    let operation = data.operation;
    if matches!(operation, Operation::Assign)
        && !matches!(call.arguments().first(), Some(Value::Table(_)))
    {
        return Ok(());
    }
    let level = enum_index(
        call,
        arg(call, 0),
        &super::super::song_options::LEVELS,
        "ModsLevel",
    )?;
    let owners = [
        table(call, call.captures()[4])?,
        table(call, call.captures()[5])?,
        table(call, call.captures()[6])?,
        table(call, call.captures()[7])?,
    ];
    match operation {
        Operation::Select => call.push(Value::Table(owners[level])),
        Operation::Text | Operation::Array => {
            let parts = levels::parts(call, owners[level])?;
            let value = if matches!(operation, Operation::Text) {
                call.try_intern(parts.join(", ").as_bytes())?
            } else {
                Value::Table(super::super::fields::array(call, parts)?)
            };
            call.push(value)
        }
        Operation::Assign => {
            let text = text(call, arg(call, 1)).unwrap_or_default();
            levels::set_options(
                call,
                owners,
                level,
                &text,
                #[cfg(feature = "test-support")]
                &data.observations,
            )
        }
    }
}
