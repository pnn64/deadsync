use crate::assets;
use crate::engine::gfx as renderer;
use crate::engine::gfx::{BlendMode, RenderList, RenderObject};
use crate::engine::present::actors::{self, SizeSpec};
use crate::engine::present::{anim, font};
use crate::engine::space::Metrics;
use glam::{Mat4 as Matrix4, Vec2 as Vector2, Vec3 as Vector3, Vec4 as Vector4};
use smallvec::SmallVec;
use std::cell::OnceCell;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, DefaultHasher, Hash, Hasher};
use std::sync::Arc;
use twox_hash::XxHash64;

/* ======================= RENDERER SCREEN BUILDER ======================= */

const MAX_RECYCLED_TEXT_MESH_VERTEX_BUFFERS: usize = 512;

#[inline(always)]
pub fn build_screen(
    actors: &[actors::Actor],
    clear_color: [f32; 4],
    m: &Metrics,
    fonts: &HashMap<&'static str, font::Font>,
    total_elapsed: f32,
) -> RenderList {
    let mut text_cache = TextLayoutCache::default();
    build_screen_cached(
        actors,
        clear_color,
        m,
        fonts,
        total_elapsed,
        &mut text_cache,
    )
}

#[inline(always)]
pub fn build_screen_cached(
    actors: &[actors::Actor],
    clear_color: [f32; 4],
    m: &Metrics,
    fonts: &HashMap<&'static str, font::Font>,
    total_elapsed: f32,
    text_cache: &mut TextLayoutCache,
) -> RenderList {
    let mut scratch = ComposeScratch::default();
    build_screen_cached_with_scratch(
        actors,
        clear_color,
        m,
        fonts,
        total_elapsed,
        text_cache,
        &mut scratch,
    )
}

#[inline(always)]
pub fn build_screen_cached_with_scratch(
    actors: &[actors::Actor],
    clear_color: [f32; 4],
    m: &Metrics,
    fonts: &HashMap<&'static str, font::Font>,
    total_elapsed: f32,
    text_cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
) -> RenderList {
    let mut objects = std::mem::take(&mut scratch.objects);
    objects.clear();
    let object_capacity = actors.len().saturating_mul(4).max(64);
    if objects.capacity() < object_capacity {
        objects.reserve(object_capacity - objects.capacity());
    }
    let mut cameras = std::mem::take(&mut scratch.cameras);
    cameras.clear();
    if cameras.capacity() < 4 {
        cameras.reserve(4 - cameras.capacity());
    }
    let mut texture_cache = std::mem::take(&mut scratch.texture_cache);
    texture_cache.begin_frame();
    cameras.push(Matrix4::orthographic_rh_gl(
        m.left, m.right, m.bottom, m.top, -1.0, 1.0,
    ));
    let mut order_counter: u32 = 0;
    let mut masks = std::mem::take(&mut scratch.masks);
    masks.clear();
    if masks.capacity() < 8 {
        masks.reserve(8 - masks.capacity());
    }

    let root_rect = SmRect {
        x: 0.0,
        y: 0.0,
        w: m.right - m.left,
        h: m.top - m.bottom,
    };
    let parent_z: i16 = 0;
    let camera: u8 = 0;

    for actor in actors {
        build_actor_recursive(
            actor,
            root_rect,
            m,
            fonts,
            scratch,
            parent_z,
            camera,
            &mut cameras,
            &mut masks,
            &mut order_counter,
            &mut objects,
            text_cache,
            &mut texture_cache,
            total_elapsed,
        );
    }

    // Prefer the dense stable z-bucket pass for common dense ranges, but fall
    // back to `(z, order)` sorting when insertion order and draw order differ.
    sort_render_objects(&mut objects, scratch);
    scratch.masks = masks;
    scratch.texture_cache = texture_cache;

    // Texture handles are resolved during composition and cached per frame so
    // draw prep/backends only see compact render objects.
    RenderList {
        clear_color,
        cameras,
        objects,
    }
}

#[derive(Default)]
pub struct ComposeScratch {
    objects: Vec<RenderObject>,
    cameras: Vec<Matrix4>,
    masks: Vec<WorldRect>,
    z_counts: Vec<usize>,
    z_perm: Vec<usize>,
    texture_cache: TextureLookupCache,
    transient_text_mesh_builders: Vec<TextMeshBatchBuilder>,
    recycled_text_mesh_vertices: Vec<Vec<renderer::TexturedMeshVertex>>,
}

impl ComposeScratch {
    pub fn recycle_render_list(&mut self, render: &mut RenderList) {
        let mut objects = std::mem::take(&mut render.objects);
        for obj in objects.drain(..) {
            let renderer::ObjectType::TexturedMesh {
                vertices: renderer::TexturedMeshVertices::Transient(mut vertices),
                ..
            } = obj.object_type
            else {
                continue;
            };
            if self.recycled_text_mesh_vertices.len() >= MAX_RECYCLED_TEXT_MESH_VERTEX_BUFFERS {
                continue;
            }
            vertices.clear();
            self.recycled_text_mesh_vertices.push(vertices);
        }
        self.objects = objects;
        let mut cameras = std::mem::take(&mut render.cameras);
        cameras.clear();
        self.cameras = cameras;
    }

    #[inline(always)]
    fn transient_text_mesh_scratch(
        &mut self,
    ) -> (
        &mut Vec<TextMeshBatchBuilder>,
        &mut Vec<Vec<renderer::TexturedMeshVertex>>,
    ) {
        (
            &mut self.transient_text_mesh_builders,
            &mut self.recycled_text_mesh_vertices,
        )
    }
}

fn sort_render_objects(objects: &mut [RenderObject], scratch: &mut ComposeScratch) {
    if objects.len() < 2 {
        return;
    }

    let mut min_z = objects[0].z;
    let mut max_z = min_z;
    let mut sorted_by_z = true;
    let mut sorted_by_key = true;
    let mut prev_key = (min_z, objects[0].order);
    for obj in &objects[1..] {
        let key = (obj.z, obj.order);
        sorted_by_z &= prev_key.0 <= obj.z;
        sorted_by_key &= prev_key <= key;
        min_z = min_z.min(obj.z);
        max_z = max_z.max(obj.z);
        prev_key = key;
    }
    if sorted_by_key {
        return;
    }
    if sorted_by_z {
        objects.sort_unstable_by_key(|o| (o.z, o.order));
        return;
    }

    let range = (i32::from(max_z) - i32::from(min_z) + 1) as usize;
    let dense_range_limit = objects.len().saturating_mul(8).max(256);
    if range > dense_range_limit {
        objects.sort_unstable_by_key(|o| (o.z, o.order));
        return;
    }

    scratch.z_counts.clear();
    scratch.z_counts.resize(range, 0);
    scratch.z_perm.clear();
    scratch.z_perm.resize(objects.len(), 0);

    let min_z_i = i32::from(min_z);
    for obj in objects.iter() {
        scratch.z_counts[(i32::from(obj.z) - min_z_i) as usize] += 1;
    }

    let mut next = 0usize;
    for count in &mut scratch.z_counts {
        let bucket_len = *count;
        *count = next;
        next += bucket_len;
    }

    for (old_idx, obj) in objects.iter().enumerate() {
        let bucket = (i32::from(obj.z) - min_z_i) as usize;
        let new_idx = scratch.z_counts[bucket];
        scratch.z_counts[bucket] = new_idx + 1;
        scratch.z_perm[old_idx] = new_idx;
    }

    for start in 0..objects.len() {
        let current = start;
        while scratch.z_perm[current] != current {
            let next = scratch.z_perm[current];
            objects.swap(current, next);
            scratch.z_perm.swap(current, next);
        }
    }

    if objects
        .windows(2)
        .any(|pair| (pair[0].z, pair[0].order) > (pair[1].z, pair[1].order))
    {
        // Some compose paths append later objects before assigning their final
        // draw order, so stable z-bucketing alone is not always sufficient.
        objects.sort_unstable_by_key(|o| (o.z, o.order));
    }

    debug_assert!(
        objects
            .windows(2)
            .all(|pair| (pair[0].z, pair[0].order) <= (pair[1].z, pair[1].order))
    );
}

#[derive(Clone, Copy)]
struct CachedGlyph {
    texture_key: *const str,
    stroke_texture_key: Option<*const str>,
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    size: [f32; 2],
    offset: [f32; 2],
    advance_i32: i32,
    char_index: usize,
    fill_batch_index: u32,
    fill_vertex_start: u32,
    draw_quad: bool,
}

#[derive(Clone)]
struct CachedLine {
    width_i32: i32,
    glyph_start: usize,
    glyph_len: usize,
}

#[derive(Clone)]
struct CachedTextMeshBatch {
    texture_key: *const str,
    geom_cache_key: renderer::TMeshCacheKey,
    vertices: Arc<[renderer::TexturedMeshVertex]>,
}

#[derive(Default)]
struct CachedTextMeshVariants {
    by_align: [OnceCell<Vec<CachedTextMeshBatch>>; 3],
}

impl CachedTextMeshVariants {
    #[inline(always)]
    const fn index(align: actors::TextAlign) -> usize {
        match align {
            actors::TextAlign::Left => 0,
            actors::TextAlign::Center => 1,
            actors::TextAlign::Right => 2,
        }
    }

    #[inline(always)]
    fn get_or_init<F>(&self, align: actors::TextAlign, init: F) -> &[CachedTextMeshBatch]
    where
        F: FnOnce(actors::TextAlign) -> Vec<CachedTextMeshBatch>,
    {
        self.by_align[Self::index(align)]
            .get_or_init(|| init(align))
            .as_slice()
    }

    #[cfg(test)]
    #[inline(always)]
    fn is_built(&self, align: actors::TextAlign) -> bool {
        self.by_align[Self::index(align)].get().is_some()
    }
}

struct CachedTextLayout {
    layout_seed: u64,
    font_height: i32,
    line_spacing: i32,
    max_logical_width_i: i32,
    glyph_count: usize,
    lines: Vec<CachedLine>,
    glyphs: Vec<CachedGlyph>,
    fill_batches: CachedTextMeshVariants,
    stroke_batches: CachedTextMeshVariants,
}

impl CachedTextLayout {
    #[inline(always)]
    fn fill_batches(&self, align: actors::TextAlign) -> &[CachedTextMeshBatch] {
        self.fill_batches.get_or_init(align, |align| {
            build_text_mesh_batches_for_align(
                self.layout_seed,
                self.font_height,
                self.line_spacing,
                self.max_logical_width_i,
                &self.lines,
                &self.glyphs,
                align,
                false,
            )
        })
    }

    #[inline(always)]
    fn stroke_batches(&self, align: actors::TextAlign) -> &[CachedTextMeshBatch] {
        self.stroke_batches.get_or_init(align, |align| {
            build_text_mesh_batches_for_align(
                self.layout_seed,
                self.font_height,
                self.line_spacing,
                self.max_logical_width_i,
                &self.lines,
                &self.glyphs,
                align,
                true,
            )
        })
    }
}

type WordGlyphs = SmallVec<[CachedGlyph; 16]>;
type AttrIndices = SmallVec<[usize; 8]>;
type ClipPolygon = SmallVec<[ClipVertex; 8]>;
type ClippedMesh = SmallVec<[renderer::TexturedMeshVertex; 18]>;

struct TextLayoutPlacement {
    sx: f32,
    sy: f32,
    block_center_x: f32,
    block_center_y: f32,
}

struct OwnedLayoutEntry {
    layout: Arc<CachedTextLayout>,
    last_used: u64,
}

struct SharedLayoutEntry {
    _owner: Arc<str>,
    layout: Arc<CachedTextLayout>,
    last_used: u64,
}

type TextLayoutHasher = BuildHasherDefault<XxHash64>;
type OwnedLayoutMap = HashMap<Box<str>, OwnedLayoutEntry, TextLayoutHasher>;
type SharedAliasMap = HashMap<usize, SharedLayoutEntry, TextLayoutHasher>;
type TextureMetaMap = HashMap<String, assets::TexMeta, TextLayoutHasher>;
type TextureSheetMap = HashMap<String, (u32, u32), TextLayoutHasher>;
type TextureHandleLookupMap = HashMap<String, renderer::TextureHandle, TextLayoutHasher>;
type PtrTextureMetaMap = HashMap<usize, assets::TexMeta, TextLayoutHasher>;
type PtrTextureSheetMap = HashMap<usize, (u32, u32), TextLayoutHasher>;
type PtrTextureHandleLookupMap = HashMap<usize, renderer::TextureHandle, TextLayoutHasher>;

#[derive(Default)]
struct TextureLookupCache {
    generation: u64,
    dims: TextureMetaMap,
    frame_dims: PtrTextureMetaMap,
    stable_dims: PtrTextureMetaMap,
    sheets: TextureSheetMap,
    frame_sheets: PtrTextureSheetMap,
    stable_sheets: PtrTextureSheetMap,
    handles: TextureHandleLookupMap,
    frame_handles: PtrTextureHandleLookupMap,
    stable_handles: PtrTextureHandleLookupMap,
}

impl TextureLookupCache {
    fn begin_frame(&mut self) {
        self.frame_dims.clear();
        self.frame_sheets.clear();
        self.frame_handles.clear();

        let generation = assets::texture_registry_generation();
        if self.generation != generation {
            self.generation = generation;
            self.dims.clear();
            self.stable_dims.clear();
            self.sheets.clear();
            self.stable_sheets.clear();
            self.handles.clear();
            self.stable_handles.clear();
        }
    }

    #[inline(always)]
    fn ptr_cache_key(key_ptr: *const str) -> usize {
        key_ptr as *const () as usize
    }

    #[inline(always)]
    fn texture_dims(&mut self, key: &str) -> Option<assets::TexMeta> {
        if let Some(&meta) = self.dims.get(key) {
            return Some(meta);
        }
        let meta = assets::texture_dims(key)?;
        self.dims.insert(key.to_owned(), meta);
        Some(meta)
    }

    #[inline(always)]
    fn texture_dims_with_ptr(
        &mut self,
        key_ptr: Option<*const str>,
        key: &str,
    ) -> Option<assets::TexMeta> {
        let Some(key_ptr) = key_ptr else {
            return self.texture_dims(key);
        };
        let key_ptr = Self::ptr_cache_key(key_ptr);
        if let Some(&meta) = self.frame_dims.get(&key_ptr) {
            return Some(meta);
        }
        let meta = self.texture_dims(key)?;
        self.frame_dims.insert(key_ptr, meta);
        Some(meta)
    }

    #[inline(always)]
    fn texture_dims_stable_ptr(
        &mut self,
        key_ptr: *const str,
        key: &str,
    ) -> Option<assets::TexMeta> {
        let key_ptr = Self::ptr_cache_key(key_ptr);
        if let Some(&meta) = self.stable_dims.get(&key_ptr) {
            return Some(meta);
        }
        let meta = self.texture_dims(key)?;
        self.stable_dims.insert(key_ptr, meta);
        Some(meta)
    }

    #[inline(always)]
    fn texture_dims_cached(
        &mut self,
        key_ptr: Option<*const str>,
        key: &str,
        stable_ptr: bool,
    ) -> Option<assets::TexMeta> {
        if stable_ptr && let Some(key_ptr) = key_ptr {
            return self.texture_dims_stable_ptr(key_ptr, key);
        }
        self.texture_dims_with_ptr(key_ptr, key)
    }

    #[inline(always)]
    fn sprite_sheet_dims(&mut self, key: &str) -> (u32, u32) {
        if let Some(&dims) = self.sheets.get(key) {
            return dims;
        }
        let dims = assets::sprite_sheet_dims(key);
        self.sheets.insert(key.to_owned(), dims);
        dims
    }

    #[inline(always)]
    fn sprite_sheet_dims_with_ptr(&mut self, key_ptr: Option<*const str>, key: &str) -> (u32, u32) {
        let Some(key_ptr) = key_ptr else {
            return self.sprite_sheet_dims(key);
        };
        let key_ptr = Self::ptr_cache_key(key_ptr);
        if let Some(&dims) = self.frame_sheets.get(&key_ptr) {
            return dims;
        }
        let dims = self.sprite_sheet_dims(key);
        self.frame_sheets.insert(key_ptr, dims);
        dims
    }

    #[inline(always)]
    fn sprite_sheet_dims_stable_ptr(&mut self, key_ptr: *const str, key: &str) -> (u32, u32) {
        let key_ptr = Self::ptr_cache_key(key_ptr);
        if let Some(&dims) = self.stable_sheets.get(&key_ptr) {
            return dims;
        }
        let dims = self.sprite_sheet_dims(key);
        self.stable_sheets.insert(key_ptr, dims);
        dims
    }

    #[inline(always)]
    fn sprite_sheet_dims_cached(
        &mut self,
        key_ptr: Option<*const str>,
        key: &str,
        stable_ptr: bool,
    ) -> (u32, u32) {
        if stable_ptr && let Some(key_ptr) = key_ptr {
            return self.sprite_sheet_dims_stable_ptr(key_ptr, key);
        }
        self.sprite_sheet_dims_with_ptr(key_ptr, key)
    }

    #[inline(always)]
    fn texture_handle(&mut self, key: &str) -> renderer::TextureHandle {
        if let Some(&handle) = self.handles.get(key) {
            return handle;
        }
        let handle = assets::texture_handle(key);
        self.handles.insert(key.to_owned(), handle);
        handle
    }

    #[inline(always)]
    fn texture_handle_with_ptr(
        &mut self,
        key_ptr: Option<*const str>,
        key: &str,
    ) -> renderer::TextureHandle {
        let Some(key_ptr) = key_ptr else {
            return self.texture_handle(key);
        };
        let key_ptr = Self::ptr_cache_key(key_ptr);
        if let Some(&handle) = self.frame_handles.get(&key_ptr) {
            return handle;
        }
        let handle = self.texture_handle(key);
        self.frame_handles.insert(key_ptr, handle);
        handle
    }

    #[inline(always)]
    fn texture_handle_stable_ptr(
        &mut self,
        key_ptr: *const str,
        key: &str,
    ) -> renderer::TextureHandle {
        let key_ptr = Self::ptr_cache_key(key_ptr);
        if let Some(&handle) = self.stable_handles.get(&key_ptr) {
            return handle;
        }
        let handle = self.texture_handle(key);
        self.stable_handles.insert(key_ptr, handle);
        handle
    }

    #[inline(always)]
    fn texture_handle_cached(
        &mut self,
        key_ptr: Option<*const str>,
        key: &str,
        stable_ptr: bool,
    ) -> renderer::TextureHandle {
        if stable_ptr && let Some(key_ptr) = key_ptr {
            return self.texture_handle_stable_ptr(key_ptr, key);
        }
        self.texture_handle_with_ptr(key_ptr, key)
    }
}

