//! Existing compiler action publication and raw sequential action-table reads.
use super::*;
use crate::{
    SongLuaCompileInfo, SongLuaMessageEvent, SongLuaOverlayMessageCommand, SongLuaSoundEvent,
};

/// Borrow the compiler's existing output and synthetic-message sequence.
pub struct SongActionOutput<'a> {
    pub counter: &'a mut usize,
    pub messages: &'a mut Vec<SongLuaMessageEvent>,
    pub sound_events: &'a mut Vec<SongLuaSoundEvent>,
    pub info: &'a mut SongLuaCompileInfo,
}

fn listener(
    overlays: &[SongMessageTarget<'_>],
    tracked: &[SongMessageTarget<'_>],
    message: &str,
) -> bool {
    crate::message_command_lists_have_listener(
        overlays
            .iter()
            .chain(tracked)
            .map(|r| r.commands.as_slice()),
        message,
    )
}

impl SongRuntime {
    /// Publish the native compiler's named broadcasts, sounds and synthetic
    /// direct-effect message after a successful function-action capture.
    pub fn compile_song_function_action(
        &mut self,
        overlays: &mut [SongMessageTarget<'_>],
        tracked: &mut [SongMessageTarget<'_>],
        function: Value,
        beat: f32,
        persists: bool,
        output: &mut SongActionOutput<'_>,
    ) -> Result<bool, String> {
        let selected = overlays
            .iter()
            .map(|r| r.actor)
            .enumerate()
            .collect::<Vec<_>>();
        let tracked_values = tracked.iter().map(|r| r.actor).collect::<Vec<_>>();
        let capture =
            self.capture_song_function_action(&selected, &tracked_values, function, beat)?;
        output.sound_events.extend(
            capture
                .sound_paths
                .into_iter()
                .map(|path| SongLuaSoundEvent {
                    beat,
                    path,
                    source_message: None,
                    queued_startup: false,
                }),
        );
        let mut handled = false;
        if !capture.broadcasts.is_empty() && capture.broadcasts.iter().all(|(_, params)| !*params) {
            for (message, _) in &capture.broadcasts {
                if listener(overlays, tracked, message) {
                    output.messages.push(SongLuaMessageEvent {
                        beat,
                        message: message.clone(),
                        persists,
                    });
                    handled = true;
                }
            }
        }
        let direct = !capture.overlay_blocks.is_empty() || !capture.tracked_blocks.is_empty();
        if handled && !direct {
            return Ok(true);
        }
        if !direct {
            return Ok(capture.side_effects > 0);
        }
        let message = format!("__songlua_overlay_fn_action_{}", output.counter);
        *output.counter += 1;
        for (index, blocks) in capture.overlay_blocks {
            overlays[index].commands.push(SongLuaOverlayMessageCommand {
                frame_advance: 0.,
                message: message.clone(),
                blocks,
            });
        }
        for (index, blocks) in capture.tracked_blocks {
            tracked[index].commands.push(SongLuaOverlayMessageCommand {
                frame_advance: 0.,
                message: message.clone(),
                blocks,
            });
        }
        output.messages.push(SongLuaMessageEvent {
            beat,
            message,
            persists,
        });
        Ok(true)
    }

    /// Read the current raw sequence after each entry. Preserve malformed-entry
    /// skips and hard string errors, and turn failed/unhandled function probes
    /// into the existing unsupported detail before continuing later entries.
    pub fn read_song_overlay_actions(
        &mut self,
        table: Option<Value>,
        overlays: &mut [SongMessageTarget<'_>],
        tracked: &mut [SongMessageTarget<'_>],
        output: &mut SongActionOutput<'_>,
    ) -> Result<(), String> {
        let Some(table) = table.filter(|value| *value != Value::Nil) else {
            return Ok(());
        };
        let mut pins = Vec::new();
        let result: Result<(), String> = (|| {
            pins.push(self.vm.pin(table).map_err(|e| e.to_string())?);
            let Value::Table(table) = table else {
                return Err("action table expected".into());
            };
            for target in overlays.iter().chain(tracked.iter()) {
                pins.push(self.vm.pin(target.actor).map_err(|e| e.to_string())?);
            }
            let mut index = 1;
            loop {
                let value = self
                    .vm
                    .raw_table_get(table, Value::Number(index as f64))
                    .map_err(|e| e.to_string())?;
                if value == Value::Nil {
                    break;
                }
                index += 1;
                let Value::Table(entry) = value else {
                    continue;
                };
                // Native owns a fresh table handle for this iteration. Retain
                // both the selected entry and its original outer table across
                // callbacks which replace globals, edit later slots or collect.
                let entry_pin = self.vm.pin(value).map_err(|e| e.to_string())?;
                let result: Result<(), String> = (|| {
                    let beat = self
                        .vm
                        .raw_table_get(entry, Value::Number(1.))
                        .map_err(|e| e.to_string())?;
                    let Some(beat) = action_beat(&self.vm, beat) else {
                        return Ok(());
                    };
                    let action = self
                        .vm
                        .raw_table_get(entry, Value::Number(2.))
                        .map_err(|e| e.to_string())?;
                    let persists = self
                        .vm
                        .raw_table_get(entry, Value::Number(3.))
                        .map_err(|e| e.to_string())?
                        .is_truthy();
                    match action {
                        Value::String(id) => {
                            let message = std::str::from_utf8(self.vm.string_bytes(id).unwrap())
                                .map_err(|e| e.to_string())?
                                .to_owned();
                            output.messages.push(SongLuaMessageEvent {
                                beat,
                                message,
                                persists,
                            });
                        }
                        Value::Function(_) | Value::Native(_)
                            if !matches!(
                                self.compile_song_function_action(
                                    overlays, tracked, action, beat, persists, output
                                ),
                                Ok(true)
                            ) =>
                        {
                            let detail = crate::record_unsupported_function_action_capture(
                                output.info,
                                beat,
                                persists,
                            );
                            log::debug!("Unsupported song lua function action capture: {detail}");
                        }
                        _ => {}
                    }
                    Ok(())
                })();
                self.vm.unpin(entry_pin).map_err(|e| e.to_string())?;
                result?;
            }
            Ok(())
        })();
        for pin in pins {
            self.vm.unpin(pin).map_err(|e| e.to_string())?;
        }
        result
    }
}
fn action_beat(vm: &Vm, value: Value) -> Option<f32> {
    match value {
        Value::Number(value) => {
            let value = value as f32;
            value.is_finite().then_some(value)
        }
        Value::String(id) => std::str::from_utf8(vm.string_bytes(id)?)
            .ok()?
            .trim()
            .parse()
            .ok(),
        _ => None,
    }
}
