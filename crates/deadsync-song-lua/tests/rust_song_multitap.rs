use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    MultitapDesc, SongLuaCompileContext, SongLuaNoteskinResolver, SongLuaOverlayKind,
    SongLuaOverlayState,
    rust_engine::{ActorScene, RustSongRuntime, SongLayerPlan, SongOverlayActors},
};
use std::sync::Arc;

type Kind = SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>;
struct Pair {
    rust: RustSongRuntime,
    rust_root: Value,
    screen: ActorScene,
    context: SongLuaCompileContext,
    tracked: deadsync_song_lua::rust_engine::SongTrackedActors,
    activation: deadsync_song_lua::rust_engine::SongActivation,
    rows: SongOverlayActors<Kind>,
    _dir: tempfile::TempDir,
}
impl Pair {
    fn stage(&mut self) -> deadsync_song_lua::rust_engine::SongUpdateOutput {
        let (mut b_sound, mut b_judgment, mut b_sprite) = (Vec::new(), Vec::new(), Vec::new());
        let b = self
            .rust
            .compile_song_update_stage(
                self.screen,
                self.rust_root,
                &self.context,
                &mut self.rows,
                &mut self.tracked,
                &[],
                &self.activation,
                &mut b_sound,
                &mut b_judgment,
                &mut b_sprite,
                SongLuaNoteskinResolver::default(),
                |_| None,
                |_| Some(visual()),
            )
            .unwrap();
        reference_test::assert_debug(
            "song/tests/rust_song_multitap.rs::stage",
            &(
                &b.eases,
                &b.overlay_eases,
                &b.overlay_tracks,
                &b.column_transforms,
                &b.column_splines,
                &b.stateful_messages,
                &b.broadcasts,
                &b_sound,
                &b_judgment,
                b_sprite.len(),
            ),
        );
        self.compare_rows();
        b
    }
    fn new(source: &str, configure: impl FnOnce(&mut SongLuaCompileContext)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        reference_test::register_path(dir.path());
        let path = dir.path().join("default.lua");
        std::fs::write(&path, source).unwrap();
        image::RgbaImage::new(2, 2)
            .save(dir.path().join("_missing.png"))
            .unwrap();
        let mut context = SongLuaCompileContext::new(dir.path(), "multitap preparation");
        context.music_length_seconds = 0.1;
        context.background_layer_count = 1;
        context.players[1].enabled = false;
        configure(&mut context);

        let mut rust = RustSongRuntime::new(&context).unwrap();
        rust.begin_song_compilation().unwrap();
        let (screen, selected) = rust.construct_gameplay_screen(&context).unwrap();
        rust.load_gameplay_theme(screen, &selected).unwrap();
        let rust_root = rust
            .initialize_song_layers(screen, &SongLayerPlan::new(&[&path], 0, &context).unwrap())
            .unwrap()
            .unwrap();
        let rust_activation = rust
            .activate_song_layers(screen, &context)
            .unwrap()
            .unwrap();

        rust.discover_song_startup_commands(screen, &rust_activation)
            .unwrap();

        let rows = rust
            .read_song_overlay_actors::<(), ()>(
                rust_root,
                &context,
                |_, _, _| Ok(Arc::from([])),
                |_| None,
                |_| {},
            )
            .unwrap();

        let tracked = rust.read_song_tracked_actors(screen).unwrap().unwrap();
        Self {
            tracked,

            activation: rust_activation,
            rust,
            rust_root,
            screen,
            context,

            rows,
            _dir: dir,
        }
    }
    fn exec(&mut self, source: &str) {
        self.rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"=multitap test mutation").unwrap())
            .unwrap();
    }
    fn isolate_factory_plan(&mut self) {
        // Exercise the analytic helper without gameplay theme jobs. The complete
        // loaded theme remains covered by the separate rejection comparisons.
        self.exec("__songlua_top_screen=nil;__songlua_top_screen_player_1=nil;__songlua_top_screen_player_2=nil");
    }
    fn gate(&mut self) -> Result<bool, String> {
        let b = self
            .rust
            .song_multitap_update_only(self.rust_root)
            .map_err(|e| e.to_string());
        reference_test::assert_debug(
            "song/tests/rust_song_multitap.rs::gate",
            &(b.is_err(), b.as_ref().ok()),
        );
        b
    }
    fn compare_rows(&self) {
        reference_test::assert_debug(
            "song/tests/rust_song_multitap.rs::rows",
            &self
                .rows
                .actors
                .iter()
                .map(|row| {
                    (
                        &row.actor.name,
                        row.actor.parent_index,
                        &row.actor.initial_state,
                        &row.actor.message_commands,
                        &row.actor.kind,
                    )
                })
                .collect::<Vec<_>>(),
        );
    }
    fn compile(&mut self) -> Option<Vec<deadsync_song_lua::SongLuaOverlayEase>> {
        let mut b_calls = Vec::new();
        let b = self.rust.compile_song_multitap_update_overlays(
            self.rust_root,
            &self.context,
            &mut self.rows,
            SongLuaNoteskinResolver::default(),
            |skin| {
                b_calls.push(skin.to_owned());
                Some(visual())
            },
        );
        reference_test::assert_debug(
            "song/tests/rust_song_multitap.rs::compiled",
            &(b.is_err(), b.as_ref().ok(), &b_calls),
        );
        self.compare_rows();
        b.ok().flatten()
    }
    fn hits(&mut self) -> bool {
        let b = self
            .rust
            .install_song_multitap_hits(&self.context, &mut self.rows);
        reference_test::assert_debug("song/tests/rust_song_multitap.rs::hits", &b.is_ok());
        self.compare_rows();
        b.is_ok()
    }
    fn finish(mut self) {
        self.rust.vm_mut().collect_garbage().unwrap();
        for row in &self.rows.actors {
            assert!(matches!(
                self.rust.song_overlay_actor_value(row).unwrap(),
                Value::Table(_)
            ));
        }
        self.compare_rows();
        self.rust
            .release_song_overlay_actors(&mut self.rows)
            .unwrap();
        self.rust
            .release_song_tracked_actors(&mut self.tracked)
            .unwrap();
        self.rust.release_actor_scene(self.screen).unwrap();
        self.rust.vm_mut().collect_garbage().unwrap();
    }
}
fn visual() -> (Kind, SongLuaOverlayState) {
    (
        SongLuaOverlayKind::Model {
            layers: Arc::from([]),
        },
        SongLuaOverlayState {
            zoom_y: -1.,
            ..Default::default()
        },
    )
}
fn factory(numbered: bool, players: usize, arrow_visual: bool) -> String {
    let mut source = String::from(
        "multitaps={Challenge={{lane=1,taps={1,2,3},peak=1.2},{lane=4,taps={3,4}}},Double_Challenge={{lane=8,taps={1,2,3},peak=1.2},{lane=4,taps={3,4}}}}; MYSTERIOUS_VERSION_DEPENDENT_RADIAN_POLTERGEIST=4; saved={}; local root=Def.ActorFrame{}; ",
    );
    for pn in 1..=players {
        source.push_str(&format!(
            "root[#root+1]=Def.ActorFrame{{Name='MultitapFrameP{pn}'}};"
        ));
        source.push_str(&format!("multitap_note_callback_P{pn}=function()end;"));
        for index in 1..=2 {
            let class = if arrow_visual { "Sprite" } else { "ActorFrame" };
            let texture = if arrow_visual {
                ",Texture='_missing.png'"
            } else {
                ""
            };
            let deco = if numbered {
                format!(
                    "Def.BitmapText{{Name='MultitapTextP{pn}_{index}',Font='Common Normal',Text='3'}}"
                )
            } else {
                format!(
                    "Def.ActorFrame{{Name='MultitapDeco{pn}_{index}',Def.Quad{{Name='deco{pn}_{index}',InitCommand=function(self)self:zoom(.5)end}}}}"
                )
            };
            source.push_str(&format!("root[#root+1]=Def.ActorFrame{{Name='MultitapP{pn}_{index}',InitCommand=function(self)self:y(15)end,Def.{class}{{Name='MultitapArrowP{pn}_{index}'{texture},InitCommand=function(self)saved.arrow=self;self.__songlua_noteskin_name='default' end}},{deco}}};"));
        }
        source.push_str(&format!("root[#root+1]=Def.ActorFrame{{Name='MultitapExplosionP{pn}_1',InitCommand=function(self)saved.explosion=self end,JudgmentCommand=function(self) self:x(10); collectgarbage() end,DimCommand=function(self) self:linear(.1):diffusealpha(.2) end,W1Command=function(self) self:y(11)end,W3Command=function(self)self:zoom(3)end,Def.Quad{{Name='burst{pn}',JudgmentCommand=function(self)self:z(6)end,W2Command=function(self)self:linear(.2):zoom(2)end}}}};"));
    }
    source.push_str("root[#root+1]=Def.ActorFrame{Name='Update',InitCommand=function(self)saved.update=self;self:SetUpdateFunction(function()end)end}; saved.root=root; return root");
    source
}
fn descs(rows: Option<Vec<MultitapDesc>>) -> Option<Vec<(usize, Vec<f32>, Option<f32>)>> {
    rows.map(|rows| rows.into_iter().map(|r| (r.lane, r.taps, r.peak)).collect())
}

