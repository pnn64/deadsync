use crate::act;
use deadlib_present::actors::Actor;
use deadlib_present::color;
use deadlib_present::space::{screen_height, screen_width};
use deadlib_render::{TMeshCacheKey, TexturedMeshVertex};
use deadsync_assets::noteskin::{self, build_model_geometry};
use deadsync_notefield::noteskin_model_actor_from_draw_depth_sorted_affine_cached_geometry;
use deadsync_noteskin::{ModelDrawState, ModelEffectMode};
use glam::{Mat4 as Matrix4, Vec3 as Vector3};
use std::cell::Cell;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, OnceLock};
use twox_hash::XxHash64;

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
    vertices: Arc<[TexturedMeshVertex]>,
    geom_cache_key: TMeshCacheKey,
    static_draw: Option<ModelDrawState>,
    static_uv: Option<[f32; 4]>,
}

#[derive(Clone)]
struct TechniqueAssets {
    circle_frag: Arc<[TechniqueLayer]>,
    ring: Arc<[TechniqueLayer]>,
    arrow: Arc<[TechniqueLayer]>,
}

#[derive(Clone, Copy)]
struct CircleLayout {
    zoom: f32,
    z_pos: f32,
    rotation_base: f64,
    rotation_speed: f64,
    alpha: f32,
}

struct TechniqueLayout {
    front: [CircleLayout; 10],
    back: [CircleLayout; 8],
}

#[derive(Clone, Copy)]
struct ModelInstance {
    center: [f32; 2],
    zoom: f32,
    base_rot: [f32; 3],
    z_pos: f32,
    color: [f32; 4],
}

#[derive(Clone, Copy)]
struct ProjectionCache {
    width_bits: u32,
    height_bits: u32,
    view_proj: Matrix4,
}

#[derive(Clone, Default)]
pub(super) struct State {
    projection: Cell<Option<ProjectionCache>>,
}

impl State {
    pub(super) const fn new() -> Self {
        Self {
            projection: Cell::new(None),
        }
    }

