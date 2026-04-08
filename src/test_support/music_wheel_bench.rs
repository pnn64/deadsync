use crate::engine::present::actors::Actor;
use crate::game::chart::{ChartData, StaminaCounts};
use crate::game::song::SongData;
use crate::screens::components::select_music::music_wheel::{self, MusicWheelParams};
use crate::screens::select_music::MusicWheelEntry;
use rssp::TechCounts;
use rssp::stats::ArrowStats;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

pub const SCENARIO_NAME: &str = "music-wheel";

pub struct MusicWheelBenchFixture {
    entries: Vec<MusicWheelEntry>,
    pack_song_counts: HashMap<String, usize>,
    song_text_color_overrides: HashMap<usize, [f32; 4]>,
    song_has_edit_ptrs: HashSet<usize>,
    selected_index: usize,
    position_offset_from_selection: f32,
    selection_animation_timer: f32,
    selection_animation_beat: f32,
    preferred_difficulty_index: usize,
    selected_steps_index: usize,
}

impl MusicWheelBenchFixture {
    pub fn build(&self) -> Vec<Actor> {
        music_wheel::build(MusicWheelParams {
            entries: &self.entries,
            selected_index: self.selected_index,
            position_offset_from_selection: self.position_offset_from_selection,
            selection_animation_timer: self.selection_animation_timer,
            selection_animation_beat: self.selection_animation_beat,
            pack_song_counts: &self.pack_song_counts,
            color_pack_headers: true,
            preferred_difficulty_index: self.preferred_difficulty_index,
            selected_steps_index: self.selected_steps_index,
            song_box_color: None,
            song_text_color: Some([0.95, 0.96, 1.0, 1.0]),
            song_text_color_overrides: Some(&self.song_text_color_overrides),
            song_has_edit_ptrs: Some(&self.song_has_edit_ptrs),
            show_music_wheel_grades: false,
            show_music_wheel_lamps: false,
            itl_wheel_mode: crate::config::SelectMusicItlWheelMode::Off,
            allow_online_fetch: false,
            new_pack_names: None,
        })
    }
}

pub fn fixture() -> MusicWheelBenchFixture {
    let mut entries = Vec::with_capacity(36);
    let mut pack_song_counts = HashMap::with_capacity(4);
    let mut song_text_color_overrides = HashMap::with_capacity(10);
    let mut song_has_edit_ptrs = HashSet::with_capacity(12);
    let pack_names = ["Stamina Lab", "Tech Alley", "Groove Works", "Night Shift"];

    for (pack_idx, pack_name) in pack_names.iter().enumerate() {
        entries.push(MusicWheelEntry::PackHeader {
            name: (*pack_name).to_string(),
            original_index: pack_idx,
            banner_path: None,
        });
        pack_song_counts.insert((*pack_name).to_string(), 7);

        for song_idx in 0..7 {
            let song = bench_song(pack_idx, song_idx);
            let song_ptr = Arc::as_ptr(&song) as usize;
            if song_idx % 2 == 0 {
                song_text_color_overrides
                    .insert(song_ptr, [0.86 + song_idx as f32 * 0.01, 0.94, 1.0, 1.0]);
            }
            if song.charts.iter().any(|chart| {
                chart.chart_type.eq_ignore_ascii_case("dance-single")
                    && chart.difficulty.eq_ignore_ascii_case("edit")
            }) {
                song_has_edit_ptrs.insert(song_ptr);
            }
            entries.push(MusicWheelEntry::Song(song));
        }
    }

    MusicWheelBenchFixture {
        entries,
        pack_song_counts,
        song_text_color_overrides,
        song_has_edit_ptrs,
        selected_index: 11,
        position_offset_from_selection: 0.35,
        selection_animation_timer: 1.375,
        selection_animation_beat: 37.5,
        preferred_difficulty_index: 3,
        selected_steps_index: 4,
    }
}

fn bench_song(pack_idx: usize, song_idx: usize) -> Arc<SongData> {
    let has_subtitle = !song_idx.is_multiple_of(3);
    let has_edit = song_idx.is_multiple_of(3);
    let base = format!("P{}-{:02}", pack_idx + 1, song_idx + 1);
    let title = format!("Benchmark {base} Velocity");
    let subtitle = if has_subtitle {
        format!("Phase {}", (song_idx % 4) + 1)
    } else {
        String::new()
    };
    Arc::new(SongData {
        simfile_path: PathBuf::from(format!("songs/Bench/{base}.ssc")),
        title: title.clone(),
        subtitle: subtitle.clone(),
        translit_title: String::new(),
        translit_subtitle: String::new(),
        artist: format!("Bench Artist {}", pack_idx + 1),
        banner_path: None,
        background_path: None,
        background_changes: Vec::new(),
        foreground_lua_changes: Vec::new(),
        has_lua: song_idx.is_multiple_of(4),
        cdtitle_path: None,
        music_path: None,
        display_bpm: String::from("160"),
        offset: 0.0,
        sample_start: None,
        sample_length: None,
        min_bpm: 160.0,
        max_bpm: 160.0,
        normalized_bpms: String::from("0.000=160.000"),
        music_length_seconds: 92.0 + song_idx as f32,
        total_length_seconds: 92 + song_idx as i32,
        precise_last_second_seconds: 92.0 + song_idx as f32,
        charts: bench_charts(&base, has_edit),
    })
}

fn bench_charts(base: &str, has_edit: bool) -> Vec<ChartData> {
    let mut charts = Vec::with_capacity(2 + usize::from(has_edit));
    charts.push(bench_chart(base, "hard", 9));
    charts.push(bench_chart(base, "challenge", 13));
    if has_edit {
        charts.push(bench_chart(base, "edit", 14));
    }
    charts
}

fn bench_chart(base: &str, difficulty: &str, meter: u32) -> ChartData {
    ChartData {
        chart_type: String::from("dance-single"),
        difficulty: difficulty.to_string(),
        description: String::new(),
        chart_name: String::new(),
        meter,
        step_artist: String::new(),
        short_hash: format!("{base}-{difficulty}"),
        stats: ArrowStats {
            total_arrows: 0,
            left: 0,
            down: 0,
            up: 0,
            right: 0,
            total_steps: 0,
            jumps: 0,
            hands: 0,
            mines: 0,
            holds: 0,
            rolls: 0,
            lifts: 0,
            fakes: 0,
            holding: 0,
        },
        tech_counts: TechCounts {
            crossovers: 0,
            half_crossovers: 0,
            full_crossovers: 0,
            footswitches: 0,
            up_footswitches: 0,
            down_footswitches: 0,
            sideswitches: 0,
            jacks: 0,
            brackets: 0,
            doublesteps: 0,
        },
        mines_nonfake: 0,
        stamina_counts: StaminaCounts::default(),
        total_streams: 0,
        matrix_rating: 0.0,
        max_nps: 0.0,
        sn_detailed_breakdown: String::new(),
        sn_partial_breakdown: String::new(),
        sn_simple_breakdown: String::new(),
        detailed_breakdown: String::new(),
        partial_breakdown: String::new(),
        simple_breakdown: String::new(),
        total_measures: 0,
        measure_nps_vec: Vec::new(),
        measure_seconds_vec: Vec::new(),
        first_second: 0.0,
        has_note_data: true,
        has_chart_attacks: false,
        possible_grade_points: 0,
        holds_total: 0,
        rolls_total: 0,
        mines_total: 0,
        display_bpm: None,
        min_bpm: 150.0,
        max_bpm: 150.0,
    }
}
