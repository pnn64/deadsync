// Frozen geometry resolver from 8b0c3004f; only test module imports are added.
use super::*;

#[derive(Debug, Default)]
pub struct TexturedMeshUploads {
    pub vertices: Vec<TexturedMeshVertex>,
    pub sources: Vec<TexturedMeshSource>,
    cache_keys: Vec<TMeshCacheKey>,
    all_cached: bool,
}

impl TexturedMeshUploads {
    #[must_use]
    pub fn with_capacity(vertices: usize, geometries: usize) -> Self {
        Self {
            vertices: Vec::with_capacity(vertices),
            sources: Vec::with_capacity(geometries),
            cache_keys: Vec::with_capacity(geometries),
            all_cached: false,
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn source(&self, geometry: u32) -> Option<TexturedMeshSource> {
        self.sources.get(geometry as usize).copied()
    }
}

/// Resolves frame geometry to retained or frame-local upload storage.
///
/// `ensure_cached` returns a non-zero backend-local buffer slot when the
/// geometry is retained. The same identity must always refer to the same GPU
/// buffer for the lifetime of `uploads`' consumer.
pub fn resolve_textured_meshes<EnsureCached>(
    frame: &RenderFrame,
    uploads: &mut TexturedMeshUploads,
    ensure_cached: EnsureCached,
) where
    EnsureCached: FnMut(TMeshCacheKey, &[TexturedMeshVertex]) -> Option<u64>,
{
    resolve_textured_mesh_geometries(&frame.tmesh_geometries, uploads, ensure_cached);
}

/// Resolves ordered geometry, including concatenated passes, into disjoint upload ranges.
///
/// Transient vertex offsets and source indices refer to the complete input stream.
pub fn resolve_textured_mesh_geometries<'a, I, EnsureCached>(
    geometries: I,
    uploads: &mut TexturedMeshUploads,
    mut ensure_cached: EnsureCached,
) where
    I: IntoIterator<Item = &'a TexturedMeshGeometry>,
    I::IntoIter: Clone,
    EnsureCached: FnMut(TMeshCacheKey, &[TexturedMeshVertex]) -> Option<u64>,
{
    let geometries = geometries.into_iter();
    let geometry_count = geometries.clone().count();
    if uploads.all_cached
        && uploads.sources.len() == geometry_count
        && uploads.cache_keys.len() == geometry_count
        && geometries
            .clone()
            .zip(&uploads.cache_keys)
            .zip(&uploads.sources)
            .all(|((geometry, cache_key), source)| {
                geometry.cache_key != INVALID_TMESH_CACHE_KEY
                    && geometry.cache_key == *cache_key
                    && source.buffer_key().is_some()
                    && source.vertex_count() == saturating_u32(geometry.vertices.len())
            })
    {
        uploads.vertices.clear();
        return;
    }

    uploads.vertices.clear();
    uploads
        .cache_keys
        .resize(geometry_count, INVALID_TMESH_CACHE_KEY);
    uploads
        .sources
        .resize(geometry_count, TexturedMeshSource::transient(0, 0));
    uploads.all_cached = true;
    for ((geometry, cache_key), source) in geometries
        .zip(&mut uploads.cache_keys)
        .zip(&mut uploads.sources)
    {
        let vertices = geometry.vertices.as_ref();
        let vertex_count = saturating_u32(vertices.len());
        if geometry.cache_key != INVALID_TMESH_CACHE_KEY
            && geometry.cache_key == *cache_key
            && source.buffer_key().is_some()
            && source.vertex_count() == vertex_count
        {
            continue;
        }
        *source = if geometry.cache_key != INVALID_TMESH_CACHE_KEY
            && let Some(buffer_key) = ensure_cached(geometry.cache_key, vertices)
        {
            TexturedMeshSource::cached(buffer_key, vertex_count)
        } else {
            let vertex_start = saturating_u32(uploads.vertices.len());
            uploads.vertices.extend_from_slice(vertices);
            uploads.all_cached = false;
            TexturedMeshSource::transient(vertex_start, vertex_count)
        };
        *cache_key = geometry.cache_key;
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
