use crate::act;
use crate::assets::i18n::{tr, tr_fmt};
use crate::assets::{FontRole, current_machine_font_key};
use crate::config;
use crate::engine::audio;
use crate::engine::input::{InputEvent, VirtualAction};
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::space::{
    screen_center_x, screen_center_y, screen_height, screen_width, widescale,
};
use crate::game::chart::ChartData;
use crate::screens::SongOffsetSyncChange;
use crate::screens::components::shared::loading_bar;
use crate::screens::input as screen_input;
use null_or_die::{BiasStreamCfg, BiasStreamEvent, GraphOrientation};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

const OVERLAY_Z: i16 = 1496;
const VIEW_ROWS: usize = 8;
const ROW_STEP: f32 = 43.0;
const PROGRESS_STEP_BEATS: usize = 4;
const MAX_MSGS_PER_FRAME: usize = 64;
const POLL_BUDGET: Duration = Duration::from_millis(2);
pub(crate) fn all_label() -> std::sync::Arc<str> {
    tr("PackSync", "AllPacksLabel")
}

pub(crate) struct TargetSpec {
    pub simfile_path: PathBuf,
    pub song_title: String,
    pub chart_label: String,
    pub chart_ix: usize,
}

enum WorkerMsg {
    RowStarted {
        index: usize,
    },
    RowInit {
        index: usize,
        total_beats: usize,
    },
    RowBeat {
        index: usize,
        beats_processed: usize,
        total_beats: usize,
    },
    RowFinished {
        index: usize,
        result: Result<AnalysisResult, String>,
    },
    Finished,
}

