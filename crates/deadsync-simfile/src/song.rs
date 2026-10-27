use crate::artwork::{ResolvedSongArtwork, resolve_song_artwork_like_itg};
use crate::cache::{
    CachedChartPayloadIndex, CachedParsedNote, CachedTimingSegments, SerializableChartData,
    SerializableSongBackgroundChange, SerializableSongBackgroundLuaChange, SerializableSongData,
    SerializableSongForegroundChange, SerializableSongForegroundLuaChange, build_song_meta,
    cache_background_changes, parse_chart_display_bpm, update_precise_song_bounds,
};
use crate::changes::{
    extract_background_lua_change_set, extract_foreground_change_sets,
    resolve_background_changes_from_roots, resolve_background_layer2_changes_from_roots,
};
use crate::media::resolve_song_asset_path_like_itg;
use crate::stats::build_stamina_counts;
use crate::timing::{parse_cached_combos, parse_cached_tickcounts, parse_cached_time_signatures};
use deadsync_chart::{STANDARD_DIFFICULTY_NAMES, SongData, song::standard_difficulty_index};
use deadsync_core::note::NoteType;
use rssp::{
    AnalysisOptions, AnalysisScratch, ChartNoteType, ParsedChartNote, PreparedAnalysis,
    SimfileSummary, analyze_prepared_in_with_notes,
};
use std::borrow::Cow;
use std::cmp::Ordering;
#[cfg(test)]
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const SONG_ANALYSIS_MONO_THRESHOLD: usize = 6;

pub struct ParseSongOptions {
    pub mono_threshold: usize,
    pub song_movie_roots: Vec<PathBuf>,
    pub random_movie_roots: Vec<PathBuf>,
    pub bg_animation_roots: Vec<PathBuf>,
}

impl ParseSongOptions {
    #[must_use]
    pub const fn new(
        song_movie_roots: Vec<PathBuf>,
        random_movie_roots: Vec<PathBuf>,
        bg_animation_roots: Vec<PathBuf>,
    ) -> Self {
        Self {
            mono_threshold: SONG_ANALYSIS_MONO_THRESHOLD,
            song_movie_roots,
            random_movie_roots,
            bg_animation_roots,
        }
    }
}

/// Read-only RSSP configuration prepared once for a batch of songs.
pub struct SongAnalyzer {
    prepared: PreparedAnalysis,
}

impl SongAnalyzer {
    #[must_use]
    pub fn new(options: &ParseSongOptions) -> Self {
        Self {
            prepared: PreparedAnalysis::new(AnalysisOptions {
                mono_threshold: options.mono_threshold,
                ..AnalysisOptions::default()
            }),
        }
    }
}

/// Worker-owned temporary storage retained across cache misses in one scan.
///
/// This is single-thread-only. The scan worker that creates it remains its sole
/// owner and drops it when the worker exits, releasing the largest simfile,
/// RSSP analysis, chart-note handoff, cache-header, and chart-payload buffers
/// retained during that scan.
#[derive(Default)]
pub struct SongParseScratch {
    input: Vec<u8>,
    analysis: AnalysisScratch,
    parsed_notes: Vec<Vec<CachedParsedNote>>,
    cache_header: Vec<u8>,
    cache_payloads: Vec<Vec<u8>>,
    cache_payload_indices: Vec<CachedChartPayloadIndex>,
}

impl SongParseScratch {
    pub(crate) const fn cache_header(&mut self) -> &mut Vec<u8> {
        &mut self.cache_header
    }

    pub(crate) const fn cache_write_buffers(
        &mut self,
    ) -> (
        &mut Vec<u8>,
        &mut Vec<Vec<u8>>,
        &mut Vec<CachedChartPayloadIndex>,
    ) {
        (
            &mut self.cache_header,
            &mut self.cache_payloads,
            &mut self.cache_payload_indices,
        )
    }
}

fn cached_note_from_rssp(note: ParsedChartNote) -> CachedParsedNote {
    let note_type = match note.note_type {
        ChartNoteType::Tap => NoteType::Tap,
        ChartNoteType::Hold => NoteType::Hold,
        ChartNoteType::Roll => NoteType::Roll,
        ChartNoteType::Mine => NoteType::Mine,
        ChartNoteType::Lift => NoteType::Lift,
        ChartNoteType::Fake => NoteType::Fake,
    };
    CachedParsedNote {
        player: 0,
        row_index: note.row_index as u32,
        column: note.column as u8,
        note_type: note_type.into(),
        tail_row_index: note.tail_row_index.map(|row| row as u32),
    }
}

struct SongBuildInput<'a> {
    path: &'a Path,
    /// The folder media tags are resolved against, or `None` for a simfile
    /// that has no folder on this machine -- see `parse_song_bytes`.
    media_dir: Option<&'a Path>,
    simfile_data: &'a [u8],
    song_music_path: Option<PathBuf>,
    music_length_seconds: f32,
    options: &'a ParseSongOptions,
    parsed_notes: &'a mut Vec<Vec<CachedParsedNote>>,
}

/// One simfile's bytes and where they claim to live.
struct SimfileSource<'a> {
    data: &'a [u8],
    /// `sm` or `ssc`, which picks the dialect RSSP reads.
    extension: &'a str,
    /// Recorded as the song's `simfile_path`.
    path: &'a Path,
    /// Whether `path` names a real file whose folder holds the song's media.
    on_disk: bool,
}

pub fn parse_song_file(
    path: &Path,
    options: &ParseSongOptions,
    music_len: impl FnOnce(Option<&Path>) -> f32,
) -> Result<SerializableSongData, String> {
    let analyzer = SongAnalyzer::new(options);
    let mut scratch = SongParseScratch::default();
    parse_song_file_in(path, options, &analyzer, &mut scratch, music_len)
}

pub fn parse_song_file_in(
    path: &Path,
    options: &ParseSongOptions,
    analyzer: &SongAnalyzer,
    scratch: &mut SongParseScratch,
    music_len: impl FnOnce(Option<&Path>) -> f32,
) -> Result<SerializableSongData, String> {
    if analyzer.prepared.options().mono_threshold != options.mono_threshold {
        return Err("Song analyzer does not match parse options".to_string());
    }
    let SongParseScratch {
        input,
        analysis,
        parsed_notes,
        ..
    } = scratch;
    input.clear();
    let mut file = File::open(path).map_err(|e| format!("Could not read file: {e}"))?;
    file.read_to_end(input)
        .map_err(|e| format!("Could not read file: {e}"))?;
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    parse_song_source(
        SimfileSource {
            data: input,
            extension,
            path,
            on_disk: true,
        },
        options,
        analyzer,
        analysis,
        parsed_notes,
        music_len,
    )
}

/// Parses a simfile held in memory: one read out of a pack archive that was
/// never unpacked, say.
///
/// Everything a simfile *says* comes back exactly as `parse_song_file` would
/// give it -- title, sample window, every chart's notes and timing. What a
/// song *finds in its folder* does not: music, artwork, background and
/// foreground changes and Lua all come back empty, and `music_len` is asked
/// about no file at all.
///
/// That is not only because there is no folder. Media tags are paths, and a
/// path the simfile chose can leave any folder it is joined to: `Path::join`
/// with an absolute tag, or a `\\host\share` one, *replaces* the base. Looked
/// up, a downloaded simfile could make this machine stat its files or open an
/// SMB connection to a stranger -- just by being previewed. So in-memory
/// parsing never looks anything up, and `virtual_path` is a label recorded as
/// the song's `simfile_path`, never a place.
///
/// `extension` is `sm` or `ssc`, ignoring case, and picks the dialect.
pub fn parse_song_bytes(
    data: &[u8],
    extension: &str,
    virtual_path: &Path,
    options: &ParseSongOptions,
    music_len: impl FnOnce(Option<&Path>) -> f32,
) -> Result<SerializableSongData, String> {
    let analyzer = SongAnalyzer::new(options);
    let mut analysis = AnalysisScratch::default();
    let mut parsed_notes = Vec::new();
    parse_song_source(
        SimfileSource {
            data,
            extension,
            path: virtual_path,
            on_disk: false,
        },
        options,
        &analyzer,
        &mut analysis,
        &mut parsed_notes,
        music_len,
    )
}

