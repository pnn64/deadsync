//! PlayerOptions objects. Group propagation and PlayerState are installed separately.
use super::catalog::{arg, float, raw, text};
use super::fields::{object, set};
use super::song_options::{enum_index, number};
use super::*;
use crate::player_options::{
    is_player_option_method_name, player_option_default_string, player_option_uses_bool,
};
#[cfg(feature = "test-support")]
use crate::rust_engine::observations;
pub(super) mod game;
mod groups;
mod levels;
mod parser;
mod scalars;
mod speed;
mod state;

pub(in crate::rust_engine) use state::install as install_states;

const STATE: &str = "__songlua_player_option_state";
const SPEEDS: &str = "__songlua_player_option_speeds";
const WINDOWS: &str = "__songlua_disabled_timing_windows";
const SKIN: &str = "__songlua_noteskin_name";
const SKIN_OVERRIDE: &str = "__songlua_noteskin_override";

pub(super) fn install(
    call: &mut HostCall<'_>,
    catalog: TableId,
    context: &crate::SongLuaCompileContext,
    #[cfg(feature = "test-support")] observations: observations::Observations,
) -> Result<(), Error> {
    let globals = call.captures()[0];
    let tonumber = call.captures()[1];
    let next = get(call, globals, "next")?;
    let setmeta = get(call, globals, "setmetatable")?;
    call.with_roots(&[next, setmeta], |call| {
        let players = object(call, |call, players| {
            for (index, player) in context.players.iter().enumerate() {
                let levels = object(call, |call, levels| {
                    for level in 1..=4 {
                        let owner = create(
                            call,
                            player,
                            [globals, tonumber, next],
                            setmeta,
                            #[cfg(feature = "test-support")]
                            observations.clone(),
                        )?;
                        set(levels, level, owner, call)?;
                    }
                    Ok(())
                })?;
                set(players, index + 1, levels, call)?;
            }
            Ok(())
        })?;
        set(catalog, "player_options", players, call)?;
        groups::install(
            call,
            catalog,
            players,
            [Value::Nil, globals, tonumber, next],
        )
    })
}

