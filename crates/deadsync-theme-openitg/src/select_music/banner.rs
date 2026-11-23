use crate::presentation::{HEIGHT, Layout, WIDTH, quad};
use deadlib_assets::present_dsl::cover_uv;
use deadlib_present::actors::{Actor, MeshTexture, SizeSpec};
use deadlib_present::render::{BlendMode, CullMode, INVALID_TMESH_CACHE_KEY, TexturedMeshVertex};
use deadlib_present::space::{current_metrics, screen_width};
use deadsync_theme::select_music::{WHEEL_OUTGOING_BANNERS, WheelBannerFade};
use glam::{Mat4, Vec3};
use std::cell::RefCell;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const WHEEL_BANNER_FADE: Duration = Duration::from_millis(500);
const MOTION_SECONDS: f32 = 0.75;
pub(super) const CENTER: [f32; 2] = [478.0, 112.0];
pub(super) const SIZE: [f32; 2] = [300.0, 128.0];
const PLATE_TINT: [f32; 4] = [0.19, 0.046, 0.042, 1.0];
const WHEEL_MATTE: [f32; 4] = [0.11, 0.026, 0.025, 1.0];

thread_local! {
    static FADE_VERTICES: RefCell<[Arc<Vec<TexturedMeshVertex>>; WHEEL_OUTGOING_BANNERS]> =
        RefCell::new(std::array::from_fn(|_| Arc::new(vec![TexturedMeshVertex::default(); 6])));
}

/// Presentation timestamps for one visit to either wheel.
#[derive(Default)]
pub struct WheelBannerMotion {
    entered_at: Option<Instant>,
    confirmed_at: Option<Instant>,
}

impl WheelBannerMotion {
    pub fn enter(&mut self, now: Instant) {
        self.entered_at = Some(now);
        self.confirmed_at = None;
    }

    pub fn confirm(&mut self, now: Instant) {
        self.confirmed_at.get_or_insert(now);
    }

    pub fn cancel_confirmation(&mut self) {
        self.confirmed_at = None;
    }

    #[must_use]
    pub fn frame<'a>(
        &self,
        now: Instant,
        outgoing: [Option<WheelBannerFade<'a>>; WHEEL_OUTGOING_BANNERS],
        exit_elapsed: f32,
    ) -> WheelBannerFrame<'a> {
        let elapsed_since = |at| now.saturating_duration_since(at).as_secs_f32();
        let incoming = self.entered_at.map_or(1.0, |at| {
            let remaining = 1.0 - (elapsed_since(at) / MOTION_SECONDS).min(1.0);
            1.0 - remaining * remaining
        });
        let departing = (self.confirmed_at.map_or(exit_elapsed, elapsed_since) / MOTION_SECONDS)
            .clamp(0.0, 1.0);
        WheelBannerFrame {
            shift: 470.0 * (1.0 - incoming) + 440.0 * departing * departing,
            orientation: incoming,
            outgoing,
        }
    }
}

pub struct WheelBannerFrame<'a> {
    shift: f32,
    orientation: f32,
    outgoing: [Option<WheelBannerFade<'a>>; WHEEL_OUTGOING_BANNERS],
}

impl Default for WheelBannerFrame<'_> {
    fn default() -> Self {
        Self {
            shift: 0.0,
            orientation: 1.0,
            outgoing: [None; WHEEL_OUTGOING_BANNERS],
        }
    }
}

fn fade_alphas(elapsed: f32) -> [f32; 4] {
    const HALF: f32 = 128.0 / 255.0;
    // Corner order is upper-left, upper-right, lower-left, lower-right.
    const STOPS: [[f32; 4]; 5] = [
        [1.0, 1.0, 1.0, 1.0],
        [1.0, 1.0, HALF, 1.0],
        [HALF, 1.0, 0.0, HALF],
        [0.0, HALF, 0.0, 0.0],
        [0.0; 4],
    ];
    let phase = (elapsed * 8.0).clamp(0.0, 4.0);
    let step = (phase as usize).min(3);
    let fraction = phase - step as f32;
    std::array::from_fn(|corner| {
        let start = STOPS[step][corner];
        let end = STOPS[step + 1][corner];
        start + (end - start) * fraction
    })
}

