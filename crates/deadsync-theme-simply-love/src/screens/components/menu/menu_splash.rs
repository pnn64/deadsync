use crate::act;
use crate::assets::visual_styles;
use deadlib_present::actors::Actor;
use deadlib_present::color;
use deadlib_present::space::{screen_center_x, screen_center_y};

// Visual-style splash used when leaving the main menu.
// This mirrors Simply Love's ScreenTitleMenu out.lua behavior.
const AF_DECEL: f32 = 0.4;
const AF_ACCEL: f32 = 0.5;
const IMG_ACCEL: f32 = 0.8;

#[derive(Clone, Copy, Debug, PartialEq)]
struct SplashState {
    dx: f32,
    dy: f32,
    zoom: f32,
    alpha: f32,
}

fn accelerate_p(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t
}

fn decelerate_p(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t)
}

fn splash_state(
    elapsed: f32,
    end_x: f32,
    first_y: f32,
    second_y: f32,
    end_zoom: f32,
    end_alpha: f32,
) -> SplashState {
    let elapsed = elapsed.max(0.0);
    let image_p = accelerate_p(elapsed / IMG_ACCEL);
    let (dy, parent_alpha) = if elapsed <= AF_DECEL {
        (first_y * decelerate_p(elapsed / AF_DECEL), 1.0)
    } else {
        let parent_p = accelerate_p((elapsed - AF_DECEL) / AF_ACCEL);
        (first_y + second_y * parent_p, 1.0 - parent_p)
    };

    SplashState {
        dx: end_x * image_p,
        dy,
        // Simply Love queues `sleep(0):zoom(0)` immediately after the
        // concurrent image tween completes.
        zoom: if elapsed < IMG_ACCEL {
            end_zoom * image_p
        } else {
            0.0
        },
        alpha: end_alpha * image_p * parent_alpha,
    }
}

#[inline(always)]
fn color1_rgba(active_color_index: i32) -> [f32; 4] {
    // GetHexColor(SL.Global.ActiveColorIndex-2, true)
    color::decorative_rgba(active_color_index - 2)
}

#[inline(always)]
fn color2_rgba(active_color_index: i32) -> [f32; 4] {
    // GetHexColor(SL.Global.ActiveColorIndex-1, true)
    color::decorative_rgba(active_color_index - 1)
}

#[inline(always)]
fn sprite_splash(
    tex: &'static str,
    x: f32,
    y: f32,
    end_x: f32,
    first_y: f32,
    second_y: f32,
    end_zoom: f32,
    end_alpha: f32,
    tint: [f32; 4],
    z: i16,
    flip_x: bool,
    elapsed: f32,
) -> Actor {
    let [r, g, b, _] = tint;
    let zoom_scale = visual_styles::effect_zoom_scale(tex);
    let state = splash_state(elapsed, end_x, first_y, second_y, end_zoom, end_alpha);
    let mut actor = act!(sprite(tex):
        align(0.5, 0.5):
        xy(x + state.dx, y + state.dy):
        zoom(state.zoom * zoom_scale):
        diffuse(r, g, b, state.alpha):
        z(z)
    );
    if flip_x && let Actor::Sprite { flip_x: fx, .. } = &mut actor {
        *fx = true;
    }
    actor
}