#[test]
fn descriptors_preserve_selection_coercion_holes_and_peak_errors() {
    for double in [false, true] {
        for source in [
            "multitaps=nil",
            "multitaps={Challenge={{lane='1.8',taps={3,' 2 ',false,1,0/0,1/0},peak=' 1.4 '},false,{lane=9,taps={1}},{lane=-1,taps={1}},{lane=2,taps={false}},nil,{lane=3,taps={1}}},Double_Challenge={{lane=8,taps={2,1}}}}",
            "multitaps=setmetatable({Challenge={{lane=1,taps={2,1}}}},{__index=function(_,k)collectgarbage();return nil end})",
            "multitaps={Challenge={setmetatable({lane=1,taps={2,1}},{__index=function()error('peak suppressed')end})}}",
            "multitaps={Challenge={setmetatable({},{__index=function(_,k)collectgarbage();if k=='lane' then return 2 elseif k=='taps' then multitaps=nil;return {4,2} end end})}}",
        ] {
            let context_dir = tempfile::tempdir().unwrap();
            reference_test::register_path(context_dir.path());
            let mut context = SongLuaCompileContext::new(context_dir.path(), "descriptor reader");
            if double {
                context.style_name = "double".into();
            }

            let mut rust = RustSongRuntime::new(&context).unwrap();
            rust.vm_mut()
                .execute(&compile_named(source.as_bytes(), b"=descriptors").unwrap())
                .unwrap();
            reference_test::assert_debug(
                "song/tests/rust_song_multitap.rs::descriptors",
                &(
                    source,
                    double,
                    &descs(rust.read_song_multitap_descs(&context).unwrap()),
                ),
            );
        }
    }
}
#[test]
fn descriptor_typed_errors_match_and_challenge_fallback_suppresses_only_fallback_errors() {
    for source in [
        "multitaps=false",
        "multitaps={Challenge=false}",
        "multitaps={Challenge={{lane=false,taps={1}}}}",
        "multitaps={Challenge={{lane=1,taps=false}}}",
    ] {
        let dir = tempfile::tempdir().unwrap();
        reference_test::register_path(dir.path());
        let context = SongLuaCompileContext::new(dir.path(), "typed descriptors");

        let mut rust = RustSongRuntime::new(&context).unwrap();
        rust.vm_mut()
            .execute(&compile_named(source.as_bytes(), b"=typed descriptors").unwrap())
            .unwrap();

        assert!(rust.read_song_multitap_descs(&context).is_err());
    }
    let mut pair = Pair::new(&factory(true, 1, false), |_| {});
    pair.context.players[0].difficulty = deadsync_song_lua::SongLuaDifficulty::Easy;
    pair.exec("multitaps=setmetatable({}, {__index=function(_,k) if k=='Challenge' then error('suppressed') end end})");

    assert!(
        pair.rust
            .read_song_multitap_descs(&pair.context)
            .unwrap()
            .is_none()
    );
    pair.finish();
}
#[test]
fn numbered_factory_curves_and_missing_visuals_match() {
    let mut pair = Pair::new(&factory(true, 1, false), |_| {});
    pair.isolate_factory_plan();
    assert!(pair.gate().unwrap());
    assert!(!pair.compile().unwrap().is_empty());
    assert!(pair.hits());
    assert!(pair.rows.actors.iter().any(|r| {
        r.actor
            .message_commands
            .iter()
            .any(|c| c.message == "__songlua_tap_1_1_W1")
    }));
    pair.finish();
}
#[test]
fn double_two_player_decorations_and_existing_visuals_match() {
    let mut pair = Pair::new(&factory(false, 2, true), |context| {
        context.style_name = "double".into();
        context.players[1].enabled = true;
    });
    pair.isolate_factory_plan();
    assert!(!pair.compile().unwrap().is_empty());
    assert!(pair.hits());
    pair.finish();
}
#[test]
fn complete_plan_rejects_hidden_root_wrapper_column_and_screen_callbacks() {
    for mutation in [
        "saved.root:SetUpdateFunction(function()end)",
        "saved.arrow:AddWrapperState().__songlua_update_function=function()end",
        "SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField'):GetColumnActors()[1].__songlua_update_function=function()end",
        "SCREENMAN:GetTopScreen().__songlua_update_function=function()end",
        "saved.arrow.__songlua_recurring_update_command='AgainCommand';saved.arrow.AgainCommand=function()end",
        "saved.arrow:hibernate(2)",
        "saved.arrow.__songlua_command_queue_deferred=true",
    ] {
        let mut pair = Pair::new(&factory(true, 1, false), |_| {});
        pair.exec(mutation);
        assert!(!pair.gate().unwrap(), "{mutation}");
        assert!(pair.compile().is_none());
        pair.finish();
    }
}
#[test]
fn plan_cache_retains_traversal_until_update_function_invalidates_it() {
    let mut pair = Pair::new(&factory(true, 1, false), |_| {});
    pair.isolate_factory_plan();
    assert!(pair.gate().unwrap());
    pair.exec("local child=Def.ActorFrame{Name='extra',InitCommand=function(self)self:SetUpdateFunction(function()end)end};child:SetUpdateFunction(function()end); saved.root[#saved.root+1]=child");
    // SetUpdateFunction above invalidated the plan before the new raw child.
    assert!(!pair.gate().unwrap());
    pair.exec("saved.root[#saved.root]=nil;collectgarbage()");
    assert!(!pair.gate().unwrap());
    pair.exec("saved.update:SetUpdateFunction(function()end)");
    assert!(pair.gate().unwrap());
    pair.finish();
}
#[test]
fn malformed_plan_fields_and_optional_rotation_propagate_errors() {
    for mutation in [
        "saved.arrow.__songlua_hibernate_seconds=false",
        "saved.update.__songlua_update_function=false",
        "saved.update.Name=false",
    ] {
        let mut pair = Pair::new(&factory(true, 1, false), |_| {});
        pair.isolate_factory_plan();
        pair.exec(mutation);
        assert!(pair.gate().is_err(), "{mutation}");
        pair.finish();
    }
    let mut pair = Pair::new(&factory(true, 1, false), |_| {});
    pair.isolate_factory_plan();
    pair.exec("MYSTERIOUS_VERSION_DEPENDENT_RADIAN_POLTERGEIST=false");
    assert!(pair.compile().is_none());
    pair.finish();
}
#[test]
fn precision_collapsed_beat_edges_preserve_replay_fallback() {
    use deadsync_rules::timing::{TimingData, TimingSegments};
    let mut pair = Pair::new(&factory(true, 1, false), |context| {
        context.song_timing = Some(TimingData::from_segments(
            1.25,
            0.,
            &TimingSegments {
                bpms: vec![(0., 120.)],
                ..Default::default()
            },
            &[],
        ));
    });
    pair.isolate_factory_plan();
    assert!(pair.compile().is_none());
    assert!(pair.hits());
    pair.finish();
}
#[test]
fn hit_command_failures_remove_temporary_commands_and_preserve_actor_state() {
    let mut pair = Pair::new(&factory(true, 1, false), |_| {});
    pair.exec("saved.explosion.DimCommand=function(self)self:x(99);collectgarbage();error('hit failure')end");
    assert!(!pair.hits());
    pair.exec("assert(saved.explosion.__songlua_tap_1_1_W1Command==nil)");
    pair.finish();
}
#[test]
fn hit_command_capture_rewinds_shared_locals_globals_and_rng() {
    let mut pair = Pair::new(&factory(true, 1, false), |_| {});
    pair.exec("n=5;local n=3;saved.explosion.JudgmentCommand=function(self) n=n+1;_G.n=_G.n+1; self:x(n):y(_G.n):aux(math.random());collectgarbage()end");
    assert!(pair.hits());
    pair.exec("assert(n==5);assert(saved.explosion.__songlua_tap_1_1_W5Command==nil)");
    pair.finish();
}

