use crate::act;
use crate::assets::AssetManager;
use crate::core::space::widescale;
use crate::core::space::{screen_width, screen_height, screen_center_x, screen_center_y};
use crate::game::judgment;
use crate::game::profile;
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;
use crate::ui::components::{gameplay_stats, notefield};
use crate::ui::components::screen_bar::{self, AvatarParams, ScreenBarParams};

use crate::game::gameplay::{TRANSITION_IN_DURATION, TRANSITION_OUT_DURATION};
pub use crate::game::gameplay::{State, init, update};

// --- TRANSITIONS ---
pub fn in_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1100):
        linear(TRANSITION_IN_DURATION): alpha(0.0):
        linear(0.0): visible(false)
    );
    (vec![actor], TRANSITION_IN_DURATION)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.0):
        z(1200):
        linear(TRANSITION_OUT_DURATION): alpha(1.0)
    );
    (vec![actor], TRANSITION_OUT_DURATION)
}

// --- DRAWING ---

fn build_background(state: &State) -> Actor {
    let sw = screen_width();
    let sh = screen_height();
    let screen_aspect = if sh > 0.0 { sw / sh } else { 16.0 / 9.0 };

    let (tex_w, tex_h) =
        if let Some(meta) = crate::assets::texture_dims(&state.background_texture_key) {
            (meta.w as f32, meta.h as f32)
        } else {
            (1.0, 1.0) // fallback, will just fill screen
        };

    let tex_aspect = if tex_h > 0.0 { tex_w / tex_h } else { 1.0 };

    if screen_aspect > tex_aspect {
        // screen is wider, match width to cover
        act!(sprite(state.background_texture_key.clone()):
            align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
            zoomtowidth(sw):
            z(-100)
        )
    } else {
        // screen is taller/equal, match height to cover
        act!(sprite(state.background_texture_key.clone()):
            align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
            zoomtoheight(sh):
            z(-100)
        )
    }
}

