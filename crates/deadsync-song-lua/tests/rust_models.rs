use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    self as song, SongLuaCompileContext,
    rust_engine::{ActorScene, RustSongRuntime},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
#[path = "support/paths.rs"]
mod paths;

fn execute(runtime: &mut RustSongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/models.lua").unwrap())
        .unwrap_or_else(|e| panic!("{source}\n{e:?}"))
        .to_vec()
}
fn path_snapshot(paths: &[PathBuf; 3]) -> [String; 3] {
    paths.each_ref().map(|path| song::file_path_string(path))
}

struct Pair {
    rust: RustSongRuntime,
    scene: ActorScene,
    seconds: f32,
}
impl Pair {
    fn new(source: &str, live: bool) -> Self {
        Self::with_path(source, live, None)
    }
    fn fixture(name: &str, live: bool) -> Self {
        let path = fixture(name);
        let source = std::fs::read_to_string(&path).unwrap();
        Self::with_path(&source, live, Some(&path))
    }
    fn with_path(source: &str, live: bool, path: Option<&Path>) -> Self {
        reference_test::register_path(&fixture(""));
        reference_test::register_path(Path::new(&song::file_path_string(
            &fixture("").canonicalize().unwrap(),
        )));
        reference_test::register_path(&std::env::current_dir().unwrap());
        let mut context = SongLuaCompileContext::new(Path::new("."), "model migration");
        context.live = live;
        let source = format!(
            "seen={{}};function log(...) for i=1,select('#',...) do seen[#seen+1]=tostring(select(i,...)) end end\n{source}"
        );

        let mut rust = RustSongRuntime::new(&context).unwrap();
        let chunk = path
            .map(|path| format!("@{}", song::file_path_string(path)))
            .unwrap_or_else(|| "@/fixture/models.lua".into());
        let value = rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), chunk.as_bytes()).unwrap())
            .unwrap()[0];
        let scene = rust.instantiate_actor(value).unwrap();
        let root = rust.actor_scene_root(scene).unwrap();
        rust.vm_mut().set_global(b"root", root).unwrap();
        let mut pair = Self {
            rust,
            scene,
            seconds: 0.,
        };
        pair.compare("construction");
        pair
    }
    fn actor(&mut self, name: &str) -> Value {
        self.rust.vm_mut().get_global(name.as_bytes()).unwrap()
    }
    fn compare(&mut self, phase: &str) {
        let actual = self
            .rust
            .actor_scene_snapshot(self.scene)
            .unwrap()
            .into_iter()
            .map(|actor| actor.render)
            .collect::<Vec<_>>();
        reference_test::assert_debug("song/tests/rust_models.rs::render", &(phase, &actual));
        let Value::String(actual) = execute(&mut self.rust, "return table.concat(seen,'|')")[0]
        else {
            panic!("observations")
        };
        reference_test::assert_debug(
            "song/tests/rust_models.rs::observations",
            &(phase, &self.rust.vm_mut().string_bytes(actual).unwrap()),
        );
    }
    fn script(&mut self, source: &str) {
        execute(&mut self.rust, source);
        self.compare(source);
    }
    fn advance(&mut self, delta: f64) {
        self.seconds += delta as f32;

        self.rust
            .set_values(self.seconds * 2., self.seconds)
            .unwrap();

        self.rust.advance_actor_scene(self.scene, delta).unwrap();
        self.compare("advance");

        self.rust.sample_model_materials(self.seconds);
    }
    fn install<V>(&mut self, name: &str, layers: &[song::SongLuaOverlayModelLayer<V>]) {
        let actor = self.actor(name);
        self.rust
            .install_actor_model_materials(actor, layers)
            .unwrap();
    }
    fn compare_samples(&mut self, name: &str) -> Vec<Arc<[song::SongLuaTextureSample]>> {
        let actor = self.actor(name);
        let actual = self.rust.take_actor_model_material_samples(actor).unwrap();
        reference_test::assert_debug("song/tests/rust_models.rs::samples", &(name, &actual));
        actual
    }
}
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/itgmania-song-lua-micro")
        .join(name)
}
const MODEL: &str = "return Def.ActorFrame{Def.Model{Name='model',InitCommand=function(self)m=self end},InitCommand=function(self)frame=self end}";

#[test]
fn model_construction_defaults_inheritance_and_immediate_render_controls() {
    for live in [false, true] {
        let mut p = Pair::new(MODEL, live);
        p.script(
            r#"
            assert(Model==nil and m.SetUpdateFunction==nil and type(m.GetChildren)=='function');assert(m:GetNumChildren()==0)
            assert(m:GetName()=='model' and m:GetParent()==root)
            log(m:GetWidth(),m:GetHeight(),m:GetNumStates(),m:GetState())
            m:zbuffer(false):backfacecull(false):texturewrapping(false):animate(true):setstate(3)
            m:linear(1):x(30):diffusealpha(.25)
            local bound=m.Load
            Actor.Load=function(self)assert(self==m);return 'override'end
            assert(m:Load('unused')=='override');Actor.Load=nil
            assert(bound({},nil)==m)
        "#,
        );
        p.advance(0.25);
        p.script("m:finishtweening():zbuffer(true):cullmode('CullMode_Front');log(m:GetX(),m:GetState())");
    }
}

