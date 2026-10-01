use super::*;
use crate::perf;
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/model_preparation/baseline.rs"
    ));
}
mod before_material_cache {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/model_preparation/before_material_cache.rs"
    ));
}

struct Fixture {
    root: PathBuf,
    meshes: PathBuf,
    materials: PathBuf,
    data: noteskin_itg::NoteskinData,
}

impl Fixture {
    fn new(mesh_count: usize, material_index: i32, materials: &[(&str, &str)]) -> Self {
        let root = temp_model_root("preparation");
        fs::create_dir(root.join("frames")).unwrap();
        for name in ["a.png", "b.png", "override.png"] {
            fs::write(root.join("frames").join(name), []).unwrap();
        }
        fs::write(root.join("tex.png"), []).unwrap();
        fs::write(root.join("materials tex.png"), []).unwrap();
        fs::write(root.join("animated.ini"), "[AnimatedTexture]\nTexVelocityX=2\nTexOffsetY=-0.25\nFrame0000=frames\\a.png\nDelay0000=0.25\nFrame0001=frames\\b.png\nDelay0001=0.5\n").unwrap();
        fs::write(root.join("shine sphere.ini"), "[AnimatedTexture]\nFrame0000=frames/a.png\nDelay0000=0.5\nFrame0001=frames/b.png\nDelay0001=0.5\n").unwrap();
        let meshes = root.join("model.txt");
        let materials_path = root.join("materials.txt");
        let mut source = format!("// MilkShape 3D ASCII\nMeshes: {mesh_count}\n");
        for mesh in 0..mesh_count {
            let count = [3, 6, 9, 3][mesh % 4];
            source.push_str(&format!("\"mesh{mesh}\" 0 {material_index}\n{count}\n"));
            for index in 0..count {
                let bone = if mesh % 3 == 0 { 0 } else { -1 };
                source.push_str(&format!(
                    "{} {} {} {} 0.5 -0.25 {bone}\n",
                    index % 8,
                    index - index / 2,
                    mesh,
                    index % 3
                ));
            }
            source.push_str("4\n0 0 3\n2 0 0\n0 0 0\nNaN inf 0\n3\n0 0 1 2 0 0 0 1\n0 0 1 2 1 1 1 1\n0 999 1 2 3 3 3 1\n");
        }
        let mut material_source = format!("Materials: {}\n", materials.len());
        for (index, (texture, additive)) in materials.iter().enumerate() {
            material_source.push_str(&format!("\"material {index} {}\"\n0 0 0 1\n1 1 1 1\n0 0 0 1\n0 0 0 1\n0\n1\n{texture}\n{additive}\n", if index % 2 == 0 { "NoMove" } else { "sphere" }));
        }
        fs::write(&meshes, format!("{source}{material_source}")).unwrap();
        fs::write(&materials_path, material_source).unwrap();
        let data = noteskin_itg::NoteskinData {
            name: "preparation".into(),
            metrics: noteskin_itg::IniData::default(),
            search_dirs: vec![root.clone()],
            overrides: Vec::new(),
        };
        Self {
            root,
            meshes,
            materials: materials_path,
            data,
        }
    }

