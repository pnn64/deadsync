use super::*;
use crate::perf;
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/animated_materials/baseline.rs"
    ));
}
mod after_frame_reuse {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/animated_materials/after_frame_reuse.rs"
    ));
}
mod after_lazy_frames {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/animated_materials/after_lazy_frames.rs"
    ));
}

struct Fixture {
    root: PathBuf,
    local: PathBuf,
    ini: PathBuf,
    model: PathBuf,
    data: noteskin_itg::NoteskinData,
}

impl Fixture {
    fn new() -> Self {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        // Fixed-width IDs keep path allocation sizes stable across benchmark processes.
        let root = std::env::temp_dir().join(format!(
            "deadsync-noteskin-animated-materials-{:010}-{suffix:020}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        let local = root.join("local");
        let first = root.join("first");
        let last = root.join("last");
        for dir in [&local, &first, &last] {
            fs::create_dir(dir).unwrap();
            fs::create_dir(dir.join("frames")).unwrap();
            for name in ["a.png", "b.png", "c.png", "spére.png", "override.png"] {
                fs::write(dir.join("frames").join(name), []).unwrap();
            }
            for index in 0..16 {
                fs::write(dir.join(format!("unused{index:02}.png")), []).unwrap();
            }
            fs::write(dir.join("shared.png"), []).unwrap();
        }
        fs::write(first.join("first-only.png"), []).unwrap();
        fs::write(last.join("last-only.png"), []).unwrap();
        let ini = local.join("sphere.ini");
        let model = local.join("model.txt");
        let data = noteskin_itg::NoteskinData {
            name: "animated-materials".into(),
            metrics: noteskin_itg::IniData::default(),
            search_dirs: vec![
                local.clone(),
                first.clone(),
                local.clone(),
                first.clone(),
                last.clone(),
                last,
            ],
            overrides: Vec::new(),
        };
        Self {
            root,
            local,
            ini,
            model,
            data,
        }
    }

    fn write_ini(&self, frames: &str) {
        fs::write(&self.ini, format!("[AnimatedTexture]\nTexVelocityX=0.125\nTexVelocityY=-0\nTexOffsetX=NaN\nTexOffsetY=-0.75\n{frames}")).unwrap();
    }

    fn write_model(&self, meshes: usize) {
        let mut source = format!("// MilkShape 3D ASCII\nMeshes: {meshes}\n");
        for index in 0..meshes {
            source.push_str(&format!("\"mesh{index}\" 0 0\n3\n0 0 0 0 0 0 0\n0 1 0 0 1 0 0\n0 0 1 0 0 1 0\n1\n0 0 1\n1\n0 0 1 2 0 0 0 1\n"));
        }
        source.push_str("Materials: 1\n\"scroll NoMove\"\n0 0 0 1\n1 1 1 1\n0 0 0 1\n0 0 0 1\n0\n1\n\"sphere.ini\"\n\"\"\n");
        fs::write(&self.model, source).unwrap();
    }

    fn check(&self) {
        let old = baseline::itg_resolve_animated_texture_ini(&self.data, &self.ini);
        for new in [
            after_frame_reuse::itg_resolve_animated_texture_ini(&self.data, &self.ini),
            after_lazy_frames::itg_resolve_animated_texture_ini(&self.data, &self.ini),
            itg_resolve_animated_texture_ini(&self.data, &self.ini),
        ] {
            assert_texture(&old, &new);
        }
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
                .starts_with("deadsync-noteskin-animated-materials-")
        );
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn assert_texture(a: &Option<ItgResolvedModelTexture>, b: &Option<ItgResolvedModelTexture>) {
    assert_eq!(a.is_some(), b.is_some());
    let (Some(a), Some(b)) = (a, b) else { return };
    assert_eq!(a.sphere_mapped, b.sphere_mapped);
    assert_eq!(a.texture_path.as_os_str(), b.texture_path.as_os_str());
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
        assert_eq!(a.path.as_os_str(), b.path.as_os_str());
        assert_eq!(a.frames.len(), b.frames.len());
        for (a, b) in a.frames.iter().zip(&b.frames) {
            assert_eq!(a.path.as_os_str(), b.path.as_os_str());
            assert_eq!(a.delay.to_bits(), b.delay.to_bits());
        }
    }
}

fn frames(count: usize, first: usize, distinct_at: Option<usize>) -> String {
    (first..first + count)
        .map(|index| {
            let name = if distinct_at == Some(index) { "b" } else { "a" };
            let delay = ["0", "0.1", "0.25", "-0", "0.33333334"][index % 5];
            format!("Frame{index:04}=frames/{name}.png\nDelay{index:04}={delay}\n")
        })
        .collect()
}

#[test]
fn repeated_frames_and_late_animation_preserve_paths_order_and_float_bits() {
    let mut fixture = Fixture::new();
    for overridden in [false, true] {
        if overridden {
            fixture.data.overrides.push((
                fixture.local.join("frames/a.png"),
                fixture.local.join("frames/override.png"),
            ));
        }
        for first in [0, 1] {
            for count in [1, 2, 4, 8, 31, 32, 33, 129, 999] {
                for distinct in [None, Some(first + count / 2), Some(first + count - 1)] {
                    fixture.write_ini(&frames(count, first, distinct));
                    fixture.check();
                }
            }
        }
        fixture.write_ini(&frames(1001, 0, Some(1000)));
        fixture.check(); // Frame1000 stays outside the historical 1000-frame limit.
        let result = itg_resolve_animated_texture_ini(&fixture.data, &fixture.ini).unwrap();
        assert!(result.animation.is_none());
    }
    fixture.write_ini(&frames(16, 0, Some(15)));
    let result = itg_resolve_animated_texture_ini(&fixture.data, &fixture.ini).unwrap();
    let animation = result.animation.unwrap();
    assert_eq!(animation.frames.len(), 16);
    assert!(
        animation.frames[..15]
            .iter()
            .all(|frame| frame.path == result.texture_path)
    );
    assert_eq!(
        animation.frames[15].path,
        fixture.local.join("frames/b.png")
    );
}

#[test]
fn animation_termination_invalid_delays_and_aliases_match_parent() {
    let mut fixture = Fixture::new();
    for body in [
        "",
        "Frame0000=\nFrame0001=frames/a.png\nDelay0001=1\n",
        "Frame0000=frames/a.png\n",
        "Frame0000=missing.png\nDelay0000=1\n",
        "Frame0000=frames/a.png\nDelay0000=0.25\nFrame0001=missing.png\n",
        "Frame0000=frames/a.png\nDelay0000=0.25\nFrame0002=frames/b.png\nDelay0002=1\n",
        "Frame0000=frames/a.png\nDelay0000=0.25\nFrame0001=frames/b.png\nDelay0001=invalid\n",
        "Frame0001=frames/a.png\nDelay0001=0.5\nFrame0000=missing.png\nDelay0000=1\n",
        "Frame0000='frames/a.png'\nDelay0000=0.25\nFrame0001=frames\\a.png\nDelay0001=0.5\nFrame0002=./frames/a.png\nDelay0002=1\n",
        "Frame0000=frames/a.png\nDelay0000=0.25\nFrame0001=frames/spére.png\nDelay0001=0.5\n",
        "Frame0000=frames/a.png\nDelay0000=0.25\nFrame0001=FRAMES/A.PNG\nDelay0001=0.5\n",
        "Frame0000=frames/a.png\nDelay0000=1.1920929e-7\n",
        "Frame0000=frames/a.png\nDelay0000=0.25\nFrame0001=./frames/a.png\nDelay0001=0.5\nFrame0002=frames/a.png\nDelay0002=0.75\nFrame0003=frames/b.png\nDelay0003=1\n",
        "Frame0000=frames/a.png\nDelay0000=0.25\nFrame0001=frames/b.png\nDelay0001=0.5\nFrame0002=frames/a.png\nDelay0002=0.75\nFrame0003=frames/c.png\nDelay0003=1\n",
    ] {
        fixture.write_ini(body);
        fixture.check();
    }
    for delay in ["-1", "NaN", "inf", "-inf", "0", "-0", "1e-30", "3.4e38"] {
        fixture.write_ini(&format!(
            "Frame0000=frames/a.png\nDelay0000={delay}\nFrame0001=frames/b.png\nDelay0001={delay}\n"
        ));
        fixture.check();
        fixture.write_ini(&format!(
            "{}Frame0015=frames/a.png\nDelay0015={delay}\n",
            frames(15, 0, None)
        ));
        fixture.check();
    }
    // Different references overridden to one path still suppress the atlas.
    fixture.data.overrides.push((
        fixture.local.join("frames/b.png"),
        fixture.local.join("frames/a.png"),
    ));
    fixture.write_ini(&frames(8, 0, Some(7)));
    fixture.check();
    assert!(
        itg_resolve_animated_texture_ini(&fixture.data, &fixture.ini)
            .unwrap()
            .animation
            .is_none()
    );
}

#[test]
fn path_search_deduplication_preserves_priority_fallback_and_refreshes() {
    let mut fixture = Fixture::new();
    let absolute = fixture.root.join("last/last-only.png");
    for raw in [
        "shared.png",
        "first-only.png",
        "last-only.png",
        "missing.png",
        "frames\\a.png",
        "FRAMES/A.PNG",
        "./frames/a.png",
        "../last/last-only.png",
        "unused01",
        "'frames/spére.png'",
        "  \"shared.png\"  ",
        "",
        absolute.to_str().unwrap(),
    ] {
        for parent in [
            &fixture.ini,
            &fixture.root.join("absent/model.txt"),
            Path::new("model.txt"),
        ] {
            let old = baseline::itg_resolve_relative_or_noteskin_path(&fixture.data, parent, raw);
            let new = itg_resolve_relative_or_noteskin_path(&fixture.data, parent, raw);
            assert_eq!(old, new, "{parent:?}, {raw:?}");
        }
    }
    assert_eq!(
        itg_resolve_relative_or_noteskin_path(&fixture.data, &fixture.ini, "shared.png"),
        Some(fixture.local.join("shared.png"))
    );
    fixture.data.overrides.push((
        fixture.root.join("last/last-only.png"),
        fixture.local.join("frames/override.png"),
    ));
    assert_eq!(
        baseline::itg_resolve_relative_or_noteskin_path(
            &fixture.data,
            &fixture.ini,
            "last-only.png"
        ),
        itg_resolve_relative_or_noteskin_path(&fixture.data, &fixture.ini, "last-only.png")
    );
    fixture.data.search_dirs.reverse();
    for raw in ["shared.png", "last-only.png", "missing.png", "unused01"] {
        assert_eq!(
            baseline::itg_resolve_relative_or_noteskin_path(&fixture.data, &fixture.ini, raw),
            itg_resolve_relative_or_noteskin_path(&fixture.data, &fixture.ini, raw)
        );
    }
    fixture.data.search_dirs.clear();
    assert_eq!(
        baseline::itg_resolve_relative_or_noteskin_path(&fixture.data, &fixture.ini, "missing.png"),
        itg_resolve_relative_or_noteskin_path(&fixture.data, &fixture.ini, "missing.png")
    );
    fs::write(fixture.local.join("missing.png"), []).unwrap();
    assert_eq!(
        itg_resolve_relative_or_noteskin_path(&fixture.data, &fixture.ini, "missing.png"),
        Some(fixture.local.join("missing.png"))
    );
}

#[test]
fn frame_reuse_is_scoped_to_one_load_and_models_match_parent() {
    let fixture = Fixture::new();
    fixture.write_ini(&frames(16, 0, None));
    let saved = itg_resolve_animated_texture_ini(&fixture.data, &fixture.ini).unwrap();
    for meshes in [1, 16] {
        fixture.write_model(meshes);
        let old = baseline::itg_parse_milkshape_model_layers(
            &fixture.data,
            &fixture.model,
            &fixture.model,
        )
        .unwrap();
        let new = itg_parse_milkshape_model_layers(&fixture.data, &fixture.model, &fixture.model)
            .unwrap();
        assert_eq!(old.len(), new.len());
        for (old, new) in old.into_iter().zip(new) {
            assert_eq!(old.bone_index, new.bone_index);
            assert_eq!(old.flags.nomove, new.flags.nomove);
            assert_eq!(
                old.animation_length.to_bits(),
                new.animation_length.to_bits()
            );
            assert_eq!(
                old.mesh.bounds.map(f32::to_bits),
                new.mesh.bounds.map(f32::to_bits)
            );
            assert_eq!(old.mesh.vertices.len(), new.mesh.vertices.len());
            for (old, new) in old.mesh.vertices.iter().zip(new.mesh.vertices.iter()) {
                assert_eq!(old.normal.map(f32::to_bits), new.normal.map(f32::to_bits));
                assert_eq!(old.pos.map(f32::to_bits), new.pos.map(f32::to_bits));
                assert_eq!(old.uv.map(f32::to_bits), new.uv.map(f32::to_bits));
                assert_eq!(
                    old.tex_matrix_scale.map(f32::to_bits),
                    new.tex_matrix_scale.map(f32::to_bits)
                );
            }
            assert_texture(&Some(old.texture), &Some(new.texture));
            assert_texture(&old.additive, &new.additive);
        }
    }
    fs::remove_file(fixture.local.join("frames/a.png")).unwrap();
    fixture.check();
    assert_eq!(
        itg_resolve_animated_texture_ini(&fixture.data, &fixture.ini)
            .unwrap()
            .texture_path,
        fixture.root.join("first/frames/a.png")
    );
    assert!(saved.animation.is_none());
    assert_eq!(saved.texture_path, fixture.local.join("frames/a.png"));
    fs::write(fixture.local.join("frames/a.png"), []).unwrap();
    fixture.write_ini(&frames(16, 0, Some(15)));
    fixture.check();
    assert!(
        itg_resolve_animated_texture_ini(&fixture.data, &fixture.ini)
            .unwrap()
            .animation
            .is_some()
    );
}

#[test]
fn all_three_optimizations_reduce_owning_allocation_churn() {
    let fixture = Fixture::new();
    fixture.write_ini(&frames(32, 0, None));
    fixture.check();
    perf::assert_reduced_churn(
        || {
            black_box(baseline::itg_resolve_animated_texture_ini(
                &fixture.data,
                &fixture.ini,
            ));
        },
        || {
            black_box(after_frame_reuse::itg_resolve_animated_texture_ini(
                &fixture.data,
                &fixture.ini,
            ));
        },
    );
    perf::assert_reduced_churn(
        || {
            black_box(after_frame_reuse::itg_resolve_animated_texture_ini(
                &fixture.data,
                &fixture.ini,
            ));
        },
        || {
            black_box(after_lazy_frames::itg_resolve_animated_texture_ini(
                &fixture.data,
                &fixture.ini,
            ));
        },
    );
    perf::assert_reduced_churn(
        || {
            black_box(baseline::itg_resolve_relative_or_noteskin_path(
                &fixture.data,
                &fixture.ini,
                "missing.png",
            ));
        },
        || {
            black_box(itg_resolve_relative_or_noteskin_path(
                &fixture.data,
                &fixture.ini,
                "missing.png",
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
fn benchmark_animated_materials() {
    for (name, body) in [
        ("single", frames(1, 0, None)),
        ("repeat32", frames(32, 0, None)),
        ("late32", frames(32, 0, Some(31))),
        ("repeat129", frames(129, 0, None)),
        ("late129", frames(129, 0, Some(128))),
        ("mixed", frames(2, 0, Some(1))),
        ("alias_late", "Frame0000=frames/a.png\nDelay0000=0.25\nFrame0001=./frames/a.png\nDelay0001=0.5\nFrame0002=frames/a.png\nDelay0002=0.75\nFrame0003=frames/b.png\nDelay0003=1\n".into()),
    ] {
        let fixture = Fixture::new();
        fixture.write_ini(&body);
        fixture.check();
        for stage in ["reuse", "lazy", "full"] {
            pairs(|label, new| {
                perf::measure_sampled(&format!("anim_{stage}_{name}_{label}"), 128, 1, || {
                    let data = black_box(&fixture.data);
                    let path = black_box(&fixture.ini);
                    black_box(match (stage, new) {
                        ("reuse", false) | ("full", false) => {
                            baseline::itg_resolve_animated_texture_ini(data, path)
                        }
                        ("reuse", true) | ("lazy", false) => {
                            after_frame_reuse::itg_resolve_animated_texture_ini(data, path)
                        }
                        ("lazy", true) => {
                            after_lazy_frames::itg_resolve_animated_texture_ini(data, path)
                        }
                        ("full", true) => itg_resolve_animated_texture_ini(data, path),
                        _ => unreachable!(),
                    });
                });
            });
        }
    }
    for (name, raw, duplicate_dirs) in [
        ("local", "frames/a.png", true),
        ("fallback", "last-only.png", true),
        ("missing", "missing.png", true),
        ("unique_missing", "missing.png", false),
    ] {
        let mut fixture = Fixture::new();
        if !duplicate_dirs {
            fixture.data.search_dirs = vec![fixture.root.join("first"), fixture.root.join("last")];
        }
        assert_eq!(
            baseline::itg_resolve_relative_or_noteskin_path(&fixture.data, &fixture.ini, raw),
            itg_resolve_relative_or_noteskin_path(&fixture.data, &fixture.ini, raw)
        );
        pairs(|label, new| {
            perf::measure_sampled(&format!("anim_search_{name}_{label}"), 128, 1, || {
                let data = black_box(&fixture.data);
                let path = black_box(&fixture.ini);
                let raw = black_box(raw);
                black_box(if new {
                    itg_resolve_relative_or_noteskin_path(data, path, raw)
                } else {
                    baseline::itg_resolve_relative_or_noteskin_path(data, path, raw)
                });
            });
        });
    }
    for (name, count, distinct) in [
        ("single", 1, None),
        ("shared", 16, None),
        ("late", 16, Some(31)),
    ] {
        let fixture = Fixture::new();
        fixture.write_ini(&frames(32, 0, distinct));
        fixture.write_model(count);
        assert_eq!(
            baseline::itg_parse_milkshape_model_layers(
                &fixture.data,
                &fixture.model,
                &fixture.model
            )
            .unwrap()
            .len(),
            itg_parse_milkshape_model_layers(&fixture.data, &fixture.model, &fixture.model)
                .unwrap()
                .len()
        );
        pairs(|label, new| {
            perf::measure_sampled(&format!("anim_model_{name}_{label}"), 128, count, || {
                let data = black_box(&fixture.data);
                let path = black_box(&fixture.model);
                black_box(if new {
                    itg_parse_milkshape_model_layers(data, path, path)
                } else {
                    baseline::itg_parse_milkshape_model_layers(data, path, path)
                });
            });
        });
    }
}
