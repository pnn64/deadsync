use crate::act;
use crate::assets::AssetManager;
use crate::core::space::*;
use crate::game::gameplay::State;
use crate::game::judgment::JudgeGrade;
use crate::game::{profile, timing as timing_stats};
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;
use crate::ui::font;
use std::collections::HashMap;
use std::sync::LazyLock;

pub fn build(
    state: &State,
    asset_manager: &AssetManager,
    playfield_center_x: f32,
    player_side: profile::PlayerSide,
) -> Vec<Actor> {
    let mut actors = Vec::new();
    actors.extend(build_banner(state, playfield_center_x, player_side));
    actors.extend(build_side_pane(
        state,
        asset_manager,
        playfield_center_x,
        player_side,
    ));
    actors.extend(build_holds_mines_rolls_pane(
        state,
        asset_manager,
        playfield_center_x,
        player_side,
    ));
    actors
}

pub fn build_versus_step_stats(state: &State, asset_manager: &AssetManager) -> Vec<Actor> {
    if !is_wide() {
        return vec![];
    }
    // Simply Love shows centered step stats in 2P versus on widescreen, but not on ultrawide
    // (ultrawide already has native per-player side panes).
    let is_ultrawide = screen_width() / screen_height().max(1.0) > (21.0 / 9.0);
    if is_ultrawide {
        return vec![];
    }
    if state.num_players < 2 || state.players.len() < 2 {
        return vec![];
    }

    let profile = profile::get();
    let show_fa_plus_window = profile.show_fa_plus_window;
    let center_x = screen_center_x();

    let total_tapnotes = state.chart.stats.total_steps as f32;
    let digits = if total_tapnotes > 0.0 {
        (total_tapnotes.log10().floor() as usize + 1).max(4)
    } else {
        4
    };

    let group_zoom_y = 0.8_f32;
    let group_zoom_x = if digits > 4 {
        (group_zoom_y - 0.12 * (digits.saturating_sub(4) as f32)).max(0.1)
    } else {
        group_zoom_y
    };
    let numbers_zoom_y = group_zoom_y * 0.5;
    let numbers_zoom_x = group_zoom_x * 0.5;
    let row_height = if show_fa_plus_window { 29.0 } else { 35.0 };
    let y_base = -280.0;

    // Keep the background bar below the top HUD (song title/BPM), but let the
    // digits sit above playfield elements if needed.
    let z_bg = 80i16;
    let z_fg = 110i16;

    let mut actors = Vec::with_capacity(128);
    // Center black column behind the counters (SL: VersusStepStatistics.lua).
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(screen_center_x(), screen_center_y()):
        zoomto(150.0, screen_height()):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(z_bg)
    ));

    let notes_per_player = if state.num_players > 0 {
        state.notes.len() / state.num_players
    } else {
        0
    };

    let fantastic_color = JUDGMENT_INFO
        .get(&JudgeGrade::Fantastic)
        .map(|info| info.color)
        .unwrap_or_else(|| color::JUDGMENT_RGBA[0]);
    let excellent_color = JUDGMENT_INFO
        .get(&JudgeGrade::Excellent)
        .map(|info| info.color)
        .unwrap_or_else(|| color::JUDGMENT_RGBA[1]);
    let great_color = JUDGMENT_INFO
        .get(&JudgeGrade::Great)
        .map(|info| info.color)
        .unwrap_or_else(|| color::JUDGMENT_RGBA[2]);
    let decent_color = JUDGMENT_INFO
        .get(&JudgeGrade::Decent)
        .map(|info| info.color)
        .unwrap_or_else(|| color::JUDGMENT_RGBA[3]);
    let wayoff_color = JUDGMENT_INFO
        .get(&JudgeGrade::WayOff)
        .map(|info| info.color)
        .unwrap_or_else(|| color::JUDGMENT_RGBA[4]);
    let miss_color = JUDGMENT_INFO
        .get(&JudgeGrade::Miss)
        .map(|info| info.color)
        .unwrap_or_else(|| color::JUDGMENT_RGBA[5]);

    let dim_fantastic = color::JUDGMENT_DIM_RGBA[0];
    let dim_excellent = color::JUDGMENT_DIM_RGBA[1];
    let dim_great = color::JUDGMENT_DIM_RGBA[2];
    let dim_decent = color::JUDGMENT_DIM_RGBA[3];
    let dim_wayoff = color::JUDGMENT_DIM_RGBA[4];
    let dim_miss = color::JUDGMENT_DIM_RGBA[5];
    let white_fa_color = color::JUDGMENT_FA_PLUS_WHITE_RGBA;
    let dim_white_fa = color::JUDGMENT_FA_PLUS_WHITE_GAMEPLAY_DIM_RGBA;

    asset_manager.with_fonts(|all_fonts| {
        asset_manager.with_font("wendy_screenevaluation", |f| {
            let digit_w = (font::measure_line_width_logical(f, "0", all_fonts) as f32) * numbers_zoom_x;
            if digit_w <= 0.0 {
                return;
            }

            // Simply Love (VersusStepStatistics.lua) positions the two TapNoteJudgments actorframes at:
            // P1: x=-64, P2: x=+66 (relative to center). TapNoteJudgments internally uses
            // `PlayerNumber:Reverse()[player]` for halign, which is P1=0 (left), P2=1 (right),
            // so both number blocks extend inward and sit inside the 150px black column.
            let base_anchor_p1 = center_x - 64.0; // left edge for P1 block
            let base_anchor_p2 = center_x + 66.0; // right edge for P2 block
            let block_w = (digits as f32) * digit_w;
            let bar_left = center_x - 75.0;
            let bar_right = center_x + 75.0;
            let margin = 4.0;
            let anchor_p1 = base_anchor_p1.clamp(bar_left + margin, bar_right - margin - block_w);
            let anchor_p2 = base_anchor_p2.clamp(bar_left + margin + block_w, bar_right - margin);

            for player_idx in 0..2usize {
                let is_p1 = player_idx == 0;
                let group_y = 100.0;
                let anchor_x = if is_p1 { anchor_p1 } else { anchor_p2 };
                let group_origin_y = screen_center_y() + group_y;

                if show_fa_plus_window && notes_per_player > 0 {
                    let start = player_idx * notes_per_player;
                    let end = (start + notes_per_player).min(state.notes.len());
                    let wc = timing_stats::compute_window_counts(&state.notes[start..end]);
                    let rows: [([f32; 4], [f32; 4], u32); 7] = [
                        (fantastic_color, dim_fantastic, wc.w0),
                        (white_fa_color, dim_white_fa, wc.w1),
                        (excellent_color, dim_excellent, wc.w2),
                        (great_color, dim_great, wc.w3),
                        (decent_color, dim_decent, wc.w4),
                        (wayoff_color, dim_wayoff, wc.w5),
                        (miss_color, dim_miss, wc.miss),
                    ];
                    for (row_i, (bright, dim, count)) in rows.iter().enumerate() {
                        let y = group_origin_y + (y_base + row_i as f32 * row_height) * group_zoom_y;
                        let s = format!("{:0width$}", count, width = digits);
                        let first_nonzero = s.find(|c: char| c != '0').unwrap_or(s.len());

                        for (i, ch) in s.chars().enumerate() {
                            let is_dim = if *count == 0 {
                                i < digits.saturating_sub(1)
                            } else {
                                i < first_nonzero
                            };
                            let c = if is_dim { *dim } else { *bright };
                            if is_p1 {
                                let x = anchor_x + (i as f32) * digit_w;
                                let mut a = act!(text:
                                    font("wendy_screenevaluation"): settext(ch.to_string()):
                                    align(0.0, 0.5): xy(x, y):
                                    zoom(numbers_zoom_y):
                                    diffuse(c[0], c[1], c[2], c[3]):
                                    z(z_fg):
                                    horizalign(left)
                                );
                                if let Actor::Text { scale, .. } = &mut a {
                                    scale[0] = numbers_zoom_x;
                                    scale[1] = numbers_zoom_y;
                                }
                                actors.push(a);
                            } else {
                                let idx_from_right = digits.saturating_sub(1).saturating_sub(i);
                                let x = anchor_x - (idx_from_right as f32) * digit_w;
                                let mut a = act!(text:
                                    font("wendy_screenevaluation"): settext(ch.to_string()):
                                    align(1.0, 0.5): xy(x, y):
                                    zoom(numbers_zoom_y):
                                    diffuse(c[0], c[1], c[2], c[3]):
                                    z(z_fg):
                                    horizalign(right)
                                );
                                if let Actor::Text { scale, .. } = &mut a {
                                    scale[0] = numbers_zoom_x;
                                    scale[1] = numbers_zoom_y;
                                }
                                actors.push(a);
                            }
                        }
                    }
                } else {
                    for (row_i, grade) in JUDGMENT_ORDER.iter().enumerate() {
                        let count = *state.players[player_idx]
                            .judgment_counts
                            .get(grade)
                            .unwrap_or(&0);
                        let bright = match grade {
                            JudgeGrade::Fantastic => fantastic_color,
                            JudgeGrade::Excellent => excellent_color,
                            JudgeGrade::Great => great_color,
                            JudgeGrade::Decent => decent_color,
                            JudgeGrade::WayOff => wayoff_color,
                            JudgeGrade::Miss => miss_color,
                        };
                        let dim = match row_i {
                            0 => dim_fantastic,
                            1 => dim_excellent,
                            2 => dim_great,
                            3 => dim_decent,
                            4 => dim_wayoff,
                            _ => dim_miss,
                        };
                        let y = group_origin_y + (y_base + row_i as f32 * row_height) * group_zoom_y;
                        let s = format!("{:0width$}", count, width = digits);
                        let first_nonzero = s.find(|c: char| c != '0').unwrap_or(s.len());

                        for (i, ch) in s.chars().enumerate() {
                            let is_dim = if count == 0 {
                                i < digits.saturating_sub(1)
                            } else {
                                i < first_nonzero
                            };
                            let c = if is_dim { dim } else { bright };
                            if is_p1 {
                                let x = anchor_x + (i as f32) * digit_w;
                                let mut a = act!(text:
                                    font("wendy_screenevaluation"): settext(ch.to_string()):
                                    align(0.0, 0.5): xy(x, y):
                                    zoom(numbers_zoom_y):
                                    diffuse(c[0], c[1], c[2], c[3]):
                                    z(z_fg):
                                    horizalign(left)
                                );
                                if let Actor::Text { scale, .. } = &mut a {
                                    scale[0] = numbers_zoom_x;
                                    scale[1] = numbers_zoom_y;
                                }
                                actors.push(a);
                            } else {
                                let idx_from_right = digits.saturating_sub(1).saturating_sub(i);
                                let x = anchor_x - (idx_from_right as f32) * digit_w;
                                let mut a = act!(text:
                                    font("wendy_screenevaluation"): settext(ch.to_string()):
                                    align(1.0, 0.5): xy(x, y):
                                    zoom(numbers_zoom_y):
                                    diffuse(c[0], c[1], c[2], c[3]):
                                    z(z_fg):
                                    horizalign(right)
                                );
                                if let Actor::Text { scale, .. } = &mut a {
                                    scale[0] = numbers_zoom_x;
                                    scale[1] = numbers_zoom_y;
                                }
                                actors.push(a);
                            }
                        }
                    }
                }
            }
        });
    });

    if let Some(banner_path) = &state.song.banner_path {
        let key = banner_path.to_string_lossy().into_owned();
        actors.push(act!(sprite(key):
            align(0.5, 0.5):
            xy(screen_center_x(), screen_center_y() + 70.0):
            setsize(418.0, 164.0):
            zoom(0.3):
            z(z_fg)
        ));
    }

    actors
}

