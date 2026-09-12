// Frozen from 4b4edc786 (0.5.1152); only imports and helper visibility adapted.
use super::*;
#[derive(Debug, Clone, Serialize)]
pub struct ArrowCloudModifiers {
    #[serde(rename = "visualDelay")]
    pub visual_delay: i32,
    pub acceleration: Vec<String>,
    pub appearance: Vec<String>,
    pub effect: Vec<String>,
    pub mini: i32,
    pub turn: String,
    #[serde(rename = "disabledWindows")]
    pub disabled_windows: String,
    pub speed: ArrowCloudSpeed,
    pub perspective: String,
    pub noteskin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll: Option<String>,
}

#[inline(always)]
fn mask_labels_u8(mask: u8, names: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, name) in names.iter().enumerate() {
        if (mask & (1u8 << i)) != 0 {
            out.push((*name).to_string());
        }
    }
    out
}

#[inline(always)]
fn mask_labels_u16(mask: u16, names: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, name) in names.iter().enumerate() {
        if (mask & (1u16 << i)) != 0 {
            out.push((*name).to_string());
        }
    }
    out
}

#[inline(always)]
#[must_use]
pub const fn turn_label(turn: profile_data::TurnOption) -> &'static str {
    match turn {
        profile_data::TurnOption::None => "None",
        profile_data::TurnOption::Mirror => "Mirror",
        profile_data::TurnOption::Left => "Left",
        profile_data::TurnOption::Right => "Right",
        profile_data::TurnOption::LRMirror => "LR-Mirror",
        profile_data::TurnOption::UDMirror => "UD-Mirror",
        profile_data::TurnOption::Shuffle
        | profile_data::TurnOption::Blender
        | profile_data::TurnOption::Random => "Shuffle",
    }
}

#[inline(always)]
#[must_use]
pub fn scroll_label(scroll: profile_data::ScrollOption) -> Option<String> {
    if scroll.contains(profile_data::ScrollOption::Reverse) {
        Some("Reverse".to_string())
    } else if scroll.contains(profile_data::ScrollOption::Split) {
        Some("Split".to_string())
    } else if scroll.contains(profile_data::ScrollOption::Alternate) {
        Some("Alternate".to_string())
    } else if scroll.contains(profile_data::ScrollOption::Cross) {
        Some("Cross".to_string())
    } else if scroll.contains(profile_data::ScrollOption::Centered) {
        Some("Centered".to_string())
    } else {
        None
    }
}

#[inline(always)]
#[must_use]
pub fn speed_payload(speed: ScrollSpeedSetting) -> ArrowCloudSpeed {
    match speed {
        ScrollSpeedSetting::CMod(value) => ArrowCloudSpeed {
            value: f64::from(value),
            speed_type: "C",
        },
        ScrollSpeedSetting::MMod(value) => ArrowCloudSpeed {
            value: f64::from(value),
            speed_type: "M",
        },
        ScrollSpeedSetting::XMod(value) => ArrowCloudSpeed {
            value: (f64::from(value) * 100.0).round() / 100.0,
            speed_type: "X",
        },
    }
}

#[must_use]
pub fn modifiers_from_profile(profile: &Profile) -> ArrowCloudModifiers {
    ArrowCloudModifiers {
        visual_delay: profile.visual_delay_ms,
        acceleration: mask_labels_u8(
            profile.accel_effects_active_mask.bits(),
            &ARROWCLOUD_ACCEL_NAMES,
        ),
        appearance: mask_labels_u8(
            profile.appearance_effects_active_mask.bits(),
            &ARROWCLOUD_APPEARANCE_NAMES,
        ),
        effect: mask_labels_u16(
            profile.visual_effects_active_mask.bits(),
            &ARROWCLOUD_EFFECT_NAMES,
        ),
        mini: profile.mini_percent.clamp(-100, 150),
        turn: turn_label(profile.turn_option).to_string(),
        disabled_windows: "None".to_string(),
        speed: speed_payload(profile.scroll_speed),
        perspective: profile.perspective.to_string(),
        noteskin: profile.noteskin.as_str().to_string(),
        scroll: scroll_label(profile.scroll_option),
    }
}

