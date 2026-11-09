//! The existing Var fallback, including its dynamically resolved GameCommand.
use super::fields::{object, set};
use super::*;

pub(super) fn install(vm: &mut Vm) -> Result<(), Error> {
    let captures = [vm.get_global(b"_G")?, vm.get_global(b"tonumber")?];
    let var = vm.new_host_function(variable, &captures)?;
    vm.set_global(b"Var", var)
}

fn variable(call: &mut HostCall<'_>) -> Result<(), Error> {
    let value = call.arguments().first().copied().unwrap_or(Value::Nil);
    let Value::String(id) = value else {
        let empty = call.try_intern(b"")?;
        return call.push(empty);
    };
    let name = call.string_bytes(id).expect("live variable name");
    if std::str::from_utf8(name).is_err() {
        let empty = call.try_intern(b"")?;
        return call.push(empty);
    }
    if name != b"GameCommand" {
        return call.push(value);
    }
    let command = object(call, |call, owner| {
        for (name, value) in [
            ("GetIndex", Value::Number(0.)),
            ("GetMultiPlayer", Value::Number(-1.)),
            ("GetCharacter", Value::Nil),
            ("GetUrl", Value::Nil),
        ] {
            value_method(call, owner, name, value)?;
        }
        for name in [
            "GetText",
            "GetName",
            "GetScreen",
            "GetIcon",
            "GetProfileID",
            "GetAnnouncer",
            "GetPreferredModifiers",
            "GetStageModifiers",
        ] {
            let empty = call.try_intern(b"")?;
            value_method(call, owner, name, empty)?;
        }
        for (name, value) in [
            ("GetDifficulty", "Difficulty_Invalid"),
            ("GetCourseDifficulty", "Difficulty_Invalid"),
            ("GetPlayMode", "PlayMode_Invalid"),
            ("GetSortOrder", "SortOrder_Invalid"),
        ] {
            let value = call.try_intern(value.as_bytes())?;
            value_method(call, owner, name, value)?;
        }
        for (name, getter, player, group) in [
            ("GetStyle", "GetCurrentStyle", false, false),
            ("GetSong", "GetCurrentSong", false, false),
            ("GetSteps", "GetCurrentSteps", true, false),
            ("GetCourse", "GetCurrentCourse", false, false),
            ("GetTrail", "GetCurrentTrail", true, false),
            ("GetSongGroup", "GetCurrentSong", false, true),
        ] {
            let callback = call.new_host_function_with_data(
                current,
                &[call.captures()[0]],
                (getter, player, group),
            )?;
            set(owner, name, callback, call)?;
        }
        let captures = call.captures().to_vec();
        let apply = call.new_host_function(apply, &captures)?;
        set(owner, "ApplyToStyle", apply, call)
    })?;
    call.push(Value::Table(command))
}

fn value_method(
    call: &mut HostCall<'_>,
    owner: TableId,
    name: &str,
    value: Value,
) -> Result<(), Error> {
    let function = call.new_host_function(constant, &[value])?;
    set(owner, name, function, call)
}
fn apply(call: &mut HostCall<'_>) -> Result<(), Error> {
    super::side_effect(call, call.captures()[0], call.captures()[1])
}
fn optional_function(call: &HostCall<'_>, value: Value) -> Result<Option<Value>, Error> {
    match value {
        Value::Nil => Ok(None),
        Value::Function(_) | Value::Native(_) => Ok(Some(value)),
        _ => Err(call.error(b"function expected")),
    }
}
fn current(call: &mut HostCall<'_>) -> Result<(), Error> {
    let (getter, player, group) = *call.function_data::<(&'static str, bool, bool)>().unwrap();
    let state = super::super::field(call, call.captures()[0], "GAMESTATE")?;
    let value = if state == Value::Nil {
        Value::Nil
    } else {
        table(call, state)?;
        call.with_roots(&[state], |call| {
            let method = super::super::field(call, state, getter)?;
            if let Some(method) = optional_function(call, method)? {
                let mut args = vec![state];
                if player {
                    args.push(call.try_intern(b"PlayerNumber_P1")?);
                }
                Ok(call
                    .call(method, &args)?
                    .first()
                    .copied()
                    .unwrap_or(Value::Nil))
            } else {
                Ok(Value::Nil)
            }
        })?
    };
    let value = if group {
        if matches!(value, Value::Table(_)) {
            call.with_roots(&[value], |call| {
                let method = super::super::field(call, value, "GetGroupName")?;
                if let Some(method) = optional_function(call, method)? {
                    let result = call
                        .call(method, &[value])?
                        .first()
                        .copied()
                        .unwrap_or(Value::Nil);
                    let id = call
                        .coerce_string(result)?
                        .ok_or_else(|| call.error(b"string expected"))?;
                    let bytes = call.string_bytes(id).unwrap();
                    std::str::from_utf8(bytes).map_err(|e| call.error(e.to_string().as_bytes()))?;
                    Ok(Value::String(id))
                } else {
                    call.try_intern(b"")
                }
            })?
        } else {
            call.try_intern(b"")?
        }
    } else {
        value
    };
    call.push(value)
}