pub fn build_double_step_stats(
    state: &State,
    asset_manager: &AssetManager,
    playfield_center_x: f32,
) -> Vec<Actor> {
    if !is_wide() {
        return vec![];
    }
    let is_ultrawide = screen_width() / screen_height().max(1.0) > (21.0 / 9.0);
    if is_ultrawide {
        return vec![];
    }
    if state.cols_per_player <= 4 {
        return vec![];
    }

    let Some(notefield_width) = notefield_width(state) else {
        return vec![];
    };

    // Simply Love: StepStatistics/default.lua
    // - StepStatsPane centered: x=_screen.cx, y=_screen.cy+80
    // - BannerAndData is scaled when the notefield is centered (aspect 16:10..16:9)
    let header_h = 80.0;
    let pane_cx = screen_center_x();
    let pane_cy = screen_center_y() + header_h;

    let note_field_is_centered = (playfield_center_x - screen_center_x()).abs() < 1.0;
    let banner_data_zoom = if note_field_is_centered {
        let ar = screen_width() / screen_height();
        let t = ((ar - (16.0 / 10.0)) / ((16.0 / 9.0) - (16.0 / 10.0))).clamp(0.0, 1.0);
        0.825 + (0.925 - 0.825) * t
    } else {
        1.0
    };

    let mut actors = Vec::with_capacity(256);

    // DarkBackground.lua (double): two 200px-wide panels flanking the notefield.
    let nf_half_w = notefield_width * 0.5;
    let bg_y = screen_center_y();
    let z_bg = -80i16;
    actors.push(act!(quad:
        align(1.0, 0.5):
        xy(pane_cx - nf_half_w, bg_y):
        zoomto(200.0, screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.95):
        z(z_bg)
    ));
    actors.push(act!(quad:
        align(0.0, 0.5):
        xy(pane_cx + nf_half_w, bg_y):
        zoomto(200.0, screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.95):
        z(z_bg)
    ));

    // Banner.lua (double): xy(GetNotefieldWidth() - 140, -200)
    if let Some(banner_path) = &state.song.banner_path {
        let banner_key = banner_path.to_string_lossy().into_owned();
        let banner_x = pane_cx + ((notefield_width - 140.0) * banner_data_zoom);
        let banner_y = pane_cy + (-200.0 * banner_data_zoom);
        actors.push(act!(sprite(banner_key):
            align(0.5, 0.5): xy(banner_x, banner_y):
            setsize(418.0, 164.0):
            zoom(0.4 * banner_data_zoom):
            z(-50)
        ));
    }

    // TapNoteJudgments.lua (double): x(-GetNotefieldWidth() + 75), y(40), zoom(0.8)
    {
        let origin_x = pane_cx + ((-notefield_width + 75.0) * banner_data_zoom);
        let origin_y = pane_cy + (40.0 * banner_data_zoom);
        let base_zoom = 0.8 * banner_data_zoom;

        let total_tapnotes = state.chart.stats.total_steps as f32;
        let digits = if total_tapnotes > 0.0 {
            (total_tapnotes.log10().floor() as usize + 1).max(4)
        } else {
            4
        };
        let profile = profile::get();
        let show_fa_plus_window = profile.show_fa_plus_window;
        let row_height = if show_fa_plus_window { 29.0 } else { 35.0 };
        let y_base = -280.0;

        asset_manager.with_fonts(|all_fonts| {
            asset_manager.with_font("wendy_screenevaluation", |f| {
                let numbers_zoom = base_zoom * 0.5;
                let digit_w = (font::measure_line_width_logical(f, "0", all_fonts) as f32) * numbers_zoom;
                if digit_w <= 0.0 {
                    return;
                }
                let block_w = digit_w * digits as f32;
                let numbers_left_x = origin_x + (1.4 * block_w);
                let label_x = origin_x + ((80.0 + (digits.saturating_sub(4) as f32 * 16.0)) * base_zoom);
                let label_zoom = base_zoom * 0.833;

                let rows: Vec<(&str, [f32; 4], [f32; 4], u32)> = if !show_fa_plus_window {
                    JUDGMENT_ORDER
                        .iter()
	                        .enumerate()
	                        .map(|(i, grade)| {
	                            let info = JUDGMENT_INFO.get(grade).unwrap();
	                            let count = *state.players[0].judgment_counts.get(grade).unwrap_or(&0);
	                            let bright = info.color;
	                            let dim = color::JUDGMENT_DIM_RGBA[i];
	                            (info.label, bright, dim, count)
	                        })
	                        .collect()
	                } else {
                    let wc = timing_stats::compute_window_counts(&state.notes);
	                    let fantastic_color = JUDGMENT_INFO
	                        .get(&JudgeGrade::Fantastic)
	                        .map(|info| info.color)
	                        .unwrap_or_else(|| color::JUDGMENT_RGBA[0]);
	                    let excellent_color = JUDGMENT_INFO
	                        .get(&JudgeGrade::Excellent)
	                        .map(|info| info.color)
	                        .unwrap_or_else(|| color::JUDGMENT_RGBA[1]);
	                    let great_color = JUDGMENT_INFO
	                        .get(&JudgeGrade::Great)
	                        .map(|info| info.color)
	                        .unwrap_or_else(|| color::JUDGMENT_RGBA[2]);
	                    let decent_color = JUDGMENT_INFO
	                        .get(&JudgeGrade::Decent)
	                        .map(|info| info.color)
	                        .unwrap_or_else(|| color::JUDGMENT_RGBA[3]);
	                    let wayoff_color = JUDGMENT_INFO
	                        .get(&JudgeGrade::WayOff)
	                        .map(|info| info.color)
	                        .unwrap_or_else(|| color::JUDGMENT_RGBA[4]);
	                    let miss_color = JUDGMENT_INFO
	                        .get(&JudgeGrade::Miss)
	                        .map(|info| info.color)
	                        .unwrap_or_else(|| color::JUDGMENT_RGBA[5]);

	                    let dim_fantastic = color::JUDGMENT_DIM_RGBA[0];
	                    let dim_excellent = color::JUDGMENT_DIM_RGBA[1];
	                    let dim_great = color::JUDGMENT_DIM_RGBA[2];
	                    let dim_decent = color::JUDGMENT_DIM_RGBA[3];
	                    let dim_wayoff = color::JUDGMENT_DIM_RGBA[4];
	                    let dim_miss = color::JUDGMENT_DIM_RGBA[5];
	                    let dim_white_fa = color::JUDGMENT_FA_PLUS_WHITE_GAMEPLAY_DIM_RGBA;

	                    let white_fa_color = color::JUDGMENT_FA_PLUS_WHITE_RGBA;

                    vec![
                        ("FANTASTIC", fantastic_color, dim_fantastic, wc.w0),
                        ("FANTASTIC", white_fa_color, dim_white_fa, wc.w1),
                        ("EXCELLENT", excellent_color, dim_excellent, wc.w2),
                        ("GREAT", great_color, dim_great, wc.w3),
                        ("DECENT", decent_color, dim_decent, wc.w4),
                        ("WAY OFF", wayoff_color, dim_wayoff, wc.w5),
                        ("MISS", miss_color, dim_miss, wc.miss),
                    ]
                };

                for (row_i, (label, bright, dim, count)) in rows.iter().enumerate() {
                    let local_y = y_base + (row_i as f32 * row_height);
                    let y_numbers = origin_y + (local_y * base_zoom);
                    let y_label = origin_y + ((local_y + 1.0) * base_zoom);

                    let s = format!("{:0width$}", count, width = digits);
                    let first_nonzero = s.find(|c: char| c != '0').unwrap_or(s.len());

                    for (i, ch) in s.chars().enumerate() {
                        let is_dim = if *count == 0 { i < digits.saturating_sub(1) } else { i < first_nonzero };
                        let c = if is_dim { *dim } else { *bright };
                        let x = numbers_left_x + (i as f32) * digit_w;
                        actors.push(act!(text:
                            font("wendy_screenevaluation"): settext(ch.to_string()):
                            align(0.0, 0.5): xy(x, y_numbers):
                            zoom(numbers_zoom):
                            diffuse(c[0], c[1], c[2], c[3]):
                            z(71):
                            horizalign(left)
                        ));
                    }

                    actors.push(act!(text:
                        font("miso"): settext(label.to_string()):
                        align(1.0, 0.5): horizalign(right):
                        xy(label_x, y_label):
                        zoom(label_zoom):
                        maxwidth(72.0 * base_zoom):
                        diffuse(bright[0], bright[1], bright[2], bright[3]):
                        z(71)
                    ));
                }
            });
        });
    }

    // HoldsMinesRolls.lua (double): x(-GetNotefieldWidth() + 212), y(-10), zoom(0.8)
    {
        let frame_cx = pane_cx + ((-notefield_width + 212.0) * banner_data_zoom);
        // Our holds/mines/rolls builder positions the frame origin at the *middle* row (Mines),
        // matching the non-double path where SL uses y=-140 and row2 is at y=28.
        // For double, SL uses y=-10 and zoom=0.8, so the middle row sits at:
        // -10 + (0.8 * 28) == 12.4
        let frame_cy = pane_cy + ((-10.0 + 0.8 * 28.0) * banner_data_zoom);
        let frame_zoom = 0.8 * banner_data_zoom;

        actors.extend(build_holds_mines_rolls_pane_at(
            state,
            asset_manager,
            frame_cx,
            frame_cy,
            frame_zoom,
        ));
    }

    // Time.lua (double): x(-GetNotefieldWidth() + 150), y(75)
    {
        let base_x = pane_cx + ((-notefield_width + 150.0) * banner_data_zoom);
        let base_y = pane_cy + (75.0 * banner_data_zoom);

        let base_total = state.song.total_length_seconds.max(0) as f32;
        let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
            state.music_rate
        } else {
            1.0
        };
        let total_display_seconds = if rate != 0.0 { base_total / rate } else { base_total };
        let elapsed_display_seconds = if rate != 0.0 {
            state.current_music_time.max(0.0) / rate
        } else {
            state.current_music_time.max(0.0)
        };

        let total_time_str = format_game_time(total_display_seconds, total_display_seconds);
        let remaining_display_seconds = if let Some(fail_time) = state.players[0].fail_time {
            let fail_disp = if rate != 0.0 { fail_time.max(0.0) / rate } else { fail_time.max(0.0) };
            (total_display_seconds - fail_disp).max(0.0)
        } else {
            (total_display_seconds - elapsed_display_seconds).max(0.0)
        };
        let remaining_time_str = format_game_time(remaining_display_seconds, total_display_seconds);

        let number_zoom = banner_data_zoom;
        let label_zoom = 0.833 * number_zoom;
        let total_w = asset_manager
            .with_fonts(|all_fonts| {
                asset_manager.with_font("miso", |f| {
                    font::measure_line_width_logical(f, &total_time_str, all_fonts) as f32
                })
            })
            .unwrap_or(0.0);

        // Simply Love (Time.lua):
        // label x = 32 + (total_width - 28) == total_width + 4
        let label_x = base_x + (total_w + 4.0) * number_zoom;

        // Remaining row (y=0)
        actors.push(act!(text:
            font("miso"):
            settext(remaining_time_str):
            align(-1.2, 0.5):
            xy(base_x, base_y):
            zoom(number_zoom):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(71)
        ));
        actors.push(act!(text:
            font("miso"):
            settext("remaining "):
            align(1.0, 0.5):
            horizalign(right):
            xy(label_x, base_y + 1.0 * number_zoom):
            zoom(label_zoom):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(71)
        ));

        // Total row (y=20)
        actors.push(act!(text:
            font("miso"):
            settext(total_time_str):
            align(-1.2, 0.5):
            xy(base_x, base_y + (20.0 * number_zoom)):
            zoom(number_zoom):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(71)
        ));
        actors.push(act!(text:
            font("miso"):
            settext("song "):
            align(1.0, 0.5):
            horizalign(right):
            xy(label_x, base_y + (20.0 * number_zoom) + 1.0 * number_zoom):
            zoom(label_zoom):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(71)
        ));
    }

    // Peak NPS text (DensityGraph.lua drives this in SL).
    {
        let scaled_peak = (state.chart.max_nps as f32 * state.music_rate).max(0.0);
        let peak_nps_text = format!("Peak NPS: {:.2}", scaled_peak);
        // Simply Love computes this inside DensityGraph.lua with a funky halign() in double,
        // but the visual intent is that the Peak NPS label lives in the right dark pane.
        let x = pane_cx + nf_half_w + 96.0;
        let y = screen_center_y() + 126.0;
        actors.push(act!(text:
            font("miso"):
            settext(peak_nps_text):
            align(1.0, 0.5):
            xy(x, y):
            zoom(0.9):
            diffuse(1.0, 1.0, 1.0, 1.0):
            horizalign(right):
            z(200)
        ));
    }

    actors
}

