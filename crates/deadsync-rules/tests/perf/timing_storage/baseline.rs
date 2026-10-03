// Table construction frozen from 4499f127a (0.5.1700).
use super::*;

fn original_points(
    bpms: &[(f32, f32)],
    song_offset_ns: TimingNs,
) -> (Arc<Vec<BeatTimePoint>>, f32) {
    let mut beat_to_time = Vec::with_capacity(bpms.len());
    let mut current_time = 0.0;
    let mut last_beat = 0.0;
    let mut last_bpm = bpms[0].1;
    let mut max_bpm = 0.0;

    for &(beat, bpm) in bpms {
        if beat > last_beat && last_bpm > 0.0 {
            current_time = (beat - last_beat).mul_add(60.0 / last_bpm, current_time);
        }
        beat_to_time.push(BeatTimePoint {
            beat,
            time_ns: timing_ns_add_seconds(song_offset_ns, current_time),
            bpm,
        });
        if bpm.is_finite() && bpm > max_bpm {
            max_bpm = bpm;
        }
        last_beat = beat;
        last_bpm = bpm;
    }

    (Arc::new(beat_to_time), max_bpm)
}
