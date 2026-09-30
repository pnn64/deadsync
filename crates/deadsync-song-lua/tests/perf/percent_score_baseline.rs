// Frozen FormatPercentScore callback body from dc21ff490 (0.5.1629).
use super::*;

pub(super) fn format_percent_score(lua: &Lua, args: MultiValue) -> mlua::Result<Value> {
    let value = args.front().cloned().and_then(read_f32).unwrap_or(0.0);
    Ok(Value::String(
        lua.create_string(format!("{:.2}%", value * 100.0))?,
    ))
}
