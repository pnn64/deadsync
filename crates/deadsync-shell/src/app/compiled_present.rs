use deadlib_assets::WHITE_TEXTURE_KEY;
use deadlib_present::actors::Actor;
use deadlib_present::compiled_scene::{
    CompileError, CompileOptions, CompiledRootPrefix, NodeId, PatchError, RootPrefixError,
    SceneCompiler, SpriteUvRectPatch, SpriteUvSlot,
};
use deadlib_present::compose::TextLayoutCache;
use deadlib_present::font::Font;
use deadlib_present::space::Metrics;
use deadlib_present::texture::{TextureContext, TextureMeta};
use deadlib_render::{INVALID_TEXTURE_HANDLE, TextureHandle};
use deadsync_theme_simply_love::screens::components::shared::visual_style_bg::{
    self, Params, TILED_SPRITE_COUNT,
};
use deadsync_theme_simply_love::visual_styles;
use log::{debug, trace, warn};
use std::collections::HashMap;
use std::fmt;

const TILED_PRIMITIVE_COUNT: usize = TILED_SPRITE_COUNT + 1;
const COMPILE_CLEAR: [f32; 4] = [0.03, 0.03, 0.03, 1.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MetricsKey {
    left: u32,
    right: u32,
    top: u32,
    bottom: u32,
}

impl From<&Metrics> for MetricsKey {
    fn from(metrics: &Metrics) -> Self {
        Self {
            left: metrics.left.to_bits(),
            right: metrics.right.to_bits(),
            top: metrics.top.to_bits(),
            bottom: metrics.bottom.to_bits(),
        }
    }
}

impl MetricsKey {
    fn fingerprint(self) -> u64 {
        [self.left, self.right, self.top, self.bottom]
            .into_iter()
            .fold(0xcbf2_9ce4_8422_2325, |hash, bits| {
                (hash ^ u64::from(bits)).wrapping_mul(0x0000_0100_0000_01b3)
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ParamsKey {
    active_color_index: i32,
    backdrop_rgba: [u32; 4],
    alpha_mul: u32,
}

impl From<Params> for ParamsKey {
    fn from(params: Params) -> Self {
        Self {
            active_color_index: params.active_color_index,
            backdrop_rgba: params.backdrop_rgba.map(f32::to_bits),
            alpha_mul: params.alpha_mul.to_bits(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompileKey {
    metrics: MetricsKey,
    params: ParamsKey,
    background_key: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ResourceSignature {
    white_handle: TextureHandle,
    background_handle: TextureHandle,
    background_dims: TextureMeta,
    background_sheet: (u32, u32),
}

struct Entry {
    key: CompileKey,
    resources: ResourceSignature,
    accepted_texture_generation: u64,
    prefix: CompiledRootPrefix,
    uv_slots: [SpriteUvSlot; TILED_SPRITE_COUNT],
}

#[derive(Clone, Copy, Debug, Default)]
struct CacheStats {
    hits: u64,
    rebuilds: u64,
    fallbacks: u64,
    texture_revalidations: u64,
}

/// One-entry Select Music compiled-background cache.
///
/// Owner/thread model: the shell/render thread only. Lifetime/capacity: one
/// tiled prefix for the current screen/session; replacement is the only
/// eviction. Warmup: Select Music actor-build boundary, never gameplay. Misses
/// and compile failures preserve the legacy actor path. Texture-registry
/// changes first revalidate only the two resource dependencies, avoiding
/// rebuilds for unrelated wheel/banner uploads. Destruction happens on this
/// owner when replaced/dropped. Instrumentation is emitted on rebuild/failure
/// and by compose's compiled-prefix frame counters. Worst cold work is lowering
/// eleven sprites; the hot path performs an atomic generation check and ten
/// direct UV writes with no allocation.
#[derive(Default)]
pub struct SelectMusicBgCache {
    entry: Option<Entry>,
    failed: Option<(CompileKey, u64)>,
    stats: CacheStats,
}

pub struct SelectMusicBgFrame<'a> {
    pub prefix: &'a CompiledRootPrefix,
    pub patches: [SpriteUvRectPatch; TILED_SPRITE_COUNT],
}

impl SelectMusicBgCache {
    pub fn prepare<T: TextureContext + ?Sized>(
        &mut self,
        params: Params,
        metrics: &Metrics,
        fonts: &HashMap<&'static str, Font>,
        text_cache: &mut TextLayoutCache,
        textures: &T,
    ) -> bool {
        if !visual_style_bg::tiled_style_active() {
            self.entry = None;
            self.failed = None;
            return self.fallback();
        }

        let key = CompileKey {
            metrics: MetricsKey::from(metrics),
            params: params.into(),
            background_key: visual_styles::shared_background_texture_key(),
        };
        let texture_generation = textures.texture_registry_generation();

        if let Some(entry) = self.entry.as_mut()
            && entry.key == key
        {
            if entry.accepted_texture_generation == texture_generation {
                self.stats.hits = self.stats.hits.saturating_add(1);
                return true;
            }
            if resource_signature(key.background_key, textures) == Some(entry.resources) {
                entry.accepted_texture_generation = texture_generation;
                self.stats.hits = self.stats.hits.saturating_add(1);
                self.stats.texture_revalidations =
                    self.stats.texture_revalidations.saturating_add(1);
                return true;
            }
        }

        if self.failed == Some((key, texture_generation)) {
            return self.fallback();
        }

        let Some(resources) = resource_signature(key.background_key, textures) else {
            self.entry = None;
            self.failed = Some((key, texture_generation));
            return self.fallback();
        };
        match build_entry(
            key,
            resources,
            texture_generation,
            params,
            metrics,
            fonts,
            text_cache,
            textures,
        ) {
            Ok(entry) => {
                self.entry = Some(entry);
                self.failed = None;
                self.stats.rebuilds = self.stats.rebuilds.saturating_add(1);
                debug!(
                    "Compiled Select Music tiled background (rebuilds={}, hits={}, texture_revalidations={})",
                    self.stats.rebuilds, self.stats.hits, self.stats.texture_revalidations
                );
                true
            }
            Err(error) => {
                self.entry = None;
                self.failed = Some((key, texture_generation));
                warn!("Select Music compiled background unavailable; using legacy actors: {error}");
                self.fallback()
            }
        }
    }

    #[inline]
    pub fn frame(&self) -> Option<SelectMusicBgFrame<'_>> {
        self.frame_with_rects(visual_style_bg::current_tiled_uv_rects())
    }

    fn frame_with_rects(
        &self,
        rects: [[f32; 4]; TILED_SPRITE_COUNT],
    ) -> Option<SelectMusicBgFrame<'_>> {
        let entry = self.entry.as_ref()?;
        Some(SelectMusicBgFrame {
            prefix: &entry.prefix,
            patches: std::array::from_fn(|index| {
                let rect = rects[index];
                SpriteUvRectPatch {
                    slot: entry.uv_slots[index],
                    scale: [rect[2] - rect[0], rect[3] - rect[1]],
                    offset: [rect[0], rect[1]],
                }
            }),
        })
    }

    #[cfg(test)]
    fn frame_at_elapsed(&self, elapsed_s: f64) -> Option<SelectMusicBgFrame<'_>> {
        self.frame_with_rects(visual_style_bg::tiled_uv_rects_at(elapsed_s))
    }

    pub fn invalidate_after_compose_error(&mut self, error: &RootPrefixError) {
        warn!("Select Music compiled background compose failed; restoring legacy actors: {error}");
        self.entry = None;
        self.failed = None;
        let _ = self.fallback();
    }

    #[inline]
    fn fallback(&mut self) -> bool {
        self.stats.fallbacks = self.stats.fallbacks.saturating_add(1);
        if self.stats.fallbacks.is_power_of_two() {
            trace!(
                "Select Music compiled background fallbacks={} rebuilds={} hits={}",
                self.stats.fallbacks, self.stats.rebuilds, self.stats.hits
            );
        }
        false
    }
}

fn resource_signature<T: TextureContext + ?Sized>(
    background_key: &str,
    textures: &T,
) -> Option<ResourceSignature> {
    let white_handle = textures.texture_handle(WHITE_TEXTURE_KEY);
    let background_handle = textures.texture_handle(background_key);
    if white_handle == INVALID_TEXTURE_HANDLE || background_handle == INVALID_TEXTURE_HANDLE {
        return None;
    }
    Some(ResourceSignature {
        white_handle,
        background_handle,
        background_dims: textures.texture_dims(background_key)?,
        background_sheet: textures.sprite_sheet_dims(background_key),
    })
}

#[allow(clippy::too_many_arguments)]
fn build_entry<T: TextureContext + ?Sized>(
    key: CompileKey,
    resources: ResourceSignature,
    texture_generation: u64,
    params: Params,
    metrics: &Metrics,
    fonts: &HashMap<&'static str, Font>,
    text_cache: &mut TextLayoutCache,
    textures: &T,
) -> Result<Entry, BuildError> {
    let actors = visual_style_bg::build_tiled_at_elapsed(params, 0.0);
    let scene = SceneCompiler::new(
        metrics,
        fonts,
        textures,
        text_cache,
        key.metrics.fingerprint(),
        0,
    )
    .compile(&actors, COMPILE_CLEAR, CompileOptions::IMMUTABLE)?;
    let prefix = scene.into_root_sprite_prefix()?;
    validate_shape(&prefix, resources)?;

    let first = prefix.sprite_uv_slot(NodeId(1))?;
    let mut uv_slots = [first; TILED_SPRITE_COUNT];
    for (index, slot) in uv_slots.iter_mut().enumerate() {
        *slot = prefix.sprite_uv_slot(NodeId((index + 1) as u32))?;
    }
    debug_assert_eq!(prefix.stamp().texture_revision, texture_generation);
    Ok(Entry {
        key,
        resources,
        accepted_texture_generation: texture_generation,
        prefix,
        uv_slots,
    })
}

fn validate_shape(
    prefix: &CompiledRootPrefix,
    resources: ResourceSignature,
) -> Result<(), BuildError> {
    if prefix.primitive_count() != TILED_PRIMITIVE_COUNT
        || prefix.sprite_count() != TILED_PRIMITIVE_COUNT
    {
        return Err(BuildError::Shape(
            "expected one backdrop and ten tiled sprites",
        ));
    }
    if prefix
        .primitive(NodeId(0))
        .map(|object| object.texture_handle)
        != Some(resources.white_handle)
    {
        return Err(BuildError::Shape(
            "backdrop did not resolve to the white texture",
        ));
    }
    for node in 1..TILED_PRIMITIVE_COUNT {
        if prefix
            .primitive(NodeId(node as u32))
            .map(|object| object.texture_handle)
            != Some(resources.background_handle)
        {
            return Err(BuildError::Shape(
                "tiled sprite did not resolve to the shared background texture",
            ));
        }
    }
    Ok(())
}

#[derive(Debug)]
enum BuildError {
    Compile(CompileError),
    Prefix(RootPrefixError),
    Patch(PatchError),
    Shape(&'static str),
}

impl From<CompileError> for BuildError {
    fn from(error: CompileError) -> Self {
        Self::Compile(error)
    }
}

impl From<RootPrefixError> for BuildError {
    fn from(error: RootPrefixError) -> Self {
        Self::Prefix(error)
    }
}

impl From<PatchError> for BuildError {
    fn from(error: PatchError) -> Self {
        Self::Patch(error)
    }
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Compile(error) => error.fmt(f),
            Self::Prefix(error) => error.fmt(f),
            Self::Patch(error) => error.fmt(f),
            Self::Shape(message) => f.write_str(message),
        }
    }
}

pub fn prepend_legacy_tiled_background(actors: &mut Vec<Actor>, params: Params) {
    actors.splice(0..0, visual_style_bg::build_tiled(params));
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::compose::{
        ComposeScratch, build_screen_cached_with_scratch_and_texture_context,
        build_screen_cached_with_scratch_and_texture_context_and_root_prefix,
    };
    use deadlib_present::space;
    use deadlib_render::ObjectType;
    use std::cell::Cell;

    struct TestTextures {
        generation: Cell<u64>,
        handle_calls: Cell<u32>,
        background_key: &'static str,
    }

    impl TextureContext for TestTextures {
        fn texture_registry_generation(&self) -> u64 {
            self.generation.get()
        }

        fn texture_dims(&self, key: &str) -> Option<TextureMeta> {
            (key == self.background_key).then_some(TextureMeta { w: 256, h: 256 })
        }

        fn sprite_sheet_dims(&self, _key: &str) -> (u32, u32) {
            (1, 1)
        }

        fn texture_handle(&self, key: &str) -> TextureHandle {
            self.handle_calls
                .set(self.handle_calls.get().saturating_add(1));
            match key {
                WHITE_TEXTURE_KEY => 1,
                key if key == self.background_key => 2,
                _ => INVALID_TEXTURE_HANDLE,
            }
        }
    }

    fn params() -> Params {
        Params {
            active_color_index: 3,
            backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
            alpha_mul: 1.0,
        }
    }

    fn metrics() -> Metrics {
        Metrics {
            left: -427.0,
            right: 427.0,
            top: 240.0,
            bottom: -240.0,
        }
    }

    fn assert_sprite_render_eq(
        expected: &deadlib_render::RenderList,
        actual: &deadlib_render::RenderList,
    ) {
        assert_eq!(expected.clear_color, actual.clear_color);
        assert_eq!(expected.cameras.len(), actual.cameras.len());
        for (expected, actual) in expected.cameras.iter().zip(&actual.cameras) {
            assert_eq!(expected.to_cols_array(), actual.to_cols_array());
        }
        assert_eq!(expected.sprite_instances, actual.sprite_instances);
        assert_eq!(expected.objects.len(), actual.objects.len());
        for (expected, actual) in expected.objects.iter().zip(&actual.objects) {
            assert_eq!(expected.texture_handle, actual.texture_handle);
            assert_eq!(expected.blend, actual.blend);
            assert_eq!(expected.z, actual.z);
            assert_eq!(expected.order, actual.order);
            assert_eq!(expected.camera, actual.camera);
            let (ObjectType::Sprite(expected), ObjectType::Sprite(actual)) =
                (&expected.object_type, &actual.object_type)
            else {
                panic!("tiled background emitted a non-sprite primitive");
            };
            assert_eq!(expected, actual);
        }
    }

    #[test]
    fn actual_tiled_background_prefix_matches_legacy_compose() {
        let metrics = metrics();
        space::set_current_metrics(metrics);
        let background_key = visual_styles::shared_background_texture_key();
        let textures = TestTextures {
            generation: Cell::new(7),
            handle_calls: Cell::new(0),
            background_key,
        };
        let fonts = HashMap::new();
        let mut text_cache = TextLayoutCache::default();
        let mut cache = SelectMusicBgCache::default();
        assert!(cache.prepare(params(), &metrics, &fonts, &mut text_cache, &textures));

        let elapsed_s = 4321.25;
        let actors = visual_style_bg::build_tiled_at_elapsed(params(), elapsed_s);
        let mut legacy_scratch = ComposeScratch::default();
        let legacy = build_screen_cached_with_scratch_and_texture_context(
            &actors,
            COMPILE_CLEAR,
            &metrics,
            &fonts,
            0.0,
            &mut text_cache,
            &mut legacy_scratch,
            &textures,
        );
        let frame = cache
            .frame_at_elapsed(elapsed_s)
            .expect("prepared prefix has a frame");
        let mut mixed_scratch = ComposeScratch::default();
        let mixed = build_screen_cached_with_scratch_and_texture_context_and_root_prefix(
            &[],
            COMPILE_CLEAR,
            &metrics,
            &fonts,
            0.0,
            &mut text_cache,
            &mut mixed_scratch,
            &textures,
            frame.prefix,
            &frame.patches,
        )
        .expect("prepared patches remain valid");

        assert_sprite_render_eq(&legacy, &mixed);
        assert_eq!(mixed_scratch.frame_stats().compiled_prefix_primitives, 11);
        assert_eq!(mixed_scratch.frame_stats().compiled_prefix_patches, 10);
    }

    #[test]
    fn unrelated_texture_generation_change_revalidates_without_rebuild() {
        let metrics = metrics();
        space::set_current_metrics(metrics);
        let textures = TestTextures {
            generation: Cell::new(10),
            handle_calls: Cell::new(0),
            background_key: visual_styles::shared_background_texture_key(),
        };
        let fonts = HashMap::new();
        let mut text_cache = TextLayoutCache::default();
        let mut cache = SelectMusicBgCache::default();
        assert!(cache.prepare(params(), &metrics, &fonts, &mut text_cache, &textures));
        assert_eq!(cache.stats.rebuilds, 1);

        textures.generation.set(11);
        assert!(cache.prepare(params(), &metrics, &fonts, &mut text_cache, &textures));
        assert_eq!(cache.stats.rebuilds, 1);
        assert_eq!(cache.stats.texture_revalidations, 1);
        assert_eq!(
            cache.entry.as_ref().unwrap().accepted_texture_generation,
            11
        );
    }
}
