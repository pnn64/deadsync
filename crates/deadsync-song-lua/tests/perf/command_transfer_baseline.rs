// Frozen from 4c7312013a6143518f0ea5088c046e56afb7c8a4; only test visibility/formatting differs.

use super::*;

pub(super) fn actor_command_args(actor: &Table, params: Option<Value>) -> MultiValue {
    let mut args = MultiValue::new();
    args.push_back(Value::Table(actor.clone()));
    if let Some(params) = params {
        args.push_back(params);
    }
    args
}

pub(super) fn call_actor_function(
    lua: &Lua,
    actor: &Table,
    command: &Function,
    params: Option<Value>,
) -> mlua::Result<()> {
    if let Some(script_dir) = actor
        .get::<Option<String>>("__songlua_script_dir")?
        .filter(|dir| !dir.trim().is_empty())
    {
        return call_with_script_dir(lua, Path::new(&script_dir), || {
            command.call::<()>(actor_command_args(actor, params))
        });
    }
    command.call::<()>(actor_command_args(actor, params))
}
