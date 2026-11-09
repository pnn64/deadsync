//! The existing chronological track compiler over Rust replay publications.
use super::{actors::perframes::Access, song_compiler_roots::Roots, *};
use crate::{
    SongLuaOverlayState as State, SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateTrack,
    SongLuaOverlayUpdateValue as Update,
    frame_capture::{
        self, CaptureSource, CaptureVisitor, OverlaySampleScratch, SongLuaScheduledOverlaySample,
    },
};
use rustc_hash::FxHashMap;

/// The caller keeps prior, message-replayed and updated poses distinct.
pub struct SongFrameCaptureStates<'a> {
    pub baseline: &'a [State],
    pub from: &'a [State],
    pub update: &'a mut [State],
    pub to: &'a mut [State],
    pub restored_indices: &'a [usize],
}
#[derive(Clone, Copy)]
pub struct SongFrameCaptureClock {
    pub prior_time: f32,
    pub next_time: f32,
    pub next_seconds: f64,
}
/// Compilation-owned tracks, pending tweens and reusable scratch. No Lua values
/// survive a call; the screen and its actor batch own their own rooted lifetimes.
#[derive(Default)]
pub struct SongOverlayFrameCapture {
    pub(super) tracks: Vec<SongLuaOverlayUpdateTrack>,
    pub(super) indices: FxHashMap<(usize, Target), usize>,
    pub(super) scheduled: Vec<SongLuaScheduledOverlaySample>,
    pub(super) scratch: OverlaySampleScratch,
}
impl SongOverlayFrameCapture {
    pub fn tracks(&self) -> &[SongLuaOverlayUpdateTrack] {
        &self.tracks
    }
    pub fn stopped_indices(&self) -> &[usize] {
        &self.scratch.stopped_indices
    }
    pub fn retargeted_values(&self) -> &[(usize, Target, Update)] {
        &self.scratch.retargeted_states
    }
    pub fn pending_updates(&self, seconds: f64) -> Vec<(usize, Target, Update)> {
        self.scheduled
            .iter()
            .filter(|sample| {
                sample.end_seconds > frame_capture::scheduled_overlay_clock(sample, seconds)
            })
            .map(|sample| (sample.overlay_index, sample.target, sample.value.clone()))
            .collect()
    }
    /// Apply the exact scheduled clock and interpolation order. The enclosing
    /// compiler supplies its engine-specific getter handoff for each write.
    pub fn sample_schedule(
        &self,
        states: &mut [State],
        seconds: f64,
        write: impl FnMut(usize, Target, &Update) -> Result<(), String>,
    ) -> Result<(), String> {
        frame_capture::apply_scheduled_overlay_states(states, &self.scheduled, seconds, write)
    }
    pub fn finish(mut self) -> Vec<SongLuaOverlayUpdateTrack> {
        for track in &mut self.tracks {
            frame_capture::sort_overlay_update_samples(&mut track.samples);
        }
        self.tracks
    }
    pub(super) fn capture(
        &mut self,
        source: &mut impl CaptureSource,
        context: &SongLuaCompileContext,
        states: SongFrameCaptureStates<'_>,
        clock: SongFrameCaptureClock,
    ) -> Result<(), String> {
        frame_capture::capture_update_overlay_samples(
            source,
            frame_capture::CaptureFrameArgs {
                context,
                baseline: states.baseline,
                from_states: states.from,
                update_states: states.update,
                to_states: states.to,
                restored_indices: states.restored_indices,
                tracks: &mut self.tracks,
                track_indices: &mut self.indices,
                time: clock.prior_time,
                next_time: clock.next_time,
                next_seconds: clock.next_seconds,
                scheduled_samples: &mut self.scheduled,
                scratch: &mut self.scratch,
            },
        )
    }
}
impl RustSongRuntime {
    /// Publish current native-queue poses after advancing the actor tree and
    /// before frame-track capture. Paused progress and empty touched rows retain
    /// the existing native ordering without cloning compilation progress arrays.
    pub fn publish_song_tween_frame(
        &mut self,
        screen: ActorScene,
        actors: &[Value],
    ) -> Result<(), String> {
        self.actor_scene_root(screen)
            .ok_or("released or foreign song screen")?;
        let mut roots = Roots::default();
        let result = (|| {
            for actor in actors {
                roots.keep(&mut self.vm, *actor)?;
            }
            if self.actors.publish_song_tween_frame(screen, actors) {
                Ok(())
            } else {
                Err("song replay capture is not active".into())
            }
        })();
        roots.clear(&mut self.vm)?;
        result
    }

