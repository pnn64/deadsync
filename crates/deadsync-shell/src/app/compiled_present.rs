use deadlib_assets::WHITE_TEXTURE_KEY;
use deadlib_present::actors::Actor;
use deadlib_present::compiled_scene::{
    CompileError, CompileOptions, CompiledDrawFrame, CompiledRootPrefix, NodeId, PatchError,
    RootPrefixError, SceneCompiler, SpriteUvRectPatch, SpriteUvSlot,
};
use deadlib_present::compose::TextLayoutCache;
use deadlib_present::font::Font;
use deadlib_present::space::Metrics;
use deadlib_present::texture::{TextureContext, TextureMeta};
use deadlib_render::{
    INVALID_TEXTURE_HANDLE, TMeshCacheEpoch, TextureHandle, draw_prep::TMeshPrewarmStats,
};
use deadlib_renderer::PreparedDrawFrame;
use deadsync_config::{
    app_config::Config,
    theme::{LogLevel, MachineFont, VersionOverlaySide},
};
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
pub struct SandboxDirectKey {
    metrics: MetricsKey,
    overscan: (i32, i32, i32, i32),
    window_px: (u32, u32),
    show_version_overlay: bool,
    version_overlay_side: VersionOverlaySide,
    log_level: LogLevel,
    machine_font: MachineFont,
}

impl SandboxDirectKey {
    pub fn new(metrics: &Metrics, config: &Config) -> Self {
        Self {
            metrics: metrics.into(),
            overscan: deadlib_present::space::overscan(),
            window_px: deadlib_present::space::current_window_px(),
            show_version_overlay: config.show_version_overlay,
            version_overlay_side: config.version_overlay_side,
            log_level: config.log_level,
            machine_font: config.machine_font,
        }
    }
}

/// Pure gate for the quiet Sandbox direct-frame canary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SandboxDirectEligibility {
    pub sandbox_screen: bool,
    pub idle_transition: bool,
    pub empty_input_log: bool,
    pub hardware_backend: bool,
    pub debug_overlays_hidden: bool,
    pub interaction_hidden: bool,
    pub screenshot_hidden: bool,
}