fn create(
    call: &mut HostCall<'_>,
    player: &crate::SongLuaPlayerContext,
    shared: [Value; 3],
    setmeta: Value,
    #[cfg(feature = "test-support")] observations: observations::Observations,
) -> Result<TableId, Error> {
    object(call, |call, owner| {
        let state = nested(call, owner, STATE)?;
        set(state, "playerautoplay", player.autoplay, call)?;
        set(state, "mini", player.mini, call)?;
        if player.perspective.tilt != 0.0 || player.perspective.skew != 0.0 {
            set(state, "tilt", player.perspective.tilt, call)?;
            set(state, "skew", player.perspective.skew, call)?;
        }
        set(
            owner,
            "__songlua_reference_bpm",
            player.display_bpms[1].max(player.display_bpms[0]).max(1.0),
            call,
        )?;
        let captures = [Value::Table(owner), shared[0], shared[1], shared[2]];
        for name in [
            "CMod",
            "CAMod",
            "MMod",
            "AMod",
            "XMod",
            "FromString",
            "Mirror",
            "Left",
            "Right",
            "Reverse",
            "Mini",
            "Skew",
            "Tilt",
            "IsEasierForSongAndSteps",
            "IsEasierForCourseAndTrail",
            "GetReversePercentForColumn",
            "UsingReverse",
            "GetStepAttacks",
            "DisableTimingWindow",
            "ResetDisabledTimingWindows",
            "GetDisabledTimingWindows",
            "NoteSkin",
        ] {
            let method = match name {
                "AMod" => Method::OtherSpeed {
                    key: "amod",
                    initial: if let crate::SongLuaSpeedMod::A(value) = player.speedmod {
                        Some(value)
                    } else {
                        None
                    },
                },
                "CAMod" => Method::OtherSpeed {
                    key: "camod",
                    initial: None,
                },
                "CMod" | "MMod" | "XMod" => Method::Speed(name.to_ascii_lowercase()),
                "FromString" => Method::FromString,
                "IsEasierForSongAndSteps" | "IsEasierForCourseAndTrail" => Method::Easier,
                "GetReversePercentForColumn" => Method::ReverseColumn,
                "UsingReverse" => Method::Reverse,
                "GetStepAttacks" => Method::Attacks,
                "DisableTimingWindow" => Method::Disable,
                "ResetDisabledTimingWindows" => Method::ResetWindows,
                "GetDisabledTimingWindows" => Method::Windows,
                "NoteSkin" => Method::Skin,
                _ => Method::Scalar(name.to_ascii_lowercase()),
            };
            let method = method_function(
                call,
                &captures,
                method,
                #[cfg(feature = "test-support")]
                observations.clone(),
            )?;
            set(owner, name, method, call)?;
        }
        let (key, value) = match player.speedmod {
            crate::SongLuaSpeedMod::X(v) => ("xmod", v),
            crate::SongLuaSpeedMod::C(v) => ("cmod", v),
            crate::SongLuaSpeedMod::M(v) => ("mmod", v),
            crate::SongLuaSpeedMod::A(v) => ("amod", v),
        };
        speed::set_mode(call, owner, key, Some(value))?;
        if key == "xmod" {
            set(owner, "__songlua_speedmod_explicit", value != 1.0, call)?;
        }
        set(owner, SKIN, player.noteskin_name.as_str(), call)?;
        let meta = object(call, |call, meta| {
            let index = call.new_host_function_with_data(
                index,
                &captures,
                Index {
                    #[cfg(feature = "test-support")]
                    observations: observations.clone(),
                },
            )?;
            set(meta, "__index", index, call)
        })?;
        call.call(setmeta, &[Value::Table(owner), Value::Table(meta)])?;
        Ok(())
    })
}

#[derive(Clone)]
enum Method {
    Scalar(String),
    Speed(String),
    OtherSpeed {
        key: &'static str,
        initial: Option<f32>,
    },
    FromString,
    Easier,
    ReverseColumn,
    Reverse,
    Attacks,
    Disable,
    ResetWindows,
    Windows,
    Skin,
}
#[derive(Clone)]
struct Binding {
    method: Method,
    #[cfg(feature = "test-support")]
    observations: observations::Observations,
}
#[derive(Clone)]
struct Index {
    #[cfg(feature = "test-support")]
    observations: observations::Observations,
}
#[cfg(feature = "test-support")]
fn audit(call: &HostCall<'_>) -> observations::Observations {
    call.function_data::<Binding>()
        .unwrap()
        .observations
        .clone()
}
fn method_function(
    call: &mut HostCall<'_>,
    captures: &[Value],
    method: Method,
    #[cfg(feature = "test-support")] observations: observations::Observations,
) -> Result<Value, Error> {
    let global = cfg!(feature = "test-support")
        || matches!(
            method,
            Method::FromString
                | Method::ReverseColumn
                | Method::Disable
                | Method::ResetWindows
                | Method::Skin
        );
    let owner = !matches!(method, Method::Easier | Method::Skin);
    let captures = [
        if owner { captures[0] } else { Value::Nil },
        if global { captures[1] } else { Value::Nil },
        captures[2],
        if matches!(method, Method::FromString) {
            captures[3]
        } else {
            Value::Nil
        },
    ];
    call.new_host_function_with_data(
        binding,
        &captures,
        Binding {
            method,
            #[cfg(feature = "test-support")]
            observations,
        },
    )
}
fn index(call: &mut HostCall<'_>) -> Result<(), Error> {
    let name = call.arguments().get(1).copied().and_then(|v| text(call, v));
    let Some(name) = name.filter(|name| is_player_option_method_name(name)) else {
        return call.push(Value::Nil);
    };
    let captures = call.captures().to_vec();
    // Recreated lazy methods always follow the native generic-method path,
    // even when Lua has deleted a normally eager special method.
    #[cfg(feature = "test-support")]
    let data = call.function_data::<Index>().unwrap().clone();
    let method = method_function(
        call,
        &captures,
        Method::Scalar(name.to_ascii_lowercase()),
        #[cfg(feature = "test-support")]
        data.observations,
    )?;
    set(table(call, captures[0])?, name, method, call)?;
    call.push(method)
}

