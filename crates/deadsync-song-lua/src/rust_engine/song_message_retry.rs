//! The existing compiler's stable cross-actor probes and deferred retries.
use super::*;
use crate::{SongLuaFunctionActionCapture, SongLuaOverlayMessageCommand};
use std::path::PathBuf;

/// Borrow compiler output while retaining the corresponding actor for a pass.
/// The caller owns its longer-lived actor roots, as with other capture inputs.
pub struct SongMessageTarget<'a> {
    pub actor: Value,
    pub commands: &'a mut Vec<SongLuaOverlayMessageCommand>,
}
type Attempt = Result<SongLuaFunctionActionCapture, String>;
type Attempts = (Attempt, Option<Attempt>);

fn effects(blocks: &SongActorFunctionBlocks, source: usize) -> SongActorFunctionBlocks {
    let rows = blocks.iter().filter(|(i, _)| *i != source);
    if blocks.windows(2).all(|p| p[0].0 < p[1].0) {
        rows.cloned().collect()
    } else {
        // Match the existing last-write-wins rule for duplicate indices.
        rows.cloned()
            .collect::<std::collections::BTreeMap<_, _>>()
            .into_iter()
            .collect()
    }
}
fn command(
    message: &str,
    blocks: Vec<crate::SongLuaOverlayCommandBlock>,
) -> SongLuaOverlayMessageCommand {
    SongLuaOverlayMessageCommand {
        frame_advance: 0.,
        message: message.to_owned(),
        blocks,
    }
}
impl RustSongRuntime {
    fn song_message_attempts(
        &mut self,
        overlays: &[(usize, Value)],
        tracked: &[Value],
        source: Value,
        name: &str,
        function: Value,
        beat: f32,
    ) -> Result<Attempts, String> {
        let mut snapshot = Some(
            self.actors
                .snapshot_function_action(&mut self.vm, function)
                .map_err(|e| e.to_string())?,
        );
        let mut runner = None;
        let result = (|| {
            let root = self
                .create_song_message_capture_runner(source, name, function)
                .map_err(|e| e.to_string())?;
            runner = Some(root);
            let function = self.vm.rooted(root).map_err(|e| e.to_string())?;
            let random = self.preserve_random();
            let previous = self.actors.begin_command_locals();
            let first =
                self.capture_song_function_action_inner(overlays, tracked, function, beat, false);
            let second = first.as_ref().ok().map(|_| {
                self.capture_song_function_action_inner(overlays, tracked, function, beat, false)
            });
            let restored = self.actors.restore_command_locals(&mut self.vm, previous);
            drop(random);
            restored.map_err(|e| e.to_string())?;
            snapshot
                .take()
                .unwrap()
                .restore(&mut self.vm)
                .map_err(|e| e.to_string())?;
            Ok((first, second))
        })();
        if let Some(snapshot) = snapshot {
            snapshot.discard(&mut self.vm).map_err(|e| e.to_string())?;
        }
        if let Some(runner) = runner {
            self.vm.unpin(runner).map_err(|e| e.to_string())?;
        }
        result
    }

