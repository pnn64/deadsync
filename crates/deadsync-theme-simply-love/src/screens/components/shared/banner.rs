use crate::act;
use deadlib_assets::present_dsl::cover_uv_for_dims_with_fudge;
pub use deadlib_assets::present_dsl::{cover_sprite, cover_uv};
use deadlib_present::actors::Actor;
use std::sync::Arc;

const SCALE_TO_CLIPPED_FUDGE: f32 = 0.15;

#[inline(always)]
fn clipped_uv_for_dims(tex_w: f32, tex_h: f32, frame_w: f32, frame_h: f32) -> Option<[f32; 4]> {
    cover_uv_for_dims_with_fudge(tex_w, tex_h, frame_w, frame_h, SCALE_TO_CLIPPED_FUDGE)
}

#[inline(always)]
#[must_use]
pub fn clipped_uv(texture_key: &str, frame_w: f32, frame_h: f32) -> Option<[f32; 4]> {
    let meta = deadlib_assets::texture_dims(texture_key)?;
    clipped_uv_for_dims(meta.w as f32, meta.h as f32, frame_w, frame_h)
}

fn build_sprite(
    texture_key: impl Into<Arc<str>>,
    x: f32,
    y: f32,
    frame_w: f32,
    frame_h: f32,
    zoom: f32,
    z: i16,
    uv: Option<[f32; 4]>,
) -> Actor {
    let texture_key = texture_key.into();
    let mut actor = act!(sprite(texture_key):
        align(0.5, 0.5):
        xy(x, y):
        setsize(frame_w, frame_h):
        zoom(zoom):
        z(z)
    );
    if let Some(uv) = uv
        && let Actor::Sprite { uv_rect, .. } = &mut actor
    {
        *uv_rect = Some(uv);
    }
    actor
}

pub fn sprite(
    texture_key: impl Into<Arc<str>>,
    x: f32,
    y: f32,
    frame_w: f32,
    frame_h: f32,
    zoom: f32,
    z: i16,
) -> Actor {
    let texture_key = texture_key.into();
    let uv = clipped_uv(texture_key.as_ref(), frame_w, frame_h);
    build_sprite(texture_key, x, y, frame_w, frame_h, zoom, z, uv)
}

#[cfg(test)]
mod tests {
    use super::clipped_uv_for_dims;
    use deadlib_assets::present_dsl::cover_uv_for_dims_with_fudge;

    #[test]
    fn crops_full_art_vertically_like_itgmania() {
        let uv = clipped_uv_for_dims(1536.0, 1024.0, 418.0, 164.0).unwrap();
        assert!((uv[1] - 0.130_741_61).abs() < 0.0001);
        assert!((uv[3] - 0.869_258_4).abs() < 0.0001);
    }

    #[test]
    fn preserves_near_banner_aspect_when_fudge_absorbs_crop() {
        assert!(clipped_uv_for_dims(1024.0, 400.0, 418.0, 164.0).is_none());
    }

    #[test]
    fn crops_very_wide_art_horizontally() {
        let uv = clipped_uv_for_dims(1600.0, 400.0, 418.0, 164.0).unwrap();
        assert!(uv[0] > 0.10);
        assert!(uv[2] < 0.90);
    }

    #[test]
    fn covers_fullscreen_background_without_stretching() {
        let uv = cover_uv_for_dims_with_fudge(1536.0, 1024.0, 1280.0, 720.0, 0.0).unwrap();
        assert!((uv[1] - 0.078_125).abs() < 0.0001);
        assert!((uv[3] - 0.921_875).abs() < 0.0001);
    }
}