#[derive(Clone, Copy, Debug)]
struct AnalysisResult {
    bias_ms: f64,
    confidence: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowPhase {
    Pending,
    Running,
    Ready,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowDisposition {
    Pending,
    Running,
    Eligible,
    BelowThreshold,
    NoChange,
    Failed,
}

struct RowState {
    simfile_path: PathBuf,
    song_title: String,
    chart_label: String,
    total_beats: usize,
    beats_processed: usize,
    final_bias_ms: Option<f64>,
    final_confidence: Option<f64>,
    phase: RowPhase,
    error_text: Option<String>,
}

#[derive(Clone, Debug)]
enum ApplyMode {
    PerRow,
    Uniform { simfile_paths: Vec<PathBuf> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OverlayPhase {
    Running,
    Review,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ThreeKeyFocus {
    #[default]
    Rows,
    Choice,
}

pub(crate) struct OverlayStateData {
    pack_name: String,
    rows: Vec<RowState>,
    scroll_index: usize,
    auto_follow: bool,
    yes_selected: bool,
    phase: OverlayPhase,
    min_confidence: f64,
    cancel: Arc<AtomicBool>,
    current_row: Option<usize>,
    rx: mpsc::Receiver<WorkerMsg>,
    menu_lr_chord: screen_input::MenuLrChordTracker,
    three_key_focus: ThreeKeyFocus,
    apply_mode: ApplyMode,
}

pub(crate) enum OverlayState {
    Hidden,
    Visible(OverlayStateData),
}

#[derive(Clone, Copy, Debug, Default)]
struct Summary {
    analyzed: usize,
    total: usize,
    eligible: usize,
    below_threshold: usize,
    no_change: usize,
    failed: usize,
}

pub(crate) fn chart_label(chart: &ChartData) -> String {
    if chart.difficulty.eq_ignore_ascii_case("edit") && !chart.description.trim().is_empty() {
        format!("{} ({})", chart.difficulty, chart.description)
    } else {
        chart.difficulty.clone()
    }
}

#[inline(always)]
fn confidence_threshold_percent() -> u8 {
    config::get().null_or_die_confidence_percent.min(100)
}

#[inline(always)]
fn confidence_threshold() -> f64 {
    f64::from(confidence_threshold_percent()) / 100.0
}

#[inline(always)]
fn confidence_percent(confidence: Option<f64>) -> u32 {
    (confidence.unwrap_or(0.0).clamp(0.0, 1.0) * 100.0).round() as u32
}

#[inline(always)]
fn pack_sync_worker_count(target_count: usize) -> usize {
    if target_count == 0 {
        return 0;
    }
    let avail_threads = std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(1);
    let configured_threads = match config::get().null_or_die_pack_sync_threads {
        0 => avail_threads,
        1 => 1,
        n => (n as usize).min(avail_threads).max(1),
    };
    configured_threads.min(target_count).max(1)
}

pub(crate) fn build_overlay(state: &OverlayState, active_color_index: i32) -> Option<Vec<Actor>> {
    let OverlayState::Visible(overlay) = state else {
        return None;
    };

    let summary = summary(overlay);
    let pane_w = widescale(760.0, 860.0);
    let pane_h = 488.0;
    let pane_cx = screen_center_x();
    let pane_cy = screen_center_y() - 4.0;
    let pane_left = pane_cx - pane_w * 0.5;
    let pane_top = pane_cy - pane_h * 0.5;
    let pane_right = pane_cx + pane_w * 0.5;
    let accent = color::simply_love_rgba(active_color_index);
    let fill = color::decorative_rgba(active_color_index);
    let start = overlay.scroll_index.min(scroll_limit(overlay.rows.len()));
    let title = if overlay.phase == OverlayPhase::Running {
        tr("PackSync", "SyncingPackTitle")
    } else if can_save(overlay) {
        tr("PackSync", "ReviewTitle")
    } else {
        tr("PackSync", "CompleteTitle")
    };
    let counts_text = format!(
        "{}/{} chart(s) analyzed - {} ready, {} below {}%, {} no change, {} failed",
        summary.analyzed,
        summary.total,
        summary.eligible,
        summary.below_threshold,
        confidence_threshold_percent(),
        summary.no_change,
        summary.failed
    );
    let scroll_text = (summary.total > VIEW_ROWS).then(|| {
        tr_fmt(
            "PackSync",
            "RowsPaginationFormat",
            &[
                ("start", &(start + 1).to_string()),
                ("end", &(start + VIEW_ROWS).min(summary.total).to_string()),
                ("total", &summary.total.to_string()),
            ],
        )
    });
    let counts_maxwidth = if scroll_text.is_some() {
        pane_w - 240.0
    } else {
        pane_w - 56.0
    };
    let song_x = pane_left + 28.0;
    let bar_x = pane_left + 360.0;
    let result_x = pane_right - 28.0;
    let row_top = pane_top + 138.0;

    let mut actors = Vec::with_capacity(96);
    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.88):
        z(OVERLAY_Z)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(pane_cx, pane_cy):
        zoomto(pane_w + 2.0, pane_h + 2.0):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(OVERLAY_Z + 1)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(pane_cx, pane_cy):
        zoomto(pane_w, pane_h):
        diffuse(0.02, 0.02, 0.02, 1.0):
        z(OVERLAY_Z + 2)
    ));
    actors.push(act!(text:
        font(current_machine_font_key(FontRole::Header)):
        settext(title):
        align(0.5, 0.5):
        xy(pane_cx, pane_top + 28.0):
        zoom(0.6):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(OVERLAY_Z + 3):
        horizalign(center)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(overlay.pack_name.clone()):
        align(0.5, 0.5):
        xy(pane_cx, pane_top + 56.0):
        zoom(0.92):
        maxwidth(pane_w - 120.0):
        diffuse(0.82, 0.82, 0.82, 1.0):
        z(OVERLAY_Z + 3):
        horizalign(center)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(counts_text):
        align(0.0, 0.5):
        xy(song_x, pane_top + 86.0):
        zoom(0.8):
        maxwidth(counts_maxwidth):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(OVERLAY_Z + 3):
        horizalign(left)
    ));
    if let Some(scroll_text) = scroll_text {
        actors.push(act!(text:
            font("miso"):
            settext(scroll_text):
            align(1.0, 0.5):
            xy(result_x, pane_top + 86.0):
            zoom(0.8):
            diffuse(0.82, 0.82, 0.82, 1.0):
            z(OVERLAY_Z + 3):
            horizalign(right)
        ));
    }
    let col_song = tr("PackSync", "SongColumnHeader");
    actors.push(act!(text:
        font("miso"):
        settext(col_song):
        align(0.0, 0.5):
        xy(song_x, row_top - 20.0):
        zoom(0.75):
        diffuse(0.6, 0.6, 0.6, 1.0):
        z(OVERLAY_Z + 3):
        horizalign(left)
    ));
    let col_progress = tr("PackSync", "ProgressColumnHeader");
    actors.push(act!(text:
        font("miso"):
        settext(col_progress):
        align(0.0, 0.5):
        xy(bar_x, row_top - 20.0):
        zoom(0.75):
        diffuse(0.6, 0.6, 0.6, 1.0):
        z(OVERLAY_Z + 3):
        horizalign(left)
    ));
    let col_result = tr("PackSync", "ResultColumnHeader");
    actors.push(act!(text:
        font("miso"):
        settext(col_result):
        align(1.0, 0.5):
        xy(result_x, row_top - 20.0):
        zoom(0.75):
        diffuse(0.6, 0.6, 0.6, 1.0):
        z(OVERLAY_Z + 3):
        horizalign(right)
    ));

    for (slot, row) in overlay.rows.iter().skip(start).take(VIEW_ROWS).enumerate() {
        let row_index = start + slot;
        let row_y = row_top + ROW_STEP * slot as f32;
        let disposition = row_disposition(row, overlay.min_confidence);
        if overlay.current_row == Some(row_index) && overlay.phase == OverlayPhase::Running {
            actors.push(act!(quad:
                align(0.0, 0.5):
                xy(song_x - 8.0, row_y + 2.0):
                zoomto(pane_w - 40.0, 38.0):
                diffuse(accent[0], accent[1], accent[2], 0.18):
                z(OVERLAY_Z + 2)
            ));
        }

        let result_rgba = match disposition {
            RowDisposition::BelowThreshold => [1.0, 0.82, 0.32, 1.0],
            RowDisposition::NoChange => [0.72, 0.72, 0.72, 1.0],
            RowDisposition::Failed => [1.0, 0.35, 0.35, 1.0],
            _ => [1.0, 1.0, 1.0, 1.0],
        };

        actors.push(act!(text:
            font("miso"):
            settext(format!("{}. {}", row_index + 1, row.song_title)):
            align(0.0, 0.5):
            xy(song_x, row_y - 6.0):
            zoom(0.84):
            maxwidth(310.0):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(OVERLAY_Z + 4):
            horizalign(left)
        ));
        actors.push(act!(text:
            font("miso"):
            settext(row.chart_label.clone()):
            align(0.0, 0.5):
            xy(song_x, row_y + 12.0):
            zoom(0.7):
            maxwidth(310.0):
            diffuse(0.72, 0.72, 0.72, 1.0):
            z(OVERLAY_Z + 4):
            horizalign(left)
        ));
        actors.push(loading_bar::build(loading_bar::LoadingBarParams {
            align: [0.0, 0.5],
            offset: [bar_x, row_y + 2.0],
            width: 220.0,
            height: 18.0,
            progress: progress(row),
            label: bar_label(row, overlay.min_confidence).into(),
            fill_rgba: [fill[0], fill[1], fill[2], 1.0],
            bg_rgba: [0.0, 0.0, 0.0, 1.0],
            border_rgba: [1.0, 1.0, 1.0, 1.0],
            text_rgba: [1.0, 1.0, 1.0, 1.0],
            text_zoom: 0.72,
            z: OVERLAY_Z + 4,
        }));
        actors.push(act!(text:
            font("miso"):
            settext(result_text(row, overlay.min_confidence)):
            align(1.0, 0.5):
            xy(result_x, row_y + 2.0):
            zoom(0.72):
            maxwidth(180.0):
            diffuse(result_rgba[0], result_rgba[1], result_rgba[2], result_rgba[3]):
            z(OVERLAY_Z + 4):
            horizalign(right)
        ));
        actors.push(act!(quad:
            align(0.0, 0.5):
            xy(song_x, row_y + 25.0):
            zoomto(pane_w - 56.0, 1.0):
            diffuse(1.0, 1.0, 1.0, 0.25):
            z(OVERLAY_Z + 2)
        ));
    }

    match overlay.phase {
        OverlayPhase::Running => {
            let help = tr("PackSync", "HelpTextRunning");
            actors.push(act!(text:
                font("miso"):
                settext(help):
                align(0.5, 0.5):
                xy(pane_cx, pane_top + pane_h - 24.0):
                zoom(0.8):
                diffuse(0.85, 0.85, 0.85, 1.0):
                z(OVERLAY_Z + 4):
                horizalign(center)
            ));
        }
        OverlayPhase::Review => {
            let prompt = save_prompt(overlay);
            if can_save(overlay) {
                let answer_y = pane_top + pane_h - 44.0;
                let choice_yes_x = pane_cx - 100.0;
                let choice_no_x = pane_cx + 100.0;
                let cursor_x = if overlay.yes_selected {
                    choice_yes_x
                } else {
                    choice_no_x
                };

                actors.push(act!(quad:
                    align(0.5, 0.5):
                    xy(cursor_x, answer_y):
                    zoomto(145.0, 36.0):
                    diffuse(accent[0], accent[1], accent[2], 1.0):
                    z(OVERLAY_Z + 4)
                ));
                actors.push(act!(text:
                    font("miso"):
                    settext(prompt):
                    align(0.5, 0.5):
                    xy(pane_cx, pane_top + pane_h - 92.0):
                    zoom(0.86):
                    maxwidth(pane_w - 90.0):
                    diffuse(1.0, 1.0, 1.0, 1.0):
                    z(OVERLAY_Z + 4):
                    horizalign(center)
                ));
                let yes_label = tr("PackSync", "YesOption");
                actors.push(act!(text:
                    font(current_machine_font_key(FontRole::Header)):
                    settext(yes_label):
                    align(0.5, 0.5):
                    xy(choice_yes_x, answer_y):
                    zoom(0.72):
                    diffuse(1.0, 1.0, 1.0, 1.0):
                    z(OVERLAY_Z + 4):
                    horizalign(center)
                ));
                let no_label = tr("PackSync", "NoOption");
                actors.push(act!(text:
                    font(current_machine_font_key(FontRole::Header)):
                    settext(no_label):
                    align(0.5, 0.5):
                    xy(choice_no_x, answer_y):
                    zoom(0.72):
                    diffuse(1.0, 1.0, 1.0, 1.0):
                    z(OVERLAY_Z + 4):
                    horizalign(center)
                ));
                let help = tr("PackSync", "HelpTextReview");
                actors.push(act!(text:
                    font("miso"):
                    settext(help):
                    align(0.5, 0.5):
                    xy(pane_cx, pane_top + pane_h - 18.0):
                    zoom(0.74):
                    diffuse(0.85, 0.85, 0.85, 1.0):
                    z(OVERLAY_Z + 4):
                    horizalign(center)
                ));
            } else {
                actors.push(act!(text:
                    font("miso"):
                    settext(prompt):
                    align(0.5, 0.5):
                    xy(pane_cx, pane_top + pane_h - 56.0):
                    zoom(0.84):
                    maxwidth(pane_w - 90.0):
                    diffuse(1.0, 1.0, 1.0, 1.0):
                    z(OVERLAY_Z + 4):
                    horizalign(center)
                ));
                let help = tr("PackSync", "HelpTextComplete");
                actors.push(act!(text:
                    font("miso"):
                    settext(help):
                    align(0.5, 0.5):
                    xy(pane_cx, pane_top + pane_h - 18.0):
                    zoom(0.74):
                    diffuse(0.85, 0.85, 0.85, 1.0):
                    z(OVERLAY_Z + 4):
                    horizalign(center)
                ));
            }
        }
    }

    Some(actors)
}

pub(crate) fn hide(state: &mut OverlayState) {
    if let OverlayState::Visible(overlay) = state {
        overlay.cancel.store(true, Ordering::Relaxed);
    }
    *state = OverlayState::Hidden;
}

pub(crate) fn begin(state: &mut OverlayState, pack_name: String, targets: Vec<TargetSpec>) -> bool {
    begin_with_mode(state, pack_name, targets, ApplyMode::PerRow)
}

pub(crate) fn begin_uniform(
    state: &mut OverlayState,
    pack_name: String,
    targets: Vec<TargetSpec>,
    simfile_paths: Vec<PathBuf>,
) -> bool {
    if simfile_paths.is_empty() {
        return false;
    }
    begin_with_mode(
        state,
        pack_name,
        targets,
        ApplyMode::Uniform { simfile_paths },
    )
}

fn begin_with_mode(
    state: &mut OverlayState,
    pack_name: String,
    targets: Vec<TargetSpec>,
    apply_mode: ApplyMode,
) -> bool {
    if targets.is_empty() {
        return false;
    }

    let min_confidence = confidence_threshold();
    let worker_count = pack_sync_worker_count(targets.len());
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_thread = Arc::clone(&cancel);
    let (tx, rx) = mpsc::channel::<WorkerMsg>();
    let rows = build_rows(&targets);

    std::thread::spawn(move || {
        let cfg = Arc::new(config::null_or_die_bias_cfg());
        let stream_cfg = BiasStreamCfg {
            emit_freq_delta: false,
            orientation: GraphOrientation::Horizontal,
        };
        let (job_tx, job_rx) = mpsc::channel::<(usize, TargetSpec)>();
        let job_rx = Arc::new(Mutex::new(job_rx));
        let mut workers = Vec::with_capacity(worker_count);

        for _ in 0..worker_count {
            let cancel_thread = Arc::clone(&cancel_thread);
            let cfg = Arc::clone(&cfg);
            let job_rx = Arc::clone(&job_rx);
            let tx = tx.clone();
            workers.push(std::thread::spawn(move || {
                loop {
                    if cancel_thread.load(Ordering::Relaxed) {
                        return;
                    }

                    let job = {
                        let Ok(rx) = job_rx.lock() else { return };
                        rx.recv()
                    };
                    let Ok((index, target)) = job else { return };
                    if cancel_thread.load(Ordering::Relaxed) {
                        return;
                    }

                    let _ = tx.send(WorkerMsg::RowStarted { index });
                    let mut total_beats = 0usize;
                    let mut last_sent = 0usize;
                    let result = null_or_die::api::analyze_chart_stream(
                        target.simfile_path.as_path(),
                        target.chart_ix,
                        cfg.as_ref(),
                        stream_cfg,
                        |event| match event {
                            BiasStreamEvent::Init(init) => {
                                total_beats = init.planned_beats;
                                let _ = tx.send(WorkerMsg::RowInit { index, total_beats });
                            }
                            BiasStreamEvent::Beat(beat) => {
                                let beats_processed = beat.beat_seq.saturating_add(1);
                                let is_last = total_beats > 0 && beats_processed >= total_beats;
                                if beats_processed == 1
                                    || is_last
                                    || beats_processed.saturating_sub(last_sent)
                                        >= PROGRESS_STEP_BEATS
                                {
                                    last_sent = beats_processed;
                                    let _ = tx.send(WorkerMsg::RowBeat {
                                        index,
                                        beats_processed,
                                        total_beats,
                                    });
                                }
                            }
                            BiasStreamEvent::Convolution(_) | BiasStreamEvent::Done(_) => {}
                        },
                    )
                    .map(|result| AnalysisResult {
                        bias_ms: result.estimate.bias_ms,
                        confidence: result.estimate.confidence,
                    });
                    let _ = tx.send(WorkerMsg::RowFinished { index, result });
                }
            }));
        }

        for (index, target) in targets.into_iter().enumerate() {
            if job_tx.send((index, target)).is_err() {
                break;
            }
        }
        drop(job_tx);
        for worker in workers {
            let _ = worker.join();
        }

        let _ = tx.send(WorkerMsg::Finished);
    });

    *state = OverlayState::Visible(OverlayStateData {
        pack_name,
        rows,
        scroll_index: 0,
        auto_follow: true,
        yes_selected: true,
        phase: OverlayPhase::Running,
        min_confidence,
        cancel,
        current_row: None,
        rx,
        menu_lr_chord: screen_input::MenuLrChordTracker::default(),
        three_key_focus: ThreeKeyFocus::Rows,
        apply_mode,
    });
    true
}

pub(crate) fn poll(state: &mut OverlayState) -> bool {
    let OverlayState::Visible(overlay) = state else {
        return false;
    };
    poll_overlay(overlay);
    true
}

pub(crate) fn handle_input(
    state: &mut OverlayState,
    ev: &InputEvent,
) -> crate::screens::ScreenAction {
    let three_key_action = {
        let OverlayState::Visible(overlay) = state else {
            return crate::screens::ScreenAction::None;
        };
        screen_input::three_key_menu_action(&mut overlay.menu_lr_chord, ev)
    };
    if !ev.pressed {
        return crate::screens::ScreenAction::None;
    }

    let mut close_overlay = false;
    let mut apply_changes: Option<Vec<SongOffsetSyncChange>> = None;
    let mut play_change = false;
    let mut play_start = false;
    let page_delta = VIEW_ROWS.saturating_sub(1).max(1) as isize;

    {
        let OverlayState::Visible(overlay) = state else {
            return crate::screens::ScreenAction::None;
        };
        if screen_input::dedicated_three_key_nav_enabled()
            && let Some((_, nav)) = three_key_action
        {
            match overlay.phase {
                OverlayPhase::Running => match nav {
                    screen_input::ThreeKeyMenuAction::Prev => {
                        if shift(overlay, -1) {
                            play_change = true;
                        }
                    }
                    screen_input::ThreeKeyMenuAction::Next => {
                        if shift(overlay, 1) {
                            play_change = true;
                        }
                    }
                    screen_input::ThreeKeyMenuAction::Confirm
                    | screen_input::ThreeKeyMenuAction::Cancel => {
                        close_overlay = true;
                        play_start = true;
                    }
                },
                OverlayPhase::Review => {
                    if matches!(overlay.three_key_focus, ThreeKeyFocus::Rows) {
                        match nav {
                            screen_input::ThreeKeyMenuAction::Prev => {
                                if shift(overlay, -1) {
                                    play_change = true;
                                }
                            }
                            screen_input::ThreeKeyMenuAction::Next => {
                                if shift(overlay, 1) {
                                    play_change = true;
                                }
                            }
                            screen_input::ThreeKeyMenuAction::Confirm => {
                                if can_save(overlay) {
                                    overlay.three_key_focus = ThreeKeyFocus::Choice;
                                    play_change = true;
                                } else {
                                    close_overlay = true;
                                    play_start = true;
                                }
                            }
                            screen_input::ThreeKeyMenuAction::Cancel => {
                                close_overlay = true;
                                play_start = true;
                            }
                        }
                    } else {
                        match nav {
                            screen_input::ThreeKeyMenuAction::Prev => {
                                if can_save(overlay) && !overlay.yes_selected {
                                    overlay.yes_selected = true;
                                    play_change = true;
                                }
                            }
                            screen_input::ThreeKeyMenuAction::Next => {
                                if can_save(overlay) && overlay.yes_selected {
                                    overlay.yes_selected = false;
                                    play_change = true;
                                }
                            }
                            screen_input::ThreeKeyMenuAction::Confirm => {
                                if can_save(overlay) && overlay.yes_selected {
                                    apply_changes = Some(collect_changes(overlay));
                                }
                                close_overlay = true;
                                play_start = true;
                            }
                            screen_input::ThreeKeyMenuAction::Cancel => {
                                overlay.three_key_focus = ThreeKeyFocus::Rows;
                                play_change = true;
                            }
                        }
                    }
                }
            }
        } else {
            match overlay.phase {
                OverlayPhase::Running => match ev.action {
                    VirtualAction::p1_up
                    | VirtualAction::p1_menu_up
                    | VirtualAction::p2_up
                    | VirtualAction::p2_menu_up => {
                        if shift(overlay, -1) {
                            play_change = true;
                        }
                    }
                    VirtualAction::p1_down
                    | VirtualAction::p1_menu_down
                    | VirtualAction::p2_down
                    | VirtualAction::p2_menu_down => {
                        if shift(overlay, 1) {
                            play_change = true;
                        }
                    }
                    VirtualAction::p1_left
                    | VirtualAction::p1_menu_left
                    | VirtualAction::p2_left
                    | VirtualAction::p2_menu_left => {
                        if shift(overlay, -page_delta) {
                            play_change = true;
                        }
                    }
                    VirtualAction::p1_right
                    | VirtualAction::p1_menu_right
                    | VirtualAction::p2_right
                    | VirtualAction::p2_menu_right => {
                        if shift(overlay, page_delta) {
                            play_change = true;
                        }
                    }
                    VirtualAction::p1_start
                    | VirtualAction::p2_start
                    | VirtualAction::p1_back
                    | VirtualAction::p2_back
                    | VirtualAction::p1_select
                    | VirtualAction::p2_select => {
                        close_overlay = true;
                        play_start = true;
                    }
                    _ => {}
                },
                OverlayPhase::Review => match ev.action {
                    VirtualAction::p1_up
                    | VirtualAction::p1_menu_up
                    | VirtualAction::p2_up
                    | VirtualAction::p2_menu_up => {
                        if shift(overlay, -1) {
                            play_change = true;
                        }
                    }
                    VirtualAction::p1_down
                    | VirtualAction::p1_menu_down
                    | VirtualAction::p2_down
                    | VirtualAction::p2_menu_down => {
                        if shift(overlay, 1) {
                            play_change = true;
                        }
                    }
                    VirtualAction::p1_menu_left | VirtualAction::p2_menu_left => {
                        if shift(overlay, -page_delta) {
                            play_change = true;
                        }
                    }
                    VirtualAction::p1_menu_right | VirtualAction::p2_menu_right => {
                        if shift(overlay, page_delta) {
                            play_change = true;
                        }
                    }
                    VirtualAction::p1_left | VirtualAction::p2_left => {
                        if can_save(overlay) && !overlay.yes_selected {
                            overlay.yes_selected = true;
                            play_change = true;
                        }
                    }
                    VirtualAction::p1_right | VirtualAction::p2_right => {
                        if can_save(overlay) && overlay.yes_selected {
                            overlay.yes_selected = false;
                            play_change = true;
                        }
                    }
                    VirtualAction::p1_start | VirtualAction::p2_start => {
                        if can_save(overlay) && overlay.yes_selected {
                            apply_changes = Some(collect_changes(overlay));
                        }
                        close_overlay = true;
                        play_start = true;
                    }
                    VirtualAction::p1_back
                    | VirtualAction::p2_back
                    | VirtualAction::p1_select
                    | VirtualAction::p2_select => {
                        close_overlay = true;
                        play_start = true;
                    }
                    _ => {}
                },
            }
        }
    }

    if play_change {
        audio::play_sfx("assets/sounds/change.ogg");
    }
    if play_start {
        audio::play_sfx("assets/sounds/start.ogg");
    }
    if close_overlay {
        hide(state);
    }
    if let Some(changes) = apply_changes
        && !changes.is_empty()
    {
        return crate::screens::ScreenAction::ApplySongOffsetSyncBatch { changes };
    }
    crate::screens::ScreenAction::None
}

fn build_rows(targets: &[TargetSpec]) -> Vec<RowState> {
    let mut rows = Vec::with_capacity(targets.len());
    for target in targets {
        rows.push(RowState {
            simfile_path: target.simfile_path.clone(),
            song_title: target.song_title.clone(),
            chart_label: target.chart_label.clone(),
            total_beats: 0,
            beats_processed: 0,
            final_bias_ms: None,
            final_confidence: None,
            phase: RowPhase::Pending,
            error_text: None,
        });
    }
    rows
}

#[inline(always)]
fn row_delta_seconds(row: &RowState) -> Option<f32> {
    row.final_bias_ms
        .map(|bias_ms| -(bias_ms as f32) * 0.001)
        .filter(|v| v.is_finite())
}

fn row_disposition(row: &RowState, min_confidence: f64) -> RowDisposition {
    match row.phase {
        RowPhase::Pending => RowDisposition::Pending,
        RowPhase::Running => RowDisposition::Running,
        RowPhase::Failed => RowDisposition::Failed,
        RowPhase::Ready => {
            let Some(delta_seconds) = row_delta_seconds(row) else {
                return RowDisposition::Failed;
            };
            if delta_seconds.abs() < 0.000_001_f32 {
                return RowDisposition::NoChange;
            }
            if row.final_confidence.unwrap_or(0.0) < min_confidence {
                RowDisposition::BelowThreshold
            } else {
                RowDisposition::Eligible
            }
        }
    }
}

fn summary(overlay: &OverlayStateData) -> Summary {
    let mut summary = Summary {
        total: overlay.rows.len(),
        ..Summary::default()
    };
    for row in &overlay.rows {
        match row_disposition(row, overlay.min_confidence) {
            RowDisposition::Pending | RowDisposition::Running => {}
            RowDisposition::Eligible => {
                summary.analyzed += 1;
                summary.eligible += 1;
            }
            RowDisposition::BelowThreshold => {
                summary.analyzed += 1;
                summary.below_threshold += 1;
            }
            RowDisposition::NoChange => {
                summary.analyzed += 1;
                summary.no_change += 1;
            }
            RowDisposition::Failed => {
                summary.analyzed += 1;
                summary.failed += 1;
            }
        }
    }
    summary
}

#[inline(always)]
fn can_save(overlay: &OverlayStateData) -> bool {
    match &overlay.apply_mode {
        ApplyMode::PerRow => summary(overlay).eligible > 0,
        ApplyMode::Uniform { simfile_paths } => {
            !simfile_paths.is_empty() && uniform_delta_seconds(overlay).is_some()
        }
    }
}

fn collect_changes(overlay: &OverlayStateData) -> Vec<SongOffsetSyncChange> {
    match &overlay.apply_mode {
        ApplyMode::PerRow => overlay
            .rows
            .iter()
            .filter(|row| row_disposition(row, overlay.min_confidence) == RowDisposition::Eligible)
            .filter_map(|row| {
                Some(SongOffsetSyncChange {
                    simfile_path: row.simfile_path.clone(),
                    delta_seconds: row_delta_seconds(row)?,
                })
            })
            .collect(),
        ApplyMode::Uniform { simfile_paths } => {
            let Some(delta_seconds) = uniform_delta_seconds(overlay) else {
                return Vec::new();
            };
            unique_simfile_paths(simfile_paths)
                .into_iter()
                .map(|simfile_path| SongOffsetSyncChange {
                    simfile_path,
                    delta_seconds,
                })
                .collect()
        }
    }
}

fn save_prompt(overlay: &OverlayStateData) -> String {
    let summary = summary(overlay);
    let min_conf_pct = confidence_threshold_percent();
    if !can_save(overlay) {
        return tr_fmt(
            "PackSync",
            "NothingToSaveMessage",
            &[
                ("below", &summary.below_threshold.to_string()),
                ("threshold", &min_conf_pct.to_string()),
                ("nochange", &summary.no_change.to_string()),
                ("failed", &summary.failed.to_string()),
            ],
        )
        .to_string();
    }
    match &overlay.apply_mode {
        ApplyMode::PerRow => tr_fmt(
            "PackSync",
            "SaveConfirmFormat",
            &[
                ("count", &summary.eligible.to_string()),
                ("below", &summary.below_threshold.to_string()),
                ("threshold", &min_conf_pct.to_string()),
                ("nochange", &summary.no_change.to_string()),
                ("failed", &summary.failed.to_string()),
            ],
        )
        .to_string(),
        ApplyMode::Uniform { simfile_paths } => {
            let delta_seconds = uniform_delta_seconds(overlay).unwrap_or(0.0);
            let direction = sync_direction_text(delta_seconds);
            tr_fmt(
                "PackSync",
                "UniformSaveConfirmFormat",
                &[
                    ("delta", &format_delta_seconds(delta_seconds)),
                    ("direction", direction.as_ref()),
                    (
                        "files",
                        &unique_simfile_paths(simfile_paths).len().to_string(),
                    ),
                    ("sources", &summary.eligible.to_string()),
                    ("below", &summary.below_threshold.to_string()),
                    ("threshold", &min_conf_pct.to_string()),
                    ("nochange", &summary.no_change.to_string()),
                    ("failed", &summary.failed.to_string()),
                ],
            )
            .to_string()
        }
    }
}

fn uniform_delta_seconds(overlay: &OverlayStateData) -> Option<f32> {
    let mut deltas = Vec::new();
    for row in &overlay.rows {
        if row_disposition(row, overlay.min_confidence) == RowDisposition::Eligible {
            deltas.push(row_delta_seconds(row)?);
        }
    }
    median_delta_seconds(deltas)
}

fn median_delta_seconds(mut deltas: Vec<f32>) -> Option<f32> {
    deltas.retain(|v| v.is_finite());
    deltas.sort_by(|a, b| a.total_cmp(b));
    match deltas.len() {
        0 => None,
        len if len % 2 == 1 => Some(deltas[len / 2]),
        len => Some((deltas[len / 2 - 1] + deltas[len / 2]) * 0.5),
    }
    .filter(|delta| delta.abs() >= 0.000_001_f32)
}

fn unique_simfile_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut unique = Vec::with_capacity(paths.len());
    for path in paths {
        if !unique.iter().any(|known| known == path) {
            unique.push(path.clone());
        }
    }
    unique
}

fn format_delta_seconds(delta_seconds: f32) -> String {
    format!("{:+.3}", (delta_seconds / 0.001).round() * 0.001)
}

fn sync_direction_text(delta_seconds: f32) -> std::sync::Arc<str> {
    tr(
        "PackSync",
        if delta_seconds > 0.0 {
            "NotesEarlier"
        } else {
            "NotesLater"
        },
    )
}

#[inline(always)]
fn scroll_limit(total: usize) -> usize {
    total.saturating_sub(VIEW_ROWS)
}

#[inline(always)]
fn progress(row: &RowState) -> f32 {
    match row.phase {
        RowPhase::Pending => 0.0,
        RowPhase::Running => {
            if row.total_beats == 0 {
                0.0
            } else {
                (row.beats_processed as f32 / row.total_beats as f32).clamp(0.0, 1.0)
            }
        }
        RowPhase::Ready | RowPhase::Failed => 1.0,
    }
}

fn bar_label(row: &RowState, min_confidence: f64) -> String {
    match row_disposition(row, min_confidence) {
        RowDisposition::Pending => tr("PackSync", "StatusQueued").to_string(),
        RowDisposition::Running => match row.total_beats.max(row.beats_processed) {
            0 => tr("PackSync", "StatusStarting").to_string(),
            total => tr_fmt(
                "PackSync",
                "ProgressFormat",
                &[
                    ("current", &row.beats_processed.min(total).to_string()),
                    ("total", &total.to_string()),
                ],
            )
            .to_string(),
        },
        RowDisposition::Eligible => tr("PackSync", "StatusReady").to_string(),
        RowDisposition::BelowThreshold => tr_fmt(
            "PackSync",
            "StatusBelowThresholdFormat",
            &[("threshold", &confidence_threshold_percent().to_string())],
        )
        .to_string(),
        RowDisposition::NoChange => tr("PackSync", "StatusNoChange").to_string(),
        RowDisposition::Failed => tr("PackSync", "StatusError").to_string(),
    }
}

fn result_text(row: &RowState, min_confidence: f64) -> String {
    let confidence_pct = confidence_percent(row.final_confidence);
    match row_disposition(row, min_confidence) {
        RowDisposition::Pending => tr("PackSync", "StatusQueued").to_string(),
        RowDisposition::Running => {
            if let Some(bias_ms) = row.final_bias_ms {
                format!("{bias_ms:+.2} ms")
            } else {
                tr("PackSync", "StatusWorking").to_string()
            }
        }
        RowDisposition::Eligible | RowDisposition::BelowThreshold => tr_fmt(
            "PackSync",
            "ResultConfidenceFormat",
            &[
                ("bias", &format!("{:+.2}", row.final_bias_ms.unwrap_or(0.0))),
                ("confidence", &confidence_pct.to_string()),
            ],
        )
        .to_string(),
        RowDisposition::NoChange => tr_fmt(
            "PackSync",
            "ResultNoChangeFormat",
            &[("confidence", &confidence_pct.to_string())],
        )
        .to_string(),
        RowDisposition::Failed => row
            .error_text
            .as_deref()
            .map(|s| s.to_string())
            .unwrap_or_else(|| tr("PackSync", "AnalysisFailed").to_string()),
    }
}

fn follow_row(overlay: &mut OverlayStateData, row_index: usize) {
    if row_index < overlay.scroll_index {
        overlay.scroll_index = row_index;
        return;
    }
    let end = overlay.scroll_index + VIEW_ROWS;
    if row_index >= end {
        overlay.scroll_index = row_index + 1 - VIEW_ROWS;
    }
}

fn shift(overlay: &mut OverlayStateData, delta: isize) -> bool {
    let limit = scroll_limit(overlay.rows.len());
    let next = (overlay.scroll_index as isize + delta).clamp(0, limit as isize) as usize;
    if next == overlay.scroll_index {
        return false;
    }
    overlay.scroll_index = next;
    overlay.auto_follow = false;
    true
}

fn poll_overlay(overlay: &mut OverlayStateData) {
    let started = Instant::now();
    let mut handled = 0usize;

    loop {
        if handled >= MAX_MSGS_PER_FRAME || started.elapsed() >= POLL_BUDGET {
            break;
        }
        match overlay.rx.try_recv() {
            Ok(WorkerMsg::RowStarted { index }) => {
                if let Some(row) = overlay.rows.get_mut(index) {
                    row.total_beats = 0;
                    row.beats_processed = 0;
                    row.final_bias_ms = None;
                    row.final_confidence = None;
                    row.phase = RowPhase::Running;
                    row.error_text = None;
                    overlay.current_row = Some(index);
                    if overlay.auto_follow {
                        follow_row(overlay, index);
                    }
                }
            }
            Ok(WorkerMsg::RowInit { index, total_beats }) => {
                if let Some(row) = overlay.rows.get_mut(index) {
                    row.total_beats = total_beats;
                    if overlay.auto_follow && overlay.current_row == Some(index) {
                        follow_row(overlay, index);
                    }
                }
            }
            Ok(WorkerMsg::RowBeat {
                index,
                beats_processed,
                total_beats,
            }) => {
                if let Some(row) = overlay.rows.get_mut(index) {
                    row.phase = RowPhase::Running;
                    row.total_beats = row.total_beats.max(total_beats);
                    row.beats_processed = row.beats_processed.max(beats_processed);
                    if overlay.auto_follow && overlay.current_row == Some(index) {
                        follow_row(overlay, index);
                    }
                }
            }
            Ok(WorkerMsg::RowFinished { index, result }) => {
                if let Some(row) = overlay.rows.get_mut(index) {
                    if overlay.current_row == Some(index) {
                        overlay.current_row = None;
                    }
                    match result {
                        Ok(result) => {
                            row.phase = RowPhase::Ready;
                            row.final_bias_ms = Some(result.bias_ms);
                            row.final_confidence = Some(result.confidence);
                            row.beats_processed = row.beats_processed.max(row.total_beats);
                        }
                        Err(err) => {
                            row.phase = RowPhase::Failed;
                            row.error_text = Some(err);
                        }
                    }
                }
            }
            Ok(WorkerMsg::Finished) => {
                overlay.phase = OverlayPhase::Review;
                overlay.current_row = None;
                overlay.three_key_focus = ThreeKeyFocus::Rows;
                break;
            }
            Err(mpsc::TryRecvError::Empty) => break,
            Err(mpsc::TryRecvError::Disconnected) => {
                overlay.phase = OverlayPhase::Review;
                overlay.current_row = None;
                overlay.three_key_focus = ThreeKeyFocus::Rows;
                break;
            }
        }
        handled += 1;
    }

    overlay.scroll_index = overlay.scroll_index.min(scroll_limit(overlay.rows.len()));
    if overlay.auto_follow
        && let Some(index) = overlay.current_row
    {
        follow_row(overlay, index);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ApplyMode, OverlayPhase, OverlayStateData, RowDisposition, RowPhase, RowState,
        ThreeKeyFocus, can_save, collect_changes, result_text, row_disposition,
    };
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, mpsc};