    pub(super) fn push_at_elapsed(
        &self,
        out: &mut Vec<Actor>,
        active_color_index: i32,
        backdrop_rgba: [f32; 4],
        alpha_mul: f32,
        elapsed_s: f64,
    ) -> bool {
        let Some(assets) = technique_assets() else {
            return false;
        };
        let width = screen_width();
        let height = screen_height();
        let center = [0.5 * width, 0.5 * height];
        let model_elapsed_s = bounded_model_elapsed(elapsed_s);
        let layout = technique_layout();
        let model_actor_count =
            assets.circle_frag.len() * 18 + assets.ring.len() * 2 + assets.arrow.len();
        out.reserve(7 + model_actor_count);
        out.push(act!(quad:
            align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(width, height):
            diffuse(backdrop_rgba[0], backdrop_rgba[1], backdrop_rgba[2], backdrop_rgba[3]):
            z(-100)
        ));
        out.push(act!(quad:
            align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(width, height):
            diffuse(BACKDROP_RGBA[0], BACKDROP_RGBA[1], BACKDROP_RGBA[2], BACKDROP_RGBA[3] * alpha_mul):
            z(-99)
        ));

        for i in 0..GRID_VELOCITY.len() {
            let uv = wrapped_grid_uv_rect(GRID_VELOCITY[i], elapsed_s);
            out.push(act!(sprite(SQUARE_TEX):
                align(0.5, 0.5):
                xy(center[0], center[1]):
                zoom(GRID_ZOOM):
                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                diffuse(1.0, 1.0, 1.0, GRID_ALPHA[i] * alpha_mul):
                z(-98)
            ));
        }

        // A flat camera scope has identical compose semantics to Actor::Camera,
        // while keeping the 3D model actors in the caller's existing buffer.
        out.push(Actor::CameraPush {
            view_proj: self.view_proj(width, height),
        });

        for (i, circle) in layout.front.iter().enumerate() {
            let rot_z = rotating_degrees(circle.rotation_base, circle.rotation_speed, elapsed_s);
            let mut color = technique_front_color(active_color_index, FRONT_COLOR_ADD[i]);
            color[3] = circle.alpha * alpha_mul;
            push_layers(
                out,
                &assets.circle_frag,
                ModelInstance {
                    center,
                    zoom: circle.zoom,
                    base_rot: [-60.0, 20.0, rot_z],
                    z_pos: circle.z_pos,
                    color,
                },
                model_elapsed_s,
            );
        }

        push_layers(
            out,
            &assets.ring,
            ModelInstance {
                center,
                zoom: 1.75,
                base_rot: [-60.0, 20.0, rotating_degrees(250.0, 10.0, elapsed_s)],
                z_pos: 0.0,
                color: [1.0, 1.0, 1.0, 0.8 * alpha_mul],
            },
            model_elapsed_s,
        );
        push_layers(
            out,
            &assets.ring,
            ModelInstance {
                center,
                zoom: 0.75,
                base_rot: [-60.0, 20.0, rotating_degrees(130.0, 4.0, elapsed_s)],
                z_pos: 0.0,
                color: [1.0, 1.0, 1.0, 0.8 * alpha_mul],
            },
            model_elapsed_s,
        );
        push_layers(
            out,
            &assets.arrow,
            ModelInstance {
                center,
                zoom: 1.2,
                base_rot: [0.0, rotating_degrees(0.0, 10.0, elapsed_s), 20.0],
                z_pos: 0.0,
                color: scale_alpha(color::decorative_rgba(active_color_index), 0.7 * alpha_mul),
            },
            model_elapsed_s,
        );

        for circle in &layout.back {
            let rot_z = rotating_degrees(circle.rotation_base, circle.rotation_speed, elapsed_s);
            let color = [1.0, 1.0, 1.0, circle.alpha * alpha_mul];
            push_layers(
                out,
                &assets.circle_frag,
                ModelInstance {
                    center,
                    zoom: circle.zoom,
                    base_rot: [-60.0, 20.0, rot_z],
                    z_pos: circle.z_pos,
                    color,
                },
                model_elapsed_s,
            );
        }

        // Simply Love's Technique.lua appends these models in a fixed ActorFrame
        // order, and ITGmania's default ActorFrame draw path preserves that order.
        // Do not depth-sort or z-buffer them here; that changes which ring family
        // sits on top and breaks parity with the theme.
        out.push(Actor::CameraPop);

        true
    }

