use super::*;

pub(super) fn push_catalog(
    out: &mut Vec<Actor>,
    data: &DownloadPacksOverlayData,
    snapshot: &Snapshot,
    accent: [f32; 4],
    cx: f32,
    cy: f32,
    bold_font: &'static str,
) {
    let selected = data.selected.min(data.results.len().saturating_sub(1));
    let start = selected
        .saturating_sub(VIEW_ROWS / 2)
        .min(data.results.len().saturating_sub(VIEW_ROWS));
    let list_x = cx + 157.0;
    let list_top = cy - 74.0;
    out.push(act!(quad:
        align(0.5, 0.5): xy(list_x, cy + 47.0): zoomto(PANE_W, 274.0):
        diffuse(0.0, 0.0, 0.0, 0.76): z(Z + 4)
    ));
    for (slot, &catalog_index) in data.results.iter().skip(start).take(VIEW_ROWS).enumerate() {
        let Some(pack) = snapshot.catalog.get(catalog_index) else {
            continue;
        };
        let row_index = start + slot;
        let active = row_index == selected;
        let y = (slot as f32).mul_add(ROW_H, list_top);
        out.push(act!(quad:
            align(0.5, 0.5): xy(list_x, y): zoomto(PANE_W - 8.0, ROW_H - 3.0):
            diffuse(accent[0], accent[1], accent[2], if active { 0.82 } else { 0.12 }):
            z(Z + 5)
        ));
        out.push(act!(text:
            font(bold_font): settext(pack.name.clone()):
            align(0.0, 0.5): xy(PANE_W.mul_add(-0.5, list_x) + 9.0, y - 5.0):
            zoom(0.25): maxwidth(258.0):
            diffuse(1.0, 1.0, 1.0, if active { 1.0 } else { 0.75 }):
            z(Z + 6): horizalign(left)
        ));
        out.push(act!(text:
            font("miso"): settext(pack_row_detail(pack, snapshot, &data.installed_names)):
            align(0.0, 0.5): xy(PANE_W.mul_add(-0.5, list_x) + 9.0, y + 8.0):
            zoom(0.67): maxwidth(258.0):
            diffuse(0.88, 0.88, 0.90, if active { 1.0 } else { 0.60 }):
            z(Z + 6): horizalign(left)
        ));
    }
    if let Some(pack) = selected_pack(data, snapshot) {
        push_pack_detail(out, data, pack, snapshot, accent, cx, cy, bold_font);
    }
}

