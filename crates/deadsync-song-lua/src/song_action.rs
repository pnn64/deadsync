//! Existing compiler function actions, preserving the native probe boundaries.
use super::*;

impl SongRuntime {
    /// Capture selected overlay and tracked actor writes plus broadcasts, sound
    /// paths and side effects. Rewind shared script state, clocks, command locals,
    /// RNG, column splines and all touched actor state after ordinary execution.
    pub fn capture_song_function_action(
        &mut self,
        overlays: &[(usize, Value)],
        tracked: &[Value],
        function: Value,
        beat: f32,
    ) -> Result<crate::SongLuaFunctionActionCapture, String> {
        self.capture_song_function_action_inner(overlays, tracked, function, beat, true)
    }

    pub(super) fn capture_song_function_action_inner(
        &mut self,
        overlays: &[(usize, Value)],
        tracked: &[Value],
        function: Value,
        beat: f32,
        restore_function_tables: bool,
    ) -> Result<crate::SongLuaFunctionActionCapture, String> {
        if !self.actors.actor_function_capture_ready(&mut self.vm) {
            return Err("function action capture requires preparation before frame replay".into());
        }
        let mut pins = Vec::new();
        let mut snapshot = None;
        let mut scope = None;
        let result = (|| {
            pins.push(self.vm.pin(function).map_err(|e| e.to_string())?);
            let mut selected = Vec::new();
            for (i, value) in overlays
                .iter()
                .map(|(_, v)| *v)
                .chain(tracked.iter().copied())
                .enumerate()
            {
                pins.push(self.vm.pin(value).map_err(|e| e.to_string())?);
                let Value::Table(actor) = value else {
                    return Err("capture actor table expected".into());
                };
                selected.push((i, actor));
            }
            if restore_function_tables {
                snapshot = Some(
                    self.actors
                        .snapshot_function_action(&mut self.vm, function)
                        .map_err(|e| e.to_string())?,
                );
            }
            let previous = self.values().map_err(|e| e.to_string())?;
            scope = Some(
                self.actors
                    .begin_function_action(&mut self.vm)
                    .map_err(|e| e.to_string())?,
            );
            self.set_beat(beat).map_err(|e| e.to_string())?;
            let session = scope.as_ref().unwrap();
            let (blocks, restored, returned) = self.actors.capture_function_action(
                &mut self.vm,
                selected,
                function,
                &self.random,
                session,
            );
            pins.extend(returned);
            if !restored {
                return Err(blocks.unwrap_err().to_string());
            }
            self.set_values(previous.0, previous.1)
                .map_err(|e| e.to_string())?;
            if let Some(snapshot) = snapshot.take() {
                snapshot.restore(&mut self.vm).map_err(|e| e.to_string())?;
            }
            let (broadcasts, sound_paths, side_effects) =
                session.output().map_err(|e| e.to_string())?;
            let blocks = blocks.map_err(|e| e.to_string())?;
            let mut output = crate::SongLuaFunctionActionCapture {
                broadcasts,
                sound_paths,
                side_effects,
                ..Default::default()
            };
            for (index, blocks) in blocks {
                if index < overlays.len() {
                    output.overlay_blocks.push((overlays[index].0, blocks));
                } else {
                    output.tracked_blocks.push((index - overlays.len(), blocks));
                }
            }
            Ok(output)
        })();
        if let Some(snapshot) = snapshot {
            snapshot.discard(&mut self.vm).map_err(|e| e.to_string())?;
        }
        if let Some(scope) = scope {
            scope.release(&mut self.vm).map_err(|e| e.to_string())?;
        }
        for pin in pins {
            self.vm.unpin(pin).map_err(|e| e.to_string())?;
        }
        result
    }
}
