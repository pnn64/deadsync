// Original implementations for differential parsing tests.
use super::*;

pub(super) fn note_analysis_bytes<'a>(
    data: &'a [u8],
    entries: &[rssp::parse::ParsedChartEntry<'a>],
) -> Cow<'a, [u8]> {
    let mut indices = Vec::new();
    for entry in entries {
        let Some(lanes) = rssp::supported_stepstype_lanes_bytes(entry.fields[0]) else {
            continue;
        };
        if !entry.note_data.contains(&b'[') {
            continue;
        }
        // ParsedChartEntry borrows each note block directly from these bytes.
        let offset = entry.note_data.as_ptr() as usize - data.as_ptr() as usize;
        let mut line_start = 0;
        for line in entry
            .note_data
            .split_inclusive(|byte| matches!(byte, b'\n' | b','))
        {
            let mut end = line
                .iter()
                .position(|byte| matches!(byte, b'\n' | b','))
                .unwrap_or(line.len());
            if let Some(comment) = line[..end].windows(2).position(|bytes| bytes == b"//") {
                end = comment;
            }
            while end > 0 && matches!(line[end - 1], b'\r' | b'\t' | b' ') {
                end -= 1;
            }
            let mut cursor = 0;
            while cursor < end && matches!(line[cursor], b'\r' | b'\t' | b' ') {
                cursor += 1;
            }
            // NoteDataUtil::LoadFromSMNoteDataStringWithPlayer consumes one
            // column, then skips its optional [keysound index] through ']'
            // or the row end. Its digits must never enter RSSP as notes.
            for _ in 0..lanes {
                if cursor >= end {
                    break;
                }
                cursor += 1;
                if cursor < end && line[cursor] == b'[' {
                    let start = cursor;
                    cursor += 1;
                    while cursor < end && line[cursor] != b']' {
                        cursor += 1;
                    }
                    if cursor < end {
                        cursor += 1;
                    }
                    indices.push(offset + line_start + start..offset + line_start + cursor);
                }
            }
            line_start += line.len();
        }
    }
    if indices.is_empty() {
        return Cow::Borrowed(data);
    }
    indices.sort_unstable_by_key(|range| range.start);
    let mut notes = Vec::with_capacity(data.len());
    let mut cursor = 0;
    for index in indices {
        notes.extend_from_slice(&data[cursor..index.start]);
        cursor = index.end;
    }
    notes.extend_from_slice(&data[cursor..]);
    Cow::Owned(notes)
}

pub(super) fn unplayed_chart_note_bounds(
    parsed: &rssp::parse::ParsedSimfileData<'_>,
    extension: &str,
) -> Option<[f32; 2]> {
    use rssp::timing::{TimingFormat, compute_timing_segments, steps_timing_allowed};
    let format = if extension.eq_ignore_ascii_case("ssc") {
        TimingFormat::Ssc
    } else {
        TimingFormat::Sm
    };
    let version = rssp::parse::parse_version(parsed.version, format);
    let own_timing_allowed = steps_timing_allowed(version, format);
    let mut bounds: Option<[f32; 2]> = None;
    for entry in &parsed.notes_list {
        let Ok(kind) = std::str::from_utf8(entry.fields[0]) else {
            continue;
        };
        let kind = kind.trim().to_ascii_lowercase();
        if rssp::supported_stepstype_lanes_bytes(entry.fields[0]).is_some() {
            continue;
        }
        // GameManager.cpp's native StepsTypeInfo table. Lights never contribute.
        let Some(lanes) = native_unplayed_step_lanes(&kind) else {
            continue;
        };
        if parsed.notes_list.len() > 1 && entry.fields[2].eq_ignore_ascii_case(b"Edit") {
            continue;
        }
        let Some([first, last]) = native_note_row_bounds(entry.note_data, lanes) else {
            continue;
        };
        if last == 0 {
            continue;
        }
        let own = own_timing_allowed
            && [
                entry.chart_bpms.as_deref(),
                entry.chart_stops.as_deref(),
                entry.chart_delays.as_deref(),
                entry.chart_warps.as_deref(),
                entry.chart_speeds.as_deref(),
                entry.chart_scrolls.as_deref(),
                entry.chart_fakes.as_deref(),
                entry.chart_offset.as_deref(),
                entry.chart_time_signatures.as_deref(),
                entry.chart_labels.as_deref(),
                entry.chart_tickcounts.as_deref(),
                entry.chart_combos.as_deref(),
            ]
            .iter()
            .any(Option::is_some);
        let text =
            |value: Option<&[u8]>| value.map(|value| String::from_utf8_lossy(value).into_owned());
        let tags = [
            (entry.chart_bpms.as_deref(), parsed.bpms),
            (entry.chart_stops.as_deref(), parsed.stops),
            (entry.chart_delays.as_deref(), parsed.delays),
            (entry.chart_warps.as_deref(), parsed.warps),
        ];
        let tags = tags.map(|(chart, global)| {
            (
                text(if own_timing_allowed { chart } else { None }),
                text(if own { None } else { global }).unwrap_or_default(),
            )
        });
        let segments = compute_timing_segments(
            tags[0].0.as_deref(),
            &tags[0].1,
            tags[1].0.as_deref(),
            &tags[1].1,
            tags[2].0.as_deref(),
            &tags[2].1,
            tags[3].0.as_deref(),
            &tags[3].1,
            None,
            "",
            None,
            "",
            None,
            "",
            format,
            false,
        );
        let offset = rssp::parse::parse_offset_seconds(if own_timing_allowed {
            entry.chart_offset.as_deref().or(parsed.offset)
        } else {
            parsed.offset
        }) as f32;
        let timing = deadsync_rules::timing::TimingData::from_segments(
            -offset,
            0.0,
            &crate::timing::timing_segments_from_rssp(&segments),
            &[],
        );
        let first = timing.get_time_for_beat(first as f32 / 48.0);
        let last = timing.get_time_for_beat(last as f32 / 48.0);
        if first.is_finite() && last.is_finite() {
            bounds =
                Some(bounds.map_or([first, last], |old| [old[0].min(first), old[1].max(last)]));
        }
    }
    bounds
}