#[inline(always)]
fn str_ptr(key: &str) -> *const str {
    key as *const str
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct TextLayoutKey {
    font_key: u64,
    line_spacing: i32,
    wrap_width_pixels: i32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TextLayoutFrameStats {
    pub owned_hits: u32,
    pub shared_hits: u32,
    pub misses: u32,
    pub built_lines: u32,
    pub built_glyphs: u32,
    pub prunes: u32,
    pub owned_entries: u32,
    pub shared_aliases: u32,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum TextLayoutOverflowPolicy {
    PruneOwnedEntries,
    #[default]
    Saturating,
}

pub struct TextLayoutCache {
    owned_entries: HashMap<TextLayoutKey, OwnedLayoutMap, TextLayoutHasher>,
    shared_aliases: HashMap<TextLayoutKey, SharedAliasMap, TextLayoutHasher>,
    entry_count: usize,
    alias_count: usize,
    max_entries: usize,
    max_aliases: usize,
    use_tick: u64,
    frame_stats: TextLayoutFrameStats,
    overflow_policy: TextLayoutOverflowPolicy,
    uncached_layout: Option<Arc<CachedTextLayout>>,
    prune_ages: Vec<u64>,
}

impl Default for TextLayoutCache {
    fn default() -> Self {
        Self::saturating(4096)
    }
}

impl TextLayoutCache {
    pub fn new(max_entries: usize) -> Self {
        Self::saturating(max_entries)
    }

    pub fn saturating(max_entries: usize) -> Self {
        Self::new_with_policy(max_entries, TextLayoutOverflowPolicy::Saturating)
    }

    pub fn pruning(max_entries: usize) -> Self {
        Self::new_with_policy(max_entries, TextLayoutOverflowPolicy::PruneOwnedEntries)
    }

    pub fn new_with_policy(max_entries: usize, overflow_policy: TextLayoutOverflowPolicy) -> Self {
        let max_entries = max_entries.max(1);
        Self {
            owned_entries: HashMap::default(),
            shared_aliases: HashMap::default(),
            entry_count: 0,
            alias_count: 0,
            max_entries,
            max_aliases: max_entries,
            use_tick: 0,
            frame_stats: TextLayoutFrameStats::default(),
            overflow_policy,
            uncached_layout: None,
            prune_ages: Vec::new(),
        }
    }

    pub fn configure(&mut self, max_entries: usize, overflow_policy: TextLayoutOverflowPolicy) {
        self.max_entries = max_entries.max(1);
        self.max_aliases = self.max_entries;
        self.overflow_policy = overflow_policy;
    }

    /// Freeze the cache at its current size so future misses saturate instead of
    /// pruning or growing during a live frame.
    pub fn lock_growth(&mut self) {
        self.max_entries = self.entry_count.max(1);
        self.max_aliases = self.alias_count;
        self.overflow_policy = TextLayoutOverflowPolicy::Saturating;
    }

    pub fn clear(&mut self) {
        self.owned_entries.clear();
        self.shared_aliases.clear();
        self.entry_count = 0;
        self.alias_count = 0;
        self.use_tick = 0;
        self.frame_stats = TextLayoutFrameStats::default();
        self.uncached_layout = None;
        self.prune_ages.clear();
    }

    #[inline(always)]
    pub fn begin_frame_stats(&mut self) {
        self.frame_stats = TextLayoutFrameStats::default();
    }

    #[inline(always)]
    pub fn frame_stats(&self) -> TextLayoutFrameStats {
        TextLayoutFrameStats {
            owned_entries: saturating_u32(self.entry_count),
            shared_aliases: saturating_u32(self.alias_count),
            ..self.frame_stats
        }
    }

    #[inline(always)]
    fn next_use_tick(&mut self) -> u64 {
        self.use_tick = self.use_tick.saturating_add(1);
        self.use_tick
    }

    fn prune_owned_entries(&mut self) {
        if self.entry_count < self.max_entries {
            return;
        }
        let keep = self
            .max_entries
            .saturating_sub((self.max_entries / 4).max(1));
        let remove = self.entry_count.saturating_sub(keep).max(1);
        let cutoff = {
            let ages = &mut self.prune_ages;
            ages.clear();
            ages.reserve(self.entry_count.saturating_sub(ages.len()));
            for font_entries in self.owned_entries.values() {
                ages.extend(font_entries.values().map(|entry| entry.last_used));
            }
            if ages.is_empty() {
                self.clear();
                return;
            }
            let cutoff_ix = remove.saturating_sub(1).min(ages.len().saturating_sub(1));
            ages.select_nth_unstable(cutoff_ix);
            ages[cutoff_ix]
        };
        let mut removed = 0usize;
        self.owned_entries.retain(|_, font_entries| {
            font_entries.retain(|_, entry| {
                let drop = removed < remove && entry.last_used <= cutoff;
                removed += usize::from(drop);
                !drop
            });
            !font_entries.is_empty()
        });
        if removed == 0 {
            self.clear();
            return;
        }
        self.frame_stats.prunes = self.frame_stats.prunes.saturating_add(1);
        self.entry_count = self.entry_count.saturating_sub(removed);
    }

    fn prune_shared_aliases(&mut self) {
        if self.alias_count < self.max_aliases {
            return;
        }
        let keep = self
            .max_aliases
            .saturating_sub((self.max_aliases / 4).max(1));
        let remove = self.alias_count.saturating_sub(keep).max(1);
        let cutoff = {
            let ages = &mut self.prune_ages;
            ages.clear();
            ages.reserve(self.alias_count.saturating_sub(ages.len()));
            for font_entries in self.shared_aliases.values() {
                ages.extend(font_entries.values().map(|entry| entry.last_used));
            }
            if ages.is_empty() {
                self.shared_aliases.clear();
                self.alias_count = 0;
                return;
            }
            let cutoff_ix = remove.saturating_sub(1).min(ages.len().saturating_sub(1));
            ages.select_nth_unstable(cutoff_ix);
            ages[cutoff_ix]
        };
        let mut removed = 0usize;
        self.shared_aliases.retain(|_, font_entries| {
            font_entries.retain(|_, entry| {
                let drop = removed < remove && entry.last_used <= cutoff;
                removed += usize::from(drop);
                !drop
            });
            !font_entries.is_empty()
        });
        if removed == 0 {
            self.shared_aliases.clear();
            self.alias_count = 0;
            return;
        }
        self.frame_stats.prunes = self.frame_stats.prunes.saturating_add(1);
        self.alias_count = self.alias_count.saturating_sub(removed);
    }

    #[inline(always)]
    fn touch_owned_layout(&mut self, key: TextLayoutKey, text: &str, tick: u64) -> bool {
        let Some(entry) = self
            .owned_entries
            .get_mut(&key)
            .and_then(|font_entries| font_entries.get_mut(text))
        else {
            return false;
        };
        entry.last_used = tick;
        true
    }

    #[inline(always)]
    fn owned_layout(&self, key: TextLayoutKey, text: &str) -> Option<&CachedTextLayout> {
        Some(self.owned_entries.get(&key)?.get(text)?.layout.as_ref())
    }

    #[inline(always)]
    fn owned_layout_arc(&self, key: TextLayoutKey, text: &str) -> Option<&Arc<CachedTextLayout>> {
        Some(&self.owned_entries.get(&key)?.get(text)?.layout)
    }

    #[inline(always)]
    fn touch_shared_layout(&mut self, key: TextLayoutKey, text_key: usize, tick: u64) -> bool {
        let Some(entry) = self
            .shared_aliases
            .get_mut(&key)
            .and_then(|font_entries| font_entries.get_mut(&text_key))
        else {
            return false;
        };
        entry.last_used = tick;
        true
    }

    #[inline(always)]
    fn shared_layout(&self, key: TextLayoutKey, text_key: usize) -> Option<&CachedTextLayout> {
        Some(
            self.shared_aliases
                .get(&key)?
                .get(&text_key)?
                .layout
                .as_ref(),
        )
    }

    #[inline(always)]
    fn uncached_layout_ref(&self) -> &CachedTextLayout {
        self.uncached_layout
            .as_deref()
            .expect("uncached text layout inserted")
    }

    #[inline(always)]
    fn record_layout_build(&mut self, layout: &CachedTextLayout) {
        self.frame_stats.misses = self.frame_stats.misses.saturating_add(1);
        self.frame_stats.built_lines = self
            .frame_stats
            .built_lines
            .saturating_add(saturating_u32(layout.lines.len()));
        self.frame_stats.built_glyphs = self
            .frame_stats
            .built_glyphs
            .saturating_add(saturating_u32(layout.glyph_count));
    }

    fn insert_owned_layout(
        &mut self,
        key: TextLayoutKey,
        text: &str,
        layout: Arc<CachedTextLayout>,
        tick: u64,
    ) -> bool {
        if self.entry_count >= self.max_entries {
            match self.overflow_policy {
                TextLayoutOverflowPolicy::PruneOwnedEntries => {
                    // Avoid hard-clearing the entire cache; that was causing visible
                    // compose spikes once gameplay churn hit the entry cap.
                    self.prune_owned_entries();
                }
                TextLayoutOverflowPolicy::Saturating => {
                    self.uncached_layout = Some(layout);
                    return false;
                }
            }
        }
        let replaced = self.owned_entries.entry(key).or_default().insert(
            text.into(),
            OwnedLayoutEntry {
                layout,
                last_used: tick,
            },
        );
        debug_assert!(replaced.is_none());
        self.entry_count += usize::from(replaced.is_none());
        true
    }

    fn insert_shared_layout(
        &mut self,
        key: TextLayoutKey,
        text_key: usize,
        text: Arc<str>,
        layout: Arc<CachedTextLayout>,
        tick: u64,
    ) -> bool {
        if self.alias_count >= self.max_aliases {
            match self.overflow_policy {
                TextLayoutOverflowPolicy::PruneOwnedEntries => {
                    self.prune_shared_aliases();
                }
                TextLayoutOverflowPolicy::Saturating => {
                    self.uncached_layout = Some(layout);
                    return false;
                }
            }
        }
        let replaced = self.shared_aliases.entry(key).or_default().insert(
            text_key,
            SharedLayoutEntry {
                _owner: text,
                layout,
                last_used: tick,
            },
        );
        debug_assert!(replaced.is_none());
        self.alias_count += usize::from(replaced.is_none());
        true
    }

    pub fn prewarm_text(
        &mut self,
        fonts: &HashMap<&'static str, font::Font>,
        font_name: &'static str,
        text: &str,
        wrap_width_pixels: Option<i32>,
    ) {
        let Some(font) = fonts.get(font_name) else {
            return;
        };
        let key = TextLayoutKey {
            font_key: font_chain_key(font, fonts),
            line_spacing: font.line_spacing,
            wrap_width_pixels: wrap_width_pixels.unwrap_or(-1),
        };
        let _ = self.get_or_build_owned(key, font, fonts, text);
    }

    fn get_or_build(
        &mut self,
        font: &font::Font,
        fonts: &HashMap<&'static str, font::Font>,
        content: &actors::TextContent,
        wrap_width_pixels: Option<i32>,
        line_spacing: Option<i32>,
    ) -> &CachedTextLayout {
        let key = TextLayoutKey {
            font_key: font_chain_key(font, fonts),
            line_spacing: line_spacing.unwrap_or(font.line_spacing),
            wrap_width_pixels: wrap_width_pixels.unwrap_or(-1),
        };
        match content {
            actors::TextContent::Static(text) => self.get_or_build_owned(key, font, fonts, text),
            actors::TextContent::Owned(text) => self.get_or_build_owned(key, font, fonts, text),
            actors::TextContent::Shared(text) => self.get_or_build_shared(key, font, fonts, text),
        }
    }

    fn get_or_build_owned(
        &mut self,
        key: TextLayoutKey,
        font: &font::Font,
        fonts: &HashMap<&'static str, font::Font>,
        text: &str,
    ) -> &CachedTextLayout {
        let tick = self.next_use_tick();
        if self.touch_owned_layout(key, text, tick) {
            self.frame_stats.owned_hits = self.frame_stats.owned_hits.saturating_add(1);
            return self
                .owned_layout(key, text)
                .expect("owned text layout cache entry touched");
        }
        let layout = Arc::new(build_cached_text_layout(
            font,
            fonts,
            text,
            key.line_spacing,
            key.wrap_width_pixels,
            text_layout_mesh_seed(key, text),
        ));
        self.record_layout_build(layout.as_ref());
        if self.insert_owned_layout(key, text, layout, tick) {
            self.owned_layout(key, text)
                .expect("owned text layout cache entry inserted")
        } else {
            self.uncached_layout_ref()
        }
    }

    fn get_or_build_shared(
        &mut self,
        key: TextLayoutKey,
        font: &font::Font,
        fonts: &HashMap<&'static str, font::Font>,
        text: &Arc<str>,
    ) -> &CachedTextLayout {
        let tick = self.next_use_tick();
        let text_key = Arc::as_ptr(text) as *const () as usize;
        let text_ref = text.as_ref();
        if self.touch_shared_layout(key, text_key, tick) {
            self.frame_stats.shared_hits = self.frame_stats.shared_hits.saturating_add(1);
            return self
                .shared_layout(key, text_key)
                .expect("shared text layout cache entry touched");
        }

        if self.touch_owned_layout(key, text_ref, tick) {
            self.frame_stats.owned_hits = self.frame_stats.owned_hits.saturating_add(1);
            if self.alias_count >= self.max_aliases
                && self.overflow_policy == TextLayoutOverflowPolicy::Saturating
            {
                return self
                    .owned_layout(key, text_ref)
                    .expect("owned text layout cache entry available");
            }
            let layout = Arc::clone(
                self.owned_layout_arc(key, text_ref)
                    .expect("owned text layout cache entry touched"),
            );
            if self.insert_shared_layout(key, text_key, Arc::clone(text), layout, tick) {
                return self
                    .shared_layout(key, text_key)
                    .expect("shared text layout cache entry inserted from owned layout");
            }
            return self
                .owned_layout(key, text_ref)
                .expect("owned text layout cache entry available");
        }

        let layout = Arc::new(build_cached_text_layout(
            font,
            fonts,
            text_ref,
            key.line_spacing,
            key.wrap_width_pixels,
            text_layout_mesh_seed(key, text_ref),
        ));
        self.record_layout_build(layout.as_ref());
        if self.insert_shared_layout(key, text_key, Arc::clone(text), layout, tick) {
            self.shared_layout(key, text_key)
                .expect("shared text layout cache entry inserted")
        } else {
            self.uncached_layout_ref()
        }
    }
}

#[inline(always)]
const fn saturating_u32(value: usize) -> u32 {
    if value > u32::MAX as usize {
        u32::MAX
    } else {
        value as u32
    }
}

fn font_chain_key(font: &font::Font, fonts: &HashMap<&'static str, font::Font>) -> u64 {
    if font.chain_key != 0 {
        return font.chain_key;
    }
    let mut hasher = DefaultHasher::new();
    let mut current = Some(font);
    while let Some(font) = current {
        (font as *const font::Font as usize).hash(&mut hasher);
        current = font.fallback_font_name.and_then(|name| fonts.get(name));
    }
    hasher.finish()
}

#[inline(always)]
fn text_layout_mesh_seed(key: TextLayoutKey, text: &str) -> u64 {
    let mut hasher = XxHash64::default();
    key.hash(&mut hasher);
    text.hash(&mut hasher);
    let seed = hasher.finish();
    if seed == renderer::INVALID_TMESH_CACHE_KEY {
        1
    } else {
        seed
    }
}

#[inline(always)]
fn text_batch_cache_key(
    layout_seed: u64,
    texture_key: *const str,
    stroke: bool,
    align: actors::TextAlign,
) -> u64 {
    let mut hasher = XxHash64::default();
    layout_seed.hash(&mut hasher);
    (texture_key as *const () as usize).hash(&mut hasher);
    stroke.hash(&mut hasher);
    match align {
        actors::TextAlign::Left => 0u8,
        actors::TextAlign::Center => 1u8,
        actors::TextAlign::Right => 2u8,
    }
    .hash(&mut hasher);
    let key = hasher.finish();
    if key == renderer::INVALID_TMESH_CACHE_KEY {
        layout_seed ^ 1
    } else {
        key
    }
}

#[inline(always)]
fn cached_glyph(
    _font: &font::Font,
    glyph: &font::Glyph,
    char_index: usize,
    draw_quad: bool,
) -> CachedGlyph {
    CachedGlyph {
        texture_key: std::ptr::from_ref(glyph.texture_key.as_ref()),
        stroke_texture_key: glyph
            .stroke_texture_key
            .as_ref()
            .map(|stroke_key| std::ptr::from_ref(stroke_key.as_ref())),
        uv_scale: glyph.uv_scale,
        uv_offset: glyph.uv_offset,
        size: glyph.size,
        offset: glyph.offset,
        advance_i32: glyph.advance_i32,
        char_index,
        fill_batch_index: 0,
        fill_vertex_start: 0,
        draw_quad,
    }
}

#[inline(always)]
fn glyph_has_fill_quad(glyph: &CachedGlyph) -> bool {
    glyph.draw_quad && glyph.size[0].abs() >= 1e-6 && glyph.size[1].abs() >= 1e-6
}

fn assign_fill_batch_slots(glyphs: &mut [CachedGlyph]) {
    let mut batches: SmallVec<[(*const str, u32); 8]> = SmallVec::new();
    for glyph in glyphs {
        if !glyph_has_fill_quad(glyph) {
            continue;
        }
        let batch_index = if let Some(index) = batches
            .iter()
            .position(|(texture_key, _)| std::ptr::addr_eq(*texture_key, glyph.texture_key))
        {
            index
        } else {
            batches.push((glyph.texture_key, 0));
            batches.len().saturating_sub(1)
        };
        let (_, next_vertex) = &mut batches[batch_index];
        glyph.fill_batch_index = batch_index as u32;
        glyph.fill_vertex_start = *next_vertex;
        *next_vertex = next_vertex.saturating_add(6);
    }
}

#[inline(always)]
fn start_x_logical(align: actors::TextAlign, block_w_logical: f32, line_w_logical: f32) -> i32 {
    let align_value = match align {
        actors::TextAlign::Left => 0.0,
        actors::TextAlign::Center => 0.5,
        actors::TextAlign::Right => 1.0,
    };
    let start = (-0.5f32).mul_add(
        block_w_logical,
        align_value * (block_w_logical - line_w_logical),
    );
    lrint_ties_even(start) as i32
}

#[inline(always)]
fn text_block_height_i(font_height: i32, line_spacing: i32, num_lines: usize) -> i32 {
    if num_lines > 1 {
        font_height + ((num_lines - 1) as i32 * line_spacing)
    } else {
        font_height
    }
}

fn resolve_text_layout_placement(
    layout: &CachedTextLayout,
    scale: [f32; 2],
    fit_width: Option<f32>,
    fit_height: Option<f32>,
    max_width: Option<f32>,
    max_height: Option<f32>,
    max_w_pre_zoom: bool,
    max_h_pre_zoom: bool,
    parent: SmRect,
    align: [f32; 2],
    offset: [f32; 2],
) -> Option<TextLayoutPlacement> {
    let num_lines = layout.lines.len();
    if num_lines == 0 {
        return None;
    }

    let block_w_logical_even = quantize_up_even_i32(layout.max_logical_width_i) as f32;
    let cap_height = if layout.font_height > 0 {
        layout.font_height as f32
    } else {
        layout.line_spacing as f32
    };
    let block_h_logical_i = text_block_height_i(layout.font_height, layout.line_spacing, num_lines);
    let block_h_logical = if block_h_logical_i > 0 {
        block_h_logical_i as f32
    } else {
        cap_height
    };

    let s_w_fit = fit_width.map_or(f32::INFINITY, |w| {
        if block_w_logical_even > 0.0 {
            w / block_w_logical_even
        } else {
            1.0
        }
    });
    let s_h_fit = fit_height.map_or(f32::INFINITY, |h| {
        if block_h_logical > 0.0 {
            h / block_h_logical
        } else {
            1.0
        }
    });
    let fit_s = if s_w_fit.is_infinite() && s_h_fit.is_infinite() {
        1.0
    } else {
        s_w_fit.min(s_h_fit).max(0.0)
    };

    let width_before_zoom = block_w_logical_even * fit_s;
    let height_before_zoom = block_h_logical * fit_s;
    let width_after_zoom = width_before_zoom * scale[0];
    let height_after_zoom = height_before_zoom * scale[1];

    let denom_w_for_max = if max_w_pre_zoom {
        width_before_zoom
    } else {
        width_after_zoom
    };
    let denom_h_for_max = if max_h_pre_zoom {
        height_before_zoom
    } else {
        height_after_zoom
    };

    let max_s_w = max_width.map_or(1.0, |mw| {
        if denom_w_for_max > mw {
            (mw / denom_w_for_max).max(0.0)
        } else {
            1.0
        }
    });
    let max_s_h = max_height.map_or(1.0, |mh| {
        if denom_h_for_max > mh {
            (mh / denom_h_for_max).max(0.0)
        } else {
            1.0
        }
    });

    let sx = scale[0] * fit_s * max_s_w;
    let sy = scale[1] * fit_s * max_s_h;
    if sx.abs() < 1e-6 || sy.abs() < 1e-6 {
        return None;
    }

    let block_w_px = block_w_logical_even * sx;
    let block_h_px = block_h_logical * sy;
    let block_left_sm = align[0].mul_add(-block_w_px, parent.x + offset[0]);
    let block_top_sm = align[1].mul_add(-block_h_px, parent.y + offset[1]);

    Some(TextLayoutPlacement {
        sx,
        sy,
        block_center_x: 0.5f32.mul_add(block_w_px, block_left_sm),
        block_center_y: 0.5f32.mul_add(block_h_px, block_top_sm),
    })
}

#[inline(always)]
fn push_cached_line(
    lines: &mut Vec<CachedLine>,
    max_logical_width_i: &mut i32,
    width_i32: i32,
    glyph_start: usize,
    glyph_end: usize,
) {
    *max_logical_width_i = (*max_logical_width_i).max(width_i32);
    lines.push(CachedLine {
        width_i32,
        glyph_start,
        glyph_len: glyph_end.saturating_sub(glyph_start),
    });
}

#[inline(always)]
fn attr_end(attr: &actors::TextAttribute) -> usize {
    attr.start.saturating_add(attr.length)
}

struct TextAttrCursor<'a> {
    attributes: &'a [actors::TextAttribute],
    start_order: AttrIndices,
    end_order: AttrIndices,
    active: AttrIndices,
    active_max: Option<usize>,
    next_start: usize,
    next_end: usize,
}

impl<'a> TextAttrCursor<'a> {
    fn new(attributes: &'a [actors::TextAttribute]) -> Option<Self> {
        if attributes.is_empty() {
            return None;
        }

        let mut start_order = AttrIndices::with_capacity(attributes.len());
        let mut end_order = AttrIndices::with_capacity(attributes.len());
        for index in 0..attributes.len() {
            start_order.push(index);
            end_order.push(index);
        }

        start_order.sort_unstable_by_key(|&index| (attributes[index].start, index));
        end_order.sort_unstable_by_key(|&index| (attr_end(&attributes[index]), index));

        Some(Self {
            attributes,
            start_order,
            end_order,
            active: AttrIndices::new(),
            active_max: None,
            next_start: 0,
            next_end: 0,
        })
    }

    #[inline(always)]
    fn push_active(&mut self, attr_index: usize) {
        self.active.push(attr_index);
        self.active_max = Some(
            self.active_max
                .map_or(attr_index, |max| max.max(attr_index)),
        );
    }

    #[inline(always)]
    fn remove_active(&mut self, attr_index: usize) {
        let Some(index) = self.active.iter().position(|&index| index == attr_index) else {
            return;
        };
        self.active.swap_remove(index);
        if self.active_max == Some(attr_index) {
            self.active_max = self.active.iter().copied().max();
        }
    }

    #[inline(always)]
    fn colors_for(&mut self, char_index: usize) -> [[f32; 4]; 4] {
        while self.next_end < self.end_order.len()
            && attr_end(&self.attributes[self.end_order[self.next_end]]) <= char_index
        {
            let attr_index = self.end_order[self.next_end];
            self.remove_active(attr_index);
            self.next_end += 1;
        }

        while self.next_start < self.start_order.len()
            && self.attributes[self.start_order[self.next_start]].start <= char_index
        {
            let attr_index = self.start_order[self.next_start];
            let attr = &self.attributes[attr_index];
            if char_index < attr_end(attr) {
                self.push_active(attr_index);
            }
            self.next_start += 1;
        }

        self.active_max
            .map(|index| self.attributes[index].colors())
            .unwrap_or([[1.0; 4]; 4])
    }

    #[cfg(test)]
    fn tint_for(&mut self, char_index: usize) -> [f32; 4] {
        self.colors_for(char_index)[0]
    }
}

fn flush_wrapped_word(
    font: &font::Font,
    lines: &mut Vec<CachedLine>,
    max_logical_width_i: &mut i32,
    line_width: &mut i32,
    line_glyph_start: &mut usize,
    glyphs: &mut Vec<CachedGlyph>,
    line_has_word: &mut bool,
    word_active: &mut bool,
    word_width: &mut i32,
    word_first_char: usize,
    word_space_before: &mut Option<usize>,
    word_glyphs: &mut WordGlyphs,
    wrap_width_pixels: i32,
    space_glyph: Option<&font::Glyph>,
    space_width: i32,
    draws_space: bool,
) {
    if !*word_active {
        return;
    }

    if !*line_has_word {
        *line_width = *word_width;
        glyphs.extend(word_glyphs.drain(..));
        *line_has_word = true;
    } else if line_width.saturating_add(space_width + *word_width) <= wrap_width_pixels {
        *line_width += space_width + *word_width;
        if let Some(glyph) = space_glyph {
            glyphs.push(cached_glyph(
                font,
                glyph,
                word_space_before.unwrap_or(word_first_char.saturating_sub(1)),
                draws_space,
            ));
        }
        glyphs.extend(word_glyphs.drain(..));
    } else {
        push_cached_line(
            lines,
            max_logical_width_i,
            *line_width,
            *line_glyph_start,
            glyphs.len(),
        );
        *line_glyph_start = glyphs.len();
        *line_width = *word_width;
        glyphs.extend(word_glyphs.drain(..));
        *line_has_word = true;
    }

    *word_active = false;
    *word_width = 0;
    *word_space_before = None;
}

struct TextMeshBatchBuilder {
    texture_key: *const str,
    vertices: Vec<renderer::TexturedMeshVertex>,
}

#[inline(always)]
fn take_recycled_text_mesh_vertices(
    recycled_vertices: &mut Vec<Vec<renderer::TexturedMeshVertex>>,
) -> Vec<renderer::TexturedMeshVertex> {
    recycled_vertices.pop().unwrap_or_default()
}

#[inline(always)]
fn text_mesh_batch_builder<'a>(
    builders: &'a mut Vec<TextMeshBatchBuilder>,
    recycled_vertices: &mut Vec<Vec<renderer::TexturedMeshVertex>>,
    texture_key: *const str,
) -> &'a mut TextMeshBatchBuilder {
    if let Some(index) = builders
        .iter()
        .position(|builder| std::ptr::addr_eq(builder.texture_key, texture_key))
    {
        return &mut builders[index];
    }
    builders.push(TextMeshBatchBuilder {
        texture_key,
        vertices: take_recycled_text_mesh_vertices(recycled_vertices),
    });
    builders
        .last_mut()
        .expect("text batch builder inserted for texture page")
}

