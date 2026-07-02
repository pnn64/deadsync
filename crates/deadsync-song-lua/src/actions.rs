use mlua::{Function, Table, Value};

use crate::{
    SongLuaMessageEvent, SongLuaOverlayCommandBlock, SongLuaOverlayMessageCommand, read_f32, truthy,
};

pub struct SongLuaFunctionActionInput {
    pub function: Function,
    pub beat: f32,
    pub persists: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaFunctionActionPlan {
    pub handled: bool,
    pub next_counter: usize,
    pub messages: Vec<SongLuaMessageEvent>,
    pub overlay_commands: Vec<(usize, SongLuaOverlayMessageCommand)>,
    pub tracked_commands: Vec<(usize, SongLuaOverlayMessageCommand)>,
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

pub fn function_action_plan<F>(
    beat: f32,
    persists: bool,
    counter: usize,
    overlay_captures: Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>,
    tracked_captures: Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>,
    broadcasts: Vec<(String, bool)>,
    saw_side_effect: bool,
    mut has_listener: F,
) -> SongLuaFunctionActionPlan
where
    F: FnMut(&str) -> bool,
{
    if !broadcasts.is_empty() && broadcasts.iter().all(|(_, has_params)| !*has_params) {
        let mut messages = Vec::new();
        for (message, _) in broadcasts {
            if has_listener(&message) {
                messages.push(SongLuaMessageEvent {
                    beat,
                    message,
                    persists,
                });
            }
        }
        if !messages.is_empty() {
            return SongLuaFunctionActionPlan {
                handled: true,
                next_counter: counter,
                messages,
                overlay_commands: Vec::new(),
                tracked_commands: Vec::new(),
            };
        }
    }

    if overlay_captures.is_empty() && tracked_captures.is_empty() {
        return SongLuaFunctionActionPlan {
            handled: saw_side_effect,
            next_counter: counter,
            messages: Vec::new(),
            overlay_commands: Vec::new(),
            tracked_commands: Vec::new(),
        };
    }

    let message = format!("__songlua_overlay_fn_action_{counter}");
    SongLuaFunctionActionPlan {
        handled: true,
        next_counter: counter + 1,
        messages: vec![SongLuaMessageEvent {
            beat,
            message: message.clone(),
            persists,
        }],
        overlay_commands: overlay_captures
            .into_iter()
            .map(|(index, blocks)| {
                (
                    index,
                    SongLuaOverlayMessageCommand {
                        message: message.clone(),
                        blocks,
                    },
                )
            })
            .collect(),
        tracked_commands: tracked_captures
            .into_iter()
            .map(|(index, blocks)| {
                (
                    index,
                    SongLuaOverlayMessageCommand {
                        message: message.clone(),
                        blocks,
                    },
                )
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        SongLuaOverlayCommandBlock, SongLuaOverlayStateDelta, actions::function_action_plan,
    };

    fn block() -> SongLuaOverlayCommandBlock {
        SongLuaOverlayCommandBlock {
            start: 0.0,
            duration: 0.0,
            easing: None,
            opt1: None,
            opt2: None,
            delta: SongLuaOverlayStateDelta {
                x: Some(1.0),
                ..SongLuaOverlayStateDelta::default()
            },
        }
    }

    #[test]
    fn function_action_plan_emits_listener_broadcasts_first() {
        let plan = function_action_plan(
            12.0,
            true,
            7,
            vec![(0, vec![block()])],
            Vec::new(),
            vec![("Ignored".to_string(), false), ("Hit".to_string(), false)],
            false,
            |message| message == "Hit",
        );

        assert!(plan.handled);
        assert_eq!(plan.next_counter, 7);
        assert_eq!(plan.messages.len(), 1);
        assert_eq!(plan.messages[0].message, "Hit");
        assert!(plan.messages[0].persists);
        assert!(plan.overlay_commands.is_empty());
    }

    #[test]
    fn function_action_plan_generates_command_message_for_captures() {
        let plan = function_action_plan(
            4.0,
            false,
            3,
            vec![(2, vec![block()])],
            vec![(1, vec![block()])],
            vec![("WithParams".to_string(), true)],
            false,
            |_| true,
        );

        assert!(plan.handled);
        assert_eq!(plan.next_counter, 4);
        assert_eq!(plan.messages[0].message, "__songlua_overlay_fn_action_3");
        assert_eq!(plan.overlay_commands[0].0, 2);
        assert_eq!(
            plan.overlay_commands[0].1.message,
            "__songlua_overlay_fn_action_3"
        );
        assert_eq!(plan.tracked_commands[0].0, 1);
    }

    #[test]
    fn function_action_plan_handles_side_effect_only_actions() {
        let plan = function_action_plan(
            1.0,
            false,
            9,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            true,
            |_| false,
        );

        assert!(plan.handled);
        assert_eq!(plan.next_counter, 9);
        assert!(plan.messages.is_empty());
    }
}
