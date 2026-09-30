// Frozen Lua integration functions from c7747938f (0.5.1631).
use crate::values::read_string;
use mlua::{Function, Lua, MultiValue, Table, Value};
const SONG_LUA_LOADER_ENVS_KEY: &str = "__songlua_loader_envs";

pub fn install_cmd_helpers(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    for name in ["queuecommand", "playcommand"] {
        globals.set(name, name)?;
    }
    globals.set(
        "cmd",
        lua.create_function(move |lua, args: MultiValue| {
            let command_name = args.front().cloned().and_then(read_string);
            let command_args = args.into_iter().skip(1).collect::<Vec<_>>();
            lua.create_function(move |_, actor: Table| {
                let Some(command_name) = command_name.as_deref() else {
                    return Ok(Value::Table(actor));
                };
                let Value::Function(method) = actor.get::<Value>(command_name)? else {
                    return Ok(Value::Table(actor));
                };
                let mut call_args = MultiValue::new();
                call_args.push_back(Value::Table(actor.clone()));
                for arg in &command_args {
                    call_args.push_back(arg.clone());
                }
                let _ = method.call::<Value>(call_args)?;
                Ok(Value::Table(actor))
            })
        })?,
    )?;
    Ok(())
}

pub fn register_loader_env(lua: &Lua, function: &Function, env: &Table) -> mlua::Result<()> {
    let globals = lua.globals();
    let envs = match globals.get::<Option<Table>>(SONG_LUA_LOADER_ENVS_KEY)? {
        Some(envs) => envs,
        None => {
            let envs = lua.create_table()?;
            globals.set(SONG_LUA_LOADER_ENVS_KEY, envs.clone())?;
            envs
        }
    };
    envs.set(loader_env_key(function), env.clone())
}

pub fn retarget_loader_env(lua: &Lua, function: &Function, env: &Table) -> mlua::Result<()> {
    let Some(envs) = lua
        .globals()
        .get::<Option<Table>>(SONG_LUA_LOADER_ENVS_KEY)?
    else {
        return Ok(());
    };
    let Some(loader_env) = envs.get::<Option<Table>>(loader_env_key(function))? else {
        return Ok(());
    };
    loader_env.set("__songlua_env_target", env.clone())
}

fn loader_env_key(function: &Function) -> String {
    format!("{:p}", function.to_pointer())
}

pub fn json_to_lua_value(lua: &Lua, value: serde_json::Value) -> mlua::Result<Value> {
    Ok(match value {
        serde_json::Value::Null => Value::Nil,
        serde_json::Value::Bool(value) => Value::Boolean(value),
        serde_json::Value::Number(value) => value
            .as_i64()
            .map(Value::Integer)
            .or_else(|| value.as_f64().map(Value::Number))
            .unwrap_or(Value::Nil),
        serde_json::Value::String(value) => Value::String(lua.create_string(value)?),
        serde_json::Value::Array(values) => {
            let table = lua.create_table()?;
            for (index, value) in values.into_iter().enumerate() {
                table.raw_set(index + 1, json_to_lua_value(lua, value)?)?;
            }
            Value::Table(table)
        }
        serde_json::Value::Object(values) => {
            let table = lua.create_table()?;
            for (key, value) in values {
                table.set(key, json_to_lua_value(lua, value)?)?;
            }
            Value::Table(table)
        }
    })
}