fn life_lerp_at(life_history: &[(f32, f32)], start_time: f32, sample_time: f32) -> f32 {
    let Some(&(_, start_life)) = life_history.first() else {
        return 0.0;
    };
    let start_life = start_life.clamp(0.0, 1.0);
    if sample_time <= start_time {
        return start_life;
    }

    let records = &life_history[life_history.partition_point(|&(time, _)| time < start_time)..];
    let later_ix = records.partition_point(|&(time, _)| time <= sample_time);
    let (earlier_time, earlier_life) = if later_ix == 0 {
        (start_time, start_life)
    } else {
        records[later_ix - 1]
    };
    let Some(&(later_time, later_life)) = records.get(later_ix) else {
        return earlier_life.clamp(0.0, 1.0);
    };
    let dt = later_time - earlier_time;
    if dt.abs() <= f32::EPSILON {
        return earlier_life.clamp(0.0, 1.0);
    }
    let alpha = ((sample_time - earlier_time) / dt).clamp(0.0, 1.0);
    (later_life - earlier_life)
        .mul_add(alpha, earlier_life)
        .clamp(0.0, 1.0)
}

#[must_use]
pub fn lifebar_points(
    life_history: &[(f32, f32)],
    chart_start_second: f32,
    first_second: f32,
    last_second: f32,
    music_rate: f32,
    point_count: usize,
) -> Vec<ArrowCloudLifePoint> {
    if life_history.is_empty() || point_count == 0 {
        return Vec::new();
    }
    let last_second = last_second.max(first_second);
    let duration = (last_second - first_second).max(0.0);
    let x_step = duration / point_count as f32;
    // Arrow Cloud's Simply Love module samples ITGmania's unscaled
    // m_fStepsSeconds record, then projects those values onto song seconds.
    let life_step = last_second.max(0.0) / point_count as f32;
    let music_rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    };
    let mut out = Vec::with_capacity(point_count);
    for i in 0..point_count {
        let sample = i as f32;
        let x = sample.mul_add(x_step, chart_start_second);
        let sample_music_time = (sample * life_step).mul_add(music_rate, chart_start_second);
        out.push(ArrowCloudLifePoint {
            x: f64::from(x),
            y: f64::from(life_lerp_at(
                life_history,
                chart_start_second,
                sample_music_time,
            )),
        });
    }
    out
}

#[must_use]
pub fn timing_data_from_scatter(
    scatter: &[ScatterPoint],
    fail_time_s: Option<f32>,
) -> Vec<ArrowCloudTimingDatum> {
    let mut out = Vec::with_capacity(scatter.len());
    for point in scatter {
        if !point.time_sec.is_finite() {
            continue;
        }
        if let Some(fail_time) = fail_time_s
            && point.time_sec > fail_time
        {
            continue;
        }
        let value = if let Some(offset_ms) = point.offset_ms {
            if !offset_ms.is_finite() {
                continue;
            }
            ArrowCloudTimingOffset::Seconds(f64::from(offset_ms / 1000.0))
        } else {
            ArrowCloudTimingOffset::Miss("Miss")
        };
        out.push((f64::from(point.time_sec), value));
    }
    out
}

#[inline(always)]
fn local_direction_code(note: &Note, col_offset: usize, cols_per_player: usize) -> Option<u8> {
    if note.column < col_offset {
        return None;
    }
    let local = note.column - col_offset;
    if local >= cols_per_player {
        return None;
    }
    let code = local.saturating_add(1).min(u8::MAX as usize) as u8;
    Some(code)
}

