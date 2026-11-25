//! Handoff from a recorded machine score to stage playback.

use deadsync_gameplay::{ReplayInputEdge, ReplayOffsetSnapshot};
use deadsync_score::StageReplayRequest;

/// Preserve recorded edge order and the beat-zero offset, including for empty recordings.
#[must_use]
pub fn prepare_stage_replay(replay: StageReplayRequest) -> (Vec<ReplayInputEdge>, ReplayOffsetSnapshot) {
    let edges = replay
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
        beat0_time_ns: replay.replay_beat0_time_ns,
    };
    (edges, offsets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_core::input::InputSource;
    use deadsync_score::ReplayEdge;

    #[test]
    fn stage_replay_preserves_edge_order_and_recorded_offset() {
        let mut request = StageReplayRequest {
            replay: Vec::new(),
            name: "AAA".to_owned(),
            score: 9876.0,
            replay_beat0_time_ns: -150_000_000,
        };
        let (empty, offset) = prepare_stage_replay(request.clone());
        assert!(empty.is_empty());
        assert_eq!(offset.beat0_time_ns, request.replay_beat0_time_ns);

        request.replay = vec![
            ReplayEdge {
                event_music_time_ns: 20,
                lane_index: 3,
                pressed: true,
                source: InputSource::Gamepad,
            },
            ReplayEdge {
                event_music_time_ns: 10,
                lane_index: 1,
                pressed: false,
                source: InputSource::Keyboard,
            },
        ];
        let (edges, offset) = prepare_stage_replay(request.clone());
        assert_eq!(offset.beat0_time_ns, request.replay_beat0_time_ns);
        assert_eq!(edges.len(), request.replay.len());
        for (actual, recorded) in edges.iter().zip(request.replay) {
            assert_eq!(actual.lane_index, recorded.lane_index);
            assert_eq!(actual.pressed, recorded.pressed);
            assert_eq!(actual.source, recorded.source);
            assert_eq!(actual.event_music_time_ns, recorded.event_music_time_ns);
        }
    }
}
