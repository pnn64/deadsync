//! Retained background/foreground scheduling in the existing live frame phases.
use super::*;

/// Actor handles and activation cursors for one live screen. Release through its
/// runtime before retiring the screen. Clocks and the last completed music time
/// belong to the enclosing scene coordinator, so failed frames do not commit it.
pub struct SongLiveLayers {
    owners: Option<Root>,
    screen: ActorScene,
}
impl SongRuntime {
    pub fn new_song_live_layers(
        &mut self,
        screen: ActorScene,
        context: &SongLuaCompileContext,
    ) -> Result<Option<SongLiveLayers>, Error> {
        if self.actor_scene_root(screen).is_none() {
            return Ok(None);
        }
        self.actors
            .live_layers(&mut self.vm, screen, context)
            .map(|owners| {
                Some(SongLiveLayers {
                    owners: Some(owners),
                    screen,
                })
            })
    }
}
impl SongLiveLayers {
    fn owners(&self, runtime: &mut SongRuntime) -> Result<Value, Error> {
        let owner = self
            .owners
            .ok_or_else(|| error(runtime, b"released song live layers"))?;
        let value = runtime.vm.rooted(owner)?;
        if runtime.actor_scene_root(self.screen).is_none() {
            return Err(error(
                runtime,
                b"live layer screen has been retired or released",
            ));
        }
        Ok(value)
    }
    fn invoke(&self, runtime: &mut SongRuntime, phase: usize, args: &[Value]) -> Result<(), Error> {
        let Value::Table(owners) = self.owners(runtime)? else {
            unreachable!()
        };
        let callback = runtime
            .vm
            .raw_table_get(owners, Value::Number(phase as f64))?;
        runtime.vm.call(callback, args).map(|_| ())
    }
    /// Run after the host clock/options phase, before synchronous Player
    /// callbacks. Only backgrounds run here; foreground On belongs to traversal.
    pub fn prepare_backgrounds(
        &mut self,
        runtime: &mut SongRuntime,
        beat: f64,
        music_seconds: f64,
        last_music_seconds: f32,
    ) -> Result<(), Error> {
        self.invoke(
            runtime,
            actors::live_layers::BACKGROUNDS,
            &[
                Value::Number(beat),
                Value::Number(music_seconds),
                Value::Number(last_music_seconds as f64),
            ],
        )
    }
    /// Advance the complete live actor traversal with foreground entry, then
    /// expire foregrounds. The caller commits last_music_seconds after its outro
    /// phase succeeds, matching the existing scene update's error boundary.
    pub fn advance(
        &mut self,
        runtime: &mut SongRuntime,
        beat: f64,
        music_seconds: f64,
        last_music_seconds: f32,
        delta_seconds: f64,
    ) -> Result<bool, Error> {
        self.invoke(
            runtime,
            actors::live_layers::CLOCK,
            &[
                Value::Number(beat),
                Value::Number(music_seconds),
                Value::Number(last_music_seconds as f64),
            ],
        )?;
        let Value::Table(owners) = self.owners(runtime)? else {
            unreachable!()
        };
        let entry = runtime
            .vm
            .raw_table_get(owners, Value::Number(actors::live_layers::ENTRY as f64))?;
        if !runtime.advance_song_live_actors(self.screen, delta_seconds, Some(entry))? {
            return Ok(false);
        }
        self.invoke(runtime, actors::live_layers::EXPIRE, &[])?;
        Ok(true)
    }
    pub fn release(&mut self, runtime: &mut SongRuntime) -> Result<(), Error> {
        if let Some(owner) = self.owners {
            runtime.vm.rooted(owner)?;
            self.owners = None;
            runtime.vm.unpin(owner)?;
        }
        Ok(())
    }
}
fn error(runtime: &mut SongRuntime, message: &[u8]) -> Error {
    match runtime.vm.with_native_stack(|s| Ok(s.error(message))) {
        Ok(e) | Err(e) => e,
    }
}
