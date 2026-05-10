use crate::act;
use crate::engine::gfx::{BlendMode, MeshVertex};
use crate::engine::present::actors::{Actor, SizeSpec};
use qrcodegen::{QrCode, QrCodeEcc};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

const QR_BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const QR_WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const QR_CACHE_LIMIT: usize = 64;

#[derive(Clone, Debug)]
struct QrMeshData {
    module_px: f32,
    vertices: Arc<[MeshVertex]>,
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

static QR_CACHE: LazyLock<Mutex<HashMap<String, QrMeshData>>> =
    // Owner: shared UI actor builders behind a mutex.
    // Lifetime: process/session.
    // Capacity: 64 entries, saturating once full.
    // Warmup: first use.
    // Miss: rebuild QR geometry in memory; no I/O or GPU work here.
    // Eviction: none. Once full, misses bypass insertion.
    LazyLock::new(|| Mutex::new(HashMap::with_capacity(QR_CACHE_LIMIT)));

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

#[inline(always)]
fn cache_key(content: &str, size: f32) -> String {
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
    let mut vertices: Vec<MeshVertex> = Vec::with_capacity(modules.saturating_mul(modules) * 6);

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
    let key = cache_key(content, size);
    if let Ok(cache) = QR_CACHE.lock()
        && let Some(data) = cache.get(&key)
    {
        return Some(data.clone());
    }

    let data = build_qr_mesh(content, size)?;
    if let Ok(mut cache) = QR_CACHE.lock()
        && cache.len() < QR_CACHE_LIMIT
    {
        cache.insert(key, data.clone());
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
        let overflow_key = cache_key(overflow, 96.0);
        let first = mesh_for(overflow, 96.0).expect("overflow qr should build");
        let second = mesh_for(overflow, 96.0).expect("overflow qr should rebuild");

        assert_eq!(QR_CACHE.lock().unwrap().len(), QR_CACHE_LIMIT);
        assert!(!QR_CACHE.lock().unwrap().contains_key(&overflow_key));
        assert!(!Arc::ptr_eq(&first.vertices, &second.vertices));
    }
}