#[inline(always)]
fn push_text_mesh_quad_with_color(
    builders: &mut Vec<TextMeshBatchBuilder>,
    recycled_vertices: &mut Vec<Vec<renderer::TexturedMeshVertex>>,
    texture_key: *const str,
    quad_x: f32,
    quad_y: f32,
    size: [f32; 2],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    color: [f32; 4],
) {
    let out = &mut text_mesh_batch_builder(builders, recycled_vertices, texture_key).vertices;
    let x0 = quad_x;
    let y0 = quad_y;
    let x1 = quad_x + size[0];
    let y1 = quad_y + size[1];
    let u0 = uv_offset[0];
    let v0 = uv_offset[1];
    let u1 = uv_offset[0] + uv_scale[0];
    let v1 = uv_offset[1] + uv_scale[1];
    let tex_matrix_scale = [1.0, 1.0];

    out.reserve(6);
    out.push(renderer::TexturedMeshVertex {
        pos: [x0, y0, 0.0],
        uv: [u0, v0],
        tex_matrix_scale,
        color,
    });
    out.push(renderer::TexturedMeshVertex {
        pos: [x0, y1, 0.0],
        uv: [u0, v1],
        tex_matrix_scale,
        color,
    });
    out.push(renderer::TexturedMeshVertex {
        pos: [x1, y1, 0.0],
        uv: [u1, v1],
        tex_matrix_scale,
        color,
    });
    out.push(renderer::TexturedMeshVertex {
        pos: [x0, y0, 0.0],
        uv: [u0, v0],
        tex_matrix_scale,
        color,
    });
    out.push(renderer::TexturedMeshVertex {
        pos: [x1, y1, 0.0],
        uv: [u1, v1],
        tex_matrix_scale,
        color,
    });
    out.push(renderer::TexturedMeshVertex {
        pos: [x1, y0, 0.0],
        uv: [u1, v0],
        tex_matrix_scale,
        color,
    });
}

#[inline(always)]
fn push_text_mesh_quad(
    builders: &mut Vec<TextMeshBatchBuilder>,
    recycled_vertices: &mut Vec<Vec<renderer::TexturedMeshVertex>>,
    texture_key: *const str,
    quad_x: f32,
    quad_y: f32,
    size: [f32; 2],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
) {
    push_text_mesh_quad_with_color(
        builders,
        recycled_vertices,
        texture_key,
        quad_x,
        quad_y,
        size,
        uv_scale,
        uv_offset,
        [1.0; 4],
    );
}

fn finish_text_mesh_batches(
    builders: Vec<TextMeshBatchBuilder>,
    layout_seed: u64,
    stroke: bool,
    align: actors::TextAlign,
) -> Vec<CachedTextMeshBatch> {
    let mut out = Vec::with_capacity(builders.len());
    for builder in builders {
        if builder.vertices.is_empty() {
            continue;
        }
        out.push(CachedTextMeshBatch {
            texture_key: builder.texture_key,
            geom_cache_key: text_batch_cache_key(layout_seed, builder.texture_key, stroke, align),
            vertices: Arc::from(builder.vertices),
        });
    }
    out
}

fn build_text_mesh_batches_for_align(
    layout_seed: u64,
    font_height: i32,
    line_spacing: i32,
    max_logical_width_i: i32,
    lines: &[CachedLine],
    glyphs: &[CachedGlyph],
    align: actors::TextAlign,
    stroke: bool,
) -> Vec<CachedTextMeshBatch> {
    if lines.is_empty() || glyphs.is_empty() {
        return Vec::new();
    }

    let block_w_logical_even = quantize_up_even_i32(max_logical_width_i) as f32;
    let block_h_logical_i = if lines.len() > 1 {
        font_height + ((lines.len() - 1) as i32 * line_spacing)
    } else {
        font_height
    };
    let mut pen_y_logical = lrint_ties_even(-(block_h_logical_i as f32) * 0.5) as i32;
    let line_padding = line_spacing - font_height;
    let mut builders = Vec::new();
    let mut recycled_vertices = Vec::new();

    for line in lines {
        pen_y_logical += font_height;
        let baseline_local_logical = pen_y_logical as f32;
        let mut pen_x_logical = start_x_logical(align, block_w_logical_even, line.width_i32 as f32);

        let line_glyphs =
            &glyphs[line.glyph_start..line.glyph_start.saturating_add(line.glyph_len)];
        for glyph in line_glyphs {
            let texture_key = if stroke {
                glyph.stroke_texture_key
            } else if glyph_has_fill_quad(glyph) {
                Some(glyph.texture_key)
            } else {
                None
            };
            let Some(texture_key) = texture_key else {
                pen_x_logical += glyph.advance_i32;
                continue;
            };

            let quad_x_logical = pen_x_logical as f32 + glyph.offset[0];
            let quad_y_logical = baseline_local_logical + glyph.offset[1];
            push_text_mesh_quad(
                &mut builders,
                &mut recycled_vertices,
                texture_key,
                quad_x_logical,
                quad_y_logical,
                glyph.size,
                glyph.uv_scale,
                glyph.uv_offset,
            );
            pen_x_logical += glyph.advance_i32;
        }
        pen_y_logical += line_padding;
    }

    finish_text_mesh_batches(builders, layout_seed, stroke, align)
}

fn push_transient_text_mesh_quad(
    builders: &mut Vec<TextMeshBatchBuilder>,
    recycled_vertices: &mut Vec<Vec<renderer::TexturedMeshVertex>>,
    texture_key: *const str,
    quad_x: f32,
    quad_y: f32,
    size: [f32; 2],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    corner_colors: [[f32; 4]; 4],
    jitter_offset: Option<[f32; 2]>,
    distortion: f32,
    char_index: usize,
) {
    const CORNERS: [usize; 6] = [0, 2, 3, 0, 3, 1];
    let out = &mut text_mesh_batch_builder(builders, recycled_vertices, texture_key).vertices;
    let x0 = quad_x;
    let y0 = quad_y;
    let x1 = quad_x + size[0];
    let y1 = quad_y + size[1];
    let u0 = uv_offset[0];
    let v0 = uv_offset[1];
    let u1 = uv_offset[0] + uv_scale[0];
    let v1 = uv_offset[1] + uv_scale[1];
    let positions = [[x0, y0, 0.0], [x1, y0, 0.0], [x0, y1, 0.0], [x1, y1, 0.0]];
    let uvs = [[u0, v0], [u1, v0], [u0, v1], [u1, v1]];
    let tex_matrix_scale = [1.0, 1.0];
    out.reserve(6);
    for corner in CORNERS {
        let mut pos = positions[corner];
        if distortion.abs() > 1e-6 {
            let [dx, dy] = text_distortion_offset(distortion, char_index, corner, size[0], size[1]);
            pos[0] += dx;
            pos[1] += dy;
        }
        if let Some([dx, dy]) = jitter_offset {
            pos[0] += dx;
            pos[1] += dy;
        }
        out.push(renderer::TexturedMeshVertex {
            pos,
            uv: uvs[corner],
            tex_matrix_scale,
            color: corner_colors[corner],
        });
    }
}

fn build_transient_text_mesh_builders(
    layout: &CachedTextLayout,
    text_align: actors::TextAlign,
    attributes: &[actors::TextAttribute],
    jitter_seed: Option<u32>,
    distortion: f32,
    stroke: bool,
    builders: &mut Vec<TextMeshBatchBuilder>,
    recycled_vertices: &mut Vec<Vec<renderer::TexturedMeshVertex>>,
) {
    builders.clear();
    if layout.lines.is_empty() || layout.glyphs.is_empty() {
        return;
    }

    let block_w_logical_even = quantize_up_even_i32(layout.max_logical_width_i) as f32;
    let block_h_logical_i = if layout.lines.len() > 1 {
        layout.font_height + ((layout.lines.len() - 1) as i32 * layout.line_spacing)
    } else {
        layout.font_height
    };
    let mut pen_y_logical = lrint_ties_even(-(block_h_logical_i as f32) * 0.5) as i32;
    let line_padding = layout.line_spacing - layout.font_height;
    let mut attr_cursor = (!stroke).then(|| TextAttrCursor::new(attributes)).flatten();

    for line in &layout.lines {
        pen_y_logical += layout.font_height;
        let baseline_local_logical = pen_y_logical as f32;
        let mut pen_x_logical =
            start_x_logical(text_align, block_w_logical_even, line.width_i32 as f32);
        let line_glyphs =
            &layout.glyphs[line.glyph_start..line.glyph_start.saturating_add(line.glyph_len)];
        for glyph in line_glyphs {
            let texture_key = if stroke {
                glyph.stroke_texture_key
            } else if glyph_has_fill_quad(glyph) {
                Some(glyph.texture_key)
            } else {
                None
            };
            let Some(texture_key) = texture_key else {
                pen_x_logical += glyph.advance_i32;
                continue;
            };
            let colors = attr_cursor
                .as_mut()
                .map_or([[1.0; 4]; 4], |cursor| cursor.colors_for(glyph.char_index));
            push_transient_text_mesh_quad(
                builders,
                recycled_vertices,
                texture_key,
                pen_x_logical as f32 + glyph.offset[0],
                baseline_local_logical + glyph.offset[1],
                glyph.size,
                glyph.uv_scale,
                glyph.uv_offset,
                colors,
                jitter_seed.map(|seed| text_jitter_offset(seed, glyph.char_index)),
                distortion,
                glyph.char_index,
            );
            pen_x_logical += glyph.advance_i32;
        }
        pen_y_logical += line_padding;
    }
}

#[inline(always)]
fn text_jitter_offset(seed: u32, char_index: usize) -> [f32; 2] {
    let mut value = seed.wrapping_mul(0x9e37_79b9);
    value ^= (char_index as u32).wrapping_mul(0x85eb_ca6b);
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    [(value & 1) as f32, ((value >> 1) % 3) as f32]
}

#[inline(always)]
fn text_distortion_offset(
    amount: f32,
    char_index: usize,
    corner: usize,
    width: f32,
    height: f32,
) -> [f32; 2] {
    let mut value = 0xa24b_aed4_u32;
    value ^= (char_index as u32).wrapping_mul(0x9e37_79b9);
    value ^= (corner as u32).wrapping_mul(0x85eb_ca6b);
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    let x = ((value % 9) as f32 / 8.0 - 0.5) * amount * width;
    value = value.rotate_left(13).wrapping_mul(0x846c_a68b);
    let y = ((value % 9) as f32 / 8.0 - 0.5) * amount * height;
    [x, y]
}

fn build_cached_text_layout(
    font: &font::Font,
    fonts: &HashMap<&'static str, font::Font>,
    text: &str,
    line_spacing: i32,
    wrap_width_pixels: i32,
    layout_seed: u64,
) -> CachedTextLayout {
    let draws_space = font.glyph_map.contains_key(&' ');
    let space_glyph = font::find_glyph(font, ' ', fonts);
    let space_width = space_glyph.map_or(0, |glyph| glyph.advance_i32);
    let mut max_logical_width_i = 0i32;
    let mut lines = Vec::with_capacity(
        text.as_bytes()
            .iter()
            .filter(|&&b| b == b'\n')
            .count()
            .saturating_add(1),
    );
    let mut glyphs = Vec::with_capacity(text.len());
    let mut start_char = 0usize;

    for src in text.split('\n') {
        let mut char_index = start_char;
        if wrap_width_pixels < 0 {
            let mut width_i32 = 0i32;
            let line_glyph_start = glyphs.len();
            for ch in src.chars() {
                if let Some(glyph) = font::find_glyph(font, ch, fonts) {
                    width_i32 += glyph.advance_i32;
                    glyphs.push(cached_glyph(
                        font,
                        glyph,
                        char_index,
                        ch != ' ' || draws_space,
                    ));
                }
                char_index += 1;
            }
            push_cached_line(
                &mut lines,
                &mut max_logical_width_i,
                width_i32,
                line_glyph_start,
                glyphs.len(),
            );
            start_char = char_index.saturating_add(1);
            continue;
        }

        let mut line_width = 0i32;
        let mut line_glyph_start = glyphs.len();
        let mut line_has_word = false;
        let mut pending_space = None;
        let mut word_active = false;
        let mut word_width = 0i32;
        let mut word_first_char = start_char;
        let mut word_space_before = None;
        let mut word_glyphs = WordGlyphs::new();

        for ch in src.chars() {
            if ch == ' ' {
                flush_wrapped_word(
                    font,
                    &mut lines,
                    &mut max_logical_width_i,
                    &mut line_width,
                    &mut line_glyph_start,
                    &mut glyphs,
                    &mut line_has_word,
                    &mut word_active,
                    &mut word_width,
                    word_first_char,
                    &mut word_space_before,
                    &mut word_glyphs,
                    wrap_width_pixels,
                    space_glyph,
                    space_width,
                    draws_space,
                );
                pending_space.get_or_insert(char_index);
            } else {
                if !word_active {
                    word_active = true;
                    word_first_char = char_index;
                    word_space_before = pending_space.take();
                }
                if let Some(glyph) = font::find_glyph(font, ch, fonts) {
                    word_width += glyph.advance_i32;
                    word_glyphs.push(cached_glyph(font, glyph, char_index, true));
                }
            }
            char_index += 1;
        }

        flush_wrapped_word(
            font,
            &mut lines,
            &mut max_logical_width_i,
            &mut line_width,
            &mut line_glyph_start,
            &mut glyphs,
            &mut line_has_word,
            &mut word_active,
            &mut word_width,
            word_first_char,
            &mut word_space_before,
            &mut word_glyphs,
            wrap_width_pixels,
            space_glyph,
            space_width,
            draws_space,
        );

        if line_has_word {
            push_cached_line(
                &mut lines,
                &mut max_logical_width_i,
                line_width,
                line_glyph_start,
                glyphs.len(),
            );
        } else {
            push_cached_line(
                &mut lines,
                &mut max_logical_width_i,
                0,
                glyphs.len(),
                glyphs.len(),
            );
        }
        start_char = char_index.saturating_add(1);
    }

    assign_fill_batch_slots(&mut glyphs);

    CachedTextLayout {
        layout_seed,
        font_height: font.height,
        line_spacing,
        max_logical_width_i,
        glyph_count: glyphs.len(),
        lines,
        glyphs,
        fill_batches: CachedTextMeshVariants::default(),
        stroke_batches: CachedTextMeshVariants::default(),
    }
}

#[cfg(test)]
fn wrap_text_lines_by_words<F>(
    text: &str,
    wrap_width_pixels: i32,
    space_width: i32,
    mut word_width: F,
) -> Vec<Box<str>>
where
    F: FnMut(&str) -> i32,
{
    let mut out = Vec::new();
    for src in text.split('\n') {
        if wrap_width_pixels < 0 {
            out.push(src.into());
            continue;
        }
        let mut words = src.split(' ').filter(|word| !word.is_empty());
        let Some(first) = words.next() else {
            out.push("".into());
            continue;
        };
        let mut line = String::from(first);
        let mut line_width = word_width(first);
        for word in words {
            let width_to_add = space_width + word_width(word);
            if line_width + width_to_add <= wrap_width_pixels {
                line.push(' ');
                line.push_str(word);
                line_width += width_to_add;
            } else {
                out.push(line.into_boxed_str());
                line = word.to_owned();
                line_width = word_width(word);
            }
        }
        out.push(line.into_boxed_str());
    }
    out
}

#[inline(always)]
// SAFETY: Callers must only pass pointers captured from cached string storage that remains valid
// and immutable for at least the lifetime of the returned borrow.
unsafe fn str_from_cached_ptr<'a>(ptr: *const str) -> &'a str {
    // SAFETY: callers only pass pointers captured from cached font glyph storage
    // that outlives the returned borrow for the duration of render-list assembly.
    unsafe { &*ptr }
}

/* ======================= ACTOR -> OBJECT CONVERSION ======================= */

#[derive(Clone, Copy)]
struct SmRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

#[inline(always)]
fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    (b - a).mul_add(t, a)
}

