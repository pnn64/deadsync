use crate::act;
use deadlib_present::actors::{Actor, SizeSpec};
use deadlib_render::{BlendMode, MeshVertex};
use qrcodegen::{QrCode, QrCodeEcc};
use rustc_hash::FxHashMap;
use smallvec::{SmallVec, smallvec};
use std::sync::{Arc, LazyLock, Mutex};

const QR_BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const QR_WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const QR_CACHE_LIMIT: usize = 64;

#[derive(Clone, Debug)]
struct QrMeshData {
    module_px: f32,
    vertices: Arc<[MeshVertex]>,
}

type QrSizeVariants = SmallVec<[(u32, QrMeshData); 1]>;

struct QrMeshCache {
    entries: FxHashMap<String, QrSizeVariants>,
    len: usize,
}

impl QrMeshCache {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: FxHashMap::with_capacity_and_hasher(capacity, Default::default()),
            len: 0,
        }
    }

    #[inline(always)]
    fn get(&self, content: &str, size: f32) -> Option<&QrMeshData> {
        let size_bits = size.to_bits();
        self.entries
            .get(content)?
            .iter()
            .find_map(|(bits, data)| (*bits == size_bits).then_some(data))
    }

    fn insert(&mut self, content: &str, size: f32, data: QrMeshData) {
        let size_bits = size.to_bits();
        if let Some(variants) = self.entries.get_mut(content) {
            if let Some((_, existing)) = variants.iter_mut().find(|(bits, _)| *bits == size_bits) {
                *existing = data;
                return;
            }
            variants.push((size_bits, data));
        } else {
            self.entries
                .insert(content.to_owned(), smallvec![(size_bits, data)]);
        }
        self.len += 1;
    }

    #[cfg(test)]
    fn contains(&self, content: &str, size: f32) -> bool {
        self.get(content, size).is_some()
    }

    #[cfg(test)]
    fn clear(&mut self) {
        self.entries.clear();
        self.len = 0;
    }

    fn len(&self) -> usize {
        self.len
    }
}

#[derive(Clone, Copy, Debug)]
pub struct QrCodeParams<'a> {
    pub content: &'a str,
    pub center_x: f32,
    pub center_y: f32,
    pub size: f32,
    pub border_modules: u8,
    pub z: i16,
}

static QR_CACHE: LazyLock<Mutex<QrMeshCache>> =
    // Owner: shared UI actor builders behind a mutex.
    // Lifetime: process/session.
    // Capacity: 64 entries, saturating once full.
    // Warmup: first use.
    // Miss: rebuild QR geometry in memory; no I/O or GPU work here.
    // Eviction: none. Once full, misses bypass insertion.
    LazyLock::new(|| Mutex::new(QrMeshCache::with_capacity(QR_CACHE_LIMIT)));

#[inline(always)]
fn push_quad(out: &mut Vec<MeshVertex>, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
    let x1 = x + w;
    let y1 = y + h;
    out.push(MeshVertex { pos: [x, y], color });
    out.push(MeshVertex {
        pos: [x1, y],
        color,
    });
    out.push(MeshVertex {
        pos: [x1, y1],
        color,
    });
    out.push(MeshVertex { pos: [x, y], color });
    out.push(MeshVertex {
        pos: [x1, y1],
        color,
    });
    out.push(MeshVertex {
        pos: [x, y1],
        color,
    });
}

#[cfg(any(test, feature = "bench-support"))]
#[inline(always)]
fn legacy_cache_key(content: &str, size: f32) -> String {
    format!("{:08x}:{content}", size.to_bits())
}

