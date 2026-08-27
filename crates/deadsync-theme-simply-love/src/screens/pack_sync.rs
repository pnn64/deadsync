use crate::act;
use crate::assets::i18n::{tr, tr_fmt};
use crate::assets::{FontRole, machine_font_key};
use crate::config::MachineFont;
use crate::screens::components::shared::loading_bar;
use crate::screens::input as screen_input;
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::color;
use deadlib_present::space::{screen_center_x, screen_center_y, widescale};
use deadsync_chart::ChartData;
use deadsync_chart::SongData;
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_simfile::sync_offset::{SongOffsetSyncChange, quantize_sync_offset_seconds};
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
    Cached,
    Ready,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowDisposition {
    Pending,
    Running,
    Cached,
    Eligible,
    BelowThreshold,
    NoChange,
    Failed,
}

struct RowState {
    simfile_path: PathBuf,
    text: RowText,
    total_beats: usize,
    beats_processed: usize,
    final_bias_ms: Option<f64>,
    final_confidence: Option<f64>,
    phase: RowPhase,
    error_text: Option<String>,
}

struct RowText {
    title: TextContent,
    chart: TextContent,
    bar: TextContent,
    result: TextContent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OverlayPhase {
    Running,
    Review,
}

pub(crate) struct OverlayStateData {
    rows: Vec<RowState>,
    summary: Summary,
    text: OverlayText,
    scroll_index: usize,
    auto_follow: bool,
    yes_selected: bool,
    phase: OverlayPhase,
    min_confidence: f64,
    owner: crate::SimplyLoveSyncOwner,
    current_row: Option<usize>,
    menu_lr_chord: screen_input::MenuLrChordTracker,
}

struct OverlayText {
    pack_name: TextContent,
    title: TextContent,
    counts: TextContent,
    pagination: Option<TextContent>,
    song_column: TextContent,
    progress_column: TextContent,
    result_column: TextContent,
    prompt: TextContent,
    yes_option: TextContent,
    no_option: TextContent,
    help: TextContent,
}

pub(crate) enum OverlayState {
    Hidden,
    Visible(Box<OverlayStateData>),
}

#[derive(Clone, Copy, Debug, Default)]
struct Summary {
    analyzed: usize,
    total: usize,
    cached: usize,
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

#[inline]
fn retained_text(args: std::fmt::Arguments<'_>) -> TextContent {
    TextContent::inline_format(args)
        .unwrap_or_else(|| TextContent::Shared(Arc::from(args.to_string())))
}

#[inline]
fn retained_str(value: &str) -> TextContent {
    TextContent::inline_str(value).unwrap_or_else(|| TextContent::Shared(Arc::from(value)))
}

fn retained_arc(value: Arc<str>) -> TextContent {
    TextContent::inline_str(value.as_ref()).unwrap_or(TextContent::Shared(value))
}

fn localized(key: &str) -> TextContent {
    retained_arc(tr("PackSync", key))
}

fn overlay_title(phase: OverlayPhase, can_save: bool) -> TextContent {
    localized(match (phase, can_save) {
        (OverlayPhase::Running, _) => "SyncingPackTitle",
        (OverlayPhase::Review, true) => "ReviewTitle",
        (OverlayPhase::Review, false) => "CompleteTitle",
    })
}

fn counts_text(summary: Summary, min_confidence: f64) -> TextContent {
    retained_arc(tr_fmt(
        "PackSync",
        "CountsFormat",
        &[
            (
                "processed",
                &(summary.analyzed + summary.cached).to_string(),
            ),
            ("total", &summary.total.to_string()),
            ("cached", &summary.cached.to_string()),
            ("ready", &summary.eligible.to_string()),
            ("below", &summary.below_threshold.to_string()),
            (
                "threshold",
                &confidence_threshold_percent(min_confidence).to_string(),
            ),
            ("nochange", &summary.no_change.to_string()),
            ("failed", &summary.failed.to_string()),
        ],
    ))
}

fn pagination_text(total: usize, start: usize, view_rows: usize) -> Option<TextContent> {
    (total > view_rows).then(|| {
        retained_arc(tr_fmt(
            "PackSync",
            "RowsPaginationFormat",
            &[
                ("start", &(start + 1).to_string()),
                ("end", &(start + view_rows).min(total).to_string()),
                ("total", &total.to_string()),
            ],
        ))
    })
}

fn prompt_text(summary: Summary, min_confidence: f64) -> TextContent {
    let min_conf_pct = confidence_threshold_percent(min_confidence);
    let text = if summary.eligible == 0 {
        tr_fmt(
            "PackSync",
            "NothingToSaveMessage",
            &[
                ("below", &summary.below_threshold.to_string()),
                ("threshold", &min_conf_pct.to_string()),
                ("nochange", &summary.no_change.to_string()),
                ("failed", &summary.failed.to_string()),
            ],
        )
    } else {
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
    };
    retained_arc(text)
}

fn overlay_help(phase: OverlayPhase, can_save: bool) -> TextContent {
    localized(match (phase, can_save) {
        (OverlayPhase::Running, _) => "HelpTextRunning",
        (OverlayPhase::Review, true) => "HelpTextReview",
        (OverlayPhase::Review, false) => "HelpTextComplete",
    })
}

fn build_overlay_text(
    pack_name: &str,
    summary: Summary,
    min_confidence: f64,
    phase: OverlayPhase,
    scroll_index: usize,
) -> OverlayText {
    let can_save = summary.eligible > 0;
    OverlayText {
        pack_name: retained_str(pack_name),
        title: overlay_title(phase, can_save),
        counts: counts_text(summary, min_confidence),
        pagination: pagination_text(summary.total, scroll_index, view_rows_for_phase(phase)),
        song_column: localized("SongColumnHeader"),
        progress_column: localized("ProgressColumnHeader"),
        result_column: localized("ResultColumnHeader"),
        prompt: if phase == OverlayPhase::Review {
            prompt_text(summary, min_confidence)
        } else {
            TextContent::Static("")
        },
        yes_option: localized("YesOption"),
        no_option: localized("NoOption"),
        help: overlay_help(phase, can_save),
    }
}

fn refresh_counts_text(overlay: &mut OverlayStateData) {
    overlay.text.counts = counts_text(overlay.summary, overlay.min_confidence);
}

fn refresh_phase_text(overlay: &mut OverlayStateData) {
    let can_save = can_save(overlay);
    overlay.text.title = overlay_title(overlay.phase, can_save);
    overlay.text.help = overlay_help(overlay.phase, can_save);
    overlay.text.prompt = if overlay.phase == OverlayPhase::Review {
        prompt_text(overlay.summary, overlay.min_confidence)
    } else {
        TextContent::Static("")
    };
}

fn refresh_pagination_text(overlay: &mut OverlayStateData) {
    overlay.text.pagination = pagination_text(
        overlay.summary.total,
        overlay.scroll_index,
        view_rows(overlay),
    );
}

pub(crate) fn build_overlay(
    state: &OverlayState,
    active_color_index: i32,
    machine_font: MachineFont,
) -> Option<Vec<Actor>> {
    let OverlayState::Visible(overlay) = state else {
        return None;
    };

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
    let counts_maxwidth = if overlay.text.pagination.is_some() {
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
        settext(overlay.text.title.clone()):
        align(0.5, 0.5):
        xy(pane_cx, pane_top + 28.0):
        zoom(0.6):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(OVERLAY_Z + 3):
        horizalign(center)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(overlay.text.pack_name.clone()):
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
        settext(overlay.text.counts.clone()):
        align(0.0, 0.5):
        xy(song_x, pane_top + 86.0):
        zoom(0.8):
        maxwidth(counts_maxwidth):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(OVERLAY_Z + 3):
        horizalign(left)
    ));
    if let Some(scroll_text) = &overlay.text.pagination {
        actors.push(act!(text:
            font("miso"):
            settext(scroll_text.clone()):
            align(1.0, 0.5):
            xy(result_x, pane_top + 86.0):
            zoom(0.8):
            diffuse(0.82, 0.82, 0.82, 1.0):
            z(OVERLAY_Z + 3):
            horizalign(right)
        ));
    }
    actors.push(act!(text:
        font("miso"):
        settext(overlay.text.song_column.clone()):
        align(0.0, 0.5):
        xy(song_x, row_top - 20.0):
        zoom(0.75):
        diffuse(0.6, 0.6, 0.6, 1.0):
        z(OVERLAY_Z + 3):
        horizalign(left)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(overlay.text.progress_column.clone()):
        align(0.0, 0.5):
        xy(bar_x, row_top - 20.0):
        zoom(0.75):
        diffuse(0.6, 0.6, 0.6, 1.0):
        z(OVERLAY_Z + 3):
        horizalign(left)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(overlay.text.result_column.clone()):
        align(1.0, 0.5):
        xy(result_x, row_top - 20.0):
        zoom(0.75):
        diffuse(0.6, 0.6, 0.6, 1.0):
        z(OVERLAY_Z + 3):
        horizalign(right)
    ));

