//! Existing game, character, memory-card, unlock and platform-hook policies.
use super::catalog::{arg, integer};
use super::fields::{Field, object, set};
use super::*;
mod colors;

pub(super) fn install(vm: &mut Vm) -> Result<(), Error> {
    colors::install(vm)?;
    let code = deadlib_lua::compile_named(
        b"return function(t) local n = 0; return function() n = n + 1; return t[n] end end",
        b"=DeadSync ivalues",
    )?;
    let values = vm.execute(&code)?[0];
    vm.set_global(b"ivalues", values)?;
    let short = vm.new_host_function(enum_short, &[])?;
    vm.set_global(b"ToEnumShortString", short)?;
    vm.set_global(b"pname", short)?;
    let captures = [vm.get_global(b"_G")?, vm.get_global(b"tonumber")?];
    let initialize = vm.new_host_function(initialize, &captures)?;
    vm.call(initialize, &[]).map(|_| ())
}

fn enum_short(call: &mut HostCall<'_>) -> Result<(), Error> {
    let value = call.arguments().first().copied().unwrap_or(Value::Nil);
    let id = call
        .coerce_string(value)?
        .ok_or_else(|| call.error(b"string expected"))?;
    let value = call.string_bytes(id).expect("live string");
    let value = std::str::from_utf8(value).map_err(|_| call.error(b"invalid UTF-8 string"))?;
    let short = value
        .split_once('_')
        .map_or(value, |(_, short)| short)
        .as_bytes();
    let short = smallvec::SmallVec::<[u8; 64]>::from_slice(short);
    let short = call.try_intern(&short)?;
    call.push(short)
}

fn value_method(
    call: &mut HostCall<'_>,
    owner: TableId,
    name: &str,
    value: impl Field,
) -> Result<(), Error> {
    let value = value.value(call)?;
    let method = call.new_host_function(constant, &[value])?;
    set(owner, name, method, call)
}

fn empty_method(
    call: &mut HostCall<'_>,
    owner: TableId,
    name: &str,
    count: usize,
) -> Result<(), Error> {
    let method = call.new_host_function_with_data(empty_tables, &[], count)?;
    set(owner, name, method, call)
}

fn effect_method(
    call: &mut HostCall<'_>,
    owner: TableId,
    name: &str,
    result: Value,
) -> Result<(), Error> {
    let captures = [result, call.captures()[0], call.captures()[1]];
    let method = call.new_host_function(effect, &captures)?;
    set(owner, name, method, call)
}

