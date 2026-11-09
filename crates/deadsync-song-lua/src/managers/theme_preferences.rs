//! Theme preference storage and the existing custom preference definitions.
use super::fields::{array, object, set};
use super::*;

#[derive(Clone, Copy)]
enum Operation {
    Get,
    Set,
    Save,
    Init,
    Definitions,
}

pub(super) fn install(vm: &mut Vm) -> Result<(), Error> {
    let prefs = vm.try_new_table()?;
    let prefs_root = vm.pin(Value::Table(prefs))?;
    let store = vm.try_new_table()?;
    let store_root = vm.pin(Value::Table(store))?;
    let result = (|| {
        let globals = vm.get_global(b"_G")?;
        let tonumber = vm.get_global(b"tonumber")?;
        let next = vm.get_global(b"next")?;
        for (name, op) in [
            ("Get", Operation::Get),
            ("Set", Operation::Set),
            ("Save", Operation::Save),
            ("InitAll", Operation::Init),
        ] {
            let captures = [
                if matches!(op, Operation::Save) {
                    Value::Nil
                } else {
                    Value::Table(store)
                },
                globals,
                tonumber,
                next,
            ];
            let f = vm.new_host_function_with_data(callback, &captures, op)?;
            super::super::put(vm, prefs, name, f)?;
        }
        vm.set_global(b"ThemePrefs", Value::Table(prefs))?;
        let custom = vm.try_new_table()?;
        let root = vm.pin(Value::Table(custom))?;
        let result = (|| {
            for (name, op) in [
                ("Get", Operation::Definitions),
                ("Init", Operation::Save),
                ("Validate", Operation::Save),
            ] {
                let f = vm.new_host_function_with_data(
                    callback,
                    &[Value::Nil, globals, tonumber, next],
                    op,
                )?;
                super::super::put(vm, custom, name, f)?;
            }
            vm.set_global(b"SL_CustomPrefs", Value::Table(custom))
        })();
        vm.unpin(root)?;
        result
    })();
    vm.unpin(store_root)?;
    vm.unpin(prefs_root)?;
    result
}
fn arg(call: &HostCall<'_>, index: usize) -> Value {
    let offset = usize::from(matches!(call.arguments().first(), Some(Value::Table(_))));
    call.arguments()
        .get(offset + index)
        .copied()
        .unwrap_or(Value::Nil)
}
fn name(call: &HostCall<'_>) -> Option<String> {
    let Value::String(id) = arg(call, 0) else {
        return None;
    };
    std::str::from_utf8(call.string_bytes(id)?)
        .ok()
        .map(str::to_owned)
}
fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
    let op = *call
        .function_data::<Operation>()
        .expect("theme preference binding");
    if matches!(op, Operation::Set | Operation::Save | Operation::Init) {
        side_effect(call, call.captures()[1], call.captures()[2])?;
    }
    if matches!(op, Operation::Definitions) {
        let defs = definitions(call)?;
        return call.push(Value::Table(defs));
    }
    if matches!(op, Operation::Save) {
        return Ok(());
    }
    if matches!(op, Operation::Init) {
        return init(call);
    }
    let Some(name) = name(call) else {
        return if matches!(op, Operation::Get) {
            call.push(Value::Nil)
        } else {
            Ok(())
        };
    };
    let key = call.try_intern(name.as_bytes())?;
    if matches!(op, Operation::Set) {
        return call.table_set(call.captures()[0], key, arg(call, 1));
    }
    let value = call.table_get(call.captures()[0], key)?;
    let value = if value == Value::Nil {
        default(call, &name)?
    } else {
        value
    };
    call.push(value)
}
fn init(call: &mut HostCall<'_>) -> Result<(), Error> {
    let defs = if call.arguments().len() == 1 {
        call.arguments()[0]
    } else {
        arg(call, 0)
    };
    let Value::Table(_) = defs else { return Ok(()) };
    let store = call.captures()[0];
    let mut previous = Value::Nil;
    loop {
        let values = call.call(call.captures()[3], &[defs, previous])?;
        let key = values.first().copied().unwrap_or(Value::Nil);
        if key == Value::Nil {
            return Ok(());
        }
        let def = values.get(1).copied().unwrap_or(Value::Nil);
        call.with_roots(&[key, def], |call| {
            let id = call
                .coerce_string(key)?
                .ok_or_else(|| call.error(b"preference definition name must be a string"))?;
            if std::str::from_utf8(call.string_bytes(id).expect("live key")).is_err() {
                return Err(call.error(b"preference definition name must be UTF-8"));
            }
            super::table(call, def)?;
            let key = Value::String(id);
            call.with_roots(&[key], |call| {
                if call.table_get(store, key)? == Value::Nil {
                    let field = call.try_intern(b"Default")?;
                    let value = match call.table_get(def, field) {
                        Ok(value) => value,
                        Err(error)
                            if matches!(
                                error.kind(),
                                deadlib_lua::ErrorKind::Limit | deadlib_lua::ErrorKind::Memory
                            ) =>
                        {
                            return Err(error);
                        }
                        Err(_) => Value::Nil,
                    };
                    if value != Value::Nil {
                        call.table_set(store, key, value)?;
                    }
                }
                Ok(())
            })
        })?;
        previous = key;
    }
}
fn default(call: &mut HostCall<'_>, name: &str) -> Result<Value, Error> {
    let key = name.to_ascii_lowercase();
    let text = match key.as_str() {
        "casualmaxmeter" => return Ok(Value::Number(12.)),
        "simplylovecolor" => return Ok(Value::Number(1.)),
        "numberofcontinuesallowed"
        | "screenselectmusicmenutimer"
        | "screenselectmusiccasualmenutimer"
        | "screenplayeroptionsmenutimer"
        | "screenevaluationmenutimer"
        | "screenevaluationnonstopmenutimer"
        | "screenevaluationsummarymenutimer"
        | "screennameentrymenutimer"
        | "screengroovestatsloginmenutimer"
        | "nice" => return Ok(Value::Number(0.)),
        "visualstyle" => "Technique",
        "themefont" => "Common",
        "defaultgamemode" => "Dance",
        "songselectbg" | "resultsbg" => "Off",
        "musicwheelstyle" | "autostyle" => "Default",
        "lastactiveevent"
        | "scoringsystem"
        | "stepstats"
        | "editmodelastseensong"
        | "editmodelastseendifficulty"
        | "editmodelastseenstepstype"
        | "editmodelastseenstyletype" => "",
        _ => return Ok(Value::Boolean(key == "useimagecache")),
    };
    call.try_intern(text.as_bytes())
}
pub(super) fn options(
    call: &mut HostCall<'_>,
    values: crate::option_rows::SongLuaOptionValues,
) -> Result<TableId, Error> {
    use crate::option_rows::SongLuaOptionValues::*;
    match values {
        Str(v) => array(call, v.iter().copied()),
        Bool(v) => array(call, v.iter().copied()),
        Int(v) => array(call, v.iter().copied()),
        Number(v) => array(call, v.iter().copied()),
    }
}
pub(super) fn definitions(call: &mut HostCall<'_>) -> Result<TableId, Error> {
    object(call, |call, defs| {
        for &name in crate::option_rows::THEME_PREF_ROW_NAMES {
            let spec = crate::option_rows::theme_pref_row_spec(name);
            let def = object(call, |call, def| {
                set(def, "Default", default(call, name)?, call)?;
                set(def, "Choices", options(call, spec.choices)?, call)?;
                if let Some(values) = spec.values {
                    set(def, "Values", options(call, values)?, call)?;
                }
                Ok(())
            })?;
            set(defs, name, def, call)?;
        }
        Ok(())
    })
}
