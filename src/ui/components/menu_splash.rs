use crate::act;
use crate::core::space::*;
use crate::ui::actors::Actor;
use crate::ui::color;

// Splash hearts used when leaving the main menu for gameplay.
// This mirrors Simply Love's ScreenTitleMenu out.lua behavior for the Hearts style.
const AF_DECEL: f32 = 0.4;
const AF_ACCEL: f32 = 0.5;
const TOTAL_DURATION: f32 = AF_DECEL + AF_ACCEL;
const FLYCENTER_TEX: &str = "titlemenu_flycenter.png";
const FLYTOP_TEX: &str = "titlemenu_flytop.png";
const FLYBOTTOM_TEX: &str = "titlemenu_flybottom.png";

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
fn sprite_heart(
    tex: &'static str,
    x: f32,
    y: f32,
    dx: f32,
    dy: f32,
    end_zoom: f32,
    end_alpha: f32,
    tint: [f32; 4],
    z: i16,
    flip_x: bool,
) -> Actor {
    let [r, g, b, _] = tint;
    let mut actor = act!(sprite(tex.to_string()):
        align(0.5, 0.5):
        xy(x, y):
        zoom(0.0):
        diffuse(r, g, b, 0.0):
        z(z):
        linear(TOTAL_DURATION):
            addx(dx):
            addy(dy):
            zoom(end_zoom):
            alpha(end_alpha):
        sleep(0.0): zoom(0.0)
    );
    if flip_x && let Actor::Sprite { flip_x: fx, .. } = &mut actor {
        *fx = true;
    }
    actor
}

pub fn build(active_color_index: i32) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(32);
    let cx = screen_center_x();
    let cy = screen_center_y();
    let c1 = color1_rgba(active_color_index);
    let c2 = color2_rgba(active_color_index);
    // Z-layer above normal UI, but below full-screen fades if they use 1400+.
    let z_layer: i16 = 1300;

    // ---------------------- center bursts ----------------------
    // Top center pair (near logo)
    {
        let base_y = cy + 50.0;
        actors.push(sprite_heart(
            FLYCENTER_TEX,
            cx,
            base_y,
            50.0,
            -230.0,
            1.0,
            0.4,
            c2,
            z_layer,
            true, // rot180 in original
        ));
        actors.push(sprite_heart(
            FLYCENTER_TEX,
            cx,
            base_y,
            -50.0,
            -230.0,
            0.6,
            0.6,
            c1,
            z_layer,
            false, // no rot
        ));
    }
    // Bottom center pair
    {
        let base_y = cy + 380.0;
        actors.push(sprite_heart(
            FLYCENTER_TEX,
            cx,
            base_y,
            50.0,
            -170.0,
            0.6,
            0.6,
            c2,
            z_layer,
            true, // rot180
        ));
        actors.push(sprite_heart(
            FLYCENTER_TEX,
            cx,
            base_y,
            -50.0,
            -170.0,
            1.0,
            0.4,
            c1,
            z_layer,
            false, // no rot
        ));
    }
    // ---------------------- upper sprays -----------------------
    {
        let base_y = cy + 200.0;
        // Up 200: wide left/right pair
        actors.push(sprite_heart(
            FLYCENTER_TEX,
            cx,
            base_y,
            -200.0,
            -100.0,
            1.0,
            0.6,
            c1,
            z_layer,
            true, // rot180 (top left in original)
        ));
        actors.push(sprite_heart(
            FLYTOP_TEX, cx, base_y, 200.0, -100.0, 1.0, 0.4, c1, z_layer,
            false, // no rot (top right)
        ));
        // Up 250 cluster
        actors.push(sprite_heart(
            FLYTOP_TEX, cx, base_y, -200.0, -150.0, 1.5, 0.3, c2, z_layer, true, // rot180
        ));
        actors.push(sprite_heart(
            FLYTOP_TEX, cx, base_y, -200.0, -150.0, 0.8, 0.6, c1, z_layer, true, // rot180
        ));
        actors.push(sprite_heart(
            FLYTOP_TEX, cx, base_y, 200.0, -150.0, 1.5, 0.2, c1, z_layer, false, // no rot
        ));
        actors.push(sprite_heart(
            FLYTOP_TEX, cx, base_y, 200.0, -150.0, 0.8, 0.4, c2, z_layer, false, // no rot
        ));
        // Up 150, out 280
        actors.push(sprite_heart(
            FLYTOP_TEX, cx, base_y, -280.0, -50.0, 1.2, 0.6, c1, z_layer, true, // rot180
        ));
        actors.push(sprite_heart(
            FLYTOP_TEX, cx, base_y, 280.0, -50.0, 1.2, 0.4, c1, z_layer, false, // no rot
        ));
        // Up 250, out 280 (small outer pair)
        actors.push(sprite_heart(
            FLYTOP_TEX, cx, base_y, -280.0, -150.0, 0.2, 0.3, c1, z_layer, true, // rot180
        ));
        actors.push(sprite_heart(
            FLYTOP_TEX, cx, base_y, 280.0, -150.0, 0.2, 0.2, c1, z_layer, false, // no rot
        ));
    }
    // ---------------------- lower sprays -----------------------
    {
        let base_y = cy + 200.0;
        // Bottom cluster (closer in)
        actors.push(sprite_heart(
            FLYBOTTOM_TEX,
            cx,
            base_y,
            -200.0,
            -100.0,
            1.0,
            0.3,
            c1,
            z_layer,
            true, // rot180 (bottom left)
        ));
        actors.push(sprite_heart(
            FLYBOTTOM_TEX,
            cx,
            base_y,
            200.0,
            -100.0,
            1.0,
            0.2,
            c1,
            z_layer,
            false, // no rot (bottom right)
        ));
        // Bottom 250 cluster
        actors.push(sprite_heart(
            FLYBOTTOM_TEX,
            cx,
            base_y,
            -200.0,
            -150.0,
            1.5,
            0.6,
            c2,
            z_layer,
            true, // rot180
        ));
        actors.push(sprite_heart(
            FLYBOTTOM_TEX,
            cx,
            base_y,
            -200.0,
            -150.0,
            0.8,
            0.3,
            c1,
            z_layer,
            true, // rot180
        ));
        actors.push(sprite_heart(
            FLYBOTTOM_TEX,
            cx,
            base_y,
            200.0,
            -150.0,
            1.5,
            0.4,
            c1,
            z_layer,
            false, // no rot
        ));
        actors.push(sprite_heart(
            FLYBOTTOM_TEX,
            cx,
            base_y,
            200.0,
            -150.0,
            0.8,
            0.2,
            c2,
            z_layer,
            false, // no rot
        ));
        // Bottom 150, out 280
        actors.push(sprite_heart(
            FLYBOTTOM_TEX,
            cx,
            base_y,
            -280.0,
            -50.0,
            1.2,
            0.3,
            c1,
            z_layer,
            true, // rot180
        ));
        actors.push(sprite_heart(
            FLYBOTTOM_TEX,
            cx,
            base_y,
            280.0,
            -50.0,
            1.2,
            0.2,
            c1,
            z_layer,
            false, // no rot
        ));
        // Bottom 250, out 280 (small outer pair)
        actors.push(sprite_heart(
            FLYBOTTOM_TEX,
            cx,
            base_y,
            -280.0,
            -150.0,
            0.2,
            0.3,
            c1,
            z_layer,
            true, // rot180
        ));
        actors.push(sprite_heart(
            FLYBOTTOM_TEX,
            cx,
            base_y,
            280.0,
            -150.0,
            0.2,
            0.2,
            c1,
            z_layer,
            false, // no rot
        ));
    }
    actors
}
