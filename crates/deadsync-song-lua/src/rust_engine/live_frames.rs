//! GameState/player host phases shared by retained song playback.
use super::*;
use crate::{SongLuaLiveFrame, SongLuaLivePlayer};
use deadlib_lua::NativeStack;

#[derive(Default)]
struct Attacks {
    windows: Arc<[deadsync_gameplay::ChartAttackWindow]>,
    active: Vec<usize>,
    pending: Vec<usize>,
}

/// Retained live frame owners and per-screen host state. Actor updates, layer
/// activation, drawing and output collection remain separate ordered phases.
/// Release this owner through its runtime when the enclosing session ends.
pub struct SongLiveFrames {
    owners: Option<Root>,
    context: SongLuaCompileContext,
    last_beat: f64,
    prepared: Option<f64>,
    attacks: [Attacks; LUA_PLAYERS],
    health: [Option<&'static str>; LUA_PLAYERS],
}

impl RustSongRuntime {
    /// Player event callbacks refresh statistics and clocks only. They neither
    /// advance options/attacks nor consume the outer prepared frame.
    pub(super) fn feed_gameplay_callback(
        &mut self,
        owners: Value,
        frame: &SongLuaLiveFrame,
        timing: Option<&deadsync_rules::timing::TimingData>,
    ) -> Result<(), Error> {
        feed(self, owners, frame)?;
        let catalog = self.vm.rooted(self.catalog)?;
        let rate = self.vm.with_native_stack(|s| {
            let base = s.top();
            s.push(catalog)?;
            read(s, s.top(), "song_options", true)?;
            index(s, s.top(), 4)?;
            read(s, s.top(), "__songlua_music_rate", true)?;
            let rate = s.number(-1)? as f32;
            s.set_top(base)?;
            Ok(rate)
        })?;
        let beat = timing.map_or(frame.beat as f32, |t| {
            t.get_song_position_with_rate(frame.music_seconds as f32, rate)
                .beat
        });
        self.set_values(beat, frame.music_seconds as f32)?;
        publish(self, frame.players);
        Ok(())
    }

    pub(super) fn gameplay_fail_type(
        &mut self,
        player: usize,
    ) -> Result<Option<deadsync_gameplay::GameplayFailType>, Error> {
        let catalog = self.vm.rooted(self.catalog)?;
        self.vm.with_native_stack(|s| {
            let base = s.top();
            s.push(catalog)?;
            read(s, s.top(), "player_options", true)?;
            index(s, s.top(), player + 1)?;
            let levels = s.top();
            let read_mode = |s: &mut NativeStack<'_, '_>, level| -> Result<Option<f32>, Error> {
                index(s, levels, level)?;
                let options = s.top();
                read(s, options, "__songlua_player_option_state", true)?;
                if s.value(-1) == Some(Value::Nil) {
                    s.set_top(s.top() - 1)?;
                    s.create_table(0, 0)?;
                    let t = s.value(-1).unwrap();
                    write(s, options, "__songlua_player_option_state", t, true)?;
                    s.push(t)?;
                }
                require_table(s, s.top())?;
                read(s, s.top(), "failsetting", true)?;
                let v = if s.value(-1) == Some(Value::Nil) {
                    None
                } else {
                    Some(s.number(-1)? as f32)
                };
                s.set_top(levels)?;
                Ok(v)
            };
            let mut value = read_mode(s, 3)?;
            if value.is_none() && read_mode(s, 4)?.is_some() {
                value = Some(0.);
            }
            let value = value
                .map(|v| {
                    deadsync_gameplay::GameplayFailType::from_value(v)
                        .ok_or_else(|| s.error(b"Invalid FailType"))
                })
                .transpose()?;
            s.set_top(base)?;
            Ok(value)
        })
    }

