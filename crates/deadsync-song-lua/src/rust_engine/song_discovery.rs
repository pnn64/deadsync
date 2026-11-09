//! Deferred rewind of shared script state after future-command discovery.
use super::{ActorScene, RustSongRuntime, SongActivation};
use deadlib_lua::Error;

/// Immutable output from chronological preparation. Each progress row is a
/// queue step; samples are [song time, native time remaining]. Values include
/// only tween properties explicitly written by startup or replay commands.
#[derive(Clone, Debug, PartialEq)]
pub struct SongTweenReplay {
    pub progress: Vec<Vec<[f32; 2]>>,
    pub values: Vec<(
        crate::SongLuaOverlayUpdateTarget,
        crate::SongLuaOverlayUpdateValue,
    )>,
}
impl SongTweenReplay {
    /// Attach the sampled clock to the existing startup command blocks. Empty
    /// (never reached) steps leave the original block progress untouched.
    pub fn apply_startup_progress(
        &self,
        blocks: &mut [crate::SongLuaOverlayCommandBlock],
        startup: &super::SongStartupState,
    ) {
        for (block, step) in blocks.iter_mut().zip(&startup.block_steps) {
            if let Some(index) = step.and_then(|step| step.checked_sub(1))
                && let Some(samples) = self.progress.get(index)
                && !samples.is_empty()
            {
                block.progress = Some(samples.clone().into());
            }
        }
    }
}

/// Owned compiler publication from immediate and scheduled render setters.
/// Each batch contains only actors touched since the previous drain.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SongReplayPropertyUpdates {
    pub overlay_index: usize,
    pub values: Vec<(
        crate::SongLuaOverlayUpdateTarget,
        crate::SongLuaOverlayUpdateValue,
    )>,
    pub scheduled: Vec<SongReplayScheduledUpdate>,
    pub final_values: Vec<(
        crate::SongLuaOverlayUpdateTarget,
        crate::SongLuaOverlayUpdateValue,
    )>,
    pub tween_reset: bool,
}

/// The immediate-only projection of a property-update batch.
pub type SongReplayImmediateUpdates = SongReplayPropertyUpdates;

pub use crate::frame_capture::ScheduledUpdate as SongReplayScheduledUpdate;

impl RustSongRuntime {
    /// Supply the compiler's previous-frame positions for cross-actor reads.
    /// Actors whose update has not run yet retain these prior positions.
    pub fn set_song_replay_prior_positions(
        &self,
        screen: ActorScene,
        states: &[crate::SongLuaOverlayState],
    ) -> bool {
        self.actor_scene_root(screen).is_some()
            && self.actors.set_song_replay_prior_positions(screen, states)
    }

    /// Apply sampled pose getters with the existing compiler setter rules.
    /// Invalid actor indices or non-pose targets leave the input unapplied.
    pub fn set_song_replay_pose_getters(
        &mut self,
        screen: ActorScene,
        values: &[(
            usize,
            crate::SongLuaOverlayUpdateTarget,
            crate::SongLuaOverlayUpdateValue,
        )],
    ) -> Result<bool, Error> {
        if self.actor_scene_root(screen).is_none() {
            return Ok(false);
        }
        self.actors
            .set_song_replay_pose_getters(&mut self.vm, screen, values)
    }

    /// Replace pending destinations for all captured overlay property families.
    pub fn set_song_pending_overlay_updates(
        &self,
        screen: ActorScene,
        values: &[(
            usize,
            crate::SongLuaOverlayUpdateTarget,
            crate::SongLuaOverlayUpdateValue,
        )],
    ) -> bool {
        self.actor_scene_root(screen).is_some()
            && self.actors.set_song_pending_overlay_updates(screen, values)
    }

    /// Drain all captured pose, renderer, text and sprite publications.
    /// This shares a buffer with the narrower drains; choose one per sample.
    pub fn drain_song_overlay_updates(
        &mut self,
        screen: ActorScene,
    ) -> Option<Vec<SongReplayPropertyUpdates>> {
        self.actor_scene_root(screen)?;
        self.actors.drain_song_overlay_updates(screen)
    }

    /// Install the complete preparation frame timeline after begin_song_replay,
    /// before chronological callbacks. Rows are [beat, delta seconds]. Optional
    /// actor frames are [native actor delta, active global-frame span].
    pub fn set_song_replay_frames(
        &self,
        screen: ActorScene,
        frames: &[(f64, f64)],
        actor_frames: Option<&[[f32; 2]]>,
    ) -> bool {
        self.actor_scene_root(screen).is_some()
            && self
                .actors
                .set_song_replay_frames(screen, frames, actor_frames)
    }

    /// Advance one installed canonical frame. Script music/beat clocks remain
    /// caller-owned, as in advance_song_replay. Out-of-range frames use the
    /// existing 60 Hz tail extrapolation.
    pub fn advance_song_replay_at_frame(
        &mut self,
        screen: ActorScene,
        frame: usize,
        delta: f64,
    ) -> Result<bool, Error> {
        if self.actor_scene_root(screen).is_none() {
            return Ok(false);
        }
        let Some(seconds) = self.actors.set_song_replay_frame(screen, frame) else {
            return Ok(false);
        };
        self.advance_song_replay(screen, seconds, delta)
    }

    /// Replace the compiler's pending render destinations after a frame drain.
    /// The next tween uses these destinations as its starting values; Finish
    /// publishes them and Stop discards them. Invalid indices/targets leave the
    /// previous pending set intact. This accepts the scheduled render family.
    pub fn set_song_pending_render_updates(
        &self,
        screen: ActorScene,
        values: &[(
            usize,
            crate::SongLuaOverlayUpdateTarget,
            crate::SongLuaOverlayUpdateValue,
        )],
    ) -> bool {
        self.actor_scene_root(screen).is_some()
            && self.actors.set_song_pending_render_updates(screen, values)
    }

