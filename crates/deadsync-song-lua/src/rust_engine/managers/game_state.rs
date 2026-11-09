//! GameState's mutable selection slots and writable Lua class table.
use super::catalog::{arg, float, raw, text};
use super::fields::{Field, array, object, set};
use super::*;
use std::sync::Arc;
mod helpers;
pub(in crate::rust_engine) mod scene;
mod selection;

#[derive(Clone)]
struct Setup {
    context: crate::SongLuaCompileContext,
    #[cfg(feature = "test-support")]
    observations: crate::rust_engine::observations::Observations,
}
#[derive(Clone)]
struct Metadata {
    enabled: [bool; crate::LUA_PLAYERS],
    screen_x: [f32; crate::LUA_PLAYERS],
    display_bpms: [f32; 2],
    rate: f32,
    center_y: f32,
}
#[derive(Clone)]
struct Binding {
    name: &'static str,
    metadata: Arc<Metadata>,
}

pub(in crate::rust_engine) fn install(
    vm: &mut Vm,
    catalog: Value,
    runtime: Value,
    position: Value,
    messages: Value,
    context: &crate::SongLuaCompileContext,
    #[cfg(feature = "test-support")] observations: crate::rust_engine::observations::Observations,
) -> Result<(), Error> {
    let captures = [
        Value::Nil,
        vm.get_global(b"_G")?,
        vm.get_global(b"tonumber")?,
        vm.get_global(b"next")?,
        catalog,
        runtime,
        position,
        messages,
        vm.get_global(b"setmetatable")?,
    ];
    let initialize = vm.new_host_function_with_data(
        initialize,
        &captures,
        Setup {
            context: context.clone(),
            #[cfg(feature = "test-support")]
            observations,
        },
    )?;
    vm.call(initialize, &[])?;
    let code=deadlib_lua::compile_named(b"return function(player) return GAMESTATE:GetPlayerState(player):GetPlayerController() == 'PlayerController_Autoplay' end",b"@theme:/Scripts/SL-Helpers.lua")?;
    let function = vm.execute(&code)?[0];
    vm.set_global(b"IsAutoplay", function)
}
fn get(call: &mut HostCall<'_>, owner: Value, key: &str) -> Result<Value, Error> {
    let key = call.try_intern(key.as_bytes())?;
    call.table_get(owner, key)
}
fn function(call: &mut HostCall<'_>, value: Value) -> Result<Value, Error> {
    if matches!(value, Value::Function(_) | Value::Native(_)) {
        Ok(value)
    } else {
        Err(call.error(b"function expected"))
    }
}
fn string(call: &mut HostCall<'_>, value: Value) -> Result<String, Error> {
    let id = call
        .coerce_string(value)?
        .ok_or_else(|| call.error(b"string expected"))?;
    text(call, Value::String(id)).ok_or_else(|| call.error(b"invalid UTF-8 string"))
}
fn truthy(v: Value) -> bool {
    !matches!(v, Value::Nil | Value::Boolean(false))
}
pub(super) fn player(call: &HostCall<'_>, value: Value) -> Option<usize> {
    match value {
        Value::Number(n) if n >= 0.0 && n < crate::LUA_PLAYERS as f64 && n.fract() == 0.0 => {
            Some(n as usize)
        }
        Value::String(_) => {
            let s = text(call, value)?;
            if s.eq_ignore_ascii_case("P1") || s.eq_ignore_ascii_case("PlayerNumber_P1") {
                Some(0)
            } else if s.eq_ignore_ascii_case("P2") || s.eq_ignore_ascii_case("PlayerNumber_P2") {
                Some(1)
            } else {
                None
            }
        }
        _ => None,
    }
}
fn required_player(call: &mut HostCall<'_>, value: Value) -> Result<usize, Error> {
    player(call, value).ok_or_else(|| call.error(b"Expected PlayerNumber"))
}
fn constant_field(
    call: &mut HostCall<'_>,
    owner: TableId,
    name: &str,
    value: impl Field,
) -> Result<(), Error> {
    let value = value.value(call)?;
    let f = call.new_host_function(constant, &[value])?;
    set(owner, name, f, call)
}
fn bind(
    call: &mut HostCall<'_>,
    owner: TableId,
    name: &'static str,
    extras: &[Value],
    metadata: &Arc<Metadata>,
) -> Result<(), Error> {
    let mut captures = vec![
        Value::Nil,
        call.captures()[1],
        call.captures()[2],
        call.captures()[3],
    ];
    captures.extend_from_slice(extras);
    let f = call.new_host_function_with_data(
        binding,
        &captures,
        Binding {
            name,
            metadata: metadata.clone(),
        },
    )?;
    set(owner, name, f, call)
}
fn slot(
    call: &mut HostCall<'_>,
    values: impl IntoIterator<Item = Value>,
) -> Result<TableId, Error> {
    let values: Vec<_> = values.into_iter().collect();
    call.with_roots(&values, |call| array(call, values.iter().copied()))
}
fn initialize(call: &mut HostCall<'_>) -> Result<(), Error> {
    let setup = call
        .function_data::<Setup>()
        .expect("game state setup")
        .clone();
    let context = setup.context;
    let metadata = Arc::new(Metadata {
        enabled: context.players.each_ref().map(|p| p.enabled),
        screen_x: context.players.each_ref().map(|p| p.screen_x),
        display_bpms: context.song_display_bpms,
        rate: crate::song_music_rate(&context),
        center_y: context.screen_height.max(1.0) * 0.5,
    });
    let globals = table(call, call.captures()[1])?;
    let catalog = table(call, call.captures()[4])?;
    let slots = object(call, |call, slots| {
        let song = raw(call, catalog, "song")?;
        let current = slot(call, [song])?;
        set(slots, "song", current, call)?;
        let style = super::utility::style(call, &context.style_name)?;
        let current = slot(call, [Value::Table(style)])?;
        set(slots, "style", current, call)?;
        for key in ["steps", "trails"] {
            let source = raw(call, catalog, key)?;
            let source = table(call, source)?;
            let values = [
                call.raw_table_get(source, Value::Number(1.0))?,
                call.raw_table_get(source, Value::Number(2.0))?,
            ];
            let current = slot(call, values)?;
            set(slots, key, current, call)?;
        }
        let name = call.try_intern(b"SortOrder_Group")?;
        let current = slot(call, [name])?;
        set(slots, "sort", current, call)
    })?;
    set(catalog, "game_state_slots", slots, call)?;
    let owner = object(call, |call, owner| {
        let class = object(call, |call, class| {
            for (key, getter, setter) in [
                ("song", "GetCurrentSong", "SetCurrentSong"),
                ("style", "GetCurrentStyle", "SetCurrentStyle"),
                ("trails", "GetCurrentTrail", "SetCurrentTrail"),
                ("steps", "GetCurrentSteps", ""),
                ("sort", "GetSortOrder", ""),
            ] {
                let value = raw(call, slots, key)?;
                bind(call, class, getter, &[value], &metadata)?;
                if !setter.is_empty() {
                    bind(call, class, setter, &[value], &metadata)?;
                }
            }
            let style = raw(call, slots, "style")?;
            set(owner, "__songlua_current_style", style, call)?;
            let env = call.try_new_table()?;
            constant_field(call, class, "Env", env)?;
            bind(call, globals, "getenv", &[Value::Table(env)], &metadata)?;
            bind(call, globals, "setenv", &[Value::Table(env)], &metadata)?;
            let enabled = array(
                call,
                (0..crate::LUA_PLAYERS)
                    .filter(|i| metadata.enabled[*i])
                    .map(crate::player_number_name),
            )?;
            constant_field(call, class, "GetEnabledPlayers", enabled)?;
            constant_field(call, class, "GetHumanPlayers", enabled)?;
            let players = raw(call, catalog, "player_states")?;
            let players = table(call, players)?;
            let states = [
                call.raw_table_get(players, Value::Number(1.0))?,
                call.raw_table_get(players, Value::Number(2.0))?,
            ];
            bind(call, class, "GetPlayerState", &states, &metadata)?;
            bind(call, globals, "GetPlayerOptionsString", &states, &metadata)?;
            for name in [
                "IsPlayerEnabled",
                "IsHumanPlayer",
                "IsSideJoined",
                "IsEventMode",
                "InsertCoin",
                "AddStageToPlayer",
                "ResetPlayerOptions",
                "SetPreferredDifficulty",
                "UnjoinPlayer",
                "JoinPlayer",
                "SaveProfiles",
            ] {
                bind(call, class, name, &[], &metadata)?;
            }
            bind(
                call,
                globals,
                "IsHumanPlayer",
                &[Value::Boolean(true)],
                &metadata,
            )?;
            let master = metadata.enabled.iter().position(|v| *v).unwrap_or(0);
            constant_field(
                call,
                class,
                "GetMasterPlayerNumber",
                crate::player_number_name(master),
            )?;
            let count = metadata.enabled.iter().filter(|v| **v).count();
            for name in ["GetNumSidesJoined", "GetNumPlayersEnabled"] {
                constant_field(call, class, name, count)?;
            }
            constant_field(call, class, "IsCourseMode", false)?;
            let course = raw(call, catalog, "course")?;
            constant_field(call, class, "GetCurrentCourse", course)?;
            let game = object(call, |call, game| {
                constant_field(call, game, "GetName", "dance")
            })?;
            constant_field(call, class, "GetCurrentGame", game)?;
            for (name, value) in [
                ("GetCoinMode", "CoinMode_Free"),
                ("GetPremium", "Premium_Off"),
                ("GetPlayMode", "PlayMode_Regular"),
                ("GetPlayerFailType", "FailType_Immediate"),
            ] {
                constant_field(call, class, name, value)?;
            }
            for name in [
                "GetCoins",
                "GetCoinsNeededToJoin",
                "GetCurrentStageIndex",
                "GetCourseSongIndex",
            ] {
                constant_field(call, class, name, 0_i32)?;
            }
            constant_field(call, class, "GetNumStagesLeft", 1_i32)?;
            let position = call.captures()[6];
            constant_field(call, class, "GetSongPosition", position)?;
            for (name, source) in [
                ("GetSongBeat", "GetSongBeat"),
                ("GetSongBPS", "GetCurBPS"),
                ("GetCurMusicSeconds", "GetMusicSeconds"),
            ] {
                let f = get(call, position, source)?;
                set(class, name, f, call)?;
            }
            let song_ops = raw(call, catalog, "song_option_operations")?;
            for (name, source) in [
                ("GetSongOptionsObject", "Select"),
                ("GetSongOptions", "Text"),
            ] {
                let f = get(call, song_ops, source)?;
                set(class, name, f, call)?;
            }
            let f = get(call, song_ops, "Text")?;
            bind(call, class, "GetSongOptionsString", &[f], &metadata)?;
            super::player_options::game::install(call, class, owner, catalog)?;
            selection::install(
                call,
                class,
                owner,
                catalog,
                slots,
                &context,
                call.captures()[7],
            )?;
            Ok(())
        })?;
        set(owner, "__songlua_class_methods", class, call)?;
        let mt = object(call, |call, mt| {
            set(mt, "__index", class, call)?;
            set(mt, "__metatable", "(hidden)", call)
        })?;
        call.call(call.captures()[8], &[Value::Table(owner), Value::Table(mt)])?;
        set(globals, "GameState", class, call)
    })?;
    set(globals, "GAMESTATE", owner, call)?;
    let options = raw(call, catalog, "player_options")?;
    let options = table(call, options)?;
    for (i, key) in crate::SONG_LUA_PLAYER_OPTIONS_KEYS.iter().enumerate() {
        let levels = call.raw_table_get(options, Value::Number((i + 1) as f64))?;
        let levels = table(call, levels)?;
        let value = call.raw_table_get(levels, Value::Number(3.0))?;
        set(globals, *key, value, call)?;
    }
    let song = raw(call, slots, "song")?;
    let steps = raw(call, slots, "steps")?;
    bind(call, globals, "GetSongAndSteps", &[song, steps], &metadata)?;
    bind(
        call,
        globals,
        "currentTimeSongOrCourse",
        &[call.captures()[5]],
        &metadata,
    )?;
    constant_field(
        call,
        globals,
        "totalLengthSongOrCourse",
        context.music_length_seconds.max(0.0) / metadata.rate.max(f32::EPSILON),
    )?;
    for name in [
        "GetDisplayBPMs",
        "StringifyDisplayBPMs",
        "GetNotefieldX",
        "GetNotefieldWidth",
        "GetGameplayLayout",
    ] {
        bind(call, globals, name, &[], &metadata)?;
    }
    for name in ["IsW0Judgment", "IsW010Judgment"] {
        constant_field(call, globals, name, false)?;
    }
    constant_field(call, globals, "GetDefaultFailType", "FailType_Immediate")?;
    constant_field(call, globals, "GetComboThreshold", "TapNoteScore_W3")
}
fn selected(call: &mut HostCall<'_>, slot: Value, index: usize) -> Result<Value, Error> {
    let slot = table(call, slot)?;
    let value = call.raw_table_get(slot, Value::Number((index + 1) as f64))?;
    if value != Value::Nil {
        table(call, value)?;
    }
    Ok(value)
}
fn effect(call: &mut HostCall<'_>) -> Result<(), Error> {
    side_effect(call, call.captures()[1], call.captures()[2])
}
fn binding(call: &mut HostCall<'_>) -> Result<(), Error> {
    let binding = call
        .function_data::<Binding>()
        .expect("GameState binding")
        .clone();
    let name = binding.name;
    let meta = &binding.metadata;
    match name {
        "GetCurrentSong" | "GetCurrentStyle" | "GetCurrentSteps" | "GetCurrentTrail" => {
            let index = match name {
                "GetCurrentSteps" => required_player(call, arg(call, 0))?,
                "GetCurrentTrail" => {
                    let Some(i) = player(call, arg(call, 0)) else {
                        return call.push(Value::Nil);
                    };
                    i
                }
                _ => 0,
            };
            let value = selected(call, call.captures()[4], index)?;
            call.push(value)
        }
        "GetPlayerState" => {
            let value = call.arguments().get(1).copied().unwrap_or(Value::Nil);
            call.push(
                player(call, value)
                    .map(|i| call.captures()[i + 4])
                    .unwrap_or(Value::Nil),
            )
        }
        "GetPlayerOptionsString" => {
            let Some(index) = player(
                call,
                call.arguments().first().copied().unwrap_or(Value::Nil),
            ) else {
                let empty = call.try_intern(b"")?;
                return call.push(empty);
            };
            let state = call.captures()[index + 4];
            let method = get(call, state, "GetPlayerOptionsString")?;
            let method = function(call, method)?;
            let level = call.try_intern(b"ModsLevel_Preferred")?;
            let result = call
                .call(method, &[state, level])?
                .first()
                .copied()
                .unwrap_or(Value::Nil);
            let text = string(call, result)?;
            let result = call.try_intern(text.as_bytes())?;
            call.push(result)
        }
        "GetSongOptionsString" => {
            let result = call
                .call(call.captures()[4], &[Value::Number(3.0)])?
                .first()
                .copied()
                .unwrap_or(Value::Nil);
            call.push(result)
        }
        "GetSortOrder" => {
            let slot = table(call, call.captures()[4])?;
            let value = call.raw_table_get(slot, Value::Number(1.0))?;
            let text = if value == Value::Nil {
                "SortOrder_Group".into()
            } else {
                string(call, value)?
            };
            let value = call.try_intern(text.as_bytes())?;
            call.push(value)
        }
        "IsPlayerEnabled" | "IsHumanPlayer" | "IsSideJoined" => {
            let value = if name == "IsHumanPlayer" && call.captures().len() > 4 {
                call.arguments().first().copied().unwrap_or(Value::Nil)
            } else {
                arg(call, 0)
            };
            call.push(Value::Boolean(
                player(call, value).is_some_and(|i| meta.enabled[i]),
            ))
        }
        "IsEventMode" => {
            let prefs = get(call, call.captures()[1], "PREFSMAN")?;
            let prefs = table(call, prefs)?;
            let values = raw(call, prefs, "__songlua_preference_values")?;
            let values = table(call, values)?;
            let value = raw(call, values, "EventMode")?;
            call.push(Value::Boolean(truthy(value)))
        }
        "SetCurrentSong" | "SetCurrentStyle" | "SetCurrentTrail" => {
            let slot = table(call, call.captures()[4])?;
            if name == "SetCurrentStyle" {
                let key = text(call, arg(call, 0)).unwrap_or_else(|| "single".into());
                let value = super::utility::style(call, &key)?;
                call.raw_table_set(slot, Value::Number(1.0), Value::Table(value))?;
            } else if name == "SetCurrentSong" {
                if let Value::Table(value) = arg(call, 0) {
                    call.raw_table_set(slot, Value::Number(1.0), Value::Table(value))?;
                }
            } else if let Some(index) = player(call, arg(call, 0))
                && let Value::Table(value) = arg(call, 1)
            {
                call.raw_table_set(slot, Value::Number((index + 1) as f64), Value::Table(value))?;
            }
            effect(call)
        }
        "InsertCoin" | "SaveProfiles" => effect(call),
        "AddStageToPlayer"
        | "ResetPlayerOptions"
        | "SetPreferredDifficulty"
        | "UnjoinPlayer"
        | "JoinPlayer" => {
            effect(call)?;
            call.push(Value::Boolean(true))
        }
        "getenv" | "setenv" => {
            let key = string(
                call,
                call.arguments().first().copied().unwrap_or(Value::Nil),
            )?;
            let key = call.try_intern(key.as_bytes())?;
            let env = call.captures()[4];
            if name == "getenv" {
                let value = call.table_get(env, key)?;
                call.push(value)
            } else {
                let value = call.arguments().get(1).copied().unwrap_or(Value::Nil);
                call.table_set(env, key, value)
            }
        }
        "GetSongAndSteps" => {
            let index = player(
                call,
                call.arguments().first().copied().unwrap_or(Value::Nil),
            )
            .unwrap_or(0);
            let song = selected(call, call.captures()[4], 0)?;
            call.push(song)?;
            let steps = selected(call, call.captures()[5], index)?;
            call.push(steps)
        }
        "currentTimeSongOrCourse" => {
            let value = get(
                call,
                call.captures()[4],
                crate::SONG_LUA_RUNTIME_SECONDS_KEY,
            )?;
            let number = super::song_options::number(call, value, call.captures()[2])?;
            call.push(Value::Number(number))
        }
        _ => helpers::binding(call, name, meta),
    }
}