/// Builds one point per judged row. `foot_by_row`, when supplied, must be
/// sorted by ascending row index so parity joins remain a single linear pass.
#[inline(always)]
#[must_use]
pub fn build_scatter_points(
    notes: &[Note],
    note_time_cache_ns: &[i64],
    col_offset: usize,
    cols_per_player: usize,
    foot_by_row: Option<&[(usize, ScatterFoot)]>,
) -> Vec<ScatterPoint> {
    debug_assert!(foot_by_row.is_none_or(|rows| rows.windows(2).all(|pair| pair[0].0 < pair[1].0)));
    let mut out = Vec::with_capacity(notes.len());
    let mut row_start = 0usize;
    let mut foot_row_idx = 0usize;

    while row_start < notes.len() {
        let row = notes[row_start].row_index;
        let mut row_end = row_start + 1;
        while row_end < notes.len() && notes[row_end].row_index == row {
            row_end += 1;
        }
        let parity_foot = if let Some(rows) = foot_by_row {
            while foot_row_idx < rows.len() && rows[foot_row_idx].0 < row {
                foot_row_idx += 1;
            }
            rows.get(foot_row_idx)
                .filter(|(foot_row, _)| *foot_row == row)
                .map(|(_, foot)| *foot)
                .unwrap_or_default()
        } else {
            ScatterFoot::Unknown
        };

        let mut row_judgment = None;
        let mut representative_ix: Option<usize> = None;
        let mut direction_code = 0u8;
        for (offset, n) in notes[row_start..row_end].iter().enumerate() {
            let i = row_start + offset;
            if n.is_fake || !n.can_be_judged || matches!(n.note_type, NoteType::Mine) {
                continue;
            }
            if let Some(judgment) = n.result.as_ref() {
                judgment::select_row_final_judgment(&mut row_judgment, judgment);
                representative_ix.get_or_insert(i);
            }
            if let Some(code) = local_direction_code(n, col_offset, cols_per_player) {
                direction_code = direction_code.saturating_add(code);
            }
        }

        let Some(judgment) = row_judgment else {
            row_start = row_end;
            continue;
        };
        let Some(idx) = representative_ix else {
            row_start = row_end;
            continue;
        };
        let t = note_time_cache_ns
            .get(idx)
            .copied()
            .map(|time_ns| (time_ns as f64 * 1.0e-9) as f32)
            .unwrap_or(0.0);
        let offset_ms = if judgment.grade == JudgeGrade::Miss {
            None
        } else {
            Some(judgment.time_error_ms)
        };

        out.push(ScatterPoint {
            time_sec: t,
            offset_ms,
            direction_code,
            miss_because_held: judgment.grade == JudgeGrade::Miss && judgment.miss_because_held,
            row_index: row,
            quantization_idx: notes[idx].quantization_idx,
            parity_foot,
        });

        row_start = row_end;
    }

    out
}
#[derive(Debug, Clone, Serialize)]
pub struct ArrowCloudPayload {
    #[serde(rename = "songName")]
    pub song_name: String,
    pub artist: String,
    pub pack: String,
    pub length: String,
    pub hash: String,
    #[serde(rename = "timingData")]
    pub timing_data: Vec<ArrowCloudTimingDatum>,
    pub difficulty: u32,
    pub stepartist: String,
    pub radar: ArrowCloudRadar,
    #[serde(rename = "judgmentCounts")]
    pub judgment_counts: ArrowCloudJudgmentCounts,
    #[serde(rename = "npsInfo")]
    pub nps_info: ArrowCloudNpsInfo,
    #[serde(rename = "lifebarInfo")]
    pub lifebar_info: Vec<ArrowCloudLifePoint>,
    pub modifiers: ArrowCloudModifiers,
    #[serde(rename = "musicRate")]
    pub music_rate: f64,
    #[serde(rename = "usedAutoplay")]
    pub used_autoplay: bool,
    pub passed: bool,
    #[serde(rename = "bodyVersion")]
    pub body_version: &'static str,
    #[serde(rename = "_arrowCloudBodyVersion")]
    pub arrow_cloud_body_version: &'static str,
    #[serde(rename = "_engineName")]
    pub engine_name: &'static str,
    #[serde(rename = "_engineVersion")]
    pub engine_version: &'static str,
}

