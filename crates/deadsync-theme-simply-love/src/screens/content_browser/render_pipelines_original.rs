// Frozen from e001ee23bfe8df93aec868d6294eb6de895883df for paired behavioral and allocation checks.
use super::*;
pub(super) fn push_reload_dialog(
    actors: &mut Vec<Actor>,
    state: &State,
    prompt: &ReloadPrompt,
    w: f32,
    h: f32,
) {
    const CHOICE_W: f32 = 180.0;
    const CHOICE_H: f32 = 30.0;
    let accent = accent(state);
    let cx = w * 0.5;
    let cy = h * 0.5;
    let panel_w = (w * 0.62).min(520.0);
    let panel_h = 150.0;

    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0): zoomto(w, h):
        diffuse(0.0, 0.0, 0.0, 0.62): z(Z_MODAL_SCRIM)
    ));
    push_bordered(
        actors,
        cx,
        cy,
        panel_w,
        panel_h,
        [0.05, 0.05, 0.07, 1.0],
        Z_MODAL_PANEL,
    );
    // The original's words for what changed: it said "New Packs Installed"
    // over a single song, because it only ever knew that something had.
    let packs = state
        .installed_dirs
        .iter()
        .filter(|dir| {
            dir.file_name()
                .and_then(|name| name.to_str())
                .is_none_or(|name| name != deadsync_online::smo_songs::SINGLES_GROUP)
        })
        .count();
    let songs = super::preview::songs_added(state);
    let title = if packs == 0 && songs > 0 {
        "NEW SONGS INSTALLED"
    } else {
        "NEW CONTENT INSTALLED"
    };
    actors.push(act!(text:
        font("wendy"): settext(title.to_owned()):
        align(0.5, 0.5): xy(cx, cy - 50.0): zoom(0.5): horizalign(center):
        diffuse(accent[0], accent[1], accent[2], 1.0): z(Z_MODAL_TEXT)
    ));

    if prompt.reloading {
        let line = match prompt.progress.as_ref() {
            Some((done, total, pack)) => format!("{done} of {total} songs  -  {pack}"),
            None => "Starting...".to_owned(),
        };
        spinner::with_label(
            actors,
            cx - 70.0,
            cy - 10.0,
            spinner::SMALL_PX,
            "Reloading songs",
            Z_MODAL_TEXT,
            accent,
        );
        actors.push(act!(text:
            font("miso"): settext(line):
            align(0.5, 0.5): xy(cx, cy + 22.0): zoom(0.5): horizalign(center):
            maxwidth(panel_w - 40.0):
            diffuse(0.75, 0.75, 0.75, 1.0): z(Z_MODAL_TEXT)
        ));
        return;
    }

    let mut bits: Vec<String> = Vec::with_capacity(2);
    if packs > 0 {
        bits.push(format!(
            "{packs} {}",
            if packs == 1 { "pack" } else { "packs" }
        ));
    }
    if songs > 0 {
        bits.push(format!(
            "{songs} {}",
            if songs == 1 { "song" } else { "songs" }
        ));
    }
    let what = if packs == 0 && songs > 0 {
        "song"
    } else {
        "content"
    };
    let added = if bits.is_empty() {
        "Library changed.".to_owned()
    } else {
        format!("{} added.", bits.join(" and "))
    };
    actors.push(act!(text:
        font("miso"): settext(added):
        align(0.5, 0.5): xy(cx, cy - 26.0): zoom(0.6): horizalign(center):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z_MODAL_TEXT)
    ));
    actors.push(act!(text:
        font("miso"): settext(format!("Reload songs now so the new {what} shows up?")):
        align(0.5, 0.5): xy(cx, cy - 8.0): zoom(0.5): horizalign(center):
        maxwidth(panel_w - 40.0):
        diffuse(0.85, 0.85, 0.85, 1.0): z(Z_MODAL_TEXT)
    ));
    for (index, label) in ["Reload songs", "Not yet"].into_iter().enumerate() {
        let picked = prompt.choice == index;
        let x = cx + (index as f32 - 0.5) * (CHOICE_W + 12.0);
        let plate = if picked {
            [accent[0], accent[1], accent[2], 0.9]
        } else {
            [1.0, 1.0, 1.0, 0.07]
        };
        let ink = if picked {
            [0.08, 0.08, 0.08, 1.0]
        } else {
            [0.75, 0.75, 0.75, 1.0]
        };
        actors.push(act!(quad:
            align(0.5, 0.5): xy(x, cy + 24.0): zoomto(CHOICE_W, CHOICE_H):
            diffuse(plate[0], plate[1], plate[2], plate[3]): z(Z_MODAL_PANEL + 2)
        ));
        actors.push(act!(text:
            font("miso"): settext(label.to_owned()):
            align(0.5, 0.5): xy(x, cy + 24.0): zoom(0.58): horizalign(center):
            diffuse(ink[0], ink[1], ink[2], ink[3]): z(Z_MODAL_TEXT)
        ));
    }
    actors.push(act!(text:
        font("miso"): settext("LEFT/RIGHT choose    START go    BACK cancel".to_owned()):
        align(0.5, 0.5): xy(cx, cy + 56.0): zoom(0.5): horizalign(center):
        diffuse(FOOTER_RGBA[0], FOOTER_RGBA[1], FOOTER_RGBA[2], FOOTER_RGBA[3]):
        z(Z_MODAL_TEXT)
    ));
}