fn nested(call: &mut HostCall<'_>, owner: TableId, key: &str) -> Result<TableId, Error> {
    match raw(call, owner, key)? {
        Value::Nil => {
            let value = call.try_new_table()?;
            set(owner, key, value, call)?;
            Ok(value)
        }
        value => table(call, value),
    }
}
fn get(call: &mut HostCall<'_>, owner: Value, key: &str) -> Result<Value, Error> {
    let key = call.try_intern(key.as_bytes())?;
    call.table_get(owner, key)
}
fn put(call: &mut HostCall<'_>, owner: TableId, key: &str, value: Value) -> Result<(), Error> {
    call.with_roots(&[value], |call| {
        let key = call.try_intern(key.as_bytes())?;
        call.table_set(Value::Table(owner), key, value)
    })
}
fn typed_float(call: &mut HostCall<'_>, value: Value, default: f32) -> Result<f32, Error> {
    if value == Value::Nil {
        Ok(default)
    } else {
        Ok(number(call, value, call.captures()[2])? as f32)
    }
}
fn raw_float(
    call: &mut HostCall<'_>,
    owner: TableId,
    key: &str,
    default: f32,
) -> Result<f32, Error> {
    let value = raw(call, owner, key)?;
    typed_float(call, value, default)
}
fn get_float(
    call: &mut HostCall<'_>,
    owner: TableId,
    key: &str,
    default: f32,
) -> Result<f32, Error> {
    let value = get(call, Value::Table(owner), key)?;
    typed_float(call, value, default)
}
fn truthy(value: Value) -> bool {
    !matches!(value, Value::Nil | Value::Boolean(false))
}
fn chained(call: &HostCall<'_>) -> bool {
    call.arguments().last() == Some(&Value::Boolean(true))
}
fn string(call: &mut HostCall<'_>, value: Value) -> Result<String, Error> {
    let id = call
        .coerce_string(value)?
        .ok_or_else(|| call.error(b"string expected"))?;
    text(call, Value::String(id)).ok_or_else(|| call.error(b"invalid UTF-8 string"))
}
fn function(call: &HostCall<'_>, value: Value) -> Result<Value, Error> {
    if matches!(value, Value::Function(_) | Value::Native(_)) {
        Ok(value)
    } else {
        Err(call.error(b"function expected"))
    }
}
fn default_value(call: &mut HostCall<'_>, name: &str) -> Result<Value, Error> {
    if name == "failsetting" {
        Ok(Value::Number(0.0))
    } else if player_option_uses_bool(name) {
        Ok(Value::Boolean(false))
    } else if let Some(value) = player_option_default_string(name) {
        call.try_intern(value.as_bytes())
    } else {
        Ok(Value::Number(0.0))
    }
}
fn effect(call: &mut HostCall<'_>) -> Result<(), Error> {
    side_effect(call, call.captures()[1], call.captures()[2])
}
fn skin_name(call: &mut HostCall<'_>, owner: TableId) -> Result<String, Error> {
    let value = raw(call, owner, SKIN)?;
    let name = if value == Value::Nil {
        String::new()
    } else {
        string(call, value)?
    };
    Ok(if name.is_empty() {
        crate::SONG_LUA_DEFAULT_NOTESKIN_NAME.into()
    } else {
        name
    })
}
fn skin_exists(call: &mut HostCall<'_>, name: &str) -> Result<bool, Error> {
    let skins = get(call, call.captures()[1], "NOTESKIN")?;
    table(call, skins)?;
    call.with_roots(&[skins], |call| {
        let method = get(call, skins, "DoesNoteSkinExist")?;
        let method = function(call, method)?;
        let name = call.try_intern(name.as_bytes())?;
        let result = call
            .call(method, &[skins, name])?
            .first()
            .copied()
            .unwrap_or(Value::Nil);
        Ok(truthy(result))
    })
}

