//! A song's chart preview, read from its simfile's bytes.
//!
//! The Content Browser previews a song it has not downloaded: a scrolling note
//! window over the stretch of the song its sample plays. The ITGmania build
//! asked a relay for that window; here the simfile is read out of the pack zip
//! and handed to this module, which runs it through the game's own parser and
//! the game's own timing -- the same `TimingData` gameplay judges against, so
//! a stop, a delay or a warp lands the preview's arrows exactly where the
//! song will.
//!
//! What it keeps is what the window draws, and nothing else:
//!
//! * **Dance pads only.** Singles and doubles; a pump or a keyboard chart
//!   would draw as nonsense on four or eight lanes.
//! * **One row per note row in the sample window**, timed from the window's
//!   start. Taps, hold and roll heads, lifts and mines; never a tail, never a
//!   body. Fakes are dropped -- `F` notes and every row under `#FAKES` -- and
//!   so are rows inside a warp: the game never judges them, and in a window
//!   keyed by time they would all pile onto the warp's one instant.
//! * **The chart's own clock and nothing else**: its `#OFFSET`, BPMs, stops,
//!   delays and warps, but neither the player's global offset nor ITG's 9 ms.
//!   The window is timed against the audio file, not against a machine.
//!
//! Everything here runs on the worker that fetched the bytes. It parses once,
//! touches no disk, and holds nothing after it returns.

use deadsync_chart::GameplayChartData;
use deadsync_chart::notes::ParsedNote;
use deadsync_chart::song::{STANDARD_DIFFICULTY_COUNT, standard_difficulty_index};
use deadsync_core::note::NoteType;
use deadsync_gameplay::quantization_index_from_beat;
use deadsync_rules::timing::{BeatTimeCache, TimingData};
use deadsync_simfile::cache::{CachedChartPayload, build_gameplay_chart_from_payload};
use deadsync_simfile::song::{ParseSongOptions, parse_song_bytes};
use deadsync_simfile::tags::latest_simfile_tag_values;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::path::Path;

/// A simfile bigger than this is not one worth previewing. The largest real
/// ones -- marathons carrying a dozen edits -- are a few megabytes; this is a
/// bound on what one hostile archive entry can make the parser chew through.
const MAX_SIMFILE_BYTES: usize = 16 * 1024 * 1024;
/// Rows kept per chart. A twenty-second window of a stream chart is a few
/// hundred rows; this is a bound on what a pathological one can cost to draw,
/// and the relay's own.
const MAX_ROWS_PER_CHART: usize = 900;
/// A song that never declared a sample still gets one: a little way in, which
/// is where a song has usually started doing something. The original's rule.
const FALLBACK_SAMPLE_START: f32 = 20.0;
const FALLBACK_SAMPLE_LENGTH: f32 = 20.0;
/// How far outside the window a row may land and still count as on its edge.
/// A row the simfile puts exactly on the sample's start or end should not be
/// lost to the last bit of a float; a tenth of a millisecond is far below
/// anything the window can show.
const EDGE_SLACK_SECONDS: f64 = 1e-4;
/// What RSSP names a song whose simfile has no `#TITLE`. A placeholder, not a
/// title: the preview says nothing rather than show it.
const MISSING_TITLE: &str = "<invalid-title>";

/// One row of a chart's note window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct NoteRow {
    /// seconds from the sample start
    pub time: f32,
    /// column bits, lowest = leftmost: taps, hold heads, roll heads, lifts and
    /// mines all set their bit
    pub cols: u8,
    /// Which of `cols` are lifts.
    pub lifts: u8,
    /// Which of `cols` are mines.
    pub mines: u8,
    /// 0=4th 1=8th 2=12th 3=16th 4=24th 5=32nd 6=48th 7=64th 8=finer
    pub quant: u8,
}

/// One difficulty of the song, as the window draws it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ChartPreview {
    pub doubles: bool,
    /// As the simfile names it, e.g. "Challenge". A duplicate of a standard
    /// difficulty is already "Edit" here, as it is everywhere in the game.
    pub difficulty: String,
    pub meter: u32,
    /// 4 for singles, 8 for doubles.
    pub lanes: u8,
    /// Time-ordered, at most `MAX_ROWS_PER_CHART` of them.
    pub notes: Vec<NoteRow>,
}