// --- Statics for Judgment Counter Display ---

static JUDGMENT_ORDER: [JudgeGrade; 6] = [
    JudgeGrade::Fantastic,
    JudgeGrade::Excellent,
    JudgeGrade::Great,
    JudgeGrade::Decent,
    JudgeGrade::WayOff,
    JudgeGrade::Miss,
];

struct JudgmentDisplayInfo {
    label: &'static str,
    color: [f32; 4],
}

static JUDGMENT_INFO: LazyLock<HashMap<JudgeGrade, JudgmentDisplayInfo>> = LazyLock::new(|| {
    HashMap::from([
        (
            JudgeGrade::Fantastic,
            JudgmentDisplayInfo {
                label: "FANTASTIC",
                color: color::JUDGMENT_RGBA[0],
            },
        ),
        (
            JudgeGrade::Excellent,
            JudgmentDisplayInfo {
                label: "EXCELLENT",
                color: color::JUDGMENT_RGBA[1],
            },
        ),
        (
            JudgeGrade::Great,
            JudgmentDisplayInfo {
                label: "GREAT",
                color: color::JUDGMENT_RGBA[2],
            },
        ),
        (
            JudgeGrade::Decent,
            JudgmentDisplayInfo {
                label: "DECENT",
                color: color::JUDGMENT_RGBA[3],
            },
        ),
        (
            JudgeGrade::WayOff,
            JudgmentDisplayInfo {
                label: "WAY OFF",
                color: color::JUDGMENT_RGBA[4],
            },
        ),
        (
            JudgeGrade::Miss,
            JudgmentDisplayInfo {
                label: "MISS",
                color: color::JUDGMENT_RGBA[5],
            },
        ),
    ])
});