pub fn get_actors(state: &State, asset_manager: &AssetManager) -> Vec<Actor> {
    let mut actors = Vec::new();
    let play_style = profile::get_session_play_style();
    let player_side = profile::get_session_player_side();
    let is_p2_single =
        play_style == profile::PlayStyle::Single && player_side == profile::PlayerSide::P2;
    let player_color = if is_p2_single {
        color::decorative_rgba(state.active_color_index - 2)
    } else {
        state.player_color
    };
    // --- Background and Filter ---
    actors.push(build_background(state));

    // Global offset adjustment overlay (centered text with subtle shadow).
    if let Some(msg) = &state.sync_overlay_message {
        let zoom = widescale(0.8, 1.0);
        let y = screen_center_y() + 120.0;

        // Main text
        actors.push(act!(text:
            font("miso"):
            settext(msg.clone()):
            align(0.5, 0.5):
            xy(screen_center_x(), y):
            zoom(zoom):
            shadowlength(2.0):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(901)
        ));
    }

    let notefield_width = |player_idx: usize| -> f32 {
        let Some(ns) = state.noteskin[player_idx].as_ref() else {
            return 256.0;
        };
        let cols = state
            .cols_per_player
            .min(ns.column_xs.len())
            .min(ns.receptor_off.len());
        if cols == 0 {
            return 256.0;
        }
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        for x in ns.column_xs.iter().take(cols) {
            let xf = *x as f32;
            min_x = min_x.min(xf);
            max_x = max_x.max(xf);
        }
        let field_zoom = state.field_zoom[player_idx].max(0.0);
        let target_arrow_px = 64.0 * field_zoom;
        let size = ns.receptor_off[0].size();
        let w = size[0].max(0) as f32;
        let h = size[1].max(0) as f32;
        let arrow_w = if h > 0.0 && target_arrow_px > 0.0 {
            w * (target_arrow_px / h)
        } else {
            w * field_zoom
        };
        ((max_x - min_x) * field_zoom) + arrow_w
    };

    let (p1_actors, p2_actors, playfield_center_x, per_player_fields): (
        Vec<Actor>,
        Option<Vec<Actor>>,
        f32,
        [(usize, f32); 2],
    ) = match play_style {
        profile::PlayStyle::Versus => {
            let (p1, p1_x) = notefield::build(
                state,
                &state.player_profiles[0],
                notefield::FieldPlacement::P1,
            );
            let (p2, p2_x) = notefield::build(
                state,
                &state.player_profiles[1],
                notefield::FieldPlacement::P2,
            );
            (p1, Some(p2), p1_x, [(0, p1_x), (1, p2_x)])
        }
        _ => {
            let placement = if is_p2_single {
                notefield::FieldPlacement::P2
            } else {
                notefield::FieldPlacement::P1
            };
            let (nf, nf_x) = notefield::build(state, &state.player_profiles[0], placement);
            (nf, None, nf_x, [(0, nf_x), (usize::MAX, 0.0)])
        }
    };

    // Background filter per-player (Simply Love parity): draw behind each notefield, not full-screen.
    for &(player_idx, field_x) in &per_player_fields {
        if player_idx == usize::MAX || player_idx >= state.num_players {
            continue;
        }
        let filter_alpha = match state.player_profiles[player_idx].background_filter {
            crate::game::profile::BackgroundFilter::Off => 0.0,
            crate::game::profile::BackgroundFilter::Dark => 0.5,
            crate::game::profile::BackgroundFilter::Darker => 0.75,
            crate::game::profile::BackgroundFilter::Darkest => 0.95,
        };
        if filter_alpha <= 0.0 {
            continue;
        }
        actors.push(act!(quad:
            align(0.5, 0.5): xy(field_x, screen_center_y()):
            zoomto(notefield_width(player_idx), screen_height()):
            diffuse(0.0, 0.0, 0.0, filter_alpha):
            z(-99)
        ));
    }

    if let Some(p2_actors) = p2_actors {
        actors.extend(p2_actors);
    }
    actors.extend(p1_actors);
    let clamped_width = screen_width().clamp(640.0, 854.0);

    let players: &[(usize, f32, f32)] = match play_style {
        profile::PlayStyle::Versus => &[
            (
                0,
                screen_center_x() - widescale(292.5, 342.5),
                screen_center_x() - clamped_width / 4.3,
            ),
            (
                1,
                screen_center_x() + widescale(292.5, 342.5),
                screen_center_x() + clamped_width / 2.75,
            ),
        ],
        _ if is_p2_single => &[(
            0,
            screen_center_x() + widescale(292.5, 342.5),
            screen_center_x() + clamped_width / 2.75,
        )],
        _ => &[(
            0,
            screen_center_x() - widescale(292.5, 342.5),
            screen_center_x() - clamped_width / 4.3,
        )],
    };

    for &(player_idx, diff_x, score_x) in players {
        let chart = &state.charts[player_idx];
        let difficulty_color =
            color::difficulty_rgba(&chart.difficulty, state.active_color_index);
        let meter_text = chart.meter.to_string();

        // Difficulty Box
        let y = 56.0;
        actors.push(Actor::Frame {
            align: [0.5, 0.5],
            offset: [diff_x, y],
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            children: vec![
                act!(quad:
                    align(0.5, 0.5): xy(0.0, 0.0): zoomto(30.0, 30.0):
                    diffuse(difficulty_color[0], difficulty_color[1], difficulty_color[2], 1.0)
                ),
                act!(text:
                    font("wendy"): settext(meter_text.clone()): align(0.5, 0.5): xy(0.0, 0.0):
                    zoom(0.4): diffuse(0.0, 0.0, 0.0, 1.0)
                ),
            ],
            background: None,
            z: 90,
        });

        // Score Display
        let score_y = 56.0;
        let (score_text, score_color) = if state.player_profiles[player_idx].show_ex_score {
            let mines_disabled = false;
            let (start, end) = state.note_ranges[player_idx];
            let ex_percent = judgment::calculate_ex_score_from_notes(
                &state.notes[start..end],
                &state.note_time_cache[start..end],
                &state.hold_end_time_cache[start..end],
                chart.stats.total_steps,
                state.holds_total[player_idx],
                state.rolls_total[player_idx],
                state.mines_total[player_idx],
                state.players[player_idx].fail_time,
                mines_disabled,
            );
            (
                format!("{:.2}", ex_percent.max(0.0)),
                color::JUDGMENT_RGBA[0],
            )
        } else {
            let score_percent = (judgment::calculate_itg_score_percent(
                &state.players[player_idx].scoring_counts,
                state.players[player_idx].holds_held_for_score,
                state.players[player_idx].rolls_held_for_score,
                state.players[player_idx].mines_hit_for_score,
                state.possible_grade_points[player_idx],
            ) * 100.0) as f32;
            (format!("{score_percent:.2}"), [1.0, 1.0, 1.0, 1.0])
        };

        actors.push(act!(text:
            font("wendy_monospace_numbers"): settext(score_text):
            align(1.0, 1.0): xy(score_x, score_y):
            zoom(0.5): horizalign(right):
            diffuse(score_color[0], score_color[1], score_color[2], score_color[3]):
            z(90)
        ));
    }
    // Current BPM Display (1:1 with Simply Love)
    {
        let base_bpm = state.timing.get_bpm_for_beat(state.current_beat);
        let display_bpm = if base_bpm.is_finite() {
            (base_bpm
                * if state.music_rate.is_finite() {
                    state.music_rate
                } else {
                    1.0
                })
            .round() as i32
        } else {
            0
        };
        let bpm_text = display_bpm.to_string();
        // Final world-space positions derived from analyzing the SM Lua transforms.
        // The parent frame is bottom-aligned to y=52, and its children are positioned
        // relative to that y-coordinate, with a zoom of 1.33 applied to the whole group.
        let frame_origin_y = 51.0;
        let frame_zoom = 1.33;
        // The BPM text is at y=0 relative to the frame's origin. Its final position is just the origin.
        let bpm_center_y = frame_origin_y;
        // The Rate text is at y=12 relative to the frame's origin. Its offset is scaled by the frame's zoom.
        let rate_center_y = 12.0f64.mul_add(frame_zoom, frame_origin_y);
        let bpm_final_zoom = 1.0 * frame_zoom;
        let rate_final_zoom = 0.5 * frame_zoom;
        let bpm_x = screen_center_x();
        actors.push(act!(text:
            font("miso"): settext(bpm_text):
            align(0.5, 0.5): xy(bpm_x, bpm_center_y):
            zoom(bpm_final_zoom): horizalign(center): z(90)
        ));
        let rate = if state.music_rate.is_finite() {
            state.music_rate
        } else {
            1.0
        };
        let rate_text = if (rate - 1.0).abs() > 0.001 {
            format!("{rate:.2}x rate")
        } else {
            String::new()
        };
        actors.push(act!(text:
            font("miso"): settext(rate_text):
            align(0.5, 0.5): xy(bpm_x, rate_center_y):
            zoom(rate_final_zoom): horizalign(center): z(90)
        ));
    }
    // Song Title Box (SongMeter)
    {
        let w = widescale(310.0, 417.0);
        let h = 22.0;
        let box_cx = screen_center_x();
        let box_cy = 20.0;
        let mut frame_children = Vec::new();
        frame_children.push(act!(quad: align(0.5, 0.5): xy(w / 2.0, h / 2.0): zoomto(w, h): diffuse(1.0, 1.0, 1.0, 1.0): z(0) ));
        frame_children.push(act!(quad: align(0.5, 0.5): xy(w / 2.0, h / 2.0): zoomto(w - 4.0, h - 4.0): diffuse(0.0, 0.0, 0.0, 1.0): z(1) ));
        if state.song.total_length_seconds > 0 && state.current_music_time >= 0.0 {
            let progress =
                (state.current_music_time / state.song.total_length_seconds as f32).clamp(0.0, 1.0);
            frame_children.push(act!(quad:
                align(0.0, 0.5): xy(2.0, h / 2.0): zoomto((w - 4.0) * progress, h - 4.0):
                diffuse(player_color[0], player_color[1], player_color[2], 1.0): z(2)
            ));
        }
        let full_title = state.song_full_title.clone();
        frame_children.push(act!(text:
            font("miso"): settext(full_title): align(0.5, 0.5): xy(w / 2.0, h / 2.0):
            zoom(0.8): maxwidth(screen_width() / 2.5 - 10.0): horizalign(center): z(3)
        ));
        actors.push(Actor::Frame {
            align: [0.5, 0.5],
            offset: [box_cx, box_cy],
            size: [SizeSpec::Px(w), SizeSpec::Px(h)],
            background: None,
            z: 90,
            children: frame_children,
        });
    }
    // --- Life Meter ---
    {
        let w = 136.0;
        let h = 18.0;
        let meter_cy = 20.0;

        let life_players: &[(usize, f32)] = match play_style {
            profile::PlayStyle::Versus => &[
                (0, screen_center_x() - widescale(238.0, 288.0)),
                (1, screen_center_x() + widescale(238.0, 288.0)),
            ],
            _ if is_p2_single => &[(0, screen_center_x() + widescale(238.0, 288.0))],
            _ => &[(0, screen_center_x() - widescale(238.0, 288.0))],
        };

        for &(player_idx, meter_cx) in life_players {
            // Frames/border
            actors.push(act!(quad: align(0.5, 0.5): xy(meter_cx, meter_cy): zoomto(w + 4.0, h + 4.0): diffuse(1.0, 1.0, 1.0, 1.0): z(90) ));
            actors.push(act!(quad: align(0.5, 0.5): xy(meter_cx, meter_cy): zoomto(w, h): diffuse(0.0, 0.0, 0.0, 1.0): z(91) ));

            // Latch-to-zero for rendering the very frame we die.
            let dead = state.players[player_idx].is_failing || state.players[player_idx].life <= 0.0;
            let life_for_render = if dead {
                0.0
            } else {
                state.players[player_idx].life.clamp(0.0, 1.0)
            };
            let is_hot = !dead && life_for_render >= 1.0;
            let life_color = if is_hot {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                player_color
            };
            let filled_width = w * life_for_render;
            // Never draw swoosh if dead OR nothing to fill.
            if filled_width > 0.0 && !dead {
                // Logic Parity:
                // velocity = -(songposition:GetCurBPS() * 0.5)
                // if songposition:GetFreeze() or songposition:GetDelay() then velocity = 0 end
                let bps = state.timing.get_bpm_for_beat(state.current_beat) / 60.0;
                let velocity_x = if state.is_in_freeze || state.is_in_delay {
                    0.0
                } else {
                    -(bps * 0.5)
                };

                let swoosh_alpha = if is_hot { 1.0 } else { 0.2 };

                // MeterSwoosh
                actors.push(act!(sprite("swoosh.png"):
                    align(0.0, 0.5):
                    xy(meter_cx - w / 2.0, meter_cy):
                    zoomto(filled_width, h):
                    diffusealpha(swoosh_alpha):
                    // Apply the calculated velocity
                    texcoordvelocity(velocity_x, 0.0):
                    z(93)
                ));

                // MeterFill
                actors.push(act!(quad:
                    align(0.0, 0.5):
                    xy(meter_cx - w / 2.0, meter_cy):
                    zoomto(filled_width, h):
                    diffuse(life_color[0], life_color[1], life_color[2], 1.0):
                    z(92)
                ));
            }
        }
    }
    let p1_profile = profile::get_for_side(profile::PlayerSide::P1);
    let p2_profile = profile::get_for_side(profile::PlayerSide::P2);
    let p1_avatar = p1_profile
        .avatar_texture_key
        .as_deref()
        .map(|texture_key| AvatarParams { texture_key });
    let p2_avatar = p2_profile
        .avatar_texture_key
        .as_deref()
        .map(|texture_key| AvatarParams { texture_key });

    let p1_joined = profile::is_session_side_joined(profile::PlayerSide::P1);
    let p2_joined = profile::is_session_side_joined(profile::PlayerSide::P2);
    let p1_guest = profile::is_session_side_guest(profile::PlayerSide::P1);
    let p2_guest = profile::is_session_side_guest(profile::PlayerSide::P2);

    let (p1_footer_text, p1_footer_avatar) = if p1_joined {
        (
            Some(if p1_guest {
                "INSERT CARD"
            } else {
                p1_profile.display_name.as_str()
            }),
            if p1_guest { None } else { p1_avatar },
        )
    } else {
        (None, None)
    };
    let (p2_footer_text, p2_footer_avatar) = if p2_joined {
        (
            Some(if p2_guest {
                "INSERT CARD"
            } else {
                p2_profile.display_name.as_str()
            }),
            if p2_guest { None } else { p2_avatar },
        )
    } else {
        (None, None)
    };

    let (footer_left, footer_right, left_avatar, right_avatar) =
        if play_style == profile::PlayStyle::Versus {
            (p1_footer_text, p2_footer_text, p1_footer_avatar, p2_footer_avatar)
        } else {
            match player_side {
                profile::PlayerSide::P1 => (p1_footer_text, None, p1_footer_avatar, None),
                profile::PlayerSide::P2 => (None, p2_footer_text, None, p2_footer_avatar),
            }
        };
    actors.push(screen_bar::build(ScreenBarParams {
        title: "",
        title_placement: screen_bar::ScreenBarTitlePlacement::Center,
        position: screen_bar::ScreenBarPosition::Bottom,
        transparent: true,
        fg_color: [1.0; 4],
        left_text: footer_left,
        center_text: None,
        right_text: footer_right,
        left_avatar,
        right_avatar,
    }));
    if state.num_cols <= 4 && play_style != profile::PlayStyle::Versus {
        actors.extend(gameplay_stats::build(
            state,
            asset_manager,
            playfield_center_x,
            player_side,
        ));
    } else if play_style == profile::PlayStyle::Versus {
        actors.extend(gameplay_stats::build_versus_step_stats(state, asset_manager));
    } else if play_style == profile::PlayStyle::Double {
        actors.extend(gameplay_stats::build_double_step_stats(
            state,
            asset_manager,
            playfield_center_x,
        ));
    }
    actors
}