fn build_qr_mesh(content: &str, size: f32) -> Option<QrMeshData> {
    if size <= 0.0 || content.trim().is_empty() {
        return None;
    }

    let qr = QrCode::encode_text(content, QrCodeEcc::High).ok()?;
    let modules_i32 = qr.size().max(1);
    let modules = modules_i32 as usize;
    let module_px = size / modules_i32 as f32;
    let max_runs_per_row = modules.div_ceil(2);
    let mut vertices =
        Vec::with_capacity(modules.saturating_mul(max_runs_per_row).saturating_mul(6));

    for y in 0..modules_i32 {
        let mut x = 0;
        while x < modules_i32 {
            if !qr.get_module(x, y) {
                x += 1;
                continue;
            }
            let run_start = x;
            x += 1;
            while x < modules_i32 && qr.get_module(x, y) {
                x += 1;
            }
            push_quad(
                &mut vertices,
                run_start as f32 * module_px,
                y as f32 * module_px,
                (x - run_start) as f32 * module_px,
                module_px,
                QR_BLACK,
            );
            // The run-ending module was already observed as white.
            if x < modules_i32 {
                x += 1;
            }
        }
    }

    Some(QrMeshData {
        module_px,
        vertices: Arc::from(vertices.into_boxed_slice()),
    })
}

#[cfg(any(test, feature = "bench-support"))]
fn build_qr_mesh_legacy(content: &str, size: f32) -> Option<QrMeshData> {
    if size <= 0.0 || content.trim().is_empty() {
        return None;
    }

    let qr = QrCode::encode_text(content, QrCodeEcc::High).ok()?;
    let modules_i32 = qr.size().max(1);
    let modules = modules_i32 as usize;
    let module_px = size / modules_i32 as f32;
    let mut vertices: Vec<MeshVertex> =
        Vec::with_capacity(modules.saturating_mul(modules).saturating_mul(6));

    for y in 0..modules_i32 {
        for x in 0..modules_i32 {
            if !qr.get_module(x, y) {
                continue;
            }
            let x0 = x as f32 * module_px;
            let y0 = y as f32 * module_px;
            push_quad(&mut vertices, x0, y0, module_px, module_px, QR_BLACK);
        }
    }

    Some(QrMeshData {
        module_px,
        vertices: Arc::from(vertices.into_boxed_slice()),
    })
}

fn mesh_for(content: &str, size: f32) -> Option<QrMeshData> {
    if let Ok(cache) = QR_CACHE.lock()
        && let Some(data) = cache.get(content, size)
    {
        return Some(data.clone());
    }

    let data = build_qr_mesh(content, size)?;
    if let Ok(mut cache) = QR_CACHE.lock()
        && cache.len() < QR_CACHE_LIMIT
    {
        cache.insert(content, size, data.clone());
    }
    Some(data)
}

#[cfg(any(test, feature = "bench-support"))]
#[inline(always)]
fn mesh_triangle_area2(data: &QrMeshData) -> u64 {
    data.vertices
        .chunks_exact(3)
        .map(|triangle| {
            let point = |vertex: &MeshVertex| {
                (
                    (vertex.pos[0] / data.module_px).round() as i64,
                    (vertex.pos[1] / data.module_px).round() as i64,
                )
            };
            let (x0, y0) = point(&triangle[0]);
            let (x1, y1) = point(&triangle[1]);
            let (x2, y2) = point(&triangle[2]);
            ((x0 * (y1 - y2) + x1 * (y2 - y0) + x2 * (y0 - y1)).unsigned_abs()) as u64
        })
        .sum()
}

#[cfg(any(test, feature = "bench-support"))]
#[inline(always)]
fn mesh_bench_checksum(data: &QrMeshData) -> u64 {
    mesh_triangle_area2(data) ^ u64::from(data.module_px.to_bits()).rotate_left(23)
}

