use super::*;
use deadsync_rules::timing::{TimingData, TimingSegments};

pub(super) fn model(bpms: [f32; 2], timing: Option<&TimingData>) -> TimingData {
    timing.cloned().unwrap_or_else(|| {
        TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, bpms[0].max(0.001))],
                ..Default::default()
            },
            &[],
        )
    })
}

#[derive(Clone)]
struct Timing {
    name: &'static str,
    model: TimingData,
    points: Arc<[(f32, f32)]>,
}
pub(super) fn create(
    call: &mut HostCall<'_>,
    bpms: [f32; 2],
    timing: Option<&TimingData>,
    rate: TableId,
) -> Result<TableId, Error> {
    let model = model(bpms, timing);
    let points: Arc<[(f32, f32)]> = model.bpm_segments().collect();
    object(call, |call, table| {
        for name in [
            "GetActualBPM",
            "GetBPMs",
            "GetBPMsAndTimes",
            "HasBPMChanges",
            "GetBPMAtBeat",
            "GetElapsedTimeFromBeat",
            "GetBeatFromElapsedTime",
        ] {
            let callback = call.new_host_function_with_data(
                query,
                &[Value::Table(rate), call.captures()[1]],
                Timing {
                    name,
                    model: model.clone(),
                    points: points.clone(),
                },
            )?;
            set(table, name, callback, call)?;
        }
        Ok(())
    })
}

pub(super) fn numeric_result(
    call: &mut HostCall<'_>,
    value: Value,
    tonumber: Value,
) -> Result<f32, Error> {
    let value = call
        .call(tonumber, &[value])?
        .first()
        .copied()
        .unwrap_or(Value::Nil);
    match value {
        Value::Number(value) => Ok(value as f32),
        _ => Err(call.error(b"number expected")),
    }
}

fn query(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call
        .function_data::<Timing>()
        .expect("timing binding")
        .clone();
    let value = match data.name {
        "HasBPMChanges" => Value::Boolean(data.model.has_bpm_changes()),
        "GetActualBPM" => {
            let range = data
                .points
                .iter()
                .fold([f32::MAX, 0.0_f32], |[min, max], &(_, bpm)| {
                    [min.min(bpm), max.max(bpm)]
                });
            Value::Table(array(call, range)?)
        }
        "GetBPMs" => Value::Table(array(call, data.points.iter().map(|&(_, bpm)| bpm))?),
        "GetBPMsAndTimes" => {
            let numeric = !matches!(arg(call, 0), Value::Nil | Value::Boolean(false));
            Value::Table(object(call, |call, table| {
                for (i, &(beat, bpm)) in data.points.iter().enumerate() {
                    if numeric {
                        let point = array(call, [beat, bpm])?;
                        set(table, i + 1, point, call)?;
                    } else {
                        set(table, i + 1, format!("{beat:.6}={bpm:.6}"), call)?;
                    }
                }
                Ok(())
            })?)
        }
        _ => {
            let value = float(call, arg(call, 0))
                .ok_or_else(|| call.error(format!("{} requires a number", data.name).as_bytes()))?;
            let result = if data.name == "GetBPMAtBeat" {
                data.model
                    .get_bpm_for_beat(deadsync_core::timing::note_row_to_beat(
                        deadsync_core::timing::beat_to_note_row(value),
                    ))
            } else {
                let owner = table(call, call.captures()[0])?;
                let rate = raw(call, owner, "__songlua_music_rate")?;
                let rate = numeric_result(call, rate, call.captures()[1])?;
                if data.name == "GetElapsedTimeFromBeat" {
                    data.model.get_song_time_for_beat(value, rate)
                } else {
                    data.model.get_song_position_with_rate(value, rate).beat
                }
            };
            Value::Number(result as f64)
        }
    };
    call.push(value)
}
