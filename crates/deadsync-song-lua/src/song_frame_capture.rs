//! The existing chronological track compiler over Rust replay publications.
use super::{
    actors::perframes::Access,
    song_compiler_roots::{RootedValues, Roots},
    *,
};
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
        initialize_updates: bool,
    ) -> Result<(), String> {
        frame_capture::capture_update_overlay_samples(
            source,
            frame_capture::CaptureFrameArgs {
                context,
                baseline: states.baseline,
                from_states: states.from,
                update_states: states.update,
                initialize_updates,
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
impl SongRuntime {
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
        let mut rooted = RootedValues::default();
        let result = (|| {
            for actor in actors {
                rooted.push(&mut self.vm, *actor)?;
            }
            self.publish_rooted_song_tween_frame(screen, &rooted)
        })();
        rooted.release(&mut self.vm)?;
        result
    }

    pub(super) fn publish_rooted_song_tween_frame(
        &mut self,
        screen: ActorScene,
        actors: &RootedValues,
    ) -> Result<(), String> {
        self.actor_scene_root(screen)
            .ok_or("released or foreign song screen")?;
        actors.validate(&self.vm)?;
        if self
            .actors
            .publish_song_tween_frame(&mut self.vm, screen, actors)
        {
            Ok(())
        } else {
            Err("song replay capture is not active".into())
        }
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
        let mut rooted = RootedValues::default();
        let result = (|| {
            for actor in actors {
                rooted.push(&mut self.vm, *actor)?;
            }
            let access = self.actors.perframe_access();
            capture.sample_schedule(states, seconds, |index, target, value| {
                if let Some(&actor) = rooted.get(index) {
                    access
                        .update_getter(&mut self.vm, actor, target, value)
                        .map_err(|e| e.to_string())?;
                }
                Ok(())
            })
        })();
        rooted.release(&mut self.vm)?;
        result
    }

    pub(super) fn sample_rooted_song_overlay_schedule(
        &mut self,
        screen: ActorScene,
        capture: &SongOverlayFrameCapture,
        actors: &RootedValues,
        states: &mut [State],
        seconds: f64,
    ) -> Result<(), String> {
        self.actor_scene_root(screen)
            .ok_or("released or foreign song screen")?;
        actors.validate(&self.vm)?;
        let mut writes = Vec::new();
        capture.sample_schedule(states, seconds, |index, target, value| {
            if let Some(&actor) = actors.get(index) {
                writes.push((actor, target, value.clone()));
            }
            Ok(())
        })?;
        self.actors
            .perframe_access()
            .update_rooted_getters(&mut self.vm, writes)
            .map_err(|e| e.to_string())
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
        let mut rooted = RootedValues::default();
        let result = (|| {
            for actor in actors {
                rooted.push(&mut self.vm, *actor)?;
            }
            self.capture_rooted_song_overlay_frame(
                screen, capture, &rooted, context, states, clock, false,
            )
        })();
        rooted.release(&mut self.vm)?;
        result
    }

    pub(super) fn capture_rooted_song_overlay_frame(
        &mut self,
        screen: ActorScene,
        capture: &mut SongOverlayFrameCapture,
        actors: &RootedValues,
        context: &SongLuaCompileContext,
        states: SongFrameCaptureStates<'_>,
        clock: SongFrameCaptureClock,
        initialize_updates: bool,
    ) -> Result<(), String> {
        self.actor_scene_root(screen)
            .ok_or("released or foreign song screen")?;
        actors.validate(&self.vm)?;
        let access = self.actors.perframe_access();
        let mut source = Source {
            runtime: self,
            screen,
            actors,
            access,
        };
        capture.capture(&mut source, context, states, clock, initialize_updates)
    }
}
struct Source<'a> {
    runtime: &'a mut SongRuntime,
    screen: ActorScene,
    actors: &'a [Value],
    access: Access,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Publications {
        count: usize,
        rows: Vec<(
            usize,
            Vec<frame_capture::ScheduledUpdate>,
            Vec<(Target, Update)>,
        )>,
        fail: bool,
    }
    impl CaptureSource for Publications {
        fn actor_count(&self) -> usize {
            self.count
        }
        fn chronological(&self) -> bool {
            true
        }
        fn drain(&mut self, visit: &mut CaptureVisitor<'_>) -> Result<(), String> {
            for (index, scheduled, final_values) in &self.rows {
                visit(*index, &[], scheduled, final_values, false, &mut || {
                    Ok(false)
                })?;
                if self.fail {
                    return Err("publication failed".into());
                }
            }
            Ok(())
        }
        fn reset(&mut self, _: &[usize]) -> Result<(), String> {
            Ok(())
        }
    }

    fn tween(target: Target, value: f32) -> frame_capture::ScheduledUpdate {
        frame_capture::ScheduledUpdate {
            progress: None,
            dispatch_seconds: None,
            frame_advance: 0.,
            initial_value: None,
            delay_seconds: 0.,
            duration_seconds: 1.,
            easing: None,
            opt1: None,
            target,
            value: Update::F32(value),
        }
    }

    #[test]
    fn replay_capture_keeps_completed_and_restored_states_across_sparse_publications() {
        let context = SongLuaCompileContext::new(std::path::Path::new("."), "sparse capture");
        let mut baseline = vec![State::default(); 3];
        let mut states = baseline.clone();
        for (state, x) in states.iter_mut().zip([10., 30., 50.]) {
            state.x = x;
        }
        let mut updates = vec![
            State {
                x: -999.,
                ..State::default()
            };
            3
        ];
        let mut capture = SongOverlayFrameCapture::default();
        frame_capture::append_scheduled_overlay_updates(
            &mut capture.scheduled,
            &context,
            &states,
            0,
            &[tween(Target::Y, 40.)],
            0.,
        );
        let mut source = Publications {
            count: 3,
            rows: vec![
                (0, vec![], vec![(Target::X, Update::F32(60.))]),
                (0, vec![tween(Target::X, 100.)], vec![]),
                (2, vec![tween(Target::X, 150.)], vec![]),
            ],
            fail: false,
        };
        let run = |source: &mut Publications,
                   baseline: &[State],
                   states: &mut [State],
                   updates: &mut [State],
                   capture: &mut SongOverlayFrameCapture,
                   at: f64| {
            let from = states.to_vec();
            capture.capture(
                source,
                &context,
                SongFrameCaptureStates {
                    baseline,
                    from: &from,
                    update: updates,
                    to: states,
                    restored_indices: &[0],
                },
                SongFrameCaptureClock {
                    prior_time: 0.,
                    next_time: at as f32,
                    next_seconds: at,
                },
                true,
            )
        };
        run(
            &mut source,
            &baseline,
            &mut states,
            &mut updates,
            &mut capture,
            1.,
        )
        .unwrap();
        assert_eq!((states[0].x, states[0].y), (10., 40.));
        assert_eq!((updates[0].x, updates[0].y), (60., 40.));
        assert_eq!(capture.scheduled[0].from, Update::F32(60.));
        assert_eq!(capture.scheduled[1].from, Update::F32(50.));

        // A previously idle actor and a newly appended wrapper must initialize
        // from this frame, without carrying another actor's scratch state.
        states[1].x = 80.;
        states.push(State {
            x: 110.,
            ..State::default()
        });
        baseline.push(State::default());
        updates.push(State::default());
        source.count = 4;
        source.rows = vec![
            (1, vec![tween(Target::X, 180.)], vec![]),
            (3, vec![tween(Target::X, 220.)], vec![]),
        ];
        run(
            &mut source,
            &baseline,
            &mut states,
            &mut updates,
            &mut capture,
            1.5,
        )
        .unwrap();
        assert_eq!(capture.scheduled[2].from, Update::F32(80.));
        assert_eq!(capture.scheduled[3].from, Update::F32(110.));

        // A failed publication must not leave initialization flags live for
        // the next capture, even when it touches the same actor again.
        source.rows = vec![(1, vec![], vec![(Target::X, Update::F32(90.))])];
        source.fail = true;
        assert!(
            run(
                &mut source,
                &baseline,
                &mut states,
                &mut updates,
                &mut capture,
                1.6
            )
            .is_err()
        );
        states[1].x = 123.;
        source.rows = vec![(1, vec![tween(Target::Z, 250.)], vec![])];
        source.fail = false;
        run(
            &mut source,
            &baseline,
            &mut states,
            &mut updates,
            &mut capture,
            1.7,
        )
        .unwrap();
        assert_eq!(updates[1].x, 123.);
    }
}
impl CaptureSource for Source<'_> {
    fn actor_count(&self) -> usize {
        self.actors.len()
    }
    fn chronological(&self) -> bool {
        self.runtime
            .actors
            .chronological_song_capture(&self.runtime.vm)
    }
    fn drain(&mut self, visit: &mut CaptureVisitor<'_>) -> Result<(), String> {
        let frames = self
            .runtime
            .actors
            .take_song_overlay_frame_updates(&mut self.runtime.vm, self.screen, true)
            .ok_or("song replay capture is not active")?;
        let result = (|| {
            let mut player_index_key = None;
            for frame in &frames {
                let mut player = || {
                    let actor = *self
                        .actors
                        .get(frame.overlay_index)
                        .ok_or("song capture actor index out of range")?;
                    let key = match player_index_key {
                        Some(key) => key,
                        None => {
                            let key = self
                                .runtime
                                .vm
                                .intern_static(b"__songlua_player_index")
                                .map_err(|e| e.to_string())?;
                            player_index_key = Some(key);
                            key
                        }
                    };
                    self.access
                        .is_player(&mut self.runtime.vm, actor, key)
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
            self.runtime.actors.restore_song_overlay_updates(
                &mut self.runtime.vm,
                self.screen,
                frames,
            );
        } else {
            self.runtime.actors.recycle_song_overlay_updates(
                &mut self.runtime.vm,
                self.screen,
                frames,
            );
        }
        result
    }
    fn reset(&mut self, indices: &[usize]) -> Result<(), String> {
        self.access
            .reset_frames(
                &mut self.runtime.vm,
                indices.iter().map(|&index| self.actors[index]),
            )
            .map_err(|e| e.to_string())
    }
}