pub(super) fn push_pack_detail(
    out: &mut Vec<Actor>,
    data: &DownloadPacksOverlayData,
    pack: &PackInfo,
    snapshot: &Snapshot,
    accent: [f32; 4],
    cx: f32,
    cy: f32,
    bold_font: &'static str,
) {
    let x = PANEL_W.mul_add(-0.5, cx) + 15.0;
    let install = install_for_pack(snapshot, pack.id);
    let installed = pack_is_installed(pack, snapshot, &data.installed_names);
    out.push(act!(quad:
        align(0.0, 0.0): xy(x, cy - 88.0): zoomto(PANE_W, 274.0):
        diffuse(0.0, 0.0, 0.0, 0.76): z(Z + 4)
    ));
    out.push(act!(text:
        font(bold_font): settext(pack.name.clone()):
        align(0.0, 0.5): xy(x + 9.0, cy - 69.0): zoom(0.34): maxwidth(262.0):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z + 6): horizalign(left)
    ));
    out.push(act!(text:
        font("miso"): settext(format!("SMO #{}", pack.id)):
        align(1.0, 0.5): xy(x + PANE_W - 9.0, cy - 50.0): zoom(0.64):
        diffuse(0.62, 0.62, 0.66, 1.0): z(Z + 6): horizalign(right)
    ));
    let (status, status_color) = pack_status(install, installed);
    out.push(act!(text:
        font(bold_font): settext(status):
        align(0.0, 0.5): xy(x + 9.0, cy - 49.0): zoom(0.25): maxwidth(205.0):
        diffuse(status_color[0], status_color[1], status_color[2], status_color[3]):
        z(Z + 6): horizalign(left)
    ));

    let songs_label = tr("OptionsDownloadPacks", "Songs");
    let size_label = tr("OptionsDownloadPacks", "Size");
    let pack_type_label = tr("OptionsDownloadPacks", "PackType");
    let substyle_label = tr("OptionsDownloadPacks", "Substyle");
    let sync_label = tr("OptionsDownloadPacks", "Sync");
    let min_version_label = tr("OptionsDownloadPacks", "MinVersion");
    let pack_type = optional_meta(pack.pack_type.as_deref());
    let substyle = optional_meta(pack.substyle.as_deref());
    let sync = optional_meta(pack.sync.as_deref());
    let min_version = optional_meta(pack.min_version.as_deref());
    push_meta_pair(
        out,
        x + 9.0,
        cy - 22.0,
        &songs_label,
        &pack.song_count.to_string(),
        &size_label,
        &format_bytes(pack.size_bytes),
        accent,
        bold_font,
    );
    push_meta_pair(
        out,
        x + 9.0,
        cy + 27.0,
        &pack_type_label,
        pack_type.as_ref(),
        &substyle_label,
        substyle.as_ref(),
        accent,
        bold_font,
    );
    push_meta_pair(
        out,
        x + 9.0,
        cy + 76.0,
        &sync_label,
        sync.as_ref(),
        &min_version_label,
        min_version.as_ref(),
        accent,
        bold_font,
    );

    if let Some(install) = install
        && matches!(install.phase, InstallPhase::Downloading)
    {
        push_progress(out, install, accent, x + 9.0, cy + 132.0);
    }
    let message = data.local_message.clone().unwrap_or_else(|| {
        if installed {
            tr("OptionsDownloadPacks", "AlreadyInstalled").to_string()
        } else if let Some(install) = install {
            install_status_text(install)
        } else {
            tr("OptionsDownloadPacks", "Ready").to_string()
        }
    });
    out.push(act!(text:
        font(bold_font): settext(message):
        align(0.0, 1.0): xy(x + 9.0, cy + 177.0): zoom(0.24): maxwidth(264.0):
        diffuse(accent[0], accent[1], accent[2], 1.0): z(Z + 6): horizalign(left)
    ));
}

pub(super) fn push_meta_pair(
    out: &mut Vec<Actor>,
    x: f32,
    y: f32,
    left_label: &str,
    left_value: &str,
    right_label: &str,
    right_value: &str,
    accent: [f32; 4],
    bold_font: &'static str,
) {
    out.push(act!(text:
        font(bold_font): settext(left_label.to_owned()):
        align(0.0, 0.5): xy(x, y): zoom(0.21): maxwidth(124.0):
        diffuse(accent[0], accent[1], accent[2], 0.92): z(Z + 6): horizalign(left)
    ));
    out.push(act!(text:
        font("miso"): settext(left_value.to_owned()): align(0.0, 0.5):
        xy(x, y + 15.0): zoom(0.72): maxwidth(124.0):
        diffuse(0.94, 0.94, 0.96, 1.0): z(Z + 6): horizalign(left)
    ));
    out.push(act!(text:
        font(bold_font): settext(right_label.to_owned()):
        align(0.0, 0.5): xy(x + 137.0, y): zoom(0.21): maxwidth(124.0):
        diffuse(accent[0], accent[1], accent[2], 0.92): z(Z + 6): horizalign(left)
    ));
    out.push(act!(text:
        font("miso"): settext(right_value.to_owned()): align(0.0, 0.5):
        xy(x + 137.0, y + 15.0): zoom(0.72): maxwidth(124.0):
        diffuse(0.94, 0.94, 0.96, 1.0): z(Z + 6): horizalign(left)
    ));
}

pub(super) fn push_status(
    out: &mut Vec<Actor>,
    message: &str,
    rgba: [f32; 4],
    cx: f32,
    cy: f32,
    bold_font: &'static str,
) {
    out.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy + 22.0): zoomto(540.0, 128.0):
        diffuse(0.0, 0.0, 0.0, 0.82): z(Z + 5)
    ));
    out.push(act!(text:
        font(bold_font): settext(message.to_owned()):
        align(0.5, 0.5): xy(cx, cy + 10.0): zoom(0.34):
        wrapwidthpixels(1050.0): maxwidth(510.0):
        diffuse(rgba[0], rgba[1], rgba[2], rgba[3]): z(Z + 6): horizalign(center)
    ));
}
