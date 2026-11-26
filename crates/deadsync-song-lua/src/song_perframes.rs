//! Sample the existing standalone perframe schedules before chronological replay.
use super::song_compiler_roots::Roots;
use super::*;
use crate::{
    SongLuaCompileInfo, SongLuaEaseWindow, SongLuaMessageEvent, SongLuaOverlayEase,
    SongLuaOverlayState as State, SongLuaOverlayUpdateTarget as Target,
    SongLuaPerframePlayerState as Player, SongLuaTrackedActorTarget,
};

struct Entry {
    start: f32,
    end: f32,
    function: usize,
}
#[derive(Clone)]
pub(super) struct Active {
    command: usize,
    start_seconds: f32,
    elapsed: Option<f32>,
    duration: f32,
    base: State,
    includes_queued: bool,
    can_restart_motion: bool,
    preserved_writes: smallvec::SmallVec<[(usize, u128); 4]>,
}
impl Active {
    pub(super) fn new(
        index: usize,
        command: &crate::SongLuaOverlayMessageCommand,
        start_seconds: f32,
        base: State,
    ) -> Self {
        let includes_queued = command.message.starts_with("__songlua_");
        let mut duration = 0.0_f32;
        let mut can_restart_motion = false;
        let mut preserved_writes = smallvec::SmallVec::new();
        for (index, block) in command.blocks.iter().enumerate() {
            // Motion replay has always examined queued blocks too.
            can_restart_motion |= matches!(
                block.delta.effect_mode,
                Some(crate::EffectMode::Bob | crate::EffectMode::Wag | crate::EffectMode::Bounce)
            );
            if block.queued && !includes_queued {
                continue;
            }
            duration = duration.max(crate::overlay_block_end(block));
            let writes = preserved_writes_mask(&block.delta);
            if writes != 0 {
                preserved_writes.push((index, writes));
            }
        }
        Self {
            command: index,
            start_seconds,
            elapsed: None,
            duration,
            base,
            includes_queued,
            can_restart_motion,
            preserved_writes,
        }
    }
}

// Use the same field list to detect newly started writes and to copy fields
// directly. Effect time has no delta field and is always kept.
macro_rules! preserved_fields {
    ($visit:ident) => {
        $visit! {
            Hibernating => hibernating,
            HAlign => halign,
            VAlign => valign,
            TextAlign => text_align,
            Size => size,
            BaseZoom => basezoom,
            BaseZoomX => basezoom_x,
            BaseZoomY => basezoom_y,
            BaseZoomZ => basezoom_z,
            BaseAlpha => base_alpha,
            Vibrate => vibrate,
            EffectMagnitude => effect_magnitude,
            EffectClock => effect_clock,
            EffectTimer => effect_timer,
            EffectNoOffset => effect_no_offset,
            EffectMode => effect_mode,
            EffectColor1 => effect_color1,
            EffectColor2 => effect_color2,
            EffectPeriod => effect_period,
            EffectOffset => effect_offset,
            EffectTiming => effect_timing,
            Rainbow => rainbow,
        }
    };
}

fn preserved_writes_mask(delta: &crate::SongLuaOverlayStateDelta) -> u128 {
    macro_rules! mask {
        ($($target:ident => $field:ident,)*) => {
            0 $(| if delta.$field.is_some() { 1 << Target::$target as u8 } else { 0 })*
        };
    }
    preserved_fields!(mask)
}

fn preserve_unwritten(state: &mut State, current: &State, writes: u128) {
    macro_rules! copy {
        ($($target:ident => $field:ident,)*) => {
            $(if writes & (1 << Target::$target as u8) == 0 {
                state.$field = current.$field;
            })*
        };
    }
    preserved_fields!(copy);
    state.effect_time = current.effect_time;
}
#[derive(Default)]
struct Snapshot {
    beat: f32,
    players: [Player; LUA_PLAYERS],
    overlays: Vec<State>,
}
type Output = (
    Vec<SongLuaEaseWindow>,
    Vec<SongLuaOverlayEase>,
    SongLuaCompileInfo,
);