fn format_game_time(s: f32, total_seconds: f32) -> String {
    if s < 0.0 {
        return format_game_time(0.0, total_seconds);
    }
    let s_u64 = s as u64;

    let minutes = s_u64 / 60;
    let seconds = s_u64 % 60;

    if total_seconds >= 3600.0 {
        // Over an hour total? use H:MM:SS
        let hours = s_u64 / 3600;
        let minutes = (s_u64 % 3600) / 60;
        format!("{}:{:02}:{:02}", hours, minutes, seconds)
    } else if total_seconds >= 600.0 {
        // Over 10 mins total? use MM:SS
        format!("{:02}:{:02}", minutes, seconds)
    } else {
        // Under 10 mins total? use M:SS
        format!("{}:{:02}", minutes, seconds)
    }
}

fn build_banner(
    state: &State,
    playfield_center_x: f32,
    player_side: profile::PlayerSide,
) -> Vec<Actor> {
    let mut actors = Vec::new();
    if let Some(banner_path) = &state.song.banner_path {
        let banner_key = banner_path.to_string_lossy().into_owned();
        let wide = is_wide();
        let sidepane_center_x = match player_side {
            profile::PlayerSide::P1 => screen_width() * 0.75,
            profile::PlayerSide::P2 => screen_width() * 0.25,
        };
        let sidepane_center_y = screen_center_y() + 80.0;
        let note_field_is_centered = (playfield_center_x - screen_center_x()).abs() < 1.0;
        let is_ultrawide = screen_width() / screen_height() > (21.0 / 9.0);
        let banner_data_zoom = if note_field_is_centered && wide && !is_ultrawide {
            let ar = screen_width() / screen_height();
            let t = ((ar - (16.0 / 10.0)) / ((16.0 / 9.0) - (16.0 / 10.0))).clamp(0.0, 1.0);
            0.825 + (0.925 - 0.825) * t
        } else {
            1.0
        };
        let mut local_banner_x = 70.0;
        if note_field_is_centered && wide {
            local_banner_x = 72.0;
        }
        if player_side == profile::PlayerSide::P2 {
            local_banner_x *= -1.0;
        }
        let local_banner_y = -200.0;
        let banner_x = sidepane_center_x + (local_banner_x * banner_data_zoom);
        let banner_y = sidepane_center_y + (local_banner_y * banner_data_zoom);
        let final_zoom = 0.4 * banner_data_zoom;
        actors.push(act!(sprite(banner_key):
            align(0.5, 0.5): xy(banner_x, banner_y):
            setsize(418.0, 164.0): zoom(final_zoom):
            z(-50)
        ));
    }
    actors
}

