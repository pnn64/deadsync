use deadlib_present::actors::{ActorResourceArena, SpriteSource};
use deadlib_present::texture::TextureContext;
use deadlib_render_core::INVALID_TEXTURE_HANDLE;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

pub struct TextureChoice {
    pub key: Arc<str>,
    pub label: String,
    cached_handle: AtomicU64,
    cached_generation: AtomicU64,
    cached_actor_texture: AtomicU64,
}

impl TextureChoice {
    #[must_use]
    pub fn new(key: String, label: String) -> Self {
        Self {
            key: Arc::from(key),
            label,
            cached_handle: AtomicU64::new(INVALID_TEXTURE_HANDLE),
            cached_generation: AtomicU64::new(u64::MAX),
            cached_actor_texture: AtomicU64::new(0),
        }
    }

    #[inline(always)]
    pub fn actor_texture_source(
        &self,
        arena: &ActorResourceArena,
        textures: &impl TextureContext,
    ) -> SpriteSource {
        let generation = textures.texture_registry_generation();
        let mut handle = self.cached_handle.load(Ordering::Relaxed);
        if self.cached_generation.load(Ordering::Relaxed) != generation {
            handle = textures.texture_handle(self.key.as_ref());
            self.cached_handle.store(handle, Ordering::Relaxed);
            self.cached_generation.store(generation, Ordering::Relaxed);
        }
        arena.texture_source(&self.key, handle, generation, &self.cached_actor_texture)
    }
}

impl Clone for TextureChoice {
    fn clone(&self) -> Self {
        Self {
            key: Arc::clone(&self.key),
            label: self.label.clone(),
            cached_handle: AtomicU64::new(self.cached_handle.load(Ordering::Relaxed)),
            cached_generation: AtomicU64::new(self.cached_generation.load(Ordering::Relaxed)),
            cached_actor_texture: AtomicU64::new(0),
        }
    }
}

impl core::fmt::Debug for TextureChoice {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TextureChoice")
            .field("key", &self.key)
            .field("label", &self.label)
            .finish()
    }
}

impl PartialEq for TextureChoice {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.label == other.label
    }
}

impl Eq for TextureChoice {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_choice_actor_source_uses_arena_ownership() {
        let choice = TextureChoice::new("key.png".to_string(), "Key".to_string());
        let arena = ActorResourceArena::new(1);
        arena.begin_hit_stats(true);

        let first = choice.actor_texture_source(&arena, &crate::METADATA_TEXTURE_CONTEXT);
        let second = choice.actor_texture_source(&arena, &crate::METADATA_TEXTURE_CONTEXT);

        assert!(matches!(first, SpriteSource::ArenaTextureHandle { .. }));
        assert!(matches!(second, SpriteSource::ArenaTextureHandle { .. }));
        assert_eq!(Arc::strong_count(&choice.key), 2);
        assert_eq!(arena.stats().texture_misses, 1);
        assert_eq!(arena.stats().texture_hits, 1);
    }
}