/// Everything the preview needs from one simfile.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SongPreviewData {
    /// Empty when the simfile names none.
    pub title: String,
    pub artist: String,
    /// #MUSIC and #PREVIEW as written (relative to the song folder; may be empty)
    ///
    /// These are the simfile's words, not paths on this machine: match them
    /// against the song's own archive entries and never join them to a real
    /// folder. A downloaded simfile can say `..` or `C:\` as easily as
    /// `song.ogg`.
    pub music: String,
    /// `.ssc` only: a short clip cut for previews, smaller to fetch than the
    /// whole song. The `.sm` loader never reads one.
    pub preview_clip: String,
    /// the sample window in seconds of the music file, after the original's
    /// fallback: when #SAMPLELENGTH is missing or <= 0, start = max(start, 20),
    /// length = 20
    ///
    /// When `preview_clip` is set this is still the window the notes come
    /// from: a clip is normally cut from exactly this stretch, and it plays
    /// from its own zero.
    pub sample_start: f32,
    pub sample_length: f32,
    /// a representative tempo for the window's beat-keyed animation (the BPM
    /// in force at the sample start)
    ///
    /// Read from the first chart's clock, the one the window opens on; 0 when
    /// there is no chart to animate.
    pub bpm: f32,
    /// Singles before doubles, then meter ascending, then Beginner through
    /// Challenge and Edit last; simfile order breaks what ties remain.
    pub charts: Vec<ChartPreview>,
}

/// Reads one simfile's bytes into the chart preview.
///
/// `extension` is the simfile's, `sm` or `ssc`, with or without its dot and
/// in either case. A simfile that parses but has no pad chart is still a
/// preview, of nothing: the audio can play without a window.
pub(crate) fn preview_from_simfile(
    bytes: &[u8],
    extension: &str,
) -> Result<SongPreviewData, String> {
    if bytes.len() > MAX_SIMFILE_BYTES {
        return Err(format!(
            "the simfile is {} bytes, past the {MAX_SIMFILE_BYTES}-byte preview limit",
            bytes.len()
        ));
    }
    let extension = extension.strip_prefix('.').unwrap_or(extension);
    let options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
    // The path is only a label; an in-memory parse never looks anything up.
    let song = parse_song_bytes(bytes, extension, Path::new(""), &options, |_| 0.0)?;
    let (music, preview_clip) = header_media_tags(bytes, extension.eq_ignore_ascii_case("ssc"));
    let (sample_start, sample_length) = sample_window(song.sample_start, song.sample_length);

    let mut charts: Vec<(ChartPreview, f32)> = Vec::new();
    for chart in song.charts {
        let Some((doubles, lanes)) = pad_style(&chart.chart_type) else {
            continue;
        };
        // The game's own construction, so the preview's clock cannot drift
        // from the one gameplay judges against. A zero global offset: the
        // window is timed against the file, not against this machine.
        let gameplay = build_gameplay_chart_from_payload(
            CachedChartPayload {
                offset: chart.offset,
                notes: chart.notes,
                parsed_notes: chart.parsed_notes,
                row_to_beat: chart.row_to_beat,
                timing_segments: chart.timing_segments,
                chart_attacks: chart.chart_attacks,
            },
            0.0,
        );
        let preview = ChartPreview {
            doubles,
            difficulty: chart.difficulty,
            meter: chart.meter,
            lanes,
            notes: window_rows(&gameplay, lanes, sample_start, sample_length),
        };
        charts.push((preview, tempo_at(&gameplay.timing, sample_start)));
    }
    // Stable, so charts the order cannot tell apart keep the simfile's order.
    charts.sort_by(|(left, _), (right, _)| chart_order(left, right));
    let bpm = charts.first().map_or(0.0, |&(_, bpm)| bpm);

    Ok(SongPreviewData {
        title: if song.title == MISSING_TITLE {
            String::new()
        } else {
            song.title
        },
        artist: song.artist,
        music,
        preview_clip,
        sample_start,
        sample_length,
        bpm,
        charts: charts.into_iter().map(|(chart, _)| chart).collect(),
    })
}

