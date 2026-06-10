use crate::actors::{SpriteSource, TextureKeyHandle};
use crate::font;
use deadsync_render as renderer;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextureMeta {
    pub w: u32,
    pub h: u32,
}

pub trait TextureContext {
    fn texture_registry_generation(&self) -> u64;
    fn texture_dims(&self, key: &str) -> Option<TextureMeta>;
    fn sprite_sheet_dims(&self, key: &str) -> (u32, u32);
    fn texture_handle(&self, key: &str) -> renderer::TextureHandle;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NullTextureContext;

impl TextureContext for NullTextureContext {
    #[inline(always)]
    fn texture_registry_generation(&self) -> u64 {
        0
    }

    #[inline(always)]
    fn texture_dims(&self, _key: &str) -> Option<TextureMeta> {
        None
    }

    #[inline(always)]
    fn sprite_sheet_dims(&self, key: &str) -> (u32, u32) {
        font::parse_sprite_sheet_dims_from_key(key)
    }

    #[inline(always)]
    fn texture_handle(&self, _key: &str) -> renderer::TextureHandle {
        renderer::INVALID_TEXTURE_HANDLE
    }
}

#[inline(always)]
pub fn cached_static_texture_source<T: TextureContext + ?Sized>(
    key: &'static str,
    cached_handle: &AtomicU64,
    cached_generation: &AtomicU64,
    textures: &T,
) -> SpriteSource {
    let generation = textures.texture_registry_generation();
    let handle = cached_handle.load(Ordering::Relaxed);
    if handle != renderer::INVALID_TEXTURE_HANDLE
        && cached_generation.load(Ordering::Relaxed) == generation
    {
        return SpriteSource::TextureStaticHandle {
            key,
            handle,
            generation,
        };
    }

    let handle = textures.texture_handle(key);
    cached_handle.store(handle, Ordering::Relaxed);
    cached_generation.store(generation, Ordering::Relaxed);
    SpriteSource::TextureStaticHandle {
        key,
        handle,
        generation,
    }
}

#[inline(always)]
pub fn cached_texture_key_handle<T: TextureContext + ?Sized>(
    key: &Arc<str>,
    cached_handle: &AtomicU64,
    cached_generation: &AtomicU64,
    textures: &T,
) -> TextureKeyHandle {
    let generation = textures.texture_registry_generation();
    let handle = cached_handle.load(Ordering::Relaxed);
    if handle != renderer::INVALID_TEXTURE_HANDLE
        && cached_generation.load(Ordering::Relaxed) == generation
    {
        return TextureKeyHandle {
            key: Arc::clone(key),
            handle,
            generation,
        };
    }

    let handle = textures.texture_handle(key.as_ref());
    cached_handle.store(handle, Ordering::Relaxed);
    cached_generation.store(generation, Ordering::Relaxed);
    TextureKeyHandle {
        key: Arc::clone(key),
        handle,
        generation,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        TextureContext, TextureMeta, cached_static_texture_source, cached_texture_key_handle,
    };
    use crate::actors::SpriteSource;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TestTextureContext {
        generation: u64,
        handle: u64,
        handle_calls: AtomicU64,
    }

    impl TextureContext for TestTextureContext {
        fn texture_registry_generation(&self) -> u64 {
            self.generation
        }

        fn texture_dims(&self, _key: &str) -> Option<TextureMeta> {
            None
        }

        fn sprite_sheet_dims(&self, _key: &str) -> (u32, u32) {
            (1, 1)
        }

        fn texture_handle(&self, _key: &str) -> u64 {
            self.handle_calls.fetch_add(1, Ordering::Relaxed);
            self.handle
        }
    }

    #[test]
    fn cached_static_texture_source_reuses_matching_generation() {
        let cached_handle = AtomicU64::new(77);
        let cached_generation = AtomicU64::new(5);
        let textures = TestTextureContext {
            generation: 5,
            handle: 99,
            handle_calls: AtomicU64::new(0),
        };

        let source =
            cached_static_texture_source("banner", &cached_handle, &cached_generation, &textures);

        assert!(matches!(
            source,
            SpriteSource::TextureStaticHandle {
                key: "banner",
                handle: 77,
                generation: 5
            }
        ));
        assert_eq!(textures.handle_calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn cached_static_texture_source_refreshes_stale_generation() {
        let cached_handle = AtomicU64::new(77);
        let cached_generation = AtomicU64::new(4);
        let textures = TestTextureContext {
            generation: 5,
            handle: 99,
            handle_calls: AtomicU64::new(0),
        };

        let source =
            cached_static_texture_source("banner", &cached_handle, &cached_generation, &textures);

        assert!(matches!(
            source,
            SpriteSource::TextureStaticHandle {
                key: "banner",
                handle: 99,
                generation: 5
            }
        ));
        assert_eq!(cached_handle.load(Ordering::Relaxed), 99);
        assert_eq!(cached_generation.load(Ordering::Relaxed), 5);
        assert_eq!(textures.handle_calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn cached_texture_key_handle_reuses_matching_generation() {
        let key = Arc::<str>::from("banner");
        let cached_handle = AtomicU64::new(77);
        let cached_generation = AtomicU64::new(5);
        let textures = TestTextureContext {
            generation: 5,
            handle: 99,
            handle_calls: AtomicU64::new(0),
        };

        let handle = cached_texture_key_handle(&key, &cached_handle, &cached_generation, &textures);

        assert_eq!(handle.key.as_ref(), "banner");
        assert_eq!(handle.handle, 77);
        assert_eq!(handle.generation, 5);
        assert_eq!(textures.handle_calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn cached_texture_key_handle_refreshes_stale_generation() {
        let key = Arc::<str>::from("banner");
        let cached_handle = AtomicU64::new(77);
        let cached_generation = AtomicU64::new(4);
        let textures = TestTextureContext {
            generation: 5,
            handle: 99,
            handle_calls: AtomicU64::new(0),
        };

        let handle = cached_texture_key_handle(&key, &cached_handle, &cached_generation, &textures);

        assert_eq!(handle.key.as_ref(), "banner");
        assert_eq!(handle.handle, 99);
        assert_eq!(handle.generation, 5);
        assert_eq!(cached_handle.load(Ordering::Relaxed), 99);
        assert_eq!(cached_generation.load(Ordering::Relaxed), 5);
        assert_eq!(textures.handle_calls.load(Ordering::Relaxed), 1);
    }
}
