//! Compiler extraction of source message commands, before chronological replay.
use super::*;

impl SongRuntime {
    /// Probe one source command, retaining its preexisting capture history and
    /// rewinding scalar globals, command/script locals and all touched actors.
    /// Command-field/child edits retain the native structural mutation boundary.
    pub fn capture_song_actor_command(
        &mut self,
        actor: Value,
        command_name: &str,
    ) -> Result<Vec<crate::SongLuaOverlayCommandBlock>, String> {
        if !self.actors.actor_function_capture_ready(&mut self.vm) {
            return Err("actor command capture requires preparation before frame replay".into());
        }
        let mut pins = Vec::new();
        let mut snapshot = None;
        let mut session = None;
        let result = (|| {
            pins.push(self.vm.pin(actor).map_err(|e| e.to_string())?);
            let command = self
                .actors
                .lookup_capture_command(&mut self.vm, actor, command_name)
                .map_err(|e| e.to_string())?;
            let Some(command) = command else {
                return Ok(Vec::new());
            };
            pins.push(command);
            let function = self.vm.rooted(command).map_err(|e| e.to_string())?;
            snapshot = Some(
                self.actors
                    .snapshot_function_action(&mut self.vm, function)
                    .map_err(|e| e.to_string())?,
            );
            session = Some(
                self.actors
                    .prepare_command_capture(&mut self.vm, actor, command_name)
                    .map_err(|e| e.to_string())?,
            );
            let session = session.as_ref().unwrap();
            let runner = session.runner(&self.vm).map_err(|e| e.to_string())?;
            let (result, restored, returned) = self.actors.capture_touched_actor_function(
                &mut self.vm,
                Vec::new(),
                runner,
                None,
                &self.random,
            );
            pins.extend(returned);
            if !restored {
                return Err(result.unwrap_err().to_string());
            }
            let raw = result
                .as_ref()
                .err()
                .is_some_and(|e| e.kind() != deadlib_lua::ErrorKind::Runtime);
            session
                .restore(&mut self.vm, raw)
                .map_err(|e| e.to_string())?;
            if let Some(snapshot) = snapshot.take() {
                snapshot.restore(&mut self.vm).map_err(|e| e.to_string())?;
            }
            result.map_err(|e| e.to_string())?;
            Ok(session.blocks())
        })();
        if let Some(snapshot) = snapshot {
            snapshot.discard(&mut self.vm).map_err(|e| e.to_string())?;
        }
        if let Some(session) = session {
            session.release(&mut self.vm).map_err(|e| e.to_string())?;
        }
        for pin in pins {
            self.vm.unpin(pin).map_err(|e| e.to_string())?;
        }
        result
    }

    /// Read message names before probing, preserve native lexical ordering and
    /// defer per-command failures for the compiler's later retry pass.
    pub fn capture_song_actor_message_commands(
        &mut self,
        actor: Value,
    ) -> Result<crate::SongLuaCapturedMessageCommands, String> {
        if self.clock.live.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(Default::default());
        }
        if !self.actors.actor_function_capture_ready(&mut self.vm) {
            return Err("actor message capture requires preparation before frame replay".into());
        }
        let pin = self.vm.pin(actor).map_err(|e| e.to_string())?;
        let result = (|| {
            let names = self
                .actors
                .message_command_names(&mut self.vm, actor)
                .map_err(|e| e.to_string())?;
            let mut output = crate::SongLuaCapturedMessageCommands::default();
            for name in names {
                let message = name.trim_end_matches("MessageCommand").to_owned();
                let sounds = self
                    .actors
                    .begin_message_sounds(&mut self.vm)
                    .map_err(|e| e.to_string())?;
                let blocks = self.capture_song_actor_command(actor, &name);
                let paths = sounds.collect_restore(&mut self.vm);
                let released = sounds.release(&mut self.vm);
                released.map_err(|e| e.to_string())?;
                let paths = paths.map_err(|e| e.to_string())?;
                let blocks = match blocks {
                    Ok(blocks) => blocks,
                    Err(error) => {
                        let detail = self
                            .actors
                            .defer_actor_message(&mut self.vm, actor, &name, &error)
                            .map_err(|e| e.to_string())?;
                        crate::push_unique_compile_detail(&mut output.skipped, detail);
                        continue;
                    }
                };
                output.sounds.extend(
                    paths
                        .map_err(|e| e.to_string())?
                        .into_iter()
                        .map(|path| (message.clone(), path)),
                );
                if !blocks.is_empty() {
                    output.commands.push(crate::SongLuaOverlayMessageCommand {
                        frame_advance: 0.,
                        message,
                        blocks,
                    });
                }
            }
            let startup = self
                .actors
                .captured_blocks(&mut self.vm, actor)
                .unwrap_or_default()
                .into_iter()
                .filter(|b| b.delta.sound_play == Some(true))
                .collect::<Vec<_>>();
            if !startup.is_empty() {
                output.commands.push(crate::SongLuaOverlayMessageCommand {
                    frame_advance: 0.,
                    message: crate::SONG_LUA_STARTUP_MESSAGE.to_owned(),
                    blocks: startup,
                });
            }
            Ok(output)
        })();
        self.vm.unpin(pin).map_err(|e| e.to_string())?;
        result
    }
}
