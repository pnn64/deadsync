// Frozen implementations from 9710f833e6e582817b28532e6217152186400fa0.
fn build_gameplay_chart_from_ref(
    chart: &SerializableChartData,
    global_offset_seconds: f32,
) -> GameplayChartData {
    let timing_segments: TimingSegments = chart.timing_segments.clone().into();
    let timing = TimingData::from_segments(
        -chart.offset,
        global_offset_seconds,
        &timing_segments,
        &chart.row_to_beat,
    );
    GameplayChartData {
        notes: chart.notes.clone(),
        parsed_notes: chart
            .parsed_notes
            .iter()
            .cloned()
            .map(ParsedNote::from)
            .collect(),
        row_to_beat: chart.row_to_beat.clone(),
        timing_segments,
        timing,
        chart_attacks: chart.chart_attacks.clone(),
    }
}

pub fn build_requested_gameplay_charts(
    song: &SerializableSongData,
    requested_chart_ixs: &[usize],
    global_offset_seconds: f32,
) -> Result<Vec<GameplayChartData>, String> {
    requested_chart_ixs
        .iter()
        .map(|&chart_ix| {
            let chart = song
                .charts
                .get(chart_ix)
                .ok_or_else(|| format!("Chart index {chart_ix} out of range"))?;
            Ok(build_gameplay_chart_from_ref(chart, global_offset_seconds))
        })
        .collect()
}

#[derive(Encode)]
struct BorrowedCachedSongMeta<'a> {
    simfile_path: &'a str,
    title: &'a str,
    subtitle: &'a str,
    translit_title: &'a str,
    translit_subtitle: &'a str,
    artist: &'a str,
    translit_artist: &'a str,
    genre: &'a str,
    banner_path: Option<&'a str>,
    background_path: Option<&'a str>,
    background_changes: &'a [SerializableSongBackgroundChange],
    background_layer2_changes: &'a [SerializableSongBackgroundChange],
    foreground_changes: &'a [SerializableSongForegroundChange],
    background_lua_changes: &'a [SerializableSongBackgroundLuaChange],
    foreground_lua_changes: &'a [SerializableSongForegroundLuaChange],
    has_lua: bool,
    cdtitle_path: Option<&'a str>,
    music_path: Option<&'a str>,
    display_bpm: &'a str,
    offset: f32,
    sample_start: Option<f32>,
    sample_length: Option<f32>,
    min_bpm: f64,
    max_bpm: f64,
    normalized_bpms: &'a str,
    song_timing: Option<&'a CachedTimingSegments>,
    music_length_seconds: f32,
    first_second: f32,
    total_length_seconds: i32,
    precise_last_second_seconds: f32,
    last_second_hint: f32,
    charts: Vec<BorrowedCachedChartMeta<'a>>,
}

#[derive(Encode)]
struct BorrowedCachedSong<'a> {
    cache_version: u8,
    rssp_version: &'static str,
    mono_threshold: usize,
    directory_hash: u64,
    data: BorrowedCachedSongMeta<'a>,
    chart_payloads: &'a [CachedChartPayloadIndex],
}

impl<'a> BorrowedCachedSongMeta<'a> {
    fn new(song: &'a SerializableSongData, global_offset_seconds: f32) -> Self {
        Self {
            simfile_path: &song.simfile_path,
            title: &song.title,
            subtitle: &song.subtitle,
            translit_title: &song.translit_title,
            translit_subtitle: &song.translit_subtitle,
            artist: &song.artist,
            translit_artist: &song.translit_artist,
            genre: &song.genre,
            banner_path: song.banner_path.as_deref(),
            background_path: song.background_path.as_deref(),
            background_changes: &song.background_changes,
            background_layer2_changes: &song.background_layer2_changes,
            foreground_changes: &song.foreground_changes,
            background_lua_changes: &song.background_lua_changes,
            foreground_lua_changes: &song.foreground_lua_changes,
            has_lua: song.has_lua,
            cdtitle_path: song.cdtitle_path.as_deref(),
            music_path: song.music_path.as_deref(),
            display_bpm: &song.display_bpm,
            offset: song.offset,
            sample_start: song.sample_start,
            sample_length: song.sample_length,
            min_bpm: song.min_bpm,
            max_bpm: song.max_bpm,
            normalized_bpms: &song.normalized_bpms,
            song_timing: song.song_timing.as_ref(),
            music_length_seconds: song.music_length_seconds,
            first_second: song.first_second,
            total_length_seconds: song.total_length_seconds,
            precise_last_second_seconds: song.precise_last_second_seconds,
            last_second_hint: song.last_second_hint,
            charts: song
                .charts
                .iter()
                .map(|chart| BorrowedCachedChartMeta::new(chart, global_offset_seconds))
                .collect(),
        }
    }
}

pub(super) fn encode_song_cache_header(
    data: &SerializableSongData,
    global_offset_seconds: f32,
    directory_hash: u64,
    chart_payloads: &[CachedChartPayloadIndex],
    encoded_header: &mut Vec<u8>,
) -> Result<(), bincode::error::EncodeError> {
    let cached_song = BorrowedCachedSong {
        cache_version: SONG_CACHE_VERSION,
        rssp_version: rssp::RSSP_VERSION,
        mono_threshold: SONG_ANALYSIS_MONO_THRESHOLD,
        directory_hash,
        data: BorrowedCachedSongMeta::new(data, global_offset_seconds),
        chart_payloads,
    };
    bincode::encode_into_vec(&cached_song, encoded_header, bincode::config::standard())
}

pub(super) fn cached_song_paths_exist(song: &CachedSong) -> bool {
    let data = &song.data;
    let bgchange_paths_ok = data
        .background_changes
        .iter()
        .chain(data.background_layer2_changes.iter())
        .all(|change| {
            let target_ok = match &change.target {
                SerializableSongBackgroundChangeTarget::File(path) => {
                    cached_path_exists(Some(path))
                }
                SerializableSongBackgroundChangeTarget::Animation(_) => true,
                SerializableSongBackgroundChangeTarget::NoSongBg
                | SerializableSongBackgroundChangeTarget::Random => true,
            };
            target_ok && cached_path_exists(change.file2.as_deref())
        });
    let foreground_paths_ok = data
        .foreground_changes
        .iter()
        .all(|change| cached_path_exists(Some(&change.path)));
    let foreground_lua_paths_ok = data
        .foreground_lua_changes
        .iter()
        .all(|change| cached_path_exists(Some(&change.path)));
    let background_lua_paths_ok = data
        .background_lua_changes
        .iter()
        .all(|change| cached_path_exists(Some(&change.path)));
    let chart_music_paths_ok = data
        .charts
        .iter()
        .all(|chart| cached_path_exists(chart.music_path.as_deref()));
    cached_path_exists(data.banner_path.as_deref())
        && cached_path_exists(data.background_path.as_deref())
        && bgchange_paths_ok
        && foreground_paths_ok
        && background_lua_paths_ok
        && foreground_lua_paths_ok
        && chart_music_paths_ok
        && cached_path_exists(data.cdtitle_path.as_deref())
        && cached_path_exists(data.music_path.as_deref())
}