fn build_holds_mines_rolls_pane_at(
    state: &State,
    asset_manager: &AssetManager,
    frame_cx: f32,
    frame_cy: f32,
    frame_zoom: f32,
) -> Vec<Actor> {
    let p = &state.players[0];
    let mut actors = Vec::new();

    let categories = [
        ("holds", p.holds_held, state.holds_total),
        ("mines", p.mines_avoided, state.mines_total),
        ("rolls", p.rolls_held, state.rolls_total),
    ];

    let largest_count = categories
        .iter()
        .map(|(_, achieved, total)| (*achieved).max(*total))
        .max()
        .unwrap_or(0);
    let digits_needed = if largest_count == 0 {
        1
    } else {
        (largest_count as f32).log10().floor() as usize + 1
    };
    let digits_to_fmt = digits_needed.clamp(3, 4);
    let row_height = 28.0 * frame_zoom;
    let mut children = Vec::new();

    asset_manager.with_fonts(|all_fonts| {
        asset_manager.with_font("wendy_screenevaluation", |metrics_font| {
            let value_zoom = 0.4 * frame_zoom;
            let label_zoom = 0.833 * frame_zoom;
            const GRAY: [f32; 4] = color::rgba_hex("#5A6166");
            let white = [1.0, 1.0, 1.0, 1.0];

            let digit_width =
                font::measure_line_width_logical(metrics_font, "0", all_fonts) as f32 * value_zoom;
            if digit_width <= 0.0 {
                return;
            }
            let slash_width =
                font::measure_line_width_logical(metrics_font, "/", all_fonts) as f32 * value_zoom;

            const LOGICAL_CHAR_WIDTH_FOR_LABEL: f32 = 36.0;
            let fixed_char_width_scaled_for_label = LOGICAL_CHAR_WIDTH_FOR_LABEL * value_zoom;

            for (i, (label_text, achieved, total)) in categories.iter().enumerate() {
                let item_y = (i as f32 - 1.0) * row_height;
                let right_anchor_x = 0.0;
                let mut cursor_x = right_anchor_x;

                let possible_str = format!("{:0width$}", *total as usize, width = digits_to_fmt);
                let achieved_str = format!("{:0width$}", *achieved as usize, width = digits_to_fmt);

                let first_nonzero_possible =
                    possible_str.find(|c: char| c != '0').unwrap_or(possible_str.len());
                for (char_idx, ch) in possible_str.chars().rev().enumerate() {
                    let is_dim = if *total == 0 {
                        char_idx > 0
                    } else {
                        let original_index = digits_to_fmt - 1 - char_idx;
                        original_index < first_nonzero_possible
                    };
                    let color = if is_dim { GRAY } else { white };
                    let x_pos = cursor_x - (char_idx as f32 * digit_width);
                    children.push(act!(text:
                        font("wendy_screenevaluation"): settext(ch.to_string()):
                        align(1.0, 0.5): xy(x_pos, item_y):
                        zoom(value_zoom): diffuse(color[0], color[1], color[2], color[3])
                    ));
                }
                cursor_x -= possible_str.len() as f32 * digit_width;

                children.push(act!(text:
                    font("wendy_screenevaluation"): settext("/"):
                    align(1.0, 0.5): xy(cursor_x, item_y):
                    zoom(value_zoom): diffuse(GRAY[0], GRAY[1], GRAY[2], GRAY[3])
                ));
                cursor_x -= slash_width;

                let achieved_block_right_x = cursor_x;
                let first_nonzero_achieved =
                    achieved_str.find(|c: char| c != '0').unwrap_or(achieved_str.len());
                for (char_idx, ch) in achieved_str.chars().rev().enumerate() {
                    let is_dim = if *achieved == 0 {
                        char_idx > 0
                    } else {
                        let original_index = digits_to_fmt - 1 - char_idx;
                        original_index < first_nonzero_achieved
                    };
                    let color = if is_dim { GRAY } else { white };
                    let x_pos = achieved_block_right_x - (char_idx as f32 * digit_width);
                    children.push(act!(text:
                        font("wendy_screenevaluation"): settext(ch.to_string()):
                        align(1.0, 0.5): xy(x_pos, item_y):
                        zoom(value_zoom): diffuse(color[0], color[1], color[2], color[3])
                    ));
                }

                let total_value_width_for_label = (achieved_str.len() + 1 + possible_str.len())
                    as f32
                    * fixed_char_width_scaled_for_label;
                let label_x = right_anchor_x - total_value_width_for_label - (10.0 * frame_zoom);

                children.push(act!(text:
                    font("miso"): settext(*label_text):
                    align(1.0, 0.5): xy(label_x, item_y):
                    zoom(label_zoom):
                    horizalign(right):
                    diffuse(white[0], white[1], white[2], white[3])
                ));
            }
        });
    });

    actors.push(Actor::Frame {
        align: [0.5, 0.5],
        offset: [frame_cx, frame_cy],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        children,
        background: None,
        z: 70,
    });
    actors
}

fn notefield_width(state: &State) -> Option<f32> {
    let ns = state.noteskin.as_ref()?;
    let cols = state
        .cols_per_player
        .min(ns.column_xs.len())
        .min(ns.receptor_off.len());
    if cols == 0 {
        return None;
    }

    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    for x in ns.column_xs.iter().take(cols) {
        let xf = *x as f32;
        min_x = min_x.min(xf);
        max_x = max_x.max(xf);
    }

    let target_arrow_px = 64.0 * state.field_zoom.max(0.0);
    let size = ns.receptor_off[0].size();
    let w = size[0].max(0) as f32;
    let h = size[1].max(0) as f32;
    let arrow_w = if h > 0.0 && target_arrow_px > 0.0 {
        w * (target_arrow_px / h)
    } else {
        w * state.field_zoom.max(0.0)
    };

    Some(((max_x - min_x) * state.field_zoom.max(0.0)) + arrow_w)
}

