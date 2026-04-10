use crate::act;
use crate::engine::input::{InputEvent, VirtualAction};
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use crate::game::online::downloads;
use crate::screens::components::shared::loading_bar;

const DOWNLOADS_Z: i16 = 1480;
const DOWNLOADS_PANEL_W: f32 = 520.0;
const DOWNLOADS_PANEL_H: f32 = 388.0;
const DOWNLOADS_ROW_STEP: f32 = 55.0;
const DOWNLOADS_VIEW_ROWS: usize = 6;
const DOWNLOADS_BAR_W: f32 = 350.0;
const DOWNLOADS_BAR_H: f32 = 20.0;
const DOWNLOADS_SEP_W: f32 = 480.0;
const DOWNLOADS_TITLE_Y: f32 = -170.0;
const DOWNLOADS_LIST_X: f32 = -240.0;
const DOWNLOADS_LIST_Y: f32 = -120.0;
const DOWNLOADS_AMOUNT_X: f32 = DOWNLOADS_BAR_W + 60.0;
const DOWNLOADS_CLOSE_HINT_Y: f32 = DOWNLOADS_PANEL_H * 0.5 + 36.0;
const DOWNLOADS_CLOSE_HINT: &str = "Press &START; to dismiss.";
const DOWNLOADS_EMPTY_TEXT: &str = "No Downloads to view";
const DOWNLOADS_DIM_ALPHA: f32 = 0.875;

#[derive(Clone, Debug)]
pub struct DownloadsOverlayStateData {
    scroll_index: usize,
}