/// Project native note-column syntax into RSSP's unannotated note input.
fn note_analysis_bytes<'a>(
    data: &'a [u8],
    entries: &[rssp::parse::ParsedChartEntry<'a>],
) -> Cow<'a, [u8]> {
    let mut indices = Vec::new();
    for entry in entries {
        let Some(lanes) = rssp::supported_stepstype_lanes_bytes(entry.fields[0]) else { continue };
        if !entry.note_data.contains(&b'[') { continue; }
        // ParsedChartEntry borrows each note block directly from these bytes.
        let offset = entry.note_data.as_ptr() as usize - data.as_ptr() as usize;
        let mut line_start = 0;
        for line in entry.note_data.split_inclusive(|byte| matches!(byte, b'\n' | b',')) {
            let mut end = line.iter().position(|byte| matches!(byte, b'\n' | b','))
                .unwrap_or(line.len());
            if let Some(comment) = line[..end].windows(2).position(|bytes| bytes == b"//") {
                end = comment;
            }
            while end > 0 && matches!(line[end - 1], b'\r' | b'\t' | b' ') { end -= 1; }
            let mut cursor = 0;
            while cursor < end && matches!(line[cursor], b'\r' | b'\t' | b' ') { cursor += 1; }
            // NoteDataUtil::LoadFromSMNoteDataStringWithPlayer consumes one
            // column, then skips its optional [keysound index] through ']'
            // or the row end. Its digits must never enter RSSP as notes.
            for _ in 0..lanes {
                if cursor >= end { break; }
                cursor += 1;
                if cursor < end && line[cursor] == b'[' {
                    let start = cursor;
                    cursor += 1;
                    while cursor < end && line[cursor] != b']' { cursor += 1; }
                    if cursor < end { cursor += 1; }
                    indices.push(offset + line_start + start..offset + line_start + cursor);
                }
            }
            line_start += line.len();
        }
    }
    if indices.is_empty() { return Cow::Borrowed(data); }
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

/// The parse both entry points share once the bytes are in hand.
fn parse_song_source(
    source: SimfileSource<'_>,
    options: &ParseSongOptions,
    analyzer: &SongAnalyzer,
    analysis: &mut AnalysisScratch,
    parsed_notes: &mut Vec<Vec<CachedParsedNote>>,
    music_len: impl FnOnce(Option<&Path>) -> f32,
) -> Result<SerializableSongData, String> {
    let parsed = rssp::parse::extract_sections(source.data, source.extension)
        .map_err(|error| error.to_string())?;
    let projection = crate::partner_charts::project(source.data, &parsed.notes_list);
    let projected = rssp::parse::extract_sections(&projection.bytes, source.extension)
        .map_err(|error| error.to_string())?;
    let analysis_bytes = note_analysis_bytes(&projection.bytes, &projected.notes_list);
    let mut summary = analyze_prepared_in_with_notes(
        &analysis_bytes,
        source.extension,
        &analyzer.prepared,
        analysis,
        parsed_notes,
        cached_note_from_rssp,
    )?;
    for (chart_index, (chart, entry)) in summary
        .charts
        .iter_mut()
        .zip(parsed.notes_list.iter().filter(|entry| {
            entry.field_count >= 5 && crate::partner_charts::playable(entry.fields[0])
        }))
        .enumerate()
    {
        chart.step_type_str = rssp::parse::decode_unescape(entry.fields[0])
            .trim()
            .to_ascii_lowercase();
        if crate::partner_charts::lanes(entry.fields[0]).is_some() {
            // Couple has separate scores, so it must not alias an identical
            // Double chart. Routine additionally hashes both ownership blocks.
            let mut hash_notes = chart.step_type_str.as_bytes().to_vec();
            hash_notes.push(b'\n');
            hash_notes.extend_from_slice(
                projection
                    .hash_notes
                    .get(&chart_index)
                    .map_or(chart.minimized_note_data.as_slice(), Vec::as_slice),
            );
            let bpms = chart
                .chart_bpms_norm
                .as_deref()
                .unwrap_or(&summary.normalized_bpms);
            (chart.short_hash, chart.bpm_neutral_hash) =
                rssp::hash::compute_chart_hash_pair(&hash_notes, bpms);
        }
        if chart.step_type_str.ends_with("-routine") {
            for note in &mut parsed_notes[chart_index] {
                let beat = chart
                    .row_to_beat
                    .get(note.row_index as usize)
                    .ok_or("Routine note has no matching beat")?;
                let row = (*beat as f64 * 48.0).round() as usize;
                note.player = *projection
                    .owners
                    .get(&(chart_index, row, note.column as usize))
                    .ok_or("Routine note lost its participant during analysis")?;
            }
        }
    }
    // RSSP rounds its report offset to milliseconds. Runtime song timing
    // uses the authored float, as NotesLoaderSM::SMSetOffset does.
    summary.offset = rssp::parse::parse_offset_seconds(parsed.offset);
    if source.extension.eq_ignore_ascii_case("sm") {
        // RSSP trims Unicode whitespace for analysis. SMLoader::LoadFromTokens
        // trims only RageUtil's CR, LF, tab and space, then uses the same
        // authored text for the description, credit and chart name.
        let entries = parsed.notes_list.iter().filter(|entry| {
            entry.field_count >= 5
                && crate::partner_charts::playable(entry.fields[0])
        });
        for (chart, entry) in summary.charts.iter_mut().zip(entries) {
            let description = rssp::parse::decode_unescape(entry.fields[1]);
            let description = description.trim_matches(['\r', '\n', '\t', ' ']);
            chart.description_str = description.to_owned();
            chart.step_artist_str = description.to_owned();
            chart.chart_name_str = description.to_owned();
        }
    } else if source.extension.eq_ignore_ascii_case("ssc") {
        let version = rssp::parse::parse_version(parsed.version, rssp::timing::TimingFormat::Ssc);
        // SSC SetDescription uses the same ASCII-only Trim. Before 0.74 the
        // tag names the chart instead, so leave that existing normalization.
        if !(version < rssp::parse::SSC_VERSION_CHART_NAME_TAG) {
            let entries = parsed.notes_list.iter().filter(|entry| {
                entry.field_count >= 5
                    && crate::partner_charts::playable(entry.fields[0])
            });
            for (chart, entry) in summary.charts.iter_mut().zip(entries) {
                chart.description_str = rssp::parse::decode_unescape(entry.fields[1])
                    .trim_matches(['\r', '\n', '\t', ' ']).to_owned();
            }
        }
    }
    let media_dir = if source.on_disk {
        Some(
            source
                .path
                .parent()
                .ok_or_else(|| "Could not determine simfile directory".to_string())?,
        )
    } else {
        None
    };
    let song_music_path = media_dir.and_then(|dir| resolve_music_path(dir, &summary.music_path));
    let music_length_seconds = final_music_len(&summary, music_len(song_music_path.as_deref()));
    let additional_bounds = unplayed_chart_note_bounds(&parsed, &source.extension);
    let mut song = build_song_data(
        summary,
        SongBuildInput {
            path: source.path,
            media_dir,
            simfile_data: source.data,
            song_music_path,
            music_length_seconds,
            options,
            parsed_notes,
        },
    );
    update_precise_song_bounds(&mut song, 0.0);
    if let Some([first, last]) = additional_bounds {
        song.first_second = if song.precise_note_last_second_seconds.is_some() {
            song.first_second.min(first)
        } else { first };
        let last = song.precise_note_last_second_seconds.map_or(last, |value| value.max(last));
        song.precise_note_last_second_seconds = Some(last);
        song.precise_last_second_seconds = song.precise_last_second_seconds.max(last);
        song.total_length_seconds = song.total_length_seconds.max(last as i32);
    }
    Ok(song)
}

/// Authored Steps retain their Song::GetAllSteps order even when their style
/// cannot be selected for gameplay in DeadSync.
pub struct NativeStepMetadata {
    pub chart_type: String,
    pub difficulty: String,
    pub description: String,
    pub chart_name: String,
    pub credit: String,
    pub meter: u32,
    pub music_file: String,
    pub playable_index: Option<usize>,
}

