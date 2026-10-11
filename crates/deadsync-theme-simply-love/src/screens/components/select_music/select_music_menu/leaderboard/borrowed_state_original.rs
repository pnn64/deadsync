use super::*;

pub(super) fn apply_leaderboard_side_snapshot(
    side: &mut LeaderboardSideState,
    snapshot: score_data::CachedPlayerLeaderboardData,
) {
    let current_pane = side.panes.get(side.pane_index).map(|pane| {
        (
            pane.name.clone(),
            pane.is_ex,
            pane.is_hard_ex(),
            pane.disabled,
            pane.personalized,
        )
    });

    if snapshot.loading {
        side.loading = true;
        side.error_text = None;
        side.show_icons = false;
        return;
    }

    side.loading = false;
    if let Some(error) = snapshot.error {
        side.error_text = Some(gs_error_text(&error));
        if side.panes.is_empty()
            && let Some(machine) = side.machine_pane.clone()
        {
            side.panes.push(machine);
        }
        side.pane_index = side.pane_index.min(side.panes.len().saturating_sub(1));
        side.show_icons = false;
        return;
    }

    let mut panes = snapshot.data.map_or_else(Vec::new, |data| {
        data.panes
            .iter()
            .filter(|pane| should_show_overlay_pane(pane))
            .cloned()
            .collect()
    });
    if let Some(machine) = side.machine_pane.clone() {
        panes.push(machine);
    }
    if panes.is_empty()
        && let Some(machine) = side.machine_pane.clone()
    {
        panes.push(machine);
    }

    side.error_text = None;
    if let Some((name, is_ex, is_hard_ex, disabled, personalized)) = current_pane {
        side.pane_index = panes
            .iter()
            .position(|pane| {
                pane.name == name
                    && pane.is_ex == is_ex
                    && pane.is_hard_ex() == is_hard_ex
                    && pane.disabled == disabled
                    && pane.personalized == personalized
            })
            .unwrap_or_else(|| side.pane_index.min(panes.len().saturating_sub(1)));
    } else {
        side.pane_index = 0;
    }
    side.show_icons = panes.len() > 1;
    side.panes = panes;
}

pub(super) fn apply_leaderboard_side_view(
    side: &mut LeaderboardSideState,
    view: SelectMusicLeaderboardSideView,
) {
    if side.chart_hash != view.chart_hash {
        return;
    }

    let machine = gs_machine_pane(view.machine_entries);
    side.machine_pane = Some(machine.clone());
    let Some(snapshot) = view.leaderboards else {
        side.loading = false;
        side.error_text = None;
        side.panes.clear();
        side.panes.push(machine);
        side.pane_index = 0;
        side.show_icons = false;
        return;
    };
    apply_leaderboard_side_snapshot(side, snapshot);
}

pub(super) fn overlay_display_entries(
    runtime: &ScoreboxSideView,
    pane: &score_data::LeaderboardPane,
) -> Vec<score_data::LeaderboardEntry> {
    let entries = entries_with_local_self_state(runtime, pane);
    score_data::prioritized_leaderboard_entries(entries.as_ref(), GS_LEADERBOARD_NUM_ENTRIES)
}

