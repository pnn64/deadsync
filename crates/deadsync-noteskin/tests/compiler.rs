use deadsync_noteskin::compiler;
use deadsync_noteskin::{compiled, itg};

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[path = "../../deadsync-assets/tests/support/skin_fixture.rs"]
mod skin_fixture;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "deadsync-rust-noteskin-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, source: &str) {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn data(paths: Vec<PathBuf>) -> itg::NoteskinData {
    itg::NoteskinData {
        overrides: vec![],
        name: "rust-engine-fixture".into(),
        metrics: itg::IniData::default(),
        search_dirs: paths,
    }
}
fn equal_bundles(
    left: &compiled::CompiledNoteskinBundle,
    right: &compiled::CompiledNoteskinBundle,
) {
    assert_eq!(
        (left.version, &left.game, &left.skin, &left.source_hash),
        (right.version, &right.game, &right.skin, &right.source_hash)
    );
    assert_eq!(
        (left.loader.version, &left.loader.game, &left.loader.skin),
        (right.loader.version, &right.loader.game, &right.loader.skin),
    );
    assert_eq!(left.loader.entries.len(), right.loader.entries.len());
    for (index, (left, right)) in left
        .loader
        .entries
        .iter()
        .zip(&right.loader.entries)
        .enumerate()
    {
        assert_eq!(left, right, "loader entry {index}");
    }
    assert_eq!(left.actors.version, right.actors.version);
    assert_eq!(left.actors.files.len(), right.actors.files.len());
    for (left, right) in left.actors.files.iter().zip(&right.actors.files) {
        assert_eq!(left.key, right.key);
        assert_eq!(
            serde_json::to_value(&left.decl).unwrap(),
            serde_json::to_value(&right.decl).unwrap(),
            "{}",
            left.key
        );
        // JSON normalizes nonfinite numbers and compares the two zero signs
        // equally. Preserve every f32 bit in the declaration's delay arrays.
        for (left_sprite, right_sprite) in left.decl.sprites.iter().zip(&right.decl.sprites) {
            let bits = |sprite: &deadsync_noteskin::actor::ItgLuaSpriteDecl| {
                sprite.frame_delays.as_ref().map(|delays| {
                    delays
                        .iter()
                        .map(|value| value.to_bits())
                        .collect::<Vec<_>>()
                })
            };
            assert_eq!(
                bits(left_sprite),
                bits(right_sprite),
                "{} frame delays",
                left.key
            );
        }
    }
}
fn compile_and_roundtrip(game: &str, data: &itg::NoteskinData) {
    let rust = compiler::compile_data(game, data, "same-input", "1.5x, Reverse").unwrap();
    assert_eq!((rust.loader.version, rust.actors.version), (30, 30));
    let output = Fixture::new();
    let path = output.0.join("bundle.bin");
    compiled::save_compiled_bundle(&path, &rust).unwrap();
    let restored = compiled::load_compiled_bundle(&path).unwrap();
    equal_bundles(&rust, &restored);
}

#[test]
fn backend_cutover_invalidates_the_legacy_cache_without_deleting_it() {
    let fixture = Fixture::new();
    fixture.write(
        "NoteSkin.lua",
        "return {Load=function() return Def.Actor{} end}",
    );
    let mut data = data(vec![fixture.0.clone()]);
    data.name = "cache-version-transition".into();
    let cache = fixture.0.join("cache");
    let old = compiled::load_compiled_bundle(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cache-version-29.bin"),
    )
    .unwrap();
    assert_eq!((old.loader.version, old.actors.version), (29, 29));
    let old_hash = old.source_hash.clone();
    let old_path = compiled::compiled_bundle_path(&cache, "dance", &data.name, &old_hash);
    compiled::save_compiled_bundle(&old_path, &old).unwrap();
    let old_bytes = fs::read(&old_path).unwrap();
    assert_eq!(
        compiler::ensure_compiled(&cache, "dance", &data).unwrap(),
        compiler::CompileOutcome::Built
    );
    let current = compiler::load_compiled(&cache, "dance", &data).unwrap();
    assert_eq!((current.loader.version, current.actors.version), (30, 30));
    assert_ne!(current.source_hash, old_hash);
    assert_eq!(fs::read(&old_path).unwrap(), old_bytes);
    assert_eq!(
        compiler::ensure_compiled(&cache, "dance", &data).unwrap(),
        compiler::CompileOutcome::Reused
    );
}