fn binding(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call
        .function_data::<Binding>()
        .expect("player option method")
        .clone();
    let method = data.method;
    if matches!(method, Method::Easier) {
        return call.push(Value::Boolean(false));
    }
    if matches!(method, Method::Skin) {
        return skin(call);
    }
    let owner = table(call, call.captures()[0])?;
    match method {
        Method::Scalar(key) => scalars::invoke(call, owner, &key),
        Method::Speed(key) => speed::mode(call, owner, &key),
        Method::OtherSpeed { key, initial } => speed::other(call, owner, key, initial),
        Method::FromString => {
            if let Some(value) = text(call, arg(call, 0)) {
                #[cfg(feature = "test-support")]
                let previous = skin_name(call, owner)?;
                #[cfg(feature = "test-support")]
                let before = audit(call).speed_before(call, call.captures()[1], owner)?;
                parser::apply(call, owner, &value)?;
                #[cfg(feature = "test-support")]
                audit(call).speed(
                    call,
                    call.captures()[1],
                    owner,
                    observations::Speed {
                        key: "fromstring",
                        before,
                        failed: false,
                        chained: true,
                    },
                )?;
                #[cfg(feature = "test-support")]
                audit(call).skin(call, call.captures()[1], owner, "fromstring", previous)?;
            }
            call.push(Value::Table(owner))
        }
        Method::ReverseColumn => reverse_column(call, owner),
        Method::Reverse | Method::Attacks => {
            let value = if matches!(method, Method::Reverse) {
                Value::Boolean(option_number(call, owner, "reverse")? == 1.0)
            } else {
                Value::Number(f64::from(
                    option_number(call, owner, "noattack")? <= 0.0
                        && option_number(call, owner, "randattack")? <= 0.0,
                ))
            };
            call.push(value)
        }
        Method::Disable | Method::ResetWindows | Method::Windows => windows(call, owner, method),
        _ => unreachable!(),
    }
}
fn option_number(call: &mut HostCall<'_>, owner: TableId, key: &str) -> Result<f32, Error> {
    let state = nested(call, owner, STATE)?;
    call.with_roots(&[Value::Table(state)], |call| {
        get_float(call, state, key, 0.0)
    })
}
fn skin(call: &mut HostCall<'_>) -> Result<(), Error> {
    let Some(Value::Table(owner)) = call.arguments().first().copied() else {
        return Ok(());
    };
    let previous = skin_name(call, owner)?;
    let mut accepted = Value::Nil;
    if let Some(value) = call.coerce_string(arg(call, 0))? {
        let name = string(call, Value::String(value))?;
        if skin_exists(call, &name)? {
            set(owner, SKIN_OVERRIDE, name.as_str(), call)?;
            set(owner, SKIN, name, call)?;
            accepted = Value::Boolean(true);
            effect(call)?;
        }
        #[cfg(feature = "test-support")]
        audit(call).skin(
            call,
            call.captures()[1],
            owner,
            "noteskin",
            previous.clone(),
        )?;
    }
    if call.arguments().len() > 1 && chained(call) {
        return call.push(Value::Table(owner));
    }
    let previous = call.try_intern(previous.as_bytes())?;
    call.push(previous)?;
    call.push(accepted)
}
fn windows(call: &mut HostCall<'_>, owner: TableId, method: Method) -> Result<(), Error> {
    if matches!(method, Method::Windows) {
        let disabled = nested(call, owner, WINDOWS)?;
        return call.with_roots(&[Value::Table(disabled)], |call| {
            let out = object(call, |call, out| {
                let mut i = 1;
                for name in crate::SONG_LUA_TIMING_WINDOW_NAMES {
                    if truthy(get(call, Value::Table(disabled), name)?) {
                        set(out, i, name, call)?;
                        i += 1;
                    }
                }
                Ok(())
            })?;
            call.push(Value::Table(out))
        });
    }
    if matches!(method, Method::ResetWindows) {
        let disabled = call.try_new_table()?;
        set(owner, WINDOWS, disabled, call)?;
        effect(call)?;
    } else if arg(call, 0) != Value::Nil {
        const NAMES: [&str; 10] = [
            "TimingWindow_W1",
            "TimingWindow_W2",
            "TimingWindow_W3",
            "TimingWindow_W4",
            "TimingWindow_W5",
            "TimingWindow_Mine",
            "TimingWindow_Attack",
            "TimingWindow_Hold",
            "TimingWindow_Roll",
            "TimingWindow_Checkpoint",
        ];
        let index = enum_index(call, arg(call, 0), &NAMES, "TimingWindow")?;
        let disabled = nested(call, owner, WINDOWS)?;
        put(call, disabled, NAMES[index], Value::Boolean(true))?;
        effect(call)?;
    }
    call.push(if chained(call) {
        Value::Table(owner)
    } else {
        arg(call, call.arguments().len().saturating_sub(2))
    })
}
pub(super) fn current_style(call: &mut HostCall<'_>) -> String {
    let style = (|| {
        let state = get(call, call.captures()[1], "GAMESTATE")?;
        let state = table(call, state)?;
        let holder = raw(call, state, "__songlua_current_style")?;
        let holder = table(call, holder)?;
        call.raw_table_get(holder, Value::Number(1.0))
    })()
    .or_else(|_: Error| {
        let state = get(call, call.captures()[1], "GAMESTATE")?;
        if state == Value::Nil {
            return Ok(Value::Nil);
        }
        table(call, state)?;
        call.with_roots(&[state], |call| {
            let method = get(call, state, "GetCurrentStyle")?;
            if method == Value::Nil {
                return Ok(Value::Nil);
            }
            let method = function(call, method)?;
            Ok(call
                .call(method, &[state])?
                .first()
                .copied()
                .unwrap_or(Value::Nil))
        })
    });
    let Ok(Value::Table(style)) = style else {
        return "single".into();
    };
    call.with_roots(&[Value::Table(style)], |call| {
        if let Ok(value) = raw(call, style, "__songlua_style_key")
            && value != Value::Nil
            && let Ok(name) = string(call, value)
        {
            return Ok(name);
        }
        let method = get(call, Value::Table(style), "GetName")?;
        let method = function(call, method)?;
        let value = call
            .call(method, &[Value::Table(style)])?
            .first()
            .copied()
            .unwrap_or(Value::Nil);
        Ok(text(call, value).unwrap_or_else(|| "single".into()))
    })
    .unwrap_or_else(|_| "single".into())
}
fn reverse_column(call: &mut HostCall<'_>, owner: TableId) -> Result<(), Error> {
    let col = float(call, arg(call, 0)).unwrap_or(0.0) as i32;
    let count = crate::song_lua_style_info(&current_style(call)).columns as i32;
    if col < 0 || col > count {
        return call.push(Value::Nil);
    }
    // Native resolves the state separately for each term, including after reentry.
    let mut read = |key: &str| {
        let state = nested(call, owner, STATE)?;
        call.with_roots(&[Value::Table(state)], |call| {
            get_float(call, state, key, 0.0)
        })
    };
    let mut value = read("reverse")? + read(&format!("reverse{}", col + 1))?;
    if col >= count / 2 {
        value += read("split")?;
    }
    if col % 2 == 1 {
        value += read("alternate")?;
    }
    if (count / 4..=count - 1 - count / 4).contains(&col) {
        value += read("cross")?;
    }
    if value > 2.0 {
        value %= 2.0;
    }
    if value > 1.0 {
        value = 2.0 - value;
    }
    call.push(Value::Number(value as f64))
}