pub fn load_native_step_metadata(song: &deadsync_chart::SongData) -> Result<Vec<NativeStepMetadata>, String> {
    let data = std::fs::read(&song.simfile_path).map_err(|error| error.to_string())?;
    let extension = song.simfile_path.extension().and_then(|value| value.to_str()).unwrap_or("sm");
    let parsed = rssp::parse::extract_sections(&data, extension).map_err(|error| error.to_string())?;
    let ssc = extension.eq_ignore_ascii_case("ssc");
    let format = if ssc { rssp::timing::TimingFormat::Ssc } else { rssp::timing::TimingFormat::Sm };
    let version = rssp::parse::parse_version(parsed.version, format);
    let text = |bytes: &[u8]| rssp::parse::decode_unescape(bytes)
        .trim_matches(['\r', '\n', '\t', ' ']).to_owned();
    let mut playable = 0;
    let mut steps = Vec::new();
    for entry in &parsed.notes_list {
        if entry.field_count < 5 { continue; }
        let kind = text(entry.fields[0]).to_ascii_lowercase();
        if crate::partner_charts::playable(entry.fields[0]) {
            let chart = song.charts.get(playable).ok_or("native Steps catalog exceeds playable charts")?;
            steps.push(NativeStepMetadata {
                chart_type: chart.chart_type.clone(), difficulty: chart.difficulty.clone(),
                description: chart.description.clone(), chart_name: chart.chart_name.clone(),
                credit: chart.step_artist.clone(), meter: chart.meter,
                music_file: chart.music_file.clone(), playable_index: Some(playable),
            });
            playable += 1;
        } else if native_unplayed_step_lanes(&kind).is_some() || kind == "lights-cabinet" {
            let legacy_name = ssc && version < rssp::parse::SSC_VERSION_CHART_NAME_TAG;
            let description = text(entry.fields[1]);
            steps.push(NativeStepMetadata {
                chart_type: kind, difficulty: text(entry.fields[2]),
                chart_name: if !ssc || legacy_name { description.clone() }
                    else { text(entry.chart_name.unwrap_or_default()) },
                credit: if ssc { text(entry.fields[4]) } else { description.clone() },
                description: if legacy_name { String::new() } else { description },
                meter: text(entry.fields[3]).parse().unwrap_or(1),
                music_file: entry.chart_music.as_deref().map(&text).unwrap_or_default(),
                playable_index: None,
            });
        }
    }
    if playable != song.charts.len() { return Err("native Steps catalog is missing playable charts".into()); }
    Ok(steps)
}

fn native_unplayed_step_lanes(kind: &str) -> Option<usize> {
    Some(match kind {
        "dance-threepanel" => 3,
        "dance-solo" | "pump-halfdouble" | "bm-single5" | "bm-versus5" | "kickbox-insect" => 6,
        "dance-couple" | "dance-routine" | "ds3ddx-single" | "techno-single8" | "techno-double4" | "bm-single7" | "bm-versus7" => 8,
        "pump-couple" | "pump-routine" | "ez2-double" | "techno-double5" => 10,
        "kb7-single" | "ez2-real" => 7,
        "ez2-single" | "para-single" | "pnm-five" | "techno-single5" => 5,
        "maniax-single" | "techno-single4" | "kickbox-quadarm" | "kickbox-human" => 4,
        "bm-double5" => 12,
        "bm-double7" | "techno-double8" => 16,
        "maniax-double" | "kickbox-arachnid" => 8,
        "pnm-nine" => 9,
        _ => return None,
    })
}

/// Song.cpp includes native charts that the gameplay analyzer does not load.
/// Their notes still determine when ScreenGameplay transitions out.
fn unplayed_chart_note_bounds(parsed: &rssp::parse::ParsedSimfileData<'_>, extension: &str) -> Option<[f32; 2]> {
    use rssp::timing::{compute_timing_segments, steps_timing_allowed, TimingFormat};
    let format = if extension.eq_ignore_ascii_case("ssc") { TimingFormat::Ssc } else { TimingFormat::Sm };
    let version = rssp::parse::parse_version(parsed.version, format);
    let own_timing_allowed = steps_timing_allowed(version, format);
    let mut bounds: Option<[f32; 2]> = None;
    for entry in &parsed.notes_list {
        let Ok(kind) = std::str::from_utf8(entry.fields[0]) else { continue; };
        let kind = kind.trim().to_ascii_lowercase();
        if crate::partner_charts::playable(entry.fields[0]) { continue; }
        // GameManager.cpp's native StepsTypeInfo table. Lights never contribute.
        let Some(lanes) = native_unplayed_step_lanes(&kind) else { continue; };
        if parsed.notes_list.len() > 1 && entry.fields[2].eq_ignore_ascii_case(b"Edit") { continue; }
        let Some([first, last]) = native_note_row_bounds(entry.note_data, lanes) else { continue; };
        if last == 0 { continue; }
        let own = own_timing_allowed && [entry.chart_bpms.as_deref(), entry.chart_stops.as_deref(),
            entry.chart_delays.as_deref(), entry.chart_warps.as_deref(), entry.chart_speeds.as_deref(),
            entry.chart_scrolls.as_deref(), entry.chart_fakes.as_deref(), entry.chart_offset.as_deref(),
            entry.chart_time_signatures.as_deref(), entry.chart_labels.as_deref(),
            entry.chart_tickcounts.as_deref(), entry.chart_combos.as_deref()].iter().any(Option::is_some);
        let text = |value: Option<&[u8]>| value.map(|value| String::from_utf8_lossy(value).into_owned());
        let tags = [(entry.chart_bpms.as_deref(), parsed.bpms), (entry.chart_stops.as_deref(), parsed.stops),
            (entry.chart_delays.as_deref(), parsed.delays), (entry.chart_warps.as_deref(), parsed.warps)];
        let tags = tags.map(|(chart, global)| (text(if own_timing_allowed { chart } else { None }),
            text(if own { None } else { global }).unwrap_or_default()));
        let segments = compute_timing_segments(tags[0].0.as_deref(), &tags[0].1,
            tags[1].0.as_deref(), &tags[1].1, tags[2].0.as_deref(), &tags[2].1,
            tags[3].0.as_deref(), &tags[3].1, None, "", None, "", None, "", format, false);
        let offset = rssp::parse::parse_offset_seconds(if own_timing_allowed {
            entry.chart_offset.as_deref().or(parsed.offset)
        } else { parsed.offset }) as f32;
        let timing = deadsync_rules::timing::TimingData::from_segments(-offset, 0.0,
            &crate::timing::timing_segments_from_rssp(&segments), &[]);
        let first = timing.get_time_for_beat(first as f32 / 48.0);
        let last = timing.get_time_for_beat(last as f32 / 48.0);
        if first.is_finite() && last.is_finite() {
            bounds = Some(bounds.map_or([first, last], |old| [old[0].min(first), old[1].max(last)]));
        }
    }
    bounds
}

fn native_note_row_bounds(data: &[u8], lanes: usize) -> Option<[i32; 2]> {
    // NoteDataUtil::LoadFromSMNoteDataStringWithPlayer keeps keysounds and fake
    // notes, resolves hold tails, and discards unterminated holds. Routine
    // player blocks share one native note grid.
    let mut tracks = vec![std::collections::BTreeMap::<i32, Option<i32>>::new(); lanes];
    for player in data.split(|byte| *byte == b'&') {
        for (measure, data) in player.split(|byte| *byte == b',').enumerate() {
            let rows: Vec<_> = data.split(|byte| *byte == b'\n').filter_map(|line| {
                let end = line.windows(2).position(|value| value == b"//").unwrap_or(line.len());
                let line = line[..end].trim_ascii();
                (!line.is_empty()).then_some(line)
            }).collect();
            for (index, line) in rows.iter().enumerate() {
                let beat = (measure as f32 + index as f32 / rows.len() as f32) * 4.0;
                let row = (beat * 48.0).round() as i32;
                let mut cursor = 0;
                for track in &mut tracks {
                    let Some(&cell) = line.get(cursor) else { break; };
                    cursor += 1;
                    if line.get(cursor) == Some(&b'[') {
                        cursor += 1;
                        while cursor < line.len() && line[cursor] != b']' { cursor += 1; }
                        cursor += usize::from(cursor < line.len());
                    }
                    match cell {
                        b'2' | b'4' => { track.insert(row, None); }
                        b'3' => {
                            if let Some((_, tail)) = track.range_mut(..row).rev().find(|(_, tail)| tail.is_none()) {
                                *tail = Some(row);
                            }
                        }
                        b'1' | b'M' | b'L' | b'F' | b'K' => { track.insert(row, Some(row)); }
                        _ => {},
                    }
                }
            }
        }
    }
    let mut rows = tracks.iter().flat_map(|track| track.iter()
        .filter_map(|(&head, tail)| tail.map(|tail| [head, tail])));
    let mut result = rows.next()?;
    for [first, last] in rows { result = [result[0].min(first), result[1].max(last)]; }
    Some(result)
}

pub fn parse_song_meta_file(
    path: &Path,
    options: &ParseSongOptions,
    global_offset_seconds: f32,
    music_len: impl FnOnce(Option<&Path>) -> f32,
) -> Result<SongData, String> {
    let song = parse_song_data_file(path, options, global_offset_seconds, music_len)?;
    Ok(build_song_meta(song, global_offset_seconds))
}

pub fn parse_song_data_file(
    path: &Path,
    options: &ParseSongOptions,
    _global_offset_seconds: f32,
    music_len: impl FnOnce(Option<&Path>) -> f32,
) -> Result<SerializableSongData, String> {
    parse_song_file(path, options, music_len)
}

