//! Adapt partner charts to RSSP's double-field analyzer without losing native
//! chart types or Routine ownership. Runtime uses the authored chart identity.
use std::borrow::Cow;
use std::collections::BTreeMap;

pub(crate) fn lanes(kind: &[u8]) -> Option<usize> {
    let kind = std::str::from_utf8(kind).ok()?.trim();
    match kind.to_ascii_lowercase().as_str() {
        "dance-couple" | "dance-routine" => Some(8),
        "pump-couple" | "pump-routine" => Some(10),
        _ => None,
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
            replacements.push((offset..offset + entry.fields[0].len(), replacement.to_vec()));
            if entry.fields[0]
                .trim_ascii()
                .to_ascii_lowercase()
                .ends_with(b"-routine")
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
                replacements.push((offset..offset + entry.note_data.len(), notes));
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
    let measures: Vec<_> = part
        .split(|&b| b == b',')
        .filter(|m| !m.is_empty())
        .collect();
    for (measure, body) in measures.iter().enumerate() {
        let lines: Vec<_> = body
            .split(|&b| b == b'\n')
            .map(trim_line)
            .filter(|line| !line.is_empty())
            .collect();
        for (index, line) in lines.iter().enumerate() {
            let row = note_row(measure, index, lines.len());
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
    (tracks, measures.len())
}

/// Resolve overlaps as NoteDataUtil::CombineCompositeNoteData does, so a
/// partner collision never rejects the chart: the second participant replaces
/// a note on the same row and column, notes inside an earlier hold are skipped,
/// and a hold absorbs what it covers. Only the first two participants are read.
fn merge_routine(data: &[u8], width: usize) -> (Vec<u8>, Ownership) {
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
    let mut cells = BTreeMap::new();
    for (col, track) in combined.iter().enumerate() {
        for (&row, note) in track {
            owners.insert((row, col), note.owner);
            cells.insert((row, col), note.symbol);
            if is_head(note) {
                // The tail keeps its owner so each participant's hash sees it.
                owners.insert((row + note.duration, col), note.owner);
                cells.insert((row + note.duration, col), b'3');
            }
        }
    }
    if let Some(&(row, _)) = cells.keys().next_back() {
        measure_count = measure_count.max(row / 192 + 1);
    }
    let mut output = Vec::new();
    for measure in 0..measure_count {
        if measure > 0 {
            output.extend_from_slice(b",\n");
        }
        for row in measure * 192..(measure + 1) * 192 {
            for col in 0..width {
                output.push(cells.get(&(row, col)).copied().unwrap_or(b'0'));
            }
            output.push(b'\n');
        }
    }
    (output, owners)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routine_participant_survives_the_binary_cache_roundtrip() {
        use crate::cache::CachedParsedNote;
        use deadsync_chart::notes::ParsedNote;
        let note = ParsedNote {
            player: 1,
            row_index: 12,
            column: 7,
            note_type: deadsync_core::note::NoteType::Hold,
            tail_row_index: Some(24),
        };
        let bytes =
            bincode::encode_to_vec(CachedParsedNote::from(&note), bincode::config::standard())
                .unwrap();
        let (cached, _) =
            bincode::decode_from_slice::<CachedParsedNote, _>(&bytes, bincode::config::standard())
                .unwrap();
        assert_eq!(ParsedNote::from(cached), note);
    }

    #[test]
    fn routine_merges_different_grids_and_preserves_owners() {
        let (notes, owners) = merge_routine(
            b"10000000\n00000000\n&\n00000000\n00000001\n00000000\n00000000",
            8,
        );
        assert_eq!(owners.get(&(0, 0)), Some(&0));
        assert_eq!(owners.get(&(48, 7)), Some(&1));
        assert_eq!(
            notes
                .split(|&b| b == b'\n')
                .filter(|s| !s.is_empty())
                .count(),
            192
        );
    }

    fn symbol(notes: &[u8], row: usize, col: usize) -> u8 {
        notes
            .split(|&b| b == b'\n')
            .filter(|line| *line != b",")
            .nth(row)
            .unwrap()[col]
    }

    #[test]
    fn routine_skips_partner_note_inside_an_earlier_hold() {
        let (notes, owners) = merge_routine(b"20000000\n30000000\n&\n00000000\n10000000", 8);
        assert_eq!(owners.len(), 2);
        assert_eq!(owners.get(&(0, 0)), Some(&0));
        assert_eq!(owners.get(&(96, 0)), Some(&0));
        assert_eq!(symbol(&notes, 0, 0), b'2');
        assert_eq!(symbol(&notes, 96, 0), b'3');
    }

    #[test]
    fn routine_second_participant_replaces_a_same_row_note() {
        let (notes, owners) = merge_routine(b"M0000000\n&\n10000000", 8);
        assert_eq!(owners.len(), 1);
        assert_eq!(owners.get(&(0, 0)), Some(&1));
        assert_eq!(symbol(&notes, 0, 0), b'1');
    }

    #[test]
    fn routine_hold_absorbs_covered_and_adjacent_partner_notes() {
        // The second hold covers the first participant's tap and the hold on its tail.
        let (notes, owners) = merge_routine(
            b"00000000\n10000000\n20000000\n00000000,\n30000000\n00000000\n00000000\n00000000\n&\n20000000\n00000000\n30000000\n00000000",
            8,
        );
        assert_eq!(owners.len(), 2);
        assert_eq!(owners.get(&(0, 0)), Some(&1));
        assert_eq!(owners.get(&(192, 0)), Some(&1));
        assert_eq!(symbol(&notes, 0, 0), b'2');
        assert_eq!(symbol(&notes, 96, 0), b'0');
        assert_eq!(symbol(&notes, 192, 0), b'3');
    }

    #[test]
    fn routine_ignores_participants_after_the_second() {
        let (_, owners) = merge_routine(b"10000000\n&\n01000000\n&\n00100000", 8);
        assert_eq!(owners.len(), 2);
        assert!(!owners.contains_key(&(0, 2)));
    }

    #[test]
    fn routine_rows_follow_stepmania_measures_and_rounding() {
        // Empty measures are skipped; a 128th row rounds half to even.
        let mut part = "00000000\n".repeat(128);
        part.replace_range(27..28, "1");
        let (_, owners) = merge_routine(format!(",,{part}&\n00000001").as_bytes(), 8);
        assert_eq!(owners.get(&(4, 0)), Some(&0));
        assert_eq!(owners.get(&(0, 7)), Some(&1));
    }

    #[test]
    fn routine_comments_do_not_create_participants() {
        let (_, owners) = merge_routine(b"10000000 // &\n&\n00000001", 8);
        assert_eq!(owners.get(&(0, 7)), Some(&1));
    }

    #[test]
    fn routine_collision_keeps_the_song_and_its_single_chart() {
        use crate::song::{ParseSongOptions, parse_song_bytes};
        use std::path::Path;
        let options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
        let source = "#TITLE:Partners;#BPMS:0=120;\
            #NOTES:dance-single::Easy:2:0,0,0,0,0:\n1000\n0100\n0010\n0001;\
            #NOTES:dance-routine::Hard:7:0,0,0,0,0:\n10000000\n00000001\n&\n10000000\n00000000;";
        let song = parse_song_bytes(
            source.as_bytes(),
            "sm",
            Path::new("partners.sm"),
            &options,
            |_| 4.0,
        )
        .unwrap();
        assert_eq!(song.charts.len(), 2);
        let single = &song.charts[0];
        assert_eq!(single.chart_type, "dance-single");
        assert_eq!(single.parsed_notes.len(), 4);
        let routine = &song.charts[1];
        assert_eq!(routine.chart_type, "dance-routine");
        let owners: Vec<_> = routine
            .parsed_notes
            .iter()
            .map(|note| (note.column, note.player))
            .collect();
        assert_eq!(owners, [(0, 1), (7, 0)]);
    }

    #[test]
    fn partner_charts_load_from_sm_and_ssc_with_native_types_and_ownership() {
        use crate::song::{ParseSongOptions, parse_song_bytes};
        use std::path::Path;
        let options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
        for (kind, width) in [
            ("dance-couple", 8),
            ("dance-routine", 8),
            ("pump-couple", 10),
            ("pump-routine", 10),
        ] {
            let first = format!("1{}", "0".repeat(width - 1));
            let last = format!("{}1", "0".repeat(width - 1));
            let separator = if kind.ends_with("routine") {
                "\n&\n"
            } else {
                "\n"
            };
            let notes = format!("{first}{separator}{last}");
            for extension in ["sm", "ssc"] {
                let chart = if extension == "sm" {
                    format!("#NOTES:{kind}:Partner:Hard:7:0,0,0,0,0:\n{notes};")
                } else {
                    format!(
                        "#NOTEDATA:;#STEPSTYPE:{kind};#DESCRIPTION:Partner;#DIFFICULTY:Hard;#METER:7;#NOTES:\n{notes};"
                    )
                };
                let source = format!("#VERSION:0.83;#TITLE:Partners;#BPMS:0=120;{chart}");
                let song = parse_song_bytes(
                    source.as_bytes(),
                    extension,
                    Path::new("partners.sm"),
                    &options,
                    |_| 4.0,
                )
                .unwrap();
                assert_eq!(song.charts.len(), 1, "{kind} {extension}");
                let chart = &song.charts[0];
                assert_eq!(chart.chart_type, kind);
                assert_eq!(chart.parsed_notes.len(), 2);
                assert_eq!(chart.parsed_notes[0].column, 0);
                assert_eq!(chart.parsed_notes[1].column as usize, width - 1);
                if kind.ends_with("routine") {
                    assert_eq!(chart.parsed_notes[0].player, 0);
                    assert_eq!(chart.parsed_notes[1].player, 1);
                    assert_eq!(
                        chart.parsed_notes[0].row_index,
                        chart.parsed_notes[1].row_index
                    );
                }
            }
        }
    }

    #[test]
    fn routine_hash_distinguishes_ownership_with_identical_shared_field() {
        use crate::song::{ParseSongOptions, parse_song_bytes};
        use std::path::Path;
        let options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
        let hashes: Vec<_> = ["10000000\n&\n00000001", "00000001\n&\n10000000"]
            .into_iter()
            .map(|body| {
                let source = format!(
                    "#TITLE:Partners;#BPMS:0=120;#NOTES:dance-routine::Hard:7:0,0,0,0,0:\n{body};"
                );
                parse_song_bytes(
                    source.as_bytes(),
                    "sm",
                    Path::new("partners.sm"),
                    &options,
                    |_| 4.0,
                )
                .unwrap()
                .charts[0]
                    .short_hash
                    .clone()
            })
            .collect();
        assert_ne!(hashes[0], hashes[1]);
    }

    #[test]
    fn routine_hash_distinguishes_hold_lengths() {
        use crate::song::{ParseSongOptions, parse_song_bytes};
        use std::path::Path;
        let options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
        let hashes: Vec<_> = [
            "20000000\n30000000\n00000000\n00000000\n&\n00000001",
            "20000000\n00000000\n30000000\n00000000\n&\n00000001",
        ]
        .into_iter()
        .map(|body| {
            let source = format!(
                "#TITLE:Partners;#BPMS:0=120;#NOTES:dance-routine::Hard:7:0,0,0,0,0:\n{body};"
            );
            parse_song_bytes(
                source.as_bytes(),
                "sm",
                Path::new("partners.sm"),
                &options,
                |_| 4.0,
            )
            .unwrap()
            .charts[0]
                .short_hash
                .clone()
        })
        .collect();
        assert_ne!(hashes[0], hashes[1]);
    }

    #[test]
    fn routine_keysound_digits_do_not_become_notes() {
        let (notes, owners) = merge_routine(b"1[24]0000000\n&\n00000001[23]", 8);
        assert_eq!(owners.len(), 2);
        assert_eq!(&notes[..8], b"10000001");
    }
}
