use crate::act;
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use crate::game::parsing::noteskin;
use crate::screens::components::shared::noteskin_model::noteskin_model_actor_from_draw_depth_sorted_affine;
use glam::{Mat4 as Matrix4, Vec3 as Vector3};
use std::sync::{Arc, OnceLock};

const SQUARE_TEX: &str = "graphics/menu_bg_technique/square.png";
const CIRCLE_FRAG_PATH: &str = "assets/graphics/menu_bg_technique/circlefrag_model.txt";
const RING_PATH: &str = "assets/graphics/menu_bg_technique/ring_model.txt";
const ARROW_PATH: &str = "assets/graphics/menu_bg_technique/arrow_model.txt";

const FRONT_COLOR_ADD: [f32; 10] = [-0.75, 0.0, 0.0, -0.75, -0.75, -0.75, 0.0, -0.75, 0.0, -0.75];
const GRID_VELOCITY: [[f32; 2]; 3] = [[0.05, 0.07], [0.04, 0.02], [0.02, 0.015]];
const GRID_ALPHA: [f32; 3] = [0.1, 0.05, 0.025];
const GRID_RECT_SPAN: f32 = 60.0;
const GRID_ZOOM: f32 = 20.0;
const BACKDROP_RGBA: [f32; 4] = [20.0 / 255.0, 20.0 / 255.0, 20.0 / 255.0, 1.0];
const MODEL_Z: i16 = -96;

#[derive(Clone)]
struct TechniqueLayer {
    slot: noteskin::SpriteSlot,
    size: [f32; 2],
}

#[derive(Clone)]
struct TechniqueAssets {
    circle_frag: Arc<[TechniqueLayer]>,
    ring: Arc<[TechniqueLayer]>,
    arrow: Arc<[TechniqueLayer]>,
}

#[derive(Clone, Copy, Default)]
pub(super) struct State;

impl State {
    pub(super) const fn new() -> Self {
        Self
    }

    pub(super) fn build_at_elapsed(
        &self,
        active_color_index: i32,
        backdrop_rgba: [f32; 4],
        alpha_mul: f32,
        elapsed_s: f32,
    ) -> Option<Vec<Actor>> {
        let assets = technique_assets()?;
        let center = [screen_center_x(), screen_center_y()];
        let mut actors = Vec::with_capacity(40);
        actors.push(act!(quad:
            align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(screen_width(), screen_height()):
            diffuse(backdrop_rgba[0], backdrop_rgba[1], backdrop_rgba[2], backdrop_rgba[3]):
            z(-100)
        ));
        actors.push(act!(quad:
            align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(screen_width(), screen_height()):
            diffuse(BACKDROP_RGBA[0], BACKDROP_RGBA[1], BACKDROP_RGBA[2], BACKDROP_RGBA[3] * alpha_mul):
            z(-99)
        ));

        for i in 0..GRID_VELOCITY.len() {
            let uv = wrapped_grid_uv_rect(GRID_VELOCITY[i], elapsed_s);
            actors.push(act!(sprite(SQUARE_TEX):
                align(0.5, 0.5):
                xy(center[0], center[1]):
                zoom(GRID_ZOOM):
                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                diffuse(1.0, 1.0, 1.0, GRID_ALPHA[i] * alpha_mul):
                z(-98)
            ));
        }

        let mut model_actors = Vec::with_capacity(21);

        for i in 1..=10 {
            let fi = i as f64;
            let zoom = random_xd(fi * 1.6) + 0.35;
            let z_pos = (random_xd(fi * 13.0) - 0.6) * (1.0 / zoom) * 850.0;
            let rot_z = random_xd(fi) * 400.0 + random_xd(fi * 3.4) * 14.0 * elapsed_s;
            let mut color = technique_front_color(active_color_index, FRONT_COLOR_ADD[i - 1]);
            color[3] = random_xd(fi) * alpha_mul;
            push_layers(
                &mut model_actors,
                &assets.circle_frag,
                center,
                zoom,
                [-60.0, 20.0, rot_z],
                z_pos,
                color,
                elapsed_s,
            );
        }

        push_layers(
            &mut model_actors,
            &assets.ring,
            center,
            1.75,
            [-60.0, 20.0, 50.0 + 10.0 * (elapsed_s + 20.0)],
            0.0,
            [1.0, 1.0, 1.0, 0.8 * alpha_mul],
            elapsed_s,
        );
        push_layers(
            &mut model_actors,
            &assets.ring,
            center,
            0.75,
            [-60.0, 20.0, 50.0 + 4.0 * (elapsed_s + 20.0)],
            0.0,
            [1.0, 1.0, 1.0, 0.8 * alpha_mul],
            elapsed_s,
        );
        push_layers(
            &mut model_actors,
            &assets.arrow,
            center,
            1.2,
            [0.0, 10.0 * elapsed_s, 20.0],
            0.0,
            scale_alpha(color::decorative_rgba(active_color_index), 0.7 * alpha_mul),
            elapsed_s,
        );

        for i in 11..=18 {
            let fi = i as f64;
            let zoom = random_xd(fi * 2.8) + 0.35;
            let z_pos = (random_xd(fi * 13.0) - 0.6) * (2.0 / zoom) * 850.0;
            let rot_z = random_xd(fi) * 2000.0
                + random_xd(fi * 3.6) * 14.0 * (elapsed_s + i as f32 * 2000.0);
            let color = [1.0, 1.0, 1.0, random_xd(fi / 1.6) * alpha_mul];
            push_layers(
                &mut model_actors,
                &assets.circle_frag,
                center,
                zoom,
                [-60.0, 20.0, rot_z],
                z_pos,
                color,
                elapsed_s,
            );
        }

        // Simply Love's Technique.lua appends these models in a fixed ActorFrame
        // order, and ITGmania's default ActorFrame draw path preserves that order.
        // Do not depth-sort or z-buffer them here; that changes which ring family
        // sits on top and breaks parity with the theme.
        actors.push(Actor::Camera {
            view_proj: technique_view_proj(),
            children: model_actors,
        });

        Some(actors)
    }
}

