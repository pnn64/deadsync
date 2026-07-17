use crate::act;
use crate::assets::i18n::{tr, tr_fmt};
use crate::assets::{FontRole, machine_font_key};
use crate::config::MachineFont;
use crate::screens::components::shared::loading_bar;
use crate::screens::input as screen_input;
use deadlib_present::actors::Actor;
use deadlib_present::color;
use deadlib_present::space::{screen_center_x, screen_center_y, widescale};
use deadsync_chart::ChartData;
use deadsync_chart::SongData;
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_simfile::sync_offset::SongOffsetSyncChange;
use std::path::PathBuf;
use std::sync::Arc;

const OVERLAY_Z: i16 = 1496;
const VIEW_ROWS_RUNNING: usize = 7;
const VIEW_ROWS_REVIEW: usize = 5;
const ROW_STEP: f32 = 43.0;
pub(crate) struct TargetSpec {
    pub song: Arc<SongData>,
    pub simfile_path: PathBuf,
    pub song_title: String,
    pub chart_label: String,
    pub chart_ix: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct NavigationPolicy {
    pub only_dedicated_menu_buttons: bool,
    pub three_key_navigation: bool,
}

impl NavigationPolicy {
    #[inline(always)]
    const fn dedicated_three_key(self) -> bool {
        self.only_dedicated_menu_buttons && self.three_key_navigation
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OverlayPhase {
    Running,
    Review,
}

pub(crate) struct OverlayStateData {
    pack_name: String,
    rows: Vec<RowState>,
    scroll_index: usize,
    auto_follow: bool,
    yes_selected: bool,
    phase: OverlayPhase,
    min_confidence: f64,
    owner: crate::SimplyLoveSyncOwner,
    current_row: Option<usize>,
    menu_lr_chord: screen_input::MenuLrChordTracker,
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

fn confidence_threshold_percent(min_confidence: f64) -> u32 {
    (min_confidence.clamp(0.0, 1.0) * 100.0).round() as u32
}

#[inline(always)]
fn confidence_percent(confidence: Option<f64>) -> u32 {
    (confidence.unwrap_or(0.0).clamp(0.0, 1.0) * 100.0).round() as u32
}

pub(crate) fn build_overlay(
    state: &OverlayState,
    active_color_index: i32,
    machine_font: MachineFont,
) -> Option<Vec<Actor>> {
    let OverlayState::Visible(overlay) = state else {
        return None;
    };

    let summary = summary(overlay);
    let pane_w = widescale(580.0, 760.0);
    let pane_h = 470.0;
    let pane_cx = screen_center_x();
    let pane_cy = screen_center_y();
    let pane_left = pane_cx - pane_w * 0.5;
    let pane_top = pane_cy - pane_h * 0.5;
    let pane_right = pane_cx + pane_w * 0.5;
    let accent = color::simply_love_rgba(active_color_index);
    let fill = color::decorative_rgba(active_color_index);
    let view_rows = view_rows(overlay);
    let start = overlay
        .scroll_index
        .min(scroll_limit(overlay.rows.len(), view_rows));
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
        confidence_threshold_percent(overlay.min_confidence),
        summary.no_change,
        summary.failed
    );
    let scroll_text = (summary.total > view_rows).then(|| {
        tr_fmt(
            "PackSync",
            "RowsPaginationFormat",
            &[
                ("start", &(start + 1).to_string()),
                ("end", &(start + view_rows).min(summary.total).to_string()),
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
    let bar_x = pane_left + widescale(250.0, 360.0);
    let result_x = pane_right - 28.0;
    let row_top = pane_top + 138.0;

    let mut actors = Vec::with_capacity(96);
    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(pane_w + 2.0, pane_h + 2.0):
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
        font(machine_font_key(machine_font, FontRole::Header)):
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

    for (slot, row) in overlay.rows.iter().skip(start).take(view_rows).enumerate() {
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
            maxwidth(widescale(200.0, 310.0)):
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
            maxwidth(widescale(200.0, 310.0)):
            diffuse(0.72, 0.72, 0.72, 1.0):
            z(OVERLAY_Z + 4):
            horizalign(left)
        ));
        actors.push(loading_bar::build(loading_bar::LoadingBarParams {
            align: [0.0, 0.5],
            offset: [bar_x, row_y + 2.0],
            width: widescale(160.0, 220.0),
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
            maxwidth(widescale(140.0, 180.0)):
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
                    font(machine_font_key(machine_font, FontRole::Header)):
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
                    font(machine_font_key(machine_font, FontRole::Header)):
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

pub(crate) fn hide(state: &mut OverlayState) -> Option<crate::SimplyLoveSyncRequest> {
    let request = match state {
        OverlayState::Visible(overlay) if overlay.phase == OverlayPhase::Running => {
            Some(crate::SimplyLoveSyncRequest::CancelAnalysis(overlay.owner))
        }
        OverlayState::Hidden | OverlayState::Visible(_) => None,
    };
    *state = OverlayState::Hidden;
    request
}

pub(crate) fn begin(
    state: &mut OverlayState,
    owner: crate::SimplyLoveSyncOwner,
    pack_name: String,
    targets: Vec<TargetSpec>,
    confidence_percent: u8,
) -> Option<crate::SimplyLoveSyncRequest> {
    if targets.is_empty() {
        return None;
    }

    let min_confidence = f64::from(confidence_percent.min(100)) / 100.0;
    let rows = build_rows(&targets);
    let request_targets = targets
        .into_iter()
        .map(|target| crate::SimplyLoveSyncTarget {
            song: target.song,
            chart_ix: target.chart_ix,
        })
        .collect();

    *state = OverlayState::Visible(OverlayStateData {
        pack_name,
        rows,
        scroll_index: 0,
        auto_follow: true,
        yes_selected: true,
        phase: OverlayPhase::Running,
        min_confidence,
        owner,
        current_row: None,
        menu_lr_chord: screen_input::MenuLrChordTracker::default(),
    });
    Some(crate::SimplyLoveSyncRequest::StartAnalysis {
        owner,
        targets: request_targets,
        emit_freq_delta: false,
    })
}

pub(crate) fn poll(state: &mut OverlayState) -> bool {
    matches!(state, OverlayState::Visible(_))
}

pub(crate) fn handle_input(
    state: &mut OverlayState,
    ev: &InputEvent,
    navigation: NavigationPolicy,
) -> crate::screens::ThemeEffect {
    if screen_input::dedicated_blocks_arrow(ev.action, navigation.only_dedicated_menu_buttons) {
        return crate::screens::ThemeEffect::None;
    }

    let three_key_action = {
        let OverlayState::Visible(overlay) = state else {
            return crate::screens::ThemeEffect::None;
        };
        screen_input::three_key_menu_action_enabled(
            &mut overlay.menu_lr_chord,
            ev,
            navigation.dedicated_three_key(),
        )
    };
    if !ev.pressed {
        return crate::screens::ThemeEffect::None;
    }

    let mut close_overlay = false;
    let mut apply_changes: Option<Vec<SongOffsetSyncChange>> = None;
    let mut play_change = false;
    let mut play_start = false;

    {
        let OverlayState::Visible(overlay) = state else {
            return crate::screens::ThemeEffect::None;
        };
        let page_delta = view_rows(overlay).saturating_sub(1).max(1) as isize;
        if navigation.dedicated_three_key()
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
                OverlayPhase::Review => match nav {
                    screen_input::ThreeKeyMenuAction::Prev => {
                        if choose_review_answer(overlay, true) {
                            play_change = true;
                        }
                    }
                    screen_input::ThreeKeyMenuAction::Next => {
                        if choose_review_answer(overlay, false) {
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
                        close_overlay = true;
                        play_start = true;
                    }
                },
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
                OverlayPhase::Review => {
                    if let Some(delta) =
                        review_choice_delta(ev.action, navigation.only_dedicated_menu_buttons)
                    {
                        if choose_review_answer(overlay, delta < 0) {
                            play_change = true;
                        }
                    } else {
                        match ev.action {
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
                        }
                    }
                }
            }
        }
    }

    let mut effects = Vec::with_capacity(3);
    if play_change {
        effects.push(crate::effects::sfx("assets/sounds/change.ogg"));
    }
    if play_start {
        effects.push(crate::effects::sfx("assets/sounds/start.ogg"));
    }
    if close_overlay && let Some(request) = hide(state) {
        effects.push(crate::screens::ThemeEffect::Runtime(
            crate::SimplyLoveRuntimeRequest::Sync(request),
        ));
    }
    if let Some(changes) = apply_changes
        && !changes.is_empty()
    {
        effects.push(crate::screens::ThemeEffect::Runtime(
            crate::SimplyLoveRuntimeRequest::Sync(
                crate::SimplyLoveSyncRequest::ApplySongOffsetBatch { changes },
            ),
        ));
    }
    match effects.len() {
        0 => crate::screens::ThemeEffect::None,
        1 => effects.remove(0),
        _ => crate::screens::ThemeEffect::Batch(effects),
    }
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
    summary(overlay).eligible > 0
}

fn collect_changes(overlay: &OverlayStateData) -> Vec<SongOffsetSyncChange> {
    overlay
        .rows
        .iter()
        .filter(|row| row_disposition(row, overlay.min_confidence) == RowDisposition::Eligible)
        .filter_map(|row| {
            Some(SongOffsetSyncChange {
                simfile_path: row.simfile_path.clone(),
                delta_seconds: row_delta_seconds(row)?,
            })
        })
        .collect()
}

#[inline(always)]
fn choose_review_answer(overlay: &mut OverlayStateData, yes: bool) -> bool {
    if !can_save(overlay) || overlay.yes_selected == yes {
        return false;
    }
    overlay.yes_selected = yes;
    true
}

#[inline(always)]
const fn review_choice_delta(action: VirtualAction, dedicated_menu_only: bool) -> Option<i8> {
    if dedicated_menu_only && action.is_gameplay_arrow() {
        return None;
    }
    match action {
        VirtualAction::p1_left | VirtualAction::p2_left => Some(-1),
        VirtualAction::p1_right | VirtualAction::p2_right => Some(1),
        VirtualAction::p1_menu_left | VirtualAction::p2_menu_left => {
            if dedicated_menu_only {
                Some(-1)
            } else {
                None
            }
        }
        VirtualAction::p1_menu_right | VirtualAction::p2_menu_right => {
            if dedicated_menu_only {
                Some(1)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn save_prompt(overlay: &OverlayStateData) -> String {
    let summary = summary(overlay);
    let min_conf_pct = confidence_threshold_percent(overlay.min_confidence);
    if summary.eligible == 0 {
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
    tr_fmt(
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
    .to_string()
}

#[inline(always)]
fn scroll_limit(total: usize, view_rows: usize) -> usize {
    total.saturating_sub(view_rows)
}

fn view_rows(overlay: &OverlayStateData) -> usize {
    match overlay.phase {
        OverlayPhase::Running => VIEW_ROWS_RUNNING,
        OverlayPhase::Review => VIEW_ROWS_REVIEW,
    }
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
            &[(
                "threshold",
                &confidence_threshold_percent(min_confidence).to_string(),
            )],
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
    let view_rows = view_rows(overlay);
    if row_index < overlay.scroll_index {
        overlay.scroll_index = row_index;
        return;
    }
    let end = overlay.scroll_index + view_rows;
    if row_index >= end {
        overlay.scroll_index = row_index + 1 - view_rows;
    }
}

fn shift(overlay: &mut OverlayStateData, delta: isize) -> bool {
    let limit = scroll_limit(overlay.rows.len(), view_rows(overlay));
    let next = (overlay.scroll_index as isize + delta).clamp(0, limit as isize) as usize;
    if next == overlay.scroll_index {
        return false;
    }
    overlay.scroll_index = next;
    overlay.auto_follow = false;
    true
}

pub(crate) fn apply_event(state: &mut OverlayState, event: crate::SimplyLoveSyncEvent) {
    let OverlayState::Visible(overlay) = state else {
        return;
    };
    match event {
        crate::SimplyLoveSyncEvent::RowStarted { index } => {
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
        crate::SimplyLoveSyncEvent::RowInit { index, total_beats } => {
            if let Some(row) = overlay.rows.get_mut(index) {
                row.total_beats = total_beats;
                if overlay.auto_follow && overlay.current_row == Some(index) {
                    follow_row(overlay, index);
                }
            }
        }
        crate::SimplyLoveSyncEvent::RowBeat {
            index,
            beats_processed,
            total_beats,
        } => {
            if let Some(row) = overlay.rows.get_mut(index) {
                row.phase = RowPhase::Running;
                row.total_beats = row.total_beats.max(total_beats);
                row.beats_processed = row.beats_processed.max(beats_processed);
                if overlay.auto_follow && overlay.current_row == Some(index) {
                    follow_row(overlay, index);
                }
            }
        }
        crate::SimplyLoveSyncEvent::RowFinished { index, result } => {
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
        crate::SimplyLoveSyncEvent::Finished => {
            overlay.phase = OverlayPhase::Review;
            overlay.current_row = None;
        }
        crate::SimplyLoveSyncEvent::Disconnected => {
            overlay.phase = OverlayPhase::Review;
            overlay.current_row = None;
        }
        crate::SimplyLoveSyncEvent::SongStream(_) | crate::SimplyLoveSyncEvent::SongFinished(_) => {
            return;
        }
    }

    overlay.scroll_index = overlay
        .scroll_index
        .min(scroll_limit(overlay.rows.len(), view_rows(overlay)));
    if overlay.auto_follow
        && let Some(index) = overlay.current_row
    {
        follow_row(overlay, index);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        NavigationPolicy, RowDisposition, RowPhase, RowState, confidence_threshold_percent,
        result_text, review_choice_delta, row_disposition,
    };
    use deadsync_input::VirtualAction;
    use std::path::PathBuf;

    fn pack_row(bias_ms: f64, confidence: f64) -> RowState {
        RowState {
            simfile_path: PathBuf::from("Songs/Test/song.ssc"),
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
    fn pack_sync_runtime_policy_is_explicit_and_bounded() {
        assert_eq!(confidence_threshold_percent(0.805), 81);
        assert_eq!(confidence_threshold_percent(2.0), 100);
        assert!(
            NavigationPolicy {
                only_dedicated_menu_buttons: true,
                three_key_navigation: true,
            }
            .dedicated_three_key()
        );
        assert!(
            !NavigationPolicy {
                only_dedicated_menu_buttons: true,
                three_key_navigation: false,
            }
            .dedicated_three_key()
        );
    }

    #[test]
    fn pack_sync_review_uses_menu_lr_in_dedicated_menu_mode() {
        assert_eq!(
            review_choice_delta(VirtualAction::p1_menu_left, true),
            Some(-1)
        );
        assert_eq!(
            review_choice_delta(VirtualAction::p1_menu_right, true),
            Some(1)
        );
        assert_eq!(review_choice_delta(VirtualAction::p1_left, true), None);
        assert_eq!(review_choice_delta(VirtualAction::p1_right, true), None);
    }

    #[test]
    fn pack_sync_review_preserves_menu_lr_paging_without_dedicated_menu_mode() {
        assert_eq!(
            review_choice_delta(VirtualAction::p1_menu_left, false),
            None
        );
        assert_eq!(
            review_choice_delta(VirtualAction::p1_menu_right, false),
            None
        );
        assert_eq!(review_choice_delta(VirtualAction::p1_left, false), Some(-1));
        assert_eq!(review_choice_delta(VirtualAction::p1_right, false), Some(1));
    }
}
