use super::*;
use crate::perf;
use std::fmt::Write;
use std::hint::black_box;

mod baseline;
#[allow(dead_code)]
#[path = "../../../deadsync-assets/tests/asset_discovery/support.rs"]
mod tree;

fn compare_vertices(a: &[ModelVertex], b: &[ModelVertex]) {
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(b) {
        assert_eq!(a.normal.map(f32::to_bits), b.normal.map(f32::to_bits));
        assert_eq!(a.pos.map(f32::to_bits), b.pos.map(f32::to_bits));
        assert_eq!(a.uv.map(f32::to_bits), b.uv.map(f32::to_bits));
        assert_eq!(
            a.tex_matrix_scale.map(f32::to_bits),
            b.tex_matrix_scale.map(f32::to_bits)
        );
    }
}

fn vertices(count: usize) -> Vec<ModelVertex> {
    (0..count)
        .map(|n| ModelVertex {
            normal: [n as f32 * 0.125, -0.0, -1.0],
            pos: [n as f32 - 5.5, -(n as f32), n as f32 * 0.25],
            uv: [n as f32 * 0.001, -0.25],
            tex_matrix_scale: [0.0, 2.0],
        })
        .collect()
}

fn triangles(count: usize) -> Vec<[usize; 3]> {
    (0..count)
        .map(|n| [n % 17, (n * 7 + 3) % 17, (n * 11 + 5) % 17])
        .collect()
}

#[test]
fn shared_mesh_expansion_preserves_vertex_order_bounds_bits_and_scratch() {
    let mut vertices = vertices(17);
    for special in [false, true] {
        if special {
            vertices[0].pos = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY];
            vertices[1].pos = [-0.0, 0.0, f32::from_bits(1)];
            vertices[2].normal = [f32::from_bits(0x7fc00042), -0.0, f32::MAX];
        }
        for count in [1, 2, 3, 16, 129, 4096] {
            for excess in [0, 7, 100] {
                let source = triangles(count);
                let mut old = Vec::with_capacity(count + excess);
                old.extend_from_slice(&source);
                let mut new = old.clone();
                let oldcap = old.capacity();
                let newcap = new.capacity();
                let oldptr = old.as_ptr();
                let newptr = new.as_ptr();
                let (a, ab) = baseline::expand_mesh_vertices(&vertices, &mut old, count + excess);
                let (b, bb) = expand_mesh_vertices(&vertices, &mut new);
                compare_vertices(&a, &b);
                assert_eq!(ab.map(f32::to_bits), bb.map(f32::to_bits));
                assert!(old.is_empty() && new.is_empty());
                assert_eq!((old.capacity(), new.capacity()), (oldcap, newcap));
                assert_eq!((old.as_ptr(), new.as_ptr()), (oldptr, newptr));
            }
        }
    }
}

#[test]
fn mesh_expansion_allocates_only_the_final_shared_vertex_buffer() {
    let vertices = vertices(17);
    for count in [1, 16, 129, 4096] {
        let mut input = triangles(count);
        let capacity = input.capacity();
        perf::assert_churn_budget(
            1,
            count * 3 * std::mem::size_of::<ModelVertex>() + 16,
            || {
                black_box(expand_mesh_vertices(&vertices, &mut input));
            },
        );
        assert!(input.is_empty());
        assert_eq!(input.capacity(), capacity);
        perf::assert_reduced_churn(
            || {
                let mut input = triangles(count);
                black_box(baseline::expand_mesh_vertices(&vertices, &mut input, count));
            },
            || {
                let mut input = triangles(count);
                black_box(expand_mesh_vertices(&vertices, &mut input));
            },
        );
    }
}

struct Fixture {
    tree: tree::Tree,
    model: PathBuf,
    materials: PathBuf,
    data: noteskin_itg::NoteskinData,
}

impl Fixture {
    fn new(mesh_count: usize, triangle_count: usize, invalid_every: usize) -> Self {
        let tree = tree::Tree::new();
        let model = tree.path.join("model.txt");
        let materials = tree.path.join("materials.txt");
        let mut text = format!("// MilkShape 3D ASCII\nMeshes: {mesh_count}\n");
        for mesh in 0..mesh_count {
            writeln!(
                text,
                "\"mesh{mesh}\" 0 {}\n17",
                if mesh % 3 == 0 { -1 } else { 0 }
            )
            .unwrap();
            for n in 0..17 {
                writeln!(
                    text,
                    "{} {} {} {} 0.5 -0.25 {}",
                    n % 8,
                    n as f32 - 5.5,
                    mesh,
                    n % 3,
                    if mesh % 2 == 0 {
                        0
                    } else if n == 0 {
                        1
                    } else {
                        2
                    }
                )
                .unwrap();
            }
            writeln!(text, "4\n0 0 3\n2 0 0\n0 0 0\nNaN inf 0\n{triangle_count}").unwrap();
            for n in 0..triangle_count {
                let indices = if invalid_every != 0 && n % invalid_every == 0 {
                    [999, 1, 2]
                } else {
                    [n % 17, (n * 7 + 3) % 17, (n * 11 + 5) % 17]
                };
                writeln!(
                    text,
                    "0 {} {} {} {} {} {} 1",
                    indices[0],
                    indices[1],
                    indices[2],
                    n % 5,
                    (n + 1) % 5,
                    (n + 2) % 5
                )
                .unwrap();
            }
        }
        let material = "Materials: 1\n\"NoMove sphere\"\n0 0 0 1\n1 1 1 1\n0 0 0 1\n0 0 0 1\n0\n1\n\"\"\n\"\"\n";
        fs::write(&model, format!("{text}{material}")).unwrap();
        fs::write(&materials, material).unwrap();
        let data = noteskin_itg::NoteskinData {
            name: "shared-mesh".into(),
            metrics: Default::default(),
            search_dirs: vec![tree.path.clone()],
            overrides: Vec::new(),
        };
        Self {
            tree,
            model,
            materials,
            data,
        }
    }
    fn check(&self, separate: bool) {
        let path = if separate {
            &self.materials
        } else {
            &self.model
        };
        compare_layers(
            &baseline::itg_parse_milkshape_model_layers(&self.data, &self.model, path),
            &itg_parse_milkshape_model_layers(&self.data, &self.model, path),
        );
    }
}

