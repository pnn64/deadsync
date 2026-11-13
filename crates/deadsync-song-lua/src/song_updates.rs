//! Production chronological song compilation over the shared Rust engine.
use super::{song_compiler_roots::Roots, *};
use crate::{
    SongLuaColumnOffsetBuildParams, SongLuaColumnOffsetWindow, SongLuaEaseWindow,
    SongLuaMessageEvent, SongLuaOverlayKind as Kind, SongLuaOverlayState as State,
    SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Update,
    SongLuaPerframePlayerState as Player, SongLuaSpanMode, SongLuaTimeUnit,
    SongLuaTrackedActorTarget as Tracked, frame_capture::*,
};
use actors::{
    chronological::{Answer, Query},
    perframes::Access,
};
use std::{
    collections::{BTreeMap, HashMap},
};
type MotionSample = (Option<[f32; 2]>, Option<[f32; 3]>);

#[derive(Default)]
pub struct SongUpdateOutput {
    pub eases: Vec<SongLuaEaseWindow>,
    pub overlay_eases: Vec<crate::SongLuaOverlayEase>,
    pub overlay_tracks: Vec<crate::SongLuaOverlayUpdateTrack>,
    #[cfg(feature = "test-support")]
    pub overlay_writes: Vec<crate::SongLuaOverlayUpdateTrack>,
    pub column_transforms: Vec<SongLuaColumnOffsetWindow>,
    pub stateful_messages: Vec<crate::SongLuaStatefulMessageCapture>,
    pub broadcasts: Vec<(f32, String, bool)>,
    pub column_splines: Vec<deadsync_gameplay::SongLuaColumnSplineTrack>,
}
struct Replay<'a> {
    messages: &'a [SongLuaMessageEvent],
    order: Vec<usize>,
    next: usize,
    active: Vec<Option<super::song_perframes::Active>>,
    access: Access,
    started: Vec<usize>,
    seen: Vec<bool>,
}
impl<'a> Replay<'a> {
    fn new(messages: &'a [SongLuaMessageEvent], count: usize, access: Access) -> Self {
        let mut order = (0..messages.len()).collect::<Vec<_>>();
        order.sort_by(|&a, &b| messages[a].beat.total_cmp(&messages[b].beat));
        Self {
            messages,
            order,
            next: 0,
            active: vec![None; count],
            access,
            started: Vec::new(),
            seen: vec![false; count],
        }
    }
    fn stop(&mut self, indices: &[usize]) {
        for &i in indices {
            if let Some(a) = self.active.get_mut(i) {
                *a = None;
            }
        }
    }
    fn advance<K>(
        &mut self,
        runtime: &mut SongRuntime,
        context: &SongLuaCompileContext,
        rows: &mut SongOverlayActors<K>,
        values: &[Value],
        states: &mut [State],
        beat: f32,
    ) -> Result<&[usize], String> {
        for &i in &self.started {
            self.seen[i] = false;
        }
        self.started.clear();
        while let Some(&i) = self.order.get(self.next) {
            let event = &self.messages[i];
            if event.beat > beat + f32::EPSILON {
                break;
            }
            for (i, row) in rows.actors.iter_mut().enumerate() {
                let target = SongMessageTarget {
                    actor: values[i],
                    commands: &mut row.actor.message_commands,
                };
                let was = self.active[i].is_some();
                super::song_perframes::apply_message_with_pose(
                    runtime,
                    &self.access,
                    context,
                    &target,
                    &mut self.active[i],
                    event.beat,
                    true,
                )?;
                if was {
                    states[i] = self
                        .access
                        .state(&mut runtime.vm, values[i])
                        .map_err(|e| e.to_string())?;
                }
                if let Some(command) = target
                    .commands
                    .iter()
                    .position(|c| c.message == event.message)
                {
                    self.active[i] = Some(super::song_perframes::Active {
                        command,
                        start: event.beat,
                        elapsed: None,
                        base: states[i],
                    });
                    if !self.seen[i] {
                        self.seen[i] = true;
                        self.started.push(i);
                    }
                    super::song_perframes::apply_message_with_pose(
                        runtime,
                        &self.access,
                        context,
                        &target,
                        &mut self.active[i],
                        event.beat,
                        true,
                    )?;
                    states[i] = self
                        .access
                        .state(&mut runtime.vm, values[i])
                        .map_err(|e| e.to_string())?;
                }
            }
            self.next += 1;
        }
        for (i, row) in rows.actors.iter_mut().enumerate().take(self.active.len()) {
            let was = self.active[i].is_some();
            let target = SongMessageTarget {
                actor: values[i],
                commands: &mut row.actor.message_commands,
            };
            super::song_perframes::apply_message_with_pose(
                runtime,
                &self.access,
                context,
                &target,
                &mut self.active[i],
                beat,
                true,
            )?;
            if was {
                states[i] = self
                    .access
                    .state(&mut runtime.vm, values[i])
                    .map_err(|e| e.to_string())?;
            }
        }
        self.started.sort_unstable();
        Ok(&self.started)
    }
}
fn push(
    capture: &mut SongOverlayFrameCapture,
    index: usize,
    target: Target,
    prior: f32,
    from: &State,
    next: f32,
    value: &Update,
) {
    push_captured_overlay_value(
        &mut capture.tracks,
        &mut capture.indices,
        index,
        target,
        prior,
        from,
        next,
        value,
    );
}
fn buffer<T>(value: T, count: usize) -> Vec<T> {
    let mut rows = Vec::with_capacity(count);
    rows.push(value);
    rows
}
impl SongRuntime {
    /// Compile the existing update-function path after static schedules and
    /// perframes. Actor batches retain their original rooted identities; output
    /// owns no Lua values. An enclosing compiler discards the VM on failure.
    #[allow(clippy::too_many_arguments)]
    pub fn compile_song_update_functions<S, V, A>(
        &mut self,
        screen: ActorScene,
        root: Value,
        context: &SongLuaCompileContext,
        overlays: &mut SongOverlayActors<Kind<S, V, A>>,
        tracked: &mut SongTrackedActors,
        messages: &[SongLuaMessageEvent],
        activation: &SongActivation,
        sound_events: &mut Vec<crate::SongLuaSoundEvent>,
        judgment_textures: &mut Vec<(usize, crate::SongLuaJudgmentTexture)>,
        sprite_textures: &mut Vec<(usize, crate::SongLuaSpriteTexture)>,
    ) -> Result<SongUpdateOutput, String> {
        self.actor_scene_root(screen)
            .ok_or("released or foreign song screen")?;
        let mut roots = Roots::default();
        let result = (|| {
            roots.keep(&mut self.vm, root)?;
            let owner = overlays.owner.ok_or("released song overlay batch")?;
            self.vm.rooted(owner).map_err(|e| e.to_string())?;
            let owner = tracked.owner.ok_or("released song tracked batch")?;
            self.vm.rooted(owner).map_err(|e| e.to_string())?;
            let mut values = Vec::new();
            for row in &overlays.actors {
                let value = self
                    .song_overlay_actor_value(row)
                    .map_err(|e| e.to_string())?;
                roots.keep(&mut self.vm, value)?;
                values.push(value);
            }
            let mut tracked_values = Vec::new();
            for row in &tracked.actors {
                let value = self
                    .song_tracked_actor_value(row)
                    .map_err(|e| e.to_string())?;
                roots.keep(&mut self.vm, value)?;
                tracked_values.push(value);
            }
            self.compile_song_updates_inner(
                screen,
                root,
                context,
                overlays,
                tracked,
                messages,
                activation,
                sound_events,
                judgment_textures,
                sprite_textures,
                &mut roots,
                values,
                &tracked_values,
            )
        })();
        #[cfg(feature = "test-support")]
        self.observations.stop();
        roots.clear(&mut self.vm)?;
        result
    }
    #[allow(clippy::too_many_arguments)]
    fn compile_song_updates_inner<S, V, A>(
        &mut self,
        screen: ActorScene,
        root: Value,
        context: &SongLuaCompileContext,
        overlays: &mut SongOverlayActors<Kind<S, V, A>>,
        tracked: &mut SongTrackedActors,
        messages: &[SongLuaMessageEvent],
        activation: &SongActivation,
        sound_events: &mut Vec<crate::SongLuaSoundEvent>,
        judgment_textures: &mut Vec<(usize, crate::SongLuaJudgmentTexture)>,
        sprite_textures: &mut Vec<(usize, crate::SongLuaSpriteTexture)>,
        roots: &mut Roots,
        mut values: Vec<Value>,
        tracked_values: &[Value],
    ) -> Result<SongUpdateOutput, String> {
        let mut splines = SongColumnSplineCapture::default();
        self.capture_song_column_splines(&mut splines, 0.)?;
        let start = crate::song_beat_at_elapsed_seconds(0., context);
        #[cfg(feature = "test-support")]
        let start = context
            .compile_clock_inputs
            .first()
            .map_or(start, |f| f[1] as f32);
        let end = crate::update_function_end_beat(context);
        let Answer::Active(active) = self
            .actors
            .chronological_read(&mut self.vm, root, Query::Active)
            .map_err(|e| e.to_string())?
        else {
            unreachable!()
        };
        if (!active && !self.actors.models_active()) || end <= start {
            let static_output = self.compile_song_static_update_output(context, splines)?;
            return Ok(SongUpdateOutput {
                eases: static_output.eases,
                column_transforms: static_output.column_transforms,
                column_splines: static_output.column_splines,
                ..Default::default()
            });
        }
        self.restore_song_startup_locals(screen)
            .map_err(|e| e.to_string())?;
        sound_events.retain(|e| !e.queued_startup);
        let access = self.actors.perframe_access();
        let mut options = self
            .actors
            .update_options(&mut self.vm)
            .map_err(|e| e.to_string())?;
        let result = (|| {
            for &value in values.iter().chain(tracked_values) {
                access
                    .reset_frame(&mut self.vm, value)
                    .map_err(|e| e.to_string())?;
            }
            let count = overlays.actors.len();
            let initials = values
                .iter()
                .zip(&overlays.actors)
                .map(|(&value, row)| {
                    let Value::Table(id) = value else {
                        unreachable!()
                    };
                    (id, row.actor.initial_state)
                })
                .collect::<Vec<_>>();
            self.begin_song_replay(screen, &initials, activation)
                .map_err(|e| e.to_string())?;
            let mut baseline = values
                .iter()
                .map(|&v| access.state(&mut self.vm, v).map_err(|e| e.to_string()))
                .collect::<Result<Vec<_>, _>>()?;
            let mut players = [None; LUA_PLAYERS];
            let mut player_indices = [None; LUA_PLAYERS];
            for (row, &value) in tracked.actors.iter().zip(tracked_values) {
                if let Tracked::Player(player) = row.target {
                    players[player] = Some(value);
                }
            }
            for (player, value) in players.iter().enumerate() {
                if let Some(value) = value {
                    player_indices[player] = Some(values.len());
                    values.push(*value);
                    let state = access
                        .state(&mut self.vm, *value)
                        .map_err(|e| e.to_string())?;
                    baseline.push(self.update_pose(*value, state)?);
                }
            }
            let mut layers = Vec::new();
            for (i, (row, &value)) in tracked.actors.iter().zip(tracked_values).enumerate() {
                if matches!(
                    row.target,
                    Tracked::SongForeground | Tracked::ScreenLayer(_)
                ) {
                    layers.push((i, values.len()));
                    values.push(value);
                    baseline.push(
                        access
                            .state(&mut self.vm, value)
                            .map_err(|e| e.to_string())?,
                    );
                }
            }
            self.actors.replace_song_capture_indices(screen, &values);
            self.begin_actor_capture(false);
            self.actors.begin_song_wrapper_capture(screen, values.len());
            #[cfg(feature = "test-support")]
            self.observations.start();
            let wrapper_start = values.len();
            let mut replay = Replay::new(messages, count, self.actors.perframe_access());
            let mut replay_states = baseline.clone();
            let started =
                replay.advance(self, context, overlays, &values, &mut replay_states, start)?;
            self.restore_update_messages(&access, &values, &replay_states, started)?;
            self.append_update_wrappers(
                screen,
                roots,
                &mut values,
                &mut [&mut baseline, &mut replay_states],
            )?;
            for row in &mut tracked.actors {
                if let Tracked::Player(player) = row.target
                    && let Some(i) = player_indices[player]
                {
                    let state = self.update_pose(values[i], replay_states[i])?;
                    access
                        .write_frame(&mut self.vm, values[i], state)
                        .map_err(|e| e.to_string())?;
                    baseline[i] = state;
                    replay_states[i] = state;
                    row.actor.initial_state = state;
                }
            }
            let mut update_states = replay_states.clone();
            let baseline_players = self.update_players(&access, &players)?;
            let baseline_mods = options
                .sample(&mut self.vm, false)
                .map_err(|e| e.to_string())?;
            let baseline_columns = self.read_song_note_column_transform_samples()?;
            let frames = crate::perframe::update_function_replay_beats(context, start, end);
            let actor_frames = crate::perframe::reference_actor_frames(context);
            let sample_count = frames.len();
            let mut sample_beats = buffer(start, sample_count);
            let rate = f64::from(song_music_rate(context));
            let origin = context
                .song_timing
                .as_ref()
                .map(|t| t.get_time_for_beat_exact(0.));
            let frame_time =
                |beat, seconds: f64| origin.map_or(beat, |origin| (seconds * rate) as f32 + origin);
            let mut sample_seconds = buffer(
                (f64::from(song_elapsed_seconds_at(start, context)) * rate) as f32,
                sample_count,
            );
            let mut player_samples = buffer(baseline_players, sample_count);
            let mut mod_samples = buffer(baseline_mods.clone(), sample_count);
            let mut mod_speed_samples = buffer(
                options
                    .sample(&mut self.vm, true)
                    .map_err(|e| e.to_string())?,
                sample_count,
            );
            let mut column_samples = buffer(baseline_columns, sample_count);
            let mut texts = Vec::new();
            for (i, row) in overlays.actors.iter().enumerate() {
                if let Kind::BitmapText { text, .. } = &row.actor.kind {
                    let Value::Table(id) = values[i] else {
                        unreachable!()
                    };
                    let value = self
                        .vm
                        .try_intern(text.as_bytes())
                        .map_err(|e| e.to_string())?;
                    let key = self.vm.try_intern(b"Text").map_err(|e| e.to_string())?;
                    self.vm
                        .raw_table_set(id, key, value)
                        .map_err(|e| e.to_string())?;
                    texts.push((i, Vec::<(f32, Arc<str>)>::new()));
                }
            }
            let mut capture = SongOverlayFrameCapture::default();
            let mut current = replay_states.clone();
            if !self.set_song_replay_frames(screen, &frames, actor_frames.as_deref()) {
                return Err("song replay timeline was rejected".into());
            }
            let start_seconds = f64::from(song_elapsed_seconds_at(start, context));
            let start_time = frame_time(start, start_seconds);
            self.capture_song_overlay_frame(
                screen,
                &mut capture,
                &values,
                context,
                SongFrameCaptureStates {
                    baseline: &baseline,
                    from: &baseline,
                    update: &mut update_states,
                    to: &mut current,
                    restored_indices: started,
                },
                SongFrameCaptureClock {
                    prior_time: start_time,
                    next_time: start_time,
                    next_seconds: start_seconds,
                },
            )?;
            replay.stop(capture.stopped_indices());
            let music = self.update_music_seconds(start_seconds, context)?;
            let mut field_bytes = 0;
            self.capture_update_fields(
                tracked,
                tracked_values,
                music,
                [music, start],
                &mut field_bytes,
            )?;
            let mut prior_field = music;
            for (i, &value) in values.iter().enumerate() {
                let (time, _) = self.update_motion(value, [music, start], false)?;
                if let Some(clock) = time {
                    let v = Update::Vec2(clock);
                    set_overlay_state_update_value(&mut replay_states[i], Target::EffectTime, &v);
                    push(
                        &mut capture,
                        i,
                        Target::EffectTime,
                        start_time,
                        &current[i],
                        start_time,
                        &v,
                    );
                    set_overlay_state_update_value(&mut current[i], Target::EffectTime, &v);
                }
            }
            let mut beat = start;
            let mut seconds = start_seconds;
            let mut scheduled = baseline.clone();
            let mut masks = self.update_masks(&players)?;
            let mut broadcasts = Vec::new();

            for (frame, (exact_beat, delta)) in frames.into_iter().enumerate().skip(1) {
                if frame & 63 == 0 {
                    context.report_bake_progress(
                        0.45 + 0.50 * frame as f32 / sample_count.max(1) as f32,
                    );
                }
                let next_beat = exact_beat as f32;
                let prior_time = frame_time(beat, seconds);
                seconds += delta;
                let next_time = frame_time(next_beat, seconds);
                for &value in tracked_values {
                    access
                        .reset_frame(&mut self.vm, value)
                        .map_err(|e| e.to_string())?;
                }
                replay_states.copy_from_slice(&current);
                let started = replay.advance(
                    self,
                    context,
                    overlays,
                    &values,
                    &mut replay_states,
                    next_beat,
                )?;
                self.sample_song_overlay_schedule(
                    screen,
                    &capture,
                    &values,
                    &mut scheduled,
                    seconds,
                )?;
                if !self.set_song_pending_overlay_updates(screen, &capture.pending_updates(seconds))
                {
                    return Err("pending song tween destinations were rejected".into());
                }
                self.set_song_replay_prior_positions(screen, &current);
                let delta = actor_frames
                    .as_ref()
                    .map_or(f64::from(seconds as f32 - (seconds - delta) as f32), |f| {
                        f64::from(f[frame][0])
                    });
                self.call_song_update_frame(
                    (screen, root),
                    frame,
                    exact_beat,
                    seconds,
                    next_beat - beat,
                    delta,
                )?;
                self.actors.sample_model_materials(seconds as f32);
                self.append_update_wrappers(
                    screen,
                    roots,
                    &mut values,
                    &mut [
                        &mut baseline,
                        &mut current,
                        &mut replay_states,
                        &mut update_states,
                        &mut scheduled,
                    ],
                )?;
                for (i, samples) in &mut texts {
                    let text = self.update_text(values[*i])?;
                    let Kind::BitmapText { text: initial, .. } = &overlays.actors[*i].actor.kind
                    else {
                        unreachable!()
                    };
                    if samples.last().map_or(initial.as_ref(), |s| s.1.as_ref()) != text {
                        samples.push((next_time, text.into()));
                    }
                }
                self.publish_song_tween_frame(screen, &values)?;
                for (i, &value) in values.iter().enumerate().take(count) {
                    replay_states[i] = self.update_pose(value, replay_states[i])?;
                }
                self.restore_update_messages(&access, &values, &replay_states, started)?;
                update_states.copy_from_slice(&replay_states);
                let next_masks = self.update_masks(&players)?;
                self.capture_song_overlay_frame(
                    screen,
                    &mut capture,
                    &values,
                    context,
                    SongFrameCaptureStates {
                        baseline: &baseline,
                        from: &current,
                        update: &mut update_states,
                        to: &mut replay_states,
                        restored_indices: started,
                    },
                    SongFrameCaptureClock {
                        prior_time,
                        next_time,
                        next_seconds: seconds,
                    },
                )?;
                replay.stop(capture.stopped_indices());
                self.sample_song_overlay_schedule(
                    screen,
                    &capture,
                    &values,
                    &mut replay_states,
                    seconds,
                )?;
                for (i, t, v) in capture.scratch.retargeted_states.drain(..) {
                    set_overlay_state_update_value(&mut replay_states[i], t, &v);
                    push_captured_overlay_value(
                        &mut capture.tracks,
                        &mut capture.indices,
                        i,
                        t,
                        prior_time,
                        &current[i],
                        next_time,
                        &v,
                    );
                }
                for sample in &capture.scheduled {
                    let (i, t) = (sample.overlay_index, sample.target);
                    if (i >= count && i < wrapper_start)
                        || scheduled_overlay_clock(sample, seconds) > sample.end_seconds
                    {
                        continue;
                    }
                    let value = overlay_state_update_value(&replay_states[i], t);
                    push_captured_overlay_value(
                        &mut capture.tracks,
                        &mut capture.indices,
                        i,
                        t,
                        prior_time,
                        &current[i],
                        next_time,
                        &value,
                    );
                }
                for index in 0..capture.tracks.len() {
                    let (i, t) = (
                        capture.tracks[index].overlay_index,
                        capture.tracks[index].target,
                    );
                    if (i >= count && i < wrapper_start)
                        || (t != Target::Zoom && !PLAYER_TRANSFORM_TARGETS.contains(&t))
                    {
                        continue;
                    }
                    let value = overlay_state_update_value(&replay_states[i], t);
                    push_captured_overlay_value(
                        &mut capture.tracks,
                        &mut capture.indices,
                        i,
                        t,
                        prior_time,
                        &current[i],
                        next_time,
                        &value,
                    );
                }
                let music = self.update_music_seconds(seconds, context)?;
                for (i, &value) in values.iter().enumerate() {
                    let (time, spin) = self.update_motion(value, [music, next_beat], true)?;
                    if let Some(clock) = time {
                        let clock = current[i]
                            .effect_time
                            .filter(|[at, units]| {
                                (units + (clock[0] - at)).to_bits() == clock[1].to_bits()
                            })
                            .unwrap_or(clock);
                        let v = Update::Vec2(clock);
                        set_overlay_state_update_value(
                            &mut replay_states[i],
                            Target::EffectTime,
                            &v,
                        );
                        push(
                            &mut capture,
                            i,
                            Target::EffectTime,
                            prior_time,
                            &current[i],
                            next_time,
                            &v,
                        );
                    }
                    if let Some(rotation) = spin {
                        for player in 0..LUA_PLAYERS {
                            if player_indices[player] == Some(i) {
                                masks[player] |= (1 << 3) | (1 << 4) | (1 << 5);
                            }
                        }
                        for (t, angle) in [Target::RotationX, Target::RotationY, Target::RotationZ]
                            .into_iter()
                            .zip(rotation)
                        {
                            let v = Update::F32(angle);
                            set_overlay_state_update_value(&mut replay_states[i], t, &v);
                            if !overlay_state_matches_update_value(&current[i], t, &v) {
                                push(&mut capture, i, t, prior_time, &current[i], next_time, &v);
                            }
                        }
                    }
                }
                let mut next_players = self.update_players(&access, &players)?;
                for player in 0..LUA_PLAYERS {
                    masks[player] |= next_masks[player];
                    if let Some(i) = player_indices[player] {
                        crate::perframe::sample_player_pose(
                            &mut next_players[player],
                            &replay_states[i],
                            Some(masks[player]),
                        );
                    }
                }
                sample_beats.push(next_beat);
                sample_seconds.push((seconds * rate) as f32);
                player_samples.push(next_players);
                mod_samples.push(
                    options
                        .sample(&mut self.vm, false)
                        .map_err(|e| e.to_string())?,
                );
                mod_speed_samples.push(
                    options
                        .sample(&mut self.vm, true)
                        .map_err(|e| e.to_string())?,
                );
                column_samples.push(self.read_song_note_column_transform_samples()?);
                self.capture_song_column_splines(&mut splines, (seconds * rate) as f32)?;
                self.capture_update_fields(
                    tracked,
                    tracked_values,
                    prior_field,
                    [music, next_beat],
                    &mut field_bytes,
                )?;
                prior_field = music;
                let mut layer_message = None;
                for &(tracked_index, i) in &layers {
                    let prior = current[i];
                    if let Some((_, delta)) =
                        crate::overlay_delta_pair_from_states(prior, prior, replay_states[i])
                    {
                        let message = layer_message
                            .get_or_insert_with(|| format!("__songlua_layer_frame_{frame}"));
                        tracked.actors[tracked_index].actor.message_commands.push(
                            crate::SongLuaOverlayMessageCommand {
                                frame_advance: 0.,
                                message: message.clone(),
                                blocks: vec![crate::SongLuaOverlayCommandBlock {
                                    progress: None,
                                    queued: false,
                                    start: 0.,
                                    duration: 0.,
                                    easing: None,
                                    opt1: None,
                                    opt2: None,
                                    delta,
                                }],
                            },
                        );
                    }
                }
                if let Some(message) = layer_message {
                    broadcasts.push((next_beat, message, true));
                }
                std::mem::swap(&mut current, &mut replay_states);
                beat = next_beat;
            }
            let fallback_bpms = [(0., song_display_bps(context) * 60.)];
            let bpms = if context.song_timing_bpms.is_empty() {
                fallback_bpms.as_slice()
            } else {
                &context.song_timing_bpms
            };
            let use_mod_clock = context
                .player_timing
                .iter()
                .flatten()
                .all(|t| t.matches_bpm_clock(bpms));
            let (eases, column_transforms) = finish_samples(
                context,
                end,
                origin,
                use_mod_clock,
                &sample_beats,
                &sample_seconds,
                &player_samples,
                &mod_samples,
                &mod_speed_samples,
                &column_samples,
                &baseline_players,
                &baseline_mods,
            );
            let wrappers = self.actors.finish_song_wrapper_capture(screen);
            self.append_update_wrapper_rows(overlays, &values, &wrappers)?;
            let mut tracks = capture.finish();
            tracks.retain_mut(|t| {
                if t.overlay_index < count {
                    true
                } else if t.overlay_index >= wrapper_start {
                    t.overlay_index = count + t.overlay_index - wrapper_start;
                    true
                } else {
                    false
                }
            });
            for (i, samples) in texts {
                if let Kind::BitmapText {
                    text_changes,
                    text_changes_in_seconds,
                    ..
                } = &mut overlays.actors[i].actor.kind
                {
                    *text_changes = samples.into();
                    *text_changes_in_seconds = context.song_timing.is_some();
                }
            }
            let mut stateful = self
                .song_replay_messages(screen)
                .ok_or("missing song replay messages")?;
            let remap = |i: usize| {
                if i < count {
                    Some(i)
                } else if i >= wrapper_start && i < wrapper_start + wrappers.len() {
                    Some(count + i - wrapper_start)
                } else {
                    None
                }
            };
            for capture in &mut stateful {
                capture.overlay_targets.retain_mut(|(i, _)| {
                    if let Some(n) = remap(*i) {
                        *i = n;
                        true
                    } else {
                        false
                    }
                });
                capture.writes.retain_mut(|w| {
                    if let Some(n) = remap(w.overlay_index) {
                        w.overlay_index = n;
                        true
                    } else {
                        false
                    }
                });
            }
            stateful.retain(|c| !c.overlay_targets.is_empty() || !c.writes.is_empty());
            let mut runtime_broadcasts = self.actors.take_song_broadcasts();
            runtime_broadcasts.extend(broadcasts);
            let replay = self
                .capture_song_tween_replay(screen)
                .ok_or("missing song tween progress")?;
            let advances = self.actors.song_message_advances(screen);
            for (i, row) in overlays.actors.iter_mut().enumerate() {
                for command in &mut row.actor.message_commands {
                    for ((index, name), advance) in &advances {
                        if remap(*index) == Some(i) && *name == command.message {
                            command.frame_advance = *advance;
                        }
                    }
                }
                let value = self
                    .song_overlay_actor_value(row)
                    .map_err(|e| e.to_string())?;
                if let Value::Table(id) = value
                    && let (Some(progress), Some(startup)) =
                        (replay.get(&id), activation.tweens.get(&id))
                    && let Some(command) = row
                        .actor
                        .message_commands
                        .iter_mut()
                        .find(|c| c.message == "__songlua_actor_startup")
                {
                    progress.apply_startup_progress(&mut command.blocks, startup);
                }
            }
            sound_events.extend(self.take_prepared_sounds());
            judgment_textures.extend(self.take_judgment_textures());
            sprite_textures.extend(
                self.take_sprite_textures()
                    .into_iter()
                    .map(|(id, v)| (id as usize, v)),
            );
            self.end_actor_capture();
            #[cfg(feature = "test-support")]
            let overlay_writes = self.actors.take_song_observed_writes(screen);
            self.actors.end_song_update_capture(screen);
            Ok(SongUpdateOutput {
                eases,
                overlay_tracks: tracks,
                #[cfg(feature = "test-support")]
                overlay_writes,
                column_transforms,
                stateful_messages: stateful,
                broadcasts: runtime_broadcasts,
                column_splines: splines.finish()?,
                ..Default::default()
            })
        })();
        options.release(&mut self.vm).map_err(|e| e.to_string())?;
        result
    }
    fn update_pose(&mut self, value: Value, mut state: State) -> Result<State, String> {
        self.actors.apply_song_replay_pose(value, &mut state);
        Ok(state)
    }
    fn update_motion(
        &mut self,
        value: Value,
        clock: [f32; 2],
        include_spin: bool,
    ) -> Result<MotionSample, String> {
        let Answer::Motion(time, spin) = self
            .actors
            .chronological_read(&mut self.vm, value, Query::Motion(clock, include_spin))
            .map_err(|e| e.to_string())?
        else {
            unreachable!()
        };
        Ok((time, spin))
    }
    fn update_masks(
        &mut self,
        players: &[Option<Value>; LUA_PLAYERS],
    ) -> Result<[u16; LUA_PLAYERS], String> {
        let mut out = [0; LUA_PLAYERS];
        for (i, v) in players.iter().enumerate() {
            if let Some(v) = v {
                let Answer::Mask(mask) = self
                    .actors
                    .chronological_read(&mut self.vm, *v, Query::Mask)
                    .map_err(|e| e.to_string())?
                else {
                    unreachable!()
                };
                out[i] = mask;
            }
        }
        Ok(out)
    }
    fn update_players(
        &mut self,
        access: &Access,
        players: &[Option<Value>; LUA_PLAYERS],
    ) -> Result<[Player; LUA_PLAYERS], String> {
        let mut out = [Player::default(); LUA_PLAYERS];
        for (i, v) in players.iter().enumerate() {
            if let Some(v) = v {
                out[i] = access.player(&mut self.vm, *v).map_err(|e| e.to_string())?;
                let mut state = State::default();
                if self.actors.apply_song_replay_pose(*v, &mut state) {
                    crate::perframe::sample_player_pose(&mut out[i], &state, None);
                }
            }
        }
        Ok(out)
    }
    fn restore_update_messages(
        &mut self,
        access: &Access,
        values: &[Value],
        states: &[State],
        started: &[usize],
    ) -> Result<(), String> {
        for &i in started {
            if let (Some(&v), Some(&s)) = (values.get(i), states.get(i)) {
                access
                    .write_frame(&mut self.vm, v, s)
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
    fn append_update_wrappers(
        &mut self,
        screen: ActorScene,
        roots: &mut Roots,
        values: &mut Vec<Value>,
        states: &mut [&mut Vec<State>],
    ) -> Result<(), String> {
        for (_, id, state) in self.actors.new_song_wrappers(screen) {
            roots.keep(&mut self.vm, Value::Table(id))?;
            values.push(Value::Table(id));
            for rows in states.iter_mut() {
                rows.push(state);
            }
        }
        Ok(())
    }
    fn capture_update_fields(
        &mut self,
        tracked: &mut SongTrackedActors,
        values: &[Value],
        prior: f32,
        clock: [f32; 2],
        bytes: &mut usize,
    ) -> Result<(), String> {
        for (row, &value) in tracked.actors.iter_mut().zip(values) {
            if !matches!(row.target, Tracked::Player(_)) {
                continue;
            }
            let Answer::Field(states) = self
                .actors
                .chronological_read(&mut self.vm, value, Query::Field(clock))
                .map_err(|e| e.to_string())?
            else {
                unreachable!()
            };
            let Some(states) = states else { continue };
            let frames = &mut row.actor.note_field_frames;
            if (states.is_empty() && frames.is_empty())
                || frames.last().is_some_and(|f| f.wrappers.as_ref() == states)
            {
                continue;
            }
            let additional = states.len() * std::mem::size_of::<State>()
                + 2 * std::mem::size_of::<crate::SongLuaNoteFieldFrame>();
            *bytes = bytes.saturating_add(additional);
            if *bytes > 256 * 1024 * 1024 {
                return Err("NoteField wrapper capture exceeds 256 MiB".into());
            }
            if let Some(previous) = frames.last().filter(|f| f.second < prior) {
                frames.push(crate::SongLuaNoteFieldFrame {
                    second: prior,
                    wrappers: previous.wrappers.clone(),
                });
            }
            frames.push(crate::SongLuaNoteFieldFrame {
                second: clock[0],
                wrappers: states.into(),
            });
        }
        Ok(())
    }
    fn append_update_wrapper_rows<S, V, A>(
        &mut self,
        overlays: &mut SongOverlayActors<Kind<S, V, A>>,
        values: &[Value],
        wrappers: &[(TableId, TableId, State)],
    ) -> Result<(), String> {
        let mut indices = HashMap::new();
        for (i, row) in overlays.actors.iter().enumerate() {
            let value = self
                .song_overlay_actor_value(row)
                .map_err(|e| e.to_string())?;
            let Value::Table(id) = value else {
                unreachable!()
            };
            indices.insert(id, i);
        }
        let owner = overlays.owner.ok_or("released song overlay batch")?;
        let Value::Table(array) = self.vm.rooted(owner).map_err(|e| e.to_string())? else {
            unreachable!()
        };
        for &(owner_id, id, initial_state) in wrappers {
            let Some(&mut_index) = indices.get(&owner_id) else {
                return Err("Runtime wrapper owner has no compiled overlay".into());
            };
            let mut inner = mut_index;
            while let Some(parent) = overlays.actors[inner].actor.parent_index {
                if !matches!(overlays.actors[parent].actor.kind, Kind::WrapperState) {
                    break;
                }
                let value = self
                    .song_overlay_actor_value(&overlays.actors[parent])
                    .map_err(|e| e.to_string())?;
                let Value::Table(value) = value else {
                    unreachable!()
                };
                let key = self
                    .vm
                    .try_intern(b"__songlua_parent")
                    .map_err(|e| e.to_string())?;
                let mut raw = self
                    .vm
                    .raw_table_get(value, key)
                    .map_err(|e| e.to_string())?;
                if raw == Value::Nil {
                    raw = self
                        .actors
                        .wrapper_parent(value)
                        .map_or(Value::Nil, Value::Table);
                }
                if raw != Value::Nil && !matches!(raw, Value::Table(_)) {
                    return Err("wrapper parent: table expected".into());
                }
                if raw != Value::Table(owner_id) {
                    break;
                }
                inner = parent;
            }
            let parent_index = overlays.actors[inner].actor.parent_index;
            let i = overlays.actors.len();
            overlays.actors[inner].actor.parent_index = Some(i);
            self.vm
                .raw_table_set(array, Value::Number((i + 1) as f64), Value::Table(id))
                .map_err(|e| e.to_string())?;
            indices.insert(id, i);
            overlays.actors.push(SongOverlayActor {
                actor: crate::SongLuaOverlayActor {
                    kind: Kind::WrapperState,
                    name: None,
                    parent_index,
                    initial_state,
                    message_commands: Vec::new(),
                },
                message_sounds: Vec::new(),
                owner,
                slot: i + 1,
            });
        }
        let _ = values;
        Ok(())
    }
    fn update_text(&mut self, actor: Value) -> Result<String, String> {
        let globals = self.compiler_globals()?;
        let read = self
            .vm
            .new_host_function(
                |call| {
                    let value = super::field(call, call.captures()[0], "Text")?;
                    let id = call
                        .coerce_string(value)?
                        .ok_or_else(|| call.error(b"text: string expected"))?;
                    call.push(Value::String(id))
                },
                &[actor, globals],
            )
            .map_err(|e| e.to_string())?;
        let Value::String(id) = self.vm.call(read, &[]).map_err(|e| e.to_string())?[0] else {
            unreachable!()
        };
        String::from_utf8(self.vm.string_bytes(id).unwrap().to_vec()).map_err(|e| e.to_string())
    }
    fn update_music_seconds(
        &mut self,
        seconds: f64,
        context: &SongLuaCompileContext,
    ) -> Result<f32, String> {
        Ok((seconds * f64::from(song_music_rate(context))) as f32
            + self
                .clock
                .timing
                .as_ref()
                .map_or(0., |t| t.get_time_for_beat_exact(0.)))
    }
    fn call_song_update_frame(
        &mut self,
        scene: (ActorScene, Value),
        frame: usize,
        beat: f64,
        seconds: f64,
        delta_beats: f32,
        delta: f64,
    ) -> Result<(), String> {
        let (screen, root) = scene;
        let prior = self.values().map_err(|e| e.to_string())?;
        let prior_delta = self.delta_values().map_err(|e| e.to_string())?;
        self.set_beat(beat as f32).map_err(|e| e.to_string())?;
        self.set_delta_values(delta_beats, delta as f32)
            .map_err(|e| e.to_string())?;
        let globals = self.compiler_globals()?;
        let runtime = self.compiler_table_field(globals, SONG_LUA_RUNTIME_KEY)?;
        if !matches!(runtime, Value::Table(_)) {
            return Err("song runtime: table expected".into());
        }
        let pin = self.vm.pin(runtime).map_err(|e| e.to_string())?;
        let result = (|| {
            let function =
                self.vm
                    .new_host_function_with_data(
                        |call| {
                            let (beat, seconds, clock) = call
                                .function_data::<(f64, f64, Arc<Clock>)>()
                                .unwrap()
                                .clone();
                            let runtime = call.captures()[0];
                            for (key, value) in [
                                (BEAT, Value::Number(beat)),
                                (SECONDS, Value::Number(seconds)),
                            ] {
                                let key = call.try_intern(key.as_bytes())?;
                                call.table_set(runtime, key, value)?;
                            }
                            if let Some(clock) = &clock.timing {
                                let value = field(call, runtime, RATE)?;
                                let rate = {
                                    let mut stack = call.native_stack()?;
                                    stack.push(value)?;
                                    stack.to_number(-1).ok_or_else(|| {
                                        stack.error(b"music rate: number expected")
                                    })? as f32
                                };
                                let position = clock.get_song_position(
                                    (seconds * f64::from(rate)) as f32
                                        + clock.get_time_for_beat_exact(0.),
                                );
                                for (key, value) in [
                                    (BPS, Value::Number(f64::from(position.bpm / 60.))),
                                    (FREEZE, Value::Boolean(position.is_in_freeze)),
                                    (DELAY, Value::Boolean(position.is_in_delay)),
                                ] {
                                    let key = call.try_intern(key.as_bytes())?;
                                    call.table_set(runtime, key, value)?;
                                }
                            }
                            Ok(())
                        },
                        &[runtime],
                        (beat, seconds, self.clock.clone()),
                    )
                    .map_err(|e| e.to_string())?;
            self.vm.call(function, &[]).map_err(|e| e.to_string())?;
            self.advance_options(seconds, None)
                .map_err(|e| e.to_string())?;
            let result = self
                .actors
                .advance_compiler_replay(&mut self.vm, screen, root, frame, delta)
                .and_then(|_| {
                    self.actors
                        .compiler_read(&mut self.vm, root, actors::compiler::Query::Draw)
                })
                .map(|_| ())
                .map_err(|e| e.to_string());
            self.set_values(prior.0, prior.1)
                .map_err(|e| e.to_string())?;
            self.set_delta_values(prior_delta.0, prior_delta.1)
                .map_err(|e| e.to_string())?;
            result
        })();
        self.vm.unpin(pin).map_err(|e| e.to_string())?;
        result
    }
}

#[allow(clippy::too_many_arguments)]
fn finish_samples<M: crate::perframe::ModState>(
    context: &SongLuaCompileContext,
    end: f32,
    origin: Option<f32>,
    use_mod_clock: bool,
    sample_beats: &[f32],
    sample_seconds: &[f32],
    player_samples: &[[Player; LUA_PLAYERS]],
    mod_samples: &[[M; LUA_PLAYERS]],
    mod_speed_samples: &[[M; LUA_PLAYERS]],
    column_samples: &[Vec<crate::SongLuaColumnTransformSample>],
    baseline_players: &[Player; LUA_PLAYERS],
    baseline_mods: &[M; LUA_PLAYERS],
) -> (Vec<SongLuaEaseWindow>, Vec<SongLuaColumnOffsetWindow>) {
    use crate::perframe::push_update_mod_targets_with_key;
    use crate::{append_column_transform_windows_from_samples, push_perframe_player_targets};
    let mut eases = Vec::new();
    let mut column_transforms = Vec::new();
    let mut last_mod_windows = BTreeMap::new();
    let mut last_mod_lookup_key = (0, String::new());
    for index in 0..sample_beats.len() {
        let seg_start = sample_beats[index];
        let seg_end = sample_beats.get(index + 1).copied().unwrap_or(end);
        // Preserve the last sampled target even when the song ends between
        // reference frames; the gameplay window builder retains its tail.
        let from_mods = &mod_samples[index];
        let to_mods = mod_samples.get(index + 1).unwrap_or(from_mods);
        let (mod_start, mod_end, mod_unit) = if let Some(timing) = &context.song_timing {
            let origin = timing.get_time_for_beat_exact(0.0);
            let start = sample_seconds[index] + origin;
            (
                start,
                sample_seconds
                    .get(index + 1)
                    .copied()
                    .map_or_else(|| start.next_up(), |second| second + origin),
                SongLuaTimeUnit::Second,
            )
        } else if use_mod_clock {
            (
                sample_seconds[index],
                sample_seconds
                    .get(index + 1)
                    .copied()
                    .unwrap_or_else(|| sample_seconds[index].next_up()),
                SongLuaTimeUnit::BeatClock,
            )
        } else {
            (
                seg_start,
                seg_end.max(seg_start.next_up()),
                SongLuaTimeUnit::Beat,
            )
        };
        // Dropping a speed mode ends its coalescing run. Selecting the same
        // value again later must start a new window after the intervening mode.
        for (player, mods) in from_mods.iter().enumerate() {
            for key in ["xmod", "cmod", "mmod"] {
                if mods.get(key).is_none() {
                    last_mod_lookup_key.0 = player;
                    last_mod_lookup_key.1.clear();
                    last_mod_lookup_key.1.push_str(key);
                    last_mod_windows.remove(&last_mod_lookup_key);
                }
            }
        }
        push_update_mod_targets_with_key(
            &mut eases,
            mod_start,
            mod_end,
            from_mods,
            to_mods,
            baseline_mods,
            &mod_speed_samples[index],
            mod_speed_samples.get(index + 1),
            &mut last_mod_windows,
            &mut last_mod_lookup_key,
            // Keep the update clock: narrowing to a beat and converting back
            // can move a step target past its own frame's timestamp.
            mod_unit,
        );
        let (frame_start, frame_end, frame_unit) = if origin.is_some() {
            (mod_start, mod_end, SongLuaTimeUnit::Second)
        } else {
            // Actor::UpdateTweening uses seconds regardless of split chart
            // timing. A rounded beat/time round trip can move a captured pose
            // past its own frame and interpolate towards the following pose.
            (
                sample_seconds[index],
                sample_seconds
                    .get(index + 1)
                    .copied()
                    .unwrap_or_else(|| sample_seconds[index].next_up()),
                SongLuaTimeUnit::BeatClock,
            )
        };
        if frame_end <= frame_start {
            continue;
        }
        let from_players = player_samples[index];
        let to_players = player_samples
            .get(index + 1)
            .copied()
            .unwrap_or(from_players);
        let first_player_window = eases.len();
        push_perframe_player_targets(
            &mut eases,
            frame_start,
            frame_end,
            &from_players,
            &to_players,
            baseline_players,
        );
        for window in &mut eases[first_player_window..] {
            window.unit = frame_unit;
        }
        let from_columns = &column_samples[index];
        let to_columns = column_samples.get(index + 1).unwrap_or(from_columns);
        append_column_transform_windows_from_samples(
            &mut column_transforms,
            from_columns,
            to_columns,
            SongLuaColumnOffsetBuildParams {
                unit: frame_unit,
                start: frame_start,
                limit: frame_end - frame_start,
                span_mode: SongLuaSpanMode::Len,
                easing: None,
                sustain: None,
                opt1: None,
                opt2: None,
            },
        );
    }
    (eases, column_transforms)
}