impl SandboxDirectEligibility {
    #[inline(always)]
    pub const fn ready(self) -> bool {
        self.sandbox_screen
            && self.idle_transition
            && self.empty_input_log
            && self.hardware_backend
            && self.debug_overlays_hidden
            && self.interaction_hidden
            && self.screenshot_hidden
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

pub(super) trait SandboxPreparedFrame {
    fn epoch(&self) -> TMeshCacheEpoch;
    fn owner(&self) -> &CompiledDrawFrame;
}

impl SandboxPreparedFrame for PreparedDrawFrame<CompiledDrawFrame> {
    #[inline(always)]
    fn epoch(&self) -> TMeshCacheEpoch {
        self.epoch()
    }

    #[inline(always)]
    fn owner(&self) -> &CompiledDrawFrame {
        self.owner()
    }
}

struct SandboxEntry<F> {
    key: SandboxDirectKey,
    frame: F,
}

#[derive(Clone, Copy, Debug, Default)]
struct SandboxCacheStats {
    rebuilds: u64,
    fallbacks: u64,
    prewarm_uploads: u64,
    prewarm_vertices: u64,
    runtime_miss_frames: u64,
    runtime_misses: u64,
}

/// One-entry retained frame for the quiet, idle Sandbox canary.
///
/// Owner/thread model: shell render thread only. Lifetime/capacity: one whole
/// frame for the current Sandbox visit and backend geometry epoch; all frame
/// and geometry storage is exact-sized during its first eligible menu frame.
/// Warmup: after live asset uploads and before the first direct submission.
/// Misses, stale epochs, unsupported actors,
/// software rendering, overlays, and incomplete backend prewarm use the legacy
/// frame unchanged. Eviction/replacement occurs on key/resource changes or
/// renderer recreation; backend destruction is therefore outside gameplay on
/// the owner thread. Rebuild/fallback/prewarm totals are logged, while successful
/// submissions are counted in `DrawStats`. A backend cached-geometry miss
/// disables direct submission for the current key/resource epoch after that
/// submission, avoiding an incomplete-frame rebuild loop. A screen exit,
/// renderer recreation, key change, or texture-generation change permits a new
/// prewarm attempt. Warm hits perform revision checks only; they do no actor
/// build, composition, draw prep, allocation, cache pruning, upload, or resource
/// destruction.
pub struct SandboxDirectCache<F = PreparedDrawFrame<CompiledDrawFrame>> {
    entry: Option<SandboxEntry<F>>,
    failed: Option<(SandboxDirectKey, u64, TMeshCacheEpoch)>,
    stats: SandboxCacheStats,
}

impl<F> Default for SandboxDirectCache<F> {
    fn default() -> Self {
        Self {
            entry: None,
            failed: None,
            stats: SandboxCacheStats::default(),
        }
    }
}

impl<F: SandboxPreparedFrame> SandboxDirectCache<F> {
    pub fn frame<T: TextureContext + ?Sized>(
        &self,
        key: SandboxDirectKey,
        epoch: TMeshCacheEpoch,
        textures: &T,
    ) -> Option<&F> {
        let entry = self.entry.as_ref()?;
        (entry.key == key
            && entry.frame.epoch() == epoch
            && entry
                .frame
                .owner()
                .is_current(key.metrics.fingerprint(), 0, textures))
        .then_some(&entry.frame)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare<T, P, E>(
        &mut self,
        key: SandboxDirectKey,
        epoch: TMeshCacheEpoch,
        actors: &[Actor],
        metrics: &Metrics,
        fonts: &HashMap<&'static str, Font>,
        text_cache: &mut TextLayoutCache,
        textures: &T,
        prewarm: P,
    ) -> bool
    where
        T: TextureContext + ?Sized,
        P: FnOnce(CompiledDrawFrame) -> Result<(Option<F>, TMeshPrewarmStats), E>,
        E: fmt::Display,
    {
        if self.frame(key, epoch, textures).is_some() {
            return true;
        }

        let texture_generation = textures.texture_registry_generation();
        if self.failed == Some((key, texture_generation, epoch)) {
            return self.fallback();
        }

        let compiled = SceneCompiler::new(
            metrics,
            fonts,
            textures,
            text_cache,
            key.metrics.fingerprint(),
            0,
        )
        .compile(actors, COMPILE_CLEAR, CompileOptions::IMMUTABLE)
        .and_then(|scene| scene.compile_draw_frame());
        let frame = match compiled {
            Ok(frame) => frame,
            Err(error) => {
                warn!("Sandbox direct frame unavailable; using legacy actors: {error}");
                self.entry = None;
                self.failed = Some((key, texture_generation, epoch));
                return self.fallback();
            }
        };

        let (prepared, prewarm_stats) = match prewarm(frame) {
            Ok((Some(prepared), stats)) if stats.ready() && prepared.epoch() == epoch => {
                (prepared, stats)
            }
            Ok((prepared, stats)) => {
                warn!(
                    "Sandbox direct-frame geometry prewarm incomplete; using legacy actors: requested={} resident={} uploaded={} unavailable={} capacity_exceeded={} identity_mismatch={} upload_failed={} prepared={} current_epoch={}",
                    stats.requested,
                    stats.resident,
                    stats.uploaded,
                    stats.unavailable,
                    stats.capacity_exceeded,
                    stats.identity_mismatch,
                    stats.upload_failed,
                    prepared.is_some(),
                    prepared
                        .as_ref()
                        .is_some_and(|prepared| prepared.epoch() == epoch),
                );
                self.entry = None;
                self.failed = Some((key, texture_generation, epoch));
                return self.fallback();
            }
            Err(error) => {
                warn!("Sandbox direct-frame geometry prewarm failed; using legacy actors: {error}");
                self.entry = None;
                self.failed = Some((key, texture_generation, epoch));
                return self.fallback();
            }
        };

        self.stats.rebuilds = self.stats.rebuilds.saturating_add(1);
        self.stats.prewarm_uploads = self
            .stats
            .prewarm_uploads
            .saturating_add(u64::from(prewarm_stats.uploaded));
        self.stats.prewarm_vertices = self
            .stats
            .prewarm_vertices
            .saturating_add(prewarm_stats.uploaded_vertices);
        debug!(
            "Compiled quiet Sandbox direct frame (rebuilds={}, geometries={}, uploaded={}, vertices={})",
            self.stats.rebuilds,
            prewarm_stats.requested,
            prewarm_stats.uploaded,
            prewarm_stats.uploaded_vertices,
        );
        self.entry = Some(SandboxEntry {
            key,
            frame: prepared,
        });
        self.failed = None;
        true
    }

    pub fn invalidate(&mut self) {
        self.entry = None;
        self.failed = None;
    }

    /// Fails closed for the current key/resource epoch after the backend skipped
    /// retained geometry. The submitted frame cannot safely be replayed, and an
    /// immediate retry could repeat the same incomplete output every frame.
    pub fn disable_after_cached_tmesh_miss<T: TextureContext + ?Sized>(
        &mut self,
        key: SandboxDirectKey,
        epoch: TMeshCacheEpoch,
        textures: &T,
        misses: u32,
    ) {
        if misses == 0 {
            return;
        }

        self.entry = None;
        self.failed = Some((key, textures.texture_registry_generation(), epoch));
        self.stats.runtime_miss_frames = self.stats.runtime_miss_frames.saturating_add(1);
        self.stats.runtime_misses = self.stats.runtime_misses.saturating_add(u64::from(misses));
        if self.stats.runtime_miss_frames.is_power_of_two() {
            warn!(
                "Sandbox direct frame missed cached textured meshes; disabling it for the current resource epoch: miss_frames={} misses={}",
                self.stats.runtime_miss_frames, self.stats.runtime_misses,
            );
        }
    }

    #[inline]
    fn fallback(&mut self) -> bool {
        self.stats.fallbacks = self.stats.fallbacks.saturating_add(1);
        if self.stats.fallbacks.is_power_of_two() {
            trace!(
                "Sandbox direct-frame fallbacks={} rebuilds={} prewarm_uploads={} prewarm_vertices={}",
                self.stats.fallbacks,
                self.stats.rebuilds,
                self.stats.prewarm_uploads,
                self.stats.prewarm_vertices,
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
    use deadlib_present::font::Glyph;
    use deadlib_present::space;
    use deadlib_render::{ObjectType, draw_prep::DrawScratch};
    use deadsync_theme_simply_love::screens::{
        components::shared::version_overlay, sandbox as sandbox_screen,
    };
    use std::{cell::Cell, sync::Arc};

    struct TestTextures {
        generation: Cell<u64>,
        handle_calls: Cell<u32>,
        background_key: &'static str,
    }

    struct TestPreparedFrame {
        owner: CompiledDrawFrame,
        epoch: TMeshCacheEpoch,
    }

    impl TestPreparedFrame {
        fn frame(&self) -> &deadlib_render::DrawFrame {
            self.owner.frame()
        }

        fn owner(&self) -> &CompiledDrawFrame {
            &self.owner
        }
    }

    impl SandboxPreparedFrame for TestPreparedFrame {
        fn epoch(&self) -> TMeshCacheEpoch {
            self.epoch
        }

        fn owner(&self) -> &CompiledDrawFrame {
            &self.owner
        }
    }

    fn prepared_frame(
        epoch: TMeshCacheEpoch,
        owner: CompiledDrawFrame,
    ) -> (Option<TestPreparedFrame>, TMeshPrewarmStats) {
        let requested = owner.geometries().len() as u32;
        (
            Some(TestPreparedFrame { owner, epoch }),
            TMeshPrewarmStats {
                requested,
                resident: requested,
                ..TMeshPrewarmStats::default()
            },
        )
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

    fn full_text_font(texture_key: &str, cache_tag: u64) -> Font {
        let glyph = Glyph {
            texture_key: Arc::from(texture_key),
            stroke_texture_key: None,
            tex_rect: [0.0, 0.0, 8.0, 8.0],
            uv_scale: [1.0, 1.0],
            uv_offset: [0.0, 0.0],
            size: [8.0, 8.0],
            offset: [0.0, 0.0],
            advance: 8.0,
            advance_i32: 8,
        };
        Font {
            glyph_map: HashMap::new(),
            ascii_glyphs: Box::new(std::array::from_fn(|_| Some(glyph.clone()))),
            default_glyph: Some(glyph),
            line_spacing: 10,
            height: 10,
            fallback_font_name: None,
            cache_tag,
            chain_key: cache_tag,
            default_stroke_color: [0.0; 4],
            stroke_texture_map: HashMap::new(),
            texture_hints_map: HashMap::new(),
        }
    }

    fn sandbox_fonts(texture_key: &str) -> HashMap<&'static str, Font> {
        HashMap::from([
            ("miso", full_text_font(texture_key, 1)),
            ("wendy", full_text_font(texture_key, 2)),
            ("mega_alpha", full_text_font(texture_key, 3)),
        ])
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

    #[test]
    fn sandbox_direct_gate_requires_every_stability_condition() {
        let ready = SandboxDirectEligibility {
            sandbox_screen: true,
            idle_transition: true,
            empty_input_log: true,
            hardware_backend: true,
            debug_overlays_hidden: true,
            interaction_hidden: true,
            screenshot_hidden: true,
        };
        assert!(ready.ready());

        for blocked in [
            SandboxDirectEligibility {
                sandbox_screen: false,
                ..ready
            },
            SandboxDirectEligibility {
                idle_transition: false,
                ..ready
            },
            SandboxDirectEligibility {
                empty_input_log: false,
                ..ready
            },
            SandboxDirectEligibility {
                hardware_backend: false,
                ..ready
            },
            SandboxDirectEligibility {
                debug_overlays_hidden: false,
                ..ready
            },
            SandboxDirectEligibility {
                interaction_hidden: false,
                ..ready
            },
            SandboxDirectEligibility {
                screenshot_hidden: false,
                ..ready
            },
        ] {
            assert!(!blocked.ready());
        }
    }

    fn sandbox_key(metrics: &Metrics) -> SandboxDirectKey {
        SandboxDirectKey {
            metrics: metrics.into(),
            overscan: space::overscan(),
            window_px: space::current_window_px(),
            show_version_overlay: false,
            version_overlay_side: VersionOverlaySide::Right,
            log_level: LogLevel::Info,
            machine_font: MachineFont::Wendy,
        }
    }

    #[test]
    fn sandbox_direct_cache_rebuilds_before_reusing_stale_resources() {
        let metrics = metrics();
        space::set_current_metrics(metrics);
        let textures = TestTextures {
            generation: Cell::new(20),
            handle_calls: Cell::new(0),
            background_key: visual_styles::shared_background_texture_key(),
        };
        let actors = visual_style_bg::build_tiled_at_elapsed(params(), 0.0);
        let fonts = HashMap::new();
        let mut text_cache = TextLayoutCache::default();
        let mut cache = SandboxDirectCache::<TestPreparedFrame>::default();
        let key = sandbox_key(&metrics);
        let mut epoch = TMeshCacheEpoch::fresh();

        assert!(cache.prepare(
            key,
            epoch,
            &actors,
            &metrics,
            &fonts,
            &mut text_cache,
            &textures,
            |frame| Ok::<_, &'static str>(prepared_frame(epoch, frame)),
        ));
        assert!(cache.frame(key, epoch, &textures).is_some());
        assert_eq!(cache.stats.rebuilds, 1);
        assert!(
            cache
                .frame(
                    SandboxDirectKey {
                        overscan: (
                            key.overscan.0.wrapping_add(1),
                            key.overscan.1,
                            key.overscan.2,
                            key.overscan.3,
                        ),
                        ..key
                    },
                    epoch,
                    &textures,
                )
                .is_none()
        );
        assert!(
            cache
                .frame(
                    SandboxDirectKey {
                        window_px: (key.window_px.0.wrapping_add(1), key.window_px.1),
                        ..key
                    },
                    epoch,
                    &textures,
                )
                .is_none()
        );

        epoch = TMeshCacheEpoch::fresh();
        assert!(cache.frame(key, epoch, &textures).is_none());
        assert!(cache.prepare(
            key,
            epoch,
            &actors,
            &metrics,
            &fonts,
            &mut text_cache,
            &textures,
            |frame| Ok::<_, &'static str>(prepared_frame(epoch, frame)),
        ));
        assert_eq!(cache.stats.rebuilds, 2);

        textures.generation.set(21);
        assert!(cache.frame(key, epoch, &textures).is_none());
        assert!(cache.prepare(
            key,
            epoch,
            &actors,
            &metrics,
            &fonts,
            &mut text_cache,
            &textures,
            |frame| Ok::<_, &'static str>(prepared_frame(epoch, frame)),
        ));
        assert_eq!(cache.stats.rebuilds, 3);
    }

    #[test]
    fn sandbox_direct_cache_falls_back_when_prewarm_is_incomplete() {
        let metrics = metrics();
        space::set_current_metrics(metrics);
        let textures = TestTextures {
            generation: Cell::new(30),
            handle_calls: Cell::new(0),
            background_key: visual_styles::shared_background_texture_key(),
        };
        let actors = visual_style_bg::build_tiled_at_elapsed(params(), 0.0);
        let fonts = HashMap::new();
        let mut text_cache = TextLayoutCache::default();
        let mut cache = SandboxDirectCache::<TestPreparedFrame>::default();
        let key = sandbox_key(&metrics);
        let epoch = TMeshCacheEpoch::fresh();

        assert!(!cache.prepare(
            key,
            epoch,
            &actors,
            &metrics,
            &fonts,
            &mut text_cache,
            &textures,
            |_| {
                Ok::<_, &'static str>((
                    None,
                    TMeshPrewarmStats {
                        requested: 1,
                        unavailable: 1,
                        capacity_exceeded: 1,
                        ..TMeshPrewarmStats::default()
                    },
                ))
            },
        ));
        assert!(cache.frame(key, epoch, &textures).is_none());
        assert_eq!(cache.stats.fallbacks, 1);
    }

    #[test]
    fn sandbox_direct_cache_fails_closed_after_runtime_geometry_miss() {
        let metrics = metrics();
        space::set_current_metrics(metrics);
        let textures = TestTextures {
            generation: Cell::new(35),
            handle_calls: Cell::new(0),
            background_key: visual_styles::shared_background_texture_key(),
        };
        let actors = visual_style_bg::build_tiled_at_elapsed(params(), 0.0);
        let fonts = HashMap::new();
        let mut text_cache = TextLayoutCache::default();
        let mut cache = SandboxDirectCache::<TestPreparedFrame>::default();
        let key = sandbox_key(&metrics);
        let epoch = TMeshCacheEpoch::fresh();

        assert!(cache.prepare(
            key,
            epoch,
            &actors,
            &metrics,
            &fonts,
            &mut text_cache,
            &textures,
            |frame| Ok::<_, &'static str>(prepared_frame(epoch, frame)),
        ));
        cache.disable_after_cached_tmesh_miss(key, epoch, &textures, 0);
        assert!(cache.frame(key, epoch, &textures).is_some());

        cache.disable_after_cached_tmesh_miss(key, epoch, &textures, 2);
        assert!(cache.frame(key, epoch, &textures).is_none());
        assert_eq!(cache.stats.runtime_miss_frames, 1);
        assert_eq!(cache.stats.runtime_misses, 2);

        let mut prewarm_called = false;
        assert!(!cache.prepare(
            key,
            epoch,
            &actors,
            &metrics,
            &fonts,
            &mut text_cache,
            &textures,
            |frame| {
                prewarm_called = true;
                Ok::<_, &'static str>(prepared_frame(epoch, frame))
            },
        ));
        assert!(!prewarm_called);
        assert!(cache.frame(key, epoch, &textures).is_none());
        assert_eq!(cache.stats.rebuilds, 1);

        textures.generation.set(36);
        assert!(cache.prepare(
            key,
            epoch,
            &actors,
            &metrics,
            &fonts,
            &mut text_cache,
            &textures,
            |frame| {
                prewarm_called = true;
                Ok::<_, &'static str>(prepared_frame(epoch, frame))
            },
        ));
        assert!(prewarm_called);
        assert!(cache.frame(key, epoch, &textures).is_some());
        assert_eq!(cache.stats.rebuilds, 2);
    }

    #[test]
    fn quiet_sandbox_direct_frame_matches_legacy_preparation() {
        let metrics = metrics();
        space::set_current_metrics(metrics);
        let texture_key = "sandbox_font_page";
        let textures = TestTextures {
            generation: Cell::new(40),
            handle_calls: Cell::new(0),
            background_key: texture_key,
        };
        let fonts = sandbox_fonts(texture_key);
        let mut actors = Vec::new();
        sandbox_screen::push_actors(&mut actors, &sandbox_screen::init());
        actors.extend(version_overlay::build(
            VersionOverlaySide::Right,
            LogLevel::Info,
            "test",
            Some("1234567"),
        ));
        let key = SandboxDirectKey {
            metrics: (&metrics).into(),
            overscan: space::overscan(),
            window_px: space::current_window_px(),
            show_version_overlay: true,
            version_overlay_side: VersionOverlaySide::Right,
            log_level: LogLevel::Info,
            machine_font: MachineFont::Wendy,
        };

        let mut compile_cache = TextLayoutCache::default();
        SceneCompiler::new(
            &metrics,
            &fonts,
            &textures,
            &mut compile_cache,
            key.metrics.fingerprint(),
            0,
        )
        .compile(&actors, COMPILE_CLEAR, CompileOptions::IMMUTABLE)
        .expect("quiet Sandbox snapshot must remain in the immutable subset");

        let mut text_cache = TextLayoutCache::default();
        let mut cache = SandboxDirectCache::<TestPreparedFrame>::default();
        let epoch = TMeshCacheEpoch::fresh();
        assert!(cache.prepare(
            key,
            epoch,
            &actors,
            &metrics,
            &fonts,
            &mut text_cache,
            &textures,
            |frame| Ok::<_, &'static str>(prepared_frame(epoch, frame)),
        ));
        let direct = cache
            .frame(key, epoch, &textures)
            .expect("prepared direct frame remains current");
        assert!(!direct.owner().geometries().is_empty());

        let mut compose_scratch = ComposeScratch::default();
        let legacy = build_screen_cached_with_scratch_and_texture_context(
            &actors,
            COMPILE_CLEAR,
            &metrics,
            &fonts,
            0.0,
            &mut text_cache,
            &mut compose_scratch,
            &textures,
        );
        let mut draw_scratch = DrawScratch::default();
        let (legacy, _) =
            deadlib_render::draw_prep::prepare_render_list(&legacy, &mut draw_scratch, |_, _| {
                deadlib_render::draw_prep::TMeshCacheResult::Resident
            });
        let direct = direct.frame();
        assert_eq!(direct.clear_color, legacy.clear_color);
        assert_eq!(direct.cameras, legacy.cameras);
        assert_eq!(direct.sprite_instances, legacy.sprite_instances);
        assert!(direct.mesh_vertices.is_empty());
        assert!(legacy.mesh_vertices.is_empty());
        assert!(direct.tmesh_vertices.is_empty());
        assert!(legacy.tmesh_vertices.is_empty());
        assert_eq!(direct.tmesh_instances, legacy.tmesh_instances);
        assert_eq!(direct.ops, legacy.ops);
    }
}
