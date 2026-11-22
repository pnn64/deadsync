//! Handoff from a recorded machine score to stage playback.

use deadsync_core::song_time::SongTimeNs;
use deadsync_gameplay::{ReplayInputEdge, ReplayOffsetSnapshot};
use deadsync_score::{MachineReplayEntry, ReplayEdge};

#[derive(Clone, Debug)]
pub struct StageReplayRequest {
    pub replay: Vec<ReplayEdge>,
    pub name: String,
    pub score: f64,
    pub replay_beat0_time_ns: SongTimeNs,
}

impl From<MachineReplayEntry> for StageReplayRequest {
    fn from(entry: MachineReplayEntry) -> Self {
        Self {
            replay: entry.replay,
            name: entry.name,
            score: entry.score,
            replay_beat0_time_ns: entry.replay_beat0_time_ns,
        }
    }
}

impl StageReplayRequest {
    /// Preserve the recorded edge order and beat-zero offset, including the
    /// offset of an empty recording.
    #[must_use]
    pub fn into_playback(self) -> (Vec<ReplayInputEdge>, ReplayOffsetSnapshot) {
        let edges = self
            .replay
            .into_iter()
            .map(|edge| ReplayInputEdge {
                lane_index: edge.lane_index,
                pressed: edge.pressed,
                source: edge.source,
                event_music_time_ns: edge.event_music_time_ns,
            })
            .collect();
        let offsets = ReplayOffsetSnapshot {
            beat0_time_ns: self.replay_beat0_time_ns,
        };
        (edges, offsets)
    }
}