#[test]
fn model_private_child_queries_preserve_names_groups_and_registry_aliases() {
    let mut p = Pair::new(
        r#"return Def.Model{InitCommand=function(self)m=self end,
            Def.Actor{Name='same',InitCommand=function(self)a=self;log('a')end},
            Def.Actor{Name='same',InitCommand=function(self)b=self;log('b')end},
            Def.Actor{InitCommand=function(self)c=self;log('c')end},
            Def.Actor{Name='hidden',Condition=false}}
        "#,
        false,
    );
    p.script(
        r#"
        assert(m:GetNumChildren()==3 and a:GetParent()==m and b:GetParent()==m)
        local group=m:GetChild('same');assert(group[1]==a and group[2]==b)
        assert(m:GetChild('')==c and m:GetChild('missing')==nil)
        assert(m:GetChildren().same[2]==b)
        local bound=m.GetChild;assert(bound({},'')==c)
        m.__songlua_children.alias=a
        assert(m:GetChild('alias')==a and m:GetNumChildren()==3)
        a:name('renamed');assert(m:GetChild('same')==b and m:GetChild('renamed')==a)
        assert(m:GetChildren().alias==a)
        collectgarbage('collect');log(m:GetNumChildren())
    "#,
    );
}

#[test]
fn model_paths_preserve_fallbacks_getter_order_gc_and_failed_query_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    reference_test::register_path(directory.path());
    for name in ["geometry.txt", "surface.txt", "bones.txt"] {
        std::fs::write(directory.path().join(name), b"fixture").unwrap();
    }
    let dir = song::file_path_string(directory.path());
    let mut p = Pair::new(
        &format!("return Def.Model{{_Dir={dir:?},InitCommand=function(self)m=self end}}"),
        true,
    );
    p.script(&format!("m.__songlua_script_dir={dir:?};m._Dir={dir:?}"));
    for script in [
        "m.Meshes=nil;m.Materials=nil;m.Bones=nil",
        "m.Meshes='geometry.txt'",
        "m.Materials='surface.txt';m.Bones='bones.txt'",
        "m.Meshes=' ';m.Materials='surface.txt';m.Bones=nil",
        "m.Meshes='missing.txt'",
        "m.Meshes=false",
        "m.Meshes='geometry';m.Materials=nil;m.Bones=nil",
    ] {
        p.script(script);
        let actor = p.actor("m");
        let actual = p.rust.actor_model_paths(actor);
        reference_test::assert_debug(
            "song/tests/rust_models.rs::path_queries",
            &(
                script,
                actual.is_err(),
                actual
                    .as_ref()
                    .ok()
                    .map(|paths| paths.as_ref().map(path_snapshot)),
            ),
        );
    }
    p.script(r#"
        m.Meshes=nil;m.Materials=nil;m.Bones=nil;m.__songlua_script_dir=nil
        local old=getmetatable(m).__index
        getmetatable(m).__index=function(self,key)
            if key=='Meshes' or key=='Materials' or key=='Bones' then
                collectgarbage('collect');log(key);return key=='Materials' and 'surface.txt' or nil
            elseif key=='__songlua_script_dir' then collectgarbage('collect');log('directory');return self._Dir end
            return old(self,key)
        end
    "#);
    let actor = p.actor("m");
    reference_test::assert_debug(
        "song/tests/rust_models.rs::collecting_paths",
        &p.rust
            .actor_model_paths(actor)
            .unwrap()
            .as_ref()
            .map(path_snapshot),
    );
    p.compare("collecting path getters");
    p.rust.release_actor_scene(p.scene).unwrap();
    assert!(p.rust.actor_model_paths(actor).is_err());
}

#[test]
fn existing_model_material_fixture_loads_identical_owned_geometry_and_materials() {
    paths::init_paths();
    let mut p = Pair::fixture("model-material/default.lua", false);
    for name in ["Triangle", "Unassigned", "Cyber", "DarkCyber"] {
        p.script(&format!("m=root:GetChild({name:?})"));
        let actor = p.actor("m");
        let expected = p.rust.actor_model_paths(actor).unwrap().unwrap();
        reference_test::assert_debug(
            "song/tests/rust_models.rs::fixture_paths",
            &(name, path_snapshot(&expected)),
        );
        let slots = deadsync_assets::noteskin::load_itg_model_slots(
            &expected[0],
            &expected[1],
            &expected[2],
        )
        .unwrap();
        let expected = song::overlay_model_layers_from_slots(
            &slots,
            deadsync_assets::song_lua::model_layer_from_slot,
        );
        let actual = p
            .rust
            .actor_model_layers(
                actor,
                deadsync_assets::noteskin::load_itg_model_slots,
                deadsync_assets::song_lua::model_layer_from_slot,
            )
            .unwrap();
        assert_eq!(format!("{actual:?}"), format!("{expected:?}"), "{name}");
        assert!(actual.is_some());
    }
    p.rust.release_actor_scene(p.scene).unwrap();
}

