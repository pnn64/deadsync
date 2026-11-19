//! Host statistics phases, separate from clock, option and health publication.
use super::*;

// Keep the frame payload inline: boxing it would allocate on every live update.
#[allow(clippy::large_enum_variant)]
pub(super) enum Phase {
    Reset(Box<SongLuaCompileContext>),
    Feed {
        seconds: f64,
        steps_seconds: f64,
        players: [crate::SongLuaLivePlayer; LUA_PLAYERS],
        routine: Option<crate::SongLuaLivePlayer>,
    },
}
#[derive(Default)]
pub(super) struct State {
    pub pending: Option<Phase>,
    pub players: Option<[crate::SongLuaLivePlayer; LUA_PLAYERS]>,
    pub routine: Option<crate::SongLuaLivePlayer>,
    pub life_records: [deadsync_gameplay::StageLifeRecord; LUA_PLAYERS],
    pub routine_life_record: deadsync_gameplay::StageLifeRecord,
    pub writes: Vec<SongLuaLivePlayerWrite>,
    /// Sticky across preparation phases: these values depend on actual play.
    pub requires_live: bool,
}

impl State {
    pub fn reset_life_records(&mut self, context: &SongLuaCompileContext) {
        let combined = crate::song_lua_style_info(&context.style_name).style_type
            == "StyleType_TwoPlayersSharedSides";
        for (record, player) in self.life_records.iter_mut().zip(&context.players) {
            record.clear();
            if player.enabled && !combined { record.record(crate::SONG_LUA_INITIAL_LIFE); }
        }
        self.routine_life_record.clear();
    }
}

impl SongRuntime {
    /// Publish actual PlayerStageStats histories while reusing their retained
    /// buffers. Keep variable-length history out of the inline frame payload.
    pub fn publish_gameplay_life_records<P: deadsync_gameplay::GameplayProfileData, O, C, D>(
        &mut self,
        game: &deadsync_gameplay::GameplayRuntimeState<P, O, C, D>,
    ) {
        let mut state = self.statistics.lock().unwrap();
        for (index, (record, player)) in state.life_records.iter_mut().zip(&game.players_runtime.players).enumerate() {
            if index < game.num_players() { record.copy_from(&player.stage_life_record); }
            else { record.clear(); }
        }
        if let Some(routine) = &game.players_runtime.routine {
            state.routine_life_record.copy_from(&routine.stats.stage_life_record);
        } else {
            state.routine_life_record.clear();
        }
    }

    pub(crate) fn require_statistics_offline_compatible(&self) -> Result<(), String> {
        if self.statistics.lock().unwrap().requires_live {
            Err(crate::runtime_fallback::STATISTICS_REQUIRED.into())
        } else {
            Ok(())
        }
    }

    /// Reset the retained stage fields and selected-step arrays at a load boundary.
    /// This is distinct from STATSMAN:Reset(), whose existing policy is a no-op.
    pub fn reset_stage_statistics(&mut self, context: &SongLuaCompileContext) -> Result<(), Error> {
        self.statistics_phase(Phase::Reset(Box::new(context.clone())))
    }
    /// Publish this frame to retained statistics and any bound life meters.
    /// It does not advance clocks, options, live players, health or messages.
    pub fn feed_stage_statistics(&mut self, frame: &crate::SongLuaLiveFrame) -> Result<(), Error> {
        self.statistics_phase(Phase::Feed {
            seconds: frame.gameplay_seconds,
            steps_seconds: frame.steps_seconds,
            players: frame.players,
            routine: frame.routine,
        })
    }
    /// The host publishes live players after its clock phase, before script updates.
    pub fn publish_live_players(&mut self, players: [crate::SongLuaLivePlayer; LUA_PLAYERS]) {
        self.statistics.lock().unwrap().players = Some(players);
    }
    /// Consume script writes once, preserving their authored order.
    pub fn take_player_writes(&mut self) -> Vec<SongLuaLivePlayerWrite> {
        std::mem::take(&mut self.statistics.lock().unwrap().writes)
    }
    fn statistics_phase(&mut self, phase: Phase) -> Result<(), Error> {
        self.statistics.lock().unwrap().pending = Some(phase);
        let result = (|| {
            let Value::Table(catalog) = self.vm.rooted(self.catalog)? else {
                unreachable!()
            };
            let key = self.vm.intern_static(b"statistics_phase")?;
            let callback = self.vm.raw_table_get(catalog, key)?;
            self.vm.call(callback, &[]).map(|_| ())
        })();
        self.statistics.lock().unwrap().pending = None;
        result
    }
}