    /// Add stable cross-actor effects, retaining the native two-run command-local
    /// boundary. Dynamic handlers remain deferred for chronological execution.
    pub fn capture_song_stable_cross_actor_message_commands(
        &mut self,
        overlays: &mut [SongMessageTarget<'_>],
        mut on_dynamic: impl FnMut(String),
    ) -> Result<(), String> {
        if !self.actors.actor_function_capture_ready() {
            return Err("stable message capture requires preparation".into());
        }
        let mut pins = Vec::new();
        let mut commands = None;
        let result = (|| {
            for row in overlays.iter() {
                pins.push(self.vm.pin(row.actor).map_err(|e| e.to_string())?);
            }
            let actors = overlays.iter().map(|r| r.actor).collect::<Vec<_>>();
            commands = Some(
                self.actors
                    .message_command_functions(&mut self.vm, &actors)
                    .map_err(|e| e.to_string())?,
            );
            let selected = actors.iter().copied().enumerate().collect::<Vec<_>>();
            let mut additions = Vec::new();
            for &(source, ref name, function) in &commands.as_ref().unwrap().rows {
                let (first, second) =
                    self.song_message_attempts(&selected, &[], actors[source], name, function, 0.)?;
                let Ok(first) = first else {
                    continue;
                };
                let second = match second.unwrap() {
                    Ok(second) => second,
                    Err(error) => {
                        let detail = self
                            .actors
                            .defer_dynamic_actor_message(
                                &mut self.vm,
                                actors[source],
                                name,
                                Some(&error),
                            )
                            .map_err(|e| e.to_string())?;
                        on_dynamic(detail);
                        continue;
                    }
                };
                let first = effects(&first.overlay_blocks, source);
                if first.is_empty() {
                    continue;
                }
                if first != effects(&second.overlay_blocks, source) {
                    let detail = self
                        .actors
                        .defer_dynamic_actor_message(&mut self.vm, actors[source], name, None)
                        .map_err(|e| e.to_string())?;
                    on_dynamic(detail);
                    continue;
                }
                let message = name.strip_suffix("MessageCommand").unwrap();
                additions.extend(first.into_iter().map(|(i, b)| (i, command(message, b))));
            }
            // The native stable pass publishes only after every source succeeds.
            for (i, command) in additions {
                overlays[i].commands.push(command);
            }
            Ok(())
        })();
        if let Some(commands) = commands {
            self.vm.unpin(commands.pin).map_err(|e| e.to_string())?;
        }
        for pin in pins {
            self.vm.unpin(pin).map_err(|e| e.to_string())?;
        }
        result
    }

    /// Consume deferred records at the current song beat. Successful stable
    /// retries publish both source and cross-actor effects and repeated sounds.
    pub fn capture_song_deferred_messages(
        &mut self,
        overlays: &mut [SongMessageTarget<'_>],
        tracked: &mut [SongMessageTarget<'_>],
        runtime_captures: &[crate::SongLuaStatefulMessageCapture],
        skipped: &mut Vec<String>,
    ) -> Result<Vec<(usize, String, PathBuf)>, String> {
        let mut pins = Vec::new();
        let mut deferred = None;
        let result = (|| {
            deferred = Some(
                self.actors
                    .take_deferred_messages(&mut self.vm)
                    .map_err(|e| e.to_string())?,
            );
            if deferred.as_ref().unwrap().rows.is_empty() {
                return Ok(Vec::new());
            }
            for row in overlays.iter().chain(tracked.iter()) {
                pins.push(self.vm.pin(row.actor).map_err(|e| e.to_string())?);
            }
            let selected = overlays
                .iter()
                .map(|r| r.actor)
                .enumerate()
                .collect::<Vec<_>>();
            let tracked_values = tracked.iter().map(|r| r.actor).collect::<Vec<_>>();
            let beat = self.values().map_err(|e| e.to_string())?.0;
            let mut sounds = Vec::new();
            for row in &deferred.as_ref().unwrap().rows {
                let Some(source) = overlays
                    .iter()
                    .position(|r| r.actor == Value::Table(row.actor))
                else {
                    continue;
                };
                let message = row.command.strip_suffix("MessageCommand").unwrap();
                if row.observed
                    || runtime_captures.iter().any(|c| {
                        c.message == message && c.overlay_targets.iter().any(|(i, _)| *i == source)
                    })
                {
                    skipped.retain(|d| d != &row.detail);
                    continue;
                }
                let _capture = self.actors.deferred_message_capture()?;
                let root = self
                    .actors
                    .lookup_capture_command(&mut self.vm, Value::Table(row.actor), &row.command)
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "deferred message command must be a function".to_owned())?;
                let function = self.vm.rooted(root).map_err(|e| e.to_string())?;
                let attempts = self.song_message_attempts(
                    &selected,
                    &tracked_values,
                    Value::Table(row.actor),
                    &row.command,
                    function,
                    beat,
                );
                self.vm.unpin(root).map_err(|e| e.to_string())?;
                let (Ok(first), Some(Ok(second))) = attempts? else {
                    continue;
                };
                if first != second
                    || first.side_effects > first.sound_paths.len() as i64
                    || !first.broadcasts.is_empty()
                {
                    continue;
                }
                // Deferred publication is incremental, including partial state
                // if a later record fails, matching native compiler mutation.
                for (i, blocks) in effects(&first.overlay_blocks, usize::MAX) {
                    overlays[i].commands.push(command(message, blocks));
                }
                for (i, blocks) in effects(&first.tracked_blocks, usize::MAX) {
                    tracked[i].commands.push(command(message, blocks));
                }
                sounds.extend(
                    first
                        .sound_paths
                        .into_iter()
                        .map(|p| (source, message.to_owned(), p)),
                );
                skipped.retain(|d| d != &row.detail);
            }
            Ok(sounds)
        })();
        if let Some(deferred) = deferred {
            self.vm.unpin(deferred.pin).map_err(|e| e.to_string())?;
        }
        for pin in pins {
            self.vm.unpin(pin).map_err(|e| e.to_string())?;
        }
        result
    }
}