fn technique_assets() -> Option<Arc<TechniqueAssets>> {
    static ASSETS: OnceLock<Option<Arc<TechniqueAssets>>> = OnceLock::new();
    ASSETS
        .get_or_init(|| match load_assets() {
            Ok(assets) => Some(Arc::new(assets)),
            Err(err) => {
                log::warn!("Failed to load technique background assets: {err}");
                None
            }
        })
        .clone()
}

fn load_assets() -> Result<TechniqueAssets, String> {
    Ok(TechniqueAssets {
        circle_frag: load_layers(CIRCLE_FRAG_PATH)?,
        ring: load_layers(RING_PATH)?,
        arrow: load_layers(ARROW_PATH)?,
    })
}

fn load_layers(path: &str) -> Result<Arc<[TechniqueLayer]>, String> {
    let slots = noteskin::load_itg_model_slots_from_path(std::path::Path::new(path))?;
    let layers = slots
        .iter()
        .cloned()
        .map(|slot| {
            let size = slot
                .model
                .as_ref()
                .map(|model| model.size())
                .unwrap_or([1.0, 1.0]);
            TechniqueLayer { slot, size }
        })
        .collect::<Vec<_>>();
    Ok(Arc::from(layers))
}

fn push_layers(
    out: &mut Vec<Actor>,
    layers: &[TechniqueLayer],
    center: [f32; 2],
    zoom: f32,
    base_rot: [f32; 3],
    z_pos: f32,
    color: [f32; 4],
    elapsed_s: f32,
) {
    for layer in layers {
        let mut draw = layer.slot.model_draw_at(elapsed_s, 0.0);
        draw.rot[0] += base_rot[0];
        draw.rot[1] += base_rot[1];
        draw.rot[2] += base_rot[2];
        draw.pos[2] += z_pos;
        let size = [layer.size[0] * zoom, layer.size[1] * zoom];
        let uv = layer.slot.uv_for_frame_at(0, elapsed_s);
        if let Some(mut actor) = noteskin_model_actor_from_draw_depth_sorted_affine(
            &layer.slot,
            draw,
            center,
            size,
            uv,
            0.0,
            color,
            crate::engine::gfx::BlendMode::Alpha,
            MODEL_Z,
        ) {
            if let Actor::TexturedMesh { depth_test, .. } = &mut actor {
                *depth_test = false;
            }
            out.push(actor);
        }
    }
}

fn technique_front_color(active_color_index: i32, offset: f32) -> [f32; 4] {
    let palette_index = active_color_index as f32 + offset;
    let rounded = palette_index.round();
    if (palette_index - rounded).abs() > 0.001 {
        [1.0, 1.0, 1.0, 1.0]
    } else {
        color::decorative_rgba(rounded as i32)
    }
}

fn technique_view_proj() -> Matrix4 {
    let width = screen_width().max(1.0);
    let height = screen_height().max(1.0);
    let theta = 45.0_f32.to_radians();
    let dist = (0.5 * width / theta.tan()).max(1.0);
    let proj = Matrix4::frustum_rh_gl(
        -0.5 * width / dist,
        0.5 * width / dist,
        -0.5 * height / dist,
        0.5 * height / dist,
        1.0,
        dist + 1000.0,
    );
    // Compose places actors in a centered world space where screen center is
    // the origin, unlike StepMania's top-left actor coordinates.
    let eye = Vector3::new(0.0, 0.0, dist);
    let target = Vector3::new(0.0, 0.0, 0.0);
    proj * Matrix4::look_at_rh(eye, target, Vector3::new(0.0, 1.0, 0.0))
}

fn random_xd(t: f64) -> f32 {
    if t == 0.0 {
        0.5
    } else {
        ((t * 3229.3).sin() * 43758.5453).rem_euclid(1.0) as f32
    }
}

#[inline(always)]
fn wrapped_grid_uv_rect(velocity: [f32; 2], elapsed_s: f32) -> [f32; 4] {
    // StepMania's Sprite::Update keeps scrolling custom texture rects bounded by
    // subtracting floor() from the top-left corner each frame. Rebuild that
    // wrapped rect directly here so the repeating square layers stay numerically
    // stable and don't leak seams across the full screen.
    let u0 = (velocity[0] * elapsed_s).rem_euclid(1.0);
    let v0 = (velocity[1] * elapsed_s).rem_euclid(1.0);
    [u0, v0, u0 + GRID_RECT_SPAN, v0 + GRID_RECT_SPAN]
}

fn scale_alpha(mut color: [f32; 4], alpha: f32) -> [f32; 4] {
    color[3] *= alpha;
    color
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn technique_fractional_color_offsets_fall_back_to_white() {
        assert_eq!(technique_front_color(2, -0.75), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(technique_front_color(2, 0.0), color::decorative_rgba(2));
    }

    #[test]
    fn technique_random_matches_lua_double_precision() {
        let samples = [
            (0.0, 0.5),
            (1.0, 0.95219797),
            (1.6, 0.67487276),
            (13.0, 0.23824042),
            (26.0, 0.862378),
            (52.0, 0.5716033),
            (117.0, 0.9204272),
        ];

        for (input, expected) in samples {
            assert!((random_xd(input) - expected).abs() < 0.000001);
        }
    }
}