    /// Select statistics through the existing Lua methods once, then retain the
    /// selected tables and native option owners across callbacks and GC.
    pub fn new_song_live_frames(
        &mut self,
        context: &SongLuaCompileContext,
    ) -> Result<SongLiveFrames, Error> {
        if !context.live {
            return Err(frame_error(
                self,
                b"song live frames require a live context",
            ));
        }
        let catalog = self.vm.rooted(self.catalog)?;
        let owners =
            self.actors
                .live_frame_owners(&mut self.vm, catalog, self.player_status.clone())?;
        let result = self.vm.with_native_stack(|stack| {
            let base = stack.top();
            let globals = globals(stack)?;
            runtime_table(stack, globals)?;
            let runtime = stack.top();
            read(stack, runtime, BEAT, false)?;
            let beat = stack.number(-1)? as f32;
            stack.set_top(stack.top() - 1)?;
            read(stack, runtime, SECONDS, false)?;
            stack.number(-1)?;
            stack.set_top(base)?;
            Ok(beat)
        });
        match result {
            Ok(beat) => Ok(SongLiveFrames {
                owners: Some(owners),
                context: context.clone(),
                last_beat: f64::from(beat),
                prepared: None,
                attacks: Default::default(),
                health: [None; LUA_PLAYERS],
            }),
            Err(error) => {
                self.vm.unpin(owners)?;
                Err(error)
            }
        }
    }
}

impl SongLiveFrames {
    pub(super) fn discard_prepared(&mut self) {
        self.prepared = None;
    }

    pub(super) fn failed_players(
        &self,
        runtime: &mut RustSongRuntime,
    ) -> Result<[bool; LUA_PLAYERS], Error> {
        let owners = self.owners(runtime)?;
        runtime.vm.with_native_stack(|s| {
            let base = s.top();
            s.push(owners)?;
            let batch = s.top();
            let mut failed = [false; LUA_PLAYERS];
            for (i, f) in failed.iter_mut().enumerate() {
                s.raw_get_integer(batch as isize, (i + 2) as i32)?;
                let owner = s.top();
                read(s, owner, "__songlua_failed", true)?;
                *f = s.to_boolean(-1);
                s.set_top(owner - 1)?;
            }
            s.set_top(base)?;
            Ok(failed)
        })
    }

    pub(super) fn record_life(
        &self,
        runtime: &mut RustSongRuntime,
        player: usize,
        life: f32,
    ) -> Result<(), Error> {
        let owners = self.owners(runtime)?;
        if let Some(p) = runtime.statistics.lock().unwrap().players.as_mut() {
            p[player].life_record_latest = Some(life);
        }
        runtime.vm.with_native_stack(|s| {
            let base = s.top();
            s.push(owners)?;
            s.raw_get_integer(s.top_offset(), (player + 2) as i32)?;
            let stats = s.top();
            write(s, stats, "__songlua_life", number(life), true)?;
            s.set_top(base)
        })?;
        runtime
            .statistics
            .lock()
            .unwrap()
            .writes
            .push(crate::live::SongLuaLivePlayerWrite::RecordLife { player, life });
        Ok(())
    }
    pub(super) fn owners(&self, runtime: &mut RustSongRuntime) -> Result<Value, Error> {
        let Some(owner) = self.owners else {
            return Err(frame_error(runtime, b"released song live frames"));
        };
        runtime.vm.rooted(owner)
    }

    /// Prepare before synchronous Player judgment callbacks, at most once before
    /// `update` consumes the prepared frame. A second preparation is a no-op.
    pub fn prepare_gameplay_frame(
        &mut self,
        runtime: &mut RustSongRuntime,
        frame: &SongLuaLiveFrame,
        deltas: Option<[f32; LUA_PLAYERS + 1]>,
        outro_started: bool,
    ) -> Result<(), Error> {
        self.owners(runtime)?;
        if self.prepared.is_none() {
            self.prepared = Some(self.prepare(runtime, frame, deltas, outro_started)?);
        }
        Ok(())
    }

