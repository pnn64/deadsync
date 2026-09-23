//! Production noteskin lookups and frozen 0.5.1203 behavior.
#[path = "perf/load_preparation_baseline.rs"]
mod baseline;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

use deadsync_noteskin::{actor::*, compiled::*, itg, runtime::itg_first_actor_sprite_slot};
use std::cell::RefCell;
use std::collections::HashMap;
use std::hint::black_box;
use std::path::{Path, PathBuf};

fn loader_fixture() -> CompiledLoader {
    let mut entries = Vec::new();
    for button in ["Down", "Left", "Right", "Up"] {
        for element in [
            "Explosion",
            "Hold Head Active",
            "Receptor",
            "Roll Explosion",
            "Tap Note",
        ] {
            entries.push(CompiledLoaderEntry {
                button: button.into(),
                element: element.into(),
                load_button: "Down".into(),
                load_element: if element == "Hold Head Active" {
                    "tAp NoTe"
                } else {
                    element
                }
                .into(),
                blank: element == "Roll Explosion",
                rotation_x: Some(-90),
                rotation_y: None,
                rotation_z: Some(270),
                init_command: (element == "Receptor")
                    .then(|| "zoom,0.75;diffuse,1,0.5,0.25,1".into()),
            });
        }
    }
    CompiledLoader {
        entries,
        ..CompiledLoader::default()
    }
}

fn actor_fixture(count: usize) -> (CompiledActors, Vec<PathBuf>, PathBuf) {
    let dir = PathBuf::from("fixtures/dance/default");
    let path = dir.join("Actor-23.lua");
    let mut decl = ItgLuaActorDecl::default();
    for index in 0..count {
        let commands = HashMap::from([
            (
                "initcommand".into(),
                "zoom,0.75;diffuse,1,0.5,0.25,1".into(),
            ),
            (
                "w1command".into(),
                "stoptweening;diffusealpha,1;linear,0.2;diffusealpha,0".into(),
            ),
            (
                "holdingoncommand".into(),
                "effectclock,beat;glowshift".into(),
            ),
        ]);
        decl.sprites.push(ItgLuaSpriteDecl {
            texture_expr: format!("\"layer-{index:03}.png\""),
            frame0: index,
            frame_count: 16,
            frame_indices: Some((0..16).collect()),
            frame_delays: Some(vec![0.0625; 16]),
            commands: commands.clone(),
        });
        decl.models.push(ItgLuaModelDecl {
            meshes_expr: Some("\"mesh.txt\"".into()),
            materials_expr: Some("\"material.txt\"".into()),
            texture_expr: Some("\"model.png\"".into()),
            frame0: index,
            commands: commands.clone(),
        });
        decl.refs.push(ItgLuaRefDecl {
            button_override: Some("Down".into()),
            element: "Tap Note".into(),
            wrapper_expr: Some("wrapper".into()),
            frame_override: Some(index),
            condition_expr: Some("true".into()),
            commands: commands.clone(),
        });
        decl.path_refs.push(ItgLuaPathRefDecl {
            path_expr: "\"child.lua\"".into(),
            arg_expr: Some("\"child.png\"".into()),
            frame_override: Some(index),
            commands,
        });
    }
    let mut files = (0..23)
        .map(|index| CompiledActorFile {
            key: format!("dance/default/actor-{index:02}.lua"),
            decl: ItgLuaActorDecl::default(),
        })
        .collect::<Vec<_>>();
    files.push(CompiledActorFile {
        key: "dance/default/actor-23.lua".into(),
        decl,
    });
    (
        CompiledActors {
            version: CACHE_SCHEMA_VERSION,
            files,
        },
        vec![dir],
        path,
    )
}

#[test]
fn borrowed_requests_preserve_fields_fallbacks_and_owned_compatibility() {
    let loader = loader_fixture();
    for button in ["Down", "UP", "left", "missing", "\u{65e5}\u{672c}", ""] {
        for element in [
            "Explosion",
            "hold head active",
            "RECEPTOR",
            "Roll Explosion",
            "Tap Note",
            "missing",
            "",
        ] {
            let old = baseline::old_load_request(&loader, button, element);
            let new = loader.load_request_ref(button, element);
            assert_eq!(new.maps_head_to_tap(), old.maps_head_to_tap());
            assert_eq!(new.into_owned(), old);
            assert_eq!(loader.load_request(button, element), old);
            if let Some(entry) = loader.find(button, element) {
                assert_eq!(new.load_button.as_ptr(), entry.load_button.as_ptr());
                assert_eq!(new.load_element.as_ptr(), entry.load_element.as_ptr());
                assert_eq!(
                    new.init_command.map(str::as_ptr),
                    entry.init_command.as_deref().map(str::as_ptr)
                );
            } else {
                assert_eq!(new.load_button.as_ptr(), button.as_ptr());
                assert_eq!(new.load_element.as_ptr(), element.as_ptr());
            }
            perf::assert_no_churn(|| {
                black_box(loader.load_request_ref(button, element));
            });
        }
    }
}

