//! Gameplay owns the installed closures; all five share the live scene's VM.
use super::super::live_runtime::Retained;
use super::*;
use actors::live_gameplay::Event;
use deadsync_gameplay::{GameplayProfileData, GameplayRuntimeState};
use deadsync_rules::{judgment::Judgment, timing::TimingData};

struct Callbacks {
    owner: SongLiveRuntime,
    statistics: Retained,
    timing: Option<TimingData>,
}
impl Callbacks {
    fn feed<P: GameplayProfileData, O, C, D>(
        &self,
        runtime: &mut SongRuntime,
        game: &GameplayRuntimeState<P, O, C, D>,
    ) -> Result<SongLuaLiveFrame, Error> {
        let frame = SongLuaLiveFrame::from_gameplay(game, 0.);
        let owners = runtime.vm.rooted(self.statistics.root)?;
        runtime.feed_gameplay_callback(owners, &frame, self.timing.as_ref())?;
        Ok(frame)
    }
    fn dispatch<P: GameplayProfileData, O, C, D>(
        &self,
        game: &mut GameplayRuntimeState<P, O, C, D>,
        flag: &'static str,
        label: &'static str,
        event: impl FnOnce(&GameplayRuntimeState<P, O, C, D>) -> Event,
    ) {
        let writes = {
            let mut guard = self.owner.access();
            let runtime = &mut *guard;
            let result = self
                .feed(runtime, game)
                .and_then(|_| runtime.actors.gameplay_event(&mut runtime.vm, event(game)));
            if let Err(e) = result {
                runtime
                    .actors
                    .gameplay_error(&mut runtime.vm, None, flag, label, e);
            }
            runtime.take_player_writes()
        };
        crate::live::apply_player_writes(writes, game);
    }
}
impl SongLiveScene {
    fn callbacks(&self) -> Result<Callbacks, Error> {
        let mut runtime = self.runtime.access();
        let owners = self.frames.owners(&mut runtime)?;
        Ok(Callbacks {
            owner: self.runtime.clone(),
            statistics: self.runtime.retain(&mut runtime, owners)?,
            timing: self.context.song_timing.clone(),
        })
    }
    pub fn context(&self) -> &SongLuaCompileContext {
        &self.context
    }
    pub fn install_step_callback<P: GameplayProfileData, O, C, D>(
        &self,
        game: &mut GameplayRuntimeState<P, O, C, D>,
    ) -> Result<(), Error> {
        let c = self.callbacks()?;
        game.step_callback = Some(Box::new(move |game, column| {
            c.dispatch(
                game,
                "__songlua_step_error_reported",
                "StepMessage",
                |game| {
                    let player = game.player_for_col(column);
                    Event::Step {
                        player,
                        column: column - player * game.cols_per_player(),
                    }
                },
            )
        }));
        Ok(())
    }
    pub fn install_tap_row_judgment_callback<P: GameplayProfileData, O, C, D>(
        &self,
        game: &mut GameplayRuntimeState<P, O, C, D>,
    ) -> Result<(), Error> {
        let c = self.callbacks()?;
        game.tap_row_judgment_callback = Some(Box::new(move |game, player, row, judgment| {
            c.dispatch(
                game,
                "__songlua_judgment_error_reported",
                "JudgmentMessage",
                |game| row_event(game, player, row, judgment),
            )
        }));
        Ok(())
    }
    pub fn install_hold_judgment_callback<P: GameplayProfileData, O, C, D>(
        &self,
        game: &mut GameplayRuntimeState<P, O, C, D>,
    ) -> Result<(), Error> {
        let c = self.callbacks()?;
        game.hold_judgment_callback = Some(Box::new(move |game, player, note, result| {
            c.dispatch(
                game,
                "__songlua_hold_judgment_error_reported",
                "Hold JudgmentMessage",
                |game| Event::Hold {
                    player,
                    columns: game.cols_per_player(),
                    note: game.chart_runtime.notes[note].clone(),
                    result,
                },
            )
        }));
        Ok(())
    }
    pub fn install_avoided_mine_judgment_callback<P: GameplayProfileData, O, C, D>(
        &self,
        game: &mut GameplayRuntimeState<P, O, C, D>,
    ) -> Result<(), Error> {
        let c = self.callbacks()?;
        game.avoided_mine_judgment_callback = Some(Box::new(move |game, player, column| {
            c.dispatch(
                game,
                "__songlua_avoided_mine_error_reported",
                "AvoidMine JudgmentMessage",
                |game| Event::Mine {
                    player,
                    column: column - player * game.cols_per_player(),
                },
            )
        }));
        Ok(())
    }
    pub fn install_note_field_tap_callback<P: GameplayProfileData, O, C, D>(
        &self,
        game: &mut GameplayRuntimeState<P, O, C, D>,
        bright_combo_threshold: u32,
    ) -> Result<(), Error> {
        let fields = {
            let mut guard = self.runtime.access();
            let runtime = &mut *guard;
            let fields = runtime.actors.gameplay_fields(&mut runtime.vm)?;
            self.runtime.retain(runtime, fields)?
        };
        let c = self.callbacks()?;
        game.note_field_tap_callback = Some(Box::new(move |game, tap| {
            if tap.player >= LUA_PLAYERS {
                return tap;
            }
            let mut guard = c.owner.access();
            let runtime = &mut *guard;
            let fields = fields.value(runtime).expect("owned note fields");
            let Value::Table(fields) = fields else {
                unreachable!()
            };
            let field = runtime
                .vm
                .raw_table_get(fields, Value::Number((tap.player + 1) as f64))
                .expect("owned note fields");
            if field == Value::Nil {
                return tap;
            }
            let result = (|| {
                let key = runtime.vm.try_intern(b"__songlua_did_tap_note_callback")?;
                let Value::Table(t) = field else {
                    unreachable!()
                };
                let callback = runtime.vm.raw_table_get(t, key)?;
                if callback == Value::Nil {
                    return Ok(tap);
                }
                if !matches!(callback, Value::Function(_) | Value::Native(_)) {
                    return Err(error(runtime, b"DidTapNote callback must be a function"));
                }
                let retained = runtime.vm.pin(callback)?;
                let result = (|| {
                    let frame = c.feed(runtime, game)?;
                    let mut native_tap = tap;
                    native_tap.bright = tap.score != "HitMine"
                        && (frame.players[tap.player].combo > bright_combo_threshold
                            || game
                                .effective_visibility_effects_for_player(tap.player)
                                .blind
                                != 0.);
                    runtime.actors.gameplay_tap(
                        &mut runtime.vm,
                        field,
                        callback,
                        native_tap,
                        game.cols_per_player(),
                    )
                })();
                runtime.vm.unpin(retained)?;
                result
            })();
            let result = match result {
                Ok(v) => v,
                Err(e) => {
                    runtime.actors.gameplay_error(
                        &mut runtime.vm,
                        Some(field),
                        "__songlua_did_tap_note_error_reported",
                        "DidTapNote callback",
                        e,
                    );
                    tap
                }
            };
            let writes = runtime.take_player_writes();
            drop(guard);
            crate::live::apply_player_writes(writes, game);
            result
        }));
        Ok(())
    }
    pub fn apply_pending_player_writes<P: GameplayProfileData, O, C, D>(
        &self,
        game: &mut GameplayRuntimeState<P, O, C, D>,
    ) {
        let writes = self.runtime.access().take_player_writes();
        crate::live::apply_player_writes(writes, game);
    }
    pub fn apply_output_player_writes<P: GameplayProfileData, O, C, D>(
        &self,
        output: &SongLuaLiveOutput,
        game: &mut GameplayRuntimeState<P, O, C, D>,
    ) {
        crate::live::apply_player_writes(output.player_writes.iter().copied(), game);
    }
    pub fn prepare_gameplay_player_options<P: GameplayProfileData, O, C, D>(
        &self,
        game: &mut GameplayRuntimeState<P, O, C, D>,
    ) -> Result<(), Error> {
        let mut runtime = self.runtime.access();
        for player in 0..LUA_PLAYERS {
            game.mods.song_lua_fail_types[player] = runtime.gameplay_fail_type(player)?;
        }
        Ok(())
    }
}
fn row_event<P: GameplayProfileData, O, C, D>(
    game: &GameplayRuntimeState<P, O, C, D>,
    player: usize,
    row_entry: usize,
    judgment: Judgment,
) -> Event {
    let row = &game.chart_runtime.row_entries[row_entry];
    let notes = &game.chart_runtime.notes;
    let columns = game.cols_per_player();
    let first = player * columns;
    let first_track = row
        .note_indices()
        .iter()
        .map(|i| notes[i.get()].column)
        .min()
        .unwrap_or(first)
        - first;
    let early = row
        .note_indices()
        .iter()
        .map(|i| &notes[i.get()])
        .find(|n| {
            n.result.is_some_and(|r| {
                r.time_error_music_ns == judgment.time_error_music_ns && r.grade == judgment.grade
            })
        })
        .and_then(|n| n.early_result);
    let mut current = Vec::new();
    let mut holds = Vec::new();
    for column in first..first + columns {
        let lane = &game.chart_runtime.lane_indices.note_indices[column];
        let at = lane.partition_point(|i| notes[i.get()].row_index < row.row_index.get());
        if let Some(i) = lane.get(at)
            && notes[i.get()].row_index == row.row_index.get()
        {
            current.push((column - first, notes[i.get()].clone()));
            continue;
        }
        let lane = &game.chart_runtime.lane_indices.hold_indices[column];
        let at = lane.partition_point(|i| notes[i.get()].row_index < row.row_index.get());
        if let Some(i) = at.checked_sub(1).and_then(|i| lane.get(i)) {
            let n = &notes[i.get()];
            if n.hold
                .as_ref()
                .is_some_and(|h| h.end_row_index >= row.row_index.get())
            {
                holds.push((column - first, n.clone()));
            }
        }
    }
    Event::Row {
        player,
        first_track,
        judgment,
        early,
        notes: current,
        holds,
    }
}