    /// Consume immediate and scheduled renderer-property output. This shares
    /// its frame buffer with drain_song_immediate_updates; use one drain per
    /// sampling pass. Tween pose/progress is exported separately.
    pub fn drain_song_property_updates(
        &mut self,
        screen: ActorScene,
    ) -> Option<Vec<SongReplayPropertyUpdates>> {
        self.actor_scene_root(screen)?;
        self.actors.drain_song_property_updates(screen)
    }

    /// Consume immediate-property output since the last drain. Queued setters
    /// retain their dispatch time, starting value and final same-frame value.
    /// This complements tween pose/progress capture. Use
    /// drain_song_property_updates to include scheduled render properties.
    pub fn drain_song_immediate_updates(
        &mut self,
        screen: ActorScene,
    ) -> Option<Vec<SongReplayImmediateUpdates>> {
        self.actor_scene_root(screen)?;
        self.actors.drain_song_immediate_updates(screen)
    }

    /// Message provenance for the tween and immediate setters captured so far.
    /// Draining frame output does not discard this session-level history.
    pub fn song_replay_messages(
        &self,
        screen: ActorScene,
    ) -> Option<Vec<crate::SongLuaStatefulMessageCapture>> {
        self.actor_scene_root(screen)?;
        self.actors.song_replay_messages(screen)
    }

    /// Advance preparation at an explicit song time, separate from actor delta
    /// and the script's music/beat clock. The caller sets script clocks first.
    pub fn advance_song_replay(
        &mut self,
        screen: ActorScene,
        seconds: f64,
        delta: f64,
    ) -> Result<bool, Error> {
        if self.actor_scene_root(screen).is_none() {
            return Ok(false);
        }
        self.actors
            .advance_song_replay(&mut self.vm, screen, seconds, delta)
    }

    /// Sample paused tweens and copy owned progress/pose output. Repeated reads
    /// at the same song time do not introduce duplicate hold samples. Identities
    /// remain owned by the active scene; output itself contains no Lua values.
    pub fn capture_song_tween_replay(
        &self,
        screen: ActorScene,
    ) -> Option<std::collections::HashMap<deadlib_lua::TableId, SongTweenReplay>> {
        self.actor_scene_root(screen)?;
        self.actors.capture_song_tween_replay(screen)
    }

    /// Ordered message writes to the replay's overlay tween properties. Indices
    /// refer to the overlay slice passed to begin_song_replay. Equal-value writes
    /// and cross-actor mutations are retained; output owns no Lua handles.
    /// This exposes tween properties only; the remaining compiler update capture
    /// is a separate integration step.
    pub fn song_replay_tween_messages(
        &self,
        screen: ActorScene,
    ) -> Option<Vec<crate::SongLuaStatefulMessageCapture>> {
        self.actor_scene_root(screen)?;
        self.actors.song_replay_tween_messages(screen)
    }

    /// Enter chronological preparation after the compiler has read discovery
    /// output. Overlay initial states must be the compiler's frame-zero poses;
    /// callback-only actors use their captured immediate startup state.
    /// Restores shared script locals, sprite bindings and overlay getters, then
    /// reconstructs the original Init/On queues. Does not run a frame or emit
    /// compiled tracks; the caller still owns frame sampling and output capture.
    pub fn begin_song_replay(
        &mut self,
        screen: ActorScene,
        overlays: &[(deadlib_lua::TableId, crate::SongLuaOverlayState)],
        activation: &SongActivation,
    ) -> Result<bool, Error> {
        if self.actor_scene_root(screen).is_none() {
            return Ok(false);
        }
        if activation.new_screen.is_some() {
            return self.vm.with_native_stack(|stack| {
                Err(stack.error(b"replaced song screens do not enter preparation replay"))
            });
        }
        self.actors
            .begin_song_replay(&mut self.vm, screen, overlays, &activation.tweens)?;
        Ok(true)
    }

    /// Owned queued-command captures and their actual frame-zero actor states.
    /// Actor identities remain traced by the active scene. Empty before discovery
    /// or for skipped probes; foreign and retired scenes return None.
    pub fn song_startup_queued_states(
        &self,
        screen: ActorScene,
    ) -> Option<std::collections::HashMap<deadlib_lua::TableId, super::SongStartupState>> {
        self.actor_scene_root(screen)?;
        self.actors.song_startup_queued_states(screen)
    }

    /// Probe startup queues once, retaining shared script state until the
    /// compiler has read the discovered declarations. Rewind explicitly at
    /// the chronological replay boundary. Live/replaced screens skip probes.
    pub fn discover_song_startup_commands(
        &mut self,
        screen: ActorScene,
        activation: &SongActivation,
    ) -> Result<bool, Error> {
        if self.actor_scene_root(screen).is_none() {
            return Ok(false);
        }
        self.actors.discover_song_startup(
            &mut self.vm,
            screen,
            activation,
            self.clock.live.load(std::sync::atomic::Ordering::Relaxed),
        )?;
        // Probe-only requests never replace the actual startup screen.
        self.screen.state.lock().unwrap().next = None;
        Ok(true)
    }

    /// Consume the saved pre-probe globals, environments and closure locals.
    /// Actor captures remain available to the compiler's separate replay setup.
    pub fn restore_song_startup_locals(&mut self, screen: ActorScene) -> Result<bool, Error> {
        if self.actor_scene_root(screen).is_none() {
            return Ok(false);
        }
        self.actors
            .restore_song_startup_locals(&mut self.vm, screen)?;
        Ok(true)
    }
}