#[test]
fn borrowed_requests_keep_the_first_duplicate_entry() {
    let mut loader = loader_fixture();
    let mut duplicate = loader.entries[0].clone();
    duplicate.blank = true;
    duplicate.init_command = Some("duplicate".into());
    loader.entries.insert(1, duplicate);
    assert_eq!(
        loader.load_request_ref("DOWN", "explosion").into_owned(),
        baseline::old_load_request(&loader, "DOWN", "explosion")
    );
    assert!(!loader.load_request_ref("DOWN", "explosion").blank);
}

#[test]
fn borrowed_actor_graph_preserves_nested_metadata_and_owned_independence() {
    for count in [0, 1, 8, 32] {
        let (actors, dirs, path) = actor_fixture(count);
        let old = baseline::old_decl_for_path(&actors, &dirs, &path).unwrap();
        let borrowed = actors.decl_for_path_ref(&dirs, &path).unwrap();
        assert!(std::ptr::eq(borrowed, &actors.files[23].decl));
        assert_eq!(
            serde_json::to_value(borrowed).unwrap(),
            serde_json::to_value(&old).unwrap()
        );
        let mut owned = actors.decl_for_path(&dirs, &path, None).unwrap();
        assert_eq!(
            serde_json::to_value(&owned).unwrap(),
            serde_json::to_value(&old).unwrap()
        );
        if let Some(sprite) = owned.sprites.first_mut() {
            sprite.texture_expr = "mutated".into();
            sprite.commands.clear();
            assert_ne!(sprite.texture_expr, borrowed.sprites[0].texture_expr);
            assert!(!borrowed.sprites[0].commands.is_empty());
        }
        // The manifest key is still owned; the actor graph is not copied.
        perf::assert_churn_budget(1, 64, || {
            black_box(actors.decl_for_path_ref(&dirs, &path));
        });
    }
}

#[test]
fn actor_path_resolution_preserves_missing_case_and_search_order() {
    let (mut actors, dirs, path) = actor_fixture(1);
    let mut duplicate = actors.files[23].clone();
    duplicate.key.make_ascii_uppercase();
    duplicate.decl.sprites[0].frame0 = 99;
    actors.files.push(duplicate);
    for path in [
        path.clone(),
        dirs[0].join("ACTOR-23.LUA"),
        dirs[0].join("missing.lua"),
        PathBuf::from("unrelated/actor-23.lua"),
    ] {
        let old = baseline::old_decl_for_path(&actors, &dirs, &path);
        let new = actors.decl_for_path_ref(&dirs, &path);
        assert_eq!(
            old.as_ref().map(|v| serde_json::to_value(v).unwrap()),
            new.map(|v| serde_json::to_value(v).unwrap())
        );
    }
    assert_eq!(
        actors.decl_for_path_ref(&dirs, &path).unwrap().sprites[0].frame0,
        0
    );
    let overlapping = [PathBuf::from("fixtures/dance"), dirs[0].clone()];
    assert!(baseline::old_decl_for_path(&actors, &overlapping, &path).is_none());
    assert!(actors.decl_for_path_ref(&overlapping, &path).is_none());
}

struct FixtureDir(PathBuf);
impl FixtureDir {
    fn new() -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "deadsync-preparation-{}-{unique}",
            std::process::id()
        ));
        // Create a new, uniquely owned directory; never clear an existing path.
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for FixtureDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[derive(Debug, PartialEq)]
enum LoadCall {
    Texture(String),
    Frame(String, usize),
    Animated(
        String,
        usize,
        usize,
        Option<Vec<usize>>,
        Option<Vec<f32>>,
        bool,
    ),
}