pub fn parse_song_data_file_in(
    path: &Path,
    options: &ParseSongOptions,
    analyzer: &SongAnalyzer,
    scratch: &mut SongParseScratch,
    _global_offset_seconds: f32,
    music_len: impl FnOnce(Option<&Path>) -> f32,
) -> Result<SerializableSongData, String> {
    parse_song_file_in(path, options, analyzer, scratch, music_len)
}

/// What a song finds in its folder rather than in its simfile.
///
/// Kept together because it is all-or-nothing: a simfile parsed from disk
/// resolves every piece of it, and one parsed from memory resolves none.
#[derive(Default)]
struct SongFolderMedia {
    artwork: ResolvedSongArtwork,
    background_changes: Vec<SerializableSongBackgroundChange>,
    background_layer2_changes: Vec<SerializableSongBackgroundChange>,
    foreground_changes: Vec<SerializableSongForegroundChange>,
    background_lua_changes: Vec<SerializableSongBackgroundLuaChange>,
    foreground_lua_changes: Vec<SerializableSongForegroundLuaChange>,
    has_lua: bool,
}

impl SongFolderMedia {
    fn resolve(
        simfile_dir: &Path,
        simfile_data: &[u8],
        summary: &SimfileSummary,
        options: &ParseSongOptions,
    ) -> Self {
        let artwork = resolve_song_artwork_like_itg(
            simfile_dir,
            simfile_data,
            &summary.banner_path,
            &summary.background_path,
            &summary.cdtitle_path,
            &summary.jacket_path,
        );
        let background_lua_changes =
            extract_background_lua_change_set(simfile_dir, simfile_data, &summary.background_path);
        let foreground_changes = extract_foreground_change_sets(simfile_dir, simfile_data);
        let has_lua = background_lua_changes.uses_lua || foreground_changes.uses_lua;
        let background_changes = cache_background_changes(resolve_background_changes_from_roots(
            simfile_dir,
            simfile_data,
            &options.song_movie_roots,
            &options.random_movie_roots,
        ));
        let background_layer2_changes =
            cache_background_changes(resolve_background_layer2_changes_from_roots(
                simfile_dir,
                simfile_data,
                &options.song_movie_roots,
                &options.random_movie_roots,
                &options.bg_animation_roots,
            ));
        Self {
            artwork,
            background_changes,
            background_layer2_changes,
            foreground_changes: foreground_changes.media,
            background_lua_changes: background_lua_changes.changes,
            foreground_lua_changes: foreground_changes.lua,
            has_lua,
        }
    }
}

fn build_song_data(mut summary: SimfileSummary, input: SongBuildInput<'_>) -> SerializableSongData {
    let SongBuildInput {
        path,
        media_dir,
        simfile_data,
        song_music_path,
        music_length_seconds,
        options,
        parsed_notes,
    } = input;
    let charts = build_charts(
        &mut summary,
        media_dir,
        song_music_path.as_deref(),
        parsed_notes,
    );
    let SongFolderMedia {
        artwork,
        background_changes,
        background_layer2_changes,
        foreground_changes,
        background_lua_changes,
        foreground_lua_changes,
        has_lua,
    } = media_dir.map_or_else(SongFolderMedia::default, |dir| {
        SongFolderMedia::resolve(dir, simfile_data, &summary, options)
    });

    let last_second_hint = summary
        .last_second_hint
        .map(|seconds| seconds as f32)
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
        .unwrap_or(0.0);
    SerializableSongData {
        simfile_path: path.to_string_lossy().into_owned(),
        title: summary.title_str,
        subtitle: summary.subtitle_str,
        translit_title: summary.titletranslit_str,
        translit_subtitle: summary.subtitletranslit_str,
        artist: summary.artist_str,
        translit_artist: summary.artisttranslit_str,
        genre: summary.genre_str,
        banner_path: artwork
            .banner_path
            .map(|p| p.to_string_lossy().into_owned()),
        background_path: artwork
            .background_path
            .map(|p| p.to_string_lossy().into_owned()),
        background_changes,
        background_layer2_changes,
        foreground_changes,
        background_lua_changes,
        foreground_lua_changes,
        has_lua,
        cdtitle_path: artwork
            .cdtitle_path
            .map(|p| p.to_string_lossy().into_owned()),
        display_bpm: summary.display_bpm_str,
        offset: summary.offset as f32,
        sample_start: (summary.sample_start > 0.0).then_some(summary.sample_start as f32),
        sample_length: (summary.sample_length > 0.0).then_some(summary.sample_length as f32),
        min_bpm: summary.min_bpm,
        max_bpm: summary.max_bpm,
        normalized_bpms: summary.normalized_bpms,
        song_timing: has_lua.then(|| {
            CachedTimingSegments::from_rssp_owned(
                summary.global_timing_segments,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            )
        }),
        music_path: song_music_path.map(|p| p.to_string_lossy().into_owned()),
        music_length_seconds,
        first_second: 0.0,
        total_length_seconds: summary.total_length.max(last_second_hint as i32),
        precise_last_second_seconds: (summary.total_length.max(0) as f32).max(last_second_hint),
        precise_note_last_second_seconds: Some(summary.total_length.max(0) as f32),
        last_second_hint,
        charts,
    }
}

fn build_charts(
    summary: &mut SimfileSummary,
    media_dir: Option<&Path>,
    song_music_path: Option<&Path>,
    parsed_notes: &mut Vec<Vec<CachedParsedNote>>,
) -> Vec<SerializableChartData> {
    let charts = std::mem::take(&mut summary.charts);
    let global_time_signatures = summary.normalized_time_signatures.as_str();
    let global_tickcounts = summary.normalized_tickcounts.as_str();
    let global_combos = summary.normalized_combos.as_str();
    let allow_steps_timing =
        rssp::timing::steps_timing_allowed(summary.ssc_version, summary.timing_format);
    let mut parsed_notes = parsed_notes.drain(..);
    let mut charts: Vec<SerializableChartData> = charts
        .into_iter()
        .map(|chart| {
            let parsed_notes = parsed_notes
                .next()
                .expect("RSSP chart notes align with chart summaries");
            let chart_time_signatures = chart
                .chart_time_signatures
                .as_deref()
                .filter(|s| !s.trim().is_empty());
            let global_time_signatures =
                (!global_time_signatures.trim().is_empty()).then_some(global_time_signatures);
            let time_signature_tag = if allow_steps_timing && chart.chart_has_own_timing {
                chart_time_signatures
            } else if allow_steps_timing {
                chart_time_signatures.or(global_time_signatures)
            } else {
                global_time_signatures
            };
            let chart_tickcounts = chart
                .chart_tickcounts
                .as_deref()
                .filter(|s| !s.trim().is_empty());
            let global_tickcounts =
                (!global_tickcounts.trim().is_empty()).then_some(global_tickcounts);
            let tickcount_tag = if allow_steps_timing && chart.chart_has_own_timing {
                chart_tickcounts
            } else if allow_steps_timing {
                chart_tickcounts.or(global_tickcounts)
            } else {
                global_tickcounts
            };
            let chart_combos = chart
                .chart_combos
                .as_deref()
                .filter(|s| !s.trim().is_empty());
            let global_combos = (!global_combos.trim().is_empty()).then_some(global_combos);
            let combo_tag = if allow_steps_timing && chart.chart_has_own_timing {
                chart_combos
            } else if allow_steps_timing {
                chart_combos.or(global_combos)
            } else {
                global_combos
            };
            let stamina_counts = build_stamina_counts(&chart);
            let meter = chart.rating_str.parse().unwrap_or(0);
            let music_path =
                media_dir.and_then(|dir| chart_music_path(dir, song_music_path, &chart.music_path));
            let (min_bpm, max_bpm) = chart_bpm_bounds(&chart.timing_segments.bpms);
            let timing_segments = CachedTimingSegments::from_rssp_owned(
                chart.timing_segments,
                parse_cached_time_signatures(time_signature_tag),
                parse_cached_tickcounts(tickcount_tag),
                parse_cached_combos(combo_tag),
            );
            let stats = (&chart.stats).into();
            let tech_counts = (&chart.tech_counts).into();
            let stamina_counts = (&stamina_counts).into();
            let display_bpm = parse_chart_display_bpm(chart.chart_display_bpm.as_deref());
            SerializableChartData {
                chart_type: chart.step_type_str,
                difficulty: chart.difficulty_str,
                description: chart.description_str,
                chart_name: chart.chart_name_str,
                meter,
                step_artist: chart.step_artist_str,
                music_file: chart.music_path,
                music_path,
                offset: chart.chart_offset_seconds as f32,
                notes: chart.minimized_note_data,
                parsed_notes,
                row_to_beat: chart.row_to_beat,
                timing_segments,
                short_hash: chart.short_hash,
                stats,
                tech_counts,
                mines_nonfake: chart.mines_nonfake,
                stamina_counts,
                total_streams: chart.total_streams,
                total_measures: chart.total_measures,
                matrix_rating: chart.matrix_rating,
                matrix_profile: chart
                    .matrix_profile
                    .iter()
                    .copied()
                    .map(Into::into)
                    .collect(),
                max_nps: chart.max_nps,
                sn_detailed_breakdown: chart.sn_detailed_breakdown,
                sn_partial_breakdown: chart.sn_partial_breakdown,
                sn_simple_breakdown: chart.sn_simple_breakdown,
                detailed_breakdown: chart.detailed_breakdown,
                partial_breakdown: chart.partial_breakdown,
                simple_breakdown: chart.simple_breakdown,
                measure_nps_vec: chart.measure_nps_vec,
                chart_attacks: chart.chart_attacks,
                display_bpm,
                min_bpm,
                max_bpm,
            }
        })
        .collect();
    adjust_duplicate_charts(&mut charts);
    charts
}