    /// Apply the full preparation getter handoff in supplied order. This is
    /// separate from actor setters: it neither queues tweens nor records writes.
    pub fn set_song_overlay_getter_values(
        &mut self,
        screen: ActorScene,
        actors: &[Value],
        values: &[(usize, Target, Update)],
    ) -> Result<(), String> {
        self.actor_scene_root(screen)
            .ok_or("released or foreign song screen")?;
        let mut roots = Roots::default();
        let result = (|| {
            for actor in actors {
                roots.keep(&mut self.vm, *actor)?;
            }
            let access = self.actors.perframe_access();
            for (index, target, value) in values {
                let actor = *actors
                    .get(*index)
                    .ok_or("song getter actor index out of range")?;
                access
                    .update_getter(&mut self.vm, actor, *target, value)
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        roots.clear(&mut self.vm)?;
        result
    }

    /// Sample schedules and hand each value to the real Rust preparation
    /// getters before the next actor callback. Missing actor slots are skipped,
    /// matching the existing native sampler's state-only tail behavior.
    pub fn sample_song_overlay_schedule(
        &mut self,
        screen: ActorScene,
        capture: &SongOverlayFrameCapture,
        actors: &[Value],
        states: &mut [State],
        seconds: f64,
    ) -> Result<(), String> {
        self.actor_scene_root(screen)
            .ok_or("released or foreign song screen")?;
        let mut roots = Roots::default();
        let result = (|| {
            for actor in actors {
                roots.keep(&mut self.vm, *actor)?;
            }
            let access = self.actors.perframe_access();
            capture.sample_schedule(states, seconds, |index, target, value| {
                if let Some(actor) = actors.get(index) {
                    access
                        .update_getter(&mut self.vm, *actor, target, value)
                        .map_err(|e| e.to_string())?;
                }
                Ok(())
            })
        })();
        roots.clear(&mut self.vm)?;
        result
    }

    /// Consume the active replay's complete ordered publication batch. Actor
    /// order must match begin_song_replay, including temporary players/layers.
    pub fn capture_song_overlay_frame(
        &mut self,
        screen: ActorScene,
        capture: &mut SongOverlayFrameCapture,
        actors: &[Value],
        context: &SongLuaCompileContext,
        states: SongFrameCaptureStates<'_>,
        clock: SongFrameCaptureClock,
    ) -> Result<(), String> {
        self.actor_scene_root(screen)
            .ok_or("released or foreign song screen")?;
        let mut roots = Roots::default();
        let result = (|| {
            for actor in actors {
                roots.keep(&mut self.vm, *actor)?;
            }
            let access = self.actors.perframe_access();
            let mut source = Source {
                runtime: self,
                screen,
                actors,
                access,
            };
            capture.capture(&mut source, context, states, clock)
        })();
        roots.clear(&mut self.vm)?;
        result
    }
}
struct Source<'a> {
    runtime: &'a mut RustSongRuntime,
    screen: ActorScene,
    actors: &'a [Value],
    access: Access,
}
impl CaptureSource for Source<'_> {
    fn actor_count(&self) -> usize {
        self.actors.len()
    }
    fn chronological(&self) -> bool {
        self.runtime.actors.chronological_song_capture()
    }
    fn drain(&mut self, visit: &mut CaptureVisitor<'_>) -> Result<(), String> {
        let frames = self
            .runtime
            .actors
            .take_song_overlay_frame_updates(self.screen, true)
            .ok_or("song replay capture is not active")?;
        let result = (|| {
            for frame in &frames {
                let mut player = || {
                    let actor = *self
                        .actors
                        .get(frame.overlay_index)
                        .ok_or("song capture actor index out of range")?;
                    self.access
                        .is_player(&mut self.runtime.vm, actor)
                        .map_err(|e| e.to_string())
                };
                visit(
                    frame.overlay_index,
                    &frame.values,
                    &frame.scheduled,
                    &frame.final_values,
                    frame.tween_reset,
                    &mut player,
                )?;
            }
            Ok(())
        })();
        if result.is_err() {
            self.runtime
                .actors
                .restore_song_overlay_updates(self.screen, frames);
        }
        result
    }
    fn reset(&mut self, index: usize) -> Result<(), String> {
        self.access
            .reset_frame(&mut self.runtime.vm, self.actors[index])
            .map_err(|e| e.to_string())
    }
}