#[inline(always)]
fn apply_effect_to_sprite(
    effect: anim::EffectState,
    elapsed: f32,
    tint: &mut [f32; 4],
    scale: &mut [f32; 2],
    rot_deg: &mut [f32; 3],
) {
    // We currently don't have song beat/time split plumbed here, so use elapsed for both.
    let beat = elapsed;
    if matches!(effect.mode, anim::EffectMode::Spin) {
        // ITGmania spin uses effect delta from clock and does not use effectoffset.
        let units = anim::effect_clock_units(effect, elapsed, beat);
        rot_deg[0] = (rot_deg[0] + effect.magnitude[0] * units).rem_euclid(360.0);
        rot_deg[1] = (rot_deg[1] + effect.magnitude[1] * units).rem_euclid(360.0);
        rot_deg[2] = (rot_deg[2] + effect.magnitude[2] * units).rem_euclid(360.0);
    }

    if let Some(percent) = anim::effect_mix(effect, elapsed, beat) {
        match effect.mode {
            anim::EffectMode::DiffuseRamp => {
                for (i, out) in tint.iter_mut().enumerate() {
                    let c = lerp_f32(effect.color2[i], effect.color1[i], percent).clamp(0.0, 1.0);
                    *out *= c;
                }
            }
            anim::EffectMode::DiffuseShift => {
                let between = (((percent + 0.25) * 2.0 * std::f32::consts::PI).sin() * 0.5 + 0.5)
                    .clamp(0.0, 1.0);
                for (i, out) in tint.iter_mut().enumerate() {
                    let c = lerp_f32(effect.color2[i], effect.color1[i], between).clamp(0.0, 1.0);
                    *out *= c;
                }
            }
            anim::EffectMode::Pulse => {
                let offset = (percent * std::f32::consts::PI).sin().clamp(0.0, 1.0);
                let zoom = lerp_f32(effect.magnitude[0], effect.magnitude[1], offset).max(0.0);
                let sx = lerp_f32(effect.color2[0], effect.color1[0], offset).max(0.0);
                let sy = lerp_f32(effect.color2[1], effect.color1[1], offset).max(0.0);
                scale[0] *= zoom * sx;
                scale[1] *= zoom * sy;
            }
            anim::EffectMode::GlowShift
            | anim::EffectMode::Bob
            | anim::EffectMode::Bounce
            | anim::EffectMode::Wag
            | anim::EffectMode::Spin
            | anim::EffectMode::None => {}
        }
    }

    tint[0] = tint[0].clamp(0.0, 1.0);
    tint[1] = tint[1].clamp(0.0, 1.0);
    tint[2] = tint[2].clamp(0.0, 1.0);
    tint[3] = tint[3].clamp(0.0, 1.0);
    scale[0] = scale[0].max(0.0);
    scale[1] = scale[1].max(0.0);
}

#[inline(always)]
fn apply_effect_to_text(
    effect: anim::EffectState,
    elapsed: f32,
    color: &mut [f32; 4],
    scale: &mut [f32; 2],
) {
    // We currently don't have song beat/time split plumbed here, so use elapsed for both.
    let beat = elapsed;
    if let Some(percent) = anim::effect_mix(effect, elapsed, beat) {
        match effect.mode {
            anim::EffectMode::DiffuseRamp => {
                for (i, out) in color.iter_mut().enumerate() {
                    let c = lerp_f32(effect.color2[i], effect.color1[i], percent).clamp(0.0, 1.0);
                    *out *= c;
                }
            }
            anim::EffectMode::DiffuseShift => {
                let between = (((percent + 0.25) * 2.0 * std::f32::consts::PI).sin() * 0.5 + 0.5)
                    .clamp(0.0, 1.0);
                for (i, out) in color.iter_mut().enumerate() {
                    let c = lerp_f32(effect.color2[i], effect.color1[i], between).clamp(0.0, 1.0);
                    *out *= c;
                }
            }
            anim::EffectMode::Pulse => {
                let offset = (percent * std::f32::consts::PI).sin().clamp(0.0, 1.0);
                let zoom = lerp_f32(effect.magnitude[0], effect.magnitude[1], offset).max(0.0);
                let sx = lerp_f32(effect.color2[0], effect.color1[0], offset).max(0.0);
                let sy = lerp_f32(effect.color2[1], effect.color1[1], offset).max(0.0);
                scale[0] *= zoom * sx;
                scale[1] *= zoom * sy;
            }
            anim::EffectMode::GlowShift
            | anim::EffectMode::Bob
            | anim::EffectMode::Bounce
            | anim::EffectMode::Wag
            | anim::EffectMode::Spin
            | anim::EffectMode::None => {}
        }
    }

    color[0] = color[0].clamp(0.0, 1.0);
    color[1] = color[1].clamp(0.0, 1.0);
    color[2] = color[2].clamp(0.0, 1.0);
    color[3] = color[3].clamp(0.0, 1.0);
    scale[0] = scale[0].max(0.0);
    scale[1] = scale[1].max(0.0);
}

#[inline(always)]
fn has_shadow(len: [f32; 2]) -> bool {
    len[0] != 0.0 || len[1] != 0.0
}

fn push_shadow_objects_for_range(
    out: &mut Vec<RenderObject>,
    start: usize,
    end: usize,
    len: [f32; 2],
    color: [f32; 4],
) {
    let t_world = Matrix4::from_translation(Vector3::new(len[0], len[1], 0.0));
    for i in start..end {
        let obj = &out[i];
        let mut obj_type = obj.object_type.clone();
        match &mut obj_type {
            renderer::ObjectType::Sprite { center, tint, .. } => {
                let mut shadow_tint = color;
                shadow_tint[3] *= (*tint)[3];
                *tint = shadow_tint;
                center[0] += len[0];
                center[1] += len[1];
            }
            renderer::ObjectType::Mesh { tint, .. } => {
                tint[0] *= color[0];
                tint[1] *= color[1];
                tint[2] *= color[2];
                tint[3] *= color[3];
            }
            renderer::ObjectType::TexturedMesh { tint, .. } => {
                let mut shadow_tint = color;
                shadow_tint[0] *= tint[0];
                shadow_tint[1] *= tint[1];
                shadow_tint[2] *= tint[2];
                shadow_tint[3] *= tint[3];
                *tint = shadow_tint;
            }
        }

        out.push(renderer::RenderObject {
            object_type: obj_type,
            texture_handle: obj.texture_handle,
            transform: match &obj.object_type {
                renderer::ObjectType::Sprite { .. } => obj.transform,
                _ => t_world * obj.transform,
            },
            blend: obj.blend,
            z: obj.z.saturating_sub(1),
            order: obj.order,
            camera: obj.camera,
        });
    }
}

#[inline(always)]
fn build_actor_recursive<'a>(
    actor: &'a actors::Actor,
    parent: SmRect,
    m: &Metrics,
    fonts: &'a HashMap<&'static str, font::Font>,
    scratch: &mut ComposeScratch,
    base_z: i16,
    camera: u8,
    cameras: &mut Vec<Matrix4>,
    masks: &mut Vec<WorldRect>,
    order_counter: &mut u32,
    out: &mut Vec<RenderObject>,
    text_cache: &mut TextLayoutCache,
    texture_cache: &mut TextureLookupCache,
    total_elapsed: f32,
) {
    match actor {
        actors::Actor::Sprite {
            align,
            offset,
            world_z,
            size,
            source,
            tint,
            z,
            cell,
            grid,
            uv_rect,
            visible,
            flip_x,
            flip_y,
            cropleft,
            cropright,
            croptop,
            cropbottom,
            blend,
            mask_source,
            mask_dest,
            glow: _,
            fadeleft,
            faderight,
            fadetop,
            fadebottom,
            rot_z_deg,
            rot_x_deg,
            rot_y_deg,
            local_offset,
            local_offset_rot_sin_cos,
            texcoordvelocity,
            animate,
            state_delay,
            scale,
            shadow_len,
            shadow_color,
            effect,
        } => {
            if !*visible {
                return;
            }

            let (is_solid, texture_name, texture_key_ptr, texture_key_stable) = match source {
                actors::SpriteSource::TextureStatic(name) => {
                    (false, *name, Some(str_ptr(name)), true)
                }
                actors::SpriteSource::Texture(name) => {
                    let name = name.as_ref();
                    (false, name, Some(str_ptr(name)), false)
                }
                actors::SpriteSource::Solid => (true, "__white", Some(str_ptr("__white")), true),
            };

            let mut chosen_cell = *cell;
            let mut chosen_grid = *grid;

            if !is_solid && uv_rect.is_none() {
                let (cols, rows) = grid.unwrap_or_else(|| {
                    texture_cache.sprite_sheet_dims_cached(
                        texture_key_ptr,
                        texture_name,
                        texture_key_stable,
                    )
                });
                let total = cols.saturating_mul(rows).max(1);

                let start_linear: u32 = match *cell {
                    Some((cx, cy)) if cy != u32::MAX => {
                        let cx = cx.min(cols.saturating_sub(1));
                        let cy = cy.min(rows.saturating_sub(1));
                        cy.saturating_mul(cols).saturating_add(cx)
                    }
                    Some((i, _)) => i,
                    None => 0,
                };

                if *animate && *state_delay > 0.0 && total > 1 {
                    let steps = (total_elapsed / *state_delay).floor().max(0.0) as u32;
                    let idx = (start_linear + (steps % total)) % total;
                    chosen_cell = Some((idx, u32::MAX));
                    chosen_grid = Some((cols, rows));
                } else if chosen_cell.is_none() && total > 1 {
                    chosen_cell = Some((0, u32::MAX));
                    chosen_grid = Some((cols, rows));
                }
            }

            let mut effect_tint = *tint;
            let mut effect_scale = *scale;
            let mut effect_rot = [*rot_x_deg, *rot_y_deg, *rot_z_deg];
            apply_effect_to_sprite(
                *effect,
                total_elapsed,
                &mut effect_tint,
                &mut effect_scale,
                &mut effect_rot,
            );

            let resolved_size = resolve_sprite_size_like_sm(
                *size,
                is_solid,
                texture_name,
                texture_key_ptr,
                texture_key_stable,
                *uv_rect,
                chosen_cell,
                chosen_grid,
                effect_scale,
                texture_cache,
            );

            let rect = place_rect(parent, *align, *offset, resolved_size);
            let mask_rect = sm_rect_to_world_edges(rect, m);
            if *mask_source {
                masks.push(mask_rect);
            }
            if *mask_source && !*mask_dest {
                return;
            }
            if *mask_dest && masks.is_empty() {
                return;
            }

            let before = out.len();
            push_sprite(
                out,
                camera,
                rect,
                m,
                is_solid,
                texture_name,
                texture_key_ptr,
                texture_key_stable,
                effect_tint,
                *uv_rect,
                chosen_cell,
                chosen_grid,
                *flip_x,
                *flip_y,
                *cropleft,
                *cropright,
                *croptop,
                *cropbottom,
                *fadeleft,
                *faderight,
                *fadetop,
                *fadebottom,
                *blend,
                effect_rot[0],
                effect_rot[1],
                effect_rot[2],
                *world_z,
                *local_offset,
                *local_offset_rot_sin_cos,
                *texcoordvelocity,
                texture_cache,
                total_elapsed,
            );
            if *mask_dest {
                clip_objects_range_to_world_masks(out, before, masks);
            }

            let end = out.len();
            let layer = base_z.saturating_add(*z);
            for obj in out.iter_mut().take(end).skip(before) {
                obj.z = layer;
                obj.order = {
                    let o = *order_counter;
                    *order_counter += 1;
                    o
                };
            }
            if has_shadow(*shadow_len) {
                push_shadow_objects_for_range(out, before, end, *shadow_len, *shadow_color);
            }
        }

        actors::Actor::Mesh {
            align,
            offset,
            size,
            vertices,
            mode,
            visible,
            blend,
            z,
        } => {
            if !*visible || vertices.is_empty() {
                return;
            }

            let rect = place_rect(parent, *align, *offset, *size);
            let base_x = m.left + rect.x;
            let base_y = m.top - rect.y;
            let transform = Matrix4::from_translation(Vector3::new(base_x, base_y, 0.0))
                * Matrix4::from_scale(Vector3::new(1.0, -1.0, 1.0));

            let before = out.len();
            out.push(renderer::RenderObject {
                object_type: renderer::ObjectType::Mesh {
                    tint: [1.0; 4],
                    vertices: Arc::clone(vertices),
                    mode: *mode,
                },
                texture_handle: renderer::INVALID_TEXTURE_HANDLE,
                transform,
                blend: *blend,
                z: 0,
                order: 0,
                camera,
            });

            let layer = base_z.saturating_add(*z);
            for obj in out.iter_mut().skip(before) {
                obj.z = layer;
                obj.order = {
                    let o = *order_counter;
                    *order_counter += 1;
                    o
                };
            }
        }

        actors::Actor::TexturedMesh {
            align,
            offset,
            world_z,
            size,
            local_transform,
            texture,
            tint,
            vertices,
            geom_cache_key,
            mode,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test,
            visible,
            blend,
            z,
        } => {
            if !*visible || vertices.is_empty() {
                return;
            }

            let rect = place_rect(parent, *align, *offset, *size);
            let base_x = m.left + rect.x;
            let base_y = m.top - rect.y;
            let transform = Matrix4::from_translation(Vector3::new(base_x, base_y, *world_z))
                * Matrix4::from_scale(Vector3::new(1.0, -1.0, 1.0))
                * *local_transform;

            let before = out.len();
            out.push(renderer::RenderObject {
                object_type: renderer::ObjectType::TexturedMesh {
                    tint: *tint,
                    vertices: renderer::TexturedMeshVertices::Shared(Arc::clone(vertices)),
                    geom_cache_key: *geom_cache_key,
                    mode: *mode,
                    uv_scale: *uv_scale,
                    uv_offset: *uv_offset,
                    uv_tex_shift: *uv_tex_shift,
                    depth_test: *depth_test,
                },
                texture_handle: texture_cache
                    .texture_handle_with_ptr(Some(str_ptr(texture.as_ref())), texture.as_ref()),
                transform,
                blend: *blend,
                z: 0,
                order: 0,
                camera,
            });

            let layer = base_z.saturating_add(*z);
            for obj in out.iter_mut().skip(before) {
                obj.z = layer;
                obj.order = {
                    let o = *order_counter;
                    *order_counter += 1;
                    o
                };
            }
        }

        actors::Actor::Shadow { len, color, child } => {
            let start = out.len();
            build_actor_recursive(
                child,
                parent,
                m,
                fonts,
                scratch,
                base_z,
                camera,
                cameras,
                masks,
                order_counter,
                out,
                text_cache,
                texture_cache,
                total_elapsed,
            );
            let end = out.len();
            if has_shadow(*len) {
                push_shadow_objects_for_range(out, start, end, *len, *color);
            }
        }

        actors::Actor::Camera {
            view_proj,
            children,
        } => {
            cameras.push(*view_proj);
            let id = cameras.len().saturating_sub(1).try_into().unwrap_or(0u8);
            for child in children {
                build_actor_recursive(
                    child,
                    parent,
                    m,
                    fonts,
                    scratch,
                    base_z,
                    id,
                    cameras,
                    masks,
                    order_counter,
                    out,
                    text_cache,
                    texture_cache,
                    total_elapsed,
                );
            }
        }

        actors::Actor::Text {
            align,
            offset,
            local_transform,
            color,
            stroke_color,
            font,
            content,
            attributes,
            align_text,
            z,
            scale,
            fit_width,
            fit_height,
            line_spacing,
            wrap_width_pixels,
            max_width,
            max_height,
            max_w_pre_zoom,
            max_h_pre_zoom,
            jitter,
            distortion,
            clip,
            mask_dest,
            blend,
            shadow_len,
            shadow_color,
            glow: _,
            effect,
        } => {
            if *mask_dest && masks.is_empty() {
                return;
            }
            if let Some(fm) = fonts.get(font) {
                let layout =
                    text_cache.get_or_build(fm, fonts, content, *wrap_width_pixels, *line_spacing);
                if layout.lines.is_empty() {
                    return;
                }
                let mut effect_color = *color;
                let mut effect_scale = *scale;
                apply_effect_to_text(*effect, total_elapsed, &mut effect_color, &mut effect_scale);
                let mut stroke_rgba = stroke_color.unwrap_or(fm.default_stroke_color);
                stroke_rgba[3] *= effect_color[3];
                let needs_stroke = stroke_rgba[3] > 0.0 && !fm.stroke_texture_map.is_empty();
                let clip_world = (*clip).map(|[x, y, w, h]| {
                    sm_rect_to_world_edges(
                        SmRect {
                            x: parent.x + x,
                            y: parent.y + y,
                            w,
                            h,
                        },
                        m,
                    )
                });
                let before = out.len();
                let layer = base_z.saturating_add(*z);
                let end = if let Some(placement) = resolve_text_layout_placement(
                    layout,
                    effect_scale,
                    *fit_width,
                    *fit_height,
                    *max_width,
                    *max_height,
                    *max_w_pre_zoom,
                    *max_h_pre_zoom,
                    parent,
                    *align,
                    *offset,
                ) {
                    let text_distortion = distortion.max(0.0);
                    if attributes.is_empty() && !*jitter && text_distortion <= 1e-6 {
                        push_text_mesh_batches(
                            out,
                            layout.fill_batches(*align_text),
                            &placement,
                            [1.0; 4],
                            *local_transform,
                            m,
                            texture_cache,
                        );
                        if let Some(clip_world) = clip_world {
                            clip_objects_range_to_world_rect(
                                out,
                                before,
                                clip_world,
                                &mut scratch.recycled_text_mesh_vertices,
                            );
                        }
                        if needs_stroke {
                            let stroke_start = out.len();
                            push_text_mesh_batches(
                                out,
                                layout.stroke_batches(*align_text),
                                &placement,
                                stroke_rgba,
                                *local_transform,
                                m,
                                texture_cache,
                            );
                            if let Some(clip_world) = clip_world {
                                clip_objects_range_to_world_rect(
                                    out,
                                    stroke_start,
                                    clip_world,
                                    &mut scratch.recycled_text_mesh_vertices,
                                );
                            }
                            for obj in out.iter_mut().skip(stroke_start) {
                                obj.z = layer;
                                obj.order = {
                                    let o = *order_counter;
                                    *order_counter += 1;
                                    o
                                };
                                obj.blend = *blend;
                                obj.camera = camera;
                            }
                        }
                    } else {
                        let (builders, recycled_vertices) = scratch.transient_text_mesh_scratch();
                        build_transient_text_mesh_builders(
                            layout,
                            *align_text,
                            attributes,
                            jitter.then(|| (total_elapsed * 8.0).floor() as u32),
                            text_distortion,
                            false,
                            builders,
                            recycled_vertices,
                        );
                        push_transient_text_mesh_builders(
                            out,
                            builders,
                            &placement,
                            [1.0; 4],
                            *local_transform,
                            m,
                            texture_cache,
                        );
                        if let Some(clip_world) = clip_world {
                            clip_objects_range_to_world_rect(
                                out,
                                before,
                                clip_world,
                                recycled_vertices,
                            );
                        }
                        if needs_stroke {
                            let stroke_start = out.len();
                            if text_distortion > 1e-6 {
                                build_transient_text_mesh_builders(
                                    layout,
                                    *align_text,
                                    &[],
                                    None,
                                    text_distortion,
                                    true,
                                    builders,
                                    recycled_vertices,
                                );
                                push_transient_text_mesh_builders(
                                    out,
                                    builders,
                                    &placement,
                                    stroke_rgba,
                                    *local_transform,
                                    m,
                                    texture_cache,
                                );
                            } else {
                                push_text_mesh_batches(
                                    out,
                                    layout.stroke_batches(*align_text),
                                    &placement,
                                    stroke_rgba,
                                    *local_transform,
                                    m,
                                    texture_cache,
                                );
                            }
                            if let Some(clip_world) = clip_world {
                                clip_objects_range_to_world_rect(
                                    out,
                                    stroke_start,
                                    clip_world,
                                    recycled_vertices,
                                );
                            }
                            for obj in out.iter_mut().skip(stroke_start) {
                                obj.z = layer;
                                obj.order = {
                                    let o = *order_counter;
                                    *order_counter += 1;
                                    o
                                };
                                obj.blend = *blend;
                                obj.camera = camera;
                            }
                        }
                    }
                    out.len()
                } else {
                    before
                };
                if *mask_dest {
                    clip_objects_range_to_world_masks(out, before, masks);
                }
                for obj in out.iter_mut().take(end).skip(before) {
                    obj.z = layer;
                    obj.order = {
                        let o = *order_counter;
                        *order_counter += 1;
                        o
                    };
                    obj.blend = *blend;
                    obj.camera = camera;
                    if let renderer::ObjectType::TexturedMesh { tint, .. } = &mut obj.object_type {
                        tint[0] *= effect_color[0];
                        tint[1] *= effect_color[1];
                        tint[2] *= effect_color[2];
                        tint[3] *= effect_color[3];
                    }
                }
                if has_shadow(*shadow_len) {
                    push_shadow_objects_for_range(out, before, end, *shadow_len, *shadow_color);
                }
            }
        }

        actors::Actor::Frame {
            align,
            offset,
            size,
            children,
            background,
            z,
        } => {
            let rect = place_rect(parent, *align, *offset, *size);
            let layer = base_z.saturating_add(*z);

            if let Some(bg) = background {
                match bg {
                    actors::Background::Color(c) => {
                        let before = out.len();
                        push_sprite(
                            out,
                            camera,
                            rect,
                            m,
                            true,
                            "__white",
                            Some(str_ptr("__white")),
                            true,
                            *c,
                            None,
                            None,
                            None,
                            false,
                            false,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            BlendMode::Alpha,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            [0.0, 0.0],
                            [0.0, 1.0],
                            None,
                            texture_cache,
                            total_elapsed,
                        );
                        for obj in out.iter_mut().skip(before) {
                            obj.z = layer;
                            obj.order = {
                                let o = *order_counter;
                                *order_counter += 1;
                                o
                            };
                        }
                    }
                    actors::Background::Texture(tex) => {
                        let before = out.len();
                        push_sprite(
                            out,
                            camera,
                            rect,
                            m,
                            false,
                            tex,
                            Some(str_ptr(tex)),
                            true,
                            [1.0; 4],
                            None,
                            None,
                            None,
                            false,
                            false,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            BlendMode::Alpha,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            [0.0, 0.0],
                            [0.0, 1.0],
                            None,
                            texture_cache,
                            total_elapsed,
                        );
                        for obj in out.iter_mut().skip(before) {
                            obj.z = layer;
                            obj.order = {
                                let o = *order_counter;
                                *order_counter += 1;
                                o
                            };
                        }
                    }
                }
            }

            for child in children {
                build_actor_recursive(
                    child,
                    rect,
                    m,
                    fonts,
                    scratch,
                    layer,
                    camera,
                    cameras,
                    masks,
                    order_counter,
                    out,
                    text_cache,
                    texture_cache,
                    total_elapsed,
                );
            }
        }
    }
}