/// Match `ITGmania`'s `SongUtil::AdjustDuplicateSteps`: within each standard
/// difficulty and step type, retain the easiest chart at that difficulty and
/// expose the others as edits. The chart vector itself stays in simfile order.
fn adjust_duplicate_charts(charts: &mut Vec<SerializableChartData>) {
    remove_identical_charts(charts);

    const STACK_GROUPS: usize = 16;
    let mut stack_groups = [(0usize, 0usize); STACK_GROUPS];
    let mut stack_len = 0usize;
    let mut overflow_groups = Vec::new();
    for (chart_index, chart) in charts.iter().enumerate() {
        let Some(difficulty) = standard_difficulty_index(&chart.difficulty) else {
            continue;
        };
        let seen = stack_groups[..stack_len]
            .iter()
            .chain(&overflow_groups)
            .any(|&(first, group_difficulty)| {
                group_difficulty == difficulty
                    && charts[first]
                        .chart_type
                        .eq_ignore_ascii_case(&chart.chart_type)
            });
        if seen {
            continue;
        }
        if stack_len < STACK_GROUPS {
            stack_groups[stack_len] = (chart_index, difficulty);
            stack_len += 1;
        } else {
            if overflow_groups.is_empty() {
                overflow_groups.reserve(charts.len().saturating_sub(STACK_GROUPS));
            }
            overflow_groups.push((chart_index, difficulty));
        }
    }

    for &(first, difficulty) in stack_groups[..stack_len].iter().chain(&overflow_groups) {
        adjust_duplicate_group(charts, first, difficulty);
    }
}

fn adjust_duplicate_group(charts: &mut [SerializableChartData], first: usize, difficulty: usize) {
    let difficulty_name = STANDARD_DIFFICULTY_NAMES[difficulty];
    let mut best = first;
    let mut count = 0usize;
    for index in 0..charts.len() {
        if charts[index]
            .chart_type
            .eq_ignore_ascii_case(&charts[first].chart_type)
            && charts[index]
                .difficulty
                .eq_ignore_ascii_case(difficulty_name)
        {
            count += 1;
            if duplicate_chart_cmp(&charts[index], &charts[best]).is_lt() {
                best = index;
            }
        }
    }
    if count < 2 {
        return;
    }

    for index in 0..charts.len() {
        let duplicate = index != best
            && charts[index]
                .chart_type
                .eq_ignore_ascii_case(&charts[first].chart_type)
            && charts[index]
                .difficulty
                .eq_ignore_ascii_case(difficulty_name);
        if duplicate {
            let chart = &mut charts[index];
            chart.difficulty = "Edit".to_string();
            if chart.description.is_empty() {
                chart.description = format!("{difficulty_name} Edit");
            }
        }
    }
}

fn remove_identical_charts(charts: &mut Vec<SerializableChartData>) {
    let mut write = 0usize;
    for read in 0..charts.len() {
        let chart = &charts[read];
        let is_standard = standard_difficulty_index(&chart.difficulty).is_some();
        let duplicate = is_standard
            && charts[..write].iter().any(|candidate| {
                candidate.chart_type.eq_ignore_ascii_case(&chart.chart_type)
                    && candidate.difficulty.eq_ignore_ascii_case(&chart.difficulty)
                    && candidate.description == chart.description
                    && candidate.step_artist == chart.step_artist
                    && candidate.meter == chart.meter
                    && candidate.notes == chart.notes
            });
        if !duplicate {
            charts.swap(write, read);
            write += 1;
        }
    }
    charts.truncate(write);
}

fn duplicate_chart_cmp(left: &SerializableChartData, right: &SerializableChartData) -> Ordering {
    left.meter
        .cmp(&right.meter)
        .then_with(|| left.stats.total_steps.cmp(&right.stats.total_steps))
        .then_with(|| lowercase_cmp(&left.description, &right.description))
}

fn lowercase_cmp(left: &str, right: &str) -> Ordering {
    left.chars()
        .flat_map(char::to_lowercase)
        .cmp(right.chars().flat_map(char::to_lowercase))
}

fn chart_music_path(
    simfile_dir: &Path,
    song_music_path: Option<&Path>,
    chart_music_tag: &str,
) -> Option<String> {
    if chart_music_tag.trim().is_empty() {
        return song_music_path.map(|path| path.to_string_lossy().into_owned());
    }
    resolve_music_path(simfile_dir, chart_music_tag).map(|path| path.to_string_lossy().into_owned())
}

fn resolve_music_path(simfile_dir: &Path, music_tag: &str) -> Option<PathBuf> {
    resolve_song_asset_path_like_itg(simfile_dir, music_tag)
        .or_else(|| rssp::assets::resolve_music_path_like_itg(simfile_dir, music_tag))
}

fn final_music_len(summary: &SimfileSummary, decoded_len: f32) -> f32 {
    let chart_length_seconds = summary.total_length.max(0) as f32;
    if decoded_len > 0.0 && chart_length_seconds > 0.0 && decoded_len < chart_length_seconds - 10.0
    {
        chart_length_seconds
    } else {
        decoded_len
    }
}

