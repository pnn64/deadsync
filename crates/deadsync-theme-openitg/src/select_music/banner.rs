use super::{WHEEL_CENTER, wheel_plane_camera};
use crate::presentation::{HEIGHT, Layout, WIDTH};
use deadlib_assets::WHITE_TEXTURE_KEY;
use deadlib_assets::dynamic::is_dynamic_video_path;
use deadlib_assets::present_dsl::cover_uv;
use deadlib_present::actors::{Actor, MeshTexture, SizeSpec};
use deadlib_present::dsl::SpriteBuilder;
use deadlib_present::render::{BlendMode, CullMode, INVALID_TMESH_CACHE_KEY, TexturedMeshVertex};
use deadlib_present::space::{Metrics, current_metrics, screen_width};
use deadsync_theme::select_music::{WHEEL_OUTGOING_BANNERS, WheelBannerFade};
use glam::{Mat4, Vec3};
use std::cell::RefCell;
use std::path::Path;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

pub const WHEEL_BANNER_FADE: Duration = Duration::from_millis(500);
const MOTION_SECONDS: f32 = 0.75;
pub(super) const CENTER: [f32; 2] = [480.0, 112.0];
const SIZE: [f32; 2] = [430.0, 130.0];
const MASK_SIZE: [f32; 2] = [432.0, 142.0];
const BANNER_Z: i16 = -6;
const FULL_UV: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
const CORNERS: [usize; 6] = [0, 2, 3, 0, 3, 1];

static MASK: LazyLock<Arc<str>> = LazyLock::new(|| "openitg/wheel-banner-mask.png".into());
static WHITE: LazyLock<Arc<str>> = LazyLock::new(|| WHITE_TEXTURE_KEY.into());
pub(super) static FALLBACK: LazyLock<Arc<str>> =
    LazyLock::new(|| "openitg/wheel-fallback-banner.png".into());
static QUAD: LazyLock<Arc<Vec<TexturedMeshVertex>>> = LazyLock::new(|| {
    Arc::new(
        CORNERS
            .map(|corner| {
                let u = (corner % 2) as f32;
                let v = (corner / 2) as f32;
                TexturedMeshVertex {
                    pos: [u, v, 0.0],
                    uv: [u, v],
                    color: [1.0; 4],
                    ..Default::default()
                }
            })
            .into(),
    )
});

thread_local! {
    static FADE_VERTICES: RefCell<[Arc<Vec<TexturedMeshVertex>>; WHEEL_OUTGOING_BANNERS]> =
        RefCell::new(std::array::from_fn(|_| Arc::clone(&QUAD)));
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
        let confirmation_seconds = self.confirmed_at.map_or(exit_elapsed, elapsed_since);
        let departing = (confirmation_seconds / MOTION_SECONDS).clamp(0.0, 1.0);
        WheelBannerFrame {
            shift: 470.0 * (1.0 - incoming) + 440.0 * departing * departing,
            orientation: incoming,
            confirmation_seconds,
            transition_seconds: exit_elapsed,
            outgoing,
        }
    }
}

pub struct WheelBannerFrame<'a> {
    shift: f32,
    orientation: f32,
    pub(super) confirmation_seconds: f32,
    pub(super) transition_seconds: f32,
    outgoing: [Option<WheelBannerFade<'a>>; WHEEL_OUTGOING_BANNERS],
}