/// `(doubles, lanes)` for a chart the window can draw.
fn pad_style(chart_type: &str) -> Option<(bool, u8)> {
    if chart_type.eq_ignore_ascii_case("dance-single") {
        Some((false, 4))
    } else if chart_type.eq_ignore_ascii_case("dance-double") {
        Some((true, 8))
    } else {
        None
    }
}

/// The sample window, after the original's fallback.
///
/// The parser hands over a start and a length only when they are positive.
/// No length means no sample was declared, so one is made: twenty seconds,
/// starting no earlier than twenty in. A start past that is the author's, and
/// kept. Neither can be infinite or negative by the time it leaves here.
fn sample_window(start: Option<f32>, length: Option<f32>) -> (f32, f32) {
    let finite = |value: Option<f32>| value.filter(|value| value.is_finite()).unwrap_or(0.0);
    let start = finite(start).max(0.0);
    let length = finite(length);
    if length > 0.0 {
        (start, length)
    } else {
        (start.max(FALLBACK_SAMPLE_START), FALLBACK_SAMPLE_LENGTH)
    }
}

/// The song's own `#MUSIC` and `#PREVIEW`, as written and trimmed.
///
/// The parser resolves `#MUSIC` against a folder this simfile does not have,
/// and never reads `#PREVIEW` at all, so both are read straight from the
/// bytes. In an `.ssc` only the song's header counts: a chart may name music
/// of its own after its `#NOTEDATA`, and that is not the song's. `#PREVIEW` is
/// an `.ssc` tag -- the `.sm` loader never reads one -- so neither does this.
fn header_media_tags(bytes: &[u8], ssc: bool) -> (String, String) {
    if !ssc {
        let [music] = latest_simfile_tag_values(bytes, [b"#MUSIC:".as_slice()]);
        return (music.trim().to_owned(), String::new());
    }
    let header = &bytes[..find_tag(bytes, b"#NOTEDATA:").unwrap_or(bytes.len())];
    let [music, preview] =
        latest_simfile_tag_values(header, [b"#MUSIC:".as_slice(), b"#PREVIEW:".as_slice()]);
    (music.trim().to_owned(), preview.trim().to_owned())
}

/// Where `tag` first starts, ignoring ASCII case. Only a `#` can start one, so
/// everything else is stepped over without a comparison.
fn find_tag(bytes: &[u8], tag: &[u8]) -> Option<usize> {
    bytes.iter().enumerate().find_map(|(at, &byte)| {
        (byte == b'#'
            && bytes
                .get(at..at + tag.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(tag)))
        .then_some(at)
    })
}