fn chart_bpm_bounds(bpms: &[(f32, f32)]) -> (f64, f64) {
    let (lo, hi) = bpms
        .iter()
        .map(|&(_, bpm)| f64::from(bpm))
        .filter(|bpm| bpm.is_finite() && *bpm > 0.0)
        .fold((f64::INFINITY, 0.0_f64), |(lo, hi), bpm| {
            (lo.min(bpm), hi.max(bpm))
        });
    (lo.min(f64::MAX), hi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn keysound_indices_preserve_note_columns_and_real_holds() {
        let root = test_dir("keysound-index-columns");
        let options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
        let notes = "1[24]000\n02[13]00\n0300\n004[23]0\n0030\n0001[4]\n0000\n0000\n";
        for (extension, chart) in [
            ("sm", format!("#NOTES2:dance-single:[12] Author:Challenge:1:0,0,0,0,0:\n{notes};")),
            ("ssc", format!("#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DESCRIPTION:[12] Author;\n\
                #DIFFICULTY:Challenge;\n#METER:1;\n#NOTES2:\n{notes};")),
        ] {
            let source = format!("#VERSION:0.83;\n#TITLE:[24] Keysounds;\n#BPMS:0=120;\n{chart}");
            let path = root.join(format!("song.{extension}"));
            fs::write(&path, &source).unwrap();
            for song in [parse_song_file(&path, &options, |_| 4.0).unwrap(),
                parse_song_bytes(source.as_bytes(), extension, &path, &options, |_| 4.0).unwrap()] {
                assert_eq!(song.title, "[24] Keysounds");
                let chart = &song.charts[0];
                assert_eq!(chart.description, "[12] Author");
                assert_eq!((chart.stats.total_steps, chart.stats.holds, chart.stats.rolls), (4, 1, 1),
                    "keysound index digits are metadata, not additional hold or roll notes");
                assert_eq!(chart.parsed_notes.len(), 4,
                    "keysound index digits must not create additional parsed note cells");
                assert_eq!(chart.parsed_notes.iter().map(|note| note.column).collect::<Vec<_>>(), [0, 1, 2, 3]);
                assert_eq!(chart.parsed_notes.iter().map(|note| NoteType::from(note.note_type)).collect::<Vec<_>>(),
                    [NoteType::Tap, NoteType::Hold, NoteType::Roll, NoteType::Tap]);
                for note in &chart.parsed_notes[1..3] {
                    assert!(note.tail_row_index.is_some_and(|tail| tail > note.row_index));
                }
            }
            assert_eq!(fs::read(&path).unwrap(), source.as_bytes());
        }
    }

    #[test]
    fn ssc_chart_description_preserves_non_ascii_whitespace() {
        let root = test_dir("ssc-description-whitespace");
        let simfile = root.join("metadata.ssc");
        let options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
        for (version, expected) in [("0.83", "\u{a0}Author: description\u{a0}"), ("0.60", "")] {
            let source = format!("#VERSION:{version};\n#TITLE:Metadata;\n#BPMS:0=120;\n\
                #NOTEDATA:;\n#STEPSTYPE:unknown-style;\n#DESCRIPTION:Skipped;\n#DIFFICULTY:Hard;\n#METER:1;\n#NOTES:1000;\n\
                #NOTEDATA:;\n#STEPSTYPE:dance-single;\n#CHARTNAME:Independent name;\n#CREDIT:Independent credit;\n\
                #DESCRIPTION:\t \u{a0}Author\\: description\u{a0} \t ;\n#DIFFICULTY:Hard;\n#METER:1;\n#NOTES:1000;");
            fs::write(&simfile, &source).unwrap();
            for song in [parse_song_file(&simfile, &options, |_| 0.0).unwrap(),
                parse_song_bytes(source.as_bytes(), "SSC", &simfile, &options, |_| 0.0).unwrap()] {
                assert_eq!(song.charts.len(), 1);
                assert_eq!(song.charts[0].description, expected);
                assert_eq!(song.charts[0].step_artist, "Independent credit");
                if version == "0.83" { assert_eq!(song.charts[0].chart_name, "Independent name"); }
                assert_eq!(song.charts[0].parsed_notes.len(), 1);
            }
        }
    }

    #[test]
    fn sm_chart_metadata_preserves_non_ascii_whitespace() {
        let root = test_dir("sm-metadata-whitespace");
        let simfile = root.join("metadata.sm");
        let source = "#TITLE:Metadata;\n#BPMS:0=120;\n\
            #NOTES:unknown-style:Skipped:Hard:1:0,0,0,0,0:1000;\n\
            #NOTES:dance-single:\r\n\t \u{a0}Author\\: credit\u{a0} \t\r\n:Hard:1:0,0,0,0,0:1000;\n\
            #NOTES:dance-double: \tSecond\t :Challenge:2:0,0,0,0,0:10000000;";
        fs::write(&simfile, source).unwrap();
        let options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
        let from_file = parse_song_file(&simfile, &options, |_| 0.0).unwrap();
        let from_bytes =
            parse_song_bytes(source.as_bytes(), "SM", &simfile, &options, |_| 0.0).unwrap();
        for song in [from_file, from_bytes] {
            assert_eq!(song.charts.len(), 2);
            for (chart, expected) in song
                .charts
                .iter()
                .zip(["\u{a0}Author: credit\u{a0}", "Second"])
            {
                assert_eq!(chart.description, expected);
                assert_eq!(chart.step_artist, expected);
                assert_eq!(chart.chart_name, expected);
                assert_eq!(chart.parsed_notes.len(), 1);
            }
        }
    }

    #[test]
    fn reused_analysis_matches_fresh_parsing_across_files() {
        let root = test_dir("reused-analysis");
        let large = root.join("large.ssc");
        let small = root.join("small.sm");
        fs::write(
            &large,
            b"#VERSION:0.83;\n\
              #TITLE:Reusable SSC;\n\
              #ARTIST:Boundary;\n\
              #BPMS:0.000=120.000,8.000=180.000;\n\
              #NOTEDATA:;\n\
              #STEPSTYPE:dance-single;\n\
              #DIFFICULTY:Challenge;\n\
              #METER:12;\n\
              #NOTES:\n\
              2000\n0100\n0010\n3000\n,\n1001\n0110\n1000\n0001\n;",
        )
        .unwrap();
        fs::write(
            &small,
            b"#TITLE:Reusable SM;\n\
              #BPMS:0.000=90.000;\n\
              #NOTES:\n\
              dance-single:\n:\nHard:\n5:\n0,0,0,0,0:\n1000\n;",
        )
        .unwrap();
        let options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
        let analyzer = SongAnalyzer::new(&options);
        let mut scratch = SongParseScratch::default();

        for path in [&large, &small, &large] {
            let expected = parse_song_file(path, &options, |_| 7.5).unwrap();
            let actual =
                parse_song_file_in(path, &options, &analyzer, &mut scratch, |_| 7.5).unwrap();
            let expected = bincode::encode_to_vec(expected, bincode::config::standard()).unwrap();
            let actual = bincode::encode_to_vec(actual, bincode::config::standard()).unwrap();
            assert_eq!(actual, expected, "reused parse diverged for {path:?}");
        }

        let large_len = usize::try_from(fs::metadata(large).unwrap().len()).unwrap();
        assert!(scratch.input.capacity() >= large_len);

        let mut mismatched = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
        mismatched.mono_threshold += 1;
        let error = parse_song_file_in(&small, &mismatched, &analyzer, &mut scratch, |_| 0.0).err();
        assert_eq!(
            error.as_deref(),
            Some("Song analyzer does not match parse options")
        );
    }

    #[test]
    fn parses_song_payload_with_injected_music_length() {
        let root = test_dir("payload");
        let song_dir = root.join("Song");
        fs::create_dir_all(&song_dir).unwrap();
        let simfile = song_dir.join("song.sm");
        let music = song_dir.join("music.ogg");
        fs::write(&music, b"stub").unwrap();
        fs::write(
            &simfile,
            b"#TITLE:Payload;\n\
              #ARTIST:Artist;\n\
              #MUSIC:music.ogg;\n\
              #BPMS:0.000=60.000;\n\
              #OFFSET:0.000;\n\
              #NOTES:\n\
              dance-single:\n\
              :\n\
              Challenge:\n\
              1:\n\
              0.000,0.000,0.000,0.000,0.000:\n\
              1000\n\
              ;",
        )
        .unwrap();
        let options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());

        let song = parse_song_file(&simfile, &options, |_| 12.5).unwrap();

        assert_eq!(song.title, "Payload");
        assert_eq!(song.artist, "Artist");
        assert_eq!(PathBuf::from(song.music_path.unwrap()), music);
        assert_eq!(song.music_length_seconds, 12.5);
        assert_eq!(song.charts.len(), 1);
        assert_eq!(song.charts[0].meter, 1);
    }

    #[test]
    fn chart_offset_survives_rssp_handoff() {
        let root = test_dir("chart-local-offset");
        let simfile = root.join("song.ssc");
        fs::write(
            &simfile,
            b"#VERSION:0.83;\n\
              #TITLE:Chart Local Offset;\n\
              #OFFSET:0.000;\n\
              #BPMS:0.000=120.000;\n\
              #NOTEDATA:;\n\
              #STEPSTYPE:dance-single;\n\
              #DIFFICULTY:Challenge;\n\
              #METER:1;\n\
              #OFFSET:1.502;\n\
              #BPMS:0.000=120.000;\n\
              #WARPS:12.000=8.000,22.000=8.000,32.000=8.000;\n\
              #SCROLLS:0.000=1.000;\n\
              #FAKES:12.000=8.000,22.000=8.000,32.000=8.000;\n\
              #NOTES:\n\
              1000\n\
              ;",
        )
        .unwrap();
        let options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());

        let song = parse_song_file(&simfile, &options, |_| 0.0).unwrap();
        assert_eq!(song.offset, 0.0);
        assert!((song.charts[0].offset - 1.502).abs() < 1e-6);

        let chart = crate::cache::build_requested_gameplay_charts(&song, &[0], 0.0)
            .unwrap()
            .pop()
            .unwrap();
        assert!((chart.timing.get_time_for_beat(12.0) - 4.498).abs() < 1e-5);
        assert!((chart.timing.get_time_for_beat(42.0) - 7.498).abs() < 1e-5);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parses_pump_charts_into_song_payload() {
        let root = test_dir("pump-chart");
        let simfile = root.join("song.ssc");
        fs::write(
            &simfile,
            b"#VERSION:0.83 StepPrime;\n\
              #TITLE:Pump Chart;\n\
              #BPMS:0.000=120.000;\n\
              #NOTEDATA:;\n\
              #STEPSTYPE:pump-single;\n\
              #DIFFICULTY:Challenge;\n\
              #METER:10;\n\
              #TICKCOUNTS:0.000=4.000\n\
              ,46.000=1.000\n\
              ,47.000=16.000\n\
              ,50.000=4.000\n\
              ;\n\
              #COMBOS:0.000=3.000=2.000;\n\
              #NOTES:\n\
              10000\n\
              00100\n\
              00001\n\
              ;",
        )
        .unwrap();
        let options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());

        let song = parse_song_file(&simfile, &options, |_| 0.0).unwrap();

        assert_eq!(song.charts.len(), 1);
        assert_eq!(song.charts[0].chart_type, "pump-single");
        assert_eq!(song.charts[0].parsed_notes.len(), 3);
        assert_eq!(song.charts[0].parsed_notes[1].column, 2);
        let timing =
            deadsync_rules::timing::TimingSegments::from(song.charts[0].timing_segments.clone());
        assert_eq!(
            timing
                .tickcounts
                .iter()
                .map(|segment| (segment.beat, segment.ticks))
                .collect::<Vec<_>>(),
            [(0.0, 4), (46.0, 1), (47.0, 16), (50.0, 4)]
        );
        assert_eq!(timing.combos[0].combo, 3);
        assert_eq!(timing.combos[0].miss_combo, 2);
    }

    #[test]
    fn duplicate_pump_difficulties_become_meter_sorted_edits() {
        let root = test_dir("pump-duplicate-difficulty");
        let simfile = root.join("song.ssc");
        fs::write(
            &simfile,
            br#"#VERSION:0.83;
#TITLE:Duplicate Pump Difficulty;
#BPMS:0.000=120.000;
#NOTEDATA:;
#STEPSTYPE:pump-single;
#DIFFICULTY:Challenge;
#METER:20;
#NOTES:
10000
;
#NOTEDATA:;
#STEPSTYPE:pump-single;
#DIFFICULTY:Challenge;
#METER:3;
#NOTES:
01000
;
#NOTEDATA:;
#STEPSTYPE:pump-single;
#DIFFICULTY:Challenge;
#METER:15;
#NOTES:
00100
;
"#,
        )
        .unwrap();
        let options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());

        let song = parse_song_file(&simfile, &options, |_| 0.0).unwrap();

        assert_eq!(
            song.charts
                .iter()
                .map(|chart| chart.meter)
                .collect::<Vec<_>>(),
            [20, 3, 15]
        );
        assert_eq!(
            song.charts
                .iter()
                .map(|chart| chart.difficulty.as_str())
                .collect::<Vec<_>>(),
            ["Edit", "Challenge", "Edit"]
        );
        assert_eq!(song.charts[0].description, "Challenge Edit");
        assert!(song.charts[1].description.is_empty());
        assert_eq!(song.charts[2].description, "Challenge Edit");

        let song = build_song_meta(song, 0.0);
        assert_eq!(
            song.chart_for_steps_index("pump-single", 4)
                .map(|chart| chart.meter),
            Some(3)
        );
        assert_eq!(
            song.edit_charts_sorted("pump-single")
                .into_iter()
                .map(|chart| chart.meter)
                .collect::<Vec<_>>(),
            [15, 20]
        );
    }

    #[test]
    fn identical_standard_charts_are_removed_before_edit_conversion() {
        let root = test_dir("identical-standard-charts");
        let simfile = root.join("song.ssc");
        fs::write(
            &simfile,
            br#"#VERSION:0.83;
#TITLE:Identical Charts;
#BPMS:0.000=120.000;
#NOTEDATA:;
#STEPSTYPE:dance-single;
#DESCRIPTION:Same;
#CREDIT:Author;
#DIFFICULTY:Hard;
#METER:9;
#NOTES:
1000
;
#NOTEDATA:;
#STEPSTYPE:dance-single;
#DESCRIPTION:Same;
#CREDIT:Author;
#DIFFICULTY:Hard;
#METER:9;
#NOTES:
1000
;
"#,
        )
        .unwrap();
        let options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());

        let song = parse_song_file(&simfile, &options, |_| 0.0).unwrap();

        assert_eq!(song.charts.len(), 1);
        assert_eq!(song.charts[0].difficulty, "Hard");
    }

    #[test]
    fn simple_lua_song_keeps_native_clock() {
        let root = test_dir("simple-lua-native-clock");
        let simfile = root.join("song.sm");
        fs::write(root.join("default.lua"), "return Def.Actor{}").expect("Lua layer");
        fs::write(
            &simfile,
            b"#TITLE:Native clock;\n#OFFSET:-2.296;\n#BPMS:0=140;\n\
            #FGCHANGES:0=default.lua;\n#NOTES:dance-single::Challenge:1:0,0,0,0,0:\n\
            1000\n0100\n0010\n0001\n;",
        )
        .expect("simple simfile");
        let options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
        let data = parse_song_file(&simfile, &options, |_| 2.0).expect("song data");
        let song = build_song_meta(data, 0.0);
        let timing = song
            .song_timing
            .expect("all Lua songs retain native timing");
        let origin = timing.get_time_for_beat_exact(0.0);
        // Native float cancellation leaves frame 6435 below beat 250.25.
        assert_eq!(
            timing.get_song_position(origin + 6435.0 / 60.0).beat,
            250.249_98
        );
        assert!(timing.get_song_position(origin + 6436.0 / 60.0).beat > 250.25);
    }

    #[test]
    fn song_clock_keeps_authored_offset_with_split_timing() {
        let root = test_dir("authored-song-offset");
        let simfile = root.join("song.ssc");
        fs::write(root.join("default.lua"), "return Def.Actor{}").expect("Lua layer");
        fs::write(
            &simfile,
            b"#VERSION:0.83;\n#TITLE:Authored offset;\n\
            #OFFSET:0.065760;\n#BPMS:0=144;\n#STOPS:4=0.5;\n\
            #FGCHANGES:0=default.lua;\n#NOTEDATA:;\n#STEPSTYPE:dance-single;\n\
            #DIFFICULTY:Challenge;\n#METER:1;\n#OFFSET:0.25;\n#BPMS:0=144;\n\
            #NOTES:\n1000\n0100\n0010\n0001\n;",
        )
        .expect("split timing simfile");
        let options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
        let data = parse_song_file(&simfile, &options, |_| 2.0).expect("song data");
        assert_eq!(data.charts[0].offset, 0.25);
        let song = build_song_meta(data, 0.0);
        assert_eq!(song.offset, 0.065760);
        let timing = song.song_timing.expect("global Lua clock");
        let origin = timing.get_time_for_beat_exact(0.0);
        assert_eq!(origin, -0.065760);
        // Native float arithmetic crosses beat one on frame 25, rather
        // than waiting for frame 26 after report rounding to 0.066.
        assert_eq!(
            timing.get_song_position(origin + 25.0 / 60.0).beat,
            1.0_f32.next_up()
        );
        fs::remove_dir_all(root).expect("remove offset fixture");
    }

    #[test]
    fn parsed_song_meta_records_first_and_last_step_seconds() {
        let root = test_dir("first-second");
        let song_dir = root.join("Song");
        fs::create_dir_all(&song_dir).unwrap();
        let simfile = song_dir.join("song.sm");
        fs::write(
            &simfile,
            b"#TITLE:First Second;\n\
              #BPMS:0.000=60.000;\n\
              #OFFSET:0.000;\n\
              #NOTES:\n\
              dance-single:\n\
              :\n\
              Challenge:\n\
              1:\n\
              0.000,0.000,0.000,0.000,0.000:\n\
              0000\n\
              ,\n\
              1000\n\
              ,\n\
              0001\n\
              ;",
        )
        .unwrap();
        let options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());

        let song = parse_song_meta_file(&simfile, &options, 0.0, |_| 0.0).unwrap();

        assert!((song.precise_first_second() - 4.0).abs() <= 1e-6);
        assert!((song.precise_last_second() - 8.0).abs() <= 1e-6);
    }

    #[test]
    fn unplayed_native_charts_extend_song_bounds_without_becoming_playable() {
        let root = test_dir("unplayed-note-bounds");
        fs::create_dir_all(&root).unwrap();
        let simfile = root.join("bounds.ssc");
        fs::write(&simfile, "#VERSION:0.83;#TITLE:Unplayed bounds;#BPMS:0=60;\n\
#NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Challenge;#METER:1;#NOTES:\n1000\n,\n1000\n;\n\
#NOTEDATA:;#STEPSTYPE:dance-solo;#DIFFICULTY:Medium;#METER:1;#BPMS:0=60;#STOPS:4=0.25;#NOTES:\n000000\n001000\n000000\n000000\n,\n000000\n,\nM00000\n;\n\
#NOTEDATA:;#STEPSTYPE:dance-solo;#DIFFICULTY:Edit;#METER:1;#NOTES:\n000000\n,\n000000\n,\n000000\n,\n100000\n;\n\
#NOTEDATA:;#STEPSTYPE:lights-cabinet;#DIFFICULTY:Hard;#METER:1;#NOTES:\n00000000\n,\n00000000\n,\n00000000\n,\n10000000\n;").unwrap();
        let options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
        for song in [parse_song_data_file(&simfile, &options, 0.125, |_| 20.0).unwrap(),
            parse_song_bytes(&fs::read(&simfile).unwrap(), "ssc", &simfile, &options, |_| 20.0).unwrap()] {
            assert_eq!(song.charts.len(), 1, "unplayed charts do not enter gameplay selection");
            assert_eq!(song.first_second, 0.0);
            assert_eq!(song.precise_note_last_second_seconds, Some(8.25));
            assert_eq!(song.precise_last_second_seconds, 8.25);
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_note_bounds_resolve_holds_keysounds_and_routine_players() {
        assert_eq!(native_note_row_bounds(b"000000\n200000\n000000\n300000\n,\n00000K[12]\n", 6), Some([48, 192]));
        assert_eq!(native_note_row_bounds(b"200000\n,\n000000\n", 6), None);
        assert_eq!(native_note_row_bounds(b"10000000\n,\n00000000\n&\n00000000\n,\n0000000F\n", 8), Some([0, 192]));
    }

    #[test]
    fn last_second_hint_extends_ssc_song_bounds() {
        let root = test_dir("last-second-hint");
        let simfile = root.join("song.ssc");
        let options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
        for (tag, hint, end) in [
            ("", 0.0, 8.0),
            ("#LASTSECONDHINT:12.75;", 12.75, 12.75),
            ("#LASTSECONDHINT:6;", 6.0, 8.0),
            ("#LASTSECONDHINT:0;", 0.0, 8.0),
            ("#LASTSECONDHINT:-10;", 0.0, 8.0),
            ("#LASTSECONDHINT:garbage;", 0.0, 8.0),
            ("#LASTSECONDHINT:NaN;", 0.0, 8.0),
            ("#LASTSECONDHINT:inf;", 0.0, 8.0),
            ("#LASTSECONDHINT:1e100;", 0.0, 8.0),
        ] {
            fs::write(
                &simfile,
                format!(
                    "#VERSION:0.83;\n#TITLE:Hint;\n#BPMS:0=60;\n{tag}\n\
                     #NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Challenge;\n\
                     #METER:1;\n#NOTES:\n0000\n,\n2000\n,\n3000\n;"
                ),
            )
            .unwrap();
            let data = parse_song_data_file(&simfile, &options, 0.0, |_| 5.0).unwrap();
            assert_eq!(data.last_second_hint, hint, "{tag}");
            let song = build_song_meta(data, 0.0);
            assert_eq!(song.last_second_hint, hint, "{tag}");
            assert_eq!(song.precise_first_second(), 4.0, "{tag}");
            assert_eq!(song.precise_last_second(), end, "{tag}");
            assert_eq!(song.total_length_seconds, end as i32, "{tag}");
            assert_eq!(song.music_length_seconds, 5.0);
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn last_second_hint_is_ignored_in_sm_like_itg() {
        let root = test_dir("sm-last-second-hint");
        let simfile = root.join("song.sm");
        fs::write(
            &simfile,
            "#TITLE:Hint;\n#BPMS:0=60;\n#LASTSECONDHINT:12.75;\n\
             #NOTES:dance-single::Challenge:1:0,0,0,0,0:\n1000\n,\n0100\n;",
        )
        .unwrap();
        let options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
        let song = parse_song_meta_file(&simfile, &options, 0.0, |_| 5.0).unwrap();
        assert_eq!(song.last_second_hint, 0.0);
        assert_eq!(song.precise_last_second(), 4.0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn in_memory_parse_matches_the_file_parse() {
        // A folder with nothing in it but the simfile, so the file parse finds
        // no media either and the two must agree to the byte.
        let root = test_dir("bytes-match");
        let options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
        for (name, data) in [
            (
                "song.ssc",
                b"#VERSION:0.83;\n#TITLE:Bytes;\n#ARTIST:Memory;\n#OFFSET:-0.250;\n\
                  #SAMPLESTART:12.5;\n#SAMPLELENGTH:15;\n#BPMS:0.000=120.000,8.000=180.000;\n\
                  #STOPS:4.000=0.500;\n#NOTEDATA:;\n#STEPSTYPE:dance-single;\n\
                  #DIFFICULTY:Challenge;\n#METER:12;\n#NOTES:\n\
                  2000\n0100\n0010\n3000\n,\n1001\n0M10\n1000\n000L\n;\n\
                  #NOTEDATA:;\n#STEPSTYPE:dance-double;\n#DIFFICULTY:Hard;\n#METER:9;\n\
                  #WARPS:2.000=1.000;\n#NOTES:\n10000001\n01000010\n00100100\n00011000\n;"
                    .as_slice(),
            ),
            (
                "song.sm",
                b"#TITLE:Bytes SM;\n#BPMS:0.000=90.000;\n#OFFSET:0.100;\n#NOTES:\n\
                  dance-single:\n:\nHard:\n5:\n0,0,0,0,0:\n1000\n0100\n0010\n0001\n;"
                    .as_slice(),
            ),
        ] {
            let path = root.join(name);
            fs::write(&path, data).unwrap();
            let extension = name.rsplit('.').next().unwrap();
            let from_file = parse_song_file(&path, &options, |_| 7.5).unwrap();
            let from_bytes = parse_song_bytes(data, extension, &path, &options, |music| {
                assert_eq!(music, None, "an in-memory parse names no music file");
                7.5
            })
            .unwrap();
            let from_file = bincode::encode_to_vec(from_file, bincode::config::standard()).unwrap();
            let from_bytes =
                bincode::encode_to_vec(from_bytes, bincode::config::standard()).unwrap();
            assert_eq!(from_bytes, from_file, "{name}");
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn in_memory_parse_never_looks_on_disk() {
        // The label points at a real folder that holds every file the simfile
        // names, by relative and by absolute path. The file parse finds them;
        // the in-memory parse must not even look.
        let root = test_dir("bytes-no-disk");
        let music = root.join("music.ogg");
        let banner = root.join("banner.png");
        let background = root.join("elsewhere.png");
        fs::write(&music, b"stub").unwrap();
        fs::write(&banner, b"stub").unwrap();
        fs::write(&background, b"stub").unwrap();
        let data = format!(
            "#TITLE:Stay Put;\n#MUSIC:music.ogg;\n#BANNER:banner.png;\n#BACKGROUND:{};\n\
             #BGCHANGES:0.000=banner.png=1.000=0=0=1;\n#FGCHANGES:0.000=banner.png=1.000=0=0=1;\n\
             #BPMS:0.000=120.000;\n#NOTES:\ndance-single:\n:\nHard:\n5:\n0,0,0,0,0:\n1000\n;",
            // A backslash is an escape in a simfile, so name it the portable way.
            background.to_string_lossy().replace('\\', "/")
        );
        let simfile = root.join("song.sm");
        fs::write(&simfile, &data).unwrap();
        let options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());

        let from_file = parse_song_file(&simfile, &options, |_| 0.0).unwrap();
        assert_eq!(from_file.music_path.map(PathBuf::from), Some(music));
        assert_eq!(from_file.banner_path.map(PathBuf::from), Some(banner));

        let from_bytes = parse_song_bytes(data.as_bytes(), "SM", &simfile, &options, |music| {
            assert_eq!(music, None);
            0.0
        })
        .unwrap();
        assert_eq!(from_bytes.simfile_path, simfile.to_string_lossy());
        assert_eq!(from_bytes.title, "Stay Put");
        assert_eq!(from_bytes.music_path, None);
        assert_eq!(from_bytes.banner_path, None);
        assert_eq!(from_bytes.background_path, None);
        assert_eq!(from_bytes.cdtitle_path, None);
        assert!(from_bytes.background_changes.is_empty());
        assert!(from_bytes.background_layer2_changes.is_empty());
        assert!(from_bytes.foreground_changes.is_empty());
        assert!(!from_bytes.has_lua);
        assert_eq!(from_bytes.charts.len(), 1);
        assert_eq!(from_bytes.charts[0].music_path, None);
        assert_eq!(from_bytes.charts[0].parsed_notes.len(), 1);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn in_memory_parse_reports_an_unknown_dialect() {
        let options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
        let error = parse_song_bytes(b"#TITLE:x;", "dwi", Path::new(""), &options, |_| 0.0)
            .err()
            .unwrap();
        assert!(error.contains(".sm or .ssc"), "{error}");
    }

    fn test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "deadsync-simfile-song-{name}-{}-{nanos}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    mod library_perf {
        include!("song_library_perf.rs");
    }
}