/* ======================= LAYOUT HELPERS ======================= */

#[inline(always)]
fn resolve_sprite_size_like_sm(
    size: [SizeSpec; 2],
    is_solid: bool,
    texture_name: &str,
    texture_key_ptr: Option<*const str>,
    texture_key_stable: bool,
    uv_rect: Option<[f32; 4]>,
    cell: Option<(u32, u32)>,
    grid: Option<(u32, u32)>,
    scale: [f32; 2],
    texture_cache: &mut TextureLookupCache,
) -> [SizeSpec; 2] {
    use SizeSpec::Px;

    #[inline(always)]
    fn native_dims(
        is_solid: bool,
        texture_name: &str,
        texture_key_ptr: Option<*const str>,
        texture_key_stable: bool,
        _uv: Option<[f32; 4]>,
        cell: Option<(u32, u32)>,
        grid: Option<(u32, u32)>,
        texture_cache: &mut TextureLookupCache,
    ) -> (f32, f32) {
        if is_solid {
            return (1.0, 1.0);
        }
        let Some(meta) =
            texture_cache.texture_dims_cached(texture_key_ptr, texture_name, texture_key_stable)
        else {
            return (0.0, 0.0);
        };
        let (mut tw, mut th) = (meta.w as f32, meta.h as f32);
        if cell.is_some() {
            let (gc, gr) = grid.unwrap_or_else(|| {
                texture_cache.sprite_sheet_dims_cached(
                    texture_key_ptr,
                    texture_name,
                    texture_key_stable,
                )
            });
            let cols = gc.max(1);
            let rows = gr.max(1);
            tw /= cols as f32;
            th /= rows as f32;
        }
        (tw, th)
    }

    let (nw, nh) = native_dims(
        is_solid,
        texture_name,
        texture_key_ptr,
        texture_key_stable,
        uv_rect,
        cell,
        grid,
        texture_cache,
    );
    let aspect = if nw > 0.0 && nh > 0.0 { nh / nw } else { 1.0 };

    match (size[0], size[1]) {
        (Px(w), Px(h)) if w == 0.0 && h == 0.0 => [Px(nw * scale[0]), Px(nh * scale[1])],
        (Px(w), Px(h)) if w > 0.0 && h == 0.0 => [Px(w), Px(w * aspect)],
        (Px(w), Px(h)) if w == 0.0 && h > 0.0 => {
            let inv_aspect = if aspect > 0.0 { 1.0 / aspect } else { 1.0 };
            [Px(h * inv_aspect), Px(h)]
        }
        _ => size,
    }
}

#[inline(always)]
fn place_rect(parent: SmRect, align: [f32; 2], offset: [f32; 2], size: [SizeSpec; 2]) -> SmRect {
    let w = match size[0] {
        SizeSpec::Px(w) => w,
        SizeSpec::Fill => parent.w,
    };
    let h = match size[1] {
        SizeSpec::Px(h) => h,
        SizeSpec::Fill => parent.h,
    };
    let rx = parent.x;
    let ry = parent.y;
    let ax = align[0];
    let ay = align[1];
    SmRect {
        x: ax.mul_add(-w, rx + offset[0]),
        y: ay.mul_add(-h, ry + offset[1]),
        w,
        h,
    }
}

#[inline(always)]
fn calculate_uvs(
    texture: &str,
    texture_key_ptr: Option<*const str>,
    texture_key_stable: bool,
    uv_rect: Option<[f32; 4]>,
    cell: Option<(u32, u32)>,
    grid: Option<(u32, u32)>,
    flip_x: bool,
    flip_y: bool,
    cl: f32,
    cr: f32,
    ct: f32,
    cb: f32,
    texcoordvelocity: Option<[f32; 2]>,
    texture_cache: &mut TextureLookupCache,
    total_elapsed: f32,
) -> ([f32; 2], [f32; 2]) {
    let (mut uv_scale, mut uv_offset) = if let Some([u0, v0, u1, v1]) = uv_rect {
        let du = (u1 - u0).abs().max(1e-6);
        let dv = (v1 - v0).abs().max(1e-6);
        ([du, dv], [u0.min(u1), v0.min(v1)])
    } else if let Some((cx, cy)) = cell {
        let (gc, gr) = grid.unwrap_or_else(|| {
            texture_cache.sprite_sheet_dims_cached(texture_key_ptr, texture, texture_key_stable)
        });
        let cols = gc.max(1);
        let rows = gr.max(1);
        let (col, row) = if cy == u32::MAX {
            let idx = cx;
            (idx % cols, (idx / cols).min(rows.saturating_sub(1)))
        } else {
            (
                cx.min(cols.saturating_sub(1)),
                cy.min(rows.saturating_sub(1)),
            )
        };
        let s = [1.0 / cols as f32, 1.0 / rows as f32];
        let o = [col as f32 * s[0], row as f32 * s[1]];
        (s, o)
    } else {
        ([1.0, 1.0], [0.0, 0.0])
    };

    uv_offset[0] += uv_scale[0] * cl;
    uv_offset[1] += uv_scale[1] * ct;
    uv_scale[0] *= (1.0 - cl - cr).max(0.0);
    uv_scale[1] *= (1.0 - ct - cb).max(0.0);

    if flip_x {
        uv_offset[0] += uv_scale[0];
        uv_scale[0] = -uv_scale[0];
    }
    if flip_y {
        uv_offset[1] += uv_scale[1];
        uv_scale[1] = -uv_scale[1];
    }

    if let Some(vel) = texcoordvelocity {
        uv_offset[0] += vel[0] * total_elapsed;
        uv_offset[1] += vel[1] * total_elapsed;
    }

    (uv_scale, uv_offset)
}

#[inline(always)]
fn fold_sprite_xy_rot(
    mut flip_x: bool,
    mut flip_y: bool,
    mut size_x: f32,
    mut size_y: f32,
    rot_x_deg: f32,
    rot_y_deg: f32,
) -> (bool, bool, f32, f32) {
    // Sprite instances only preserve 2D rotation in the fast path. Fold SM's
    // X/Y rotations into foreshortening plus texture flips so Y=180 mirrors
    // horizontally instead of becoming an accidental in-plane 180-degree turn.
    if rot_x_deg == 0.0 && rot_y_deg == 0.0 {
        return (flip_x, flip_y, size_x, size_y);
    }

    let cos_y = rot_y_deg.to_radians().cos();
    size_x *= cos_y.abs();
    if cos_y.is_sign_negative() {
        flip_x = !flip_x;
    }

    let cos_x = rot_x_deg.to_radians().cos();
    size_y *= cos_x.abs();
    if cos_x.is_sign_negative() {
        flip_y = !flip_y;
    }

    (flip_x, flip_y, size_x, size_y)
}

#[inline(always)]
fn push_sprite<'a>(
    out: &mut Vec<renderer::RenderObject>,
    camera: u8,
    rect: SmRect,
    m: &Metrics,
    is_solid: bool,
    texture_id: &'a str,
    texture_key_ptr: Option<*const str>,
    texture_key_stable: bool,
    tint: [f32; 4],
    uv_rect: Option<[f32; 4]>,
    cell: Option<(u32, u32)>,
    grid: Option<(u32, u32)>,
    flip_x: bool,
    flip_y: bool,
    cropleft: f32,
    cropright: f32,
    croptop: f32,
    cropbottom: f32,
    fadeleft: f32,
    faderight: f32,
    fadetop: f32,
    fadebottom: f32,
    blend: BlendMode,
    rot_x_deg: f32,
    rot_y_deg: f32,
    rot_z_deg: f32,
    world_z: f32,
    local_offset: [f32; 2],
    local_offset_rot_sin_cos: [f32; 2],
    texcoordvelocity: Option<[f32; 2]>,
    texture_cache: &mut TextureLookupCache,
    total_elapsed: f32,
) {
    if tint[3] <= 0.0 {
        return;
    }

    let (cl, cr, ct, cb) = clamp_crop_fractions(cropleft, cropright, croptop, cropbottom);

    let (base_center, base_size) = sm_rect_to_world_center_size(rect, m);
    if base_size.x <= 0.0 || base_size.y <= 0.0 {
        return;
    }

    let sx_crop = (1.0 - cl - cr).max(0.0);
    let sy_crop = (1.0 - ct - cb).max(0.0);
    if sx_crop <= 0.0 || sy_crop <= 0.0 {
        return;
    }

    // StepMania parity: crop shifts geometry toward the un-cropped side(s).
    // (This matches Sprite::DrawTexture(), which moves quad vertices instead of the actor.)
    let center_x = ((cl - cr) * base_size.x).mul_add(0.5, base_center.x);
    let center_y = ((cb - ct) * base_size.y).mul_add(0.5, base_center.y);
    let size_x = base_size.x * sx_crop;
    let size_y = base_size.y * sy_crop;
    let (uv_scale, uv_offset) = if is_solid {
        ([1.0, 1.0], [0.0, 0.0])
    } else {
        calculate_uvs(
            texture_id,
            texture_key_ptr,
            texture_key_stable,
            uv_rect,
            cell,
            grid,
            flip_x,
            flip_y,
            cl,
            cr,
            ct,
            cb,
            texcoordvelocity,
            texture_cache,
            total_elapsed,
        )
    };

    let (flip_x, flip_y, size_x, size_y) =
        fold_sprite_xy_rot(flip_x, flip_y, size_x, size_y, rot_x_deg, rot_y_deg);
    let (sin_z, cos_z) = if rot_z_deg == 0.0 {
        (0.0, 1.0)
    } else {
        rot_z_deg.to_radians().sin_cos()
    };

    let fl = fadeleft.clamp(0.0, 1.0);
    let fr = faderight.clamp(0.0, 1.0);
    let ft = fadetop.clamp(0.0, 1.0);
    let fb = fadebottom.clamp(0.0, 1.0);

    // StepMania parity (Sprite::DrawPrimitives edge-fade behavior):
    // - Fade distances are specified in the *pre-crop* [0..1] space.
    // - Visible (post-crop) fraction is `(1 - crop_a - crop_b)`.
    // - Negative crop values can "cancel" fade (used by Simply Love transitions).
    let mut fl_size = (fl + cropleft.min(0.0)).max(0.0);
    let mut fr_size = (fr + cropright.min(0.0)).max(0.0);
    let mut ft_size = (ft + croptop.min(0.0)).max(0.0);
    let mut fb_size = (fb + cropbottom.min(0.0)).max(0.0);

    let sum_x = fl_size + fr_size;
    if sum_x > 0.0 && sx_crop < sum_x {
        let s = sx_crop / sum_x;
        fl_size *= s;
        fr_size *= s;
    }

    let sum_y = ft_size + fb_size;
    if sum_y > 0.0 && sy_crop < sum_y {
        let s = sy_crop / sum_y;
        ft_size *= s;
        fb_size *= s;
    }

    let mut fl_eff = (fl_size / sx_crop).clamp(0.0, 1.0);
    let mut fr_eff = (fr_size / sx_crop).clamp(0.0, 1.0);
    let mut ft_eff = (ft_size / sy_crop).clamp(0.0, 1.0);
    let mut fb_eff = (fb_size / sy_crop).clamp(0.0, 1.0);

    if flip_x {
        std::mem::swap(&mut fl_eff, &mut fr_eff);
    }
    if flip_y {
        std::mem::swap(&mut ft_eff, &mut fb_eff);
    }

    let texture_key = if is_solid { "__white" } else { texture_id };

    out.push(renderer::RenderObject {
        object_type: renderer::ObjectType::Sprite {
            center: [center_x, center_y, world_z, 0.0],
            size: [size_x, size_y],
            rot_sin_cos: [sin_z, cos_z],
            tint,
            uv_scale,
            uv_offset,
            local_offset,
            local_offset_rot_sin_cos,
            edge_fade: [fl_eff, fr_eff, ft_eff, fb_eff],
        },
        texture_handle: texture_cache.texture_handle_cached(
            texture_key_ptr,
            texture_key,
            texture_key_stable,
        ),
        transform: Matrix4::IDENTITY,
        blend,
        z: 0,
        order: 0,
        camera,
    });
}

#[inline(always)]
#[must_use]
const fn clamp_crop_fractions(l: f32, r: f32, t: f32, b: f32) -> (f32, f32, f32, f32) {
    (
        l.clamp(0.0, 1.0),
        r.clamp(0.0, 1.0),
        t.clamp(0.0, 1.0),
        b.clamp(0.0, 1.0),
    )
}

#[inline(always)]
#[must_use]
fn lrint_ties_even(v: f32) -> f32 {
    if !v.is_finite() {
        return 0.0;
    }
    // Fast path: already an integer (including -0.0)
    if v.fract() == 0.0 {
        return v;
    }

    let floor = v.floor();
    let frac = v - floor;

    if frac < 0.5 {
        floor
    } else if frac > 0.5 {
        floor + 1.0
    } else {
        // frac == 0.5 exactly: ties-to-even
        // Use i64 for parity check to avoid edge overflow on extreme values.
        let f_even = ((floor as i64) & 1) == 0;
        if f_even { floor } else { floor + 1.0 }
    }
}

#[inline(always)]
#[must_use]
const fn quantize_up_even_i32(v: i32) -> i32 {
    if v <= 0 {
        0
    } else if (v & 1) != 0 {
        v + 1
    } else {
        v
    }
}

fn push_text_mesh_batches(
    out: &mut Vec<RenderObject>,
    batches: &[CachedTextMeshBatch],
    placement: &TextLayoutPlacement,
    tint: [f32; 4],
    local_transform: Matrix4,
    m: &Metrics,
    texture_cache: &mut TextureLookupCache,
) {
    if batches.is_empty() || tint[3] <= 0.0 {
        return;
    }

    let transform = Matrix4::from_translation(Vector3::new(
        m.left + placement.block_center_x,
        m.top - placement.block_center_y,
        0.0,
    )) * Matrix4::from_scale(Vector3::new(placement.sx, -placement.sy, 1.0))
        * local_transform;

    out.reserve(batches.len());
    for batch in batches {
        // SAFETY: `batch.texture_key` is captured from immutable font storage and
        // remains valid while the cached text layout is alive.
        let texture_key = unsafe { str_from_cached_ptr(batch.texture_key) };
        out.push(RenderObject {
            object_type: renderer::ObjectType::TexturedMesh {
                tint,
                vertices: renderer::TexturedMeshVertices::Shared(Arc::clone(&batch.vertices)),
                geom_cache_key: batch.geom_cache_key,
                mode: renderer::MeshMode::Triangles,
                uv_scale: [1.0, 1.0],
                uv_offset: [0.0, 0.0],
                uv_tex_shift: [0.0, 0.0],
                depth_test: false,
            },
            texture_handle: texture_cache.texture_handle_stable_ptr(batch.texture_key, texture_key),
            transform,
            blend: BlendMode::Alpha,
            z: 0,
            order: 0,
            camera: 0,
        });
    }
}

fn push_transient_text_mesh_builders(
    out: &mut Vec<RenderObject>,
    builders: &mut Vec<TextMeshBatchBuilder>,
    placement: &TextLayoutPlacement,
    tint: [f32; 4],
    local_transform: Matrix4,
    m: &Metrics,
    texture_cache: &mut TextureLookupCache,
) {
    if builders.is_empty() || tint[3] <= 0.0 {
        return;
    }

    let transform = Matrix4::from_translation(Vector3::new(
        m.left + placement.block_center_x,
        m.top - placement.block_center_y,
        0.0,
    )) * Matrix4::from_scale(Vector3::new(placement.sx, -placement.sy, 1.0))
        * local_transform;

    out.reserve(builders.len());
    for builder in builders.drain(..) {
        if builder.vertices.is_empty() {
            continue;
        }
        // SAFETY: `builder.texture_key` is captured from immutable font storage and
        // remains valid while the cached text layout is alive.
        let texture_key = unsafe { str_from_cached_ptr(builder.texture_key) };
        out.push(RenderObject {
            object_type: renderer::ObjectType::TexturedMesh {
                tint,
                vertices: renderer::TexturedMeshVertices::Transient(builder.vertices),
                geom_cache_key: renderer::INVALID_TMESH_CACHE_KEY,
                mode: renderer::MeshMode::Triangles,
                uv_scale: [1.0, 1.0],
                uv_offset: [0.0, 0.0],
                uv_tex_shift: [0.0, 0.0],
                depth_test: false,
            },
            texture_handle: texture_cache
                .texture_handle_stable_ptr(builder.texture_key, texture_key),
            transform,
            blend: BlendMode::Alpha,
            z: 0,
            order: 0,
            camera: 0,
        });
    }
}

#[inline(always)]
fn sm_rect_to_world_center_size(rect: SmRect, m: &Metrics) -> (Vector2, Vector2) {
    (
        Vector2::new(
            0.5f32.mul_add(rect.w, m.left + rect.x),
            m.top - 0.5f32.mul_add(rect.h, rect.y),
        ),
        Vector2::new(rect.w, rect.h),
    )
}

#[derive(Clone, Copy, Debug)]
struct WorldRect {
    left: f32,
    right: f32,
    bottom: f32,
    top: f32,
}

#[inline(always)]
fn sm_rect_to_world_edges(rect: SmRect, m: &Metrics) -> WorldRect {
    let left = m.left + rect.x;
    let right = rect.w.mul_add(1.0, left);

    let top = m.top - rect.y;
    let bottom = top - rect.h;

    WorldRect {
        left,
        right,
        bottom,
        top,
    }
}

fn clip_objects_range_to_world_masks(
    objects: &mut Vec<RenderObject>,
    start: usize,
    masks: &[WorldRect],
) {
    if start >= objects.len() {
        return;
    }
    if masks.is_empty() {
        objects.truncate(start);
        return;
    }
    if let [mask] = masks {
        let mut recycled_vertices = Vec::new();
        clip_objects_range_to_world_rect(objects, start, *mask, &mut recycled_vertices);
        return;
    }
    let len = objects.len();
    let mut write = start;
    for read in start..len {
        let keep = {
            let obj = &mut objects[read];
            clip_object_to_world_masks(obj, masks)
        };
        if keep {
            if write != read {
                objects.swap(write, read);
            }
            write += 1;
        }
    }
    objects.truncate(write);
}

struct ClippedSpriteObject {
    object_type: renderer::ObjectType,
    transform: Matrix4,
}