/// The chart's rows that land in the sample window, timed from its start.
///
/// Notes arrive from the parser grouped by row, rows in order, so one pass
/// folds each row's notes into one `NoteRow` and times each row once. A row is
/// timed by the chart's `TimingData` exactly as gameplay times it, through
/// the same row-time cursor when the chart's BPM map allows one.
fn window_rows(chart: &GameplayChartData, lanes: u8, start: f32, length: f32) -> Vec<NoteRow> {
    let timing = &chart.timing;
    // The parser emits rows in order and this relies on it. Should that ever
    // change, sorting a copy keeps the fold right rather than splitting rows.
    let notes: Cow<'_, [ParsedNote]> = if chart.parsed_notes.is_sorted_by_key(|n| n.row_index) {
        Cow::Borrowed(&chart.parsed_notes)
    } else {
        let mut sorted = chart.parsed_notes.clone();
        sorted.sort_by_key(|note| note.row_index);
        Cow::Owned(sorted)
    };
    // Amortise the cursor's validation over a real chart; ambiguous BPM maps
    // convert each row independently, as the lights and holds code does.
    let mut cursor =
        (notes.len() >= 16 && timing.supports_row_time_cache()).then(|| BeatTimeCache::new(timing));
    let start = f64::from(start);
    let length = f64::from(length);
    let mut row_in_window = |row_index: usize| -> Option<NoteRow> {
        let beat = timing.get_beat_for_row(row_index)?;
        // Fake segments, and warps: never judged, and timed to one instant.
        if !timing.is_judgable_at_beat(beat) {
            return None;
        }
        let time_ns = match cursor.as_mut() {
            Some(cursor) => timing.get_time_for_beat_ns_cached(beat, cursor),
            None => timing.get_time_for_beat_ns(beat),
        };
        let time = time_ns as f64 * 1e-9 - start;
        (-EDGE_SLACK_SECONDS..=length + EDGE_SLACK_SECONDS)
            .contains(&time)
            .then(|| NoteRow {
                time: time.clamp(0.0, length) as f32,
                cols: 0,
                lifts: 0,
                mines: 0,
                quant: quantization_index_from_beat(beat),
            })
    };

    let lanes = usize::from(lanes);
    let mut rows = Vec::new();
    let mut current_row = None;
    // The row being filled, when it is one the window draws.
    let mut open: Option<NoteRow> = None;
    for note in notes.iter() {
        if note.note_type == NoteType::Fake || note.column >= lanes {
            continue;
        }
        if current_row != Some(note.row_index) {
            rows.extend(open.take());
            if rows.len() == MAX_ROWS_PER_CHART {
                break;
            }
            current_row = Some(note.row_index);
            open = row_in_window(note.row_index);
        }
        if let Some(row) = open.as_mut() {
            let bit = 1u8 << note.column;
            row.cols |= bit;
            match note.note_type {
                NoteType::Mine => row.mines |= bit,
                NoteType::Lift => row.lifts |= bit,
                NoteType::Tap | NoteType::Hold | NoteType::Roll | NoteType::Fake => {}
            }
        }
    }
    // A row is only opened while there is room for it.
    rows.extend(open);
    // The window walks rows forward with a cursor, so order is load-bearing.
    // Row order is time order unless a chart runs its clock backwards; this
    // only costs a scan when it does not.
    if !rows.is_sorted_by(|a, b| a.time <= b.time) {
        rows.sort_by(|a, b| a.time.total_cmp(&b.time));
    }
    rows
}

/// The BPM in force `seconds` into the file, on this chart's clock.
fn tempo_at(timing: &TimingData, seconds: f32) -> f32 {
    let bpm = timing.get_bpm_for_beat(timing.get_beat_for_time(seconds));
    if bpm.is_finite() && bpm > 0.0 {
        bpm
    } else {
        0.0
    }
}