    /// Resume a prepared frame after Player callbacks, refreshing statistics and
    /// live feedback without reapplying uptime, attacks or option advancement.
    pub fn update(
        &mut self,
        runtime: &mut RustSongRuntime,
        frame: &SongLuaLiveFrame,
        deltas: Option<[f32; LUA_PLAYERS + 1]>,
        outro_started: bool,
    ) -> Result<f64, Error> {
        let owners = self.owners(runtime)?;
        if let Some(beat) = self.prepared.take() {
            feed(runtime, owners, frame)?;
            runtime.vm.with_native_stack(|stack| {
                let base = stack.top();
                let globals = globals(stack)?;
                runtime_table(stack, globals)?;
                let current = stack.top();
                write(stack, current, BEAT, number(beat as f32), false)?;
                write(
                    stack,
                    current,
                    SECONDS,
                    number(frame.music_seconds as f32),
                    false,
                )?;
                stack.set_top(base)
            })?;
            publish(runtime, frame.players);
            Ok(beat)
        } else {
            self.prepare(runtime, frame, deltas, outro_started)
        }
    }

    fn prepare(
        &mut self,
        runtime: &mut RustSongRuntime,
        frame: &SongLuaLiveFrame,
        deltas: Option<[f32; LUA_PLAYERS + 1]>,
        outro_started: bool,
    ) -> Result<f64, Error> {
        let owners = self.owners(runtime)?;
        let rate = runtime.vm.with_native_stack(|stack| {
            let base = stack.top();
            stack.push(owners)?;
            let batch = stack.top();
            index(stack, batch, 4)?;
            let current = stack.top();
            read(stack, current, "__songlua_music_rate", true)?;
            let rate = stack.number(-1)? as f32;
            stack.set_top(base)?;
            Ok(rate)
        })?;
        let position = self
            .context
            .song_timing
            .as_ref()
            .map(|timing| timing.get_song_position_with_rate(frame.music_seconds as f32, rate));
        let beat = position
            .as_ref()
            .map_or(frame.beat, |position| f64::from(position.beat));
        feed(runtime, owners, frame)?;
        let mut attack_positions = [0_f32; LUA_PLAYERS];
        runtime.vm.with_native_stack(|stack| {
            let base = stack.top();
            let globals = globals(stack)?;
            runtime_table(stack, globals)?;
            let retained = stack.top();
            read(stack, retained, "__songlua_uptime_seconds", false)?;
            let prior = if stack.value(-1) == Some(Value::Nil) {
                0.
            } else {
                stack.number(-1)?
            };
            stack.set_top(stack.top() - 1)?;
            write(
                stack,
                retained,
                "__songlua_uptime_seconds",
                Value::Number(frame.uptime_seconds.unwrap_or(prior + frame.delta_seconds)),
                false,
            )?;
            // Each native helper reacquires the current runtime; retain the
            // first one separately for PlayerState's position table below.
            runtime_table(stack, globals)?;
            let current = stack.top();
            write(stack, current, BEAT, number(beat as f32), false)?;
            write(
                stack,
                current,
                SECONDS,
                number(frame.music_seconds as f32),
                false,
            )?;
            stack.set_top(stack.top() - 1)?;
            if let Some(position) = &position {
                runtime_table(stack, globals)?;
                let current = stack.top();
                write(stack, current, BPS, number(position.bpm / 60.), false)?;
                write(
                    stack,
                    current,
                    FREEZE,
                    Value::Boolean(position.is_in_freeze),
                    false,
                )?;
                write(
                    stack,
                    current,
                    DELAY,
                    Value::Boolean(position.is_in_delay),
                    false,
                )?;
                stack.set_top(stack.top() - 1)?;
            } else {
                write(
                    stack,
                    retained,
                    BPS,
                    number(crate::song_bps_at_beat(beat as f32, &self.context)),
                    false,
                )?;
            }
            read(stack, retained, PLAYERS, true)?;
            let positions = stack.top();
            require_table(stack, positions)?;
            for (i, attack_position) in attack_positions.iter_mut().enumerate() {
                if !self.context.players[i].enabled {
                    continue;
                }
                index(stack, positions, i + 1)?;
                let owned = stack.top();
                require_table(stack, owned)?;
                read(stack, owned, SECONDS, true)?;
                *attack_position = stack.number(-1)? as f32;
                stack.set_top(stack.top() - 1)?;
                let position = self.context.player_timing[i]
                    .as_ref()
                    .or(self.context.song_timing.as_ref())
                    .map(|timing| {
                        timing.get_song_position_with_rate(frame.music_seconds as f32, rate)
                    });
                write(
                    stack,
                    owned,
                    SECONDS,
                    number(frame.music_seconds as f32),
                    true,
                )?;
                write(
                    stack,
                    owned,
                    BEAT,
                    number(position.as_ref().map_or(frame.beat as f32, |p| p.beat)),
                    true,
                )?;
                // Option::map_or evaluates the fallback even when a timing
                // position exists; the ordinary runtime read must still occur.
                read(stack, retained, BPS, true)?;
                let bps = stack.number(-1)? as f32;
                stack.set_top(stack.top() - 1)?;
                write(
                    stack,
                    owned,
                    BPS,
                    number(position.as_ref().map_or(bps, |p| p.bpm / 60.)),
                    true,
                )?;
                write(
                    stack,
                    owned,
                    FREEZE,
                    Value::Boolean(position.as_ref().is_some_and(|p| p.is_in_freeze)),
                    true,
                )?;
                write(
                    stack,
                    owned,
                    DELAY,
                    Value::Boolean(position.as_ref().is_some_and(|p| p.is_in_delay)),
                    true,
                )?;
                stack.set_top(stack.top() - 1)?;
            }
            runtime_table(stack, globals)?;
            let current = stack.top();
            write(
                stack,
                current,
                DELTA_BEAT,
                number((beat - self.last_beat) as f32),
                false,
            )?;
            write(
                stack,
                current,
                DELTA_SECONDS,
                number(frame.delta_seconds as f32),
                false,
            )?;
            self.last_beat = beat;
            stack.set_top(base)
        })?;
        publish(runtime, frame.players);
        if !outro_started && let Some(windows) = &frame.chart_attacks {
            let master = self.context.players.iter().position(|p| p.enabled);
            for i in 0..LUA_PLAYERS {
                let attacks = &mut self.attacks[i];
                let replaced = !Arc::ptr_eq(&attacks.windows, &windows[i]);
                if replaced {
                    attacks.windows = windows[i].clone();
                    attacks.pending.reserve(windows[i].len());
                    attacks.active.reserve(windows[i].len());
                }
                attacks.pending.clear();
                for (j, attack) in windows[i].iter().enumerate() {
                    let seconds = attack_positions[i];
                    if attack.start_second < seconds
                        && seconds < attack.start_second + attack.len_seconds
                    {
                        attacks.pending.push(j);
                    }
                }
                if attacks.active != attacks.pending || (replaced && !attacks.active.is_empty()) {
                    let modifiers = attacks
                        .pending
                        .iter()
                        .map(|&j| windows[i][j].mods.clone())
                        .collect::<Vec<_>>();
                    runtime.rebuild_attack_options(i, master == Some(i), &modifiers)?;
                    std::mem::swap(&mut attacks.active, &mut attacks.pending);
                }
            }
        }
        runtime.advance_options(
            frame.screen_seconds,
            Some(deltas.unwrap_or([frame.delta_seconds as f32; LUA_PLAYERS + 1])),
        )?;
        Ok(beat)
    }

