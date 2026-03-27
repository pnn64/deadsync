use crate::act;
use crate::assets;
use crate::engine::present::actors::Actor;
use std::sync::Arc;

const SCALE_TO_CLIPPED_FUDGE: f32 = 0.15;

#[inline(always)]
fn cover_uv_for_dims_with_fudge(
    tex_w: f32,
    tex_h: f32,
    frame_w: f32,
    frame_h: f32,
    fudge: f32,
) -> Option<[f32; 4]> {
    if !(tex_w > 0.0 && tex_h > 0.0 && frame_w > 0.0 && frame_h > 0.0) {
        return None;
    }

    let scale = (frame_w / tex_w).max(frame_h / tex_h);
    let zoom_w = tex_w * scale;
    let zoom_h = tex_h * scale;
    let crop_x = zoom_w > frame_w + 0.01;
    let cut = if crop_x {
        (zoom_w - frame_w) / zoom_w
    } else {
        (zoom_h - frame_h) / zoom_h
    };
    let each = ((cut - fudge).max(0.0)) * 0.5;
    if each <= 0.0 {
        return None;
    }

    Some(if crop_x {
        [each, 0.0, 1.0 - each, 1.0]
    } else {
        [0.0, each, 1.0, 1.0 - each]
    })
}

#[inline(always)]
fn clipped_uv_for_dims(tex_w: f32, tex_h: f32, frame_w: f32, frame_h: f32) -> Option<[f32; 4]> {
    cover_uv_for_dims_with_fudge(tex_w, tex_h, frame_w, frame_h, SCALE_TO_CLIPPED_FUDGE)
}

#[inline(always)]
fn cover_uv_for_dims(tex_w: f32, tex_h: f32, frame_w: f32, frame_h: f32) -> Option<[f32; 4]> {
    cover_uv_for_dims_with_fudge(tex_w, tex_h, frame_w, frame_h, 0.0)
}

#[inline(always)]
pub fn clipped_uv(texture_key: &str, frame_w: f32, frame_h: f32) -> Option<[f32; 4]> {
    let meta = assets::texture_dims(texture_key)?;
    clipped_uv_for_dims(meta.w as f32, meta.h as f32, frame_w, frame_h)
}

#[inline(always)]
pub fn cover_uv(texture_key: &str, frame_w: f32, frame_h: f32) -> Option<[f32; 4]> {
    let meta = assets::texture_dims(texture_key)?;
    cover_uv_for_dims(meta.w as f32, meta.h as f32, frame_w, frame_h)
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
    let mut actor = act!(sprite(texture_key.clone()):
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

pub fn cover_sprite(
    texture_key: impl Into<Arc<str>>,
    x: f32,
    y: f32,
    frame_w: f32,
    frame_h: f32,
    zoom: f32,
    z: i16,
) -> Actor {
    let texture_key = texture_key.into();
    let uv = cover_uv(texture_key.as_ref(), frame_w, frame_h);
    build_sprite(texture_key, x, y, frame_w, frame_h, zoom, z, uv)
}

#[cfg(test)]
mod tests {
    use super::{clipped_uv_for_dims, cover_uv_for_dims};

    #[test]
    fn crops_full_art_vertically_like_itgmania() {
        let uv = clipped_uv_for_dims(1536.0, 1024.0, 418.0, 164.0).unwrap();
        assert!((uv[1] - 0.13074161).abs() < 0.0001);
        assert!((uv[3] - 0.8692584).abs() < 0.0001);
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
        let uv = cover_uv_for_dims(1536.0, 1024.0, 1280.0, 720.0).unwrap();
        assert!((uv[1] - 0.078125).abs() < 0.0001);
        assert!((uv[3] - 0.921875).abs() < 0.0001);
    }
}
