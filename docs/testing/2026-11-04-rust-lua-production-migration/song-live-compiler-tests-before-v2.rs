use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    self as song, SongLuaCompileContext, SongLuaLiveFrame, SongLuaLiveOutput, rust_engine as rust,
};
use std::{path::Path, sync::Arc};
type Kind = song::SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>;
type Native = song::CompiledSongLua<song::SongLuaOverlayActor<Kind>>;
type Rust = song::CompiledSongLuaData<song::SongLuaOverlayActor<Kind>, rust::SongLiveScene>;
fn slots(_: &Path, _: &Path, _: &Path) -> Result<Arc<[()]>, String> {
    Ok(Arc::from([]))
}
fn layer(_: &()) -> Option<song::SongLuaOverlayModelLayer<()>> {
    None
}
fn metadata(native: &[Native], rust: &[Rust]) {
    assert_eq!(rust.len(), native.len());
    for (i, (n, r)) in native.iter().zip(rust).enumerate() {
        assert_eq!(
            format!("{r:?}"),
            format!("{n:?}"),
            "complete compiled payload, layer {i}"
        );
    }
}
struct Pair {
    native: Option<song::SongLuaLiveScene>,
    rust: Option<rust::SongLiveScene>,
    _dir: tempfile::TempDir,
}
impl Drop for Pair {
    fn drop(&mut self) {
        if let Some(s) = &mut self.native {
            s.end().unwrap();
        }
        if let Some(s) = &mut self.rust {
            s.end().unwrap();
        }
    }
}
impl Pair {
    fn new(sources: &[&str], configure: impl FnOnce(&mut SongLuaCompileContext)) -> Self {
        let dir = tempfile::tempdir().unwrap();
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
        let mut n = song::compile_song_lua_layers_with_default_host(
            &paths,
            0,
            &context,
            Default::default(),
            slots,
            layer,
            |_, _| None,
        )
        .unwrap();
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
        metadata(&n, &r);
        let mut pair = Self {
            native: n.iter_mut().find_map(|o| o.live_scene.take()),
            rust: r.iter_mut().find_map(|o| o.live_scene.take()),
            _dir: dir,
        };
        assert!(pair.native.is_some() && pair.rust.is_some());
        pair.startup();
        pair.states();
        pair
    }
    fn run(&mut self, source: &str) {
        let n: mlua::MultiValue =
            rust::test_support::legacy_live_scene_lua(self.native.as_ref().unwrap())
                .load(source)
                .set_name("@/live-compiler-check.lua")
                .call(())
                .unwrap();
        let n = n
            .into_iter()
            .map(|v| match v {
                mlua::Value::Nil => "nil".into(),
                mlua::Value::Boolean(v) => v.to_string(),
                mlua::Value::Integer(v) => format!("{:016x}", (v as f64).to_bits()),
                mlua::Value::Number(v) => format!("{:016x}", v.to_bits()),
                mlua::Value::String(v) => format!("{:?}", v.as_bytes()),
                v => panic!("scalar: {v:?}"),
            })
            .collect::<Vec<String>>();
        let vm = self.rust.as_mut().unwrap().runtime_mut().vm_mut();
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
        assert_eq!(r, n, "{source}");
    }
    fn output(&mut self, n: SongLuaLiveOutput, r: SongLuaLiveOutput) {
        assert_eq!(format!("{:?}", r.request), format!("{:?}", n.request));
        assert_eq!(
            format!("{:?}", r.source_requests),
            format!("{:?}", n.source_requests)
        );
        assert_eq!(r.broadcasts, n.broadcasts);
        assert_eq!(r.sounds, n.sounds);
        assert_eq!(r.player_writes, n.player_writes);
        let source = |v: Vec<song::SongLuaLiveBroadcast>| {
            v.into_iter()
                .map(|v| (v.beat, v.music_seconds, v.name, v.params))
                .collect::<Vec<_>>()
        };
        assert_eq!(source(r.source_broadcasts), source(n.source_broadcasts));
        assert_eq!(r.coverage_gaps, n.coverage_gaps);
        assert_eq!(r.draw_frames, n.draw_frames);
        assert_eq!(
            r.errors.len(),
            n.errors.len(),
            "native {:?}; Rust {:?}",
            n.errors,
            r.errors
        );
        assert!(
            n.actor_sounds.is_empty() && r.actor_sounds.is_empty(),
            "sound owner checks use the dedicated live-scene suite"
        );
    }
    fn states(&self) {
        let n = self.native.as_ref().unwrap();
        let r = self.rust.as_ref().unwrap();
        assert_eq!(r.states(), n.states());
        let target = |(t, s)| {
            use song::SongLuaTrackedActorTarget as T;
            (
                match t {
                    T::Player(i) => (0, i),
                    T::PlayerJudgment(i) => (1, i),
                    T::PlayerCombo(i) => (2, i),
                    T::SongForeground => (3, 0),
                    T::ScreenLayer(i) => (4, i),
                },
                s,
            )
        };
        assert_eq!(
            r.tracked_states().map(target).collect::<Vec<_>>(),
            n.tracked_states().map(target).collect::<Vec<_>>()
        );
    }
    fn startup(&mut self) {
        let n = self.native.as_mut().unwrap().take_startup_output();
        let r = self.rust.as_mut().unwrap().take_startup_output();
        self.output(n, r);
    }
    fn update(&mut self, t: f64) {
        let f = SongLuaLiveFrame {
            beat: t * 2.,
            music_seconds: t,
            screen_seconds: t,
            delta_seconds: 0.1,
            ..Default::default()
        };
        let n = self.native.as_mut().unwrap().update(f.clone()).unwrap();
        let r = self.rust.as_mut().unwrap().update(f).unwrap();
        self.output(n, r);
        self.states();
    }
    fn reload(&mut self) {
        let mut n = song::reload_song_lua_with_default_host(
            self.native.take().unwrap(),
            slots,
            layer,
            |_, _| None,
        )
        .unwrap();
        let mut r = rust::reload_song_lua(self.rust.take().unwrap(), slots, layer).unwrap();
        metadata(&n, &r);
        self.native = n.iter_mut().find_map(|o| o.live_scene.take());
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
    p.run("assert(COUNT==2 and SAVED~=Q and SAVED:GetX()==77);collectgarbage('collect');return RANDOM,math.random(),table.concat(EVENTS,';')");
    p.update(0.2);
    p.reload();
    p.run("assert(COUNT==3);return table.concat(EVENTS,';'),math.random()");
}
#[test]
fn unsupported_startup_screen_rejection_matches() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("screen.lua");
    std::fs::write(
        &path,
        "return Def.Actor{OnCommand=function()SCREENMAN:SetNewScreen('ScreenSelectMusic')end}",
    )
    .unwrap();
    let mut c = SongLuaCompileContext::new(dir.path(), "invalid screen");
    c.live = true;
    let n = song::compile_song_lua_layers_with_default_host(
        &[&path],
        0,
        &c,
        Default::default(),
        slots,
        layer,
        |_, _| None,
    )
    .unwrap_err();
    let r =
        rust::compile_song_lua_layers(&[&path], 0, &c, Default::default(), slots, layer, |_, _| {
            None
        })
        .unwrap_err();
    assert_eq!(r, n);
}
#[test]
fn unified_offline_entry_preserves_existing_finite_output() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("offline.lua");
    std::fs::write(&path,"return Def.Quad{InitCommand=function(self)self:setsize(17,23)end,OnCommand=function(self)self:linear(0.1):x(30)end}").unwrap();
    let mut c = SongLuaCompileContext::new(dir.path(), "offline");
    c.music_length_seconds = 0.1;
    let n = song::compile_song_lua_layers_with_default_host(
        &[&path],
        0,
        &c,
        Default::default(),
        slots,
        layer,
        |_, _| None,
    )
    .unwrap();
    let r =
        rust::compile_song_lua_layers(&[&path], 0, &c, Default::default(), slots, layer, |_, _| {
            None
        })
        .unwrap();
    metadata(&n, &r);
}
