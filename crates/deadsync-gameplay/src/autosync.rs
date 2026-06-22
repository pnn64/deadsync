#[inline(always)]
pub fn autosync_mean_ns(samples: &[SongTimeNs; AUTOSYNC_OFFSET_SAMPLE_COUNT]) -> SongTimeNs {
    let mut sum = 0i128;
    for value in samples {
        sum += i128::from(*value);
    }
    let count = AUTOSYNC_OFFSET_SAMPLE_COUNT as i128;
    let rounded = if sum >= 0 {
        (sum + count / 2) / count
    } else {
        (sum - count / 2) / count
    };
    rounded.clamp(i64::MIN as i128, i64::MAX as i128) as SongTimeNs
}

#[inline(always)]
pub fn autosync_stddev_seconds(
    samples: &[SongTimeNs; AUTOSYNC_OFFSET_SAMPLE_COUNT],
    mean_ns: SongTimeNs,
) -> f32 {
    let mut dev = 0.0_f64;
    for value in samples {
        let d = (i128::from(*value) - i128::from(mean_ns)) as f64 / 1_000_000_000.0;
        dev += d * d;
    }
    (dev / AUTOSYNC_OFFSET_SAMPLE_COUNT as f64).sqrt() as f32
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AutosyncOffsetCorrection {
    Song(f32),
    Machine(f32),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AutosyncSampleResult {
    pub standard_deviation: Option<f32>,
    pub correction: Option<AutosyncOffsetCorrection>,
}

pub fn apply_autosync_offset_sample(
    samples: &mut [SongTimeNs; AUTOSYNC_OFFSET_SAMPLE_COUNT],
    sample_count: &mut usize,
    mode: AutosyncMode,
    note_off_by_ns: SongTimeNs,
) -> AutosyncSampleResult {
    if song_time_ns_invalid(note_off_by_ns) || mode == AutosyncMode::Off {
        return AutosyncSampleResult::default();
    }

    let sample_ix = (*sample_count).min(AUTOSYNC_OFFSET_SAMPLE_COUNT.saturating_sub(1));
    samples[sample_ix] = note_off_by_ns;
    *sample_count = (*sample_count).saturating_add(1);
    if *sample_count < AUTOSYNC_OFFSET_SAMPLE_COUNT {
        return AutosyncSampleResult::default();
    }

    let mean_ns = autosync_mean_ns(samples);
    let stddev = autosync_stddev_seconds(samples, mean_ns);
    let correction = if stddev < AUTOSYNC_STDDEV_MAX_SECONDS {
        let mean = song_time_ns_to_seconds(mean_ns);
        match mode {
            AutosyncMode::Off => None,
            AutosyncMode::Song => Some(AutosyncOffsetCorrection::Song(mean)),
            AutosyncMode::Machine => Some(AutosyncOffsetCorrection::Machine(mean)),
        }
    } else {
        None
    };

    *sample_count = 0;
    AutosyncSampleResult {
        standard_deviation: Some(stddev),
        correction,
    }
}

#[inline(always)]
pub const fn autosync_row_hits_enabled(
    replay_mode: bool,
    scoring_blocked: bool,
    mode: AutosyncMode,
    course_active: bool,
) -> bool {
    !replay_mode && !scoring_blocked && !matches!(mode, AutosyncMode::Off) && !course_active
}

pub fn collect_autosync_row_hit_offsets(
    notes: &[Note],
    row_entry: &RowEntry,
    offsets: &mut [SongTimeNs; MAX_COLS],
) -> usize {
    let mut count = 0usize;
    for &note_index in row_entry.note_indices() {
        if count >= offsets.len() {
            break;
        }
        let Some(judgment) = notes.get(note_index).and_then(|note| note.result.as_ref()) else {
            continue;
        };
        if matches!(
            judgment.grade,
            JudgeGrade::Fantastic | JudgeGrade::Excellent | JudgeGrade::Great
        ) {
            // ITG's fNoteOffset is positive when stepping early.
            offsets[count] = judgment.time_error_music_ns.saturating_neg();
            count += 1;
        }
    }
    count
}

