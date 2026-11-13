use super::*;
use deadsync_rules::timing::TimingData;

pub(super) fn steps(
    call: &mut HostCall<'_>,
    difficulty: SongLuaDifficulty,
    bpms: [f32; 2],
    song_dir: &Path,
    kind: &str,
    model: Option<&TimingData>,
    rate: TableId,
) -> Result<TableId, Error> {
    object(call, |call, steps| {
        method(call, steps, "IsAutogen", false)?;
        method(call, steps, "GetDifficulty", difficulty.sm_name())?;
        method(call, steps, "GetStepsType", kind)?;
        for name in [
            "GetDescription",
            "GetChartName",
            "GetAuthorCredit",
            "GetCredit",
        ] {
            method(call, steps, name, "")?;
        }
        method(
            call,
            steps,
            "GetFilename",
            crate::song_simfile_path(song_dir)
                .as_deref()
                .map(crate::file_path_string)
                .unwrap_or_default(),
        )?;
        method(
            call,
            steps,
            "GetMusicPath",
            crate::song_music_path(song_dir)
                .as_deref()
                .map(crate::file_path_string)
                .unwrap_or_default(),
        )?;
        method(call, steps, "GetMeter", difficulty.meter())?;
        let timing = timing::create(call, bpms, model, rate)?;
        method(call, steps, "GetTimingData", timing)?;
        let bpms_table = array(call, bpms)?;
        method(call, steps, "GetDisplayBpms", bpms_table)?;
        let radar = radar(call)?;
        method(call, steps, "GetRadarValues", radar)?;
        method(call, steps, "GetDisplayBPMType", "DISPLAY_BPM_ACTUAL")?;
        density_methods(call, steps, Arc::from([]), Arc::from([]))?;
        cue_method(call, steps, Arc::from([]), Arc::from([]), kind)
    })
}
pub(super) fn all_steps(
    call: &mut HostCall<'_>,
    context: &SongLuaCompileContext,
    rate: TableId,
) -> Result<TableId, Error> {
    object(call, |call, all| {
        for (index, metadata) in context.song_steps.iter().enumerate() {
            let player = context
                .player_step_indices
                .iter()
                .position(|selected| *selected == Some(index));
            let bpms = player.map_or(context.song_display_bpms, |player| {
                context.players[player].display_bpms
            });
            let timing = context
                .song_step_timing
                .get(index)
                .or_else(|| player.and_then(|player| context.player_timing[player].as_ref()))
                .or(context.song_timing.as_ref());
            let steps = steps(
                call,
                metadata.difficulty,
                bpms,
                &context.song_dir,
                &metadata.steps_type,
                timing,
                rate,
            )?;
            set(all, index + 1, steps, call)?;
            if let Some(notes) = context.song_step_notes.get(index) {
                let row_beats = context
                    .song_step_row_beats
                    .get(index)
                    .cloned()
                    .unwrap_or_default();
                install_density(
                    call,
                    steps,
                    notes,
                    &row_beats,
                    &metadata.steps_type,
                    timing,
                    bpms,
                )?;
                cue_method(call, steps, notes.clone(), row_beats, &metadata.steps_type)?;
            }
            for (name, value) in [
                ("GetDescription", &metadata.description),
                ("GetChartName", &metadata.chart_name),
                ("GetAuthorCredit", &metadata.credit),
                ("GetCredit", &metadata.credit),
            ] {
                method(call, steps, name, value.as_str())?;
            }
            method(call, steps, "GetMeter", metadata.meter as f64)?;
        }
        Ok(())
    })
}
fn density_player(call: &mut HostCall<'_>) -> Result<(), Error> {
    let value = arg(call, 0);
    let valid = match value {
        Value::Nil => true,
        Value::Number(n) => (0.0..2.0).contains(&n) && n.fract() == 0.0,
        Value::String(id) => call.string_bytes(id).is_some_and(|s| {
            [
                b"P1".as_slice(),
                b"P2",
                b"PlayerNumber_P1",
                b"PlayerNumber_P2",
            ]
            .iter()
            .any(|v| s.eq_ignore_ascii_case(v))
        }),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(call.error(b"invalid PlayerNumber for Steps density"))
    }
}
#[derive(Clone)]
enum Density {
    Counts(Arc<[u32]>),
    Nps(Arc<[f32]>),
    Peak(f32),
}
fn density_methods(
    call: &mut HostCall<'_>,
    steps: TableId,
    counts: Arc<[u32]>,
    densities: Arc<[f32]>,
) -> Result<(), Error> {
    let peak = densities.iter().copied().fold(0.0_f32, f32::max);
    for (name, data) in [
        ("GetNotesPerMeasure", Density::Counts(counts)),
        ("GetNpsPerMeasure", Density::Nps(densities)),
        ("GetPeakNps", Density::Peak(peak)),
    ] {
        let callback = call.new_host_function_with_data(density, &[], data)?;
        set(steps, name, callback, call)?;
    }
    Ok(())
}
fn density(call: &mut HostCall<'_>) -> Result<(), Error> {
    density_player(call)?;
    let value = match call.function_data::<Density>().expect("density").clone() {
        Density::Counts(values) => Value::Table(array(call, values.iter().map(|&n| n as f64))?),
        Density::Nps(values) => Value::Table(array(call, values.iter().copied())?),
        Density::Peak(value) => Value::Number(value as f64),
    };
    call.push(value)
}
fn install_density(
    call: &mut HostCall<'_>,
    steps: TableId,
    notes: &[u8],
    row_beats: &[f32],
    kind: &str,
    timing_model: Option<&TimingData>,
    bpms: [f32; 2],
) -> Result<(), Error> {
    use deadsync_core::note::NoteType;
    let lanes = notes
        .split(|&ch| ch == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .find(|line| !line.is_empty() && *line != b",")
        .map_or_else(|| deadsync_chart::notes::step_type_lanes(kind), <[u8]>::len);
    let parsed = deadsync_chart::notes::parse_chart_notes(notes, lanes);
    let beat = |index: usize| row_beats.get(index).copied();
    let measure = |beat: f32| {
        timing_model.map_or((beat / 4.0).floor().max(0.0) as usize, |timing| {
            timing.measure_index_at_beat(beat)
        })
    };
    let missing = b"missing chart row beat for cached measure density";
    let last = match parsed
        .iter()
        .map(|note| note.tail_row_index.unwrap_or(note.row_index))
        .max()
    {
        Some(index) => beat(index).ok_or_else(|| call.error(missing))?,
        None => 0.0,
    };
    let mut radar_values = [0_u32; 4];
    for note in &parsed {
        let beat = beat(note.row_index).ok_or_else(|| call.error(missing))?;
        if timing_model.is_some_and(|timing| !timing.is_judgable_at_beat(beat)) {
            continue;
        }
        match note.note_type {
            NoteType::Tap | NoteType::Lift => radar_values[0] += 1,
            NoteType::Hold => {
                radar_values[0] += 1;
                radar_values[1] += 1;
            }
            NoteType::Roll => {
                radar_values[0] += 1;
                radar_values[3] += 1;
            }
            NoteType::Mine => radar_values[2] += 1,
            _ => (),
        }
    }
    let radar = object(call, |call, radar| {
        let callback = call.new_host_function_with_data(radar_value, &[], radar_values)?;
        set(radar, "GetValue", callback, call)
    })?;
    method(call, steps, "GetRadarValues", radar)?;
    let mut rows = std::collections::BTreeMap::<usize, bool>::new();
    for note in &parsed {
        let active = matches!(
            note.note_type,
            NoteType::Tap | NoteType::Hold | NoteType::Roll
        );
        rows.entry(note.row_index)
            .and_modify(|row| *row |= active)
            .or_insert(active);
    }
    let final_row = rows.last_key_value().map(|(&row, _)| row);
    let mut counts = vec![0_u32; measure(last) + 1];
    for (&row, &active) in &rows {
        if !active {
            continue;
        }
        let beat = beat(row).ok_or_else(|| call.error(missing))?;
        if Some(row) != final_row
            && timing_model
                .is_some_and(|timing| timing.is_fake_at_beat(beat) || timing.is_warp_at_beat(beat))
        {
            continue;
        }
        counts[measure(beat)] += 1;
    }
    // These freshly constructed timing getters cannot yet be overridden by Lua.
    let timing = timing::model(bpms, timing_model);
    let densities: Arc<[f32]> = counts
        .iter()
        .enumerate()
        .map(|(index, &count)| {
            let duration = timing.get_song_time_for_beat((4 * (index + 1)) as f32, 1.0)
                - timing.get_song_time_for_beat((4 * index) as f32, 1.0);
            if count == 0 || duration < 0.12 {
                0.0
            } else {
                count as f32 / duration
            }
        })
        .collect();
    density_methods(call, steps, counts.into(), densities)
}
fn radar_value(call: &mut HostCall<'_>) -> Result<(), Error> {
    let name = text(call, arg(call, 0)).unwrap_or_default();
    let values = call.function_data::<[u32; 4]>().expect("radar values");
    let value = match name.strip_prefix("RadarCategory_").unwrap_or(&name) {
        "Notes" => values[0],
        "Holds" => values[1],
        "Mines" => values[2],
        "Rolls" => values[3],
        _ => 0,
    };
    call.push(Value::Number(value as f64))
}
#[derive(Clone)]
struct Cues {
    notes: Arc<[u8]>,
    beats: Arc<[f32]>,
    lanes: usize,
}
fn cue_method(
    call: &mut HostCall<'_>,
    steps: TableId,
    notes: Arc<[u8]>,
    beats: Arc<[f32]>,
    kind: &str,
) -> Result<(), Error> {
    let lanes =
        deadsync_chart::notes::step_type_lanes(kind.strip_prefix("StepsType_").unwrap_or(kind));
    let callback = call.new_host_function_with_data(
        cues,
        &[call.captures()[1]],
        Cues {
            notes,
            beats,
            lanes,
        },
    )?;
    set(steps, "GetColumnCues", callback, call)
}
fn cues(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Cues>().expect("column cues").clone();
    let minimum = float(call, arg(call, 0)).unwrap_or(1.5);
    let steps = call.arguments().first().copied().unwrap_or(Value::Nil);
    table(call, steps)?;
    let getter = get(call, steps, "GetTimingData")?;
    if !function(getter) {
        return Err(call.error(b"Steps timing getter must be a function"));
    }
    let timing = call
        .call(getter, &[steps])?
        .first()
        .copied()
        .unwrap_or(Value::Nil);
    table(call, timing)?;
    call.with_roots(&[timing], |call| {
        let elapsed = get(call, timing, "GetElapsedTimeFromBeat")?;
        if !function(elapsed) {
            return Err(call.error(b"elapsed time getter must be a function"));
        }
        call.with_roots(&[elapsed], |call| {
            let notes = deadsync_chart::notes::parse_chart_notes(&data.notes, data.lanes);
            let cues = object(call, |call, cues| {
                let mut previous = 0.0_f32;
                let mut cursor = 0;
                let mut count = 0;
                while cursor < notes.len() {
                    let row = notes[cursor].row_index;
                    let beat = data
                        .beats
                        .get(row)
                        .copied()
                        .ok_or_else(|| call.error(b"missing chart row beat for column cue"))?;
                    let time = call
                        .call(elapsed, &[timing, Value::Number(beat as f64)])?
                        .first()
                        .copied()
                        .unwrap_or(Value::Nil);
                    let time = timing::numeric_result(call, time, call.captures()[0])?;
                    let duration = time - previous;
                    let emit = duration > minimum || previous == 0.0;
                    let columns = emit.then(|| call.try_new_table()).transpose()?;
                    let mut column_count = 0;
                    while cursor < notes.len() && notes[cursor].row_index == row {
                        use deadsync_core::note::NoteType;
                        let note = &notes[cursor];
                        let kind = match note.note_type {
                            NoteType::Tap => Some(1),
                            NoteType::Hold | NoteType::Roll => Some(2),
                            NoteType::Mine => Some(4),
                            NoteType::Lift => Some(5),
                            NoteType::Fake => None,
                        };
                        if emit && let Some(kind) = kind {
                            let column = object(call, |call, column| {
                                set(column, "colNum", note.column + 1, call)?;
                                set(column, "noteType", kind, call)
                            })?;
                            column_count += 1;
                            set(
                                columns.expect("emitted row owns columns"),
                                column_count,
                                column,
                                call,
                            )?;
                        }
                        cursor += 1;
                    }
                    if emit {
                        let cue = object(call, |call, cue| {
                            set(cue, "startTime", previous, call)?;
                            set(cue, "duration", duration, call)?;
                            set(
                                cue,
                                "columns",
                                columns.expect("emitted row owns columns"),
                                call,
                            )
                        })?;
                        count += 1;
                        set(cues, count, cue, call)?;
                    }
                    previous = time;
                }
                Ok(())
            })?;
            call.push(Value::Table(cues))
        })
    })
}
