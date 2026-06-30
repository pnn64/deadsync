use mlua::{Function, Table, Value};

use crate::{SongLuaMessageEvent, read_f32, truthy};

pub struct SongLuaFunctionActionInput {
    pub function: Function,
    pub beat: f32,
    pub persists: bool,
}

pub fn read_actions_with_function_capture<F>(
    table: Option<Table>,
    messages: &mut Vec<SongLuaMessageEvent>,
    mut capture_function: F,
) -> Result<(), String>
where
    F: FnMut(SongLuaFunctionActionInput, &mut Vec<SongLuaMessageEvent>) -> Result<(), String>,
{
    let Some(table) = table else {
        return Ok(());
    };
    for value in table.sequence_values::<Value>() {
        let Value::Table(entry) = value.map_err(|err| err.to_string())? else {
            continue;
        };
        let Some(beat) = read_f32(entry.raw_get::<Value>(1).map_err(|err| err.to_string())?) else {
            continue;
        };
        let action = entry.raw_get::<Value>(2).map_err(|err| err.to_string())?;
        let persists = truthy(&entry.raw_get::<Value>(3).map_err(|err| err.to_string())?);
        match action {
            Value::String(text) => messages.push(SongLuaMessageEvent {
                beat,
                message: text.to_str().map_err(|err| err.to_string())?.to_string(),
                persists,
            }),
            Value::Function(function) => capture_function(
                SongLuaFunctionActionInput {
                    function,
                    beat,
                    persists,
                },
                messages,
            )?,
            _ => {}
        }
    }
    Ok(())
}
