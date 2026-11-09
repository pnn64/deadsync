use deadlib_present::line::build_graph_line_mesh;
use deadlib_render_core::MeshVertex;
use std::{hint::black_box, sync::Arc};

#[path = "../src/line_graph_original.rs"]
mod original;
#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;

type BuildGraph = fn(&[[f32; 2]], f32, [f32; 4]) -> Arc<[MeshVertex]>;

#[test]
fn graph_mesh_keeps_one_exact_allocation() {
    for len in [0, 1, 2, 4, 32, 100, 256, 2048, 16384] {
        let points = (0..len)
            .map(|i| [i as f32, (i % 13) as f32 * 0.125])
            .collect::<Vec<_>>();
        for thickness in [2.0, 0.0, f32::NAN] {
            let run = |build: BuildGraph| {
                perf::measure(|| {
                    let mesh = black_box(build)(
                        black_box(&points),
                        black_box(thickness),
                        black_box([1.0; 4]),
                    );
                    let vertices = mesh.len();
                    black_box(&mesh);
                    drop(mesh);
                    vertices
                })
            };
            let expected = run(original::build_graph_line_mesh);
            let actual = run(build_graph_line_mesh);
            assert_eq!(actual, expected, "points={len}, thickness={thickness:?}");
            assert_eq!(actual.1.allocs, 1);
            assert_eq!(actual.1.reallocs, 0);
            assert_eq!(actual.1.frees, 1);
            assert_eq!(actual.1.allocated_bytes, actual.1.freed_bytes);
            println!(
                "points={len}, thickness={thickness:?}: {} vertices, {:?}",
                actual.0, actual.1
            );
        }
    }
}