    fn pack_row(bias_ms: f64, confidence: f64) -> RowState {
        pack_row_at("Songs/Test/song.ssc", bias_ms, confidence)
    }

    fn pack_row_at(path: &str, bias_ms: f64, confidence: f64) -> RowState {
        RowState {
            simfile_path: PathBuf::from(path),
            song_title: "Test Song".to_string(),
            chart_label: "Challenge".to_string(),
            total_beats: 100,
            beats_processed: 100,
            final_bias_ms: Some(bias_ms),
            final_confidence: Some(confidence),
            phase: RowPhase::Ready,
            error_text: None,
        }
    }

    fn test_overlay(rows: Vec<RowState>, apply_mode: ApplyMode) -> OverlayStateData {
        let (_tx, rx) = mpsc::channel();
        OverlayStateData {
            pack_name: "Test Pack".to_string(),
            rows,
            scroll_index: 0,
            auto_follow: false,
            yes_selected: true,
            phase: OverlayPhase::Review,
            min_confidence: 0.80,
            cancel: Arc::new(AtomicBool::new(false)),
            current_row: None,
            rx,
            menu_lr_chord: crate::screens::input::MenuLrChordTracker::default(),
            three_key_focus: ThreeKeyFocus::Rows,
            apply_mode,
        }
    }