pub fn build(
    active_color_index: i32,
    effects: &crate::visual_styles::EffectAssets,
    elapsed: f32,
) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(32);
    let cx = screen_center_x();
    let cy = screen_center_y();
    let c1 = color1_rgba(active_color_index);
    let c2 = color2_rgba(active_color_index);
    let flycenter_tex = effects.titlemenu_flycenter;
    let flytop_tex = effects.titlemenu_flytop;
    let flybottom_tex = effects.titlemenu_flybottom;
    // Z-layer above normal UI, but below full-screen fades if they use 1400+.
    let z_layer: i16 = 1300;

    // ---------------------- center bursts ----------------------
    // Top center pair (near logo)
    {
        let base_y = cy + 50.0;
        actors.push(sprite_splash(
            flycenter_tex,
            cx,
            base_y,
            50.0,
            -250.0,
            20.0,
            1.0,
            0.4,
            c2,
            z_layer,
            true, // rot180 in original
            elapsed,
        ));
        actors.push(sprite_splash(
            flycenter_tex,
            cx,
            base_y,
            -50.0,
            -250.0,
            20.0,
            0.6,
            0.6,
            c1,
            z_layer,
            false, // no rot
            elapsed,
        ));
    }
    // Bottom center pair
    {
        let base_y = cy + 380.0;
        actors.push(sprite_splash(
            flycenter_tex,
            cx,
            base_y,
            50.0,
            -250.0,
            80.0,
            0.6,
            0.6,
            c2,
            z_layer,
            true, // rot180
            elapsed,
        ));
        actors.push(sprite_splash(
            flycenter_tex,
            cx,
            base_y,
            -50.0,
            -250.0,
            80.0,
            1.0,
            0.4,
            c1,
            z_layer,
            false, // no rot
            elapsed,
        ));
    }
    // ---------------------- upper sprays -----------------------
    {
        let base_y = cy + 200.0;
        // Up 200: wide left/right pair
        actors.push(sprite_splash(
            flycenter_tex,
            cx,
            base_y,
            -200.0,
            -200.0,
            100.0,
            1.0,
            0.6,
            c1,
            z_layer,
            true, // rot180 (top left in original)
            elapsed,
        ));
        actors.push(sprite_splash(
            flytop_tex, cx, base_y, 200.0, -200.0, 100.0, 1.0, 0.4, c1, z_layer,
            false, // no rot (top right)
            elapsed,
        ));
        // Up 250 cluster
        actors.push(sprite_splash(
            flytop_tex, cx, base_y, -200.0, -250.0, 100.0, 1.5, 0.3, c2, z_layer, true,
            elapsed, // rot180
        ));
        actors.push(sprite_splash(
            flytop_tex, cx, base_y, -200.0, -250.0, 100.0, 0.8, 0.6, c1, z_layer, true,
            elapsed, // rot180
        ));
        actors.push(sprite_splash(
            flytop_tex, cx, base_y, 200.0, -250.0, 100.0, 1.5, 0.2, c1, z_layer, false,
            elapsed, // no rot
        ));
        actors.push(sprite_splash(
            flytop_tex, cx, base_y, 200.0, -250.0, 100.0, 0.8, 0.4, c2, z_layer, false,
            elapsed, // no rot
        ));
        // Up 150, out 280
        actors.push(sprite_splash(
            flytop_tex, cx, base_y, -280.0, -150.0, 100.0, 1.2, 0.6, c1, z_layer, true,
            elapsed, // rot180
        ));
        actors.push(sprite_splash(
            flytop_tex, cx, base_y, 280.0, -150.0, 100.0, 1.2, 0.4, c1, z_layer, false,
            elapsed, // no rot
        ));
        // Up 250, out 280 (small outer pair)
        actors.push(sprite_splash(
            flytop_tex, cx, base_y, -280.0, -250.0, 100.0, 0.2, 0.3, c1, z_layer, true,
            elapsed, // rot180
        ));
        actors.push(sprite_splash(
            flytop_tex, cx, base_y, 280.0, -250.0, 100.0, 0.2, 0.2, c1, z_layer, false,
            elapsed, // no rot
        ));
    }
    // ---------------------- lower sprays -----------------------
    {
        let base_y = cy + 200.0;
        // Bottom cluster (closer in)
        actors.push(sprite_splash(
            flybottom_tex,
            cx,
            base_y,
            -200.0,
            -200.0,
            100.0,
            1.0,
            0.3,
            c1,
            z_layer,
            true, // rot180 (bottom left)
            elapsed,
        ));
        actors.push(sprite_splash(
            flybottom_tex,
            cx,
            base_y,
            200.0,
            -200.0,
            100.0,
            1.0,
            0.2,
            c1,
            z_layer,
            false, // no rot (bottom right)
            elapsed,
        ));
        // Bottom 250 cluster
        actors.push(sprite_splash(
            flybottom_tex,
            cx,
            base_y,
            -200.0,
            -250.0,
            100.0,
            1.5,
            0.6,
            c2,
            z_layer,
            true, // rot180
            elapsed,
        ));
        actors.push(sprite_splash(
            flybottom_tex,
            cx,
            base_y,
            -200.0,
            -250.0,
            100.0,
            0.8,
            0.3,
            c1,
            z_layer,
            true, // rot180
            elapsed,
        ));
        actors.push(sprite_splash(
            flybottom_tex,
            cx,
            base_y,
            200.0,
            -250.0,
            100.0,
            1.5,
            0.4,
            c1,
            z_layer,
            false, // no rot
            elapsed,
        ));
        actors.push(sprite_splash(
            flybottom_tex,
            cx,
            base_y,
            200.0,
            -250.0,
            100.0,
            0.8,
            0.2,
            c2,
            z_layer,
            false, // no rot
            elapsed,
        ));
        // Bottom 150, out 280
        actors.push(sprite_splash(
            flybottom_tex,
            cx,
            base_y,
            -280.0,
            -150.0,
            100.0,
            1.2,
            0.3,
            c1,
            z_layer,
            true, // rot180
            elapsed,
        ));
        actors.push(sprite_splash(
            flybottom_tex,
            cx,
            base_y,
            280.0,
            -150.0,
            100.0,
            1.2,
            0.2,
            c1,
            z_layer,
            false, // no rot
            elapsed,
        ));
        // Bottom 250, out 280 (small outer pair)
        actors.push(sprite_splash(
            flybottom_tex,
            cx,
            base_y,
            -280.0,
            -250.0,
            100.0,
            0.2,
            0.3,
            c1,
            z_layer,
            true, // rot180
            elapsed,
        ));
        actors.push(sprite_splash(
            flybottom_tex,
            cx,
            base_y,
            280.0,
            -250.0,
            100.0,
            0.2,
            0.2,
            c1,
            z_layer,
            false, // no rot
            elapsed,
        ));
    }
    actors
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1.0e-5,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn splash_uses_concurrent_parent_and_image_tweens() {
        let start = splash_state(0.0, 50.0, -250.0, 20.0, 1.0, 0.4);
        assert_eq!(
            start,
            SplashState {
                dx: 0.0,
                dy: 0.0,
                zoom: 0.0,
                alpha: 0.0,
            }
        );

        let turn = splash_state(0.4, 50.0, -250.0, 20.0, 1.0, 0.4);
        approx(turn.dx, 12.5);
        approx(turn.dy, -250.0);
        approx(turn.zoom, 0.25);
        approx(turn.alpha, 0.1);

        let image_end = splash_state(0.8, 50.0, -250.0, 20.0, 1.0, 0.4);
        approx(image_end.dx, 50.0);
        approx(image_end.dy, -237.2);
        approx(image_end.zoom, 0.0);
        approx(image_end.alpha, 0.144);

        let parent_end = splash_state(0.9, 50.0, -250.0, 20.0, 1.0, 0.4);
        approx(parent_end.dy, -230.0);
        approx(parent_end.zoom, 0.0);
        approx(parent_end.alpha, 0.0);
    }
}