#[test]
fn loaded_theme_and_truthy_deferred_fields_keep_chronological_replay() {
    let mut pair = Pair::new(&factory(true, 1, false), |_| {});
    assert!(!pair.gate().unwrap());
    assert!(pair.compile().is_none());
    pair.isolate_factory_plan();
    // A new SetUpdateFunction invalidates the cached complete-screen plan.
    pair.exec("saved.update:SetUpdateFunction(function()end);saved.arrow.__songlua_command_queue_deferred=2");
    assert!(!pair.gate().unwrap());
    pair.finish();
}

#[test]
fn update_stage_selects_analytic_curves_and_prepares_hits() {
    let mut pair = Pair::new(&factory(true, 1, false), |_| {});
    pair.isolate_factory_plan();
    let out = pair.stage();
    assert!(!out.overlay_eases.is_empty());
    assert!(out.overlay_tracks.is_empty());
    assert!(pair.rows.actors.iter().any(|r| {
        r.actor
            .message_commands
            .iter()
            .any(|c| c.message == "__songlua_tap_1_1_W1")
    }));
    pair.finish();
}

#[test]
fn update_stage_keeps_chronological_theme_and_authored_callbacks() {
    let source = factory(true, 1, false).replace(
        "self:SetUpdateFunction(function()end)",
        "self:SetUpdateFunction(function(self)self:x(GAMESTATE:GetSongBeat()*10)end)",
    );
    let mut pair = Pair::new(&source, |_| {});
    let out = pair.stage();
    assert!(!out.overlay_tracks.is_empty());
    pair.finish();
}

