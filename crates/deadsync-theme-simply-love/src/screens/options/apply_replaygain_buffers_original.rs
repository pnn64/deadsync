use super::*;

pub(super) fn push_apply_replaygain_overlay_actors_unreserved(
    out: &mut Vec<Actor>,
    ui: &ApplyReplayGainUiState,
    active_color_index: i32,
) {
    let (done, total, progress) = apply_replaygain_progress(ui);
    let elapsed = ui.started_at.elapsed().as_secs_f32().max(0.0);
    let count_text = if total == 0 {
        String::new()
    } else {
        crate::screens::progress_count_text(done, total)
    };
    let show_speed_row = total > 0 || done > 0;
    let speed_text = if show_speed_row {
        let rate = if ui.finished {
            0.0
        } else if elapsed > 0.0 {
            ui.displayed_done.max(0.0) / elapsed
        } else {
            0.0
        };
        let mut text = tr_fmt(
            "SelectMusic",
            "LoadingSpeed",
            &[("speed", &format!("{rate:.0}"))],
        )
        .to_string();
        if !ui.finished && total > 0 && rate > 0.0 {
            let remaining = total.saturating_sub(done) as f32;
            if remaining > 0.0 {
                let eta_secs = (remaining / rate).round() as u64;
                text = format!(
                    "{text}  \u{2022}  {}",
                    tr_fmt(
                        "OptionsScoreImport",
                        "ImportEta",
                        &[("eta", &format_eta(eta_secs))],
                    ),
                );
            }
        }
        text
    } else {
        String::new()
    };

    let header = if ui.finished {
        if ui.cancelled {
            tr("OptionsSound", "ApplyReplayGainCancelled")
        } else {
            tr("OptionsSound", "ApplyReplayGainComplete")
        }
    } else if ui.cancel_requested {
        tr("OptionsSound", "ApplyReplayGainCancelling")
    } else {
        tr("OptionsSound", "ApplyReplayGainRunning")
    };

    let fill = color::decorative_rgba(active_color_index);
    let bar_w = widescale(360.0, 520.0);
    let bar_h = RELOAD_BAR_H;
    let bar_cx = screen_width() * 0.5;
    let bar_cy = screen_height().mul_add(0.5, 34.0);
    let fill_w = (bar_w - 4.0) * progress.clamp(0.0, 1.0);

    out.reserve(8);
    out.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.65):
        z(300)
    ));
    out.push(act!(text:
        font("miso"):
        settext(header):
        align(0.5, 0.5):
        xy(screen_width() * 0.5, bar_cy - 98.0):
        zoom(1.05):
        horizalign(center):
        z(301)
    ));
    if !ui.line2.is_empty() {
        out.push(act!(text:
            font("miso"):
            settext(ui.line2.clone()):
            align(0.5, 0.5):
            xy(screen_width() * 0.5, bar_cy - 74.0):
            zoom(0.95):
            maxwidth(screen_width() * 0.9):
            horizalign(center):
            z(301)
        ));
    }
    if !ui.line3.is_empty() {
        out.push(act!(text:
            font("miso"):
            settext(ui.line3.clone()):
            align(0.5, 0.5):
            xy(screen_width() * 0.5, bar_cy - 50.0):
            zoom(0.95):
            maxwidth(screen_width() * 0.9):
            horizalign(center):
            z(301)
        ));
    }

    let mut bar_children = Vec::with_capacity(4);
    bar_children.push(act!(quad:
        align(0.5, 0.5):
        xy(bar_w / 2.0, bar_h / 2.0):
        zoomto(bar_w, bar_h):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(0)
    ));
    bar_children.push(act!(quad:
        align(0.5, 0.5):
        xy(bar_w / 2.0, bar_h / 2.0):
        zoomto(bar_w - 4.0, bar_h - 4.0):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1)
    ));
    if fill_w > 0.0 {
        bar_children.push(act!(quad:
            align(0.0, 0.5):
            xy(2.0, bar_h / 2.0):
            zoomto(fill_w, bar_h - 4.0):
            diffuse(fill[0], fill[1], fill[2], 1.0):
            z(2)
        ));
    }
    bar_children.push(act!(text:
        font("miso"):
        settext(count_text):
        align(0.5, 0.5):
        xy(bar_w / 2.0, bar_h / 2.0):
        zoom(0.9):
        horizalign(center):
        z(3)
    ));
    out.push(Actor::Frame {
        align: [0.5, 0.5],
        offset: [bar_cx, bar_cy],
        size: [actors::SizeSpec::Px(bar_w), actors::SizeSpec::Px(bar_h)],
        background: None,
        z: 301,
        children: bar_children,
    });

    if show_speed_row && !speed_text.is_empty() {
        out.push(act!(text:
            font("miso"):
            settext(speed_text):
            align(0.5, 0.5):
            xy(screen_width() * 0.5, bar_cy + 36.0):
            zoom(0.9):
            horizalign(center):
            z(301)
        ));
    }

    let footer = if ui.finished {
        tr("OptionsScoreImport", "PressStartToDismiss")
    } else {
        tr("OptionsSound", "ApplyReplayGainCancelHint")
    };
    out.push(act!(text:
        font("miso"):
        settext(footer):
        align(0.5, 0.5):
        xy(screen_width() * 0.5, bar_cy + 66.0):
        zoom(0.9):
        horizalign(center):
        z(301)
    ));
}
