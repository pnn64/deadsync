use crate::game::chart::{ChartData, StaminaCounts};
use crate::game::gameplay::{self, ActiveHold, ActiveTapExplosion, Arrow, MAX_COLS, MAX_PLAYERS};
use crate::game::judgment::JudgeGrade;
use crate::game::note::NoteType;
use crate::game::parsing::notes::ParsedNote;
use crate::game::profile;
use crate::game::scroll::ScrollSpeedSetting;
use crate::game::song::SongData;
use crate::game::timing::{ROWS_PER_BEAT, TimingData, TimingSegments, note_row_to_beat};
use crate::screens::components::notefield::{self, FieldPlacement};
use crate::ui::actors::Actor;
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
    let mut player_profiles = [profile::Profile::default(), profile::Profile::default()];
    player_profiles[0].noteskin = profile::NoteSkin::new(profile::NoteSkin::CEL_NAME);
    player_profiles[0].scroll_speed = ScrollSpeedSetting::CMod(620.0);
    player_profiles[0].judgment_graphic = profile::JudgmentGraphic::Wendy;
    player_profiles[0].hold_judgment_graphic = profile::HoldJudgmentGraphic::Love;
    player_profiles[0].hide_combo = false;
    player_profiles[0].error_bar = profile::ErrorBarStyle::Colorful;
    player_profiles[0].error_bar_active_mask =
        profile::error_bar_mask_from_style(profile::ErrorBarStyle::Colorful, false);

    let mut state = gameplay::init(
        song,
        charts,
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
    state.total_elapsed_in_screen = 7.25;
    state.current_beat = beat;
    state.current_beat_display = beat;
    state.current_music_time = time;
    state.current_music_time_display = time;
    state.current_beat_visible[0] = beat;
    state.current_beat_visible[1] = beat;
    state.current_music_time_visible[0] = time;
    state.current_music_time_visible[1] = time;

    for col in 0..MAX_COLS {
        state.arrows[col].clear();
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
        if !matches!(note.note_type, NoteType::Hold | NoteType::Roll) {
            state.arrows[note.column].push(Arrow {
                beat: note.beat,
                note_type: note.note_type,
                note_index: idx,
            });
        }
    }

    state.note_spawn_cursor[0] = end_cursor.max(note_start);
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
        let end_time = state.hold_end_time_cache[note_index].unwrap_or(time + 1.0);
        state.active_holds[column] = Some(ActiveHold {
            note_index,
            end_time,
            note_type,
            let_go: false,
            is_pressed: true,
            life: 1.0,
        });
    }

    state.tap_explosions[0] = Some(ActiveTapExplosion {
        window: "W1".to_string(),
        elapsed: 0.08,
        start_beat: beat,
    });
    state.receptor_bop_timers[0] = 0.05;
    state.players[0].combo = 327;
    state.players[0].current_combo_grade = Some(JudgeGrade::Fantastic);
    state.players[0].full_combo_grade = Some(JudgeGrade::Fantastic);
    state.players[0].last_judgment = None;
}

fn bench_song() -> SongData {
    let chart = bench_chart();
    SongData {
        simfile_path: PathBuf::from("Songs/Bench/Notefield/notefield-bench.ssc"),
        title: "Notefield Benchmark".to_string(),
        subtitle: "Cache Warmup".to_string(),
        translit_title: String::new(),
        translit_subtitle: String::new(),
        artist: "Bench Artist".to_string(),
        banner_path: None,
        background_path: None,
        cdtitle_path: None,
        music_path: None,
        display_bpm: "150".to_string(),
        offset: 0.0,
        sample_start: None,
        sample_length: None,
        min_bpm: 150.0,
        max_bpm: 150.0,
        normalized_bpms: "0.000=150.000".to_string(),
        normalized_stops: String::new(),
        normalized_delays: String::new(),
        normalized_warps: String::new(),
        normalized_speeds: String::new(),
        normalized_scrolls: String::new(),
        normalized_fakes: String::new(),
        music_length_seconds: 128.0,
        total_length_seconds: 128,
        charts: vec![chart],
    }
}

fn bench_chart() -> ChartData {
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

    ChartData {
        chart_type: "dance-single".to_string(),
        difficulty: "challenge".to_string(),
        description: String::new(),
        chart_name: String::new(),
        meter: 15,
        step_artist: String::new(),
        notes: Vec::new(),
        parsed_notes,
        row_to_beat,
        timing_segments,
        timing,
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
        max_nps: 12.5,
        sn_detailed_breakdown: String::new(),
        sn_partial_breakdown: String::new(),
        sn_simple_breakdown: String::new(),
        detailed_breakdown: String::new(),
        partial_breakdown: String::new(),
        simple_breakdown: String::new(),
        total_measures: 0,
        measure_nps_vec: Vec::new(),
        chart_attacks: None,
        chart_bpms: None,
        chart_stops: None,
        chart_delays: None,
        chart_warps: None,
        chart_speeds: None,
        chart_scrolls: None,
        chart_fakes: None,
    }
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