fn build_holds_mines_rolls_pane(
    state: &State,
    asset_manager: &AssetManager,
    playfield_center_x: f32,
    player_side: profile::PlayerSide,
) -> Vec<Actor> {
    if !is_wide() {
        return vec![];
    }
    let p = &state.players[0];
    let mut actors = Vec::new();

    let sidepane_center_x = match player_side {
        profile::PlayerSide::P1 => screen_width() * 0.75,
        profile::PlayerSide::P2 => screen_width() * 0.25,
    };
    let sidepane_center_y = screen_center_y() + 80.0;
    let note_field_is_centered = (playfield_center_x - screen_center_x()).abs() < 1.0;
    let is_ultrawide = screen_width() / screen_height() > (21.0 / 9.0);
    let banner_data_zoom = if note_field_is_centered && is_wide() && !is_ultrawide {
        let ar = screen_width() / screen_height();
        let t = ((ar - (16.0 / 10.0)) / ((16.0 / 9.0) - (16.0 / 10.0))).clamp(0.0, 1.0);
        0.825 + (0.925 - 0.825) * t
    } else {
        1.0
    };
    let local_x = match player_side {
        profile::PlayerSide::P1 => 155.0,
        profile::PlayerSide::P2 => -85.0,
    };
    let local_y = -112.0;
    let frame_cx = sidepane_center_x + (local_x * banner_data_zoom);
    let frame_cy = sidepane_center_y + (local_y * banner_data_zoom);
    let frame_zoom = banner_data_zoom;

    let categories = [
        ("holds", p.holds_held, state.holds_total),
        ("mines", p.mines_avoided, state.mines_total),
        ("rolls", p.rolls_held, state.rolls_total),
    ];

    let largest_count = categories
        .iter()
        .map(|(_, achieved, total)| (*achieved).max(*total))
        .max()
        .unwrap_or(0);
    let digits_needed = if largest_count == 0 {
        1
    } else {
        (largest_count as f32).log10().floor() as usize + 1
    };
    let digits_to_fmt = digits_needed.clamp(3, 4);
    let row_height = 28.0 * frame_zoom;
    let mut children = Vec::new();

    asset_manager.with_fonts(|all_fonts| asset_manager.with_font("wendy_screenevaluation", |metrics_font| {
        let value_zoom = 0.4 * frame_zoom;
        let label_zoom = 0.833 * frame_zoom;
        let gray = color::rgba_hex("#5A6166");
        let white = [1.0, 1.0, 1.0, 1.0];

        // --- HYBRID LAYOUT LOGIC ---
        // 1. Measure real character widths for number layout.
        let digit_width = font::measure_line_width_logical(metrics_font, "0", all_fonts) as f32 * value_zoom;
        if digit_width <= 0.0 { return; }
        let slash_width = font::measure_line_width_logical(metrics_font, "/", all_fonts) as f32 * value_zoom;

        // 2. Use a hardcoded width for calculating the label's position (for theme parity).
        const LOGICAL_CHAR_WIDTH_FOR_LABEL: f32 = 36.0;
        let fixed_char_width_scaled_for_label = LOGICAL_CHAR_WIDTH_FOR_LABEL * value_zoom;

        for (i, (label_text, achieved, total)) in categories.iter().enumerate() {
            let item_y = (i as f32 - 1.0) * row_height;
            let right_anchor_x = match player_side {
                profile::PlayerSide::P1 => 0.0,
                profile::PlayerSide::P2 => 100.0 * frame_zoom,
            };
            let mut cursor_x = right_anchor_x;

            let possible_str = format!("{:0width$}", *total as usize, width = digits_to_fmt);
            let achieved_str = format!("{:0width$}", *achieved as usize, width = digits_to_fmt);

            // --- Layout Numbers using MEASURED widths ---
            // 1. Draw "possible" number (right-most part)
            let first_nonzero_possible = possible_str.find(|c: char| c != '0').unwrap_or(possible_str.len());
            for (char_idx, ch) in possible_str.chars().rev().enumerate() {
                let is_dim = if *total == 0 { char_idx > 0 } else {
                    let original_index = digits_to_fmt - 1 - char_idx;
                    original_index < first_nonzero_possible
                };
                let color = if is_dim { gray } else { white };
                let x_pos = cursor_x - (char_idx as f32 * digit_width);
                children.push(act!(text:
                    font("wendy_screenevaluation"): settext(ch.to_string()):
                    align(1.0, 0.5): xy(x_pos, item_y):
                    zoom(value_zoom): diffuse(color[0], color[1], color[2], color[3])
                ));
            }
            cursor_x -= possible_str.len() as f32 * digit_width;

            // 2. Draw slash
            children.push(act!(text: font("wendy_screenevaluation"): settext("/"): align(1.0, 0.5): xy(cursor_x, item_y): zoom(value_zoom): diffuse(gray[0], gray[1], gray[2], gray[3])));
            cursor_x -= slash_width;

            // 3. Draw "achieved" number
            let achieved_block_right_x = cursor_x;
            let first_nonzero_achieved = achieved_str.find(|c: char| c != '0').unwrap_or(achieved_str.len());
            for (char_idx, ch) in achieved_str.chars().rev().enumerate() {
                let is_dim = if *achieved == 0 { char_idx > 0 } else {
                    let original_index = digits_to_fmt - 1 - char_idx;
                    original_index < first_nonzero_achieved
                };
                let color = if is_dim { gray } else { white };
                let x_pos = achieved_block_right_x - (char_idx as f32 * digit_width);
                children.push(act!(text:
                    font("wendy_screenevaluation"): settext(ch.to_string()):
                    align(1.0, 0.5): xy(x_pos, item_y):
                    zoom(value_zoom): diffuse(color[0], color[1], color[2], color[3])
                ));
            }

            // --- Position Label using HARDCODED width assumption ---
            let total_value_width_for_label = (achieved_str.len() + 1 + possible_str.len()) as f32 * fixed_char_width_scaled_for_label;
            let label_x = right_anchor_x - total_value_width_for_label - (10.0 * frame_zoom);

            children.push(act!(text:
                font("miso"): settext(*label_text): align(1.0, 0.5): xy(label_x, item_y):
                zoom(label_zoom): horizalign(right): diffuse(white[0], white[1], white[2], white[3])
            ));
        }
    }));

    actors.push(Actor::Frame {
        align: [0.5, 0.5],
        offset: [frame_cx, frame_cy],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        children,
        background: None,
        z: 70,
    });
    actors
}