    fn check(&self, separate: bool) {
        let materials = if separate {
            &self.materials
        } else {
            &self.meshes
        };
        let old = baseline::itg_parse_milkshape_model_layers(&self.data, &self.meshes, materials);
        let new = itg_parse_milkshape_model_layers(&self.data, &self.meshes, materials);
        assert_layers(&old, &new);
        assert_layers(
            &new,
            &before_material_cache::itg_parse_milkshape_model_layers(
                &self.data,
                &self.meshes,
                materials,
            ),
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        assert_eq!(self.root.parent(), Some(std::env::temp_dir().as_path()));
        assert!(
            self.root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("deadsync-noteskin-model-preparation-")
        );
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn assert_texture(a: &ItgResolvedModelTexture, b: &ItgResolvedModelTexture) {
    assert_eq!(a.sphere_mapped, b.sphere_mapped);
    assert_eq!(a.texture_path, b.texture_path);
    assert_eq!(
        a.tex.uv_velocity.map(f32::to_bits),
        b.tex.uv_velocity.map(f32::to_bits)
    );
    assert_eq!(
        a.tex.uv_offset.map(f32::to_bits),
        b.tex.uv_offset.map(f32::to_bits)
    );
    assert_eq!(
        a.tex.uv_cycle_seconds.map(f32::to_bits),
        b.tex.uv_cycle_seconds.map(f32::to_bits)
    );
    assert_eq!(a.animation.is_some(), b.animation.is_some());
    if let (Some(a), Some(b)) = (&a.animation, &b.animation) {
        assert_eq!(a.path, b.path);
        assert_eq!(a.frames.len(), b.frames.len());
        for (a, b) in a.frames.iter().zip(&b.frames) {
            assert_eq!(a.path, b.path);
            assert_eq!(a.delay.to_bits(), b.delay.to_bits());
        }
    }
}

fn assert_layers(a: &Option<Vec<ItgResolvedModelLayer>>, b: &Option<Vec<ItgResolvedModelLayer>>) {
    assert_eq!(a.is_some(), b.is_some());
    let (Some(a), Some(b)) = (a, b) else {
        return;
    };
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(b) {
        assert_eq!(a.bone_index, b.bone_index);
        assert_eq!(a.flags.nomove, b.flags.nomove);
        assert_eq!(a.animation_length.to_bits(), b.animation_length.to_bits());
        assert_eq!(
            a.mesh.bounds.map(f32::to_bits),
            b.mesh.bounds.map(f32::to_bits)
        );
        assert_eq!(a.mesh.vertices.len(), b.mesh.vertices.len());
        for (a, b) in a.mesh.vertices.iter().zip(b.mesh.vertices.iter()) {
            assert_eq!(a.normal.map(f32::to_bits), b.normal.map(f32::to_bits));
            assert_eq!(a.pos.map(f32::to_bits), b.pos.map(f32::to_bits));
            assert_eq!(a.uv.map(f32::to_bits), b.uv.map(f32::to_bits));
            assert_eq!(
                a.tex_matrix_scale.map(f32::to_bits),
                b.tex_matrix_scale.map(f32::to_bits)
            );
        }
        assert_texture(&a.texture, &b.texture);
        assert_eq!(a.additive.is_some(), b.additive.is_some());
        if let (Some(a), Some(b)) = (&a.additive, &b.additive) {
            assert_texture(a, b);
        }
    }
}

#[test]
fn model_materials_meshes_and_separate_files_match_parent() {
    let material_sets: &[&[(&str, &str)]] = &[
        &[],
        &[("\"\"", "\"\"")],
        &[("\"frames/a.png\"", "\"\"")],
        &[("\"animated.ini\"", "\"shine sphere.ini\"")],
        &[
            ("\"missing.png\"", "\"missing.ini\""),
            ("\"animated.ini\"", "\"\""),
        ],
        &[("''", "\"frames/a.png\"")],
        &[("\"frames\\a.png\"", "\"frames/b.png\"")],
    ];
    for mesh_count in [0, 1, 2, 8, 32] {
        for materials in material_sets {
            for material_index in [-2, -1, 0, 1, 99] {
                let mut fixture = Fixture::new(mesh_count, material_index, materials);
                for separate in [false, true] {
                    fixture.check(separate);
                }
                fixture.data.overrides.push((
                    fixture.root.join("frames").join("a.png"),
                    fixture.root.join("frames").join("override.png"),
                ));
                fixture.check(true);
            }
        }
    }
}

#[test]
fn mesh_scratch_preserves_last_normal_wins_and_invalid_triangle_rules() {
    let fixture = Fixture::new(8, -1, &[]);
    fixture.check(false);
    let layers =
        itg_parse_milkshape_model_layers(&fixture.data, &fixture.meshes, &fixture.meshes).unwrap();
    assert_eq!(layers.len(), 8);
    for layer in layers {
        assert_eq!(layer.mesh.vertices.len(), 6);
        assert!(
            layer
                .mesh
                .vertices
                .iter()
                .all(|v| v.normal == [1.0, 0.0, 0.0])
        );
    }
    let original = fs::read_to_string(&fixture.meshes).unwrap();
    for (from, to) in [
        ("0 999 1 2 3 3 3 1", "0 0 1 2 999 999 999 1"),
        ("4\n0 0 3\n2 0 0\n0 0 0\nNaN inf 0\n", "0\n"),
        ("0 0 1 2 1 1 1 1", "0 0 0 0 0 1 2 1"),
        ("0.5 -0.25", "NaN -inf"),
        ("Meshes: 8", "Meshes: invalid"),
        ("3\n0 0 1 2", "3\n0 bad 1 2"),
    ] {
        fs::write(&fixture.meshes, original.replace(from, to)).unwrap();
        fixture.check(false);
    }
    let lines: Vec<_> = original.split_inclusive('\n').collect();
    for end in (0..lines.len()).step_by(11) {
        fs::write(&fixture.meshes, lines[..end].concat()).unwrap();
        fixture.check(false);
    }
}

#[test]
fn material_cache_refreshes_between_loads_and_preserves_unused_animation_length() {
    let fixture = Fixture::new(
        8,
        0,
        &[
            ("\"frames/a.png\"", "\"animated.ini\""),
            ("\"animated.ini\"", "\"missing.ini\""),
        ],
    );
    fixture.check(true);
    let first =
        itg_parse_milkshape_model_layers(&fixture.data, &fixture.meshes, &fixture.materials)
            .unwrap();
    assert_eq!(first[0].animation_length, 1.0);
    fs::write(fixture.root.join("animated.ini"), "[AnimatedTexture]\nFrame0000=frames/a.png\nDelay0000=2\nFrame0001=frames/b.png\nDelay0001=3\n").unwrap();
    fixture.check(true);
    let second =
        itg_parse_milkshape_model_layers(&fixture.data, &fixture.meshes, &fixture.materials)
            .unwrap();
    assert_eq!(second[0].animation_length, 5.0);
    assert_eq!(
        second[0].additive.as_ref().unwrap().tex.uv_cycle_seconds,
        Some(5.0)
    );
    assert_eq!(
        first[0].additive.as_ref().unwrap().tex.uv_cycle_seconds,
        Some(0.75)
    );
    fs::write(
        fixture.root.join("animated.ini"),
        "[AnimatedTexture]\nFrame0000=missing.png\nDelay0000=-1\n",
    )
    .unwrap();
    fixture.check(true);
}

#[test]
fn animated_first_frame_reuse_preserves_keys_delays_and_path_resolution() {
    let mut fixture = Fixture::new(1, -1, &[]);
    let path = fixture.root.join("probe.ini");
    let cases = [
        "Frame0000=frames\\a.png\nDelay0000=0.25\n",
        "Frame0001=frames/a.png\nDelay0001=0.5\n",
        "Frame0000=frames/a.png\nDelay0000=0.25\nFrame0001=frames/a.png\nDelay0001=0.5\n",
        "Frame0000=frames/a.png\nDelay0000=0\nFrame0001=frames/b.png\nDelay0001=0.5\n",
        "Frame0000=frames/a.png\nDelay0000=0.25\nFrame0001=frames/b.png\nDelay0001=0.5\n",
        "Frame0000=frames/a.png\nDelay0000=-1\n",
        "Frame0000=frames/a.png\nDelay0000=NaN\n",
        "Frame0000=missing.png\nDelay0000=1\n",
        "Frame0000=\nFrame0001=frames/a.png\nDelay0001=1\n",
        "Frame0000=frames/a.png\n",
        "Frame0000=frames/a.png\nDelay0000=0.25\nFrame0002=frames/b.png\nDelay0002=0.5\n",
    ];
    for with_override in [false, true] {
        if with_override {
            fixture.data.overrides.push((
                fixture.root.join("frames/a.png"),
                fixture.root.join("frames/override.png"),
            ));
        }
        for frames in cases {
            fs::write(
                &path,
                format!("[AnimatedTexture]\nTexVelocityX=2\nTexOffsetY=-0.5\n{frames}"),
            )
            .unwrap();
            let old = baseline::itg_resolve_animated_texture_ini(&fixture.data, &path);
            let new = itg_resolve_animated_texture_ini(&fixture.data, &path);
            assert_eq!(old.is_some(), new.is_some());
            if let (Some(old), Some(new)) = (old, new) {
                assert_texture(&old, &new);
                // A missing delay never enters the frame loop; every accepted
                // first delay replaces one resolution with an owning path copy.
                if frames.contains("Delay0000=") || frames.contains("Delay0001=") {
                    perf::assert_reduced_churn(
                        || {
                            black_box(baseline::itg_resolve_animated_texture_ini(
                                &fixture.data,
                                &path,
                            ));
                        },
                        || {
                            black_box(itg_resolve_animated_texture_ini(&fixture.data, &path));
                        },
                    );
                }
            }
        }
    }
}

#[test]
fn mesh_and_material_preparation_reduce_allocation_churn() {
    let fixture = Fixture::new(32, -1, &[]);
    fixture.check(false);
    perf::assert_reduced_allocation_calls(
        || {
            black_box(baseline::itg_parse_milkshape_model_layers(
                &fixture.data,
                &fixture.meshes,
                &fixture.meshes,
            ));
        },
        || {
            black_box(itg_parse_milkshape_model_layers(
                &fixture.data,
                &fixture.meshes,
                &fixture.meshes,
            ));
        },
    );
    let fixture = Fixture::new(16, 0, &[("\"animated.ini\"", "\"shine sphere.ini\"")]);
    fixture.check(true);
    perf::assert_reduced_churn(
        || {
            black_box(before_material_cache::itg_parse_milkshape_model_layers(
                &fixture.data,
                &fixture.meshes,
                &fixture.materials,
            ));
        },
        || {
            black_box(itg_parse_milkshape_model_layers(
                &fixture.data,
                &fixture.meshes,
                &fixture.materials,
            ));
        },
    );
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
fn benchmark_model_preparation() {
    for (name, frames) in [
        (
            "single",
            "Frame0000=frames\\a.png\nDelay0000=0.25\n".to_string(),
        ),
        (
            "repeated",
            (0..8)
                .map(|i| format!("Frame{i:04}=frames/a.png\nDelay{i:04}=0.25\n"))
                .collect::<String>(),
        ),
        (
            "mixed",
            "Frame0000=frames/a.png\nDelay0000=0.25\nFrame0001=frames/b.png\nDelay0001=0.5\n"
                .into(),
        ),
        (
            "one_based",
            "Frame0001=frames/a.png\nDelay0001=0.5\n".into(),
        ),
        ("invalid", "Frame0000=frames/a.png\nDelay0000=-1\n".into()),
    ] {
        let fixture = Fixture::new(1, -1, &[]);
        let path = fixture.root.join("probe.ini");
        fs::write(&path, format!("[AnimatedTexture]\n{frames}")).unwrap();
        let old = baseline::itg_resolve_animated_texture_ini(&fixture.data, &path);
        let new = itg_resolve_animated_texture_ini(&fixture.data, &path);
        assert_eq!(old.is_some(), new.is_some());
        if let (Some(old), Some(new)) = (old, new) {
            assert_texture(&old, &new);
        }
        pairs(|label, new| {
            perf::measure_sampled(&format!("prep_ini_{name}_{label}"), 128, 1, || {
                if new {
                    black_box(itg_resolve_animated_texture_ini(
                        black_box(&fixture.data),
                        black_box(&path),
                    ));
                } else {
                    black_box(baseline::itg_resolve_animated_texture_ini(
                        black_box(&fixture.data),
                        black_box(&path),
                    ));
                }
            });
        });
    }
    for count in [1, 8, 64] {
        let fixture = Fixture::new(count, -1, &[]);
        fixture.check(false);
        pairs(|label, new| {
            perf::measure_sampled(&format!("prep_scratch_{count}_{label}"), 256, count, || {
                if new {
                    black_box(itg_parse_milkshape_model_layers(
                        black_box(&fixture.data),
                        black_box(&fixture.meshes),
                        black_box(&fixture.meshes),
                    ));
                } else {
                    black_box(baseline::itg_parse_milkshape_model_layers(
                        black_box(&fixture.data),
                        black_box(&fixture.meshes),
                        black_box(&fixture.meshes),
                    ));
                }
            });
        });
    }
    for (name, count, texture, additive) in [
        ("image", 16, "\"frames/a.png\"", "\"frames/b.png\""),
        ("ini", 16, "\"animated.ini\"", "\"shine sphere.ini\""),
        ("single", 1, "\"animated.ini\"", "\"\""),
        ("missing", 16, "\"missing.png\"", "\"missing.ini\""),
    ] {
        let fixture = Fixture::new(count, 0, &[(texture, additive)]);
        fixture.check(true);
        pairs(|label, new| {
            perf::measure_sampled(&format!("prep_material_{name}_{label}"), 128, count, || {
                if new {
                    black_box(itg_parse_milkshape_model_layers(
                        black_box(&fixture.data),
                        black_box(&fixture.meshes),
                        black_box(&fixture.materials),
                    ));
                } else {
                    black_box(before_material_cache::itg_parse_milkshape_model_layers(
                        black_box(&fixture.data),
                        black_box(&fixture.meshes),
                        black_box(&fixture.materials),
                    ));
                }
            });
        });
    }
    for (name, count) in [("single", 1), ("shared", 16)] {
        let fixture = Fixture::new(count, 0, &[("\"animated.ini\"", "\"shine sphere.ini\"")]);
        fixture.check(false);
        pairs(|label, new| {
            perf::measure_sampled(&format!("prep_full_{name}_{label}"), 128, count, || {
                if new {
                    black_box(itg_parse_milkshape_model_layers(
                        black_box(&fixture.data),
                        black_box(&fixture.meshes),
                        black_box(&fixture.meshes),
                    ));
                } else {
                    black_box(baseline::itg_parse_milkshape_model_layers(
                        black_box(&fixture.data),
                        black_box(&fixture.meshes),
                        black_box(&fixture.meshes),
                    ));
                }
            });
        });
    }
}