#[inline(always)]
fn object_world_area(object_type: &renderer::ObjectType, transform: &Matrix4) -> f32 {
    match object_type {
        renderer::ObjectType::Sprite { size, .. } => (size[0] * size[1]).abs(),
        renderer::ObjectType::TexturedMesh { vertices, .. } => {
            if vertices.len() < 3 {
                return 0.0;
            }
            let t = transform;
            let mut area = 0.0_f32;
            let mut i = 0usize;
            while i + 2 < vertices.len() {
                let p0 = world_xy_3d(t, vertices[i].pos);
                let p1 = world_xy_3d(t, vertices[i + 1].pos);
                let p2 = world_xy_3d(t, vertices[i + 2].pos);
                let a = (p1[0] - p0[0]) * (p2[1] - p0[1]) - (p1[1] - p0[1]) * (p2[0] - p0[0]);
                area += 0.5 * a.abs();
                i += 3;
            }
            area
        }
        renderer::ObjectType::Mesh { .. } => 0.0,
    }
}

fn clip_object_to_world_masks(obj: &mut RenderObject, masks: &[WorldRect]) -> bool {
    let mut best_obj: Option<ClippedSpriteObject> = None;
    let mut best_area = -1.0_f32;
    for &mask in masks {
        let Some(candidate) = clipped_sprite_object_to_world_rect(obj, mask, None) else {
            continue;
        };
        let area = object_world_area(&candidate.object_type, &candidate.transform);
        if area > best_area {
            best_area = area;
            best_obj = Some(candidate);
        }
    }
    if let Some(chosen) = best_obj {
        obj.object_type = chosen.object_type;
        obj.transform = chosen.transform;
        true
    } else {
        false
    }
}

fn clip_objects_range_to_world_rect(
    objects: &mut Vec<RenderObject>,
    start: usize,
    clip: WorldRect,
    recycled_vertices: &mut Vec<Vec<renderer::TexturedMeshVertex>>,
) {
    if start >= objects.len() {
        return;
    }
    if clip.left >= clip.right || clip.bottom >= clip.top {
        objects.truncate(start);
        return;
    }

    let len = objects.len();
    let mut write = start;
    for read in start..len {
        let keep = {
            let obj = &mut objects[read];
            clip_sprite_object_to_world_rect_with_recycled(obj, clip, Some(&mut *recycled_vertices))
        };
        if keep {
            if write != read {
                objects.swap(write, read);
            }
            write += 1;
        }
    }
    objects.truncate(write);
}

#[cfg(test)]
fn clip_sprite_object_to_world_rect(obj: &mut RenderObject, clip: WorldRect) -> bool {
    clip_sprite_object_to_world_rect_with_recycled(obj, clip, None)
}

fn clip_sprite_object_to_world_rect_with_recycled(
    obj: &mut RenderObject,
    clip: WorldRect,
    recycled_vertices: Option<&mut Vec<Vec<renderer::TexturedMeshVertex>>>,
) -> bool {
    let Some(clipped) = clipped_sprite_object_to_world_rect(obj, clip, recycled_vertices) else {
        return false;
    };
    obj.object_type = clipped.object_type;
    obj.transform = clipped.transform;
    true
}

fn clipped_sprite_object_to_world_rect(
    obj: &RenderObject,
    clip: WorldRect,
    recycled_vertices: Option<&mut Vec<Vec<renderer::TexturedMeshVertex>>>,
) -> Option<ClippedSpriteObject> {
    if clip.left >= clip.right || clip.bottom >= clip.top {
        return None;
    }
    match &obj.object_type {
        renderer::ObjectType::Sprite {
            center,
            size,
            rot_sin_cos,
            tint,
            uv_scale,
            uv_offset,
            local_offset,
            local_offset_rot_sin_cos,
            edge_fade,
        } => {
            let eps = 1e-6;
            let offset_world = [
                local_offset_rot_sin_cos[1].mul_add(
                    local_offset[0],
                    -(local_offset_rot_sin_cos[0] * local_offset[1]),
                ),
                local_offset_rot_sin_cos[0].mul_add(
                    local_offset[0],
                    local_offset_rot_sin_cos[1] * local_offset[1],
                ),
            ];
            let world_center = [center[0] + offset_world[0], center[1] + offset_world[1]];
            if rot_sin_cos[0].abs() > eps || rot_sin_cos[1] < 1.0 - eps {
                return clip_rotated_sprite_to_world_rect(
                    *tint,
                    *center,
                    *size,
                    *rot_sin_cos,
                    *uv_scale,
                    *uv_offset,
                    offset_world,
                    clip,
                );
            }

            let w = size[0];
            let h = size[1];
            if w <= eps || h <= eps {
                return None;
            }

            let half_w = w * 0.5;
            let half_h = h * 0.5;

            let left = world_center[0] - half_w;
            let right = world_center[0] + half_w;
            let bottom = world_center[1] - half_h;
            let top = world_center[1] + half_h;

            let inter_left = left.max(clip.left);
            let inter_right = right.min(clip.right);
            let inter_bottom = bottom.max(clip.bottom);
            let inter_top = top.min(clip.top);
            if inter_left >= inter_right || inter_bottom >= inter_top {
                return None;
            }

            let inv_w = 1.0 / w;
            let inv_h = 1.0 / h;

            let cl = ((inter_left - left) * inv_w).clamp(0.0, 1.0);
            let cr = ((right - inter_right) * inv_w).clamp(0.0, 1.0);
            let cb = ((inter_bottom - bottom) * inv_h).clamp(0.0, 1.0);
            let ct = ((top - inter_top) * inv_h).clamp(0.0, 1.0);

            let sx_crop = (1.0 - cl - cr).max(0.0);
            let sy_crop = (1.0 - ct - cb).max(0.0);
            if sx_crop <= eps || sy_crop <= eps {
                return None;
            }

            let uv_offset = [
                uv_offset[0] + uv_scale[0] * cl,
                uv_offset[1] + uv_scale[1] * ct,
            ];
            let uv_scale = [uv_scale[0] * sx_crop, uv_scale[1] * sy_crop];

            let center_x = ((cl - cr) * w).mul_add(0.5, world_center[0]) - offset_world[0];
            let center_y = ((cb - ct) * h).mul_add(0.5, world_center[1]) - offset_world[1];
            let new_w = w * sx_crop;
            let new_h = h * sy_crop;

            Some(ClippedSpriteObject {
                object_type: renderer::ObjectType::Sprite {
                    center: [center_x, center_y, center[2], center[3]],
                    size: [new_w, new_h],
                    rot_sin_cos: *rot_sin_cos,
                    tint: *tint,
                    uv_scale,
                    uv_offset,
                    local_offset: *local_offset,
                    local_offset_rot_sin_cos: *local_offset_rot_sin_cos,
                    edge_fade: *edge_fade,
                },
                transform: Matrix4::IDENTITY,
            })
        }
        renderer::ObjectType::TexturedMesh {
            tint,
            vertices,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            ..
        } => {
            let vertices = vertices.as_ref();
            let Some(bounds) = textured_mesh_world_bounds(vertices, obj.transform) else {
                return None;
            };
            if bounds.right < clip.left
                || bounds.left > clip.right
                || bounds.top < clip.bottom
                || bounds.bottom > clip.top
            {
                return None;
            }
            if bounds.left >= clip.left
                && bounds.right <= clip.right
                && bounds.bottom >= clip.bottom
                && bounds.top <= clip.top
            {
                return Some(ClippedSpriteObject {
                    object_type: obj.object_type.clone(),
                    transform: obj.transform,
                });
            }
            clip_textured_mesh_to_world_rect(
                *tint,
                vertices,
                obj.transform,
                *uv_scale,
                *uv_offset,
                *uv_tex_shift,
                clip,
                recycled_vertices,
            )
        }
        renderer::ObjectType::Mesh { .. } => Some(ClippedSpriteObject {
            object_type: obj.object_type.clone(),
            transform: obj.transform,
        }),
    }
}

#[derive(Clone, Copy)]
struct ClipVertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

#[inline(always)]
fn sprite_world_xy(
    center: [f32; 4],
    size: [f32; 2],
    rot_sin_cos: [f32; 2],
    offset_world: [f32; 2],
    p: [f32; 2],
) -> [f32; 2] {
    let local_x = p[0] * size[0];
    let local_y = p[1] * size[1];
    [
        rot_sin_cos[1].mul_add(
            local_x,
            (-rot_sin_cos[0] * local_y) + center[0] + offset_world[0],
        ),
        rot_sin_cos[0].mul_add(
            local_x,
            (rot_sin_cos[1] * local_y) + center[1] + offset_world[1],
        ),
    ]
}

#[inline(always)]
fn world_xy_3d(t: &Matrix4, p: [f32; 3]) -> [f32; 2] {
    let clip = *t * Vector4::new(p[0], p[1], p[2], 1.0);
    let inv_w = if clip.w.abs() > f32::EPSILON {
        clip.w.recip()
    } else {
        1.0
    };
    [clip.x * inv_w, clip.y * inv_w]
}

fn textured_mesh_world_bounds(
    vertices: &[renderer::TexturedMeshVertex],
    transform: Matrix4,
) -> Option<WorldRect> {
    let first = vertices.first()?;
    let first = world_xy_3d(&transform, first.pos);
    let mut bounds = WorldRect {
        left: first[0],
        right: first[0],
        bottom: first[1],
        top: first[1],
    };
    for vertex in &vertices[1..] {
        let p = world_xy_3d(&transform, vertex.pos);
        bounds.left = bounds.left.min(p[0]);
        bounds.right = bounds.right.max(p[0]);
        bounds.bottom = bounds.bottom.min(p[1]);
        bounds.top = bounds.top.max(p[1]);
    }
    Some(bounds)
}

#[inline(always)]
fn lerp_clip(a: ClipVertex, b: ClipVertex, t: f32) -> ClipVertex {
    let t = t.clamp(0.0, 1.0);
    ClipVertex {
        pos: [
            (b.pos[0] - a.pos[0]).mul_add(t, a.pos[0]),
            (b.pos[1] - a.pos[1]).mul_add(t, a.pos[1]),
        ],
        uv: [
            (b.uv[0] - a.uv[0]).mul_add(t, a.uv[0]),
            (b.uv[1] - a.uv[1]).mul_add(t, a.uv[1]),
        ],
        color: [
            (b.color[0] - a.color[0]).mul_add(t, a.color[0]),
            (b.color[1] - a.color[1]).mul_add(t, a.color[1]),
            (b.color[2] - a.color[2]).mul_add(t, a.color[2]),
            (b.color[3] - a.color[3]).mul_add(t, a.color[3]),
        ],
    }
}

fn clip_poly_edge_into(
    poly: &[ClipVertex],
    axis: usize,
    bound: f32,
    keep_greater: bool,
) -> ClipPolygon {
    let mut out = ClipPolygon::new();
    if poly.is_empty() {
        return out;
    }
    let mut prev = poly[poly.len() - 1];
    let mut prev_in = if keep_greater {
        prev.pos[axis] >= bound
    } else {
        prev.pos[axis] <= bound
    };

    for &curr in poly {
        let curr_in = if keep_greater {
            curr.pos[axis] >= bound
        } else {
            curr.pos[axis] <= bound
        };
        if prev_in && curr_in {
            out.push(curr);
        } else if prev_in && !curr_in {
            let denom = curr.pos[axis] - prev.pos[axis];
            if denom.abs() > 1e-6 {
                let t = (bound - prev.pos[axis]) / denom;
                out.push(lerp_clip(prev, curr, t));
            }
        } else if !prev_in && curr_in {
            let denom = curr.pos[axis] - prev.pos[axis];
            if denom.abs() > 1e-6 {
                let t = (bound - prev.pos[axis]) / denom;
                out.push(lerp_clip(prev, curr, t));
            }
            out.push(curr);
        }
        prev = curr;
        prev_in = curr_in;
    }
    out
}

fn clip_polygon_to_world_rect(poly: &[ClipVertex], clip: WorldRect) -> ClipPolygon {
    let mut p = clip_poly_edge_into(poly, 0, clip.left, true);
    p = clip_poly_edge_into(&p, 0, clip.right, false);
    p = clip_poly_edge_into(&p, 1, clip.bottom, true);
    clip_poly_edge_into(&p, 1, clip.top, false)
}

#[inline(always)]
fn baked_tmesh_uv(
    vertex: &renderer::TexturedMeshVertex,
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
) -> [f32; 2] {
    [
        vertex.uv[0].mul_add(uv_scale[0], uv_offset[0])
            + uv_tex_shift[0] * (vertex.tex_matrix_scale[0] - 1.0),
        vertex.uv[1].mul_add(uv_scale[1], uv_offset[1])
            + uv_tex_shift[1] * (vertex.tex_matrix_scale[1] - 1.0),
    ]
}

#[inline(always)]
fn clipped_text_mesh_out<'a>(
    out: &'a mut Option<Vec<renderer::TexturedMeshVertex>>,
    recycled_vertices: &mut Option<&mut Vec<Vec<renderer::TexturedMeshVertex>>>,
    source_len: usize,
) -> &'a mut Vec<renderer::TexturedMeshVertex> {
    out.get_or_insert_with(|| {
        let mut vertices = recycled_vertices
            .take()
            .map(take_recycled_text_mesh_vertices)
            .unwrap_or_default();
        vertices.reserve(source_len.min(48));
        vertices
    })
}

fn clip_textured_mesh_to_world_rect(
    tint: [f32; 4],
    vertices: &[renderer::TexturedMeshVertex],
    transform: Matrix4,
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
    clip: WorldRect,
    mut recycled_vertices: Option<&mut Vec<Vec<renderer::TexturedMeshVertex>>>,
) -> Option<ClippedSpriteObject> {
    if vertices.len() < 3 {
        return None;
    }

    let mut out: Option<Vec<renderer::TexturedMeshVertex>> = None;
    for tri in vertices.chunks_exact(3) {
        let p0 = world_xy_3d(&transform, tri[0].pos);
        let p1 = world_xy_3d(&transform, tri[1].pos);
        let p2 = world_xy_3d(&transform, tri[2].pos);
        let left = p0[0].min(p1[0]).min(p2[0]);
        let right = p0[0].max(p1[0]).max(p2[0]);
        let bottom = p0[1].min(p1[1]).min(p2[1]);
        let top = p0[1].max(p1[1]).max(p2[1]);
        if right < clip.left || left > clip.right || top < clip.bottom || bottom > clip.top {
            continue;
        }

        let uv0 = baked_tmesh_uv(&tri[0], uv_scale, uv_offset, uv_tex_shift);
        let uv1 = baked_tmesh_uv(&tri[1], uv_scale, uv_offset, uv_tex_shift);
        let uv2 = baked_tmesh_uv(&tri[2], uv_scale, uv_offset, uv_tex_shift);
        if left >= clip.left && right <= clip.right && bottom >= clip.bottom && top <= clip.top {
            let out = clipped_text_mesh_out(&mut out, &mut recycled_vertices, vertices.len());
            out.push(renderer::TexturedMeshVertex {
                pos: [p0[0], p0[1], 0.0],
                uv: uv0,
                tex_matrix_scale: [1.0, 1.0],
                color: tri[0].color,
            });
            out.push(renderer::TexturedMeshVertex {
                pos: [p1[0], p1[1], 0.0],
                uv: uv1,
                tex_matrix_scale: [1.0, 1.0],
                color: tri[1].color,
            });
            out.push(renderer::TexturedMeshVertex {
                pos: [p2[0], p2[1], 0.0],
                uv: uv2,
                tex_matrix_scale: [1.0, 1.0],
                color: tri[2].color,
            });
            continue;
        }

        let poly = [
            ClipVertex {
                pos: p0,
                uv: uv0,
                color: tri[0].color,
            },
            ClipVertex {
                pos: p1,
                uv: uv1,
                color: tri[1].color,
            },
            ClipVertex {
                pos: p2,
                uv: uv2,
                color: tri[2].color,
            },
        ];
        let clipped = clip_polygon_to_world_rect(&poly, clip);
        if clipped.len() < 3 {
            continue;
        }
        let out = clipped_text_mesh_out(&mut out, &mut recycled_vertices, vertices.len());

        let base = clipped[0];
        let mut i = 1usize;
        while i + 1 < clipped.len() {
            for vertex in [base, clipped[i], clipped[i + 1]] {
                out.push(renderer::TexturedMeshVertex {
                    pos: [vertex.pos[0], vertex.pos[1], 0.0],
                    uv: vertex.uv,
                    tex_matrix_scale: [1.0, 1.0],
                    color: vertex.color,
                });
            }
            i += 1;
        }
    }

    let out = out?;
    if out.is_empty() {
        return None;
    }

    Some(ClippedSpriteObject {
        object_type: renderer::ObjectType::TexturedMesh {
            tint,
            vertices: renderer::TexturedMeshVertices::Transient(out),
            geom_cache_key: renderer::INVALID_TMESH_CACHE_KEY,
            mode: renderer::MeshMode::Triangles,
            uv_scale: [1.0, 1.0],
            uv_offset: [0.0, 0.0],
            uv_tex_shift: [0.0, 0.0],
            depth_test: false,
        },
        transform: Matrix4::IDENTITY,
    })
}

