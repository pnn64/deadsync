use crate::act;
use crate::assets::{FontRole, machine_font_key};
use crate::config::MachineFont;
use crate::screens::components::select_music::push_retained_overlay;
use crate::screens::components::shared::loading_bar;
use crate::views::SelectMusicDownloadView;
use deadlib_present::actors::Actor;
use deadlib_present::color;
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use deadsync_input::{InputEvent, VirtualAction};
use std::cell::RefCell;
use std::sync::Arc;

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
const DOWNLOADS_RETRY_HINT: &str = "Press F5 to retry failed downloads.  Press &START; to dismiss.";
const DOWNLOADS_EMPTY_TEXT: &str = "No Downloads to view";
const DOWNLOADS_DIM_ALPHA: f32 = 0.875;

#[derive(Clone, Debug)]
pub struct DownloadsOverlayStateData {
    scroll_index: usize,
    presentation: RefCell<Option<DownloadsPresentation>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DownloadsPresentationKey {
    scroll_index: usize,
    active_color_index: i32,
    machine_font: MachineFont,
    screen_width_bits: u32,
    screen_height_bits: u32,
}

#[derive(Clone, Debug)]
struct DownloadsPresentation {
    key: DownloadsPresentationKey,
    snapshots: Box<[SelectMusicDownloadView]>,
    children: Arc<[Actor]>,
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

const fn downloads_scroll_limit(total: usize) -> usize {
    total.saturating_sub(DOWNLOADS_VIEW_ROWS)
}

#[must_use]
pub const fn show_downloads_overlay() -> DownloadsOverlayState {
    DownloadsOverlayState::Visible(DownloadsOverlayStateData {
        scroll_index: 0,
        presentation: RefCell::new(None),
    })
}

#[inline(always)]
pub fn hide_downloads_overlay(state: &mut DownloadsOverlayState) {
    *state = DownloadsOverlayState::Hidden;
}

pub fn update_downloads_overlay(state: &mut DownloadsOverlayState, total: usize) {
    let DownloadsOverlayState::Visible(overlay) = state else {
        return;
    };
    overlay.scroll_index = overlay.scroll_index.min(downloads_scroll_limit(total));
}

#[inline(always)]
fn downloads_shift(overlay: &mut DownloadsOverlayStateData, delta: isize, total: usize) -> bool {
    let limit = downloads_scroll_limit(total);
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
    total: usize,
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
            if downloads_shift(overlay, -1, total) {
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
            if downloads_shift(overlay, 1, total) {
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
const fn download_size(bytes: u64) -> (&'static str, u64) {
    if bytes >= 1024 * 1024 {
        ("MiB", 1024 * 1024)
    } else if bytes >= 1024 {
        ("KiB", 1024)
    } else {
        ("bytes", 1)
    }
}

pub fn push_downloads_overlay(
    actors: &mut Vec<Actor>,
    state: &DownloadsOverlayState,
    active_color_index: i32,
    snapshots: &[SelectMusicDownloadView],
    machine_font: MachineFont,
) -> bool {
    let DownloadsOverlayState::Visible(overlay) = state else {
        return false;
    };
    let capacity = if snapshots.is_empty() {
        7
    } else {
        6 + snapshots.len().min(DOWNLOADS_VIEW_ROWS) * 4
    };
    let key = DownloadsPresentationKey {
        scroll_index: overlay.scroll_index,
        active_color_index,
        machine_font,
        screen_width_bits: screen_width().to_bits(),
        screen_height_bits: screen_height().to_bits(),
    };
    let cached = overlay
        .presentation
        .borrow()
        .as_ref()
        .filter(|presentation| {
            presentation.key == key && presentation.snapshots.as_ref() == snapshots
        })
        .map(|presentation| Arc::clone(&presentation.children));
    let children = cached.unwrap_or_else(|| {
        let mut children = Vec::with_capacity(capacity);
        push_downloads_overlay_unreserved(
            &mut children,
            overlay,
            active_color_index,
            snapshots,
            machine_font,
        );
        let children = Arc::<[Actor]>::from(children);
        *overlay.presentation.borrow_mut() = Some(DownloadsPresentation {
            key,
            snapshots: snapshots.to_vec().into_boxed_slice(),
            children: Arc::clone(&children),
        });
        children
    });
    push_retained_overlay(actors, children);
    true
}

fn push_downloads_overlay_unreserved(
    actors: &mut Vec<Actor>,
    overlay: &DownloadsOverlayStateData,
    active_color_index: i32,
    snapshots: &[SelectMusicDownloadView],
    machine_font: MachineFont,
) {
    let finished = snapshots
        .iter()
        .filter(|snapshot| snapshot.complete)
        .count();
    let retry_available = snapshots
        .iter()
        .any(|snapshot| snapshot.complete && snapshot.error_message.is_some());
    let total = snapshots.len();
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
        font(machine_font_key(machine_font, FontRole::Header)):
        settext("View Downloads"):
        align(0.5, 0.5):
        xy(center_x, center_y + DOWNLOADS_TITLE_Y):
        zoom(0.54):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(DOWNLOADS_Z + 3)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(if retry_available { DOWNLOADS_RETRY_HINT } else { DOWNLOADS_CLOSE_HINT }):
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
        xy(DOWNLOADS_PANEL_W.mul_add(0.5, center_x) - 18.0, center_y + DOWNLOADS_TITLE_Y):
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
        return;
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
        let row_y = DOWNLOADS_ROW_STEP.mul_add(slot as f32, center_y + DOWNLOADS_LIST_Y);
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
}

/// Stable old/new fixture for a populated download-list actor batch.
#[cfg(any(test, feature = "bench-support"))]
pub struct DownloadsOverlayAppendBenchmark {
    state: DownloadsOverlayState,
    snapshots: Vec<SelectMusicDownloadView>,
}

#[cfg(any(test, feature = "bench-support"))]
impl DownloadsOverlayAppendBenchmark {
    #[must_use]
    pub fn new() -> Self {
        let snapshots = (0..8)
            .map(|index| SelectMusicDownloadView {
                name: format!("Benchmark Pack {index:02}"),
                current_bytes: (index as u64 + 1) * 384 * 1024,
                total_bytes: 4 * 1024 * 1024,
                complete: index < 2,
                error_message: (index == 1).then(|| "network timeout".to_string()),
            })
            .collect();
        Self {
            state: show_downloads_overlay(),
            snapshots,
        }
    }

    #[must_use]
    pub fn actor_count(&self) -> usize {
        let DownloadsOverlayState::Visible(overlay) = &self.state else {
            unreachable!("benchmark overlay is visible");
        };
        let mut actors = Vec::with_capacity(32);
        push_downloads_overlay_unreserved(
            &mut actors,
            overlay,
            2,
            &self.snapshots,
            MachineFont::Mega,
        );
        actors.len()
    }

    #[must_use]
    pub fn legacy_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        let DownloadsOverlayState::Visible(overlay) = &self.state else {
            unreachable!("benchmark overlay is visible");
        };
        push_downloads_overlay_unreserved(out, overlay, 2, &self.snapshots, MachineFont::Mega);
        std::hint::black_box(&*out);
        super::overlay_actor_checksum(out)
    }

    #[must_use]
    pub fn direct_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        push_downloads_overlay(out, &self.state, 2, &self.snapshots, MachineFont::Mega);
        std::hint::black_box(&*out);
        super::overlay_actor_checksum(out)
    }
}

#[cfg(any(test, feature = "bench-support"))]
impl Default for DownloadsOverlayAppendBenchmark {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_uses_prepared_download_count() {
        let mut overlay = DownloadsOverlayStateData {
            scroll_index: 0,
            presentation: RefCell::new(None),
        };

        assert!(downloads_shift(&mut overlay, 1, DOWNLOADS_VIEW_ROWS + 2));
        assert_eq!(overlay.scroll_index, 1);
        assert!(downloads_shift(&mut overlay, 1, DOWNLOADS_VIEW_ROWS + 2));
        assert_eq!(overlay.scroll_index, 2);
        assert!(!downloads_shift(&mut overlay, 1, DOWNLOADS_VIEW_ROWS + 2));
    }

    #[test]
    fn update_clamps_scroll_after_prepared_rows_shrink() {
        let mut state = DownloadsOverlayState::Visible(DownloadsOverlayStateData {
            scroll_index: 3,
            presentation: RefCell::new(None),
        });

        update_downloads_overlay(&mut state, 2);

        let DownloadsOverlayState::Visible(overlay) = state else {
            panic!("downloads overlay should stay visible");
        };
        assert_eq!(overlay.scroll_index, 0);
    }

    #[test]
    fn retained_download_tree_matches_immediate_reuses_and_tracks_progress() {
        let mut fixture = DownloadsOverlayAppendBenchmark::new();
        let mut immediate = Vec::with_capacity(32);
        let _ = fixture.legacy_frame(&mut immediate);

        let mut retained = Vec::with_capacity(1);
        let _ = fixture.direct_frame(&mut retained);
        let [Actor::SharedFrame { children, .. }] = retained.as_slice() else {
            panic!("retained downloads overlay should use one shared frame");
        };
        assert_eq!(
            format!("{immediate:#?}"),
            format!("{:#?}", children.as_ref())
        );
        let first = Arc::clone(children);

        retained.clear();
        let _ = fixture.direct_frame(&mut retained);
        let [Actor::SharedFrame { children, .. }] = retained.as_slice() else {
            panic!("stable downloads overlay should remain shared");
        };
        assert!(Arc::ptr_eq(&first, children));

        fixture.snapshots[2].current_bytes += 1024;
        retained.clear();
        let _ = fixture.direct_frame(&mut retained);
        let [Actor::SharedFrame { children, .. }] = retained.as_slice() else {
            panic!("changed downloads overlay should rebuild a shared frame");
        };
        assert!(!Arc::ptr_eq(&first, children));

        immediate.clear();
        let _ = fixture.legacy_frame(&mut immediate);
        assert_eq!(
            format!("{immediate:#?}"),
            format!("{:#?}", children.as_ref())
        );
    }
}
