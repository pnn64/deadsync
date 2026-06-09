pub fn sec_at_beat_from_bpms(normalized_bpms: &str, target_beat: f64) -> f64 {
    if !target_beat.is_finite() || target_beat <= 0.0 {
        return 0.0;
    }
    let bpm_map = normalized_bpm_map(normalized_bpms);
    let mut time = 0.0;
    let mut last_beat = 0.0;
    let mut last_bpm = bpm_map[0].1;
    for &(beat, bpm) in &bpm_map {
        if target_beat <= beat {
            let delta_beats = (target_beat - last_beat).max(0.0);
            if last_bpm > 0.0 {
                time += (delta_beats * 60.0) / last_bpm;
            }
            return time.max(0.0);
        }
        if beat > last_beat && last_bpm > 0.0 {
            time += ((beat - last_beat) * 60.0) / last_bpm;
        }
        last_beat = beat;
        last_bpm = bpm;
    }
    if last_bpm > 0.0 {
        time += ((target_beat - last_beat).max(0.0) * 60.0) / last_bpm;
    }
    time.max(0.0)
}

pub fn beat_at_sec_from_bpms(normalized_bpms: &str, target_sec: f64) -> f64 {
    if !target_sec.is_finite() || target_sec <= 0.0 {
        return 0.0;
    }
    let bpm_map = normalized_bpm_map(normalized_bpms);
    let mut elapsed = 0.0;
    let mut last_beat = 0.0;
    let mut last_bpm = bpm_map[0].1;
    for &(beat, bpm) in &bpm_map {
        let delta_beats = (beat - last_beat).max(0.0);
        let delta_sec = if last_bpm > 0.0 {
            (delta_beats * 60.0) / last_bpm
        } else {
            0.0
        };
        if elapsed + delta_sec >= target_sec {
            let remain = (target_sec - elapsed).max(0.0);
            let add_beats = if last_bpm > 0.0 {
                remain * last_bpm / 60.0
            } else {
                0.0
            };
            return (last_beat + add_beats).max(0.0);
        }
        elapsed += delta_sec;
        last_beat = beat;
        last_bpm = bpm;
    }
    let remain = (target_sec - elapsed).max(0.0);
    let add_beats = if last_bpm > 0.0 {
        remain * last_bpm / 60.0
    } else {
        0.0
    };
    (last_beat + add_beats).max(0.0)
}

fn normalized_bpm_map(normalized_bpms: &str) -> Vec<(f64, f64)> {
    let mut bpm_map = rssp::bpm::parse_bpm_map(normalized_bpms);
    if bpm_map.is_empty() {
        bpm_map.push((0.0, 60.0));
    }
    if bpm_map.first().is_none_or(|(beat, _)| *beat != 0.0) {
        let first_bpm = bpm_map[0].1;
        bpm_map.insert(0, (0.0, first_bpm));
    }
    bpm_map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sec_at_beat_uses_bpm_segments() {
        let bpms = "0.000=120.000,4.000=60.000";

        assert_eq!(sec_at_beat_from_bpms(bpms, 4.0), 2.0);
        assert_eq!(sec_at_beat_from_bpms(bpms, 6.0), 4.0);
    }

    #[test]
    fn beat_at_sec_uses_bpm_segments() {
        let bpms = "0.000=120.000,4.000=60.000";

        assert_eq!(beat_at_sec_from_bpms(bpms, 2.0), 4.0);
        assert_eq!(beat_at_sec_from_bpms(bpms, 4.0), 6.0);
    }

    #[test]
    fn empty_bpm_map_defaults_to_sixty_bpm() {
        assert_eq!(sec_at_beat_from_bpms("", 2.0), 2.0);
        assert_eq!(beat_at_sec_from_bpms("", 2.0), 2.0);
    }
}