impl Default for WheelBannerFrame<'_> {
    fn default() -> Self {
        Self {
            shift: 0.0,
            orientation: 1.0,
            confirmation_seconds: 0.0,
            transition_seconds: 0.0,
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
    let alpha = fade_alphas(fade.elapsed_seconds);
    let vertices = FADE_VERTICES.with_borrow_mut(|layers| {
        let vertices = &mut layers[layer];
        for (vertex, corner) in Arc::make_mut(vertices).iter_mut().zip(CORNERS) {
            vertex.color[3] = alpha[corner];
        }
        Arc::clone(vertices)
    });
    banner_quad(layout, fade.texture, vertices)
}

fn banner_quad(
    layout: Layout,
    texture: &Arc<str>,
    vertices: Arc<Vec<TexturedMeshVertex>>,
) -> Actor {
    quad(
        layout.position(CENTER[0], CENTER[1]),
        layout.size(SIZE),
        texture,
        vertices,
        cover_uv(texture, SIZE[0], SIZE[1]).unwrap_or(FULL_UV),
        None,
    )
}

fn quad(
    offset: [f32; 2],
    [width, height]: [f32; 2],
    texture: &Arc<str>,
    vertices: Arc<Vec<TexturedMeshVertex>>,
    [u0, v0, u1, v1]: [f32; 4],
    mask: Option<(f32, bool)>,
) -> Actor {
    let (world_z, clear_depth) = mask.unwrap_or((0.0, false));
    Actor::ReusableTexturedMesh {
        environment: None,
        align: [0.5; 2],
        offset,
        world_z,
        size: [SizeSpec::Px(width), SizeSpec::Px(height)],
        local_transform: Mat4::from_scale(Vec3::new(width, height, 1.0)),
        texture: MeshTexture::Key(Arc::clone(texture)),
        tint: [1.0; 4],
        vertex_color_encoding: Default::default(),
        glow: [0.0; 4],
        vertices,
        geom_cache_key: INVALID_TMESH_CACHE_KEY,
        uv_scale: [u1 - u0, v1 - v0],
        uv_offset: [u0, v0],
        uv_tex_shift: [0.0; 2],
        depth_test: mask.is_none(),
        depth_write: mask.is_some(),
        depth_test_on_fail: false,
        clear_depth,
        clear_depth_after: false,
        cull_mode: CullMode::None,
        visible: true,
        blend: if mask.is_some() {
            BlendMode::NoEffect
        } else {
            BlendMode::Alpha
        },
        z: if mask.is_some() {
            BANNER_Z - 1
        } else {
            BANNER_Z
        },
    }
}

struct BannerProjection {
    matrix: Mat4,
    metrics: Metrics,
    right_edge: f32,
}

impl BannerProjection {
    fn projected_right(&self, x: f32, y: f32, width: f32) -> f32 {
        let start = Vec3::new(self.metrics.left + x, self.metrics.top - y, 0.0);
        let distance_to_edge = |point: Vec3| {
            let projected = self.matrix * point.extend(1.0);
            projected.x - self.right_edge * projected.w
        };
        let left = distance_to_edge(start);
        let right = distance_to_edge(start + Vec3::X * width);
        (-left / (right - left)).clamp(0.0, 1.0)
    }
}

fn video_banner(
    actors: &mut Vec<Actor>,
    layout: Layout,
    projection: &BannerProjection,
    texture: &Arc<str>,
    alpha: [f32; 4],
) {
    // Video textures use the sprite shader's YUV sampling. Sample the source
    // mask's inside edge through the 130-pixel banner for that draw path.
    const MASK_EDGE: [f32; 16] = [
        75.0, 62.0, 56.0, 50.0, 42.0, 33.0, 25.0, 19.0, 18.0, 14.0, 8.0, 6.0, 3.0, 2.0, 0.0, 0.0,
    ];
    let [x, y] = layout.position(CENTER[0] - SIZE[0] * 0.5, CENTER[1] - SIZE[1] * 0.5);
    let [width, height] = layout.size(SIZE);
    let [u0, v0, u1, v1] = cover_uv(texture, SIZE[0], SIZE[1]).unwrap_or(FULL_UV);
    let top_alpha = f32::midpoint(alpha[0], alpha[1]);
    let bottom_alpha = f32::midpoint(alpha[2], alpha[3]);
    for (index, px) in MASK_EDGE.into_iter().enumerate() {
        let top = index as f32 / MASK_EDGE.len() as f32;
        let bottom = (index + 1) as f32 / MASK_EDGE.len() as f32;
        let middle = f32::midpoint(top, bottom);
        let band_y = height.mul_add(top, y);
        // A subpixel overlap closes raster seams between projected bands.
        let band_bottom = (height.mul_add(bottom, y) + 0.125).min(y + height);
        let bottom_uv = (band_bottom - y) / height;
        let left = (px - 1.0).max(0.0) / SIZE[0];
        let right = projection
            .projected_right(x, band_y, width)
            .min(projection.projected_right(x, band_bottom, width));
        let opacity = (bottom_alpha - top_alpha).mul_add(middle, top_alpha);
        if right <= left || opacity <= 0.0 {
            continue;
        }
        let mut band = SpriteBuilder::texture(Arc::clone(texture));
        band.align(0.0, 0.0);
        band.xy(width.mul_add(left, x), band_y);
        band.size(width * (right - left), band_bottom - band_y);
        band.diffuse([1.0, 1.0, 1.0, opacity]);
        band.z(BANNER_Z);
        band.customtexturerect([
            (u1 - u0).mul_add(left, u0),
            (v1 - v0).mul_add(top, v0),
            (u1 - u0).mul_add(right, u0),
            (v1 - v0).mul_add(bottom_uv, v0),
        ]);
        actors.push(band.build(0));
    }
}

pub(super) fn push(
    actors: &mut Vec<Actor>,
    layout: Layout,
    frame: WheelBannerFrame<'_>,
    texture: Option<&Arc<str>>,
    push_text: impl FnOnce(&mut Vec<Actor>, Layout),
) {
    if CENTER[0] - 0.5 * SIZE[0] + frame.shift >= WIDTH {
        return;
    }
    let moving = layout.translated_x(layout.size([frame.shift, 0.0])[0]);
    let metrics = current_metrics();
    let [x, y] = moving.position(CENTER[0], CENTER[1]);
    let yaw = (20.0 * frame.orientation).to_radians();
    let roll = (0.6 * frame.orientation).to_radians();
    let matrix = wheel_plane_camera(
        layout,
        [CENTER[0] + frame.shift, CENTER[1]],
        WHEEL_CENTER[1],
        Mat4::from_rotation_y(yaw) * Mat4::from_rotation_z(roll),
    );
    let projection = BannerProjection {
        matrix,
        metrics,
        right_edge: layout.size([WIDTH, 0.0])[0] / screen_width(),
    };
    let is_video = |key: &Arc<str>| is_dynamic_video_path(Path::new(key.as_ref()));
    let has_stills = texture.is_some_and(|key| !is_video(key))
        || frame
            .outgoing
            .iter()
            .flatten()
            .any(|fade| !is_video(fade.texture));
    let [right, top] = layout.position(WIDTH, 0.0);
    let margin = screen_width() - right;
    if has_stills && margin > 0.0 {
        // Constrain the moving image to the centered frame without painting
        // over the backdrop beside it.
        let height = layout.size([0.0, HEIGHT])[1];
        actors.push(quad(
            [right + margin * 0.5, top + height * 0.5],
            [margin, height],
            &WHITE,
            Arc::clone(&QUAD),
            FULL_UV,
            Some((0.9, true)),
        ));
    }
    actors.push(Actor::CameraPush { view_proj: matrix });
    if has_stills {
        // Match the native NoEffect sprite's depth bias toward the camera.
        actors.push(quad(
            [x, y],
            layout.size(MASK_SIZE),
            &MASK,
            Arc::clone(&QUAD),
            FULL_UV,
            Some((0.125, margin <= 0.0)),
        ));
    }
    let image_start = actors.len();
    if let Some(texture) = texture {
        if is_video(texture) {
            video_banner(actors, moving, &projection, texture, [1.0; 4]);
        } else {
            actors.push(banner_quad(moving, texture, Arc::clone(&QUAD)));
        }
    }
    push_text(actors, moving);
    for (layer, fade) in frame.outgoing.into_iter().enumerate().rev() {
        if let Some(fade) = fade {
            if is_video(fade.texture) {
                video_banner(
                    actors,
                    moving,
                    &projection,
                    fade.texture,
                    fade_alphas(fade.elapsed_seconds),
                );
            } else {
                actors.push(fading_image(moving, layer, fade));
            }
        }
    }
    if let Some(Actor::ReusableTexturedMesh {
        clear_depth_after, ..
    }) = actors[image_start..]
        .iter_mut()
        .rev()
        .find(|actor| matches!(actor, Actor::ReusableTexturedMesh { .. }))
    {
        *clear_depth_after = true;
    }
    actors.push(Actor::CameraPop);
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