#[test]
fn bundled_noteskins_compile_and_roundtrip_with_their_real_fallback_chains() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/noteskins");
    for (game, count) in [("dance", 12), ("pump", 14)] {
        let skins = itg::discover_skins(std::slice::from_ref(&root), game);
        assert_eq!(skins.len(), count, "bundled {game} inventory changed");
        for skin in skins {
            eprintln!("bundled {game}/{skin}");
            let data = itg::load_noteskin_data(&root, game, &skin).unwrap();
            compile_and_roundtrip(game, &data);
        }
    }
}

#[test]
fn authored_third_party_and_song_local_noteskins_compile_and_roundtrip() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/noteskins");
    for name in [
        "FNF-3d",
        "FNF_Quantize-3d",
        "composite-3d",
        "enchantment-original-3d",
        "enchantment-original-3d+",
    ] {
        let fixture = Fixture::new();
        skin_fixture::install(name, &fixture.0.join(name));
        let data = itg::load_song_skin_data(&fixture.0, std::slice::from_ref(&root), "dance", name)
            .unwrap();
        eprintln!("authored dance/{name}");
        compile_and_roundtrip("dance", &data);
    }
    let fixture = Fixture::new();
    skin_fixture::install("rosewood", &fixture.0);
    let data =
        itg::load_song_skin_data(&fixture.0, &[root], "dance", "SCH-CLASSIC-RAINBOW").unwrap();
    eprintln!("authored dance/SCH-CLASSIC-RAINBOW");
    compile_and_roundtrip("dance", &data);
}

#[test]
fn existing_noteskin_fixtures_roundtrip_complete_bundles() {
    for name in [
        "alias-normalization",
        "base-xy-rotation",
        "game-aware",
        "implicit-animation",
        "note-color-texture",
        "rainbow-effect",
        "shock-mine",
    ] {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        let mut data = data(vec![root.clone()]);
        data.metrics = itg::IniData::parse_file(&root.join("metrics.ini")).unwrap();
        for game in ["dance", "pump"] {
            eprintln!("compare {name}/{game}");
            if name == "alias-normalization" && game == "pump" {
                // This fixture deliberately has only dance button aliases.
                // Preserve the reference failure as an explicit rejection check.
                let error = compiler::compile_data(game, &data, "same-input", "").unwrap_err();
                assert!(error.contains("assertion failed"), "{error}");
                continue;
            }
            compile_and_roundtrip(game, &data);
        }
    }
}