    #[cfg(any(test, feature = "bench-support"))]
    fn push_at_elapsed_legacy(
        &self,
        out: &mut Vec<Actor>,
        active_color_index: i32,
        backdrop_rgba: [f32; 4],
        alpha_mul: f32,
        elapsed_s: f64,
    ) -> bool {
        let Some(assets) = technique_assets() else {
            return false;
        };
        let width = screen_width();
        let height = screen_height();
        let center = [0.5 * width, 0.5 * height];
        out.reserve(40);
        out.push(act!(quad:
            align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(width, height):
            diffuse(backdrop_rgba[0], backdrop_rgba[1], backdrop_rgba[2], backdrop_rgba[3]):
            z(-100)
        ));
        out.push(act!(quad:
            align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(width, height):
            diffuse(BACKDROP_RGBA[0], BACKDROP_RGBA[1], BACKDROP_RGBA[2], BACKDROP_RGBA[3] * alpha_mul):
            z(-99)
        ));

        for i in 0..GRID_VELOCITY.len() {
            let uv = wrapped_grid_uv_rect(GRID_VELOCITY[i], elapsed_s);
            out.push(act!(sprite(SQUARE_TEX):
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
            let rot_z = rotating_degrees(
                random_xd(fi) as f64 * 400.0,
                random_xd(fi * 3.4) as f64 * 14.0,
                elapsed_s,
            );
            let mut color = technique_front_color(active_color_index, FRONT_COLOR_ADD[i - 1]);
            color[3] = random_xd(fi) * alpha_mul;
            push_layers_legacy(
                &mut model_actors,
                &assets.circle_frag,
                ModelInstance {
                    center,
                    zoom,
                    base_rot: [-60.0, 20.0, rot_z],
                    z_pos,
                    color,
                },
                elapsed_s,
            );
        }

        push_layers_legacy(
            &mut model_actors,
            &assets.ring,
            ModelInstance {
                center,
                zoom: 1.75,
                base_rot: [-60.0, 20.0, rotating_degrees(250.0, 10.0, elapsed_s)],
                z_pos: 0.0,
                color: [1.0, 1.0, 1.0, 0.8 * alpha_mul],
            },
            elapsed_s,
        );
        push_layers_legacy(
            &mut model_actors,
            &assets.ring,
            ModelInstance {
                center,
                zoom: 0.75,
                base_rot: [-60.0, 20.0, rotating_degrees(130.0, 4.0, elapsed_s)],
                z_pos: 0.0,
                color: [1.0, 1.0, 1.0, 0.8 * alpha_mul],
            },
            elapsed_s,
        );
        push_layers_legacy(
            &mut model_actors,
            &assets.arrow,
            ModelInstance {
                center,
                zoom: 1.2,
                base_rot: [0.0, rotating_degrees(0.0, 10.0, elapsed_s), 20.0],
                z_pos: 0.0,
                color: scale_alpha(color::decorative_rgba(active_color_index), 0.7 * alpha_mul),
            },
            elapsed_s,
        );

        for i in 11..=18 {
            let fi = i as f64;
            let zoom = random_xd(fi * 2.8) + 0.35;
            let z_pos = (random_xd(fi * 13.0) - 0.6) * (2.0 / zoom) * 850.0;
            let rot_z = rotating_degrees(
                random_xd(fi) as f64 * 2000.0
                    + random_xd(fi * 3.6) as f64 * 14.0 * i as f64 * 2000.0,
                random_xd(fi * 3.6) as f64 * 14.0,
                elapsed_s,
            );
            let color = [1.0, 1.0, 1.0, random_xd(fi / 1.6) * alpha_mul];
            push_layers_legacy(
                &mut model_actors,
                &assets.circle_frag,
                ModelInstance {
                    center,
                    zoom,
                    base_rot: [-60.0, 20.0, rot_z],
                    z_pos,
                    color,
                },
                elapsed_s,
            );
        }

        out.push(Actor::Camera {
            view_proj: technique_view_proj(width, height),
            children: model_actors,
        });
        true
    }

    #[inline]
    fn view_proj(&self, width: f32, height: f32) -> Matrix4 {
        let width_bits = width.to_bits();
        let height_bits = height.to_bits();
        if let Some(cached) = self.projection.get()
            && cached.width_bits == width_bits
            && cached.height_bits == height_bits
        {
            return cached.view_proj;
        }
        let view_proj = technique_view_proj(width, height);
        self.projection.set(Some(ProjectionCache {
            width_bits,
            height_bits,
            view_proj,
        }));
        view_proj
    }
}

fn technique_layout() -> &'static TechniqueLayout {
    static LAYOUT: OnceLock<TechniqueLayout> = OnceLock::new();
    LAYOUT.get_or_init(|| TechniqueLayout {
        front: std::array::from_fn(|index| front_circle_layout(index + 1)),
        back: std::array::from_fn(|index| back_circle_layout(index + 11)),
    })
}

fn front_circle_layout(i: usize) -> CircleLayout {
    let fi = i as f64;
    let zoom = random_xd(fi * 1.6) + 0.35;
    CircleLayout {
        zoom,
        z_pos: (random_xd(fi * 13.0) - 0.6) * (1.0 / zoom) * 850.0,
        rotation_base: random_xd(fi) as f64 * 400.0,
        rotation_speed: random_xd(fi * 3.4) as f64 * 14.0,
        alpha: random_xd(fi),
    }
}

fn back_circle_layout(i: usize) -> CircleLayout {
    let fi = i as f64;
    let zoom = random_xd(fi * 2.8) + 0.35;
    let random_rotation = random_xd(fi * 3.6) as f64;
    CircleLayout {
        zoom,
        z_pos: (random_xd(fi * 13.0) - 0.6) * (2.0 / zoom) * 850.0,
        rotation_base: random_xd(fi) as f64 * 2000.0 + random_rotation * 14.0 * fi * 2000.0,
        rotation_speed: random_rotation * 14.0,
        alpha: random_xd(fi / 1.6),
    }
}

fn technique_assets() -> Option<&'static TechniqueAssets> {
    static ASSETS: OnceLock<Option<TechniqueAssets>> = OnceLock::new();
    ASSETS
        .get_or_init(|| match load_assets() {
            Ok(assets) => Some(assets),
            Err(err) => {
                log::warn!("Failed to load technique background assets: {err}");
                None
            }
        })
        .as_ref()
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
        .enumerate()
        .map(|(index, slot)| {
            let size = slot
                .model
                .as_ref()
                .map(|model| model.size())
                .unwrap_or([1.0, 1.0]);
            let vertices = slot
                .model
                .as_ref()
                .map(|_| build_model_geometry(&slot))
                .unwrap_or_else(|| Arc::from([]));
            let geom_cache_key = technique_geom_cache_key(path, index, &slot, vertices.len());
            let static_draw = (slot.model_timeline.is_empty()
                && slot.model_auto_rot_z_keys.is_empty()
                && slot.model_effect.mode == ModelEffectMode::None)
                .then(|| slot.model_draw_at(0.0, 0.0));
            let static_uv = (slot.uv_velocity == [0.0, 0.0]).then(|| slot.uv_for_frame_at(0, 0.0));
            TechniqueLayer {
                slot,
                size,
                vertices,
                geom_cache_key,
                static_draw,
                static_uv,
            }
        })
        .collect::<Vec<_>>();
    Ok(Arc::from(layers))
}

