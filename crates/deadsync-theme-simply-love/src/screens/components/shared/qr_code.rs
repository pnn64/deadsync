use crate::act;
use deadlib_present::actors::{Actor, SizeSpec};
use deadlib_render_core::{BlendMode, MeshVertex};
use qrcodegen::{QrCode, QrCodeEcc};
use rustc_hash::FxHashMap;
use smallvec::{SmallVec, smallvec};
use std::cell::RefCell;
use std::sync::Arc;

const QR_BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const QR_WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const QR_CACHE_LIMIT: usize = 64;

#[derive(Clone, Debug)]
struct QrMeshData {
    module_px: f32,
    vertices: Arc<[MeshVertex]>,
}

/// Screen-owned immutable QR geometry prepared outside recurring actor builds.
#[derive(Clone, Debug)]
pub(crate) struct PreparedQrCode {
    size: f32,
    data: QrMeshData,
}

impl PreparedQrCode {
    pub(crate) fn push(
        &self,
        out: &mut Vec<Actor>,
        center_x: f32,
        center_y: f32,
        border_modules: u8,
        z: i16,
    ) {
        push_mesh(
            out,
            self.data.clone(),
            self.size,
            center_x,
            center_y,
            border_modules,
            z,
        );
    }
}

type QrSizeVariants = SmallVec<[(u32, QrMeshData); 1]>;

struct QrMeshCache {
    entries: FxHashMap<String, QrSizeVariants>,
    len: usize,
}

impl QrMeshCache {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: FxHashMap::with_capacity_and_hasher(
                capacity,
                rustc_hash::FxBuildHasher::default(),
            ),
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

    const fn len(&self) -> usize {
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

thread_local! {
    // Owner/thread model: game-thread UI actor builders only.
    // Lifetime: game thread. Capacity: 64 entries, saturating once full.
    // Warmup: first QR actor build. A hit clones one `Arc` with no locks or
    // allocations. A miss builds geometry in memory; there is no I/O or GPU
    // work. There is no eviction: full-cache misses bypass insertion, and all
    // retained geometry is destroyed when the game thread exits. Existing
    // actor-build timing accounts for the bounded lookup and miss work.
    static QR_CACHE: RefCell<QrMeshCache> =
        RefCell::new(QrMeshCache::with_capacity(QR_CACHE_LIMIT));
}

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

pub(crate) fn prepare(content: &str, size: f32) -> Option<PreparedQrCode> {
    build_qr_mesh(content, size).map(|data| PreparedQrCode { size, data })
}

fn mesh_for(content: &str, size: f32) -> Option<QrMeshData> {
    if let Some(data) = QR_CACHE.with(|cache| cache.borrow().get(content, size).cloned()) {
        return Some(data);
    }

    let data = build_qr_mesh(content, size)?;
    QR_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() < QR_CACHE_LIMIT {
            cache.insert(content, size, data.clone());
        }
    });
    Some(data)
}

pub fn push(out: &mut Vec<Actor>, params: QrCodeParams<'_>) -> bool {
    let Some(data) = mesh_for(params.content, params.size) else {
        return false;
    };

    push_mesh(
        out,
        data,
        params.size,
        params.center_x,
        params.center_y,
        params.border_modules,
        params.z,
    );
    true
}

fn push_mesh(
    out: &mut Vec<Actor>,
    data: QrMeshData,
    size: f32,
    center_x: f32,
    center_y: f32,
    border_modules: u8,
    z: i16,
) {
    let border_px = data.module_px * f32::from(border_modules);
    let outer_size = border_px.mul_add(2.0, size);

    out.push(Actor::Frame {
        align: [0.5, 0.5],
        offset: [center_x, center_y],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        background: None,
        z,
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
                size: [SizeSpec::Px(size), SizeSpec::Px(size)],
                tint: [1.0; 4],
                vertices: data.vertices,
                visible: true,
                blend: BlendMode::Alpha,
                z: 1,
            },
        ],
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clear_qr_cache() {
        QR_CACHE.with(|cache| cache.borrow_mut().clear());
    }

    fn qr_cache_len() -> usize {
        QR_CACHE.with(|cache| cache.borrow().len())
    }

    #[test]
    fn mesh_for_reuses_cached_vertices() {
        clear_qr_cache();

        let first = mesh_for("https://example.com/score/1", 96.0).expect("qr should build");
        let second = mesh_for("https://example.com/score/1", 96.0).expect("qr should reuse");

        assert!(Arc::ptr_eq(&first.vertices, &second.vertices));
        assert_eq!(qr_cache_len(), 1);
    }

    #[test]
    fn prepared_qr_shares_geometry_without_populating_cache() {
        clear_qr_cache();

        let prepared =
            prepare("https://example.com/score/prepared", 96.0).expect("prepared QR should build");
        let clone = prepared.clone();

        assert!(Arc::ptr_eq(&prepared.data.vertices, &clone.data.vertices));
        assert_eq!(qr_cache_len(), 0);
    }

    #[test]
    fn mesh_for_saturates_after_cache_limit() {
        clear_qr_cache();

        for i in 0..QR_CACHE_LIMIT {
            let content = format!("https://example.com/score/{i}");
            let _ = mesh_for(&content, 96.0).expect("qr should build");
        }

        let overflow = "https://example.com/score/overflow";
        let first = mesh_for(overflow, 96.0).expect("overflow qr should build");
        let second = mesh_for(overflow, 96.0).expect("overflow qr should rebuild");

        assert_eq!(qr_cache_len(), QR_CACHE_LIMIT);
        assert!(!QR_CACHE.with(|cache| cache.borrow().contains(overflow, 96.0)));
        assert!(!Arc::ptr_eq(&first.vertices, &second.vertices));
    }
}
