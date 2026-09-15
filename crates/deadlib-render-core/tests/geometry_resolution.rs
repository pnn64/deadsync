//! Compare geometry resolution with its frozen preflight-based implementation.
use deadlib_render_core::{
    INVALID_TMESH_CACHE_KEY, RenderFrame, TMeshCacheKey, TexturedMeshGeometry, TexturedMeshSource,
    TexturedMeshUploads, TexturedMeshVertex, TexturedMeshVertices,
    resolve_textured_mesh_geometries,
};
use std::hint::black_box;

#[path = "geometry_resolution/baseline.rs"]
#[allow(dead_code)]
mod baseline;
#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;

fn geometry(key: u64, count: usize, value: f32) -> TexturedMeshGeometry {
    TexturedMeshGeometry {
        cache_key: key,
        vertices: TexturedMeshVertices::Transient(vec![
            TexturedMeshVertex {
                pos: [value, 0.0, 0.0],
                ..Default::default()
            };
            count
        ]),
    }
}

#[test]
fn resolution_matches_across_cache_and_geometry_transitions() {
    let mut old = baseline::TexturedMeshUploads::default();
    let mut new = TexturedMeshUploads::default();
    let mut seed = 0x569319bfc56u64;
    let mut random = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut geometries = Vec::new();
    for step in 0..8192 {
        // Stable frames alternate with changes to length, keys, and vertex bits.
        if step % 4 == 0 {
            geometries = (0..random() % 32)
                .map(|_| {
                    geometry(
                        random() % 24,
                        (random() % 13) as usize,
                        f32::from_bits(random() as u32),
                    )
                })
                .collect();
        } else if step % 4 == 2 && !geometries.is_empty() {
            geometries.last_mut().unwrap().cache_key = random() % 24;
        }
        let mut old_calls = Vec::new();
        let mut new_calls = Vec::new();
        let admit = |key: u64| (key % 5 != 0 || step % 7 == 0).then_some(key * 7 + 1);
        baseline::resolve_textured_mesh_geometries(&geometries, &mut old, |key, v| {
            old_calls.push((key, v.len()));
            admit(key)
        });
        resolve_textured_mesh_geometries(&geometries, &mut new, |key, v| {
            new_calls.push((key, v.len()));
            admit(key)
        });
        assert_eq!(old_calls, new_calls, "cache requests at step {step}");
        assert_eq!(old.sources, new.sources, "sources at step {step}");
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&old.vertices),
            bytemuck::cast_slice::<_, u8>(&new.vertices),
            "vertex bits at step {step}"
        );
        assert_eq!(old.vertices.capacity(), new.vertices.capacity());
        assert_eq!(old.sources.capacity(), new.sources.capacity());
    }
}

#[derive(Clone, Copy, Debug)]
enum Case {
    Stable,
    ChangeFirst,
    ChangeLast,
    Mixed,
    Misses,
}

fn admit(key: u64, _: &[TexturedMeshVertex]) -> Option<u64> {
    (key != 999).then_some(key * 7 + 1)
}

fn benchmark_variant<U>(
    name: &str,
    count: usize,
    case: Case,
    mut uploads: U,
    mut resolve: impl FnMut(&[TexturedMeshGeometry], &mut U),
) {
    let mut geometries: Vec<_> = (0..count)
        .map(|index| {
            let key = match case {
                Case::Mixed if index % 4 == 0 => 0,
                Case::Misses => 999,
                _ => index as u64 + 1,
            };
            geometry(key, 6, index as f32)
        })
        .collect();
    resolve(&geometries, &mut uploads);
    let changing = match case {
        Case::ChangeFirst => Some(0),
        Case::ChangeLast => count.checked_sub(1),
        _ => None,
    };
    perf::measure_sampled(name, 4096, count.max(1), || {
        if let Some(index) = changing {
            geometries[index].cache_key ^= 1 << 20;
        }
        resolve(black_box(&geometries), black_box(&mut uploads));
        black_box(&uploads);
    });
}

#[test]
#[ignore = "manual release benchmark for cycles, allocations, and throughput"]
fn geometry_resolution_bench() {
    #[cfg(windows)]
    if let Ok(mask) = std::env::var("DEADSYNC_PERF_AFFINITY") {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentThread() -> *mut std::ffi::c_void;
            fn SetThreadAffinityMask(thread: *mut std::ffi::c_void, mask: usize) -> usize;
        }
        let mask = mask.parse().expect("decimal affinity mask");
        // SAFETY: the pseudo-handle names this live benchmark thread. Windows
        // validates the supplied affinity mask; no memory pointers are retained.
        assert_ne!(
            unsafe { SetThreadAffinityMask(GetCurrentThread(), mask) },
            0
        );
    }
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for count in [0, 1, 16, 64, 256] {
        for case in [
            Case::Stable,
            Case::ChangeFirst,
            Case::ChangeLast,
            Case::Mixed,
            Case::Misses,
        ] {
            if count == 0 && !matches!(case, Case::Stable) {
                continue;
            }
            for old in if reverse {
                [false, true]
            } else {
                [true, false]
            } {
                let name = format!("{case:?}/{count}/{}", if old { "old" } else { "new" });
                if old {
                    benchmark_variant(
                        &name,
                        count,
                        case,
                        baseline::TexturedMeshUploads::with_capacity(count * 6, count),
                        |g, u| baseline::resolve_textured_mesh_geometries(g, u, admit),
                    );
                } else {
                    benchmark_variant(
                        &name,
                        count,
                        case,
                        TexturedMeshUploads::with_capacity(count * 6, count),
                        |g, u| resolve_textured_mesh_geometries(g, u, admit),
                    );
                }
            }
        }
    }
}