/// Singles before doubles, then meter, then Beginner through Challenge with
/// Edit -- and anything else the simfile calls itself -- last.
fn chart_order(left: &ChartPreview, right: &ChartPreview) -> Ordering {
    let rank = |chart: &ChartPreview| {
        standard_difficulty_index(&chart.difficulty).unwrap_or(STANDARD_DIFFICULTY_COUNT)
    };
    left.doubles
        .cmp(&right.doubles)
        .then(left.meter.cmp(&right.meter))
        .then_with(|| rank(left).cmp(&rank(right)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(time: f32, cols: u8, quant: u8) -> NoteRow {
        NoteRow {
            time,
            cols,
            lifts: 0,
            mines: 0,
            quant,
        }
    }

    fn mine(time: f32, cols: u8, quant: u8) -> NoteRow {
        NoteRow {
            mines: cols,
            ..row(time, cols, quant)
        }
    }

    fn lift(time: f32, cols: u8, quant: u8) -> NoteRow {
        NoteRow {
            lifts: cols,
            ..row(time, cols, quant)
        }
    }

    /// Times to a thousandth of a second; everything else exactly.
    #[track_caller]
    fn assert_rows(actual: &[NoteRow], expected: &[NoteRow]) {
        assert_eq!(
            actual.len(),
            expected.len(),
            "\nactual:   {actual:?}\nexpected: {expected:?}"
        );
        for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            assert!(
                (actual.time - expected.time).abs() < 1e-3,
                "row {index}: time {actual:?} vs {expected:?}"
            );
            assert_eq!(
                (actual.cols, actual.lifts, actual.mines, actual.quant),
                (
                    expected.cols,
                    expected.lifts,
                    expected.mines,
                    expected.quant
                ),
                "row {index}: {actual:?} vs {expected:?}"
            );
        }
    }

    fn charts(preview: &SongPreviewData) -> Vec<(bool, &str, u32, u8)> {
        preview
            .charts
            .iter()
            .map(|chart| {
                (
                    chart.doubles,
                    chart.difficulty.as_str(),
                    chart.meter,
                    chart.lanes,
                )
            })
            .collect()
    }

    /// Beat 0 sits at 0.5 s (`#OFFSET:-0.5`); 120 BPM, so half a second a beat,
    /// until beat 8 and 240 BPM; a half-second stop at beat 4, which is also
    /// where the window opens: 2.5 s in, four seconds long, so it closes on
    /// beat 14. Measures of 4ths, 8ths, 12ths and 16ths.
    const SM: &str = "#TITLE:Window Test;
#ARTIST:Tester;
#MUSIC:Song Audio.ogg;
#PREVIEW:not-an-sm-tag.ogg;
#OFFSET:-0.500;
#SAMPLESTART:2.500;
#SAMPLELENGTH:4.000;
#BPMS:0.000=120.000,8.000=240.000;
#STOPS:4.000=0.500;
#NOTES:
     dance-double:
     :
     Challenge:
     9:
     0,0,0,0,0:
00000000
00000000
00000000
00000000
,
10000001
00011000
0000000M
00000000
;
#NOTES:
     pump-single:
     :
     Hard:
     7:
     0,0,0,0,0:
10000
01000
00100
00010
,
10001
01010
00100
00011
;
#NOTES:
     dance-single:
     :
     Hard:
     9:
     0,0,0,0,0:
1000
0000
0000
0100
,
1001
0M00
2000
0000
3040
000L
0030
0000
,
1000
0100
0010
0000
0000
0000
0000
0000
0000
0000
0000
0000
,
0000
1000
0F00
0000
0000
0000
0000
0000
0001
0010
0000
0000
0000
0000
0000
0000
;
";

    #[test]
    fn an_sm_window_is_timed_by_the_charts_own_clock() {
        let preview = preview_from_simfile(SM.as_bytes(), "sm").expect("parse");
        assert_eq!(preview.title, "Window Test");
        assert_eq!(preview.artist, "Tester");
        assert_eq!(preview.music, "Song Audio.ogg");
        // `#PREVIEW` is an .ssc tag; the .sm loader never reads one
        assert_eq!(preview.preview_clip, "");
        assert_eq!((preview.sample_start, preview.sample_length), (2.5, 4.0));
        // beat 4, under the opening 120
        assert_eq!(preview.bpm, 120.0);

        // the pump chart is gone, and singles lead though the double came first
        assert_eq!(
            charts(&preview),
            [(false, "Hard", 9, 4), (true, "Challenge", 9, 8)]
        );

        assert_rows(
            &preview.charts[0].notes,
            &[
                // beat 3 at 2.0 s is before the window; beat 4 opens it, hit
                // before its stop rather than after
                row(0.0, 0b1001, 0),
                // the 8ths of measure two, a half-second stop later
                mine(0.75, 0b0010, 1),
                // a hold head is a tap; its tail at beat 6 is not sent
                row(1.0, 0b0001, 0),
                // a roll head
                row(1.5, 0b0100, 0),
                lift(1.75, 0b1000, 1),
                // beat 7 holds only the roll's tail: no row
                // the 12ths, at 240 BPM
                row(2.5, 0b0001, 0),
                row(2.5 + 0.25 / 3.0, 0b0010, 2),
                row(2.5 + 0.5 / 3.0, 0b0100, 2),
                // the 16ths: the fake at beat 12.5 is dropped, beat 14 closes
                // the window and is kept, beat 14.25 is past it
                row(3.5625, 0b0001, 3),
                row(4.0, 0b1000, 0),
            ],
        );
        assert_rows(
            &preview.charts[1].notes,
            &[
                row(0.0, 0b1000_0001, 0),
                row(1.0, 0b0001_1000, 0),
                mine(1.5, 0b1000_0000, 0),
            ],
        );
    }

    /// The single has a clock of its own: beat 0 a second in, a warp over
    /// beats 2-4, a fake beat 6 and a half-second delay at beat 7. The double
    /// keeps the song's. The window is 2.5-5.5 s.
    const SSC: &str = "#VERSION:0.83;
#TITLE:Warp Zone;
#ARTIST:Tester;
#MUSIC: audio/song.ogg ;
#PREVIEW:preview.ogg;
#OFFSET:0.000;
#BPMS:0.000=60.000,4.000=120.000;
#SAMPLESTART:2.500;
#SAMPLELENGTH:3.000;
#NOTEDATA:;
#STEPSTYPE:dance-double;
#DIFFICULTY:Medium;
#METER:6;
#MUSIC:chart-only.ogg;
#NOTES:
00000000
00000000
00000000
10000001
,
00000000
01000010
00000000
00000000
;
#NOTEDATA:;
#STEPSTYPE:dance-single;
#DIFFICULTY:Challenge;
#METER:11;
#OFFSET:-1.000;
#BPMS:0.000=60.000,4.000=120.000;
#WARPS:2.000=2.000;
#FAKES:6.000=1.000;
#DELAYS:7.000=0.500;
#NOTES:
1000
0100
0010
0001
,
1000
0100
0010
0001
,
1000
0100
0000
0000
;
";

    #[test]
    fn an_ssc_window_follows_warps_fakes_delays_and_split_timing() {
        let preview = preview_from_simfile(SSC.as_bytes(), ".SSC").expect("parse");
        assert_eq!(preview.title, "Warp Zone");
        // the song's own, trimmed; the chart's music is not the song's
        assert_eq!(preview.music, "audio/song.ogg");
        assert_eq!(preview.preview_clip, "preview.ogg");
        assert_eq!((preview.sample_start, preview.sample_length), (2.5, 3.0));
        // the single leads: 2.5 s is beat 1.5 on its clock, still at 60
        assert_eq!(preview.bpm, 60.0);
        assert_eq!(
            charts(&preview),
            [(false, "Challenge", 11, 4), (true, "Medium", 6, 8)]
        );

        assert_rows(
            &preview.charts[0].notes,
            &[
                // beats 2 and 3 are warped over: on a clock they would land on
                // 3.0 s with beat 4, so they are not drawn at all
                row(0.5, 0b0001, 0),
                row(1.0, 0b0010, 0),
                // beat 6 is fake; beat 7 waits out its delay first
                row(2.5, 0b1000, 0),
                // and everything after it is half a second later
                row(3.0, 0b0001, 0),
            ],
        );
        // the song's clock: no offset, no warp
        assert_rows(
            &preview.charts[1].notes,
            &[row(0.5, 0b1000_0001, 0), row(2.0, 0b0100_0010, 0)],
        );
    }

    #[test]
    fn a_song_without_a_declared_sample_gets_the_originals_fallback() {
        // 60 BPM to beat 10, then 150: twenty seconds in is beat 35, and
        // forty is beat 85. A start of five is lifted to twenty.
        let mut measures = vec![["0000"; 4]; 22];
        measures[8][2] = "1000"; // beat 34, 19.6 s: just before
        measures[8][3] = "0100"; // beat 35: the window's first instant
        measures[10][0] = "0010"; // beat 40
        measures[21][1] = "0001"; // beat 85: its last
        measures[21][2] = "1000"; // beat 86: past it
        let notes = measures
            .iter()
            .map(|measure| measure.join("\n"))
            .collect::<Vec<_>>()
            .join("\n,\n");
        let simfile = format!(
            "#BPMS:0.000=60.000,10.000=150.000;\n#SAMPLESTART:5.000;\n\
             #NOTES:dance-single::Easy:2:0,0,0,0,0:\n{notes}\n;"
        );

        let preview = preview_from_simfile(simfile.as_bytes(), "sm").expect("parse");
        assert_eq!((preview.sample_start, preview.sample_length), (20.0, 20.0));
        assert_eq!(preview.bpm, 150.0);
        // no #TITLE is no title, not the parser's placeholder for one
        assert_eq!(preview.title, "");
        assert_eq!(preview.music, "");
        assert_rows(
            &preview.charts[0].notes,
            &[
                row(0.0, 0b0010, 0),
                row(2.0, 0b0100, 0),
                row(20.0, 0b1000, 0),
            ],
        );

        // a declared length of nothing is no length at all
        let simfile = simfile.replace("#SAMPLESTART:5.000;", "#SAMPLESTART:30;#SAMPLELENGTH:0;");
        let preview = preview_from_simfile(simfile.as_bytes(), "sm").expect("parse");
        assert_eq!((preview.sample_start, preview.sample_length), (30.0, 20.0));
    }

    #[test]
    fn a_row_on_the_windows_edge_survives_float_noise() {
        // An ITG-synced song whose sample starts and ends on a beat. Beat 4 is
        // 2.009 s on its clock, and 2.009 as the simfile's f32 is a hair
        // later: compared bare, the window would open just after its first
        // row. Beat 8 closes it the same way from the other side.
        let simfile = "#OFFSET:-0.009;#BPMS:0=120;#SAMPLESTART:2.009;#SAMPLELENGTH:2;\n\
                       #NOTES:dance-single::Hard:9:0,0,0,0,0:\n\
                       0000\n0000\n0000\n1000\n,\n1000\n0000\n0000\n0000\n,\n0001\n0000\n0000\n0000\n;";
        let preview = preview_from_simfile(simfile.as_bytes(), "sm").expect("parse");
        assert_eq!(preview.sample_start, 2.009);
        let notes = &preview.charts[0].notes;
        assert_rows(notes, &[row(0.0, 0b0001, 0), row(2.0, 0b1000, 0)]);
        // and the contract holds to the bit: never before the start, never
        // past the end
        assert!(notes[0].time >= 0.0 && notes[1].time <= preview.sample_length);
    }

    #[test]
    fn the_sample_window_rule() {
        assert_eq!(sample_window(Some(42.5), Some(15.0)), (42.5, 15.0));
        assert_eq!(sample_window(None, Some(12.0)), (0.0, 12.0));
        assert_eq!(sample_window(Some(5.0), None), (20.0, 20.0));
        // a start already past twenty is kept
        assert_eq!(sample_window(Some(61.5), None), (61.5, 20.0));
        assert_eq!(sample_window(None, None), (20.0, 20.0));
        assert_eq!(sample_window(Some(30.0), Some(-1.0)), (30.0, 20.0));
        // nothing infinite or negative survives
        assert_eq!(sample_window(Some(f32::INFINITY), Some(10.0)), (0.0, 10.0));
        assert_eq!(sample_window(Some(30.0), Some(f32::INFINITY)), (30.0, 20.0));
        assert_eq!(sample_window(Some(f32::NAN), Some(f32::NAN)), (20.0, 20.0));
        assert_eq!(sample_window(Some(-3.0), Some(10.0)), (0.0, 10.0));
    }

    #[test]
    fn charts_run_singles_first_then_meter_then_difficulty() {
        let chart = |style: &str, difficulty: &str, meter: u32, notes: &str| {
            format!("#NOTES:{style}::{difficulty}:{meter}:0,0,0,0,0:\n{notes}\n;\n")
        };
        let simfile = [
            "#BPMS:0=120;#SAMPLESTART:0.1;#SAMPLELENGTH:10;\n".to_owned(),
            chart("dance-double", "Easy", 2, "00000000\n10000000"),
            chart("dance-single", "Challenge", 9, "0000\n1000"),
            chart("dance-single", "Hard", 9, "0000\n0100"),
            chart("dance-single", "Edit", 3, "0000\n0010"),
            chart("dance-single", "Beginner", 3, "0000\n0001"),
            chart("dance-single", "Medium", 5, "0000\n1100"),
            chart("dance-solo", "Hard", 1, "000000\n100000"),
        ]
        .concat();
        let preview = preview_from_simfile(simfile.as_bytes(), "sm").expect("parse");
        assert_eq!(
            charts(&preview),
            [
                (false, "Beginner", 3, 4),
                (false, "Edit", 3, 4),
                (false, "Medium", 5, 4),
                (false, "Hard", 9, 4),
                (false, "Challenge", 9, 4),
                (true, "Easy", 2, 8),
            ]
        );
        // each chart kept its own notes through the sort: beat 2 at 120 BPM
        // is 1.0 s, 0.9 s into a window opening at 0.1
        assert_rows(&preview.charts[0].notes, &[row(0.9, 0b1000, 0)]);
        assert_rows(&preview.charts[5].notes, &[row(0.9, 0b0000_0001, 0)]);
    }

    #[test]
    fn rows_are_capped_and_quantized_down_to_the_finest() {
        // Six measures of 192nds at 240 BPM: a row every 1/192 s, 1152 of
        // them inside a twenty-second window.
        let measure = vec!["1000"; 192].join("\n");
        let simfile = format!(
            "#BPMS:0=240;#SAMPLELENGTH:20;\n#NOTES:dance-single::Hard:12:0,0,0,0,0:\n{}\n;",
            vec![measure; 6].join("\n,\n")
        );
        let preview = preview_from_simfile(simfile.as_bytes(), "sm").expect("parse");
        assert_eq!(preview.sample_start, 0.0);
        let notes = &preview.charts[0].notes;
        assert_eq!(notes.len(), MAX_ROWS_PER_CHART);
        assert!((notes[899].time - 899.0 / 192.0).abs() < 1e-3);
        assert!(notes.is_sorted_by(|a, b| a.time < b.time));
        // a beat is 48 rows: 4th, 192nd, 192nd, 64th, 48th, 32nd, 24th, then
        // 16th at 12, 12th at 16, 8th at 24 and the next 4th at 48
        let quant = |row: usize| notes[row].quant;
        assert_eq!(
            [0, 1, 2, 3, 4, 6, 8, 12, 16, 24, 48].map(quant),
            [0, 8, 8, 7, 6, 5, 4, 3, 2, 1, 0]
        );
    }

    #[test]
    fn a_simfile_with_no_pad_chart_previews_nothing() {
        let simfile = "#TITLE:Pump Only;#BPMS:0=120;\n\
                       #NOTES:pump-single::Hard:7:0,0,0,0,0:\n10000\n01000\n;";
        let preview = preview_from_simfile(simfile.as_bytes(), "sm").expect("parse");
        assert_eq!(preview.title, "Pump Only");
        assert!(preview.charts.is_empty());
        assert_eq!(preview.bpm, 0.0);
    }

    #[test]
    fn what_is_not_a_simfile_is_refused() {
        let error = preview_from_simfile(b"#TITLE:x;", "dwi").unwrap_err();
        assert!(error.contains(".sm or .ssc"), "{error}");
        let huge = vec![b' '; MAX_SIMFILE_BYTES + 1];
        let error = preview_from_simfile(&huge, "sm").unwrap_err();
        assert!(error.contains("preview limit"), "{error}");
    }

    #[test]
    fn header_tags_stop_at_the_first_chart_of_an_ssc() {
        let ssc = b"#MUSIC:a.ogg;#PREVIEW:p.ogg;#NoteData:;#MUSIC:b.ogg;#PREVIEW:q.ogg;";
        assert_eq!(
            header_media_tags(ssc, true),
            ("a.ogg".to_owned(), "p.ogg".to_owned())
        );
        // an .sm has no charts to stop at, and its last word wins, as the
        // parser's does
        let sm = b"#MUSIC:a.ogg;#PREVIEWVID:v.avi;#MUSIC:b.ogg;";
        assert_eq!(
            header_media_tags(sm, false),
            ("b.ogg".to_owned(), String::new())
        );
        assert_eq!(find_tag(b"x#notedata:", b"#NOTEDATA:"), Some(1));
        assert_eq!(find_tag(b"#NOTEDATA", b"#NOTEDATA:"), None);
    }
}