    for (slot, row) in overlay.rows.iter().skip(start).take(view_rows).enumerate() {
        let row_index = start + slot;
        let row_y = ROW_STEP.mul_add(slot as f32, row_top);
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
            RowDisposition::Cached => [0.62, 0.82, 0.62, 1.0],
            RowDisposition::BelowThreshold => [1.0, 0.82, 0.32, 1.0],
            RowDisposition::NoChange => [0.72, 0.72, 0.72, 1.0],
            RowDisposition::Failed => [1.0, 0.35, 0.35, 1.0],
            _ => [1.0, 1.0, 1.0, 1.0],
        };

        actors.push(act!(text:
            font("miso"):
            settext(row.text.title.clone()):
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
            settext(row.text.chart.clone()):
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
            label: row.text.bar.clone(),
            fill_rgba: [fill[0], fill[1], fill[2], 1.0],
            bg_rgba: [0.0, 0.0, 0.0, 1.0],
            border_rgba: [1.0, 1.0, 1.0, 1.0],
            text_rgba: [1.0, 1.0, 1.0, 1.0],
            text_zoom: 0.72,
            z: OVERLAY_Z + 4,
        }));
        actors.push(act!(text:
            font("miso"):
            settext(row.text.result.clone()):
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
            actors.push(act!(text:
                font("miso"):
                settext(overlay.text.help.clone()):
                align(0.5, 0.5):
                xy(pane_cx, pane_top + pane_h - 24.0):
                zoom(0.8):
                diffuse(0.85, 0.85, 0.85, 1.0):
                z(OVERLAY_Z + 4):
                horizalign(center)
            ));
        }
        OverlayPhase::Review => {
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
                    settext(overlay.text.prompt.clone()):
                    align(0.5, 0.5):
                    xy(pane_cx, pane_top + pane_h - 92.0):
                    zoom(0.86):
                    maxwidth(pane_w - 90.0):
                    diffuse(1.0, 1.0, 1.0, 1.0):
                    z(OVERLAY_Z + 4):
                    horizalign(center)
                ));
                actors.push(act!(text:
                    font(machine_font_key(machine_font, FontRole::Header)):
                    settext(overlay.text.yes_option.clone()):
                    align(0.5, 0.5):
                    xy(choice_yes_x, answer_y):
                    zoom(0.72):
                    diffuse(1.0, 1.0, 1.0, 1.0):
                    z(OVERLAY_Z + 4):
                    horizalign(center)
                ));
                actors.push(act!(text:
                    font(machine_font_key(machine_font, FontRole::Header)):
                    settext(overlay.text.no_option.clone()):
                    align(0.5, 0.5):
                    xy(choice_no_x, answer_y):
                    zoom(0.72):
                    diffuse(1.0, 1.0, 1.0, 1.0):
                    z(OVERLAY_Z + 4):
                    horizalign(center)
                ));
                actors.push(act!(text:
                    font("miso"):
                    settext(overlay.text.help.clone()):
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
                    settext(overlay.text.prompt.clone()):
                    align(0.5, 0.5):
                    xy(pane_cx, pane_top + pane_h - 56.0):
                    zoom(0.84):
                    maxwidth(pane_w - 90.0):
                    diffuse(1.0, 1.0, 1.0, 1.0):
                    z(OVERLAY_Z + 4):
                    horizalign(center)
                ));
                actors.push(act!(text:
                    font("miso"):
                    settext(overlay.text.help.clone()):
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
    let (rows, request_targets) = build_rows(targets, min_confidence);
    let summary = Summary {
        total: rows.len(),
        ..Summary::default()
    };
    let phase = OverlayPhase::Running;
    let scroll_index = 0;
    let text = build_overlay_text(&pack_name, summary, min_confidence, phase, scroll_index);

    *state = OverlayState::Visible(Box::new(OverlayStateData {
        rows,
        summary,
        text,
        scroll_index,
        auto_follow: true,
        yes_selected: true,
        phase,
        min_confidence,
        owner,
        current_row: None,
        menu_lr_chord: screen_input::MenuLrChordTracker::default(),
    }));
    Some(crate::SimplyLoveSyncRequest::StartAnalysis {
        owner,
        targets: request_targets,
        emit_freq_delta: false,
    })
}

pub(crate) const fn poll(state: &mut OverlayState) -> bool {
    matches!(state, OverlayState::Visible(_))
}

pub(crate) fn handle_input(
    state: &mut OverlayState,
    ev: &InputEvent,
    navigation: NavigationPolicy,
    effects: &mut Vec<crate::screens::ThemeEffect>,
) {
    if screen_input::dedicated_blocks_arrow(ev.action, navigation.only_dedicated_menu_buttons) {
        return;
    }

    let three_key_action = {
        let OverlayState::Visible(overlay) = state else {
            return;
        };
        screen_input::three_key_menu_action_enabled(
            &mut overlay.menu_lr_chord,
            ev,
            navigation.dedicated_three_key(),
        )
    };
    if !ev.pressed {
        return;
    }

    let mut close_overlay = false;
    let mut apply_changes: Option<Vec<SongOffsetSyncChange>> = None;
    let mut play_change = false;
    let mut play_start = false;

    {
        let OverlayState::Visible(overlay) = state else {
            return;
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
}

fn build_rows(
    targets: Vec<TargetSpec>,
    min_confidence: f64,
) -> (Vec<RowState>, Vec<crate::SimplyLoveSyncTarget>) {
    let mut rows = Vec::with_capacity(targets.len());
    let mut request_targets = Vec::with_capacity(targets.len());
    for (index, target) in targets.into_iter().enumerate() {
        let mut row = RowState {
            simfile_path: target.simfile_path,
            text: RowText {
                title: retained_text(format_args!("{}. {}", index + 1, target.song_title)),
                chart: retained_str(&target.chart_label),
                bar: TextContent::Static(""),
                result: TextContent::Static(""),
            },
            total_beats: 0,
            beats_processed: 0,
            final_bias_ms: None,
            final_confidence: None,
            phase: RowPhase::Pending,
            error_text: None,
        };
        refresh_row_text(&mut row, min_confidence);
        rows.push(row);
        request_targets.push(crate::SimplyLoveSyncTarget {
            song: target.song,
            chart_ix: target.chart_ix,
        });
    }
    (rows, request_targets)
}

#[inline(always)]
fn row_delta_seconds(row: &RowState) -> Option<f32> {
    row.final_bias_ms
        .map(|bias_ms| -(bias_ms as f32) * 0.001)
        .filter(|v| v.is_finite())
        .map(quantize_sync_offset_seconds)
}

fn row_disposition(row: &RowState, min_confidence: f64) -> RowDisposition {
    match row.phase {
        RowPhase::Pending => RowDisposition::Pending,
        RowPhase::Running => RowDisposition::Running,
        RowPhase::Cached => RowDisposition::Cached,
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

const fn add_disposition(summary: &mut Summary, disposition: RowDisposition) {
    let counter = match disposition {
        RowDisposition::Pending | RowDisposition::Running => return,
        RowDisposition::Cached => {
            summary.cached += 1;
            return;
        }
        RowDisposition::Eligible => &mut summary.eligible,
        RowDisposition::BelowThreshold => &mut summary.below_threshold,
        RowDisposition::NoChange => &mut summary.no_change,
        RowDisposition::Failed => &mut summary.failed,
    };
    summary.analyzed += 1;
    *counter += 1;
}

const fn remove_disposition(summary: &mut Summary, disposition: RowDisposition) {
    let counter = match disposition {
        RowDisposition::Pending | RowDisposition::Running => return,
        RowDisposition::Cached => {
            summary.cached = summary.cached.saturating_sub(1);
            return;
        }
        RowDisposition::Eligible => &mut summary.eligible,
        RowDisposition::BelowThreshold => &mut summary.below_threshold,
        RowDisposition::NoChange => &mut summary.no_change,
        RowDisposition::Failed => &mut summary.failed,
    };
    summary.analyzed = summary.analyzed.saturating_sub(1);
    *counter = counter.saturating_sub(1);
}

fn replace_disposition(
    summary: &mut Summary,
    previous: RowDisposition,
    current: RowDisposition,
) -> bool {
    if previous == current {
        return false;
    }
    remove_disposition(summary, previous);
    add_disposition(summary, current);
    true
}

#[inline(always)]
const fn can_save(overlay: &OverlayStateData) -> bool {
    overlay.summary.eligible > 0
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
const fn choose_review_answer(overlay: &mut OverlayStateData, yes: bool) -> bool {
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

#[inline(always)]
const fn scroll_limit(total: usize, view_rows: usize) -> usize {
    total.saturating_sub(view_rows)
}

const fn view_rows(overlay: &OverlayStateData) -> usize {
    view_rows_for_phase(overlay.phase)
}

const fn view_rows_for_phase(phase: OverlayPhase) -> usize {
    match phase {
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
        RowPhase::Cached | RowPhase::Ready | RowPhase::Failed => 1.0,
    }
}

fn bar_text(row: &RowState, min_confidence: f64) -> TextContent {
    let text = match row_disposition(row, min_confidence) {
        RowDisposition::Pending => tr("PackSync", "StatusQueued"),
        RowDisposition::Running => match row.total_beats.max(row.beats_processed) {
            0 => tr("PackSync", "StatusStarting"),
            total => tr_fmt(
                "PackSync",
                "ProgressFormat",
                &[
                    ("current", &row.beats_processed.min(total).to_string()),
                    ("total", &total.to_string()),
                ],
            ),
        },
        RowDisposition::Cached => tr("PackSync", "StatusCached"),
        RowDisposition::Eligible => tr("PackSync", "StatusReady"),
        RowDisposition::BelowThreshold => tr_fmt(
            "PackSync",
            "StatusBelowThresholdFormat",
            &[(
                "threshold",
                &confidence_threshold_percent(min_confidence).to_string(),
            )],
        ),
        RowDisposition::NoChange => tr("PackSync", "StatusNoChange"),
        RowDisposition::Failed => tr("PackSync", "StatusError"),
    };
    retained_arc(text)
}

fn result_text(row: &RowState, min_confidence: f64) -> TextContent {
    let confidence_pct = confidence_percent(row.final_confidence);
    match row_disposition(row, min_confidence) {
        RowDisposition::Pending => retained_arc(tr("PackSync", "StatusQueued")),
        RowDisposition::Running => {
            if let Some(bias_ms) = row.final_bias_ms {
                retained_text(format_args!("{bias_ms:+.2} ms"))
            } else {
                retained_arc(tr("PackSync", "StatusWorking"))
            }
        }
        RowDisposition::Cached => retained_arc(tr("PackSync", "StatusCached")),
        RowDisposition::Eligible | RowDisposition::BelowThreshold => retained_arc(tr_fmt(
            "PackSync",
            "ResultConfidenceFormat",
            &[
                (
                    "adjustment",
                    &format!("{:+.0}", row_delta_seconds(row).unwrap_or(0.0) * 1_000.0),
                ),
                ("confidence", &confidence_pct.to_string()),
            ],
        )),
        RowDisposition::NoChange => retained_arc(tr_fmt(
            "PackSync",
            "ResultNoChangeFormat",
            &[("confidence", &confidence_pct.to_string())],
        )),
        RowDisposition::Failed => row
            .error_text
            .as_deref()
            .map(retained_str)
            .unwrap_or_else(|| retained_arc(tr("PackSync", "AnalysisFailed"))),
    }
}

fn refresh_row_text(row: &mut RowState, min_confidence: f64) {
    let bar = bar_text(row, min_confidence);
    let result = result_text(row, min_confidence);
    row.text.bar = bar;
    row.text.result = result;
}

const fn follow_row(overlay: &mut OverlayStateData, row_index: usize) {
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
    refresh_pagination_text(overlay);
    true
}

pub(crate) fn apply_event(state: &mut OverlayState, event: crate::SimplyLoveSyncEvent) {
    let OverlayState::Visible(overlay) = state else {
        return;
    };
    let previous_phase = overlay.phase;
    let previous_scroll = overlay.scroll_index;
    let mut summary_changed = false;
    match event {
        crate::SimplyLoveSyncEvent::RowStarted { index } => {
            if let Some(row) = overlay.rows.get_mut(index) {
                let previous = row_disposition(row, overlay.min_confidence);
                row.total_beats = 0;
                row.beats_processed = 0;
                row.final_bias_ms = None;
                row.final_confidence = None;
                row.phase = RowPhase::Running;
                row.error_text = None;
                refresh_row_text(row, overlay.min_confidence);
                let current = row_disposition(row, overlay.min_confidence);
                summary_changed |= replace_disposition(&mut overlay.summary, previous, current);
                overlay.current_row = Some(index);
                if overlay.auto_follow {
                    follow_row(overlay, index);
                }
            }
        }
        crate::SimplyLoveSyncEvent::RowInit { index, total_beats } => {
            if let Some(row) = overlay.rows.get_mut(index) {
                row.total_beats = total_beats;
                refresh_row_text(row, overlay.min_confidence);
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
                let previous = row_disposition(row, overlay.min_confidence);
                row.phase = RowPhase::Running;
                row.total_beats = row.total_beats.max(total_beats);
                row.beats_processed = row.beats_processed.max(beats_processed);
                refresh_row_text(row, overlay.min_confidence);
                let current = row_disposition(row, overlay.min_confidence);
                summary_changed |= replace_disposition(&mut overlay.summary, previous, current);
                if overlay.auto_follow && overlay.current_row == Some(index) {
                    follow_row(overlay, index);
                }
            }
        }
        crate::SimplyLoveSyncEvent::RowCached { index } => {
            if let Some(row) = overlay.rows.get_mut(index) {
                let previous = row_disposition(row, overlay.min_confidence);
                row.phase = RowPhase::Cached;
                row.total_beats = 0;
                row.beats_processed = 0;
                row.final_bias_ms = None;
                row.final_confidence = None;
                row.error_text = None;
                refresh_row_text(row, overlay.min_confidence);
                let current = row_disposition(row, overlay.min_confidence);
                summary_changed |= replace_disposition(&mut overlay.summary, previous, current);
            }
        }
        crate::SimplyLoveSyncEvent::RowFinished { index, result } => {
            if let Some(row) = overlay.rows.get_mut(index) {
                let previous = row_disposition(row, overlay.min_confidence);
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
                refresh_row_text(row, overlay.min_confidence);
                let current = row_disposition(row, overlay.min_confidence);
                summary_changed |= replace_disposition(&mut overlay.summary, previous, current);
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
    let phase_changed = overlay.phase != previous_phase;
    if summary_changed {
        refresh_counts_text(overlay);
    }
    if phase_changed || (summary_changed && overlay.phase == OverlayPhase::Review) {
        refresh_phase_text(overlay);
    }
    if phase_changed || overlay.scroll_index != previous_scroll {
        refresh_pagination_text(overlay);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        NavigationPolicy, OverlayPhase, OverlayState, OverlayStateData, RowDisposition, RowPhase,
        RowState, RowText, Summary, build_overlay_text, can_save, confidence_threshold_percent,
        refresh_row_text, result_text, review_choice_delta, row_disposition,
    };
    use crate::screens::ThemeEffect;
    use deadlib_present::actors::TextContent;
    use deadsync_core::input::InputSource;
    use deadsync_input::{InputEvent, VirtualAction};
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    fn pack_row(bias_ms: f64, confidence: f64) -> RowState {
        let mut row = RowState {
            simfile_path: PathBuf::from("Songs/Test/song.ssc"),
            text: RowText {
                title: TextContent::Static("1. Test Song"),
                chart: TextContent::Static("Challenge"),
                bar: TextContent::Static(""),
                result: TextContent::Static(""),
            },
            total_beats: 100,
            beats_processed: 100,
            final_bias_ms: Some(bias_ms),
            final_confidence: Some(confidence),
            phase: RowPhase::Ready,
            error_text: None,
        };
        refresh_row_text(&mut row, 0.80);
        row
    }

    fn overlay(phase: OverlayPhase) -> OverlayState {
        let rows = vec![pack_row(12.5, 0.87)];
        let summary = Summary {
            analyzed: 1,
            total: 1,
            eligible: 1,
            ..Summary::default()
        };
        OverlayState::Visible(Box::new(OverlayStateData {
            rows,
            summary,
            text: build_overlay_text("Test Pack", summary, 0.80, phase, 0),
            scroll_index: 0,
            auto_follow: false,
            yes_selected: true,
            phase,
            min_confidence: 0.80,
            owner: crate::SimplyLoveSyncOwner::SelectMusicPack,
            current_row: None,
            menu_lr_chord: crate::screens::input::MenuLrChordTracker::default(),
        }))
    }

    fn press(action: VirtualAction) -> InputEvent {
        let now = Instant::now();
        InputEvent::new(action, 0, true, InputSource::Keyboard, now, 0, now, now)
    }

    #[test]
    fn pack_sync_row_below_threshold_is_skipped() {
        let row = pack_row(12.5, 0.79);
        assert_eq!(row_disposition(&row, 0.80), RowDisposition::BelowThreshold);
    }

    #[test]
    fn pack_sync_result_text_labels_confidence() {
        let row = pack_row(-2.72, 0.87);
        let text = result_text(&row, 0.80);
        assert!(text.as_str().contains("+3 ms"));
        assert!(!text.as_str().contains("-2.72 ms"));
        assert!(text.as_str().contains("87% confidence"));
    }

    #[test]
    fn pack_sync_uses_same_whole_millisecond_adjustment_as_song_sync() {
        let no_change = pack_row(-0.49, 0.87);
        assert_eq!(row_disposition(&no_change, 0.80), RowDisposition::NoChange);

        let eligible = pack_row(-0.50, 0.87);
        assert_eq!(row_disposition(&eligible, 0.80), RowDisposition::Eligible);
        assert_eq!(super::row_delta_seconds(&eligible), Some(0.001));
    }

    #[test]
    fn pack_sync_row_text_refreshes_at_worker_events() {
        let mut state = overlay(OverlayPhase::Running);

        super::apply_event(
            &mut state,
            crate::SimplyLoveSyncEvent::RowStarted { index: 0 },
        );
        let OverlayState::Visible(overlay) = &state else {
            panic!("pack sync overlay should remain visible");
        };
        assert_eq!(overlay.rows[0].text.bar.as_str(), "Starting");
        assert_eq!(overlay.rows[0].text.result.as_str(), "Working");

        super::apply_event(
            &mut state,
            crate::SimplyLoveSyncEvent::RowBeat {
                index: 0,
                beats_processed: 25,
                total_beats: 100,
            },
        );
        let OverlayState::Visible(overlay) = &state else {
            panic!("pack sync overlay should remain visible");
        };
        assert!(overlay.rows[0].text.bar.as_str().contains("25"));
        assert!(overlay.rows[0].text.bar.as_str().contains("100"));
    }

    #[test]
    fn pack_sync_summary_text_refreshes_at_source_transitions() {
        let mut state = overlay(OverlayPhase::Running);

        super::apply_event(
            &mut state,
            crate::SimplyLoveSyncEvent::RowStarted { index: 0 },
        );
        let OverlayState::Visible(overlay) = &state else {
            panic!("pack sync overlay should remain visible");
        };
        assert_eq!(overlay.summary.analyzed, 0);
        assert_eq!(overlay.summary.eligible, 0);
        assert!(overlay.text.counts.as_str().starts_with("0/1"));

        super::apply_event(
            &mut state,
            crate::SimplyLoveSyncEvent::RowFinished {
                index: 0,
                result: Ok(crate::SimplyLoveSyncResult {
                    bias_ms: 12.5,
                    confidence: 0.87,
                }),
            },
        );
        super::apply_event(&mut state, crate::SimplyLoveSyncEvent::Finished);
        let OverlayState::Visible(overlay) = &state else {
            panic!("pack sync overlay should remain visible");
        };
        assert_eq!(overlay.summary.analyzed, 1);
        assert_eq!(overlay.summary.eligible, 1);
        assert!(overlay.text.counts.as_str().starts_with("1/1"));
        assert!(overlay.text.prompt.as_str().contains('1'));
        assert!(overlay.text.help.as_str().contains("ACCEPT"));
    }

    #[test]
    fn pack_sync_cached_row_is_processed_but_not_saveable() {
        let mut state = overlay(OverlayPhase::Running);

        super::apply_event(
            &mut state,
            crate::SimplyLoveSyncEvent::RowCached { index: 0 },
        );
        super::apply_event(&mut state, crate::SimplyLoveSyncEvent::Finished);

        let OverlayState::Visible(overlay) = &state else {
            panic!("pack sync overlay should remain visible");
        };
        assert_eq!(overlay.summary.analyzed, 0);
        assert_eq!(overlay.summary.cached, 1);
        assert_eq!(overlay.summary.eligible, 0);
        assert_eq!(overlay.rows[0].text.result.as_str(), "Cached");
        assert!(overlay.text.counts.as_str().starts_with("1/1"));
        assert!(!can_save(overlay));
    }

    #[test]
    fn pack_sync_oversized_row_text_is_pointer_shared() {
        let text = super::retained_str("an external pack label too long for inline text");
        let clone = text.clone();
        let (TextContent::Shared(text), TextContent::Shared(clone)) = (&text, &clone) else {
            panic!("oversized retained text should use shared storage");
        };
        assert!(std::sync::Arc::ptr_eq(text, clone));
    }

    #[test]
    fn pack_sync_retains_localized_arc_without_copying() {
        let source =
            std::sync::Arc::<str>::from("a localized Pack Sync label too long for inline text");
        let text = super::retained_arc(std::sync::Arc::clone(&source));
        let TextContent::Shared(text) = text else {
            panic!("oversized localized text should keep shared storage");
        };
        assert!(std::sync::Arc::ptr_eq(&source, &text));
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

    #[test]
    fn running_cancel_appends_sound_before_sync_cancel() {
        let mut state = overlay(OverlayPhase::Running);
        let mut effects = Vec::with_capacity(8);

        super::handle_input(
            &mut state,
            &press(VirtualAction::p1_start),
            NavigationPolicy::default(),
            &mut effects,
        );

        assert!(matches!(state, OverlayState::Hidden));
        assert!(matches!(
            effects.as_slice(),
            [
                ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
                    deadsync_theme::AudioRequest::PlaySfx(path)
                )),
                ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Sync(
                    crate::SimplyLoveSyncRequest::CancelAnalysis(
                        crate::SimplyLoveSyncOwner::SelectMusicPack
                    )
                )),
            ] if *path == "assets/sounds/start.ogg"
        ));
        assert_eq!(effects.capacity(), 8);
    }

    #[test]
    fn review_confirm_appends_sound_before_offset_changes() {
        let mut state = overlay(OverlayPhase::Review);
        let mut effects = Vec::with_capacity(8);

        super::handle_input(
            &mut state,
            &press(VirtualAction::p1_start),
            NavigationPolicy::default(),
            &mut effects,
        );

        assert!(matches!(state, OverlayState::Hidden));
        assert!(matches!(
            effects.as_slice(),
            [
                ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
                    deadsync_theme::AudioRequest::PlaySfx(path)
                )),
                ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Sync(
                    crate::SimplyLoveSyncRequest::ApplySongOffsetBatch { changes }
                )),
            ] if *path == "assets/sounds/start.ogg"
                && matches!(changes.as_slice(), [change]
                    if change.simfile_path == Path::new("Songs/Test/song.ssc")
                        && (change.delta_seconds + 0.013).abs() <= f32::EPSILON)
        ));
        assert_eq!(effects.capacity(), 8);
    }
}
