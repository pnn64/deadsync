use crate::act;
use deadlib_present::actors::{Actor, SizeSpec};
use deadlib_render_core::{BlendMode, MeshVertex};
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
