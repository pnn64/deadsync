//! Adapt partner charts to RSSP's double-field analyzer without losing native
//! chart types or Routine ownership. Runtime uses the authored chart identity.
use std::borrow::Cow;
use std::collections::BTreeMap;

pub(crate) fn lanes(kind: &[u8]) -> Option<usize> {
    let kind = std::str::from_utf8(kind).ok()?.trim();
    if kind.eq_ignore_ascii_case("dance-couple") || kind.eq_ignore_ascii_case("dance-routine") {
        Some(8)
    } else if kind.eq_ignore_ascii_case("pump-couple") || kind.eq_ignore_ascii_case("pump-routine")
    {
        Some(10)
    } else {
        None
    }
}

pub(crate) fn playable(kind: &[u8]) -> bool {
    rssp::supported_stepstype_lanes_bytes(kind).is_some() || lanes(kind).is_some()
}

pub(crate) struct Projection<'a> {
    pub bytes: Cow<'a, [u8]>,
    /// Chart ordinal, StepMania row (48 per beat), column -> participant.
    pub owners: BTreeMap<(usize, usize, usize), u8>,
    pub hash_notes: BTreeMap<usize, Vec<u8>>,
}

pub(crate) fn project<'a>(
    data: &'a [u8],
    entries: &[rssp::parse::ParsedChartEntry<'_>],
) -> Projection<'a> {
    let mut replacements = Vec::new();
    let mut owners = BTreeMap::new();
    let mut hash_notes = BTreeMap::new();
    let mut chart = 0;
    for entry in entries {
        if entry.field_count < 5 || !playable(entry.fields[0]) {
            continue;
        }
        if let Some(width) = lanes(entry.fields[0]) {
            let offset = entry.fields[0].as_ptr() as usize - data.as_ptr() as usize;
            let replacement = if width == 8 {
                b"dance-double".as_slice()
            } else {
                b"pump-double".as_slice()
            };
            replacements.push((
                offset..offset + entry.fields[0].len(),
                Cow::Borrowed(replacement),
            ));
            let kind = entry.fields[0].trim_ascii();
            if kind
                .get(kind.len().saturating_sub(8)..)
                .is_some_and(|suffix| suffix.eq_ignore_ascii_case(b"-routine"))
            {
                let (notes, ownership) = merge_routine(entry.note_data, width);
                let mut composite = Vec::new();
                for participant in 0..2 {
                    if participant > 0 {
                        composite.extend_from_slice(b"&\n");
                    }
                    let mut part = Vec::new();
                    let mut row = 0;
                    for line in notes.split(|&b| b == b'\n') {
                        if line == b"," {
                            part.extend_from_slice(b",\n");
                        } else if line.len() == width {
                            for (col, &symbol) in line.iter().enumerate() {
                                part.push(if ownership.get(&(row, col)) == Some(&participant) {
                                    symbol
                                } else {
                                    b'0'
                                });
                            }
                            part.push(b'\n');
                            row += 1;
                        }
                    }
                    composite
                        .extend_from_slice(&rssp::stats::minimize_chart_for_hash(&part, width));
                }
                hash_notes.insert(chart, composite);
                owners.extend(
                    ownership
                        .into_iter()
                        .map(|((row, col), owner)| ((chart, row, col), owner)),
                );
                let offset = entry.note_data.as_ptr() as usize - data.as_ptr() as usize;
                replacements.push((offset..offset + entry.note_data.len(), Cow::Owned(notes)));
            }
        }
        chart += 1;
    }
    if replacements.is_empty() {
        return Projection {
            bytes: Cow::Borrowed(data),
            owners,
            hash_notes,
        };
    }
    replacements.sort_unstable_by_key(|(range, _)| range.start);
    let mut bytes = Vec::with_capacity(data.len());
    let mut cursor = 0;
    for (range, replacement) in replacements {
        bytes.extend_from_slice(&data[cursor..range.start]);
        bytes.extend_from_slice(&replacement);
        cursor = range.end;
    }
    bytes.extend_from_slice(&data[cursor..]);
    Projection {
        bytes: Cow::Owned(bytes),
        owners,
        hash_notes,
    }
}

type Ownership = BTreeMap<(usize, usize), u8>;

/// StepMania's NoteData::MAX_NOTE_ROW, the length of a hold awaiting its tail.
const OPEN_HOLD: usize = 1 << 30;

#[derive(Clone, Copy)]
struct TapNote {
    symbol: u8,
    duration: usize,
    owner: u8,
}

type Track = BTreeMap<usize, TapNote>;

fn is_head(note: &TapNote) -> bool {
    matches!(note.symbol, b'2' | b'4')
}

/// NoteData::IsHoldNoteAtRow: the head of a hold whose body covers `row`.
/// A head on `row` itself does not count, and autokeysounds are ignored.
fn hold_head_before(track: &Track, row: usize) -> Option<usize> {
    for (&head, note) in track.range(..row).rev() {
        match note.symbol {
            b'K' => continue,
            _ if is_head(note) => return (head + note.duration >= row).then_some(head),
            _ => return None,
        }
    }
    None
}

/// NoteData::AddHoldNote: absorb overlapping or adjacent holds, then clear the
/// covered rows, including a note sitting on the tail.
fn add_hold(track: &mut Track, mut start: usize, mut end: usize, mut note: TapNote) {
    let mut covered: Vec<usize> = track.range(start..end).map(|(&row, _)| row).collect();
    if let Some((&row, prev)) = track.range(..start).next_back()
        && is_head(prev)
        && row + prev.duration + 1 > start
    {
        covered.push(row);
    }
    if let Some((&row, next)) = track.range(end..).next()
        && is_head(next)
        && row == end
    {
        covered.push(row);
    }
    for row in &covered {
        let other = track[row];
        if is_head(&other) {
            start = start.min(*row);
            end = end.max(row + other.duration);
        }
    }
    for row in covered {
        track.remove(&row);
    }
    track.remove(&end);
    note.duration = end - start;
    track.insert(start, note);
}