fn compare_layers(a: &Option<Vec<ItgResolvedModelLayer>>, b: &Option<Vec<ItgResolvedModelLayer>>) {
    assert_eq!(a.is_some(), b.is_some());
    if let (Some(a), Some(b)) = (a, b) {
        assert_eq!(a.len(), b.len());
        for (a, b) in a.iter().zip(b) {
            assert_eq!(a.bone_index, b.bone_index);
            assert_eq!(format!("{:?}", a.texture), format!("{:?}", b.texture));
            assert_eq!(format!("{:?}", a.additive), format!("{:?}", b.additive));
            assert_eq!(format!("{:?}", a.flags), format!("{:?}", b.flags));
            assert_eq!(a.animation_length.to_bits(), b.animation_length.to_bits());
            assert_eq!(
                a.mesh.bounds.map(f32::to_bits),
                b.mesh.bounds.map(f32::to_bits)
            );
            compare_vertices(&a.mesh.vertices, &b.mesh.vertices);
        }
    }
}

#[test]
fn parsed_models_keep_last_normal_wins_invalid_triangle_rules_and_materials() {
    for meshes in [0, 1, 4, 17] {
        for triangles in [0, 1, 3, 17, 129] {
            for invalid in [0, 1, 3] {
                let fixture = Fixture::new(meshes, triangles, invalid);
                fixture.check(false);
                fixture.check(true);
                if meshes > 0 && triangles > 0 && invalid != 1 {
                    perf::assert_reduced_churn(
                        || {
                            black_box(baseline::itg_parse_milkshape_model_layers(
                                &fixture.data,
                                &fixture.model,
                                &fixture.model,
                            ));
                        },
                        || {
                            black_box(itg_parse_milkshape_model_layers(
                                &fixture.data,
                                &fixture.model,
                                &fixture.model,
                            ));
                        },
                    );
                }
            }
        }
    }
}

#[test]
fn empty_invalid_truncated_and_missing_mesh_files_match_parent() {
    let fixture = Fixture::new(2, 7, 0);
    let original = fs::read_to_string(&fixture.model).unwrap();
    for text in [
        "",
        "not MilkShape",
        "// MilkShape 3D ASCII\nMeshes: 1\n\"bad\" 0 -1\n0\n0\n0\nMaterials: 0\n",
    ] {
        fs::write(&fixture.model, text).unwrap();
        fixture.check(false);
        fixture.check(true);
    }
    // Truncate at every line boundary through both mesh sections.
    let lines = original.lines().collect::<Vec<_>>();
    for end in 0..lines.len() {
        fs::write(&fixture.model, lines[..end].join("\n")).unwrap();
        fixture.check(false);
    }
    fs::write(&fixture.model, &original).unwrap();
    fs::remove_file(&fixture.materials).unwrap();
    fixture.check(true);
    fs::remove_file(&fixture.model).unwrap();
    fixture.check(false);
    assert!(fixture.tree.path.exists());
}

fn pairs(mut work: impl FnMut(&str, bool)) {
    if std::env::var("DEADSYNC_PERF_ORDER").as_deref() == Ok("new-first") {
        work("new", true);
        work("old", false);
    } else {
        work("old", false);
        work("new", true);
    }
}

#[test]
#[ignore = "manual paired release CPU, throughput and allocation benchmark"]
fn benchmark_shared_mesh_preparation() {
    let vertices = vertices(17);
    for (name, count, declared, iterations) in [
        ("triangle", 1, 1, 16384),
        ("mesh128", 128, 128, 2048),
        ("mesh4096", 4096, 4096, 128),
        ("mostly_invalid", 128, 4096, 2048),
    ] {
        let triangles = triangles(count);
        pairs(|label, new| {
            perf::measure_sampled(
                &format!("shared_expand_{name}_{label}"),
                iterations,
                1,
                || {
                    let mut input = black_box(&triangles).clone();
                    black_box(if new {
                        expand_mesh_vertices(black_box(&vertices), &mut input)
                    } else {
                        baseline::expand_mesh_vertices(
                            black_box(&vertices),
                            &mut input,
                            black_box(declared),
                        )
                    });
                },
            )
        });
    }
    for (name, meshes, triangles, invalid, iterations) in [
        ("small", 1, 3, 0, 128),
        ("many_meshes", 32, 64, 0, 16),
        ("dense", 1, 4096, 0, 16),
        ("invalid", 4, 129, 3, 32),
    ] {
        let fixture = Fixture::new(meshes, triangles, invalid);
        fixture.check(false);
        pairs(|label, new| {
            perf::measure_sampled(
                &format!("shared_parse_{name}_{label}"),
                iterations,
                1,
                || {
                    black_box(if new {
                        itg_parse_milkshape_model_layers(
                            black_box(&fixture.data),
                            black_box(&fixture.model),
                            black_box(&fixture.model),
                        )
                    } else {
                        baseline::itg_parse_milkshape_model_layers(
                            black_box(&fixture.data),
                            black_box(&fixture.model),
                            black_box(&fixture.model),
                        )
                    });
                },
            )
        });
    }
}