#[derive(Debug, Clone)]
pub struct ArrowCloudPayloadParts {
    pub song_name: String,
    pub artist: String,
    pub pack: String,
    pub music_length_seconds: f32,
    pub hash: String,
    pub timing_data: Vec<ArrowCloudTimingDatum>,
    pub difficulty: u32,
    pub stepartist: String,
    pub submit_stats: ArrowCloudSubmitStats,
    pub total_holds: u32,
    pub total_mines: u32,
    pub total_rolls: u32,
    pub nps_info: ArrowCloudNpsInfo,
    pub lifebar_info: Vec<ArrowCloudLifePoint>,
    pub modifiers: ArrowCloudModifiers,
    pub music_rate: f32,
    pub used_autoplay: bool,
    pub passed: bool,
}

impl ArrowCloudPayload {
    pub const fn fill_metadata(&mut self) {
        self.body_version = ARROWCLOUD_BODY_VERSION;
        self.arrow_cloud_body_version = ARROWCLOUD_BODY_VERSION;
        self.engine_name = ARROWCLOUD_ENGINE_NAME;
        self.engine_version = deadsync_version::current_static();
    }
}

#[inline(always)]
#[must_use]
pub fn submit_music_rate(music_rate: f32) -> f64 {
    if music_rate.is_finite() && music_rate > 0.0 {
        f64::from(music_rate)
    } else {
        1.0
    }
}

#[must_use]
pub fn payload_from_parts(input: ArrowCloudPayloadParts) -> ArrowCloudPayload {
    let mut payload = ArrowCloudPayload {
        song_name: input.song_name,
        artist: input.artist,
        pack: input.pack.trim().to_string(),
        length: format_length(input.music_length_seconds),
        hash: input.hash,
        timing_data: input.timing_data,
        difficulty: input.difficulty,
        stepartist: input.stepartist,
        radar: ArrowCloudRadar {
            holds: [input.submit_stats.holds_held, input.total_holds],
            mines: [input.submit_stats.mines_avoided, input.total_mines],
            rolls: [input.submit_stats.rolls_held, input.total_rolls],
        },
        judgment_counts: judgment_counts_from_stats(
            input.submit_stats.judgment_counts,
            input.submit_stats.window_counts,
            input.submit_stats.holds_held,
            input.total_holds,
            input.submit_stats.mines_hit,
            input.total_mines,
            input.submit_stats.rolls_held,
            input.total_rolls,
        ),
        nps_info: input.nps_info,
        lifebar_info: input.lifebar_info,
        modifiers: input.modifiers,
        music_rate: submit_music_rate(input.music_rate),
        used_autoplay: input.used_autoplay,
        passed: input.passed,
        body_version: "",
        arrow_cloud_body_version: "",
        engine_name: "",
        engine_version: "",
    };
    payload.fill_metadata();
    payload
}

pub fn payload_from_gameplay_input(input: ArrowCloudGameplayPayloadInput<'_>) -> ArrowCloudPayload {
    let first_second = input.density_first_second.min(0.0);
    let last_second = input.density_last_second.max(first_second);
    let chart_start_second = input.song_first_second;
    let scatter = build_scatter_points(
        input.notes,
        input.note_times,
        input.col_offset,
        input.cols_per_player,
        None,
    );
    let fail_time_s = input.fail_time_ns.map(song_time_ns_to_seconds);

    payload_from_parts(ArrowCloudPayloadParts {
        song_name: input.song_name,
        artist: input.artist,
        pack: input.pack_group.to_string(),
        music_length_seconds: input.music_length_seconds,
        hash: input.chart_hash,
        timing_data: timing_data_from_scatter(&scatter, fail_time_s),
        difficulty: input.difficulty,
        stepartist: input.stepartist,
        submit_stats: input.submit_stats,
        total_holds: input.total_holds,
        total_mines: input.total_mines,
        total_rolls: input.total_rolls,
        nps_info: nps_info_from_measure_data(
            input.max_nps,
            input.measure_nps,
            input.measure_seconds,
            first_second,
            last_second,
        ),
        lifebar_info: lifebar_points(
            input.life_history,
            chart_start_second,
            first_second,
            last_second,
            input.music_rate,
            ARROWCLOUD_LIFEBAR_POINTS,
        ),
        modifiers: modifiers_from_profile(input.profile),
        music_rate: input.music_rate,
        used_autoplay: input.used_autoplay,
        passed: !gameplay_run_failed(input.is_failing, input.has_fail_time),
    })
}