#[derive(Clone, Debug)]
pub enum DownloadsOverlayState {
    Hidden,
    Visible(DownloadsOverlayStateData),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadsInputOutcome {
    None,
    ChangedSelection,
    Closed,
}

fn downloads_scroll_limit(total: usize) -> usize {
    total.saturating_sub(DOWNLOADS_VIEW_ROWS)
}

pub fn show_downloads_overlay() -> DownloadsOverlayState {
    DownloadsOverlayState::Visible(DownloadsOverlayStateData { scroll_index: 0 })
}

#[inline(always)]
pub fn hide_downloads_overlay(state: &mut DownloadsOverlayState) {
    *state = DownloadsOverlayState::Hidden;
}

pub fn update_downloads_overlay(state: &mut DownloadsOverlayState, _dt: f32) {
    let DownloadsOverlayState::Visible(overlay) = state else {
        return;
    };
    overlay.scroll_index = overlay
        .scroll_index
        .min(downloads_scroll_limit(downloads::snapshots().len()));
}

#[inline(always)]
fn downloads_shift(overlay: &mut DownloadsOverlayStateData, delta: isize) -> bool {
    let limit = downloads_scroll_limit(downloads::snapshots().len());
    let next = (overlay.scroll_index as isize + delta).clamp(0, limit as isize) as usize;
    if next == overlay.scroll_index {
        return false;
    }
    overlay.scroll_index = next;
    true
}

pub fn handle_downloads_input(
    state: &mut DownloadsOverlayState,
    ev: &InputEvent,
) -> DownloadsInputOutcome {
    if !ev.pressed {
        return DownloadsInputOutcome::None;
    }
    let DownloadsOverlayState::Visible(overlay) = state else {
        return DownloadsInputOutcome::None;
    };

    match ev.action {
        VirtualAction::p1_up
        | VirtualAction::p1_left
        | VirtualAction::p1_menu_up
        | VirtualAction::p1_menu_left
        | VirtualAction::p2_up
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_up
        | VirtualAction::p2_menu_left => {
            if downloads_shift(overlay, -1) {
                return DownloadsInputOutcome::ChangedSelection;
            }
        }
        VirtualAction::p1_down
        | VirtualAction::p1_right
        | VirtualAction::p1_menu_down
        | VirtualAction::p1_menu_right
        | VirtualAction::p2_down
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_down
        | VirtualAction::p2_menu_right => {
            if downloads_shift(overlay, 1) {
                return DownloadsInputOutcome::ChangedSelection;
            }
        }
        VirtualAction::p1_start
        | VirtualAction::p2_start
        | VirtualAction::p1_back
        | VirtualAction::p2_back
        | VirtualAction::p1_select
        | VirtualAction::p2_select => {
            hide_downloads_overlay(state);
            return DownloadsInputOutcome::Closed;
        }
        _ => {}
    }

    DownloadsInputOutcome::None
}

#[inline(always)]
fn download_percent(current_bytes: u64, total_bytes: u64) -> u32 {
    if total_bytes == 0 {
        return 0;
    }
    (((current_bytes.min(total_bytes)) * 100) / total_bytes) as u32
}

fn download_amount_text(current_bytes: u64, total_bytes: u64) -> String {
    let (suffix, divisor) = download_size(total_bytes);
    format!(
        "{}/{} {}",
        current_bytes / divisor,
        total_bytes / divisor,
        suffix
    )
}

#[inline(always)]
fn download_size(bytes: u64) -> (&'static str, u64) {
    if bytes >= 1024 * 1024 {
        ("MiB", 1024 * 1024)
    } else if bytes >= 1024 {
        ("KiB", 1024)
    } else {
        ("bytes", 1)
    }
}

pub fn build_downloads_overlay(
    state: &DownloadsOverlayState,
    active_color_index: i32,
) -> Option<Vec<Actor>> {
    let DownloadsOverlayState::Visible(overlay) = state else {
        return None;
    };
    let snapshots = downloads::snapshots();
    let (finished, total) = downloads::completion_counts();
    let mut actors = Vec::new();
    let center_x = screen_center_x();
    let center_y = screen_center_y();
    let fill = color::decorative_rgba(active_color_index);

    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, DOWNLOADS_DIM_ALPHA):
        z(DOWNLOADS_Z)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(center_x, center_y):
        zoomto(DOWNLOADS_PANEL_W + 2.0, DOWNLOADS_PANEL_H + 2.0):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(DOWNLOADS_Z + 1)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(center_x, center_y):
        zoomto(DOWNLOADS_PANEL_W, DOWNLOADS_PANEL_H):
        diffuse(0.0, 0.0, 0.0, 0.96):
        z(DOWNLOADS_Z + 2)
    ));
    actors.push(act!(text:
        font("wendy"):
        settext("View Downloads"):
        align(0.5, 0.5):
        xy(center_x, center_y + DOWNLOADS_TITLE_Y):
        zoom(0.54):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(DOWNLOADS_Z + 3)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(DOWNLOADS_CLOSE_HINT):
        align(0.5, 0.5):
        xy(center_x, center_y + DOWNLOADS_CLOSE_HINT_Y):
        zoom(0.95):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(DOWNLOADS_Z + 3):
        horizalign(center)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(format!("{finished}/{total}")):
        align(1.0, 0.5):
        xy(center_x + DOWNLOADS_PANEL_W * 0.5 - 18.0, center_y + DOWNLOADS_TITLE_Y):
        zoom(0.85):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(DOWNLOADS_Z + 3):
        horizalign(right)
    ));

    if snapshots.is_empty() {
        actors.push(act!(text:
            font("miso"):
            settext(DOWNLOADS_EMPTY_TEXT):
            align(0.5, 0.5):
            xy(center_x, center_y):
            zoom(1.25):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(DOWNLOADS_Z + 3):
            horizalign(center)
        ));
        return Some(actors);
    }

    let start = overlay
        .scroll_index
        .min(downloads_scroll_limit(snapshots.len()));
    for (slot, snapshot) in snapshots
        .iter()
        .skip(start)
        .take(DOWNLOADS_VIEW_ROWS)
        .enumerate()
    {
        let row_y = center_y + DOWNLOADS_LIST_Y + DOWNLOADS_ROW_STEP * slot as f32;
        let row_x = center_x + DOWNLOADS_LIST_X;
        let percent = download_percent(snapshot.current_bytes, snapshot.total_bytes);
        let progress = if snapshot.complete {
            1.0
        } else {
            percent as f32 / 100.0
        };
        let amount_text = download_amount_text(snapshot.current_bytes, snapshot.total_bytes);
        actors.push(act!(text:
            font("miso"):
            settext(format!("{}. {}", start + slot + 1, snapshot.name)):
            align(0.0, 0.5):
            xy(row_x, row_y):
            zoom(0.82):
            maxwidth(470.0):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(DOWNLOADS_Z + 3):
            horizalign(left)
        ));
        let bar_text = match snapshot.error_message.as_deref() {
            Some(message) if snapshot.complete => format!("Error: {message}"),
            None if snapshot.complete => "Done!".to_string(),
            _ => format!("{percent}%"),
        };
        actors.push(loading_bar::build(loading_bar::LoadingBarParams {
            align: [0.0, 0.5],
            offset: [row_x, row_y + 24.0],
            width: DOWNLOADS_BAR_W,
            height: DOWNLOADS_BAR_H,
            progress,
            label: bar_text.into(),
            fill_rgba: [fill[0], fill[1], fill[2], 1.0],
            bg_rgba: [0.0, 0.0, 0.0, 1.0],
            border_rgba: [1.0, 1.0, 1.0, 1.0],
            text_rgba: [1.0, 1.0, 1.0, 1.0],
            text_zoom: 0.82,
            z: DOWNLOADS_Z + 3,
        }));
        actors.push(act!(text:
            font("miso"):
            settext(amount_text):
            align(0.0, 0.5):
            xy(row_x + DOWNLOADS_AMOUNT_X, row_y + 24.0):
            zoom(0.82):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(DOWNLOADS_Z + 6):
            horizalign(left)
        ));
        actors.push(act!(quad:
            align(0.0, 0.5):
            xy(row_x, row_y + 40.0):
            zoomto(DOWNLOADS_SEP_W, 1.0):
            diffuse(1.0, 1.0, 1.0, 0.7):
            z(DOWNLOADS_Z + 2)
        ));
    }

    Some(actors)
}