impl SongRuntime {
    /// Consume the two selected static tables. Other lexical inputs stay rooted
    /// in the keeper until the enclosing compiler releases it. Callback writes
    /// and shared script state remain available to subsequent compiler stages.
    pub fn compile_song_static_perframes(
        &mut self,
        tables: &mut SongStaticTables,
        context: &SongLuaCompileContext,
        overlays: &[SongMessageTarget<'_>],
        tracked: &SongTrackedActors,
        messages: &[SongLuaMessageEvent],
    ) -> Result<Output, String> {
        let values = self.song_static_perframe_tables(tables)?;
        let mut roots = Roots::default();
        let result = (|| {
            let mut inputs = Vec::new();
            for value in values {
                inputs.push(value.map(|v| roots.keep(&mut self.vm, v)).transpose()?);
            }
            for index in std::mem::take(&mut tables.perframes).into_iter().flatten() {
                tables
                    .roots
                    .as_mut()
                    .unwrap()
                    .release(&mut self.vm, index)?;
            }
            for row in overlays {
                roots.keep(&mut self.vm, row.actor)?;
            }
            let mut players = [None; LUA_PLAYERS];
            let mut tracked_values = Vec::new();
            for row in &tracked.actors {
                let value = self
                    .song_tracked_actor_value(row)
                    .map_err(|e| e.to_string())?;
                roots.keep(&mut self.vm, value)?;
                if let SongLuaTrackedActorTarget::Player(player) = row.target {
                    players[player] = Some(value);
                }
                tracked_values.push(value);
            }
            let mut entries = Vec::new();
            for index in inputs.into_iter().flatten() {
                let source = roots.value(&self.vm, index)?;
                entries.extend(self.schedule_rows(Some(source), |runtime, row| {
                    let Some(start) = runtime.schedule_float(runtime.schedule_slot(row, 1)?) else {
                        return Ok(None);
                    };
                    let Some(end) = runtime.schedule_float(runtime.schedule_slot(row, 2)?) else {
                        return Ok(None);
                    };
                    let function = runtime.schedule_slot(row, 3)?;
                    if !matches!(function, Value::Function(_) | Value::Native(_))
                        || !start.is_finite()
                        || !end.is_finite()
                        || end <= start
                    {
                        return Ok(None);
                    }
                    Ok(Some(Entry {
                        start,
                        end,
                        function: roots.keep(&mut runtime.vm, function)?,
                    }))
                })?);
                roots.release(&mut self.vm, index)?;
            }
            self.sample_song_perframes(
                &roots,
                &entries,
                context,
                overlays,
                &tracked_values,
                &players,
                messages,
            )
        })();
        roots.clear(&mut self.vm)?;
        result
    }
    #[allow(clippy::too_many_arguments)]
    fn sample_song_perframes(
        &mut self,
        roots: &Roots,
        entries: &[Entry],
        context: &SongLuaCompileContext,
        overlays: &[SongMessageTarget<'_>],
        tracked: &[Value],
        players: &[Option<Value>; LUA_PLAYERS],
        messages: &[SongLuaMessageEvent],
    ) -> Result<Output, String> {
        let mut boundaries = entries
            .iter()
            .flat_map(|e| [e.start, e.end])
            .filter(|v| v.is_finite())
            .collect::<Vec<_>>();
        boundaries.sort_by(f32::total_cmp);
        boundaries.dedup_by(|a, b| (*a - *b).abs() <= f32::EPSILON);
        if boundaries.len() < 2 {
            return Ok((Vec::new(), Vec::new(), SongLuaCompileInfo::default()));
        }
        let access = self.actors.perframe_access();
        let mut previous = Snapshot::default();
        let mut current = Snapshot::default();
        current.capture(self, &access, 0., players, overlays)?;
        let baseline_players = current.players;
        let baseline_overlays = current.overlays.clone();
        let mut message_states = baseline_overlays.clone();
        let mut order = (0..messages.len()).collect::<Vec<_>>();
        order.sort_by(|&a, &b| messages[a].beat.total_cmp(&messages[b].beat));
        let mut next_message = 0;
        let mut active_messages = vec![None; overlays.len()];
        let mut output = (Vec::new(), Vec::new(), SongLuaCompileInfo::default());
        let mut side_effect = false;
        let first = boundaries[0];
        let last = boundaries[boundaries.len() - 1];
        for window in boundaries.windows(2) {
            let [start, end] = [window[0], window[1]];
            if end <= start {
                continue;
            }
            let mid = 0.5_f32.mul_add(end - start, start);
            let active = entries
                .iter()
                .filter(|e| mid > e.start && mid < e.end)
                .collect::<Vec<_>>();
            if active.is_empty() {
                current.capture(self, &access, start, players, overlays)?;
                message_states.clone_from(&current.overlays);
                crate::push_perframe_static_targets(
                    &mut output.0,
                    &mut output.1,
                    start,
                    end,
                    &current.players,
                    &current.overlays,
                    &baseline_players,
                    &baseline_overlays,
                );
                continue;
            }
            let mut has_previous = false;
            for (index, sample) in crate::perframe::perframe_sample_iter(start, end).enumerate() {
                if index & 63 == 0 {
                    context.report_bake_progress(
                        0.25 + 0.20 * (sample.eval_beat - first) / (last - first),
                    );
                }
                let sample_seconds = crate::song_elapsed_seconds_at(sample.eval_beat, context);
                let prior_seconds =
                    crate::song_elapsed_seconds_at(sample.eval_beat - sample.delta_beats, context);
                let delta = (sample_seconds - prior_seconds).max(0.);
                while let Some(&index) = order.get(next_message) {
                    let event = &messages[index];
                    if event.beat > sample.eval_beat + f32::EPSILON {
                        break;
                    }
                    let event_seconds = crate::song_elapsed_seconds_at(event.beat, context);
                    for (i, row) in overlays.iter().enumerate() {
                        let was_active = active_messages[i].is_some();
                        apply_message(self, &access, row, &mut active_messages[i], event_seconds)?;
                        if was_active {
                            message_states[i] = access
                                .state(&mut self.vm, row.actor)
                                .map_err(|e| e.to_string())?;
                        }
                        if let Some(command) =
                            row.commands.iter().position(|c| c.message == event.message)
                        {
                            active_messages[i] = Some(Active::new(
                                command,
                                &row.commands[command],
                                event_seconds,
                                message_states[i],
                            ));
                            apply_message(
                                self,
                                &access,
                                row,
                                &mut active_messages[i],
                                event_seconds,
                            )?;
                            message_states[i] = access
                                .state(&mut self.vm, row.actor)
                                .map_err(|e| e.to_string())?;
                        }
                    }
                    next_message += 1;
                }
                for (i, row) in overlays.iter().enumerate() {
                    let was_active = active_messages[i].is_some();
                    apply_message(self, &access, row, &mut active_messages[i], sample_seconds)?;
                    if was_active {
                        message_states[i] = access
                            .state(&mut self.vm, row.actor)
                            .map_err(|e| e.to_string())?;
                    }
                }
                for actor in overlays
                    .iter()
                    .map(|r| r.actor)
                    .chain(tracked.iter().copied())
                {
                    access
                        .reset(&mut self.vm, actor)
                        .map_err(|e| e.to_string())?;
                }
                for entry in &active {
                    side_effect |= self.call_song_perframe(
                        roots.value(&self.vm, entry.function)?,
                        sample.eval_beat,
                        sample.delta_beats,
                        delta,
                    )?;
                }
                current.capture(self, &access, sample.beat, players, overlays)?;
                message_states.clone_from(&current.overlays);
                if has_previous {
                    previous.segment(
                        &current,
                        current.beat,
                        &mut output,
                        &baseline_players,
                        &baseline_overlays,
                    );
                }
                std::mem::swap(&mut previous, &mut current);
                has_previous = true;
            }
            if has_previous {
                previous.segment(
                    &previous,
                    end,
                    &mut output,
                    &baseline_players,
                    &baseline_overlays,
                );
            }
        }
        if output.0.is_empty() && output.1.is_empty() && !side_effect {
            output.2.unsupported_perframes = entries.len();
            for entry in entries {
                crate::push_unique_compile_detail(
                    &mut output.2.unsupported_perframe_captures,
                    format!("perframe start={:.3} end={:.3}", entry.start, entry.end),
                );
            }
        }
        Ok(output)
    }
    fn call_song_perframe(
        &mut self,
        function: Value,
        beat: f32,
        delta_beats: f32,
        delta_seconds: f32,
    ) -> Result<bool, String> {
        let previous = self.values().map_err(|e| e.to_string())?;
        let previous_delta = self.delta_values().map_err(|e| e.to_string())?;
        let before = self.perframe_side_effects()?;
        self.set_beat(beat).map_err(|e| e.to_string())?;
        self.set_delta_values(delta_beats, delta_seconds)
            .map_err(|e| e.to_string())?;
        let result = self
            .vm
            .call(function, &[number(beat), number(delta_seconds)])
            .map(|_| ())
            .map_err(|e| e.to_string());
        self.set_values(previous.0, previous.1)
            .map_err(|e| e.to_string())?;
        self.set_delta_values(previous_delta.0, previous_delta.1)
            .map_err(|e| e.to_string())?;
        let side_effect = self.perframe_side_effects()? > before;
        result?;
        Ok(side_effect)
    }
    fn perframe_side_effects(&mut self) -> Result<i64, String> {
        let globals = self.compiler_globals()?;
        let read = self
            .vm
            .new_host_function(
                |call| {
                    let globals = call.captures()[0];
                    let value = super::field(call, globals, crate::SONG_LUA_SIDE_EFFECT_COUNT_KEY)?;
                    let result = if value == Value::Nil {
                        0
                    } else {
                        let mut stack = call.native_stack()?;
                        stack.push(value)?;
                        stack
                            .to_integer(-1)
                            .ok_or_else(|| stack.error(b"side-effect integer expected"))?
                            as i64
                    };
                    call.push(Value::Number(result as f64))
                },
                &[globals],
            )
            .map_err(|e| e.to_string())?;
        let Value::Number(value) = self.vm.call(read, &[]).map_err(|e| e.to_string())?[0] else {
            unreachable!()
        };
        Ok(value as i64)
    }
}
fn apply_message(
    runtime: &mut SongRuntime,
    access: &actors::perframes::Access,
    row: &SongMessageTarget<'_>,
    active: &mut Option<Active>,
    seconds: f32,
) -> Result<(), String> {
    apply_message_with_pose(runtime, access, row, active, seconds, false)
}
pub(super) fn apply_message_with_pose(
    runtime: &mut SongRuntime,
    access: &actors::perframes::Access,
    row: &SongMessageTarget<'_>,
    active: &mut Option<Active>,
    seconds: f32,
    chronological: bool,
) -> Result<(), String> {
    let Some(message) = active.as_mut() else {
        return Ok(());
    };
    let Some(command) = row.commands.get(message.command) else {
        *active = None;
        return Ok(());
    };
    let elapsed = (seconds - message.start_seconds).max(0.);
    let mut state = crate::overlay_state_after_blocks(
        message.base,
        command
            .blocks
            .iter()
            .filter(|block| !block.queued || message.includes_queued),
        elapsed,
    );
    let current = access
        .state(&mut runtime.vm, row.actor)
        .map_err(|e| e.to_string())?;
    let mut written = 0;
    for &(index, fields) in &message.preserved_writes {
        let block = &command.blocks[index];
        if message
            .elapsed
            .is_none_or(|prior| crate::overlay_block_factor(block, prior).is_none())
            && crate::overlay_block_factor(block, elapsed).is_some()
        {
            written |= fields;
        }
    }
    preserve_unwritten(&mut state, &current, written);
    if chronological && command.message == "__songlua_actor_startup" {
        runtime
            .actors
            .apply_song_replay_pose(&runtime.vm, row.actor, &mut state);
    }
    message.elapsed = Some(elapsed);
    if message.can_restart_motion
        && let Some(epoch) = crate::motion_restart_epoch(
            message.base,
            &command.blocks,
            elapsed,
            message.start_seconds,
        )
    {
        access.motion(&mut runtime.vm, row.actor, epoch, seconds);
    }
    if chronological {
        access.write_frame(&mut runtime.vm, row.actor, state)
    } else {
        access.write(&mut runtime.vm, row.actor, state)
    }
    .map_err(|e| e.to_string())?;
    if elapsed >= message.duration {
        *active = None;
    }
    Ok(())
}
impl Snapshot {
    fn capture(
        &mut self,
        runtime: &mut SongRuntime,
        access: &actors::perframes::Access,
        beat: f32,
        players: &[Option<Value>; LUA_PLAYERS],
        overlays: &[SongMessageTarget<'_>],
    ) -> Result<(), String> {
        self.beat = beat;
        self.players = [Player::default(); LUA_PLAYERS];
        for (i, actor) in players.iter().enumerate() {
            if let Some(actor) = actor {
                self.players[i] = access
                    .player(&mut runtime.vm, *actor)
                    .map_err(|e| e.to_string())?;
            }
        }
        self.overlays.clear();
        self.overlays.reserve_exact(overlays.len());
        for row in overlays {
            self.overlays.push(
                access
                    .state(&mut runtime.vm, row.actor)
                    .map_err(|e| e.to_string())?,
            );
        }
        Ok(())
    }
    fn segment(
        &self,
        next: &Self,
        end: f32,
        output: &mut Output,
        baseline_players: &[Player; LUA_PLAYERS],
        baseline_overlays: &[State],
    ) {
        if end <= self.beat {
            return;
        }
        crate::push_perframe_player_targets(
            &mut output.0,
            self.beat,
            end,
            &self.players,
            &next.players,
            baseline_players,
        );
        crate::push_perframe_overlay_targets(
            &mut output.1,
            self.beat,
            end,
            &self.overlays,
            &next.overlays,
            baseline_overlays,
            false,
        );
    }
}