fn build_side_pane(
    state: &State,
    asset_manager: &AssetManager,
    playfield_center_x: f32,
    player_side: profile::PlayerSide,
) -> Vec<Actor> {
    if !is_wide() {
        return vec![];
    }
    let mut actors = Vec::new();

    let sidepane_center_x = match player_side {
        profile::PlayerSide::P1 => screen_width() * 0.75,
        profile::PlayerSide::P2 => screen_width() * 0.25,
    };
    let sidepane_center_y = screen_center_y() + 80.0;
    let note_field_is_centered = (playfield_center_x - screen_center_x()).abs() < 1.0;
    let is_ultrawide = screen_width() / screen_height() > (21.0 / 9.0);
    let banner_data_zoom = if note_field_is_centered && is_wide() && !is_ultrawide {
        let ar = screen_width() / screen_height();
        let t = ((ar - (16.0 / 10.0)) / ((16.0 / 9.0) - (16.0 / 10.0))).clamp(0.0, 1.0);
        0.825 + (0.925 - 0.825) * t
    } else {
        1.0
    };

    let x_sign = match player_side {
        profile::PlayerSide::P1 => 1.0,
        profile::PlayerSide::P2 => -1.0,
    };
    let judgments_local_x = -widescale(152.0, 204.0) * x_sign;
    let final_judgments_center_x = sidepane_center_x + (judgments_local_x * banner_data_zoom);
    let final_judgments_center_y = sidepane_center_y;
    let parent_local_zoom = 0.8;
    let final_text_base_zoom = banner_data_zoom * parent_local_zoom;

    let total_tapnotes = state.chart.stats.total_steps as f32;
    let digits = if total_tapnotes > 0.0 {
        (total_tapnotes.log10().floor() as usize + 1).max(4)
    } else {
        4
    };
    let extra_digits = digits.saturating_sub(4) as f32;
    let base_label_local_x_offset = 80.0;
    const LABEL_DIGIT_STEP: f32 = 16.0;
    const NUMBER_TO_LABEL_GAP: f32 = 8.0;
    let base_numbers_local_x_offset = base_label_local_x_offset - NUMBER_TO_LABEL_GAP;
    let profile = profile::get();
    let show_fa_plus_window = profile.show_fa_plus_window;
    let row_height = if show_fa_plus_window { 29.0 } else { 35.0 };
    let y_base = -280.0;

    asset_manager.with_fonts(|all_fonts| asset_manager.with_font("wendy_screenevaluation", |f| {
        let numbers_zoom = final_text_base_zoom * 0.5;
        let max_digit_w = (font::measure_line_width_logical(f, "0", all_fonts) as f32) * numbers_zoom;
        if max_digit_w <= 0.0 { return; }

        let digit_local_width = max_digit_w / final_text_base_zoom;
        let label_local_x_offset = base_label_local_x_offset + (extra_digits * LABEL_DIGIT_STEP);
        let label_world_x =
            final_judgments_center_x + (x_sign * label_local_x_offset * final_text_base_zoom);
        let numbers_local_x_offset = base_numbers_local_x_offset + (extra_digits * digit_local_width);
        let numbers_cx =
            final_judgments_center_x + (x_sign * numbers_local_x_offset * final_text_base_zoom);

        if !show_fa_plus_window {
            // Standard ITG-style rows: Fantastic..Miss using aggregate grade counts.
            for (index, grade) in JUDGMENT_ORDER.iter().enumerate() {
                let info = JUDGMENT_INFO.get(grade).unwrap();
                let count = *state.players[0].judgment_counts.get(grade).unwrap_or(&0);

                let local_y = y_base + (index as f32 * row_height);
                let world_y = final_judgments_center_y + (local_y * final_text_base_zoom);

                let bright = info.color;
                let dim = color::JUDGMENT_DIM_RGBA[index];
                let full_number_str = format!("{:0width$}", count, width = digits);

                for (i, ch) in full_number_str.chars().enumerate() {
                    let is_dim = if count == 0 { i < digits - 1 } else {
                        let first_nonzero = full_number_str.find(|c: char| c != '0').unwrap_or(full_number_str.len());
                        i < first_nonzero
                    };
                    let color = if is_dim { dim } else { bright };
                    if player_side == profile::PlayerSide::P1 {
                        let index_from_right = digits - 1 - i;
                        let cell_right_x = numbers_cx - (index_from_right as f32 * max_digit_w);
                        actors.push(act!(text:
                            font("wendy_screenevaluation"): settext(ch.to_string()):
                            align(1.0, 0.5): xy(cell_right_x, world_y): zoom(numbers_zoom):
                            diffuse(color[0], color[1], color[2], color[3]): z(71)
                        ));
                    } else {
                        let cell_left_x = numbers_cx + (i as f32 * max_digit_w);
                        actors.push(act!(text:
                            font("wendy_screenevaluation"): settext(ch.to_string()):
                            align(0.0, 0.5): xy(cell_left_x, world_y): zoom(numbers_zoom):
                            diffuse(color[0], color[1], color[2], color[3]): z(71):
                            horizalign(left)
                        ));
                    }
                }

                let label_world_y = world_y + (1.0 * final_text_base_zoom);
                let label_zoom = final_text_base_zoom * 0.833;

                if player_side == profile::PlayerSide::P1 {
                    actors.push(act!(text:
                        font("miso"): settext(info.label): align(0.0, 0.5):
                        xy(label_world_x, label_world_y): zoom(label_zoom):
                        maxwidth(72.0 * final_text_base_zoom): horizalign(left):
                        diffuse(bright[0], bright[1], bright[2], bright[3]):
                        z(71)
                    ));
                } else {
                    actors.push(act!(text:
                        font("miso"): settext(info.label): align(1.0, 0.5):
                        xy(label_world_x, label_world_y): zoom(label_zoom):
                        maxwidth(72.0 * final_text_base_zoom): horizalign(right):
                        diffuse(bright[0], bright[1], bright[2], bright[3]):
                        z(71)
                    ));
                }
            }
        } else {
            // FA+ mode: split Fantastic into W0 (blue) and W1 (white) using per-note windows,
            // matching Simply Love's FA+ Step Statistics semantics.
            let wc = timing_stats::compute_window_counts(&state.notes);
	            let fantastic_color = JUDGMENT_INFO
	                .get(&JudgeGrade::Fantastic)
	                .map(|info| info.color)
	                .unwrap_or_else(|| color::JUDGMENT_RGBA[0]);
	            let excellent_color = JUDGMENT_INFO
	                .get(&JudgeGrade::Excellent)
	                .map(|info| info.color)
	                .unwrap_or_else(|| color::JUDGMENT_RGBA[1]);
	            let great_color = JUDGMENT_INFO
	                .get(&JudgeGrade::Great)
	                .map(|info| info.color)
	                .unwrap_or_else(|| color::JUDGMENT_RGBA[2]);
	            let decent_color = JUDGMENT_INFO
	                .get(&JudgeGrade::Decent)
	                .map(|info| info.color)
	                .unwrap_or_else(|| color::JUDGMENT_RGBA[3]);
	            let wayoff_color = JUDGMENT_INFO
	                .get(&JudgeGrade::WayOff)
	                .map(|info| info.color)
	                .unwrap_or_else(|| color::JUDGMENT_RGBA[4]);
	            let miss_color = JUDGMENT_INFO
	                .get(&JudgeGrade::Miss)
	                .map(|info| info.color)
	                .unwrap_or_else(|| color::JUDGMENT_RGBA[5]);

            // Dim palette for FA+ side pane: reuse gameplay dim colors for Fantastic..Miss,
            // and a dedicated dim color for the white FA+ row.
	            let dim_fantastic = color::JUDGMENT_DIM_RGBA[0];
	            let dim_excellent = color::JUDGMENT_DIM_RGBA[1];
	            let dim_great = color::JUDGMENT_DIM_RGBA[2];
	            let dim_decent = color::JUDGMENT_DIM_RGBA[3];
	            let dim_wayoff = color::JUDGMENT_DIM_RGBA[4];
	            let dim_miss = color::JUDGMENT_DIM_RGBA[5];
	            let dim_white_fa = color::JUDGMENT_FA_PLUS_WHITE_GAMEPLAY_DIM_RGBA;

	            let white_fa_color = color::JUDGMENT_FA_PLUS_WHITE_RGBA;

            let rows: [(&str, [f32; 4], [f32; 4], u32); 7] = [
                ("FANTASTIC", fantastic_color, dim_fantastic, wc.w0),
                ("FANTASTIC",       white_fa_color, dim_white_fa, wc.w1),
                ("EXCELLENT", excellent_color, dim_excellent, wc.w2),
                ("GREAT",     great_color, dim_great, wc.w3),
                ("DECENT",    decent_color, dim_decent, wc.w4),
                ("WAY OFF",   wayoff_color, dim_wayoff, wc.w5),
                ("MISS",      miss_color, dim_miss, wc.miss),
            ];

            for (index, (label, bright, dim, count)) in rows.iter().enumerate() {
                let local_y = y_base + (index as f32 * row_height);
                let world_y = final_judgments_center_y + (local_y * final_text_base_zoom);

                let full_number_str = format!("{:0width$}", count, width = digits);

                for (i, ch) in full_number_str.chars().enumerate() {
                    let is_dim = if *count == 0 { i < digits - 1 } else {
                        let first_nonzero = full_number_str.find(|c: char| c != '0').unwrap_or(full_number_str.len());
                        i < first_nonzero
                    };
                    let color = if is_dim { dim } else { bright };
                    if player_side == profile::PlayerSide::P1 {
                        let index_from_right = digits - 1 - i;
                        let cell_right_x = numbers_cx - (index_from_right as f32 * max_digit_w);
                        actors.push(act!(text:
                            font("wendy_screenevaluation"): settext(ch.to_string()):
                            align(1.0, 0.5): xy(cell_right_x, world_y): zoom(numbers_zoom):
                            diffuse(color[0], color[1], color[2], color[3]): z(71)
                        ));
                    } else {
                        let cell_left_x = numbers_cx + (i as f32 * max_digit_w);
                        actors.push(act!(text:
                            font("wendy_screenevaluation"): settext(ch.to_string()):
                            align(0.0, 0.5): xy(cell_left_x, world_y): zoom(numbers_zoom):
                            diffuse(color[0], color[1], color[2], color[3]): z(71):
                            horizalign(left)
                        ));
                    }
                }

                let label_world_y = world_y + (1.0 * final_text_base_zoom);
                let label_zoom = final_text_base_zoom * 0.833;

                if player_side == profile::PlayerSide::P1 {
                    actors.push(act!(text:
                        font("miso"): settext(label.to_string()): align(0.0, 0.5):
                        xy(label_world_x, label_world_y): zoom(label_zoom):
                        maxwidth(72.0 * final_text_base_zoom): horizalign(left):
                        diffuse(bright[0], bright[1], bright[2], bright[3]):
                        z(71)
                    ));
                } else {
                    actors.push(act!(text:
                        font("miso"): settext(label.to_string()): align(1.0, 0.5):
                        xy(label_world_x, label_world_y): zoom(label_zoom):
                        maxwidth(72.0 * final_text_base_zoom): horizalign(right):
                        diffuse(bright[0], bright[1], bright[2], bright[3]):
                        z(71)
                    ));
                }
            }
        }

        // --- Time Display (Remaining / Total) ---
        {
            let local_y = -40.0 * banner_data_zoom;

            // Base chart length in seconds (GetLastSecond semantics).
            let base_total = state.song.total_length_seconds.max(0) as f32;
            // Displayed duration should respect music rate (SongLength / MusicRate),
            // while the on-screen timer still advances in real seconds.
            let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
                state.music_rate
            } else {
                1.0
            };
            let total_display_seconds = if rate != 0.0 {
                base_total / rate
            } else {
                base_total
            };
            let elapsed_display_seconds = if rate != 0.0 {
                state.current_music_time.max(0.0) / rate
            } else {
                state.current_music_time.max(0.0)
            };

            let total_time_str = format_game_time(total_display_seconds, total_display_seconds);

            let remaining_display_seconds = if let Some(fail_time) = state.players[0].fail_time {
                let fail_disp = if rate != 0.0 {
                    fail_time.max(0.0) / rate
                } else {
                    fail_time.max(0.0)
                };
                (total_display_seconds - fail_disp).max(0.0)
            } else {
                (total_display_seconds - elapsed_display_seconds).max(0.0)
            };
            let remaining_time_str =
                format_game_time(remaining_display_seconds, total_display_seconds);

            let font_name = "miso";
            let text_zoom = banner_data_zoom * 0.833;
            // Time values currently render without explicit zoom, so treat as 1.0
            let time_value_zoom = 1.0_f32;

            let numbers_block_width = (digits as f32) * max_digit_w;
            let numbers_left_x = numbers_cx - numbers_block_width + 2.0;

            // Measure dynamic widths so labels always appear after the time text
            let (total_width_px, remaining_width_px, baseline_width_px) =
                asset_manager
                    .with_font(font_name, |time_font| {
                        let total_w = (font::measure_line_width_logical(
                            time_font,
                            &total_time_str,
                            all_fonts,
                        ) as f32)
                            * time_value_zoom;
                        let remaining_w = (font::measure_line_width_logical(
                            time_font,
                            &remaining_time_str,
                            all_fonts,
                        ) as f32)
                            * time_value_zoom;
                        // Use "9:59" as the baseline look the layout was tuned for
                        let baseline_w = (font::measure_line_width_logical(
                            time_font,
                            "9:59",
                            all_fonts,
                        ) as f32)
                            * time_value_zoom;
                        (total_w, remaining_w, baseline_w)
                    })
                    .unwrap_or((0.0_f32, 0.0_f32, 0.0_f32));

            let red_color = color::rgba_hex("#ff3030");
            let white_color = [1.0, 1.0, 1.0, 1.0];
            let remaining_color = if state.players[0].is_failing { red_color } else { white_color };

            // --- Total Time Row ---
            let y_pos_total = sidepane_center_y + local_y + 13.0;
            let label_offset: f32 = 29.0;
            // Keep original spacing for <= 9:59, otherwise push label after the time width
            let desired_gap_px = (label_offset - baseline_width_px).max(4.0_f32);
            let label_offset_total = if total_width_px > baseline_width_px {
                total_width_px + desired_gap_px
            } else {
                label_offset
            };

            let (time_x, label_dir) = if player_side == profile::PlayerSide::P1 {
                (numbers_left_x, 1.0_f32)
            } else {
                let numbers_right_x = numbers_cx + numbers_block_width - 2.0;
                (numbers_right_x, -1.0_f32)
            };

            if player_side == profile::PlayerSide::P1 {
                actors.push(act!(text: font(font_name): settext(total_time_str):
                    align(0.0, 0.5): horizalign(left):
                    xy(time_x, y_pos_total):
                    z(71):
                    diffuse(white_color[0], white_color[1], white_color[2], white_color[3])
                ));
                actors.push(act!(text: font(font_name): settext(" song"):
                    align(0.0, 0.5): horizalign(left):
                    xy(time_x + label_dir * label_offset_total, y_pos_total + 1.0):
                    zoom(text_zoom): z(71):
                    diffuse(white_color[0], white_color[1], white_color[2], white_color[3])
                ));
            } else {
                actors.push(act!(text: font(font_name): settext(total_time_str):
                    align(1.0, 0.5): horizalign(right):
                    xy(time_x, y_pos_total):
                    z(71):
                    diffuse(white_color[0], white_color[1], white_color[2], white_color[3])
                ));
                actors.push(act!(text: font(font_name): settext(" song"):
                    align(1.0, 0.5): horizalign(right):
                    xy(time_x + label_dir * label_offset_total, y_pos_total + 1.0):
                    zoom(text_zoom): z(71):
                    diffuse(white_color[0], white_color[1], white_color[2], white_color[3])
                ));
            }

            // --- Remaining Time Row ---
            let y_pos_remaining = sidepane_center_y + local_y - 7.0;

            // Keep original spacing for <= 9:59, otherwise push label after the time width
            let label_offset_remaining = if remaining_width_px > baseline_width_px {
                remaining_width_px + desired_gap_px
            } else {
                label_offset
            };

            if player_side == profile::PlayerSide::P1 {
                actors.push(act!(text: font(font_name): settext(remaining_time_str):
                    align(0.0, 0.5): horizalign(left):
                    xy(time_x, y_pos_remaining):
                    z(71):
                    diffuse(remaining_color[0], remaining_color[1], remaining_color[2], remaining_color[3])
                ));
                actors.push(act!(text: font(font_name): settext(" remaining"):
                    align(0.0, 0.5): horizalign(left):
                    xy(time_x + label_dir * label_offset_remaining, y_pos_remaining + 1.0):
                    zoom(text_zoom): z(71):
                    diffuse(remaining_color[0], remaining_color[1], remaining_color[2], remaining_color[3])
                ));
            } else {
                actors.push(act!(text: font(font_name): settext(remaining_time_str):
                    align(1.0, 0.5): horizalign(right):
                    xy(time_x, y_pos_remaining):
                    z(71):
                    diffuse(remaining_color[0], remaining_color[1], remaining_color[2], remaining_color[3])
                ));
                actors.push(act!(text: font(font_name): settext(" remaining"):
                    align(1.0, 0.5): horizalign(right):
                    xy(time_x + label_dir * label_offset_remaining, y_pos_remaining + 1.0):
                    zoom(text_zoom): z(71):
                    diffuse(remaining_color[0], remaining_color[1], remaining_color[2], remaining_color[3])
                ));
            }
        }
    }));

    // --- Peak NPS Display (as seen in Simply Love's Step Statistics) ---
    if is_wide() {
        let scaled_peak = (state.chart.max_nps as f32 * state.music_rate).max(0.0);
        let peak_nps_text = format!("Peak NPS: {:.2}", scaled_peak);

        // Positioned based on visual parity with Simply Love's Step Statistics pane
        // for Player 1, which is on the right side of the screen.
        let peak_nps_x = match player_side {
            profile::PlayerSide::P1 => screen_width() - 59.0,
            profile::PlayerSide::P2 => widescale(6.0, 130.0),
        };
        let peak_nps_y = screen_center_y() + 126.0;

        actors.push(act!(text:
            font("miso"):
            settext(peak_nps_text):
            // Pivot point is the text's right-center
            align(1.0, 0.5):
            xy(peak_nps_x, peak_nps_y):
            zoom(0.9):
            diffuse(1.0, 1.0, 1.0, 1.0):
            // Align the text content itself to the right
            horizalign(right):
            z(200)
        ));
    }

    actors
}