    /// ScreenGameplay's post-actor health phase. Publish each new status before
    /// notifying its listeners, so nested writes affect the following player.
    /// The scene coordinator owns the existing report-once error policy.
    pub fn finish_health_update(&mut self, runtime: &mut RustSongRuntime) -> Result<(), Error> {
        let owners = self.owners(runtime)?;
        let screen = runtime.vm.with_native_stack(|stack| {
            let base = stack.top();
            let globals = globals(stack)?;
            read(stack, globals, "__songlua_top_screen", true)?;
            require_table(stack, stack.top())?;
            let screen = stack.value(-1).unwrap();
            stack.set_top(base)?;
            Ok(screen)
        })?;
        let screen = runtime.vm.pin(screen)?;
        let statistics = runtime.statistics.clone();
        let result = (|| {
            for i in 0..LUA_PLAYERS {
                let screen_value = runtime.vm.rooted(screen)?;
                let state = runtime.vm.with_native_stack(|stack| {
                    let base = stack.top();
                    stack.push(screen_value)?;
                    let top = stack.top();
                    require_table(stack, top)?;
                    read(
                        stack,
                        top,
                        if i == 0 {
                            "__songlua_player1_enabled"
                        } else {
                            "__songlua_player2_enabled"
                        },
                        true,
                    )?;
                    let enabled = stack.value(-1) != Some(Value::Boolean(false));
                    stack.set_top(stack.top() - 1)?;
                    if !enabled {
                        stack.set_top(base)?;
                        return Ok(None);
                    }
                    let Some(player) = statistics
                        .lock()
                        .unwrap()
                        .players
                        .and_then(|p| p.get(i).copied())
                    else {
                        stack.set_top(base)?;
                        return Ok(None);
                    };
                    stack.push(owners)?;
                    let batch = stack.top();
                    index(stack, batch, i + 5)?;
                    let options = stack.top();
                    read(stack, options, "__songlua_player_option_state", true)?;
                    if stack.value(-1) == Some(Value::Nil) {
                        stack.set_top(stack.top() - 1)?;
                        stack.create_table(0, 0)?;
                        let created = stack.value(-1).unwrap();
                        write(
                            stack,
                            options,
                            "__songlua_player_option_state",
                            created,
                            true,
                        )?;
                    }
                    let state = stack.top();
                    require_table(stack, state)?;
                    read(stack, state, "failsetting", true)?;
                    let fail = if stack.value(-1) == Some(Value::Nil) {
                        player.fail_enabled
                    } else {
                        deadsync_gameplay::GameplayFailType::from_value(stack.number(-1)? as f32)
                            .ok_or_else(|| stack.error(b"Invalid FailType"))?
                            != deadsync_gameplay::GameplayFailType::Off
                    };
                    let state = if fail && player.life <= 0. {
                        "HealthState_Dead"
                    } else if player.life >= 1. {
                        "HealthState_Hot"
                    } else if fail && player.life < crate::SONG_LUA_DANGER_LIFE {
                        "HealthState_Danger"
                    } else {
                        "HealthState_Alive"
                    };
                    stack.set_top(base)?;
                    Ok(Some(state))
                })?;
                let Some(state) = state else {
                    continue;
                };
                let old = self.health[i].unwrap_or("HealthState_Alive");
                self.health[i] = Some(state);
                runtime.player_status.lock().unwrap()[i].health = Some(state);
                if state != old {
                    let params = runtime.vm.try_new_table()?;
                    let root = runtime.vm.pin(Value::Table(params))?;
                    let result = (|| {
                        for (key, text) in [
                            ("PlayerNumber", crate::player_number_name(i)),
                            ("HealthState", state),
                            ("OldHealthState", old),
                        ] {
                            let text = runtime.vm.try_intern(text.as_bytes())?;
                            put(&mut runtime.vm, params, key, text)?;
                        }
                        runtime.broadcast_message("HealthStateChanged", Some(Value::Table(params)))
                    })();
                    runtime.vm.unpin(root)?;
                    result?;
                }
            }
            Ok(())
        })();
        runtime.vm.unpin(screen)?;
        result
    }

