use super::*;

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

pub const fn show_downloads_overlay() -> DownloadsOverlayState {
    DownloadsOverlayState::Visible(DownloadsOverlayStateData {
        scroll_index: 0,
        presentation: RefCell::new(None),
    })
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
