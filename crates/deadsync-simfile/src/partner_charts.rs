//! Preserve DeadSync's score identity for the native partner styles RSSP parses.

pub(crate) fn is_partner(kind: &[u8]) -> bool {
    let kind = kind.trim_ascii();
    [b"dance-couple".as_slice(), b"pump-couple", b"dance-routine", b"pump-routine"]
        .iter()
        .any(|stepstype| kind.eq_ignore_ascii_case(stepstype))
}

#[cfg(test)]
mod tests {
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
    fn routine_score_ids_preserve_legacy_participant_hashes() {
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
        assert_eq!(hashes, ["ef3857717e6ef16f", "4d356df688be6f27"]);
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

}