fn clip_rotated_sprite_to_world_rect(
    tint: [f32; 4],
    center: [f32; 4],
    size: [f32; 2],
    rot_sin_cos: [f32; 2],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    offset_world: [f32; 2],
    clip: WorldRect,
) -> Option<ClippedSpriteObject> {
    let poly = [
        ClipVertex {
            pos: sprite_world_xy(
                center,
                size,
                rot_sin_cos,
                offset_world,
                [-0.5_f32, -0.5_f32],
            ),
            uv: [uv_offset[0], uv_offset[1] + uv_scale[1]],
            color: [1.0; 4],
        },
        ClipVertex {
            pos: sprite_world_xy(center, size, rot_sin_cos, offset_world, [0.5_f32, -0.5_f32]),
            uv: [uv_offset[0] + uv_scale[0], uv_offset[1] + uv_scale[1]],
            color: [1.0; 4],
        },
        ClipVertex {
            pos: sprite_world_xy(center, size, rot_sin_cos, offset_world, [0.5_f32, 0.5_f32]),
            uv: [uv_offset[0] + uv_scale[0], uv_offset[1]],
            color: [1.0; 4],
        },
        ClipVertex {
            pos: sprite_world_xy(center, size, rot_sin_cos, offset_world, [-0.5_f32, 0.5_f32]),
            uv: [uv_offset[0], uv_offset[1]],
            color: [1.0; 4],
        },
    ];
    let clipped = clip_polygon_to_world_rect(&poly, clip);
    if clipped.len() < 3 {
        return None;
    }

    let mut out = ClippedMesh::new();
    let base = clipped[0];
    let mut i = 1usize;
    while i + 1 < clipped.len() {
        for v in [base, clipped[i], clipped[i + 1]] {
            out.push(renderer::TexturedMeshVertex {
                pos: [v.pos[0], v.pos[1], 0.0],
                uv: v.uv,
                tex_matrix_scale: [1.0, 1.0],
                color: v.color,
            });
        }
        i += 1;
    }

    Some(ClippedSpriteObject {
        object_type: renderer::ObjectType::TexturedMesh {
            tint,
            vertices: renderer::TexturedMeshVertices::Transient(out.into_vec()),
            geom_cache_key: renderer::INVALID_TMESH_CACHE_KEY,
            mode: renderer::MeshMode::Triangles,
            uv_scale: [1.0, 1.0],
            uv_offset: [0.0, 0.0],
            uv_tex_shift: [0.0, 0.0],
            depth_test: false,
        },
        transform: Matrix4::IDENTITY,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        CachedTextLayout, CachedTextMeshVariants, ComposeScratch, TextAttrCursor, TextLayoutCache,
        TextLayoutKey, TextLayoutOverflowPolicy, TextureLookupCache, WorldRect,
        build_cached_text_layout, build_screen, clip_object_to_world_masks,
        clip_sprite_object_to_world_rect, fold_sprite_xy_rot, resolve_sprite_size_like_sm,
        sort_render_objects, wrap_text_lines_by_words,
    };
    use crate::assets;
    use crate::engine::gfx::{
        BlendMode, INVALID_TMESH_CACHE_KEY, MeshMode, ObjectType, RenderObject, TMeshCacheKey,
        TexturedMeshVertex,
    };
    use crate::engine::present::actors::{Actor, SizeSpec, TextAlign, TextAttribute, TextContent};
    use crate::engine::present::font::{Font, Glyph};
    use crate::engine::space::Metrics;
    use glam::Mat4 as Matrix4;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn boxed_lines(lines: &[&str]) -> Vec<Box<str>> {
        lines.iter().map(|line| Box::<str>::from(*line)).collect()
    }

    fn test_layout() -> CachedTextLayout {
        CachedTextLayout {
            layout_seed: 1,
            font_height: 10,
            line_spacing: 10,
            max_logical_width_i: 0,
            glyph_count: 0,
            lines: Vec::new(),
            glyphs: Vec::new(),
            fill_batches: CachedTextMeshVariants::default(),
            stroke_batches: CachedTextMeshVariants::default(),
        }
    }

    fn test_glyph(texture_key: &Arc<str>) -> Glyph {
        Glyph {
            texture_key: Arc::clone(texture_key),
            stroke_texture_key: None,
            tex_rect: [0.0, 0.0, 8.0, 8.0],
            uv_scale: [0.5, 0.5],
            uv_offset: [0.0, 0.0],
            size: [8.0, 10.0],
            offset: [0.0, -10.0],
            advance: 8.0,
            advance_i32: 8,
        }
    }

    fn test_stroked_glyph(texture_key: &Arc<str>, stroke_key: &Arc<str>) -> Glyph {
        let mut glyph = test_glyph(texture_key);
        glyph.stroke_texture_key = Some(Arc::clone(stroke_key));
        glyph
    }

    fn test_font() -> Font {
        let texture_key = Arc::<str>::from("test_font_page");
        let glyph_a = test_glyph(&texture_key);
        let glyph_b = test_glyph(&texture_key);
        let mut glyph_map = HashMap::new();
        glyph_map.insert('A', glyph_a.clone());
        glyph_map.insert('B', glyph_b.clone());
        let mut ascii = std::array::from_fn(|_| None);
        ascii['A' as usize] = Some(glyph_a);
        ascii['B' as usize] = Some(glyph_b);
        Font {
            glyph_map,
            ascii_glyphs: Box::new(ascii),
            default_glyph: None,
            line_spacing: 10,
            height: 10,
            fallback_font_name: None,
            cache_tag: 1,
            chain_key: 1,
            default_stroke_color: [0.0; 4],
            stroke_texture_map: HashMap::new(),
            texture_hints_map: HashMap::new(),
        }
    }

    fn test_font_split_pages() -> Font {
        let texture_key_a = Arc::<str>::from("test_font_page_a");
        let texture_key_b = Arc::<str>::from("test_font_page_b");
        let glyph_a = test_glyph(&texture_key_a);
        let glyph_b = test_glyph(&texture_key_b);
        let mut glyph_map = HashMap::new();
        glyph_map.insert('A', glyph_a.clone());
        glyph_map.insert('B', glyph_b.clone());
        let mut ascii = std::array::from_fn(|_| None);
        ascii['A' as usize] = Some(glyph_a);
        ascii['B' as usize] = Some(glyph_b);
        Font {
            glyph_map,
            ascii_glyphs: Box::new(ascii),
            default_glyph: None,
            line_spacing: 10,
            height: 10,
            fallback_font_name: None,
            cache_tag: 2,
            chain_key: 2,
            default_stroke_color: [0.0; 4],
            stroke_texture_map: HashMap::new(),
            texture_hints_map: HashMap::new(),
        }
    }

    fn test_font_with_stroke() -> Font {
        let texture_key = Arc::<str>::from("test_font_page");
        let stroke_key = Arc::<str>::from("test_font_stroke_page");
        let glyph_a = test_stroked_glyph(&texture_key, &stroke_key);
        let glyph_b = test_stroked_glyph(&texture_key, &stroke_key);
        let mut glyph_map = HashMap::new();
        glyph_map.insert('A', glyph_a.clone());
        glyph_map.insert('B', glyph_b.clone());
        let mut ascii = std::array::from_fn(|_| None);
        ascii['A' as usize] = Some(glyph_a);
        ascii['B' as usize] = Some(glyph_b);
        let mut stroke_texture_map = HashMap::new();
        stroke_texture_map.insert(
            "test_font_page".to_owned(),
            "test_font_stroke_page".to_owned(),
        );
        Font {
            glyph_map,
            ascii_glyphs: Box::new(ascii),
            default_glyph: None,
            line_spacing: 10,
            height: 10,
            fallback_font_name: None,
            cache_tag: 2,
            chain_key: 2,
            default_stroke_color: [1.0; 4],
            stroke_texture_map,
            texture_hints_map: HashMap::new(),
        }
    }

    #[test]
    fn text_layout_builds_only_requested_fill_align() {
        let fonts = HashMap::from([("test", test_font())]);
        let font = fonts.get("test").expect("test font");
        let layout = build_cached_text_layout(font, &fonts, "AB", font.line_spacing, -1, 17);

        assert!(!layout.fill_batches.is_built(TextAlign::Left));
        assert!(!layout.fill_batches.is_built(TextAlign::Center));
        assert!(!layout.fill_batches.is_built(TextAlign::Right));
        assert!(!layout.stroke_batches.is_built(TextAlign::Left));

        let left_batches = layout.fill_batches(TextAlign::Left);
        assert_eq!(left_batches.len(), 1);

        assert!(layout.fill_batches.is_built(TextAlign::Left));
        assert!(!layout.fill_batches.is_built(TextAlign::Center));
        assert!(!layout.fill_batches.is_built(TextAlign::Right));
        assert!(!layout.stroke_batches.is_built(TextAlign::Left));
    }

    #[test]
    fn text_layout_builds_stroke_batches_only_on_demand() {
        let fonts = HashMap::from([("test", test_font_with_stroke())]);
        let font = fonts.get("test").expect("test font");
        let layout = build_cached_text_layout(font, &fonts, "AB", font.line_spacing, -1, 23);

        assert!(!layout.stroke_batches.is_built(TextAlign::Left));

        let stroke_batches = layout.stroke_batches(TextAlign::Left);
        assert_eq!(stroke_batches.len(), 1);
        assert!(layout.stroke_batches.is_built(TextAlign::Left));
        assert!(!layout.stroke_batches.is_built(TextAlign::Center));
        assert!(!layout.fill_batches.is_built(TextAlign::Left));
    }

    fn test_render_object(z: i16, order: u32) -> RenderObject {
        RenderObject {
            object_type: ObjectType::Sprite {
                center: [0.0, 0.0, 0.0, 0.0],
                size: [1.0, 1.0],
                rot_sin_cos: [0.0, 1.0],
                tint: [1.0; 4],
                uv_scale: [1.0, 1.0],
                uv_offset: [0.0, 0.0],
                local_offset: [0.0, 0.0],
                local_offset_rot_sin_cos: [0.0, 1.0],
                edge_fade: [0.0; 4],
            },
            texture_handle: 0,
            transform: Matrix4::IDENTITY,
            blend: BlendMode::Alpha,
            z,
            order,
            camera: 0,
        }
    }

    #[test]
    fn wrapwidthpixels_wraps_on_spaces() {
        let lines = wrap_text_lines_by_words("A BB CCC", 3, 1, |word| word.len() as i32);
        assert_eq!(lines, boxed_lines(&["A", "BB", "CCC"]));
    }

    #[test]
    fn wrapwidthpixels_keeps_empty_lines() {
        let lines = wrap_text_lines_by_words("AA\n\nBB CC", 5, 1, |word| word.len() as i32);
        assert_eq!(lines, boxed_lines(&["AA", "", "BB CC"]));
    }

    #[test]
    fn wrapwidthpixels_keeps_long_word_on_own_line() {
        let lines = wrap_text_lines_by_words("AAAA BB", 3, 1, |word| word.len() as i32);
        assert_eq!(lines, boxed_lines(&["AAAA", "BB"]));
    }

    #[test]
    fn text_attr_cursor_uses_last_matching_attribute() {
        let attrs = [
            TextAttribute {
                start: 2,
                length: 4,
                color: [1.0, 0.0, 0.0, 1.0],
                vertex_colors: None,
                glow: None,
            },
            TextAttribute {
                start: 3,
                length: 2,
                color: [0.0, 1.0, 0.0, 1.0],
                vertex_colors: None,
                glow: None,
            },
            TextAttribute {
                start: 2,
                length: 1,
                color: [0.0, 0.0, 1.0, 1.0],
                vertex_colors: None,
                glow: None,
            },
        ];
        let mut cursor = TextAttrCursor::new(&attrs).expect("attributes should build a cursor");

        assert_eq!(cursor.tint_for(0), [1.0; 4]);
        assert_eq!(cursor.tint_for(2), [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(cursor.tint_for(3), [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(cursor.tint_for(5), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(cursor.tint_for(6), [1.0; 4]);
    }

    #[test]
    fn text_attr_cursor_keeps_slice_order_precedence_with_unsorted_starts() {
        let attrs = [
            TextAttribute {
                start: 5,
                length: 1,
                color: [0.0, 1.0, 0.0, 1.0],
                vertex_colors: None,
                glow: None,
            },
            TextAttribute {
                start: 0,
                length: 10,
                color: [1.0, 0.0, 0.0, 1.0],
                vertex_colors: None,
                glow: None,
            },
        ];
        let mut cursor = TextAttrCursor::new(&attrs).expect("attributes should build a cursor");

        assert_eq!(cursor.tint_for(5), [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn text_attr_cursor_handles_skipped_char_indices() {
        let attrs = [
            TextAttribute {
                start: 1,
                length: 1,
                color: [1.0, 0.0, 0.0, 1.0],
                vertex_colors: None,
                glow: None,
            },
            TextAttribute {
                start: 2,
                length: 3,
                color: [0.0, 1.0, 0.0, 1.0],
                vertex_colors: None,
                glow: None,
            },
            TextAttribute {
                start: 5,
                length: 2,
                color: [0.0, 0.0, 1.0, 1.0],
                vertex_colors: None,
                glow: None,
            },
        ];
        let mut cursor = TextAttrCursor::new(&attrs).expect("attributes should build a cursor");

        assert_eq!(cursor.tint_for(0), [1.0; 4]);
        assert_eq!(cursor.tint_for(3), [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(cursor.tint_for(6), [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn mask_clip_chooses_largest_intersection() {
        let mut obj = RenderObject {
            object_type: ObjectType::Sprite {
                center: [0.0, 0.0, 0.0, 0.0],
                size: [10.0, 10.0],
                rot_sin_cos: [0.0, 1.0],
                tint: [1.0; 4],
                uv_scale: [1.0, 1.0],
                uv_offset: [0.0, 0.0],
                local_offset: [0.0, 0.0],
                local_offset_rot_sin_cos: [0.0, 1.0],
                edge_fade: [0.0; 4],
            },
            texture_handle: 0,
            transform: Matrix4::IDENTITY,
            blend: BlendMode::Alpha,
            z: 0,
            order: 0,
            camera: 0,
        };

        assert!(clip_object_to_world_masks(
            &mut obj,
            &[
                WorldRect {
                    left: -2.0,
                    right: 2.0,
                    bottom: -2.0,
                    top: 2.0,
                },
                WorldRect {
                    left: -5.0,
                    right: 5.0,
                    bottom: -5.0,
                    top: 5.0,
                },
            ],
        ));

        if let ObjectType::Sprite {
            size,
            uv_scale,
            uv_offset,
            ..
        } = &obj.object_type
        {
            assert_eq!(*size, [10.0, 10.0]);
            assert_eq!(*uv_scale, [1.0, 1.0]);
            assert_eq!(*uv_offset, [0.0, 0.0]);
        } else {
            panic!("expected sprite to remain in fast clip path");
        }
        assert_eq!(obj.transform, Matrix4::IDENTITY);
    }

    #[test]
    fn rotated_clip_preserves_texture_handle() {
        let mut obj = RenderObject {
            object_type: ObjectType::Sprite {
                center: [0.0, 0.0, 0.0, 0.0],
                size: [10.0, 10.0],
                rot_sin_cos: [45.0_f32.to_radians().sin(), 45.0_f32.to_radians().cos()],
                tint: [0.25, 0.5, 0.75, 1.0],
                uv_scale: [1.0, 1.0],
                uv_offset: [0.0, 0.0],
                local_offset: [0.0, 0.0],
                local_offset_rot_sin_cos: [0.0, 1.0],
                edge_fade: [0.0; 4],
            },
            texture_handle: 17,
            transform: Matrix4::IDENTITY,
            blend: BlendMode::Alpha,
            z: 0,
            order: 0,
            camera: 0,
        };

        assert!(clip_sprite_object_to_world_rect(
            &mut obj,
            WorldRect {
                left: -3.0,
                right: 3.0,
                bottom: -3.0,
                top: 3.0,
            },
        ));

        match &obj.object_type {
            ObjectType::TexturedMesh { vertices, .. } => {
                assert_eq!(obj.texture_handle, 17);
                assert!(!vertices.is_empty());
            }
            _ => panic!("expected rotated clip to produce textured mesh"),
        }
    }

    #[test]
    fn texture_lookup_cache_clears_frame_ptr_tables_each_frame() {
        let mut cache = TextureLookupCache::default();
        let key = Arc::<str>::from("frame_tex");
        let key_ptr = key.as_ref() as *const str;
        let key_addr = TextureLookupCache::ptr_cache_key(key_ptr);
        cache.generation = assets::texture_registry_generation();
        cache
            .dims
            .insert(key.to_string(), assets::TexMeta { w: 64, h: 32 });
        cache.sheets.insert(key.to_string(), (4, 2));
        cache.handles.insert(key.to_string(), 11);

        let Some(meta) = cache.texture_dims_with_ptr(Some(key_ptr), key.as_ref()) else {
            panic!("expected cached texture dims");
        };
        assert_eq!(meta.w, 64);
        assert_eq!(meta.h, 32);
        assert_eq!(
            cache.sprite_sheet_dims_with_ptr(Some(key_ptr), key.as_ref()),
            (4, 2)
        );
        assert_eq!(
            cache.texture_handle_with_ptr(Some(key_ptr), key.as_ref()),
            11
        );
        let Some(frame_meta) = cache.frame_dims.get(&key_addr) else {
            panic!("expected frame-local texture dims");
        };
        assert_eq!(frame_meta.w, 64);
        assert_eq!(frame_meta.h, 32);
        assert_eq!(cache.frame_sheets.get(&key_addr), Some(&(4, 2)));
        assert_eq!(cache.frame_handles.get(&key_addr), Some(&11));

        cache.begin_frame();

        assert!(cache.frame_dims.is_empty());
        assert!(cache.frame_sheets.is_empty());
        assert!(cache.frame_handles.is_empty());
        let Some(meta) = cache.dims.get("frame_tex") else {
            panic!("expected persistent texture dims");
        };
        assert_eq!(meta.w, 64);
        assert_eq!(meta.h, 32);
        assert_eq!(cache.sheets.get("frame_tex"), Some(&(4, 2)));
        assert_eq!(cache.handles.get("frame_tex"), Some(&11));
    }

    #[test]
    fn texture_lookup_cache_keeps_stable_ptr_tables_across_frames() {
        let mut cache = TextureLookupCache::default();
        const KEY: &str = "stable_tex";
        let key_ptr = KEY as *const str;
        let key_addr = TextureLookupCache::ptr_cache_key(key_ptr);
        cache.generation = assets::texture_registry_generation();
        cache
            .dims
            .insert(KEY.to_string(), assets::TexMeta { w: 128, h: 64 });
        cache.sheets.insert(KEY.to_string(), (8, 4));
        cache.handles.insert(KEY.to_string(), 23);

        let Some(meta) = cache.texture_dims_cached(Some(key_ptr), KEY, true) else {
            panic!("expected cached texture dims");
        };
        assert_eq!(meta.w, 128);
        assert_eq!(meta.h, 64);
        assert_eq!(
            cache.sprite_sheet_dims_cached(Some(key_ptr), KEY, true),
            (8, 4)
        );
        assert_eq!(cache.texture_handle_cached(Some(key_ptr), KEY, true), 23);
        assert!(cache.frame_dims.is_empty());
        assert!(cache.frame_sheets.is_empty());
        assert!(cache.frame_handles.is_empty());
        assert_eq!(
            cache
                .stable_dims
                .get(&key_addr)
                .map(|meta| (meta.w, meta.h)),
            Some((128, 64))
        );
        assert_eq!(cache.stable_sheets.get(&key_addr), Some(&(8, 4)));
        assert_eq!(cache.stable_handles.get(&key_addr), Some(&23));

        cache.begin_frame();

        assert_eq!(
            cache
                .stable_dims
                .get(&key_addr)
                .map(|meta| (meta.w, meta.h)),
            Some((128, 64))
        );
        assert_eq!(cache.stable_sheets.get(&key_addr), Some(&(8, 4)));
        assert_eq!(cache.stable_handles.get(&key_addr), Some(&23));
    }

    #[test]
    fn custom_texture_rect_does_not_change_native_sprite_size() {
        const KEY: &str = "compose_test/custom_rect_size.png";
        assets::register_texture_dims(KEY, 256, 128);
        let mut cache = TextureLookupCache::default();

        let plain = resolve_sprite_size_like_sm(
            [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            false,
            KEY,
            None,
            false,
            None,
            None,
            None,
            [1.0, 1.0],
            &mut cache,
        );
        let repeated = resolve_sprite_size_like_sm(
            [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            false,
            KEY,
            None,
            false,
            Some([0.0, 0.0, 60.0, 60.0]),
            None,
            None,
            [1.0, 1.0],
            &mut cache,
        );
        let zoomed = resolve_sprite_size_like_sm(
            [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            false,
            KEY,
            None,
            false,
            Some([0.0, 0.0, 60.0, 60.0]),
            None,
            None,
            [20.0, 20.0],
            &mut cache,
        );

        fn assert_px_size(size: [SizeSpec; 2], want: [f32; 2]) {
            let [SizeSpec::Px(got_w), SizeSpec::Px(got_h)] = size else {
                panic!("expected pixel size, got {size:?}");
            };
            assert!(
                (got_w - want[0]).abs() <= 1e-6,
                "width mismatch: {got_w} vs {}",
                want[0]
            );
            assert!(
                (got_h - want[1]).abs() <= 1e-6,
                "height mismatch: {got_h} vs {}",
                want[1]
            );
        }

        assert_px_size(plain, [256.0, 128.0]);
        assert_px_size(repeated, [256.0, 128.0]);
        assert_px_size(zoomed, [5120.0, 2560.0]);
    }

    #[test]
    fn sprite_rotationy_180_folds_to_horizontal_flip() {
        let (flip_x, flip_y, size_x, size_y) =
            fold_sprite_xy_rot(false, false, 22.0, 10.0, 0.0, 180.0);
        assert!(flip_x);
        assert!(!flip_y);
        assert!((size_x - 22.0).abs() < 0.0001);
        assert!((size_y - 10.0).abs() < 0.0001);
    }

    #[test]
    fn sprite_rotationx_180_folds_to_vertical_flip() {
        let (flip_x, flip_y, size_x, size_y) =
            fold_sprite_xy_rot(false, false, 22.0, 10.0, 180.0, 0.0);
        assert!(!flip_x);
        assert!(flip_y);
        assert!((size_x - 22.0).abs() < 0.0001);
        assert!((size_y - 10.0).abs() < 0.0001);
    }

    #[test]
    fn lock_growth_saturates_future_inserts() {
        let key = TextLayoutKey {
            font_key: 7,
            line_spacing: 10,
            wrap_width_pixels: -1,
        };
        let mut cache =
            TextLayoutCache::new_with_policy(4, TextLayoutOverflowPolicy::PruneOwnedEntries);
        assert!(cache.insert_owned_layout(key, "alpha", Arc::new(test_layout()), 1));
        assert_eq!(cache.entry_count, 1);

        cache.lock_growth();

        assert_eq!(cache.max_entries, 1);
        assert_eq!(cache.max_aliases, 0);
        assert!(cache.overflow_policy == TextLayoutOverflowPolicy::Saturating);
        assert!(!cache.insert_owned_layout(key, "beta", Arc::new(test_layout()), 2));
        assert_eq!(cache.entry_count, 1);
        assert_eq!(cache.frame_stats.prunes, 0);
        assert!(cache.owned_layout(key, "beta").is_none());
        assert!(cache.uncached_layout.is_some());
    }

    #[test]
    fn text_layout_cache_defaults_to_saturating() {
        assert!(TextLayoutOverflowPolicy::default() == TextLayoutOverflowPolicy::Saturating);
        assert!(TextLayoutCache::default().overflow_policy == TextLayoutOverflowPolicy::Saturating);
        assert!(TextLayoutCache::new(4).overflow_policy == TextLayoutOverflowPolicy::Saturating);
        assert!(
            TextLayoutCache::pruning(4).overflow_policy
                == TextLayoutOverflowPolicy::PruneOwnedEntries
        );
    }

    #[test]
    fn recycle_render_list_recovers_transient_textured_mesh_vertices() {
        let mut scratch = ComposeScratch::default();
        let mut render = crate::engine::gfx::RenderList {
            clear_color: [0.0, 0.0, 0.0, 1.0],
            cameras: Vec::new(),
            objects: vec![RenderObject {
                object_type: ObjectType::TexturedMesh {
                    tint: [1.0; 4],
                    vertices: crate::engine::gfx::TexturedMeshVertices::Transient(vec![
                        TexturedMeshVertex::default();
                        6
                    ]),
                    geom_cache_key: INVALID_TMESH_CACHE_KEY,
                    mode: MeshMode::Triangles,
                    uv_scale: [1.0, 1.0],
                    uv_offset: [0.0, 0.0],
                    uv_tex_shift: [0.0, 0.0],
                    depth_test: false,
                },
                texture_handle: 9,
                transform: Matrix4::IDENTITY,
                blend: BlendMode::Alpha,
                z: 0,
                order: 0,
                camera: 0,
            }],
        };

        scratch.recycle_render_list(&mut render);

        assert!(scratch.objects.is_empty());
        assert_eq!(scratch.recycled_text_mesh_vertices.len(), 1);
        assert!(scratch.recycled_text_mesh_vertices[0].is_empty());
        assert!(scratch.recycled_text_mesh_vertices[0].capacity() >= 6);
    }

    #[test]
    fn sort_render_objects_repairs_equal_z_order() {
        let mut objects = vec![test_render_object(5, 2), test_render_object(5, 1)];
        let mut scratch = ComposeScratch::default();

        sort_render_objects(&mut objects, &mut scratch);

        let keys = objects
            .iter()
            .map(|obj| (obj.z, obj.order))
            .collect::<Vec<_>>();
        assert_eq!(keys, vec![(5, 1), (5, 2)]);
    }

    #[test]
    fn sort_render_objects_falls_back_when_dense_buckets_keep_bad_order() {
        let mut objects = vec![
            test_render_object(5, 3),
            test_render_object(4, 0),
            test_render_object(5, 1),
            test_render_object(5, 2),
        ];
        let mut scratch = ComposeScratch::default();

        sort_render_objects(&mut objects, &mut scratch);

        let keys = objects
            .iter()
            .map(|obj| (obj.z, obj.order))
            .collect::<Vec<_>>();
        assert_eq!(keys, vec![(4, 0), (5, 1), (5, 2), (5, 3)]);
    }

    #[test]
    fn shadowed_textured_mesh_keeps_geom_cache_key() {
        const CACHE_KEY: TMeshCacheKey = 77;
        let metrics = Metrics {
            left: 0.0,
            right: 100.0,
            top: 100.0,
            bottom: 0.0,
        };
        let mesh = Actor::TexturedMesh {
            align: [0.0, 0.0],
            offset: [10.0, 20.0],
            world_z: 0.0,
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            local_transform: Matrix4::IDENTITY,
            texture: Arc::from("mesh"),
            tint: [0.25, 0.5, 0.75, 0.8],
            vertices: Arc::from(vec![TexturedMeshVertex::default(); 3]),
            geom_cache_key: CACHE_KEY,
            mode: MeshMode::Triangles,
            uv_scale: [1.0, 1.0],
            uv_offset: [0.0, 0.0],
            uv_tex_shift: [0.0, 0.0],
            depth_test: false,
            visible: true,
            blend: BlendMode::Alpha,
            z: 5,
        };
        let actors = [Actor::Shadow {
            len: [4.0, 3.0],
            color: [0.5, 0.25, 0.75, 0.5],
            child: Box::new(mesh),
        }];
        let fonts = HashMap::new();
        let render = build_screen(&actors, [0.0, 0.0, 0.0, 1.0], &metrics, &fonts, 0.0);

        assert_eq!(render.objects.len(), 2);

        let shadow = render
            .objects
            .iter()
            .find(|obj| obj.z == 4)
            .expect("shadow draw should be present");
        let original = render
            .objects
            .iter()
            .find(|obj| obj.z == 5)
            .expect("original draw should be present");

        match (&shadow.object_type, &original.object_type) {
            (
                crate::engine::gfx::ObjectType::TexturedMesh {
                    tint: shadow_tint,
                    geom_cache_key: shadow_key,
                    ..
                },
                crate::engine::gfx::ObjectType::TexturedMesh {
                    tint: original_tint,
                    geom_cache_key: original_key,
                    ..
                },
            ) => {
                assert_eq!(*shadow_key, CACHE_KEY);
                assert_eq!(*original_key, CACHE_KEY);
                assert_eq!(*original_tint, [0.25, 0.5, 0.75, 0.8]);
                assert_eq!(*shadow_tint, [0.125, 0.125, 0.5625, 0.4]);
            }
            _ => panic!("expected textured-mesh objects"),
        }
    }

    #[test]
    fn simple_left_aligned_text_batches_into_textured_mesh() {
        let metrics = Metrics {
            left: 0.0,
            right: 200.0,
            top: 100.0,
            bottom: 0.0,
        };
        let actors = [Actor::Text {
            align: [0.0, 0.0],
            offset: [10.0, 20.0],
            local_transform: Matrix4::IDENTITY,
            color: [0.5, 0.75, 1.0, 1.0],
            stroke_color: None,
            glow: [0.0; 4],
            font: "test",
            content: TextContent::static_str("AB"),
            attributes: Vec::new(),
            align_text: TextAlign::Left,
            z: 3,
            scale: [1.0, 1.0],
            fit_width: None,
            fit_height: None,
            line_spacing: None,
            wrap_width_pixels: None,
            max_width: None,
            max_height: None,
            max_w_pre_zoom: false,
            max_h_pre_zoom: false,
            jitter: false,
            distortion: 0.0,
            clip: None,
            mask_dest: false,
            blend: BlendMode::Alpha,
            shadow_len: [0.0, 0.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            effect: Default::default(),
        }];
        let fonts = HashMap::from([("test", test_font())]);
        let render = build_screen(&actors, [0.0, 0.0, 0.0, 1.0], &metrics, &fonts, 0.0);

        assert_eq!(render.objects.len(), 1);
        match &render.objects[0].object_type {
            ObjectType::TexturedMesh {
                tint,
                vertices,
                geom_cache_key,
                ..
            } => {
                assert_eq!(*tint, [0.5, 0.75, 1.0, 1.0]);
                assert_eq!(vertices.len(), 12);
                assert_ne!(*geom_cache_key, INVALID_TMESH_CACHE_KEY);
            }
            _ => panic!("expected batched text to use textured mesh"),
        }
    }

    #[test]
    fn clipped_left_aligned_batched_text_stays_textured_mesh() {
        let metrics = Metrics {
            left: 0.0,
            right: 200.0,
            top: 100.0,
            bottom: 0.0,
        };
        let actors = [Actor::Text {
            align: [0.0, 0.0],
            offset: [10.0, 20.0],
            local_transform: Matrix4::IDENTITY,
            color: [1.0; 4],
            stroke_color: None,
            glow: [0.0; 4],
            font: "test",
            content: TextContent::static_str("AB"),
            attributes: Vec::new(),
            align_text: TextAlign::Left,
            z: 0,
            scale: [1.0, 1.0],
            fit_width: None,
            fit_height: None,
            line_spacing: None,
            wrap_width_pixels: None,
            max_width: None,
            max_height: None,
            max_w_pre_zoom: false,
            max_h_pre_zoom: false,
            jitter: false,
            distortion: 0.0,
            clip: Some([10.0, 20.0, 4.0, 10.0]),
            mask_dest: false,
            blend: BlendMode::Alpha,
            shadow_len: [0.0, 0.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            effect: Default::default(),
        }];
        let fonts = HashMap::from([("test", test_font())]);
        let render = build_screen(&actors, [0.0, 0.0, 0.0, 1.0], &metrics, &fonts, 0.0);

        assert_eq!(render.objects.len(), 1);
        match &render.objects[0].object_type {
            ObjectType::TexturedMesh {
                vertices,
                geom_cache_key,
                ..
            } => {
                assert!(!vertices.is_empty());
                assert_eq!(*geom_cache_key, INVALID_TMESH_CACHE_KEY);
            }
            _ => panic!("expected clipped batched text to remain textured mesh"),
        }
    }

    #[test]
    fn fully_inside_clipped_text_keeps_cached_mesh() {
        let metrics = Metrics {
            left: 0.0,
            right: 200.0,
            top: 100.0,
            bottom: 0.0,
        };
        let actors = [Actor::Text {
            align: [0.0, 0.0],
            offset: [10.0, 20.0],
            local_transform: Matrix4::IDENTITY,
            color: [1.0; 4],
            stroke_color: None,
            glow: [0.0; 4],
            font: "test",
            content: TextContent::static_str("AB"),
            attributes: Vec::new(),
            align_text: TextAlign::Left,
            z: 0,
            scale: [1.0, 1.0],
            fit_width: None,
            fit_height: None,
            line_spacing: None,
            wrap_width_pixels: None,
            max_width: None,
            max_height: None,
            max_w_pre_zoom: false,
            max_h_pre_zoom: false,
            jitter: false,
            distortion: 0.0,
            clip: Some([0.0, 0.0, 200.0, 100.0]),
            mask_dest: false,
            blend: BlendMode::Alpha,
            shadow_len: [0.0, 0.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            effect: Default::default(),
        }];
        let fonts = HashMap::from([("test", test_font())]);
        let render = build_screen(&actors, [0.0, 0.0, 0.0, 1.0], &metrics, &fonts, 0.0);

        assert_eq!(render.objects.len(), 1);
        match &render.objects[0].object_type {
            ObjectType::TexturedMesh { geom_cache_key, .. } => {
                assert_ne!(*geom_cache_key, INVALID_TMESH_CACHE_KEY);
            }
            _ => panic!("expected fully inside clipped text to keep cached textured mesh"),
        }
    }

    #[test]
    fn centered_text_batches_into_textured_mesh() {
        let metrics = Metrics {
            left: 0.0,
            right: 200.0,
            top: 100.0,
            bottom: 0.0,
        };
        let actors = [Actor::Text {
            align: [0.0, 0.0],
            offset: [10.0, 20.0],
            local_transform: Matrix4::IDENTITY,
            color: [1.0; 4],
            stroke_color: None,
            glow: [0.0; 4],
            font: "test",
            content: TextContent::static_str("AB"),
            attributes: Vec::new(),
            align_text: TextAlign::Center,
            z: 0,
            scale: [1.0, 1.0],
            fit_width: None,
            fit_height: None,
            line_spacing: None,
            wrap_width_pixels: None,
            max_width: None,
            max_height: None,
            max_w_pre_zoom: false,
            max_h_pre_zoom: false,
            jitter: false,
            distortion: 0.0,
            clip: None,
            mask_dest: false,
            blend: BlendMode::Alpha,
            shadow_len: [0.0, 0.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            effect: Default::default(),
        }];
        let fonts = HashMap::from([("test", test_font())]);
        let render = build_screen(&actors, [0.0, 0.0, 0.0, 1.0], &metrics, &fonts, 0.0);

        assert_eq!(render.objects.len(), 1);
        match &render.objects[0].object_type {
            ObjectType::TexturedMesh {
                vertices,
                geom_cache_key,
                ..
            } => {
                assert_eq!(vertices.len(), 12);
                assert_ne!(*geom_cache_key, INVALID_TMESH_CACHE_KEY);
            }
            _ => panic!("expected centered text to use batched textured mesh"),
        }
    }

    #[test]
    fn attributed_text_batches_into_transient_textured_mesh() {
        let metrics = Metrics {
            left: 0.0,
            right: 200.0,
            top: 100.0,
            bottom: 0.0,
        };
        let actors = [Actor::Text {
            align: [0.0, 0.0],
            offset: [10.0, 20.0],
            local_transform: Matrix4::IDENTITY,
            color: [1.0; 4],
            stroke_color: None,
            glow: [0.0; 4],
            font: "test",
            content: TextContent::static_str("AB"),
            attributes: vec![TextAttribute {
                start: 1,
                length: 1,
                color: [0.0, 1.0, 0.0, 1.0],
                vertex_colors: None,
                glow: None,
            }],
            align_text: TextAlign::Left,
            z: 0,
            scale: [1.0, 1.0],
            fit_width: None,
            fit_height: None,
            line_spacing: None,
            wrap_width_pixels: None,
            max_width: None,
            max_height: None,
            max_w_pre_zoom: false,
            max_h_pre_zoom: false,
            jitter: false,
            distortion: 0.0,
            clip: None,
            mask_dest: false,
            blend: BlendMode::Alpha,
            shadow_len: [0.0, 0.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            effect: Default::default(),
        }];
        let fonts = HashMap::from([("test", test_font())]);
        let render = build_screen(&actors, [0.0, 0.0, 0.0, 1.0], &metrics, &fonts, 0.0);

        assert_eq!(render.objects.len(), 1);
        match &render.objects[0].object_type {
            ObjectType::TexturedMesh {
                vertices,
                geom_cache_key,
                ..
            } => {
                assert_eq!(vertices.len(), 12);
                assert_eq!(*geom_cache_key, INVALID_TMESH_CACHE_KEY);
                assert_eq!(vertices[0].color, [1.0; 4]);
                assert_eq!(vertices[6].color, [0.0, 1.0, 0.0, 1.0]);
            }
            _ => panic!("expected attributed text to use transient textured mesh"),
        }
    }

    #[test]
    fn attributed_text_applies_corner_colors_to_glyph_vertices() {
        let metrics = Metrics {
            left: 0.0,
            right: 200.0,
            top: 100.0,
            bottom: 0.0,
        };
        let colors = [
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
            [1.0, 1.0, 0.0, 1.0],
        ];
        let actors = [Actor::Text {
            align: [0.0, 0.0],
            offset: [10.0, 20.0],
            local_transform: Matrix4::IDENTITY,
            color: [1.0; 4],
            stroke_color: None,
            glow: [0.0; 4],
            font: "test",
            content: TextContent::static_str("A"),
            attributes: vec![TextAttribute {
                start: 0,
                length: 1,
                color: colors[0],
                vertex_colors: Some(colors),
                glow: None,
            }],
            align_text: TextAlign::Left,
            z: 0,
            scale: [1.0, 1.0],
            fit_width: None,
            fit_height: None,
            line_spacing: None,
            wrap_width_pixels: None,
            max_width: None,
            max_height: None,
            max_w_pre_zoom: false,
            max_h_pre_zoom: false,
            jitter: false,
            distortion: 0.0,
            clip: None,
            mask_dest: false,
            blend: BlendMode::Alpha,
            shadow_len: [0.0, 0.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            effect: Default::default(),
        }];
        let fonts = HashMap::from([("test", test_font())]);
        let render = build_screen(&actors, [1.0; 4], &metrics, &fonts, 0.0);

        let ObjectType::TexturedMesh { vertices, .. } = &render.objects[0].object_type else {
            panic!("expected attributed text to use textured mesh");
        };
        assert_eq!(vertices[0].color, colors[0]);
        assert_eq!(vertices[1].color, colors[2]);
        assert_eq!(vertices[2].color, colors[3]);
        assert_eq!(vertices[3].color, colors[0]);
        assert_eq!(vertices[4].color, colors[3]);
        assert_eq!(vertices[5].color, colors[1]);
    }

    #[test]
    fn jittered_text_uses_transient_offset_vertices() {
        let metrics = Metrics {
            left: 0.0,
            right: 200.0,
            top: 100.0,
            bottom: 0.0,
        };
        let mut actor = Actor::Text {
            align: [0.0, 0.0],
            offset: [10.0, 20.0],
            local_transform: Matrix4::IDENTITY,
            color: [1.0; 4],
            stroke_color: None,
            glow: [0.0; 4],
            font: "test",
            content: TextContent::static_str("A"),
            attributes: Vec::new(),
            align_text: TextAlign::Left,
            z: 0,
            scale: [1.0, 1.0],
            fit_width: None,
            fit_height: None,
            line_spacing: None,
            wrap_width_pixels: None,
            max_width: None,
            max_height: None,
            max_w_pre_zoom: false,
            max_h_pre_zoom: false,
            jitter: false,
            distortion: 0.0,
            clip: None,
            mask_dest: false,
            blend: BlendMode::Alpha,
            shadow_len: [0.0, 0.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            effect: Default::default(),
        };
        let fonts = HashMap::from([("test", test_font())]);
        let base = build_screen(&[actor.clone()], [1.0; 4], &metrics, &fonts, 0.25);
        let Actor::Text { jitter, .. } = &mut actor else {
            panic!("expected text actor");
        };
        *jitter = true;
        let jittered = build_screen(&[actor], [1.0; 4], &metrics, &fonts, 0.25);

        let ObjectType::TexturedMesh {
            vertices: base_vertices,
            ..
        } = &base.objects[0].object_type
        else {
            panic!("expected base text mesh");
        };
        let ObjectType::TexturedMesh {
            vertices: jittered_vertices,
            geom_cache_key,
            ..
        } = &jittered.objects[0].object_type
        else {
            panic!("expected jittered text mesh");
        };
        assert_eq!(*geom_cache_key, INVALID_TMESH_CACHE_KEY);
        assert_ne!(jittered_vertices[0].pos, base_vertices[0].pos);
    }

    #[test]
    fn distorted_text_uses_transient_corner_offsets() {
        let metrics = Metrics {
            left: 0.0,
            right: 200.0,
            top: 100.0,
            bottom: 0.0,
        };
        let mut actor = Actor::Text {
            align: [0.0, 0.0],
            offset: [10.0, 20.0],
            local_transform: Matrix4::IDENTITY,
            color: [1.0; 4],
            stroke_color: None,
            glow: [0.0; 4],
            font: "test",
            content: TextContent::static_str("A"),
            attributes: Vec::new(),
            align_text: TextAlign::Left,
            z: 0,
            scale: [1.0, 1.0],
            fit_width: None,
            fit_height: None,
            line_spacing: None,
            wrap_width_pixels: None,
            max_width: None,
            max_height: None,
            max_w_pre_zoom: false,
            max_h_pre_zoom: false,
            jitter: false,
            distortion: 0.0,
            clip: None,
            mask_dest: false,
            blend: BlendMode::Alpha,
            shadow_len: [0.0, 0.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            effect: Default::default(),
        };
        let fonts = HashMap::from([("test", test_font())]);
        let base = build_screen(&[actor.clone()], [1.0; 4], &metrics, &fonts, 0.0);
        let Actor::Text { distortion, .. } = &mut actor else {
            panic!("expected text actor");
        };
        *distortion = 0.5;
        let distorted = build_screen(&[actor], [1.0; 4], &metrics, &fonts, 0.0);

        let ObjectType::TexturedMesh {
            vertices: base_vertices,
            ..
        } = &base.objects[0].object_type
        else {
            panic!("expected base text mesh");
        };
        let ObjectType::TexturedMesh {
            vertices: distorted_vertices,
            geom_cache_key,
            ..
        } = &distorted.objects[0].object_type
        else {
            panic!("expected distorted text mesh");
        };
        assert_eq!(*geom_cache_key, INVALID_TMESH_CACHE_KEY);
        assert!(
            base_vertices
                .iter()
                .zip(distorted_vertices.iter())
                .any(|(base, distorted)| base.pos != distorted.pos)
        );
    }

    #[test]
    fn attributed_text_keeps_colors_across_texture_batches() {
        let metrics = Metrics {
            left: 0.0,
            right: 200.0,
            top: 100.0,
            bottom: 0.0,
        };
        let actors = [Actor::Text {
            align: [0.0, 0.0],
            offset: [10.0, 20.0],
            local_transform: Matrix4::IDENTITY,
            color: [1.0; 4],
            stroke_color: None,
            glow: [0.0; 4],
            font: "test",
            content: TextContent::static_str("AB"),
            attributes: vec![TextAttribute {
                start: 1,
                length: 1,
                color: [0.0, 1.0, 0.0, 1.0],
                vertex_colors: None,
                glow: None,
            }],
            align_text: TextAlign::Left,
            z: 0,
            scale: [1.0, 1.0],
            fit_width: None,
            fit_height: None,
            line_spacing: None,
            wrap_width_pixels: None,
            max_width: None,
            max_height: None,
            max_w_pre_zoom: false,
            max_h_pre_zoom: false,
            jitter: false,
            distortion: 0.0,
            clip: None,
            mask_dest: false,
            blend: BlendMode::Alpha,
            shadow_len: [0.0, 0.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            effect: Default::default(),
        }];
        let fonts = HashMap::from([("test", test_font_split_pages())]);
        let render = build_screen(&actors, [0.0, 0.0, 0.0, 1.0], &metrics, &fonts, 0.0);

        assert_eq!(render.objects.len(), 2);
        match (
            &render.objects[0].object_type,
            &render.objects[1].object_type,
        ) {
            (
                ObjectType::TexturedMesh {
                    vertices: first_vertices,
                    geom_cache_key: first_key,
                    ..
                },
                ObjectType::TexturedMesh {
                    vertices: second_vertices,
                    geom_cache_key: second_key,
                    ..
                },
            ) => {
                assert_eq!(*first_key, INVALID_TMESH_CACHE_KEY);
                assert_eq!(*second_key, INVALID_TMESH_CACHE_KEY);
                assert_eq!(first_vertices[0].color, [1.0; 4]);
                assert_eq!(second_vertices[0].color, [0.0, 1.0, 0.0, 1.0]);
            }
            _ => panic!("expected attributed text to batch into textured meshes"),
        }
    }
}