#[test]
fn fallback_closures_commands_metrics_and_deferred_loaders_survive_collection() {
    let fixture = Fixture::new();
    fixture.write(
        "base/NoteSkin.lua",
        r#"
        assert(Var 'Button' == nil and Var 'Element' == nil)
        local state, marker = {count=0}, {}
        return { marker=marker, bump=function() state.count=state.count+1; return state.count end }
    "#,
    );
    fixture.write(
        "derived/NoteSkin.lua",
        r#"
        local skin = ...
        local marker = skin.marker
        assert(GAMESTATE:GetPlayerState():GetPlayerOptionsString() == '1.5x, Reverse')
        assert(THEME:GetThemeAuthor() == '')
        assert(NOTESKIN:GetMetric('Other','Test') == 'fallback')
        skin.Load = function()
            collectgarbage()
            assert(marker == skin.marker and skin.bump() > 0)
            local load = loadfile(NOTESKIN:GetPath('Down', 'Tap Note'))
            assert(type(load) == 'function')
            collectgarbage()
            local actor = load()
            actor.BaseRotationX, actor.BaseRotationY, actor.BaseRotationZ = '12', 34.5, -56
            actor.InitCommand = function(self)
                collectgarbage()
                self:x(1):zoom(1.25):visible(false):diffuse('red', nil, true)
            end
            return actor
        end
        return skin
    "#,
    );
    fixture.write(
        "derived/Down Tap Note.lua",
        "return Def.Sprite { Texture='note.png' }",
    );
    fixture.write("derived/metrics.ini", "[NoteDisplay]\nTest=fallback\n");
    let mut data = data(vec![fixture.0.join("derived"), fixture.0.join("base")]);
    data.metrics = itg::IniData::parse_file(&fixture.0.join("derived/metrics.ini")).unwrap();
    compile_and_roundtrip("dance", &data);
}

#[test]
fn color_variant_resource_expressions_and_model_properties_roundtrip() {
    let fixture = Fixture::new();
    fixture.write(
        "NoteSkin.lua",
        r#"
        local previous = setmetatable({}, {__mode='v'})
        return {Load=function()
            collectgarbage()
            assert(previous[1] == nil, 'completed actor retained by host')
            local actor = Def.Sprite {Texture=NOTESKIN:GetPath(Var 'Button', Var 'Color')}
            previous[1] = actor
            return actor:rotationz(Var 'Color' == '4th' and 0 or 90)
        end}
    "#,
    );
    fixture.write(
        "Down Tap Note.lua",
        r#"
        local color, button = Var 'Color', Var 'Button'
        collectgarbage()
        return Def.ActorFrame {
            Def.Model { Meshes='mesh-'..color, Materials='material-'..button,
                Bones='bones-'..color, Texture=NOTESKIN:GetPath(button, color) },
            Def.Sprite { Texture='texture-'..color }
        }
    "#,
    );
    compile_and_roundtrip("dance", &data(vec![fixture.0.clone()]));
}

#[test]
fn property_metamethods_and_numeric_string_coercion_roundtrip() {
    let fixture = Fixture::new();
    fixture.write(
        "NoteSkin.lua",
        r#"
        return {Load=function()
            local properties = setmetatable({}, {__index=function(_, key)
                collectgarbage()
                if key == 'Texture' then
                    return setmetatable({}, {__index=function(_, part)
                        collectgarbage()
                        if part == 'load_button' then return 1.234567890123456 end
                        if part == 'load_element' then return 17 end
                    end})
                end
                if key == 'BaseRotationZ' then return '45.5' end
                if key == 'InitCommand' then
                    return function(self) collectgarbage(); self:x(-0):y(1/0):z(0/0) end
                end
            end})
            return Def.Sprite(properties)
        end}
    "#,
    );
    compile_and_roundtrip("dance", &data(vec![fixture.0.clone()]));
}

#[test]
fn invalid_loaders_and_resource_paths_are_rejected() {
    for (loader, actor, expected) in [
        ("error('loader exploded')", "", "loader exploded"),
        (
            "return {Load=function() error('callback exploded') end}",
            "",
            "callback exploded",
        ),
        ("return {}", "", "Load"),
        (
            "return {Load=function() return Def.Actor{} end}",
            "local color=Var 'Color'; return Def.Sprite {Texture=42}",
            "invalid noteskin resource path",
        ),
    ] {
        let fixture = Fixture::new();
        fixture.write("NoteSkin.lua", loader);
        if !actor.is_empty() {
            fixture.write("Down Tap Note.lua", actor);
        }
        let data = data(vec![fixture.0.clone()]);
        let rust = compiler::compile_data("dance", &data, "error", "").unwrap_err();
        assert!(rust.contains(expected), "{rust}");
    }
}
