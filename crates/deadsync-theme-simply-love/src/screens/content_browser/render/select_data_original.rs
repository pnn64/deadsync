// Frozen from 58e888a2404a7e7b68e7028b8da444c2784b1032 for differential tests and paired benchmarks.
// Function bodies are unchanged; visibility is widened only where tests need it.
use super::*;

pub(super) fn meta_line(state: &State, pack: &PackInfo) -> String {
    let mut bits: Vec<String> = Vec::with_capacity(4);
    if pack.song_count > 0 {
        bits.push(format!("{} songs", pack.song_count));
    }
    bits.push(format_bytes(pack.size_bytes));
    if let Some(date) = details_for(state, pack).and_then(|d| d.date_added.as_deref()) {
        bits.push(format!("added {}", format_date(date, true)));
    }
    // Why this row is in a search's answer, last. A result that matched on a
    // charter's name rather than the pack's looks like a wrong answer until
    // the row says so.
    if let Some(why) = state.search_why.get(&pack.id) {
        bits.push(why.clone());
    }
    bits.join("  -  ")
}

pub(super) fn push_type_badge(
    actors: &mut Vec<Actor>,
    state: &State,
    pack: &PackInfo,
    list_w: f32,
    y: f32,
) {
    const KEYBOARD_INK: [f32; 4] = [0.55, 0.72, 1.0, 1.0];
    const DDR_INK: [f32; 4] = [1.0, 0.72, 0.35, 1.0];
    const PAD_INK: [f32; 4] = [0.55, 0.55, 0.55, 1.0];

    let kind = pack
        .pack_type
        .as_deref()
        .map(str::trim)
        .filter(|value| meaningful(value))
        .unwrap_or_default()
        .to_ascii_lowercase();

    let (text, rgba) = if state.query.is_empty() {
        match kind.as_str() {
            "mixed" => ("PAD+KEY", KEYBOARD_INK),
            "ddr" => ("DDR", DDR_INK),
            _ => return,
        }
    } else {
        match kind.as_str() {
            "keyboard" => ("KEY", KEYBOARD_INK),
            "mixed" => ("PAD+KEY", KEYBOARD_INK),
            "ddr" => ("DDR", DDR_INK),
            _ => ("PAD", PAD_INK),
        }
    };

    actors.push(act!(text:
        font("miso"): settext(text.to_owned()):
        align(0.0, 0.5):
        xy(lo::LIST_X + list_w - lo::ROW_TYPE_BADGE_INSET, y + lo::ROW_BADGE_Y):
        zoom(0.5): maxwidth(40.0): horizalign(left):
        diffuse(rgba[0], rgba[1], rgba[2], rgba[3]): z(Z_BADGE)
    ));
}

pub(super) fn push_status_badge(
    actors: &mut Vec<Actor>,
    state: &State,
    pack: &PackInfo,
    right: f32,
    y: f32,
) {
    let accent = accent(state);
    let (text, rgba) = if let Some(install) = install_for(state, pack) {
        match install.phase {
            InstallPhase::Queued => ("queued".to_owned(), META_RGBA),
            InstallPhase::Downloading => {
                let pct = if install.total_bytes > 0 {
                    (install.downloaded_bytes as f64 / install.total_bytes as f64 * 100.0) as u32
                } else {
                    0
                };
                (format!("{pct}%"), accent)
            }
            InstallPhase::Extracting => ("Installing".to_owned(), accent),
            InstallPhase::Installed => ("Installed".to_owned(), INSTALLED_NOW),
            InstallPhase::Error => ("Error".to_owned(), ERROR_RGBA),
        }
    } else if is_installed(state, pack) {
        ("In Library".to_owned(), IN_LIBRARY)
    } else {
        return;
    };

    actors.push(act!(text:
        font("miso"): settext(text):
        align(1.0, 0.5): xy(right - lo::ROW_STATUS_BADGE_INSET, y + lo::ROW_BADGE_Y):
        zoom(0.5): horizalign(right):
        diffuse(rgba[0], rgba[1], rgba[2], rgba[3]): z(Z_BADGE)
    ));
}

pub(super) fn push_row(
    actors: &mut Vec<Actor>,
    state: &State,
    pack: &PackInfo,
    at: &RowPlacement,
    accent: [f32; 4],
) {
    let focus_alpha = if at.selected && at.focused {
        0.16
    } else if at.selected {
        0.09
    } else {
        0.05
    };
    actors.push(act!(quad:
        align(0.0, 0.0): xy(at.x, at.y): zoomto(at.width, lo::ROW_FOCUS_H):
        diffuse(1.0, 1.0, 1.0, focus_alpha): z(Z_ROW_BG)
    ));

    // Art, or the accent spinner standing in for it.
    let art_x = at.x + at.art_cx - at.art_w * 0.5;
    let art_y = at.y + lo::ROW_ART_CY - lo::ROW_ART_H * 0.5;
    push_art_or_status(
        actors,
        state,
        pack.id,
        art_x,
        art_y,
        at.art_w,
        lo::ROW_ART_H,
        Z_ART,
        accent,
        banner_expected(state, pack.id),
    );

    let reserve = if at.badges {
        lo::ROW_TEXT_RESERVE
    } else {
        at.text_x + 12.0
    };
    let text_w = (at.width - reserve).max(40.0);
    actors.push(act!(text:
        font("miso"): settext(pack.name.clone()):
        align(0.0, 0.5): xy(at.x + at.text_x, at.y + lo::ROW_NAME_Y):
        zoom(lo::ROW_NAME_ZOOM): maxwidth(text_w): horizalign(left):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z_TEXT)
    ));

    let meta = meta_line(state, pack);
    if !meta.is_empty() {
        actors.push(act!(text:
            font("miso"): settext(meta):
            align(0.0, 0.5): xy(at.x + at.text_x, at.y + lo::ROW_META_Y):
            zoom(lo::ROW_META_ZOOM): maxwidth(text_w): horizalign(left):
            diffuse(META_RGBA[0], META_RGBA[1], META_RGBA[2], META_RGBA[3]): z(Z_TEXT)
        ));
    }

    if at.badges {
        push_type_badge(actors, state, pack, at.width, at.y);
    }
    push_status_badge(actors, state, pack, at.x + at.width, at.y);
}