#[test]
fn first_sprite_keeps_loader_fallback_order_and_animation_metadata() {
    let root = FixtureDir::new();
    let dir = root.0.join("dance/default");
    std::fs::create_dir_all(&dir).unwrap();
    for name in ["first.png", "second.png", "third.png"] {
        std::fs::write(dir.join(name), []).unwrap();
    }
    let data = itg::NoteskinData {
        name: "default".into(),
        metrics: itg::IniData::default(),
        search_dirs: vec![dir.clone()],
        overrides: Vec::new(),
    };
    let (mut actors, _, _) = actor_fixture(3);
    let path = dir.join("Actor-23.lua");
    for (index, name) in ["first.png", "second.png", "third.png"].iter().enumerate() {
        let sprite = &mut actors.files[23].decl.sprites[index];
        sprite.texture_expr = format!("\"{name}\"");
        sprite.frame_count = if index == 0 { 16 } else { 1 };
    }
    let run = |old: bool| {
        let calls = RefCell::new(Vec::new());
        let file = |path: &Path| path.file_name().unwrap().to_str().unwrap().to_owned();
        let texture = |path: &Path| {
            calls.borrow_mut().push(LoadCall::Texture(file(path)));
            None
        };
        let frame = |path: &Path, index: usize| {
            calls.borrow_mut().push(LoadCall::Frame(file(path), index));
            (index == 1).then_some(index)
        };
        let animated = |path: &Path,
                        first: usize,
                        count: usize,
                        indices: Option<&[usize]>,
                        delays: Option<&[f32]>,
                        beat: bool| {
            calls.borrow_mut().push(LoadCall::Animated(
                file(path),
                first,
                count,
                indices.map(<[usize]>::to_vec),
                delays.map(<[f32]>::to_vec),
                beat,
            ));
            None
        };
        let result = if old {
            baseline::old_first_actor_sprite_slot(&data, &actors, &path, texture, frame, animated)
        } else {
            itg_first_actor_sprite_slot(&data, &actors, &path, texture, frame, animated)
        };
        (result, calls.into_inner())
    };
    let old = run(true);
    let new = run(false);
    assert_eq!(new, old);
    assert_eq!(new.0, Some(1));
    assert_eq!(new.1.len(), 4);
    assert!(
        matches!(&new.1[0], LoadCall::Animated(name, 0, 16, Some(indices), Some(delays), _) if name == "first.png" && indices.len() == 16 && delays == &vec![0.0625; 16])
    );
    assert_eq!(new.1[1], LoadCall::Frame("first.png".into(), 0));
    assert_eq!(new.1[2], LoadCall::Texture("first.png".into()));
    assert_eq!(new.1[3], LoadCall::Frame("second.png".into(), 1));
}

#[test]
#[ignore = "manual release benchmark; filter benchmark_load_preparation"]
fn benchmark_load_preparation_noteskin() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    let loader = loader_fixture();
    for (name, button, element) in [
        ("request_plain", "left", "Tap Note"),
        ("request_command", "up", "receptor"),
        ("request_blank", "Down", "Roll Explosion"),
        ("request_fallback", "Custom", "Custom Head"),
    ] {
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            if old {
                perf::measure_sampled(&format!("{name}/old"), 100_000, 1, || {
                    baseline::old_load_request(
                        black_box(&loader),
                        black_box(button),
                        black_box(element),
                    )
                });
            } else {
                perf::measure_sampled(&format!("{name}/new"), 100_000, 1, || {
                    black_box(&loader).load_request_ref(black_box(button), black_box(element))
                });
            }
        }
    }
    for count in [0, 1, 8, 32, 128] {
        let (actors, dirs, path) = actor_fixture(count);
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let name = format!("actor_{count}/{}", if old { "old" } else { "new" });
            let iterations = if count >= 32 { 1_000 } else { 10_000 };
            if old {
                perf::measure_sampled(&name, iterations, 1, || {
                    baseline::old_decl_for_path(
                        black_box(&actors),
                        black_box(&dirs),
                        black_box(&path),
                    )
                });
            } else {
                perf::measure_sampled(&name, iterations, 1, || {
                    black_box(&actors).decl_for_path_ref(black_box(&dirs), black_box(&path))
                });
            }
        }
    }
    let (actors, dirs, _) = actor_fixture(32);
    let path = dirs[0].join("missing.lua");
    for old in if reverse {
        [false, true]
    } else {
        [true, false]
    } {
        if old {
            perf::measure_sampled("actor_missing/old", 10_000, 1, || {
                baseline::old_decl_for_path(black_box(&actors), black_box(&dirs), black_box(&path))
            });
        } else {
            perf::measure_sampled("actor_missing/new", 10_000, 1, || {
                black_box(&actors).decl_for_path_ref(black_box(&dirs), black_box(&path))
            });
        }
    }
}
