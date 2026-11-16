//! Existing native replay controls; absent from normal application builds.
use super::*;

impl SongLiveScene {
    /// Independent ModsGroup deltas consumed by the next unprepared frame.
    pub fn set_option_clock_deltas(&mut self, deltas: [f32; LUA_PLAYERS + 1]) {
        self.option_clock_deltas = Some(deltas);
    }

    /// Captured identities for the next screen epoch, installed on reload.
    pub fn set_message_actor_order(&mut self, order: Vec<(String, usize)>) {
        self.context.message_actor_order = order;
    }

    /// Observe after the overlay callback, before scheduled song foregrounds.
    pub fn enable_overlay_stage_observer(&self) -> Result<(), Error> {
        let mut runtime = self.runtime.access();
        let catalog = runtime.vm.rooted(runtime.catalog)?;
        let statistics = runtime.statistics.clone();
        let SongRuntime { actors, vm, .. } = &mut *runtime;
        actors.enable_overlay_observer(
            vm,
            catalog,
            statistics,
            self.context.song_steps.len(),
            self.overlay_observation.clone(),
        )
    }

    pub fn take_overlay_stage_observation(&self) -> Option<crate::SongLuaOverlayStageObservation> {
        self.overlay_observation.lock().unwrap().take()
    }
}
