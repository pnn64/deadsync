//! Geometry resolution across independent pass-local geometry tables.
use deadlib_render_core::{
    TexturedMeshGeometry, TexturedMeshUploads, TexturedMeshVertex, TexturedMeshVertices,
    resolve_textured_mesh_geometries,
};
use std::sync::Arc;

fn geometry(x: f32, count: usize, cache_key: u64) -> TexturedMeshGeometry {
    TexturedMeshGeometry {
        vertices: TexturedMeshVertices::Shared(Arc::from(vec![
            TexturedMeshVertex {
                pos: [x, 0.0, 0.0],
                ..TexturedMeshVertex::default()
            };
            count
        ])),
        cache_key,
    }
}

#[test]
fn mixed_passes_keep_transient_ranges_and_cached_sources() {
    let mut passes = [
        vec![geometry(1.0, 3, 0), geometry(2.0, 6, 17)],
        Vec::new(),
        vec![geometry(3.0, 9, 23), geometry(4.0, 6, 0)],
    ];
    let mut uploads = TexturedMeshUploads::default();
    for _ in 0..3 {
        resolve_textured_mesh_geometries(passes.iter().flatten(), &mut uploads, |key, _| {
            (key == 17).then_some(99)
        });
        assert_eq!(uploads.sources.len(), 4);
        assert_eq!(uploads.sources[1].buffer_key(), Some(99));
        for (source_index, value) in [(0, 1.0), (2, 3.0), (3, 4.0)] {
            let source = uploads.sources[source_index];
            assert_eq!(source.buffer_key(), None);
            let start = source.vertex_start() as usize;
            let end = start + source.vertex_count() as usize;
            assert!(
                uploads.vertices[start..end]
                    .iter()
                    .all(|v| v.pos[0] == value)
            );
        }
        assert_eq!(uploads.vertices.len(), 18);
        // Rejected cache admission is retried without disturbing prior ranges.
    }
    passes[0].clear();
    resolve_textured_mesh_geometries(passes.iter().flatten(), &mut uploads, |_, _| None);
    assert_eq!(uploads.sources.len(), 2);
    assert_eq!(uploads.sources[0].vertex_start(), 0);
    assert_eq!(uploads.sources[1].vertex_start(), 9);
    assert_eq!(uploads.vertices[0].pos[0], 3.0);
    assert_eq!(uploads.vertices[9].pos[0], 4.0);
}

#[test]
fn repeated_cached_passes_reuse_resolution_until_order_changes() {
    let mut passes = [vec![geometry(1.0, 3, 17)], vec![geometry(2.0, 6, 23)]];
    let mut uploads = TexturedMeshUploads::default();
    resolve_textured_mesh_geometries(passes.iter().flatten(), &mut uploads, |key, _| Some(key));
    resolve_textured_mesh_geometries(passes.iter().flatten(), &mut uploads, |_, _| {
        panic!("warm cache lookup")
    });
    passes.swap(0, 1);
    resolve_textured_mesh_geometries(passes.iter().flatten(), &mut uploads, |key, _| Some(key));
    assert_eq!(uploads.sources[0].buffer_key(), Some(23));
    assert_eq!(uploads.sources[0].vertex_count(), 6);
    assert_eq!(uploads.sources[1].buffer_key(), Some(17));
    assert!(uploads.vertices.is_empty());
}