#[cfg(any(test, feature = "bench-support"))]
#[inline(always)]
fn cache_hit_checksum(data: &QrMeshData) -> u64 {
    data.vertices.len() as u64 ^ u64::from(data.module_px.to_bits()).rotate_left(23)
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
pub fn qr_mesh_build_for_bench(content: &str, size: f32) -> u64 {
    build_qr_mesh(content, size)
        .as_ref()
        .map_or(0, mesh_bench_checksum)
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
pub fn qr_mesh_build_legacy_for_bench(content: &str, size: f32) -> u64 {
    build_qr_mesh_legacy(content, size)
        .as_ref()
        .map_or(0, mesh_bench_checksum)
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
pub struct QrCacheBenchFixture {
    contents: Vec<String>,
    size: f32,
    legacy: std::collections::HashMap<String, QrMeshData>,
    optimized: QrMeshCache,
}

#[cfg(any(test, feature = "bench-support"))]
impl QrCacheBenchFixture {
    pub fn new(contents: Vec<String>, size: f32) -> Self {
        let mut legacy = std::collections::HashMap::with_capacity(contents.len());
        let mut optimized = QrMeshCache::with_capacity(contents.len());
        for content in &contents {
            let data = build_qr_mesh(content, size).expect("benchmark QR content must be valid");
            legacy.insert(legacy_cache_key(content, size), data.clone());
            optimized.insert(content, size, data);
        }
        Self {
            contents,
            size,
            legacy,
            optimized,
        }
    }

    #[inline(always)]
    pub fn legacy_hit(&self, index: usize) -> u64 {
        if self.contents.is_empty() {
            return 0;
        }
        let content = &self.contents[index % self.contents.len()];
        self.legacy
            .get(&legacy_cache_key(content, self.size))
            .map_or(0, cache_hit_checksum)
    }

    #[inline(always)]
    pub fn optimized_hit(&self, index: usize) -> u64 {
        if self.contents.is_empty() {
            return 0;
        }
        let content = &self.contents[index % self.contents.len()];
        self.optimized
            .get(content, self.size)
            .map_or(0, cache_hit_checksum)
    }
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
pub struct QrRenderBenchFixture {
    legacy: Vec<QrMeshData>,
    optimized: Vec<QrMeshData>,
}

#[cfg(any(test, feature = "bench-support"))]
impl QrRenderBenchFixture {
    pub fn new(contents: &[String], size: f32) -> Self {
        Self {
            legacy: contents
                .iter()
                .filter_map(|content| build_qr_mesh_legacy(content, size))
                .collect(),
            optimized: contents
                .iter()
                .filter_map(|content| build_qr_mesh(content, size))
                .collect(),
        }
    }

    #[inline(always)]
    pub fn legacy_traversal(&self) -> u64 {
        self.legacy.iter().fold(0, |checksum, data| {
            checksum.rotate_left(7) ^ mesh_triangle_area2(data)
        })
    }

    #[inline(always)]
    pub fn optimized_traversal(&self) -> u64 {
        self.optimized.iter().fold(0, |checksum, data| {
            checksum.rotate_left(7) ^ mesh_triangle_area2(data)
        })
    }

    pub fn legacy_vertices(&self) -> usize {
        self.legacy.iter().map(|data| data.vertices.len()).sum()
    }

    pub fn optimized_vertices(&self) -> usize {
        self.optimized.iter().map(|data| data.vertices.len()).sum()
    }
}

pub fn build(params: QrCodeParams<'_>) -> Vec<Actor> {
    let Some(data) = mesh_for(params.content, params.size) else {
        return vec![];
    };

    let border_px = data.module_px * params.border_modules as f32;
    let outer_size = params.size + border_px * 2.0;

    vec![Actor::Frame {
        align: [0.5, 0.5],
        offset: [params.center_x, params.center_y],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        background: None,
        z: params.z,
        children: vec![
            act!(quad:
                align(0.5, 0.5):
                xy(0.0, 0.0):
                setsize(outer_size, outer_size):
                z(0):
                diffuse(QR_WHITE[0], QR_WHITE[1], QR_WHITE[2], QR_WHITE[3])
            ),
            Actor::Mesh {
                align: [0.5, 0.5],
                offset: [0.0, 0.0],
                size: [SizeSpec::Px(params.size), SizeSpec::Px(params.size)],
                tint: [1.0; 4],
                vertices: data.vertices,
                visible: true,
                blend: BlendMode::Alpha,
                z: 1,
            },
        ],
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn clear_qr_cache() {
        QR_CACHE.lock().unwrap().clear();
    }

    fn module_coverage(data: &QrMeshData, size: f32) -> Vec<bool> {
        let modules = (size / data.module_px).round() as usize;
        let mut covered = vec![false; modules * modules];
        for quad in data.vertices.chunks_exact(6) {
            let x0 = (quad[0].pos[0] / data.module_px).round() as usize;
            let y = (quad[0].pos[1] / data.module_px).round() as usize;
            let x1 = (quad[1].pos[0] / data.module_px).round() as usize;
            for x in x0..x1 {
                covered[y * modules + x] = true;
            }
        }
        covered
    }

    #[test]
    fn merged_runs_preserve_legacy_module_coverage() {
        for (content, size) in [
            ("https://example.com/score/1", 96.0),
            ("DEADSYNC-QR-PARITY-1234567890", 173.0),
            ("a", 31.0),
        ] {
            let legacy = build_qr_mesh_legacy(content, size).expect("legacy mesh");
            let optimized = build_qr_mesh(content, size).expect("optimized mesh");

            assert_eq!(
                module_coverage(&legacy, size),
                module_coverage(&optimized, size)
            );
            assert_eq!(
                mesh_triangle_area2(&legacy),
                mesh_triangle_area2(&optimized)
            );
            assert!(optimized.vertices.len() <= legacy.vertices.len());
        }
    }

    #[test]
    fn optimized_qr_workloads_match_legacy_behavior() {
        let contents = (0..16)
            .map(|index| format!("https://example.com/scores/{index:04}"))
            .collect::<Vec<_>>();
        let cache = QrCacheBenchFixture::new(contents.clone(), 96.0);
        let render = QrRenderBenchFixture::new(&contents, 96.0);

        for index in 0..contents.len() * 2 {
            assert_eq!(cache.legacy_hit(index), cache.optimized_hit(index));
        }
        for content in &contents {
            assert_eq!(
                qr_mesh_build_legacy_for_bench(content, 96.0),
                qr_mesh_build_for_bench(content, 96.0)
            );
        }
        assert_eq!(render.legacy_traversal(), render.optimized_traversal());
        assert!(render.optimized_vertices() < render.legacy_vertices());
    }

    #[test]
    fn mesh_for_reuses_cached_vertices() {
        let _guard = TEST_LOCK.lock().unwrap();
        clear_qr_cache();

        let first = mesh_for("https://example.com/score/1", 96.0).expect("qr should build");
        let second = mesh_for("https://example.com/score/1", 96.0).expect("qr should reuse");

        assert!(Arc::ptr_eq(&first.vertices, &second.vertices));
        assert_eq!(QR_CACHE.lock().unwrap().len(), 1);
    }

    #[test]
    fn mesh_for_saturates_after_cache_limit() {
        let _guard = TEST_LOCK.lock().unwrap();
        clear_qr_cache();

        for i in 0..QR_CACHE_LIMIT {
            let content = format!("https://example.com/score/{i}");
            let _ = mesh_for(&content, 96.0).expect("qr should build");
        }

        let overflow = "https://example.com/score/overflow";
        let first = mesh_for(overflow, 96.0).expect("overflow qr should build");
        let second = mesh_for(overflow, 96.0).expect("overflow qr should rebuild");

        assert_eq!(QR_CACHE.lock().unwrap().len(), QR_CACHE_LIMIT);
        assert!(!QR_CACHE.lock().unwrap().contains(overflow, 96.0));
        assert!(!Arc::ptr_eq(&first.vertices, &second.vertices));
    }
}