fn fading_image(layout: Layout, layer: usize, fade: WheelBannerFade<'_>) -> Actor {
    let [w, h] = layout.size(SIZE);
    let [u0, v0, u1, v1] = cover_uv(fade.texture, SIZE[0], SIZE[1]).unwrap_or([0.0, 0.0, 1.0, 1.0]);
    let alpha = fade_alphas(fade.elapsed_seconds);
    let corners = [
        ([0.0, 0.0, 0.0], [u0, v0]),
        ([w, 0.0, 0.0], [u1, v0]),
        ([0.0, h, 0.0], [u0, v1]),
        ([w, h, 0.0], [u1, v1]),
    ];
    let vertices = FADE_VERTICES.with_borrow_mut(|layers| {
        let vertices = &mut layers[layer];
        for (vertex, corner) in Arc::make_mut(vertices).iter_mut().zip([0, 2, 3, 0, 3, 1]) {
            (vertex.pos, vertex.uv) = corners[corner];
            vertex.color = [1.0, 1.0, 1.0, alpha[corner]];
        }
        Arc::clone(vertices)
    });
    Actor::ReusableTexturedMesh {
        environment: None,
        align: [0.5; 2],
        offset: layout.position(CENTER[0], CENTER[1]),
        world_z: 0.0,
        size: [SizeSpec::Px(w), SizeSpec::Px(h)],
        local_transform: Mat4::IDENTITY,
        texture: MeshTexture::Key(Arc::clone(fade.texture)),
        tint: [1.0; 4],
        vertex_color_encoding: Default::default(),
        glow: [0.0; 4],
        vertices,
        geom_cache_key: INVALID_TMESH_CACHE_KEY,
        uv_scale: [1.0; 2],
        uv_offset: [0.0; 2],
        uv_tex_shift: [0.0; 2],
        depth_test: false,
        depth_write: false,
        depth_test_on_fail: false,
        clear_depth: false,
        clear_depth_after: false,
        cull_mode: CullMode::None,
        visible: true,
        blend: BlendMode::Alpha,
        z: -6,
    }
}

pub(super) fn push(
    actors: &mut Vec<Actor>,
    layout: Layout,
    frame: WheelBannerFrame<'_>,
    texture: Option<&Arc<str>>,
    push_fallback: impl FnOnce(&mut Vec<Actor>, Layout),
) {
    // Fully departed banners cannot affect the detail plate or the side mat.
    let left = CENTER[0] - 0.5 * SIZE[0];
    if left + frame.shift >= WIDTH {
        return;
    }
    let moving = layout.translated_x(layout.size([frame.shift, 0.0])[0]);
    let metrics = current_metrics();
    let [x, y] = moving.position(CENTER[0], CENTER[1]);
    let pivot = Vec3::new(metrics.left + x, metrics.top - y, 0.0);
    let yaw = (20.0 * frame.orientation).to_radians();
    // Include the settled projection's slope in the flat banner plane so
    // the moving image follows the rails at the opening.
    let roll = (-6.5 * frame.orientation).to_radians();
    let view_proj = metrics.projection()
        * Mat4::from_translation(pivot)
        * Mat4::from_rotation_z(roll)
        * Mat4::from_scale(Vec3::new(yaw.cos(), 1.0, 1.0))
        * Mat4::from_translation(-pivot);
    actors.push(Actor::CameraPush { view_proj });
    actors.push(moving.panel(left, CENTER[1] - 0.5 * SIZE[1], SIZE, PLATE_TINT, -7));
    if let Some(texture) = texture {
        actors.push(moving.image(CENTER, SIZE, texture, -6));
    } else {
        let first = actors.len();
        push_fallback(actors, moving);
        for actor in &mut actors[first..] {
            if let Actor::Text { z, .. } = actor {
                *z = -6;
            }
        }
    }
    for (layer, fade) in frame.outgoing.into_iter().enumerate().rev() {
        if let Some(fade) = fade {
            actors.push(fading_image(moving, layer, fade));
        }
    }
    actors.push(Actor::CameraPop);
    if frame.shift > 0.0 {
        let [right, top] = layout.position(WIDTH, 0.0);
        let width = screen_width() - right;
        if width > 0.0 {
            actors.push(quad(
                [right, top],
                [width, layout.size([0.0, HEIGHT])[1]],
                WHEEL_MATTE,
                -4,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wipe_fades_each_corner_at_the_reference_times() {
        let half = 128.0 / 255.0;
        assert_eq!(fade_alphas(0.0), [1.0; 4]);
        assert_eq!(fade_alphas(0.125), [1.0, 1.0, half, 1.0]);
        assert_eq!(fade_alphas(0.250), [half, 1.0, 0.0, half]);
        assert_eq!(fade_alphas(0.375), [0.0, half, 0.0, 0.0]);
        assert_eq!(fade_alphas(0.500), [0.0; 4]);
        assert_eq!(fade_alphas(0.0625)[2], f32::midpoint(1.0, half));
    }

    #[test]
    fn second_start_does_not_restart_banner_exit() {
        let mut motion = WheelBannerMotion::default();
        let entry = Instant::now();
        motion.enter(entry);
        assert_eq!(motion.frame(entry, [None; 4], 0.0).shift, 470.0);
        let half_entered = motion.frame(entry + Duration::from_millis(375), [None; 4], 0.0);
        assert_eq!(half_entered.shift, 117.5);
        assert_eq!(half_entered.orientation, 0.75);

        let confirm = entry + Duration::from_secs(1);
        motion.confirm(confirm);
        let second_start = confirm + Duration::from_millis(375);
        motion.confirm(second_start);
        assert_eq!(motion.frame(second_start, [None; 4], 0.0).shift, 110.0);
        let done = motion.frame(confirm + Duration::from_millis(750), [None; 4], 0.0);
        assert_eq!(done.shift, 440.0);
        motion.enter(confirm + Duration::from_secs(1));
        assert_eq!(
            motion
                .frame(confirm + Duration::from_secs(1), [None; 4], 0.0)
                .shift,
            470.0
        );
    }
}
