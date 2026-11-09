use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    self as song, SongLuaCompileContext, SongLuaLiveFrame, SongLuaLiveOutput, rust_engine as rust,
};
use std::{path::Path, sync::Arc};
type Kind = song::SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>;
type Rust = song::CompiledSongLuaData<song::SongLuaOverlayActor<Kind>, rust::SongLiveScene>;
fn slots(_: &Path, _: &Path, _: &Path) -> Result<Arc<[()]>, String> {
    Ok(Arc::from([]))
}
fn layer(_: &()) -> Option<song::SongLuaOverlayModelLayer<()>> {
    None
}
fn metadata(outputs: &[Rust]) {
    reference_test::assert_debug(
        "song/tests/rust_live_compiler.rs::layer_count",
        &outputs.len(),
    );
    for (index, output) in outputs.iter().enumerate() {
        reference_test::assert_debug(
            "song/tests/rust_live_compiler.rs::metadata",
            &(index, output),
        );
    }
}
struct Pair {
    rust: Option<rust::SongLiveScene>,
    _dir: tempfile::TempDir,
}
impl Drop for Pair {
    fn drop(&mut self) {
        if let Some(s) = &mut self.rust {
            s.end().unwrap();
        }
    }
}
impl Pair {
    fn new(sources: &[&str], configure: impl FnOnce(&mut SongLuaCompileContext)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        reference_test::register_path(dir.path());
        let mut wav = Vec::new();
        wav.extend(b"RIFF");
        wav.extend(38u32.to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(16000u32.to_le_bytes());
        wav.extend(32000u32.to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(2u32.to_le_bytes());
        wav.extend(0i16.to_le_bytes());
        std::fs::write(dir.path().join("beep.wav"), wav).unwrap();
        let paths = sources
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let p = dir.path().join(format!("layer{i}.lua"));
                std::fs::write(&p, s).unwrap();
                p
            })
            .collect::<Vec<_>>();
        image::RgbaImage::new(2, 2)
            .save(dir.path().join("art.png"))
            .unwrap();
        let mut context = SongLuaCompileContext::new(dir.path(), "live compiler");
        context.live = true;
        context.players[1].enabled = true;
        context.music_length_seconds = 1.;
        configure(&mut context);
        let paths = paths.iter().map(|p| p.as_path()).collect::<Vec<_>>();

        let mut r = rust::compile_song_lua_layers(
            &paths,
            0,
            &context,
            Default::default(),
            slots,
            layer,
            |_, _| None,
        )
        .unwrap();
        metadata(&r);
        let mut pair = Self {
            rust: r.iter_mut().find_map(|o| o.live_scene.take()),
            _dir: dir,
        };
        assert!(pair.rust.is_some());
        pair.startup();
        pair.states();
        pair
    }
    fn run(&mut self, source: &str) {
        let mut runtime = self.rust.as_mut().unwrap().runtime_mut();
        let vm = runtime.vm_mut();
        let values = vm
            .execute(&compile_named(source.as_bytes(), b"@/live-compiler-check.lua").unwrap())
            .unwrap()
            .to_vec();
        let r = values
            .into_iter()
            .map(|v| match v {
                Value::Nil => "nil".into(),
                Value::Boolean(v) => v.to_string(),
                Value::Number(v) => format!("{:016x}", v.to_bits()),
                Value::String(id) => format!("{:?}", vm.string_bytes(id).unwrap()),
                v => panic!("scalar: {v:?}"),
            })
            .collect::<Vec<String>>();
        reference_test::assert_debug("song/tests/rust_live_compiler.rs::observations", &r);
    }
    fn output(&mut self, output: SongLuaLiveOutput) {
        reference_test::assert_debug(
            "song/tests/rust_live_compiler.rs::output",
            &live_snapshot::output(&output),
        );
        if !output.actor_sounds.is_empty() {
            let mut runtime = self.rust.as_mut().unwrap().runtime_mut();
            let vm = runtime.vm_mut();
            let Value::Table(owners) = vm.get_global(b"SOUND_OWNERS").unwrap() else {
                panic!("owners")
            };
            let ids = (1..=vm.raw_table_len(owners).unwrap())
                .map(|index| {
                    let owner = vm
                        .raw_table_get(owners, Value::Number(index as f64))
                        .unwrap();
                    vm.object_pointer(owner).unwrap() as u64
                })
                .collect::<Vec<_>>();
            let sounds = live_snapshot::sounds(output.actor_sounds, &ids);
            reference_test::assert_debug(
                "song/tests/rust_live_compiler.rs::actor_sounds",
                &("sounds", &sounds),
            );
        }
    }
    fn states(&self) {
        let runtime = self.rust.as_ref().unwrap();
        reference_test::assert_debug(
            "song/tests/rust_live_compiler.rs::states",
            &runtime.states(),
        );
        reference_test::assert_debug(
            "song/tests/rust_live_compiler.rs::tracked",
            &live_snapshot::tracked(runtime.tracked_states()),
        );
    }
    fn startup(&mut self) {
        let r = self.rust.as_mut().unwrap().take_startup_output();
        self.output(r);
    }
    fn update(&mut self, t: f64) {
        let f = SongLuaLiveFrame {
            beat: t * 2.,
            music_seconds: t,
            screen_seconds: t,
            delta_seconds: 0.1,
            ..Default::default()
        };

        let r = self.rust.as_mut().unwrap().update(f).unwrap();
        self.output(r);
        self.states();
    }
    fn reload(&mut self) {
        let mut r = rust::reload_song_lua(self.rust.take().unwrap(), slots, layer).unwrap();
        metadata(&r);

        self.rust = r.iter_mut().find_map(|o| o.live_scene.take());
        self.startup();
        self.states();
    }
}
const SOURCE: &str = r#"
COUNT=(COUNT or 0)+1;EVENTS=EVENTS or {};local count=COUNT
return Def.ActorFrame{Name='Root',InitCommand=function(self)ROOT=self end,
OnCommand=function(self)
 EVENTS[#EVENTS+1]='on'..count
 self:SetUpdateFunction(function(self,d)EVENTS[#EVENTS+1]='update'..count;Q:addx(d)end)
end,
OffCommand=function(self)EVENTS[#EVENTS+1]='off'..count end,
EndCommand=function(self)EVENTS[#EVENTS+1]='end'..count end,
Def.Quad{Name='Q',InitCommand=function(self)Q=self;self:setsize(8,12)end}}
"#;
#[test]
fn unified_live_compilation_freezes_initial_assets_and_retains_future_callbacks() {
    let source = SOURCE
        .replace(
            "EVENTS[#EVENTS+1]='on'..count",
            "EVENTS[#EVENTS+1]='on'..count;self:linear(0.5):x(100):queuecommand('Future')",
        )
        .replace(
            "OffCommand=function(self)",
            "FutureCommand=function(self)FUTURE=true end,OffCommand=function(self)",
        );
    let mut p = Pair::new(&[&source], |_| {});
    p.run("assert(COUNT==1 and ROOT and Q and FUTURE==nil);collectgarbage('collect')");
    p.update(0.1);
    p.update(0.2);
    p.run("return ROOT:GetX(),Q:GetX(),table.concat(EVENTS,';')");
}
#[test]
fn multilayer_wrapper_proxy_aft_and_sprite_payloads_use_shared_layer_splitting() {
    let source = r#"return Def.ActorFrame{Name='Media',Def.Sprite{Name='Art',Texture='art.png',InitCommand=function(self)ART=self;self:AddWrapperState():x(3)end},Def.ActorProxy{InitCommand=function(self)self:SetTarget(ART)end},Def.ActorFrameTexture{Name='Aft',InitCommand=function(self)self:SetTextureName('fixture'):SetWidth(16):SetHeight(8):Create()end},Def.Sprite{InitCommand=function(self)self:SetTexture('fixture')end}}"#;
    let mut p = Pair::new(&[SOURCE, source], |c| {
        c.layer_start_beats = vec![0., 0.];
    });
    p.update(0.1);
}
#[test]
fn empty_live_session_keeps_the_screen_and_zero_authored_entries() {
    let mut p = Pair::new(&[], |_| {});
    p.update(0.1);
    p.run("assert(SCREENMAN:GetTopScreen());collectgarbage('collect')");
}
#[test]
fn malformed_live_file_keeps_rust_dummy_fallback_and_prior_side_effects() {
    let mut p = Pair::new(&["BEFORE=17;error('fixture failure')"], |_| {});
    p.run("assert(BEFORE==17)");
    p.update(0.1);
}
#[test]
fn startup_reloads_preserve_globals_rng_end_order_and_saved_actor_ownership() {
    let source=SOURCE.replace("EVENTS[#EVENTS+1]='on'..count","EVENTS[#EVENTS+1]='on'..count;RANDOMS=RANDOMS or {};RANDOMS[#RANDOMS+1]=math.random();SAVED=SAVED or Q;if count<3 then SCREENMAN:SetNewScreen('ScreenGameplay')end");
    let mut p = Pair::new(&[&source], |_| {});
    p.run("assert(COUNT==3 and SAVED~=Q);collectgarbage('collect');assert(SAVED:GetName()=='Q');return table.concat(EVENTS,';'),unpack(RANDOMS)");
    p.update(0.1);
}
#[test]
fn retained_scene_reload_runs_end_then_rebuilds_in_the_same_vm() {
    let mut p = Pair::new(&[SOURCE], |_| {});
    p.update(0.1);
    p.run("SAVED=Q;RANDOM=math.random();Q:x(77);collectgarbage('collect')");
    p.reload();
    p.run("assert(COUNT==2 and SAVED~=Q);collectgarbage('collect');return SAVED:GetX(),RANDOM,math.random(),table.concat(EVENTS,';')");
    p.update(0.2);
    p.reload();
    p.run("assert(COUNT==3);return table.concat(EVENTS,';'),math.random()");
}
#[test]
fn unsupported_startup_screen_rejection_matches() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    let path = dir.path().join("screen.lua");
    std::fs::write(
        &path,
        "return Def.Actor{OnCommand=function()SCREENMAN:SetNewScreen('ScreenSelectMusic')end}",
    )
    .unwrap();
    let mut c = SongLuaCompileContext::new(dir.path(), "invalid screen");
    c.live = true;

    let r =
        rust::compile_song_lua_layers(&[&path], 0, &c, Default::default(), slots, layer, |_, _| {
            None
        })
        .unwrap_err();
    reference_test::assert_debug("song/tests/rust_live_compiler.rs::compile_error", &r);
}
#[test]
fn unified_offline_entry_preserves_existing_finite_output() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    let path = dir.path().join("offline.lua");
    std::fs::write(&path,"return Def.Quad{InitCommand=function(self)self:setsize(17,23)end,OnCommand=function(self)self:linear(0.1):x(30)end}").unwrap();
    let mut c = SongLuaCompileContext::new(dir.path(), "offline");
    c.music_length_seconds = 0.1;

    let r =
        rust::compile_song_lua_layers(&[&path], 0, &c, Default::default(), slots, layer, |_, _| {
            None
        })
        .unwrap();
    metadata(&r);
}

#[test]
fn saved_scalar_getters_and_writes_revert_to_lua_backing_after_reload() {
    let source=SOURCE.replace("self:setsize(8,12)","self:setsize(8,12):xy(5,7):z(9):aux(2):zoomx(1.5):zoomy(2.5):zoomz(3.5):basezoom(4):rotationz(33):halign(1):valign(0)");
    let mut p = Pair::new(&[&source], |_| {});
    p.update(0.1);
    p.run("SAVED=Q;Q:x(77):zoomx(9):aux(8):rotationz(66)");
    p.reload();
    p.run("return SAVED:GetX(),SAVED:GetY(),SAVED:GetZ(),SAVED:GetDestX(),SAVED:GetZoomX(),SAVED:GetZoomY(),SAVED:GetZoomZ(),SAVED:getaux(),SAVED:GetBaseZoomX(),SAVED:GetZoomedWidth(),SAVED:GetRotationZ(),SAVED:GetHAlign(),SAVED:GetVAlign()");
    p.run("SAVED:addx(3):addy(4):z(11):aux(6):zoomx(2);return SAVED:GetX(),SAVED:GetY(),SAVED:GetZ(),SAVED:GetDestX(),SAVED:GetZoomX(),SAVED:getaux()");
    p.run("collectgarbage('collect');return SAVED:GetName(),SAVED:GetX()");
}

#[test]
fn replacement_init_does_not_observe_the_previous_frames_live_player_feedback() {
    let source=SOURCE.replace("ROOT=self end", "ROOT=self;INIT_LIVES=INIT_LIVES or {};INIT_LIVES[#INIT_LIVES+1]=SCREENMAN:GetTopScreen():GetLifeMeter(0):GetLife();INIT_LIVES[#INIT_LIVES+1]=STATSMAN:GetCurStageStats():GetPlayerStageStats(0):GetCurrentLife() end");
    let mut p = Pair::new(&[&source], |_| {});
    p.update(0.1);
    p.reload();
    p.run("return unpack(INIT_LIVES)");
}

#[test]
fn live_startup_reload_limit_rejects_after_the_existing_128_reconstructions() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    let path = dir.path().join("loop.lua");
    std::fs::write(
        &path,
        "return Def.Actor{OnCommand=function()SCREENMAN:SetNewScreen('ScreenGameplay')end}",
    )
    .unwrap();
    let mut c = SongLuaCompileContext::new(dir.path(), "reload bound");
    c.live = true;
    c.defer_initial_live_update = true;

    let r =
        rust::compile_song_lua_layers(&[&path], 0, &c, Default::default(), slots, layer, |_, _| {
            None
        })
        .unwrap_err();
    assert!(r.contains("128 startup gameplay reloads"));
    reference_test::assert_debug("song/tests/rust_live_compiler.rs::compile_error", &r);
}

#[test]
fn sound_owner_events_survive_startup_and_runtime_reload_in_native_order() {
    let source=SOURCE.replace("EVENTS[#EVENTS+1]='on'..count", "EVENTS[#EVENTS+1]='on'..count;if count<3 then SCREENMAN:SetNewScreen('ScreenGameplay')end").replace("Def.Quad{Name='Q'", "Def.Sound{Name='Sound',File='beep.wav',InitCommand=function(self)SOUND_OWNERS=SOUND_OWNERS or {};SOUND_OWNERS[#SOUND_OWNERS+1]=self end,OnCommand=function(self)self:play()end,EndCommand=function(self)self:stop()end},Def.Quad{Name='Q'");
    let mut p = Pair::new(&[&source], |_| {});
    p.run("assert(COUNT==3 and #SOUND_OWNERS==3);collectgarbage('collect')");
    p.update(0.1);
    p.reload();
    p.run("assert(COUNT==4 and #SOUND_OWNERS==4)");

    let r = p.rust.as_mut().unwrap().end().unwrap();
    p.output(r);
}

#[test]
fn original_unsupported_statistics_getlife_fixture_remains_a_rejection_control() {
    let mut p = Pair::new(&[SOURCE], |_| {});
    p.run("local stats=STATSMAN:GetCurStageStats():GetPlayerStageStats(0);assert(stats.GetLife==nil and type(stats.GetCurrentLife)=='function')");
}

#[test]
fn retained_rust_scene_moves_from_loading_worker_to_the_playback_thread() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    let path = dir.path().join("thread.lua");
    std::fs::write(&path, SOURCE).unwrap();
    let mut scene = std::thread::spawn(move || {
        let mut c = SongLuaCompileContext::new(path.parent().unwrap(), "thread handoff");
        c.live = true;
        let mut out = rust::compile_song_lua_layers(
            &[&path],
            0,
            &c,
            Default::default(),
            slots,
            layer,
            |_, _| None,
        )
        .unwrap();
        out[0].live_scene.take().unwrap()
    })
    .join()
    .unwrap();
    scene
        .runtime_mut()
        .vm_mut()
        .execute(
            &compile_named(
                b"collectgarbage('collect');assert(ROOT and Q)",
                b"@/thread-handoff.lua",
            )
            .unwrap(),
        )
        .unwrap();
    scene
        .update(SongLuaLiveFrame {
            delta_seconds: 0.1,
            ..Default::default()
        })
        .unwrap();
    scene.end().unwrap();
}

#[test]
fn nested_tail_loaded_actor_keeps_live_file_directory() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    std::fs::create_dir(dir.path().join("nested")).unwrap();
    let path = dir.path().join("entry.lua");
    std::fs::write(
        &path,
        "SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField'):y(10)\nreturn LoadActor('nested/tail.lua')",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("nested/tail.lua"),
        "return LoadActor('child.lua')",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("nested/child.lua"),
        "return Def.Quad{Name='relative-child',InitCommand=function(self)collectgarbage('collect');self:x(17)end}",
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(dir.path(), "nested live actor");
    context.live = true;

    let rust = rust::compile_song_lua_layers(
        &[&path],
        0,
        &context,
        Default::default(),
        slots,
        layer,
        |_, _| None,
    )
    .unwrap();
    assert!(
        rust[0]
            .overlays
            .iter()
            .any(|a| a.name.as_deref() == Some("relative-child"))
    );
    metadata(&rust);
}

#[test]
fn source_callback_scope_survives_nested_theme_gc_and_unwinds_after_error() {
    let mut pair = Pair::new(
        &[r#"return Def.Actor{
            InitCommand=function(self)ROOT=self end,
            FailCommand=function(self)
                collectgarbage('collect')
                THEME_EMIT()
                error('scope cleanup control')
            end,
        }"#],
        |_| {},
    );
    pair.run(r#"
        THEME_EMIT=assert(loadstring("MESSAGEMAN:Broadcast('NestedTheme')",'@theme:/scope-control.lua'))
        assert(not pcall(ROOT.playcommand,ROOT,'Fail'))
        THEME_EMIT()
    "#);
    let frame = SongLuaLiveFrame {
        beat: 0.2,
        music_seconds: 0.1,
        screen_seconds: 0.1,
        delta_seconds: 0.1,
        ..Default::default()
    };

    let rust = pair.rust.as_mut().unwrap().update(frame).unwrap();
    assert_eq!(
        rust.source_broadcasts
            .iter()
            .map(|event| event.name.as_str())
            .collect::<Vec<_>>(),
        ["NestedTheme"]
    );
    pair.output(rust);
}

#[test]
fn dynamic_command_source_scope_preserves_the_selected_observer_profile() {
    for enabled in [false, true] {
        let mut pair = Pair::new(
            &["return Def.Actor{InitCommand=function(self)ROOT=self end}"],
            |context| context.trace_dynamic_commands = enabled,
        );
        pair.run(r#"
            THEME_EMIT=assert(loadstring("MESSAGEMAN:Broadcast('NestedDynamicTheme')",'@theme:/dynamic-scope-control.lua'))
            ROOT:addcommand('Fail',function(self)
                collectgarbage('collect')
                THEME_EMIT()
                error('dynamic scope cleanup control')
            end)
            assert(not pcall(ROOT.playcommand,ROOT,'Fail'))
            THEME_EMIT()
        "#);
        let frame = SongLuaLiveFrame {
            beat: 0.2,
            music_seconds: 0.1,
            screen_seconds: 0.1,
            delta_seconds: 0.1,
            ..Default::default()
        };

        let rust = pair.rust.as_mut().unwrap().update(frame).unwrap();
        assert_eq!(rust.source_broadcasts.len(), usize::from(enabled));
        pair.output(rust);
    }
}

#[path = "support/live_snapshot.rs"]
mod live_snapshot;
#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
