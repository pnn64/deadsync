use deadsync_noteskin::actor as noteskin_actor;
use deadsync_noteskin::{Quantization, actor::*, compiled::*};
use std::hint::black_box;
use std::path::{Path, PathBuf};
#[path = "compiled_lookup/baseline.rs"]
mod baseline;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

fn file(key: String, id: usize) -> CompiledActorFile {
    CompiledActorFile {
        key,
        decl: ItgLuaActorDecl {
            sprites: vec![ItgLuaSpriteDecl {
                texture_expr: format!("texture-{id}"),
                frame0: id,
                ..Default::default()
            }],
            ..Default::default()
        },
    }
}

fn assert_lookup(
    actors: &CompiledActors,
    dirs: &[PathBuf],
    path: &Path,
    button: Option<&str>,
    color: Option<Quantization>,
) {
    let old = baseline::decl_for_path(actors, dirs, path, button, color);
    let new = actors.decl_for_path(dirs, path, button, color);
    assert_eq!(
        format!("{old:?}"),
        format!("{new:?}"),
        "{path:?} / {button:?} / {color:?}"
    );
}

#[test]
fn manifest_lookup_preserves_specificity_duplicates_and_path_rules() {
    for (root, filename) in [
        ("fixtures/dance/default", "Down Receptor.lua"),
        ("fixtures/日/éSkin", "éActor.lua"),
        ("fixtures/dance/a|color=8th", "x|Down.lua"),
    ] {
        let dirs = [PathBuf::from(root)];
        let path = dirs[0].join(filename);
        let key = actor_manifest_key(&dirs, &path).unwrap();
        for mask in 0..16 {
            let variants = [
                key.clone(),
                format!("{key}|Down"),
                format!("{key}|color=8th"),
                format!("{key}|Down|color=8th"),
            ];
            let mut files = vec![file("unrelated/key.lua".into(), 999)];
            for (i, key) in variants.iter().enumerate() {
                if mask & (1 << i) != 0 {
                    files.push(file(key.clone(), i));
                }
            }
            for i in 0..4 {
                files.rotate_left(1);
                let mut actors = CompiledActors {
                    files: files.clone(),
                    ..Default::default()
                };
                if let Some(first) = actors.files.first().cloned() {
                    actors
                        .files
                        .push(file(first.key.to_ascii_uppercase(), 100 + i));
                }
                for button in [
                    None,
                    Some("Down"),
                    Some("dOwN"),
                    Some("Missing"),
                    Some("Down|color=8th"),
                    Some(""),
                ] {
                    for color in [
                        None,
                        Some(Quantization::Q4th),
                        Some(Quantization::Q8th),
                        Some(Quantization::Q192nd),
                    ] {
                        assert_lookup(&actors, &dirs, &path, button, color);
                    }
                }
                let expected = actors.find(&key).map(|file| &file.decl);
                let actual = actors.decl_for_path_ref(&dirs, &path);
                assert_eq!(
                    expected.map(|p| p as *const _),
                    actual.map(|p| p as *const _)
                );
                perf::assert_no_churn(|| {
                    black_box(actors.decl_for_path_ref(&dirs, &path));
                });
                perf::assert_reduced_churn(
                    || {
                        black_box(baseline::decl_for_path(
                            &actors,
                            &dirs,
                            &path,
                            Some("Down"),
                            Some(Quantization::Q8th),
                        ));
                    },
                    || {
                        black_box(actors.decl_for_path(
                            &dirs,
                            &path,
                            Some("Down"),
                            Some(Quantization::Q8th),
                        ));
                    },
                );
                let missing = dirs[0].join("Missing.lua");
                perf::assert_no_churn(|| {
                    black_box(actors.decl_for_path(
                        &dirs,
                        &missing,
                        Some("Missing"),
                        Some(Quantization::Q12th),
                    ));
                });
            }
        }
    }
}

#[test]
fn manifest_lookup_keeps_first_matching_root_and_never_slices_utf8_text() {
    let dirs = [
        PathBuf::from("fixtures/dance/default"),
        PathBuf::from("fixtures/dance"),
    ];
    let path = dirs[0].join("nested/Actor.lua");
    let actors = CompiledActors {
        files: vec![
            file("dance/default/actor.lua".into(), 1),
            file("dance/default/acté.lua|Down".into(), 2),
            file("fixtures/dance/actor.lua".into(), 3),
        ],
        ..Default::default()
    };
    for path in [
        path,
        PathBuf::from("outside/Actor.lua"),
        dirs[0].join("Acté.lua"),
        PathBuf::from("/"),
    ] {
        assert_lookup(
            &actors,
            &dirs,
            &path,
            Some("Down"),
            Some(Quantization::Q8th),
        );
    }
    // The first matching root lacks a parent; the old API does not try later roots.
    assert_lookup(
        &actors,
        &[PathBuf::from(""), dirs[0].clone()],
        &dirs[0].join("Actor.lua"),
        None,
        None,
    );
}

#[test]
fn long_manifest_keys_preserve_results_and_spill_at_most_once() {
    let dirs = [PathBuf::from("fixtures/dance/default")];
    for capacity in [255, 256, 257, 512, 1024] {
        let path = dirs[0].join(format!("{}.lua", "a".repeat(capacity - 33)));
        let key = actor_manifest_key(&dirs, &path).unwrap();
        let actors = CompiledActors {
            files: vec![CompiledActorFile {
                key: format!("{key}|Down|color=8th"),
                decl: ItgLuaActorDecl::default(),
            }],
            ..Default::default()
        };
        assert_lookup(
            &actors,
            &dirs,
            &path,
            Some("Down"),
            Some(Quantization::Q8th),
        );
        perf::assert_churn_budget(usize::from(capacity > 256), capacity, || {
            assert!(
                actors
                    .decl_for_path(&dirs, &path, Some("Down"), Some(Quantization::Q8th))
                    .is_some()
            );
        });
    }
}

#[test]
#[ignore = "manual release benchmark"]
fn benchmark_compiled_lookup() {
    let dirs = [PathBuf::from("fixtures/dance/default")];
    let path = dirs[0].join("target.lua");
    let key = actor_manifest_key(&dirs, &path).unwrap();
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for count in [16, 256, 1024] {
        for (name, suffix) in [
            ("specific", "|Down|color=8th"),
            ("color", "|color=8th"),
            ("base", ""),
            ("miss", "|Other"),
        ] {
            let mut files: Vec<_> = (0..count)
                .map(|i| file(format!("dance/default/file-{i:04}.lua"), i))
                .collect();
            files.push(file(format!("{key}{suffix}"), 9999));
            let actors = CompiledActors {
                files,
                ..Default::default()
            };
            assert_lookup(
                &actors,
                &dirs,
                &path,
                Some("Down"),
                Some(Quantization::Q8th),
            );
            for old in if reverse {
                [false, true]
            } else {
                [true, false]
            } {
                perf::measure_sampled(
                    &format!("actor_{count}_{name}/{}", if old { "old" } else { "new" }),
                    4096,
                    1,
                    || {
                        if old {
                            baseline::decl_for_path(
                                black_box(&actors),
                                black_box(&dirs),
                                black_box(&path),
                                Some("Down"),
                                Some(Quantization::Q8th),
                            )
                        } else {
                            actors.decl_for_path(
                                black_box(&dirs),
                                black_box(&path),
                                Some("Down"),
                                Some(Quantization::Q8th),
                            )
                        }
                    },
                );
            }
        }
    }
}
