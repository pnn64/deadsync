//! Host statistics phases, separate from clock, option and health publication.
use super::*;

// Keep the frame payload inline: boxing it would allocate on every live update.
#[allow(clippy::large_enum_variant)]
pub(super) enum Phase {
    Reset(Box<SongLuaCompileContext>),
    Feed {
        seconds: f64,
        players: [crate::SongLuaLivePlayer; LUA_PLAYERS],
    },
}
#[derive(Default)]
pub(super) struct State {
    pub pending: Option<Phase>,
    pub players: Option<[crate::SongLuaLivePlayer; LUA_PLAYERS]>,
    pub writes: Vec<SongLuaLivePlayerWrite>,
}

impl SongRuntime {
    /// Reset the retained stage fields and selected-step arrays at a load boundary.
    /// This is distinct from STATSMAN:Reset(), whose existing policy is a no-op.
    pub fn reset_stage_statistics(&mut self, context: &SongLuaCompileContext) -> Result<(), Error> {
        self.statistics_phase(Phase::Reset(Box::new(context.clone())))
    }
    /// Publish this frame to retained statistics and any bound life meters.
    /// It does not advance clocks, options, live players, health or messages.
    pub fn feed_stage_statistics(&mut self, frame: &crate::SongLuaLiveFrame) -> Result<(), Error> {
        self.statistics_phase(Phase::Feed {
            seconds: frame.screen_seconds,
            players: frame.players,
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
            let key = self.vm.try_intern(b"statistics_phase")?;
            let callback = self.vm.raw_table_get(catalog, key)?;
            self.vm.call(callback, &[]).map(|_| ())
        })();
        self.statistics.lock().unwrap().pending = None;
        result
    }
}