    #[test]
    fn pack_sync_row_below_threshold_is_skipped() {
        let row = pack_row(12.5, 0.79);
        assert_eq!(row_disposition(&row, 0.80), RowDisposition::BelowThreshold);
    }

    #[test]
    fn pack_sync_result_text_labels_confidence() {
        let row = pack_row(12.5, 0.87);
        let text = result_text(&row, 0.80);
        assert!(text.contains("87% confidence"));
    }

    #[test]
    fn uniform_pack_sync_applies_one_delta_to_all_pack_files() {
        let rows = vec![
            pack_row_at("Songs/Pack/a.ssc", 10.0, 0.90),
            pack_row_at("Songs/Pack/b.ssc", 20.0, 0.90),
            pack_row_at("Songs/Pack/c.ssc", 99.0, 0.40),
        ];
        let overlay = test_overlay(
            rows,
            ApplyMode::Uniform {
                simfile_paths: vec![
                    PathBuf::from("Songs/Pack/a.ssc"),
                    PathBuf::from("Songs/Pack/b.ssc"),
                    PathBuf::from("Songs/Pack/c.ssc"),
                    PathBuf::from("Songs/Pack/b.ssc"),
                ],
            },
        );

        assert!(can_save(&overlay));
        let changes = collect_changes(&overlay);
        assert_eq!(changes.len(), 3);
        assert_eq!(changes[0].simfile_path, PathBuf::from("Songs/Pack/a.ssc"));
        assert_eq!(changes[1].simfile_path, PathBuf::from("Songs/Pack/b.ssc"));
        assert_eq!(changes[2].simfile_path, PathBuf::from("Songs/Pack/c.ssc"));
        for change in changes {
            assert!((change.delta_seconds + 0.015).abs() < f32::EPSILON);
        }
    }
}