#[test]
fn existing_texture_order_fixture_preserves_hibernation_parent_rates_and_queued_selection() {
    paths::init_paths();
    let mut p = Pair::fixture("model-texture-order/default.lua", false);
    p.script("q=root:GetChild('Queued');s=root:GetChild('Sleeping');c=root:GetChild('SleepingParent'):GetChild('SleepingChild');f=root:GetChild('FastParent'):GetChild('FastChild')");
    for name in ["q", "s", "c", "f"] {
        let actor = p.actor(name);
        let layers = p
            .rust
            .actor_model_layers(
                actor,
                deadsync_assets::noteskin::load_itg_model_slots,
                deadsync_assets::song_lua::model_layer_from_slot,
            )
            .unwrap()
            .unwrap();
        p.install(name, &layers);
    }
    for delta in [0.125, 0.125, 0.25, 0.5, 0.0, 0.125, 0.75, 0.0] {
        p.advance(delta);
    }
    for name in ["q", "s", "c", "f"] {
        let samples = p.compare_samples(name);
        assert!(samples.iter().any(|s| s.len() == 9));
    }
}

fn layer() -> song::SongLuaOverlayModelLayer<()> {
    let mut layer = song::SongLuaOverlayModelLayer::new(
        Arc::from("material"),
        Arc::from([]),
        [1., 1.],
        [1., 1.],
        [0., 0.],
        [0., 0.],
        [0.25, -0.5],
        Some(1.),
        song::SongLuaOverlayModelDraw::new([0.; 3], [0.; 3], [1.; 3], [1.; 4], 0.5, false, true),
    );
    layer.texture_frames = (0..3)
        .map(|i| {
            let diffuse = song::SongLuaModelTextureUv {
                scale: [1. + i as f32, 0.5],
                offset: [0.1, 0.2],
                shift: [0.3, 0.4],
            };
            let glow = song::SongLuaModelTextureUv {
                scale: [0.5, 1.],
                offset: [0., 0.],
                shift: [0., 0.],
            };
            song::SongLuaModelTextureFrame {
                texture_key: None,
                delay: 0.25 * (i + 1) as f32,
                diffuse,
                glow,
            }
        })
        .collect::<Vec<_>>()
        .into();
    layer.additive_frames = Arc::clone(&layer.texture_frames);
    layer
}

#[test]
fn material_state_selection_callback_order_static_layers_and_reset_match_control() {
    let mut p = Pair::new(MODEL, true);
    p.script("m:setstate(2):animate(false);m.__songlua_update_function=function(self,dt) self:setstate(1);collectgarbage('collect') end");
    let dynamic = layer();
    let mut static_layer = dynamic.clone();
    static_layer.texture_frames = Arc::from([]);
    static_layer.additive_frames = Arc::from([]);
    static_layer.uv_velocity = [0.; 2];
    p.install("m", &[dynamic.clone(), static_layer.clone()]);
    for delta in [0.25, 1.5, 0.0, 0.0, 0.125] {
        p.advance(delta);
    }
    let samples = p.compare_samples("m");
    assert_eq!(samples.len(), 2);
    assert!(samples[1].is_empty());
    p.script("m.__songlua_update_function=nil;m:setstate(4294967295)");
    p.install("m", &[dynamic, static_layer]);
    p.script("m:setstate(4294967295);m:setstate(-1);m:setstate('2');m:setstate(0.5)");
    p.advance(0.125);
    p.compare_samples("m");
    let actor = p.actor("m");
    assert!(
        p.rust
            .take_actor_model_material_samples(actor)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn capture_owns_frame_data_until_finish_or_last_scene_release() {
    let mut p = Pair::new(MODEL, false);
    let actor = p.actor("m");
    let data = layer();
    let weak = Arc::downgrade(&data.texture_frames);
    p.rust
        .install_actor_model_materials(actor, &[data.clone()])
        .unwrap();
    drop(data);
    assert!(weak.upgrade().is_some());
    execute(&mut p.rust, "collectgarbage('collect');m:setstate(2)");
    p.rust.advance_actor_scene(p.scene, 0.5).unwrap();
    p.rust.sample_model_materials(0.5);
    let samples = p.rust.take_actor_model_material_samples(actor).unwrap();
    assert!(weak.upgrade().is_none() && samples[0].len() == 2);
    let data = layer();
    let weak = Arc::downgrade(&data.texture_frames);
    p.rust
        .install_actor_model_materials(actor, &[data.clone()])
        .unwrap();
    drop(data);
    p.rust.release_actor_scene(p.scene).unwrap();
    assert!(weak.upgrade().is_none());
    assert!(p.rust.take_actor_model_material_samples(actor).is_err());
    assert!(
        p.rust
            .install_actor_model_materials(actor, &[layer()])
            .is_err()
    );
    assert_eq!(samples[0].len(), 2);
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
