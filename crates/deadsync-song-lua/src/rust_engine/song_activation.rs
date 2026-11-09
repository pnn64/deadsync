//! The compiler's screen activation boundary, before future queue discovery.
use super::{ActorScene, RustSongRuntime};
use crate::{SongLuaCompileContext, SongLuaStartup};
use deadlib_lua::{Error, TableId, Value};
use std::collections::HashMap;

pub use crate::frame_capture::StartupState as SongStartupState;

/// Output of Init/Begin/ScreenChanged/On and the real initial update.
/// Lua handles remain traced by the active screen; pin separately to retain
/// them across scene retirement/release. Future queued commands have not run.
#[derive(Clone, Debug)]
pub struct SongActivation {
    pub initialized_globals: [Option<Value>; 4],
    pub options: SongLuaStartup,
    pub new_screen: Option<String>,
    pub updates: HashMap<TableId, SongStartupState>,
    pub tweens: HashMap<TableId, SongStartupState>,
    pub wrapper_initials: HashMap<TableId, crate::SongLuaOverlayState>,
    pub broadcasts: Vec<(f32, String, bool)>,
}
impl RustSongRuntime {
    /// Activate initialized authored layers once, in the production order.
    /// Init requests are discarded at their existing boundary. Only NewScreen
    /// is consumed after Update(0); a pending transition remains pending.
    /// This does not probe future commands or compile output timelines.
    pub fn activate_song_layers(
        &mut self,
        screen: ActorScene,
        context: &SongLuaCompileContext,
    ) -> Result<Option<SongActivation>, Error> {
        if self.actor_scene_root(screen).is_none() {
            return Ok(None);
        }
        let mut activation = self.actors.activate_song_layers(
            &mut self.vm,
            screen,
            context,
            self.screen.state.clone(),
        )?;
        if !(context.live && context.defer_initial_live_update) {
            let before = self.actors.before_song_update(&mut self.vm, screen)?;
            self.set_values(crate::song_beat_at_elapsed_seconds(0., context), 0.)?;
            self.actors.song_zero_update(true);
            let result = (|| {
                #[cfg(feature = "test-support")]
                self.reference_start_clock(context)?;
                self.advance_options(0., None)?;
                self.actors.zero_song_update(&mut self.vm, screen)
            })();
            self.actors.song_zero_update(false);
            result?;
            activation.updates = self
                .actors
                .after_song_update(&mut self.vm, screen, before)?;
        }
        activation.new_screen = self.screen.state.lock().unwrap().next.take();
        #[cfg(feature = "test-support")]
        self.observations.skin_active(false);
        self.actors.capture_song_startup_text(
            &mut self.vm,
            screen,
            activation.wrapper_initials.keys().copied().collect(),
        )?;
        activation.broadcasts = self.actors.take_song_broadcasts();
        for (actor, state) in self.actors.song_wrapper_initials(&mut self.vm, screen)? {
            activation.wrapper_initials.entry(actor).or_insert(state);
        }
        if !context.live && activation.new_screen.is_none() {
            activation.tweens = self.actors.song_startup_tweens(
                &mut self.vm,
                screen,
                activation.wrapper_initials.clone(),
            )?;
        }
        self.actors.complete_song_activation(screen);
        Ok(Some(activation))
    }

    #[cfg(feature = "test-support")]
    fn reference_start_clock(&mut self, context: &SongLuaCompileContext) -> Result<(), Error> {
        if let Some(&[music, beat, _]) = context.compile_clock_inputs.first() {
            let origin = context
                .song_timing
                .as_ref()
                .map_or(0., |timing| f64::from(timing.get_time_for_beat_exact(0.)));
            self.set_values(
                beat as f32,
                ((music - origin) / f64::from(crate::song_music_rate(context))) as f32,
            )?;
        }
        Ok(())
    }
}