fn push_layers(
    out: &mut Vec<Actor>,
    layers: &[TechniqueLayer],
    instance: ModelInstance,
    model_elapsed_s: f32,
) {
    for layer in layers {
        let mut draw = layer
            .static_draw
            .unwrap_or_else(|| layer.slot.model_draw_at(model_elapsed_s, 0.0));
        draw.rot[0] += instance.base_rot[0];
        draw.rot[1] += instance.base_rot[1];
        draw.rot[2] += instance.base_rot[2];
        draw.pos[2] += instance.z_pos;
        let size = [layer.size[0] * instance.zoom, layer.size[1] * instance.zoom];
        let uv = layer
            .static_uv
            .unwrap_or_else(|| layer.slot.uv_for_frame_at(0, model_elapsed_s));
        if let Some(mut actor) = noteskin_model_actor_from_draw_depth_sorted_affine_cached_geometry(
            &layer.slot,
            draw,
            instance.center,
            size,
            uv,
            0.0,
            instance.color,
            deadlib_render::BlendMode::Alpha,
            MODEL_Z,
            Arc::clone(&layer.vertices),
            layer.geom_cache_key,
        ) {
            if let Actor::TexturedMesh { depth_test, .. } = &mut actor {
                *depth_test = false;
            }
            out.push(actor);
        }
    }
}

#[cfg(any(test, feature = "bench-support"))]
fn push_layers_legacy(
    out: &mut Vec<Actor>,
    layers: &[TechniqueLayer],
    instance: ModelInstance,
    elapsed_s: f64,
) {
    let model_elapsed_s = bounded_model_elapsed(elapsed_s);
    for layer in layers {
        let mut draw = layer.slot.model_draw_at(model_elapsed_s, 0.0);
        draw.rot[0] += instance.base_rot[0];
        draw.rot[1] += instance.base_rot[1];
        draw.rot[2] += instance.base_rot[2];
        draw.pos[2] += instance.z_pos;
        let size = [layer.size[0] * instance.zoom, layer.size[1] * instance.zoom];
        let uv = layer.slot.uv_for_frame_at(0, model_elapsed_s);
        if let Some(mut actor) = noteskin_model_actor_from_draw_depth_sorted_affine_cached_geometry(
            &layer.slot,
            draw,
            instance.center,
            size,
            uv,
            0.0,
            instance.color,
            deadlib_render::BlendMode::Alpha,
            MODEL_Z,
            Arc::clone(&layer.vertices),
            layer.geom_cache_key,
        ) {
            if let Actor::TexturedMesh { depth_test, .. } = &mut actor {
                *depth_test = false;
            }
            out.push(actor);
        }
    }
}

