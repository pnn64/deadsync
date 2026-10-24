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
) -> Result<Projection<'a>, String> {
    let mut replacements = Vec::new();
    let mut owners = BTreeMap::new();
    let mut hash_notes = BTreeMap::new();
    let mut chart = 0;
    for entry in entries {
        if entry.field_count < 5 || !playable(entry.fields[0]) {
            continue;
        }
        if let Some(width) = lanes(entry.fields[0]) {
            let kind = std::str::from_utf8(entry.fields[0])
                .map_err(|e| e.to_string())?
                .trim();
            let offset = entry.fields[0].as_ptr() as usize - data.as_ptr() as usize;
            let replacement = if width == 8 {
                b"dance-double".as_slice()
            } else {
                b"pump-double".as_slice()
            };
            replacements.push((offset..offset + entry.fields[0].len(), replacement.to_vec()));
            if kind.to_ascii_lowercase().ends_with("-routine") {
                let (notes, ownership) = merge_routine(entry.note_data, width)?;
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
        return Ok(Projection {
            bytes: Cow::Borrowed(data),
            owners,
            hash_notes,
        });
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
    Ok(Projection {
        bytes: Cow::Owned(bytes),
        owners,
        hash_notes,
    })
}

type Ownership = BTreeMap<(usize, usize), u8>;

fn merge_routine(data: &[u8], width: usize) -> Result<(Vec<u8>, Ownership), String> {
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
    let parts: Vec<_> = clean.split(|&b| b == b'&').collect();
    if parts.len() > 2 {
        return Err("Routine contains more than two participants".into());
    }
    let mut cells = BTreeMap::<(usize, usize), (u8, u8)>::new();
    let mut measure_count = 0;
    for (owner, part) in parts.iter().enumerate() {
        for (measure, body) in part.split(|&b| b == b',').enumerate() {
            let rows: Vec<_> = body
                .split(|&b| b == b'\n')
                .map(|line| line.trim_ascii())
                .filter(|line| !line.is_empty())
                .collect();
            if rows.is_empty() {
                continue;
            }
            measure_count = measure_count.max(measure + 1);
            for (index, line) in rows.iter().enumerate() {
                // Match StepMania's 48 rows per beat, including unusual grids.
                let row = measure * 192 + (index * 192 + rows.len() / 2) / rows.len();
                let mut cursor = 0;
                for col in 0..width {
                    let Some(&symbol) = line.get(cursor) else {
                        break;
                    };
                    cursor += 1;
                    if line.get(cursor) == Some(&b'[') {
                        cursor += 1;
                        while cursor < line.len() && line[cursor] != b']' {
                            cursor += 1;
                        }
                        if cursor < line.len() {
                            cursor += 1;
                        }
                    }
                    if symbol == b'0' {
                        continue;
                    }
                    if cells.insert((row, col), (symbol, owner as u8)).is_some() {
                        return Err(format!("Routine notes collide at row {row}, column {col}"));
                    }
                }
            }
        }
    }
    // A participant cannot place a note within the other's active hold.
    let mut active = vec![None; width];
    let mut owners = Ownership::new();
    for (&(row, col), &(symbol, owner)) in &cells {
        if let Some(held_by) = active[col] {
            if held_by != owner {
                return Err(format!(
                    "Routine hold overlaps another participant at row {row}, column {col}"
                ));
            }
        }
        active[col] = if matches!(symbol, b'2' | b'4') {
            Some(owner)
        } else {
            None
        };
        owners.insert((row, col), owner);
    }
    let mut output = Vec::new();
    for measure in 0..measure_count {
        if measure > 0 {
            output.extend_from_slice(b",\n");
        }
        for row in measure * 192..(measure + 1) * 192 {
            for col in 0..width {
                output.push(cells.get(&(row, col)).map_or(b'0', |&(symbol, _)| symbol));
            }
            output.push(b'\n');
        }
    }
    Ok((output, owners))
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
        )
        .unwrap();
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

    #[test]
    fn routine_rejects_cross_player_hold_overlap() {
        assert!(merge_routine(b"20000000\n30000000\n&\n00000000\n10000000", 8).is_err());
    }

    #[test]
    fn routine_comments_do_not_create_participants() {
        assert!(merge_routine(b"10000000 // &\n&\n00000001", 8).is_ok());
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
    fn routine_keysound_digits_do_not_become_notes() {
        let (notes, owners) = merge_routine(b"1[24]0000000\n&\n00000001[23]", 8).unwrap();
        assert_eq!(owners.len(), 2);
        assert_eq!(&notes[..8], b"10000001");
    }
}
