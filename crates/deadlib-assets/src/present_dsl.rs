use crate::METADATA_TEXTURE_CONTEXT;
use deadlib_present::{
    actors::{Actor, IntoTextureKey},
    dsl as present_dsl,
};

#[doc(hidden)]
pub struct SpriteBuilder {
    inner: present_dsl::SpriteBuilder,
}

impl SpriteBuilder {
    #[inline(always)]
    pub fn texture<T: IntoTextureKey>(tex: T) -> Self {
        Self {
            inner: present_dsl::SpriteBuilder::texture(tex),
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn static_texture(tex: &'static str) -> Self {
        Self {
            inner: present_dsl::SpriteBuilder::static_texture(tex),
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn solid() -> Self {
        Self {
            inner: present_dsl::SpriteBuilder::solid(),
        }
    }

    #[inline(always)]
    pub fn zoomto(&mut self, w: f32, h: f32) {
        self.inner
            .zoomto_with_texture_context(w, h, &METADATA_TEXTURE_CONTEXT);
    }

    #[inline(always)]
    #[must_use]
    pub fn build(self, site_base: u64) -> Actor {
        self.inner.build(site_base)
    }

    #[inline(always)]
    pub fn build_tweened(
        self,
        site_base: u64,
        build_steps: impl FnOnce() -> present_dsl::TweenSteps,
    ) -> Actor {
        self.inner.build_tweened_with_texture_context(
            site_base,
            &METADATA_TEXTURE_CONTEXT,
            build_steps,
        )
    }
}

impl std::ops::Deref for SpriteBuilder {
    type Target = present_dsl::SpriteBuilder;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for SpriteBuilder {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

use std::sync::Arc;

pub fn cover_uv_for_dims_with_fudge(
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

fn cover_uv_for_dims(tex_w: f32, tex_h: f32, frame_w: f32, frame_h: f32) -> Option<[f32; 4]> {
    cover_uv_for_dims_with_fudge(tex_w, tex_h, frame_w, frame_h, 0.0)
}

pub fn cover_uv(texture_key: &str, frame_w: f32, frame_h: f32) -> Option<[f32; 4]> {
    let meta = crate::texture_dims(texture_key)?;
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
    let mut actor = deadlib_present::__act_from_builder!((
        align(0.5, 0.5):
        xy(x, y):
        setsize(frame_w, frame_h):
        zoom(zoom):
        z(z)
    ) SpriteBuilder::texture(texture_key));
    if let Some(uv) = uv
        && let Actor::Sprite { uv_rect, .. } = &mut actor
    {
        *uv_rect = Some(uv);
    }
    actor
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
