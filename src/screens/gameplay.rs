use crate::act;
use crate::assets::AssetManager;
use crate::core::space::widescale;
use crate::core::space::*;
use crate::game::judgment;
use crate::game::profile;
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;
use crate::ui::components::{gameplay_stats, notefield};
use crate::ui::components::screen_bar::{self, ScreenBarParams};

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
    let profile = profile::get();
    // --- Background and Filter ---
    actors.push(build_background(state));
    let filter_alpha = match profile.background_filter {
        crate::game::profile::BackgroundFilter::Off => 0.0,
        crate::game::profile::BackgroundFilter::Dark => 0.5,
        crate::game::profile::BackgroundFilter::Darker => 0.75,
        crate::game::profile::BackgroundFilter::Darkest => 0.95,
    };
    if filter_alpha > 0.0 {
        actors.push(act!(quad:
            align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
            zoomto(screen_width(), screen_height()):
            diffuse(0.0, 0.0, 0.0, filter_alpha):
            z(-99) // Draw just above the background
        ));
    }

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

    let (notefield_actors, playfield_center_x) = notefield::build(state, &profile);
    actors.extend(notefield_actors);
    // Difficulty Box
    let x = screen_center_x() - widescale(292.5, 342.5);
    let y = 56.0;
    let difficulty_color = color::difficulty_rgba(&state.chart.difficulty, state.active_color_index);
    let meter_text = state.chart.meter.to_string();
    actors.push(Actor::Frame {
        align: [0.5, 0.5],
        offset: [x, y],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        children: vec![
            act!(quad:
                align(0.5, 0.5): xy(0.0, 0.0): zoomto(30.0, 30.0):
                diffuse(difficulty_color[0], difficulty_color[1], difficulty_color[2], 1.0)
            ),
            act!(text:
                font("wendy"): settext(meter_text): align(0.5, 0.5): xy(0.0, 0.0):
                zoom(0.4): diffuse(0.0, 0.0, 0.0, 1.0)
            ),
        ],
        background: None,
        z: 90,
    });
    // Score Display (P1)
    let clamped_width = screen_width().clamp(640.0, 854.0);
    let score_x = screen_center_x() - clamped_width / 4.3;
    let score_y = 56.0;
    let (score_text, score_color) = if profile.show_ex_score {
        // FA+ EX score display (Simply Love EX scoring semantics), with
        // failure-aware gating so score stops changing after life reaches 0.
        let mines_disabled = false; // NoMines handling not wired yet.
        let ex_percent = judgment::calculate_ex_score_from_notes(
            &state.notes,
            &state.note_time_cache,
            &state.hold_end_time_cache,
            state.chart.stats.total_steps, // <- use this
            state.holds_total,
            state.rolls_total,
            state.mines_total,
            state.fail_time,
            mines_disabled,
        );
        let text = format!("{:.2}", ex_percent.max(0.0));
        let color = color::rgba_hex(color::JUDGMENT_HEX[0]); // Fantastic blue (#21CCE8)
        (text, color)
    } else {
        let score_percent = (judgment::calculate_itg_score_percent(
            &state.scoring_counts,
            state.holds_held_for_score,
            state.rolls_held_for_score,
            state.mines_hit_for_score,
            state.possible_grade_points,
        ) * 100.0) as f32;
        let text = format!("{:.2}", score_percent);
        let color = [1.0, 1.0, 1.0, 1.0];
        (text, color)
    };
    actors.push(act!(text:
        font("wendy_monospace_numbers"): settext(score_text):
        align(1.0, 1.0): xy(score_x, score_y):
        zoom(0.5): horizalign(right):
        diffuse(score_color[0], score_color[1], score_color[2], score_color[3]):
        z(90)
    ));
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
        let rate_center_y = frame_origin_y + (12.0 * frame_zoom);
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
                diffuse(state.player_color[0], state.player_color[1], state.player_color[2], 1.0): z(2)
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
    // --- Life Meter (P1) ---
    {
        let w = 136.0;
        let h = 18.0;
        let meter_cx = screen_center_x() - widescale(238.0, 288.0);
        let meter_cy = 20.0;
        // Frames/border
        actors.push(act!(quad: align(0.5, 0.5): xy(meter_cx, meter_cy): zoomto(w + 4.0, h + 4.0): diffuse(1.0, 1.0, 1.0, 1.0): z(90) ));
        actors.push(act!(quad: align(0.5, 0.5): xy(meter_cx, meter_cy): zoomto(w, h): diffuse(0.0, 0.0, 0.0, 1.0): z(91) ));
        // Latch-to-zero for rendering the very frame we die.
        let dead = state.is_failing || state.life <= 0.0;
        let life_for_render = if dead {
            0.0
        } else {
            state.life.clamp(0.0, 1.0)
        };
        let is_hot = !dead && life_for_render >= 1.0;
        let life_color = if is_hot {
            [1.0, 1.0, 1.0, 1.0]
        } else {
            state.player_color
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
    actors.push(screen_bar::build(ScreenBarParams {
        title: "",
        title_placement: screen_bar::ScreenBarTitlePlacement::Center,
        position: screen_bar::ScreenBarPosition::Bottom,
        transparent: true,
        fg_color: [1.0; 4],
        left_text: Some(&profile.display_name),
        center_text: None,
        right_text: None,
        left_avatar: None,
    }));
    if state.num_cols <= 4 {
        actors.extend(gameplay_stats::build(state, asset_manager, playfield_center_x));
    }
    actors
}
