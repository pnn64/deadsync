// Frozen from 4499f127a (0.5.1700).
use super::*;

fn original_time_signatures(timing_segments: &TimingSegments) -> Vec<TimeSignatureSegment> {
    let mut sigs = timing_segments.time_signatures.clone();
    if sigs.is_empty() {
        sigs.push(default_time_signature());
    }
    sigs.sort_by(|a, b| a.beat.total_cmp(&b.beat));
    if sigs
        .first()
        .is_none_or(|sig| beat_to_note_row(sig.beat) > 0)
    {
        sigs.insert(0, default_time_signature());
    }
    sigs
}
