use super::*;
use crate::resource_perf_support as support;
use std::hint::black_box;
#[path = "model_merge_original.rs"]
mod original;

fn fixture(root: &Path, triangles: usize, merge: bool) -> (noteskin_itg::NoteskinData, PathBuf) {
    use std::fmt::Write;
    let mut source = String::from("// MilkShape 3D ASCII\nFrames: 1\nFrame: 1\nMeshes: 2\n");
    for mesh in 0..2 {
        let name = if merge || mesh == 0 {
            "joined"
        } else {
            "other"
        };
        writeln!(source, "\"{name}\" 0 -1\n3").unwrap();
        for vertex in 0..3 {
            writeln!(
                source,
                "{vertex} {} {} {} 0.5 -0.25 -1",
                mesh * 30 + vertex * 10,
                vertex * 20,
                vertex * 5
            )
            .unwrap();
        }
        writeln!(source, "1\n0 0 2\n{triangles}").unwrap();
        for _ in 0..triangles {
            source.push_str("0 0 1 2 0 0 0 1\n");
        }
    }
    source.push_str("Materials: 0\nBones: 0\n");
    let path = root.join("model.txt");
    fs::write(&path, source).unwrap();
    let data = noteskin_itg::NoteskinData {
        name: "fixture".into(),
        overrides: vec![],
        metrics: Default::default(),
        search_dirs: vec![root.to_owned()],
    };
    (data, path)
}

fn assert_layers(
    actual: &Option<Vec<ItgResolvedModelLayer>>,
    expected: &Option<Vec<ItgResolvedModelLayer>>,
) {
    // The full structural snapshot includes material, texture states, bounds,
    // flags, bone binding and both retained draws; vertices also compare bits.
    assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
    if let (Some(a), Some(b)) = (actual, expected) {
        for (a, b) in a.iter().zip(b) {
            assert_eq!(
                a.mesh.bounds.map(f32::to_bits),
                b.mesh.bounds.map(f32::to_bits)
            );
            for (a, b) in a.mesh.vertices.iter().zip(b.mesh.vertices.iter()) {
                assert_eq!(a.pos.map(f32::to_bits), b.pos.map(f32::to_bits));
                assert_eq!(a.normal.map(f32::to_bits), b.normal.map(f32::to_bits));
                assert_eq!(a.uv.map(f32::to_bits), b.uv.map(f32::to_bits));
                assert_eq!(
                    a.tex_matrix_scale.map(f32::to_bits),
                    b.tex_matrix_scale.map(f32::to_bits)
                );
            }
        }
    }
}

#[test]
fn merged_model_vertices_preserve_native_layers_and_invalid_inputs() {
    let dir = support::Directory::new("model");
    for count in [0, 1, 17, 1024] {
        for merge in [false, true] {
            let (data, path) = fixture(&dir.0, count, merge);
            assert_layers(
                &itg_parse_milkshape_model_layers(&data, &path, &path),
                &original::itg_parse_milkshape_model_layers(&data, &path, &path),
            );
        }
    }
    let (data, path) = fixture(&dir.0, 1, true);
    let source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/itgmania-song-lua-micro/model-merged-meshes/model-merged-meshes.txt"
    ));
    for source in [
        source.to_string(),
        source.replacen("joined mesh", "other", 1),
        source.replace("0 0 1 2 0 0 0 1", "0 0 1 999 0 0 0 1"),
        source
            .replace("Meshes: 2", "Meshes: 3")
            .replace("Materials: 1", "\"empty\" 0 -1\n0\n0\n0\nMaterials: 1"),
        source.replace("0 0 2", "NaN -inf 0"),
        "invalid".to_string(),
    ] {
        fs::write(&path, source).unwrap();
        assert_layers(
            &itg_parse_milkshape_model_layers(&data, &path, &path),
            &original::itg_parse_milkshape_model_layers(&data, &path, &path),
        );
    }
}

#[test]
fn merged_model_drops_one_full_vertex_buffer_allocation() {
    let dir = support::Directory::new("model-alloc");
    let (data, path) = fixture(&dir.0, 1024, true);
    let (expected, before) =
        support::measure(|| original::itg_parse_milkshape_model_layers(&data, &path, &path));
    let (actual, after) =
        support::measure(|| itg_parse_milkshape_model_layers(&data, &path, &path));
    assert_layers(&actual, &expected);
    let saved =
        actual.as_ref().unwrap()[0].mesh.vertices.len() * std::mem::size_of::<ModelVertex>();
    assert_eq!(before.allocs, after.allocs + 1, "{before:?} -> {after:?}");
    assert_eq!(
        before.allocated_bytes,
        after.allocated_bytes + saved,
        "{before:?} -> {after:?}"
    );
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_resource_traversal() {
    for (count, merge) in [
        (0, true),
        (1, true),
        (64, true),
        (1024, true),
        (8192, true),
        (1024, false),
    ] {
        let dir = support::Directory::new("model-bench");
        let (data, path) = fixture(&dir.0, count, merge);
        support::compare(
            &format!("model/{count}/merge={merge}"),
            || {
                black_box(original::itg_parse_milkshape_model_layers(
                    black_box(&data),
                    black_box(&path),
                    black_box(&path),
                ));
            },
            || {
                black_box(itg_parse_milkshape_model_layers(
                    black_box(&data),
                    black_box(&path),
                    black_box(&path),
                ));
            },
        );
    }
}