/// StepMania's BeatToNoteRow, rounding the float beat with lrint.
fn note_row(measure: usize, line: usize, lines: usize) -> usize {
    let beat = (measure as f32 + line as f32 / lines as f32) * 4.0;
    (beat * 48.0).round_ties_even() as usize
}

fn trim_line(mut line: &[u8]) -> &[u8] {
    while let [b'\r' | b'\n' | b'\t' | b' ', rest @ ..] = line {
        line = rest;
    }
    while let [rest @ .., b'\r' | b'\n' | b'\t' | b' '] = line {
        line = rest;
    }
    line
}

/// NoteDataUtil's LoadFromSMNoteDataStringWithPlayer for one participant.
/// Returns the tracks and the number of measures read.
fn load_participant(part: &[u8], width: usize, owner: u8) -> (Vec<Track>, usize) {
    let mut tracks = vec![Track::new(); width];
    // Empty measures are skipped without advancing the measure index.
    let mut measure_count = 0;
    for (measure, body) in part
        .split(|&b| b == b',')
        .filter(|m| !m.is_empty())
        .enumerate()
    {
        measure_count = measure + 1;
        let lines = body
            .split(|&b| b == b'\n')
            .map(trim_line)
            .filter(|line| !line.is_empty());
        // The authored row grid needs the line count before reading its notes.
        let line_count = lines.clone().count();
        for (index, line) in lines.enumerate() {
            let row = note_row(measure, index, line_count);
            let mut cursor = 0;
            for track in tracks.iter_mut() {
                let Some(&symbol) = line.get(cursor) else {
                    break;
                };
                cursor += 1;
                if line.get(cursor) == Some(&b'[') {
                    while cursor < line.len() && line[cursor] != b']' {
                        cursor += 1;
                    }
                    cursor = (cursor + 1).min(line.len());
                }
                let duration = match symbol {
                    b'1' | b'M' | b'K' | b'L' | b'F' => 0,
                    b'2' | b'4' => OPEN_HOLD,
                    b'3' => {
                        // An unmatched tail is ignored.
                        if let Some(head) = hold_head_before(track, row) {
                            track.get_mut(&head).unwrap().duration = row - head;
                        }
                        continue;
                    }
                    _ => continue,
                };
                track.insert(
                    row,
                    TapNote {
                        symbol,
                        duration,
                        owner,
                    },
                );
            }
        }
    }
    // A hold that never found its tail is dropped.
    for track in &mut tracks {
        track.retain(|_, note| note.duration != OPEN_HOLD);
    }
    (tracks, measure_count)
}

/// Resolve overlaps as NoteDataUtil::CombineCompositeNoteData does, so a
/// partner collision never rejects the chart: the second participant replaces
/// a note on the same row and column, notes inside an earlier hold are skipped,
/// and a hold absorbs what it covers. Only the first two participants are read.
pub(super) fn merge_routine(data: &[u8], width: usize) -> (Vec<u8>, Ownership) {
    // Remove comments before splitting: '&' in a comment is not a participant.
    let clean: Vec<u8> = data
        .split(|&b| b == b'\n')
        .flat_map(|line| {
            let end = line
                .windows(2)
                .position(|s| s == b"//")
                .unwrap_or(line.len());
            line[..end].iter().copied().chain(std::iter::once(b'\n'))
        })
        .collect();
    let mut combined = vec![Track::new(); width];
    let mut measure_count = 0;
    for (owner, part) in clean.split(|&b| b == b'&').take(2).enumerate() {
        let (tracks, measures) = load_participant(part, width, owner as u8);
        measure_count = measure_count.max(measures);
        for (out, track) in combined.iter_mut().zip(tracks) {
            for (row, note) in track {
                if hold_head_before(out, row).is_some() {
                    continue;
                }
                if is_head(&note) {
                    add_hold(out, row, row + note.duration, note);
                } else {
                    out.insert(row, note);
                }
            }
        }
    }
    let mut owners = Ownership::new();
    for (col, track) in combined.iter().enumerate() {
        for (&row, note) in track {
            owners.insert((row, col), note.owner);
            if is_head(note) {
                // The tail keeps its owner so each participant's hash sees it.
                owners.insert((row + note.duration, col), note.owner);
            }
        }
    }
    if let Some(&(row, _)) = owners.keys().next_back() {
        measure_count = measure_count.max(row / 192 + 1);
    }
    let row_bytes = width + 1;
    let mut output =
        Vec::with_capacity(measure_count * 192 * row_bytes + measure_count.saturating_sub(1) * 2);
    for measure in 0..measure_count {
        if measure > 0 {
            output.extend_from_slice(b",\n");
        }
        let start = output.len();
        output.resize(start + 192 * row_bytes, b'0');
        for row in output[start..].chunks_exact_mut(row_bytes) {
            row[width] = b'\n';
        }
    }
    for (col, track) in combined.iter().enumerate() {
        let position = |row: usize| row * row_bytes + row / 192 * 2 + col;
        // Preserve the original cell insertion order when a head replaces a tail.
        for (&row, note) in track {
            output[position(row)] = note.symbol;
            if is_head(note) {
                output[position(row + note.duration)] = b'3';
            }
        }
    }
    (output, owners)
}
