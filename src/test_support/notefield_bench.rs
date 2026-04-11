use crate::engine::present::actors::Actor;
use crate::game::chart::{ChartData, GameplayChartData, StaminaCounts};
use crate::game::gameplay::{
    self, ActiveHold, ActiveTapExplosion, ColumnCue, ColumnCueColumn, ErrorBarText, ErrorBarTick,
    MAX_COLS, MAX_PLAYERS,
};
use crate::game::judgment::{JudgeGrade, TimingWindow};
use crate::game::note::NoteType;
use crate::game::parsing::notes::ParsedNote;
use crate::game::profile;
use crate::game::scroll::ScrollSpeedSetting;
use crate::game::song::SongData;
use crate::game::timing::{ROWS_PER_BEAT, TimingData, TimingSegments, note_row_to_beat};
use crate::screens::components::gameplay::notefield::{self, FieldPlacement};
use rssp::TechCounts;
use rssp::stats::ArrowStats;
use std::path::PathBuf;
use std::sync::Arc;

pub const SCENARIO_NAME: &str = "notefield";
const VISIBLE_BEAT: f32 = 48.0;
const WINDOW_BEATS_BEFORE: f32 = 8.0;
const WINDOW_BEATS_AFTER: f32 = 24.0;

pub struct NotefieldBenchFixture {
    state: gameplay::State,
    profile: profile::Profile,
}