    pub fn release(&mut self, runtime: &mut RustSongRuntime) -> Result<(), Error> {
        self.owners(runtime)?;
        runtime.vm.unpin(self.owners.take().unwrap()).map(|_| ())
    }
}

fn publish(runtime: &mut RustSongRuntime, players: [SongLuaLivePlayer; LUA_PLAYERS]) {
    runtime.publish_live_players(players);
    let mut status = runtime.player_status.lock().unwrap();
    for (status, player) in status.iter_mut().zip(players) {
        status.autoplay = Some(player.autoplay);
    }
}
fn globals(stack: &mut NativeStack<'_, '_>) -> Result<usize, Error> {
    stack.push(stack.value(-10002).unwrap())?;
    Ok(stack.top())
}
fn require_table(stack: &NativeStack<'_, '_>, index: usize) -> Result<(), Error> {
    if matches!(stack.value(index as isize), Some(Value::Table(_))) {
        Ok(())
    } else {
        Err(stack.error(b"live frame table expected"))
    }
}
fn read(stack: &mut NativeStack<'_, '_>, owner: usize, key: &str, raw: bool) -> Result<(), Error> {
    stack.push_bytes(key.as_bytes())?;
    stack.get_table(owner as isize, raw)
}
fn index(stack: &mut NativeStack<'_, '_>, owner: usize, index: usize) -> Result<(), Error> {
    stack.push(Value::Number(index as f64))?;
    stack.get_table(owner as isize, true)
}
fn write(
    stack: &mut NativeStack<'_, '_>,
    owner: usize,
    key: &str,
    value: Value,
    raw: bool,
) -> Result<(), Error> {
    stack.push_bytes(key.as_bytes())?;
    stack.push(value)?;
    stack.set_table(owner as isize, raw)
}
fn runtime_table(stack: &mut NativeStack<'_, '_>, globals: usize) -> Result<(), Error> {
    read(stack, globals, SONG_LUA_RUNTIME_KEY, false)?;
    require_table(stack, stack.top())
}
fn feed(
    runtime: &mut RustSongRuntime,
    owners: Value,
    frame: &SongLuaLiveFrame,
) -> Result<(), Error> {
    runtime.vm.with_native_stack(|stack| {
        let base = stack.top();
        stack.push(owners)?;
        let batch = stack.top();
        index(stack, batch, 1)?;
        let stage = stack.top();
        write(
            stack,
            stage,
            "__songlua_gameplay_seconds",
            Value::Number(frame.screen_seconds),
            true,
        )?;
        stack.set_top(stack.top() - 1)?;
        for (i, player) in frame.players.iter().enumerate() {
            index(stack, batch, i + 2)?;
            let stats = stack.top();
            for (key, value) in [
                (
                    "__songlua_possible_dance_points",
                    player.possible_points as f64,
                ),
                ("__songlua_actual_dance_points", player.actual_points as f64),
                (
                    "__songlua_current_possible_dance_points",
                    player.current_possible_points as f64,
                ),
            ] {
                write(stack, stats, key, Value::Number(value), true)?;
            }
            write(
                stack,
                stats,
                "__songlua_failed",
                Value::Boolean(player.failed),
                true,
            )?;
            write(
                stack,
                stats,
                "__songlua_life",
                number(player.life_record_latest.unwrap_or(player.life)),
                true,
            )?;
            read(stack, stats, "__songlua_life_meter", true)?;
            if stack.value(-1) != Some(Value::Nil) {
                let meter = stack.top();
                require_table(stack, meter)?;
                write(stack, meter, "__songlua_life", number(player.life), true)?;
            }
            stack.set_top(stack.top() - 1)?;
            write(
                stack,
                stats,
                "__songlua_combo",
                Value::Number(player.combo as f64),
                true,
            )?;
            write(
                stack,
                stats,
                "__songlua_miss_combo",
                Value::Number(player.miss_combo as f64),
                true,
            )?;
            read(stack, stats, "__songlua_tap_scores", true)?;
            let counts = stack.top();
            require_table(stack, counts)?;
            for (name, count) in crate::live::TAP_SCORES.into_iter().zip(player.tap_scores) {
                write(stack, counts, name, Value::Number(count as f64), true)?;
            }
            stack.set_top(stack.top() - 2)?;
        }
        stack.set_top(base)
    })
}

fn frame_error(runtime: &mut RustSongRuntime, message: &[u8]) -> Error {
    match runtime
        .vm
        .with_native_stack(|stack| Ok(stack.error(message)))
    {
        Ok(error) | Err(error) => error,
    }
}