fn technique_geom_cache_key(
    path: &str,
    index: usize,
    slot: &noteskin::SpriteSlot,
    vertex_count: usize,
) -> TMeshCacheKey {
    let mut hasher = XxHash64::default();
    "deadsync-technique-bg-v1".hash(&mut hasher);
    path.hash(&mut hasher);
    index.hash(&mut hasher);
    slot.texture_key().hash(&mut hasher);
    vertex_count.hash(&mut hasher);
    hasher.finish().max(1)
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

fn technique_view_proj(width: f32, height: f32) -> Matrix4 {
    let width = width.max(1.0);
    let height = height.max(1.0);
    let theta = 45.0_f32.to_radians();
    let dist = (0.5 * width / theta.tan()).max(1.0);
    let proj = glam::camera::rh::proj::opengl::frustum(
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
    proj * glam::camera::rh::view::look_at_mat4(eye, target, Vector3::new(0.0, 1.0, 0.0))
}

fn random_xd(t: f64) -> f32 {
    if t == 0.0 {
        0.5
    } else {
        ((t * 3229.3).sin() * 43758.5453).rem_euclid(1.0) as f32
    }
}

#[inline(always)]
fn rotating_degrees(base_deg: f64, deg_per_second: f64, elapsed_s: f64) -> f32 {
    (base_deg + deg_per_second * elapsed_s).rem_euclid(360.0) as f32
}

#[inline(always)]
fn bounded_model_elapsed(elapsed_s: f64) -> f32 {
    elapsed_s.rem_euclid(3600.0) as f32
}

#[inline(always)]
fn wrapped_grid_uv_rect(velocity: [f32; 2], elapsed_s: f64) -> [f32; 4] {
    // StepMania's Sprite::Update keeps scrolling custom texture rects bounded by
    // subtracting floor() from the top-left corner each frame. Rebuild that
    // wrapped rect directly here so the repeating square layers stay numerically
    // stable and don't leak seams across the full screen.
    let u0 = (f64::from(velocity[0]) * elapsed_s).rem_euclid(1.0) as f32;
    let v0 = (f64::from(velocity[1]) * elapsed_s).rem_euclid(1.0) as f32;
    [u0, v0, u0 + GRID_RECT_SPAN, v0 + GRID_RECT_SPAN]
}

fn scale_alpha(mut color: [f32; 4], alpha: f32) -> [f32; 4] {
    color[3] *= alpha;
    color
}

#[cfg(feature = "bench-support")]
pub(super) struct BenchState {
    state: State,
}

#[cfg(feature = "bench-support")]
impl BenchState {
    pub(super) fn new() -> Self {
        Self {
            state: State::new(),
        }
    }

    pub(super) fn build(&self, elapsed_s: f64) -> Vec<Actor> {
        let mut actors = Vec::new();
        assert!(
            self.state
                .push_at_elapsed(&mut actors, 3, [0.05, 0.1, 0.15, 1.0], 0.8, elapsed_s,)
        );
        actors
    }

    pub(super) fn build_legacy(&self, elapsed_s: f64) -> Vec<Actor> {
        let mut actors = Vec::new();
        assert!(self.state.push_at_elapsed_legacy(
            &mut actors,
            3,
            [0.05, 0.1, 0.15, 1.0],
            0.8,
            elapsed_s,
        ));
        actors
    }

    pub(super) fn projection(&self, width: f32, height: f32) -> [f32; 16] {
        self.state.view_proj(width, height).to_cols_array()
    }
}

#[cfg(feature = "bench-support")]
pub(super) fn projection_legacy_for_bench(width: f32, height: f32) -> [f32; 16] {
    technique_view_proj(width, height).to_cols_array()
}

#[cfg(feature = "bench-support")]
pub(super) fn layout_checksum_for_bench(elapsed_s: f64) -> u64 {
    let layout = technique_layout();
    circle_layout_checksum(layout.front.iter().chain(&layout.back), elapsed_s)
}

#[cfg(feature = "bench-support")]
pub(super) fn layout_legacy_checksum_for_bench(elapsed_s: f64) -> u64 {
    let front = std::array::from_fn::<_, 10, _>(|index| front_circle_layout(index + 1));
    let back = std::array::from_fn::<_, 8, _>(|index| back_circle_layout_legacy(index + 11));
    circle_layout_checksum(front.iter().chain(&back), elapsed_s)
}

#[cfg(feature = "bench-support")]
fn back_circle_layout_legacy(i: usize) -> CircleLayout {
    let fi = i as f64;
    let zoom = random_xd(fi * 2.8) + 0.35;
    CircleLayout {
        zoom,
        z_pos: (random_xd(fi * 13.0) - 0.6) * (2.0 / zoom) * 850.0,
        rotation_base: random_xd(fi) as f64 * 2000.0
            + random_xd(fi * 3.6) as f64 * 14.0 * fi * 2000.0,
        rotation_speed: random_xd(fi * 3.6) as f64 * 14.0,
        alpha: random_xd(fi / 1.6),
    }
}

#[cfg(feature = "bench-support")]
fn circle_layout_checksum<'a>(
    circles: impl Iterator<Item = &'a CircleLayout>,
    elapsed_s: f64,
) -> u64 {
    circles.fold(0_u64, |checksum, circle| {
        checksum.rotate_left(7)
            ^ u64::from(circle.zoom.to_bits())
            ^ u64::from(circle.z_pos.to_bits()).rotate_left(13)
            ^ u64::from(
                rotating_degrees(circle.rotation_base, circle.rotation_speed, elapsed_s).to_bits(),
            )
            ^ u64::from(circle.alpha.to_bits()).rotate_left(29)
    })
}

#[cfg(feature = "bench-support")]
pub(super) fn layer_checksum_for_bench(elapsed_s: f64) -> u64 {
    layer_checksum(elapsed_s, false)
}

#[cfg(feature = "bench-support")]
pub(super) fn layer_legacy_checksum_for_bench(elapsed_s: f64) -> u64 {
    layer_checksum(elapsed_s, true)
}

#[cfg(feature = "bench-support")]
fn layer_checksum(elapsed_s: f64, legacy: bool) -> u64 {
    let Some(assets) = technique_assets() else {
        return 0;
    };
    let model_elapsed_s = (!legacy).then(|| bounded_model_elapsed(elapsed_s));
    let mut checksum = 0_u64;
    for (layers, instances) in [
        (assets.circle_frag.as_ref(), 18),
        (assets.ring.as_ref(), 2),
        (assets.arrow.as_ref(), 1),
    ] {
        for _ in 0..instances {
            let elapsed = model_elapsed_s.unwrap_or_else(|| bounded_model_elapsed(elapsed_s));
            for layer in layers {
                let draw = if legacy {
                    layer.slot.model_draw_at(elapsed, 0.0)
                } else {
                    layer
                        .static_draw
                        .unwrap_or_else(|| layer.slot.model_draw_at(elapsed, 0.0))
                };
                let uv = if legacy {
                    layer.slot.uv_for_frame_at(0, elapsed)
                } else {
                    layer
                        .static_uv
                        .unwrap_or_else(|| layer.slot.uv_for_frame_at(0, elapsed))
                };
                for value in draw
                    .pos
                    .into_iter()
                    .chain(draw.rot)
                    .chain(draw.zoom)
                    .chain(draw.tint)
                    .chain(uv)
                {
                    checksum = checksum.rotate_left(5) ^ u64::from(value.to_bits());
                }
            }
        }
    }
    checksum
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_model_scene(actors: &[Actor]) -> ([f32; 16], Vec<String>) {
        let camera_index = actors
            .iter()
            .position(|actor| matches!(actor, Actor::CameraPush { .. }))
            .expect("Technique background should push its camera");
        let Actor::CameraPush { view_proj } = &actors[camera_index] else {
            unreachable!();
        };
        let pop_index = actors[camera_index + 1..]
            .iter()
            .position(|actor| matches!(actor, Actor::CameraPop))
            .map(|index| index + camera_index + 1)
            .expect("Technique background should pop its camera");
        (
            view_proj.to_cols_array(),
            actors[camera_index + 1..pop_index]
                .iter()
                .map(|actor| format!("{actor:?}"))
                .collect(),
        )
    }

    fn nested_model_scene(actors: &[Actor]) -> ([f32; 16], Vec<String>) {
        let Actor::Camera {
            view_proj,
            children,
        } = actors
            .iter()
            .find(|actor| matches!(actor, Actor::Camera { .. }))
            .expect("Legacy Technique background should wrap its camera")
        else {
            unreachable!();
        };
        (
            view_proj.to_cols_array(),
            children.iter().map(|actor| format!("{actor:?}")).collect(),
        )
    }

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

    #[test]
    fn technique_layers_use_stable_cached_mesh_keys() {
        let state = State::new();
        let mut actors = Vec::new();
        assert!(state.push_at_elapsed(&mut actors, 3, [0.0, 0.0, 0.0, 1.0], 1.0, 1_000_000.0,));
        let camera_index = actors
            .iter()
            .position(|actor| matches!(actor, Actor::CameraPush { .. }))
            .expect("Technique background should emit a camera scope");
        let pop_index = actors
            .iter()
            .position(|actor| matches!(actor, Actor::CameraPop))
            .expect("Technique background should close its camera scope");
        let mut cached_meshes = 0usize;
        for child in &actors[camera_index + 1..pop_index] {
            if let Actor::TexturedMesh {
                geom_cache_key,
                vertices,
                ..
            } = child
            {
                assert_ne!(*geom_cache_key, deadlib_render::INVALID_TMESH_CACHE_KEY);
                assert!(!vertices.is_empty());
                cached_meshes += 1;
            }
        }
        assert!(cached_meshes > 0);
    }

    #[test]
    fn optimized_technique_scene_matches_legacy_actor_output() {
        for (active_color, alpha, elapsed) in
            [(0, 1.0, 0.0), (3, 0.65, 12.375), (7, 0.2, 1_000_000.0)]
        {
            let state = State::new();
            let mut optimized = Vec::new();
            let mut legacy = Vec::new();
            assert!(state.push_at_elapsed(
                &mut optimized,
                active_color,
                [0.05, 0.1, 0.15, 1.0],
                alpha,
                elapsed,
            ));
            assert!(state.push_at_elapsed_legacy(
                &mut legacy,
                active_color,
                [0.05, 0.1, 0.15, 1.0],
                alpha,
                elapsed,
            ));
            assert_eq!(
                optimized[..5]
                    .iter()
                    .map(|actor| format!("{actor:?}"))
                    .collect::<Vec<_>>(),
                legacy[..5]
                    .iter()
                    .map(|actor| format!("{actor:?}"))
                    .collect::<Vec<_>>()
            );
            assert_eq!(flat_model_scene(&optimized), nested_model_scene(&legacy));
        }
    }

    #[test]
    fn technique_projection_cache_tracks_viewport_dimensions() {
        let state = State::new();
        let default = state.view_proj(854.0, 480.0);
        assert_eq!(
            default.to_cols_array(),
            technique_view_proj(854.0, 480.0).to_cols_array()
        );
        assert_eq!(
            default.to_cols_array(),
            state.view_proj(854.0, 480.0).to_cols_array()
        );
        assert_eq!(
            state.view_proj(640.0, 480.0).to_cols_array(),
            technique_view_proj(640.0, 480.0).to_cols_array()
        );
        assert_ne!(
            default.to_cols_array(),
            state.view_proj(640.0, 480.0).to_cols_array()
        );
    }

    #[test]
    fn technique_arrow_keeps_animated_texture_scroll() {
        let state = State::new();
        let mut at_zero = Vec::new();
        let mut later = Vec::new();
        assert!(state.push_at_elapsed(&mut at_zero, 3, [0.0; 4], 1.0, 0.0));
        assert!(state.push_at_elapsed(&mut later, 3, [0.0; 4], 1.0, 2.5));

        let arrow_shift = |actors: &[Actor]| {
            actors.iter().find_map(|actor| match actor {
                Actor::TexturedMesh {
                    texture,
                    uv_tex_shift,
                    ..
                } if texture.contains("arrow_tex") => Some(*uv_tex_shift),
                _ => None,
            })
        };
        assert_ne!(arrow_shift(&at_zero), arrow_shift(&later));
    }
}