pub(super) fn push_leaderboard_overlay_unreserved<const INCLUDE_ICONS: bool>(
    actors: &mut Vec<Actor>,
    overlay: &LeaderboardOverlayStateData,
    machine_font: MachineFont,
) {
    let overlay_elapsed = overlay.elapsed;
    let joined_count = usize::from(overlay.p1.joined) + usize::from(overlay.p2.joined);
    let pane_width = if joined_count <= 1 {
        GS_LEADERBOARD_PANE_WIDTH_SINGLE
    } else {
        GS_LEADERBOARD_PANE_WIDTH_MULTI
    };
    let show_date = joined_count <= 1;
    let pane_cy = screen_center_y() + GS_LEADERBOARD_PANE_CENTER_Y;
    let row_center = f32::midpoint(GS_LEADERBOARD_NUM_ENTRIES as f32, 1.0);

    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, GS_LEADERBOARD_DIM_ALPHA):
        z(GS_LEADERBOARD_Z)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(GS_LEADERBOARD_CLOSE_HINT):
        align(0.5, 0.5):
        xy(screen_center_x(), screen_height() - 50.0):
        zoom(1.1):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(GS_LEADERBOARD_Z + 1):
        horizalign(center)
    ));

    let mut draw_panel = |side: &LeaderboardSideState, center_x: f32| {
        let pane = side
            .panes
            .get(side.pane_index.min(side.panes.len().saturating_sub(1)));
        let display_entries = pane.map(|pane| overlay_display_entries(&side.scorebox, pane));
        let header_text = if side.loading {
            "GrooveStats".to_string()
        } else if let Some(p) = pane {
            p.name.replace("ITL Online", "ITL")
        } else {
            "GrooveStats".to_string()
        };
        let show_ex = !side.loading
            && side.error_text.is_none()
            && pane.is_some_and(|p| p.is_ex && !p.disabled);
        let show_itg_arrowcloud = !side.loading
            && side.error_text.is_none()
            && pane
                .is_some_and(|p| p.is_arrowcloud() && !p.is_ex && !p.is_hard_ex() && !p.disabled);
        let show_hard_ex = !side.loading
            && side.error_text.is_none()
            && pane.is_some_and(|p| p.is_hard_ex() && !p.disabled);
        let is_disabled = !side.loading && pane.is_some_and(|p| p.disabled);

        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(center_x, pane_cy):
            zoomto(pane_width + 2.0, GS_LEADERBOARD_PANE_HEIGHT + 2.0):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(GS_LEADERBOARD_Z + 2)
        ));
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(center_x, pane_cy):
            zoomto(pane_width, GS_LEADERBOARD_PANE_HEIGHT):
            diffuse(0.0, 0.0, 0.0, 1.0):
            z(GS_LEADERBOARD_Z + 3)
        ));

        let header_y = GS_LEADERBOARD_ROW_HEIGHT
            .mul_add(0.5, GS_LEADERBOARD_PANE_HEIGHT.mul_add(-0.5, pane_cy));
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(center_x, header_y):
            zoomto(pane_width + 2.0, GS_LEADERBOARD_ROW_HEIGHT + 2.0):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(GS_LEADERBOARD_Z + 4)
        ));
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(center_x, header_y):
            zoomto(pane_width, GS_LEADERBOARD_ROW_HEIGHT):
            diffuse(
                GS_LEADERBOARD_HEADER_BG[0],
                GS_LEADERBOARD_HEADER_BG[1],
                GS_LEADERBOARD_HEADER_BG[2],
                GS_LEADERBOARD_HEADER_BG[3]
            ):
            z(GS_LEADERBOARD_Z + 5)
        ));
        actors.push(act!(text:
            font(machine_font_key(machine_font, FontRole::Header)):
            settext(header_text):
            align(0.5, 0.5):
            xy(center_x, header_y):
            zoom(0.5):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(GS_LEADERBOARD_Z + 6):
            horizalign(center)
        ));
        if show_ex {
            actors.push(act!(text:
                font(machine_font_key(machine_font, FontRole::Header)):
                settext("EX"):
                align(1.0, 0.5):
                xy(center_x + pane_width * 0.5 - 16.0, header_y):
                zoom(0.5):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(GS_LEADERBOARD_Z + 6):
                horizalign(right)
            ));
        } else if show_itg_arrowcloud {
            actors.push(act!(text:
                font(machine_font_key(machine_font, FontRole::Header)):
                settext("ITG"):
                align(1.0, 0.5):
                xy(center_x + pane_width * 0.5 - 16.0, header_y):
                zoom(0.5):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(GS_LEADERBOARD_Z + 6):
                horizalign(right)
            ));
        } else if show_hard_ex {
            actors.push(act!(text:
                font(machine_font_key(machine_font, FontRole::Header)):
                settext("H.EX"):
                align(1.0, 0.5):
                xy(center_x + pane_width * 0.5 - 16.0, header_y):
                zoom(0.5):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(GS_LEADERBOARD_Z + 6):
                horizalign(right)
            ));
        }

        let rank_x = center_x - pane_width * 0.5 + 32.0;
        let name_x = center_x - pane_width * 0.5 + 100.0;
        let score_x = if show_date {
            center_x + 63.0
        } else {
            center_x + pane_width * 0.5 - 2.0
        };
        let date_x = center_x + pane_width * 0.5 - 2.0;

        for i in 0..GS_LEADERBOARD_NUM_ENTRIES {
            let y = GS_LEADERBOARD_ROW_HEIGHT.mul_add((i + 1) as f32 - row_center, pane_cy);
            let mut rank = String::new();
            let mut name = String::new();
            let mut score = String::new();
            let mut date = String::new();
            let mut has_highlight = false;
            let mut highlight_rgb = [0.0, 0.0, 0.0];
            let mut rank_col = [1.0, 1.0, 1.0, 1.0];
            let mut name_col = [1.0, 1.0, 1.0, 1.0];
            let mut score_col = if show_ex {
                color::JUDGMENT_RGBA[0]
            } else if show_hard_ex {
                color::HARD_EX_SCORE_RGBA
            } else {
                [1.0, 1.0, 1.0, 1.0]
            };
            let mut date_col = [1.0, 1.0, 1.0, 1.0];

            if side.loading {
                if i == 0 {
                    name = GS_LEADERBOARD_LOADING_TEXT.to_string();
                }
            } else if let Some(err) = &side.error_text {
                if i == 0 {
                    name.clone_from(err);
                }
            } else if is_disabled {
                if i == 0 {
                    name = GS_LEADERBOARD_DISABLED_TEXT.to_string();
                }
            } else if pane.is_some() {
                if let Some(entry) = display_entries.as_ref().and_then(|entries| entries.get(i)) {
                    rank = format!("{}.", entry.rank);
                    name.clone_from(&entry.name);
                    score = format!("{:.2}%", entry.score / 100.0);
                    date = score_data::format_leaderboard_date(&entry.date);

                    if entry.is_rival || entry.is_self {
                        has_highlight = true;
                        if entry.is_rival {
                            highlight_rgb = [
                                GS_LEADERBOARD_RIVAL_COLOR[0],
                                GS_LEADERBOARD_RIVAL_COLOR[1],
                                GS_LEADERBOARD_RIVAL_COLOR[2],
                            ];
                        } else {
                            highlight_rgb = [
                                GS_LEADERBOARD_SELF_COLOR[0],
                                GS_LEADERBOARD_SELF_COLOR[1],
                                GS_LEADERBOARD_SELF_COLOR[2],
                            ];
                        }
                        rank_col = [0.0, 0.0, 0.0, 1.0];
                        name_col = [0.0, 0.0, 0.0, 1.0];
                        score_col = [0.0, 0.0, 0.0, 1.0];
                        date_col = [0.0, 0.0, 0.0, 1.0];
                    }
                    if entry.is_fail {
                        score_col = [1.0, 0.0, 0.0, 1.0];
                    }
                } else if i == 0 && display_entries.as_ref().is_none_or(std::vec::Vec::is_empty) {
                    name = GS_LEADERBOARD_NO_SCORES_TEXT.to_string();
                }
            }

            if has_highlight {
                actors.push(act!(quad:
                    align(0.5, 0.5):
                    xy(center_x, y):
                    zoomto(pane_width, GS_LEADERBOARD_ROW_HEIGHT):
                    diffuse(highlight_rgb[0], highlight_rgb[1], highlight_rgb[2], 1.0):
                    z(GS_LEADERBOARD_Z + 5)
                ));
            }

            actors.push(act!(text:
                font("miso"):
                settext(rank):
                align(1.0, 0.5):
                xy(rank_x, y):
                zoom(GS_LEADERBOARD_TEXT_ZOOM):
                maxwidth(30.0):
                diffuse(rank_col[0], rank_col[1], rank_col[2], rank_col[3]):
                z(GS_LEADERBOARD_Z + 7):
                horizalign(right)
            ));
            actors.push(act!(text:
                font("miso"):
                settext(name):
                align(0.5, 0.5):
                xy(name_x, y):
                zoom(GS_LEADERBOARD_TEXT_ZOOM):
                maxwidth(130.0):
                diffuse(name_col[0], name_col[1], name_col[2], name_col[3]):
                z(GS_LEADERBOARD_Z + 7):
                horizalign(center)
            ));
            actors.push(act!(text:
                font("miso"):
                settext(score):
                align(1.0, 0.5):
                xy(score_x, y):
                zoom(GS_LEADERBOARD_TEXT_ZOOM):
                diffuse(score_col[0], score_col[1], score_col[2], score_col[3]):
                z(GS_LEADERBOARD_Z + 7):
                horizalign(right)
            ));
            if show_date {
                actors.push(act!(text:
                    font("miso"):
                    settext(date):
                    align(1.0, 0.5):
                    xy(date_x, y):
                    zoom(GS_LEADERBOARD_TEXT_ZOOM):
                    diffuse(date_col[0], date_col[1], date_col[2], date_col[3]):
                    z(GS_LEADERBOARD_Z + 7):
                    horizalign(right)
                ));
            }
        }

        if INCLUDE_ICONS {
            push_leaderboard_panel_icons::<true>(
                actors,
                side,
                center_x,
                pane_width,
                pane_cy,
                overlay_elapsed,
            );
        }
    };

    if joined_count <= 1 {
        if overlay.p1.joined {
            draw_panel(&overlay.p1, screen_center_x());
        } else if overlay.p2.joined {
            draw_panel(&overlay.p2, screen_center_x());
        }
    } else {
        draw_panel(
            &overlay.p1,
            screen_center_x() - GS_LEADERBOARD_PANE_SIDE_OFFSET,
        );
        draw_panel(
            &overlay.p2,
            screen_center_x() + GS_LEADERBOARD_PANE_SIDE_OFFSET,
        );
    }
}