impl NotefieldBenchFixture {
    pub fn state(&self) -> &gameplay::State {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut gameplay::State {
        &mut self.state
    }

    pub fn profile(&self) -> &profile::Profile {
        &self.profile
    }

    pub fn build(&self, retained: bool) -> Vec<Actor> {
        if !retained {
            for cache in &self.state.notefield_model_cache {
                cache.borrow_mut().clear();
            }
        }
        notefield::build(
            &self.state,
            &self.profile,
            FieldPlacement::P1,
            profile::PlayStyle::Single,
            false,
        )
        .0
    }
}

pub fn fixture() -> NotefieldBenchFixture {
    profile::set_session_play_style(profile::PlayStyle::Single);
    profile::set_session_player_side(profile::PlayerSide::P1);
    profile::set_session_joined(true, false);

    let song = Arc::new(bench_song());
    let chart = Arc::new(song.charts[0].clone());
    let charts: [Arc<ChartData>; MAX_PLAYERS] = [chart.clone(), chart];
    let gameplay_chart = Arc::new(bench_gameplay_chart());
    let gameplay_charts: [Arc<GameplayChartData>; MAX_PLAYERS] =
        [gameplay_chart.clone(), gameplay_chart];
    let mut player_profiles = [profile::Profile::default(), profile::Profile::default()];
    player_profiles[0].noteskin = profile::NoteSkin::new(profile::NoteSkin::CEL_NAME);
    player_profiles[0].scroll_speed = ScrollSpeedSetting::CMod(620.0);
    player_profiles[0].judgment_graphic = profile::JudgmentGraphic::new("Wendy");
    player_profiles[0].hold_judgment_graphic = profile::HoldJudgmentGraphic::new("Love");
    player_profiles[0].hide_combo = false;
    player_profiles[0].column_cues = true;
    player_profiles[0].error_bar = profile::ErrorBarStyle::Colorful;
    player_profiles[0].error_bar_active_mask =
        profile::error_bar_mask_from_style(profile::ErrorBarStyle::Colorful, true);
    player_profiles[0].error_bar_text = true;
    player_profiles[0].measure_lines = profile::MeasureLines::Eighth;

    let mut state = gameplay::init(
        song,
        charts,
        gameplay_charts,
        0,
        1.0,
        [
            ScrollSpeedSetting::CMod(620.0),
            ScrollSpeedSetting::CMod(620.0),
        ],
        player_profiles.clone(),
        None,
        None,
        None,
        Arc::from("BENCH"),
        None,
        None,
        None,
        [0; MAX_PLAYERS],
    );

    prime_visible_window(&mut state);

    NotefieldBenchFixture {
        state,
        profile: player_profiles[0].clone(),
    }
}

fn prime_visible_window(state: &mut gameplay::State) {
    let beat = VISIBLE_BEAT;
    let time = state.timing_players[0].get_time_for_beat(beat);
    let time_ns = state.timing_players[0].get_time_for_beat_ns(beat);
    state.total_elapsed_in_screen = 7.25;
    state.current_beat = beat;
    state.current_beat_display = beat;
    state.current_music_time_ns = time_ns;
    state.current_music_time_display = time;
    state.current_beat_visible[0] = beat;
    state.current_beat_visible[1] = beat;
    state.current_music_time_visible_ns[0] = time_ns;
    state.current_music_time_visible_ns[1] = time_ns;
    state.current_music_time_visible[0] = time;
    state.current_music_time_visible[1] = time;

    for col in 0..MAX_COLS {
        state.tap_explosions[col] = None;
        state.active_holds[col] = None;
    }

    let lower = beat - WINDOW_BEATS_BEFORE;
    let upper = beat + WINDOW_BEATS_AFTER;
    let (note_start, note_end) = state.note_ranges[0];
    let mut end_cursor = note_start;

    for idx in note_start..note_end {
        let note = &state.notes[idx];
        if note.beat < lower {
            continue;
        }
        if note.beat > upper {
            break;
        }
        end_cursor = idx + 1;
    }

    state.next_tap_miss_cursor[0] = end_cursor.max(note_start);

    if let Some((note_index, note_type)) = state.notes[note_start..end_cursor]
        .iter()
        .enumerate()
        .find_map(|(ix, note)| {
            matches!(note.note_type, NoteType::Hold | NoteType::Roll)
                .then_some((note_start + ix, note.note_type))
        })
    {
        let column = state.notes[note_index].column;
        let end_time_ns = state.hold_end_time_cache_ns[note_index]
            .unwrap_or_else(|| gameplay::song_time_ns_from_seconds(time + 1.0));
        let start_time_ns = state.note_time_cache_ns[note_index];
        state.active_holds[column] = Some(ActiveHold {
            note_index,
            start_time_ns,
            end_time_ns,
            note_type,
            let_go: false,
            is_pressed: true,
            life: 1.0,
            last_update_time_ns: time_ns,
        });
    }

    state.tap_explosions[0] = Some(ActiveTapExplosion {
        window: "W1".to_string(),
        elapsed: 0.08,
        start_beat: beat,
    });
    state.column_cues[0] = vec![ColumnCue {
        start_time: time - 1.4,
        duration: 8.0,
        columns: vec![
            ColumnCueColumn {
                column: 0,
                is_mine: false,
            },
            ColumnCueColumn {
                column: 1,
                is_mine: true,
            },
            ColumnCueColumn {
                column: 3,
                is_mine: false,
            },
        ],
    }];
    state.receptor_bop_timers[0] = 0.05;
    state.players[0].combo = 327;
    state.players[0].current_combo_grade = Some(JudgeGrade::Fantastic);
    state.players[0].full_combo_grade = Some(JudgeGrade::Fantastic);
    state.players[0].error_bar_color_bar_started_at = Some(state.total_elapsed_in_screen - 0.06);
    state.players[0].error_bar_color_ticks[0] = Some(ErrorBarTick {
        started_at: state.total_elapsed_in_screen - 0.04,
        offset_s: -0.011,
        window: TimingWindow::W1,
    });
    state.players[0].error_bar_color_ticks[1] = Some(ErrorBarTick {
        started_at: state.total_elapsed_in_screen - 0.08,
        offset_s: 0.019,
        window: TimingWindow::W2,
    });
    state.players[0].error_bar_text = Some(ErrorBarText {
        started_at: state.total_elapsed_in_screen - 0.05,
        early: true,
    });
    state.players[0].last_judgment = None;
}

fn bench_song() -> SongData {
    let chart = bench_chart();
    SongData {
        simfile_path: PathBuf::from("songs/Bench/Notefield/notefield-bench.ssc"),
        title: "Notefield Benchmark".to_string(),
        subtitle: "Cache Warmup".to_string(),
        translit_title: String::new(),
        translit_subtitle: String::new(),
        artist: "Bench Artist".to_string(),
        genre: String::new(),
        banner_path: None,
        background_path: None,
        background_changes: Vec::new(),
        foreground_lua_changes: Vec::new(),
        has_lua: false,
        cdtitle_path: None,
        music_path: None,
        display_bpm: "150".to_string(),
        offset: 0.0,
        sample_start: None,
        sample_length: None,
        min_bpm: 150.0,
        max_bpm: 150.0,
        normalized_bpms: "0.000=150.000".to_string(),
        music_length_seconds: 128.0,
        total_length_seconds: 128,
        precise_last_second_seconds: 128.0,
        charts: vec![chart],
    }
}

fn bench_chart() -> ChartData {
    let (gameplay, holds, rolls, mines, total_steps) = bench_chart_bundle();
    ChartData {
        chart_type: "dance-single".to_string(),
        difficulty: "challenge".to_string(),
        description: String::new(),
        chart_name: String::new(),
        meter: 15,
        step_artist: String::new(),
        short_hash: "notefield-bench".to_string(),
        stats: ArrowStats {
            total_arrows: total_steps,
            left: 0,
            down: 0,
            up: 0,
            right: 0,
            total_steps,
            jumps: 0,
            hands: 0,
            mines,
            holds,
            rolls,
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
        mines_nonfake: mines,
        stamina_counts: StaminaCounts::default(),
        total_streams: 0,
        matrix_rating: 0.0,
        max_nps: 12.5,
        sn_detailed_breakdown: String::new(),
        sn_partial_breakdown: String::new(),
        sn_simple_breakdown: String::new(),
        detailed_breakdown: String::new(),
        partial_breakdown: String::new(),
        simple_breakdown: String::new(),
        total_measures: 0,
        measure_nps_vec: Vec::new(),
        measure_seconds_vec: Vec::new(),
        first_second: gameplay.timing.get_time_for_beat(0.0).min(0.0),
        has_note_data: true,
        has_chart_attacks: false,
        possible_grade_points: 0,
        holds_total: holds,
        rolls_total: rolls,
        mines_total: mines,
        display_bpm: None,
        min_bpm: 120.0,
        max_bpm: 120.0,
    }
}

fn bench_gameplay_chart() -> GameplayChartData {
    let (gameplay, _, _, _, _) = bench_chart_bundle();
    gameplay
}

fn bench_chart_bundle() -> (GameplayChartData, u32, u32, u32, u32) {
    let parsed_notes = bench_notes();
    let max_row = parsed_notes
        .iter()
        .map(|note| note.tail_row_index.unwrap_or(note.row_index))
        .max()
        .unwrap_or(0);
    let row_to_beat: Vec<f32> = (0..=max_row)
        .map(|row| note_row_to_beat(row as i32))
        .collect();
    let timing_segments = TimingSegments {
        beat0_offset_adjust: 0.0,
        bpms: vec![(0.0, 150.0)],
        stops: Vec::new(),
        delays: Vec::new(),
        warps: Vec::new(),
        speeds: Vec::new(),
        scrolls: Vec::new(),
        fakes: Vec::new(),
    };
    let timing = TimingData::from_segments(0.0, 0.0, &timing_segments, &row_to_beat);
    let holds = parsed_notes
        .iter()
        .filter(|note| note.note_type == NoteType::Hold)
        .count() as u32;
    let rolls = parsed_notes
        .iter()
        .filter(|note| note.note_type == NoteType::Roll)
        .count() as u32;
    let mines = parsed_notes
        .iter()
        .filter(|note| note.note_type == NoteType::Mine)
        .count() as u32;
    let total_steps = parsed_notes
        .iter()
        .filter(|note| !matches!(note.note_type, NoteType::Mine | NoteType::Fake))
        .count() as u32;
    (
        GameplayChartData {
            notes: Vec::new(),
            parsed_notes,
            row_to_beat,
            timing_segments,
            timing,
            chart_attacks: None,
        },
        holds,
        rolls,
        mines,
        total_steps,
    )
}

fn bench_notes() -> Vec<ParsedNote> {
    let mut notes = Vec::with_capacity(384);
    let step_rows = ROWS_PER_BEAT as usize / 4;
    for step in 0..384usize {
        let row = step * step_rows;
        let col = step & 3;
        notes.push(ParsedNote {
            row_index: row,
            column: col,
            note_type: NoteType::Tap,
            tail_row_index: None,
        });
        if step % 12 == 3 {
            notes.push(ParsedNote {
                row_index: row,
                column: (col + 1) & 3,
                note_type: NoteType::Hold,
                tail_row_index: Some(row + step_rows * 6),
            });
        }
        if step % 24 == 11 {
            notes.push(ParsedNote {
                row_index: row,
                column: (col + 2) & 3,
                note_type: NoteType::Roll,
                tail_row_index: Some(row + step_rows * 4),
            });
        }
        if step % 10 == 5 {
            notes.push(ParsedNote {
                row_index: row + step_rows / 2,
                column: (col + 3) & 3,
                note_type: NoteType::Mine,
                tail_row_index: None,
            });
        }
    }
    notes.sort_by_key(|note| (note.row_index, note.column));
    notes
}
