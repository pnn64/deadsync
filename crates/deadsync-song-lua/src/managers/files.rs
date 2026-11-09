//! Existing file queries and sandboxed mutation stubs used by song/theme Lua.
use super::catalog::{arg, text};
use super::fields::{object, set};
use super::*;
use std::path::PathBuf;
mod rage;

#[derive(Clone, Copy)]
enum Operation {
    Listing,
    Exists,
    Size,
    Hash,
    Effect(Option<bool>),
}

pub(super) fn install(vm: &mut Vm, context: &crate::SongLuaCompileContext) -> Result<(), Error> {
    let globals = vm.get_global(b"_G")?;
    let initialize =
        vm.new_host_function_with_data(initialize, &[globals], context.song_dir.clone())?;
    vm.call(initialize, &[])?;
    rage::install(vm, &context.song_dir)?;
    let code = deadlib_lua::compile_named(
        include_bytes!("get_combo_fonts.lua"),
        b"Simply Love/Scripts/SL-Helpers.lua:GetComboFonts",
    )?;
    let function = vm.execute(&code)?[0];
    vm.set_global(b"GetComboFonts", function)
}

fn initialize(call: &mut HostCall<'_>) -> Result<(), Error> {
    let globals = table(call, call.captures()[0])?;
    let song_dir = call.function_data::<PathBuf>().unwrap().clone();
    let fileman = object(call, |call, owner| {
        for (name, operation) in [
            ("GetDirListing", Operation::Listing),
            ("DoesFileExist", Operation::Exists),
            ("GetFileSizeBytes", Operation::Size),
            ("GetHashForFile", Operation::Hash),
            ("Copy", Operation::Effect(Some(false))),
            ("CreateDir", Operation::Effect(Some(true))),
            ("Remove", Operation::Effect(Some(true))),
            ("Unzip", Operation::Effect(Some(false))),
            ("FlushDirCache", Operation::Effect(None)),
        ] {
            let captures = if matches!(operation, Operation::Effect(_)) {
                &[Value::Table(globals)][..]
            } else {
                &[]
            };
            let function = call.new_host_function_with_data(
                dispatch,
                captures,
                (operation, song_dir.clone()),
            )?;
            set(owner, name, function, call)?;
        }
        Ok(())
    })?;
    set(globals, "FILEMAN", fileman, call)?;
    let strip = call.new_host_function(strip_sprite_hints, &[])?;
    set(globals, "StripSpriteHints", strip, call)
}

fn boolish(call: &HostCall<'_>, value: Value) -> bool {
    match value {
        Value::Boolean(value) => value,
        Value::Number(value) => value != 0.,
        Value::String(_) => text(call, value).is_some_and(|value| {
            let value = value.trim();
            if ["true", "yes", "on"]
                .iter()
                .any(|s| value.eq_ignore_ascii_case(s))
            {
                true
            } else if ["false", "no", "off"]
                .iter()
                .any(|s| value.eq_ignore_ascii_case(s))
            {
                false
            } else {
                value.parse::<f32>().is_ok_and(|value| value != 0.)
            }
        }),
        _ => false,
    }
}

fn dispatch(call: &mut HostCall<'_>) -> Result<(), Error> {
    let (operation, song_dir) = call
        .function_data::<(Operation, PathBuf)>()
        .unwrap()
        .clone();
    let value = match operation {
        Operation::Listing => {
            let result = object(call, |call, owner| {
                let Some(path) = text(call, arg(call, 0)) else {
                    return Ok(());
                };
                let entries = crate::fileman_dir_listing(
                    &song_dir,
                    &path,
                    boolish(call, arg(call, 1)),
                    boolish(call, arg(call, 2)),
                )
                .map_err(|error| call.error(error.to_string().as_bytes()))?;
                for (index, entry) in entries.into_iter().enumerate() {
                    set(owner, index + 1, entry, call)?;
                }
                Ok(())
            })?;
            Value::Table(result)
        }
        Operation::Exists => Value::Boolean(
            text(call, arg(call, 0))
                .is_some_and(|path| crate::resolve_compat_path(&song_dir, &path).exists()),
        ),
        Operation::Size => {
            let size = text(call, arg(call, 0))
                .and_then(|path| crate::resolve_compat_path(&song_dir, &path).metadata().ok())
                .filter(std::fs::Metadata::is_file)
                .map(|metadata| metadata.len().min(i64::MAX as u64) as i64)
                .unwrap_or(0);
            Value::Number(size as f64)
        }
        Operation::Hash => Value::Number(0.),
        Operation::Effect(result) => {
            // These methods record a side effect but never mutate the filesystem.
            // Use the native number conversion without calling authored tonumber.
            let globals = call.captures()[0];
            let mut stack = call.native_stack()?;
            stack.push(globals)?;
            let globals = stack.top() as isize;
            stack.get_field(globals, crate::SONG_LUA_SIDE_EFFECT_COUNT_KEY.as_bytes())?;
            let count = if stack.value(-1) == Some(Value::Nil) {
                0
            } else {
                let value = stack.number(-1)?;
                if !(i64::MIN as f64..-(i64::MIN as f64)).contains(&value) {
                    return Err(stack.error(b"integer out of range"));
                }
                value as i64
            };
            stack.push_bytes(crate::SONG_LUA_SIDE_EFFECT_COUNT_KEY.as_bytes())?;
            stack.push(Value::Number(count.saturating_add(1) as f64))?;
            stack.set_table(globals, false)?;
            if let Some(value) = result {
                stack.push(Value::Boolean(value))?;
            }
            return stack.finish(usize::from(result.is_some()));
        }
    };
    call.push(value)
}

fn strip_sprite_hints(call: &mut HostCall<'_>) -> Result<(), Error> {
    let value = call.arguments().first().copied().unwrap_or(Value::Nil);
    let id = call
        .coerce_string(value)?
        .ok_or_else(|| call.error(b"string expected"))?;
    let value = std::str::from_utf8(call.string_bytes(id).expect("live string"))
        .map_err(|_| call.error(b"UTF-8 string expected"))?;
    let value = crate::strip_sprite_hints(value);
    let value = call.try_intern(value.as_bytes())?;
    call.push(value)
}