fn initialize(call: &mut HostCall<'_>) -> Result<(), Error> {
    let globals = table(call, call.captures()[0])?;
    for (name, value) in [
        ("ProductFamily", crate::SONG_LUA_PRODUCT_FAMILY),
        ("ProductID", crate::SONG_LUA_PRODUCT_ID),
        ("ProductVersion", crate::SONG_LUA_PRODUCT_VERSION),
    ] {
        value_method(call, globals, name, value)?;
    }
    let gameman = object(call, |call, owner| {
        let method = call.new_host_function(styles, &[])?;
        set(owner, "GetStylesForGame", method, call)
    })?;
    set(globals, "GAMEMAN", gameman, call)?;
    let charman = object(call, |call, owner| {
        empty_method(call, owner, "GetAllCharacters", 1)?;
        value_method(call, owner, "GetCharacterCount", 0_i32)?;
        for name in ["GetCharacter", "GetDefaultCharacter", "GetRandomCharacter"] {
            value_method(call, owner, name, Value::Nil)?;
        }
        Ok(())
    })?;
    set(globals, "CHARMAN", charman, call)?;
    let memcardman = object(call, |call, owner| {
        value_method(call, owner, "GetCardState", "MemoryCardState_none")?;
        value_method(call, owner, "GetName", "")?;
        for name in ["MountCard", "UnmountCard"] {
            effect_method(call, owner, name, Value::Boolean(false))?;
        }
        Ok(())
    })?;
    set(globals, "MEMCARDMAN", memcardman, call)?;
    let unlockman = object(call, |call, owner| {
        for name in [
            "GetNumUnlocks",
            "GetNumUnlocked",
            "GetPoints",
            "GetPointsForProfile",
            "GetPointsUntilNextUnlock",
            "IsSongLocked",
            "IsCourseLocked",
            "IsStepsLocked",
        ] {
            value_method(call, owner, name, 0_i32)?;
        }
        value_method(call, owner, "AnyUnlocksToCelebrate", false)?;
        value_method(call, owner, "GetUnlockEntryIndexToCelebrate", -1_i32)?;
        for name in ["FindEntryID", "GetUnlockEntry"] {
            value_method(call, owner, name, Value::Nil)?;
        }
        empty_method(call, owner, "GetSongsUnlockedByEntryID", 1)?;
        empty_method(call, owner, "GetStepsUnlockedByEntryID", 2)?;
        for name in [
            "PreferUnlockEntryID",
            "UnlockEntryID",
            "UnlockEntryIndex",
            "LockEntryID",
            "LockEntryIndex",
        ] {
            effect_method(call, owner, name, Value::Table(owner))?;
        }
        Ok(())
    })?;
    set(globals, "UNLOCKMAN", unlockman, call)?;
    let hooks = object(call, |call, owner| {
        value_method(call, owner, "GetArchName", crate::song_lua_arch_name())?;
        value_method(call, owner, "GetClipboard", "")?;
        for name in ["SetClipboard", "OpenFile", "OpenURL", "RestartProgram"] {
            effect_method(call, owner, name, Value::Boolean(false))?;
        }
        Ok(())
    })?;
    set(globals, "HOOKS", hooks, call)
}

fn empty_tables(call: &mut HostCall<'_>) -> Result<(), Error> {
    let count = *call.function_data::<usize>().expect("result count");
    for _ in 0..count {
        let result = call.try_new_table()?;
        call.push(Value::Table(result))?;
    }
    Ok(())
}

fn effect(call: &mut HostCall<'_>) -> Result<(), Error> {
    side_effect(call, call.captures()[1], call.captures()[2])?;
    call.push(call.captures()[0])
}

fn styles(call: &mut HostCall<'_>) -> Result<(), Error> {
    // The production host returns a fresh single-style list for every request,
    // independent of the requested game or the session's current style.
    let result = object(call, |call, result| {
        let style = style(call, "single")?;
        set(result, 1_i32, style, call)
    })?;
    call.push(Value::Table(result))
}

pub(super) fn style(call: &mut HostCall<'_>, key: &str) -> Result<TableId, Error> {
    let info = crate::song_lua_style_info(key);
    object(call, |call, owner| {
        set(owner, "__songlua_style_key", key, call)?;
        value_method(call, owner, "GetName", info.name)?;
        value_method(call, owner, "GetStepsType", info.steps_type)?;
        value_method(call, owner, "GetStyleType", info.style_type)?;
        value_method(call, owner, "ColumnsPerPlayer", info.columns)?;
        value_method(call, owner, "GetWidth", info.width)?;
        let method = call.new_host_function_with_data(column, &[], info)?;
        set(owner, "GetColumnInfo", method, call)
    })
}

fn column(call: &mut HostCall<'_>) -> Result<(), Error> {
    let info = *call
        .function_data::<crate::SongLuaStyleInfo>()
        .expect("style metadata");
    let index = integer(call, arg(call, 1))
        .unwrap_or(1)
        .clamp(1, info.columns as i32) as usize
        - 1;
    let result = object(call, |call, result| {
        set(
            result,
            "Name",
            crate::song_lua_style_column_name(index),
            call,
        )?;
        set(result, "Track", index, call)?;
        set(
            result,
            "XOffset",
            crate::song_lua_style_column_x(info.name, index),
            call,
        )
    })?;
    call.push(Value::Table(result))
}