#[test]
fn update_stage_installs_model_clocks_from_compiled_initial_state() {
    use deadsync_song_lua::{
        SongLuaModelTextureFrame, SongLuaModelTextureUv, SongLuaOverlayModelDraw,
        SongLuaOverlayModelLayer,
    };
    let mut pair = Pair::new(&factory(true, 1, false), |_| {});
    let index = pair
        .rows
        .actors
        .iter()
        .position(|r| r.actor.name.as_deref() == Some("MultitapArrowP1_1"))
        .unwrap();
    let uv = SongLuaModelTextureUv {
        scale: [1.; 2],
        offset: [0.; 2],
        shift: [0.; 2],
    };
    let mut layer = SongLuaOverlayModelLayer::new(
        Arc::from("material fixture"),
        Arc::<[()]>::from([]),
        [2.; 2],
        [1.; 2],
        [0.; 2],
        [0.; 2],
        [1., 0.5],
        None,
        SongLuaOverlayModelDraw {
            pos: [0.; 3],
            rot: [0.; 3],
            zoom: [1.; 3],
            tint: [1.; 4],
            vert_align: 0.,
            blend_add: false,
            visible: true,
        },
    );
    layer.texture_frames = Arc::from([
        SongLuaModelTextureFrame {
            texture_key: None,
            delay: 0.02,
            diffuse: uv,
            glow: uv,
        },
        SongLuaModelTextureFrame {
            texture_key: None,
            delay: 0.03,
            diffuse: SongLuaModelTextureUv {
                offset: [0.25, 0.5],
                ..uv
            },
            glow: uv,
        },
    ]);
    let kind = SongLuaOverlayKind::Model {
        layers: Arc::from([layer]),
    };

    pair.rows.actors[index].actor.kind = kind;
    // Preparation can rewind a startup texture state independently of the
    // actor's current exported getter. Native installation uses compiled state.

    pair.rows.actors[index]
        .actor
        .initial_state
        .sprite_state_index = Some(1);
    pair.stage();

    let actor = pair
        .rust
        .song_overlay_actor_value(&pair.rows.actors[index])
        .unwrap();
    let b = pair.rust.take_actor_model_material_samples(actor).unwrap();
    reference_test::assert_debug("song/tests/rust_song_multitap.rs::material_samples", &b);
    assert!(!b[0].is_empty());
    assert_eq!(b[0][0].diffuse_state, 1);
    pair.finish();
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
