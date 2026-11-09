use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    MultitapDesc, SongLuaCompileContext, SongLuaOverlayActor, SongLuaOverlayKind,
    SongLuaOverlayState, SongLuaNoteskinResolver,
    rust_engine::{ActorScene, RustSongRuntime, SongLayerPlan, SongOverlayActors, test_support},
};
use std::sync::Arc;

type Kind = SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>;
struct Pair {
    native: mlua::Lua,
    root: mlua::Table,
    rust: RustSongRuntime,
    rust_root: Value,
    screen: ActorScene,
    context: SongLuaCompileContext,
    native_rows: test_support::LegacySongOverlayActors<(), ()>,
    rows: SongOverlayActors<Kind>,
    _dir: tempfile::TempDir,
}
impl Pair {
    fn new(source: &str, configure: impl FnOnce(&mut SongLuaCompileContext)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("default.lua");
        std::fs::write(&path,source).unwrap();
        let mut context = SongLuaCompileContext::new(dir.path(),"multitap preparation");
        context.music_length_seconds = 0.1;
        configure(&mut context);
        let native = test_support::legacy_song_compilation_vm();
        let activation = test_support::legacy_activate_song_layers(&native,&[&path],0,&context).unwrap();
        let mut rust = RustSongRuntime::new(&context).unwrap();
        rust.begin_song_compilation().unwrap();
        let (screen,selected) = rust.construct_gameplay_screen(&context).unwrap();
        rust.load_gameplay_theme(screen,&selected).unwrap();
        let rust_root = rust.initialize_song_layers(screen,&SongLayerPlan::new(&[&path],0,&context).unwrap()).unwrap().unwrap();
        let rust_activation = rust.activate_song_layers(screen,&context).unwrap().unwrap();
        test_support::legacy_discover_song_startup_commands(&native,&activation,false).unwrap();
        rust.discover_song_startup_commands(screen,&rust_activation).unwrap();
        let native_rows = test_support::legacy_read_song_overlay_kinds::<(),()>(&native,activation.root.clone(),&context,|_,_,_|Ok(Arc::from([])),|_|None).unwrap();
        let rows = rust.read_song_overlay_actors::<(),()>(rust_root,&context,|_,_,_|Ok(Arc::from([])),|_|None,|_|{}).unwrap();
        Self {native,root:activation.root,rust,rust_root,screen,context,native_rows,rows,_dir:dir}
    }
    fn exec(&mut self,source: &str) {
        self.native.load(source).exec().unwrap();
        self.rust.vm_mut().execute(&compile_named(source.as_bytes(),b"=multitap test mutation").unwrap()).unwrap();
    }
    fn gate(&mut self) -> Result<bool,String> {
        let a = test_support::legacy_multitap_update_only(&self.native,&self.root);
        let b = self.rust.song_multitap_update_only(self.rust_root).map_err(|e|e.to_string());
        assert_eq!(a.is_err(),b.is_err(),"native {a:?}; Rust {b:?}");
        if let (Ok(a),Ok(b)) = (&a,&b) { assert_eq!(a,b); }
        b
    }
    fn compare_rows(&self) {
        assert_eq!(self.native_rows.len(),self.rows.actors.len());
        for (i,(a,b)) in self.native_rows.iter().zip(&self.rows.actors).enumerate() {
            assert_eq!(a.actor.name,b.actor.name,"name {i}");
            assert_eq!(a.actor.parent_index,b.actor.parent_index,"parent {i}");
            assert_eq!(a.actor.initial_state,b.actor.initial_state,"initial {i}");
            assert_eq!(a.actor.message_commands,b.actor.message_commands,"commands {i}");
            assert_eq!(format!("{:?}",a.actor.kind),format!("{:?}",b.actor.kind),"kind {i}");
        }
    }
    fn compile(&mut self) -> Option<Vec<deadsync_song_lua::SongLuaOverlayEase>> {
        let (mut a_calls,mut b_calls) = (Vec::new(),Vec::new());
        let a = test_support::legacy_compile_multitap(&self.native,&self.root,&self.context,&mut self.native_rows,|skin| {a_calls.push(skin.to_owned());Some(visual())});
        let b = self.rust.compile_song_multitap_update_overlays(self.rust_root,&self.context,&mut self.rows,SongLuaNoteskinResolver::default(),|skin| {b_calls.push(skin.to_owned());Some(visual())});
        assert_eq!(a.is_err(),b.is_err(),"native {a:?}; Rust {b:?}");
        assert_eq!(a_calls,b_calls,"visual loader order");
        self.compare_rows();
        match (a,b) { (Ok(a),Ok(b)) => {assert_eq!(a,b);b},(Err(_),Err(_))=>None,_=>unreachable!() }
    }
    fn hits(&mut self) -> bool {
        let a = test_support::legacy_install_multitap_hits(&self.native,&self.context,&mut self.native_rows);
        let b = self.rust.install_song_multitap_hits(&self.context,&mut self.rows);
        assert_eq!(a.is_err(),b.is_err(),"native {a:?}; Rust {b:?}");
        self.compare_rows();
        a.is_ok()
    }
    fn finish(mut self) {
        self.native.gc_collect().unwrap();
        self.rust.vm_mut().collect_garbage().unwrap();
        for row in &self.rows.actors { assert!(matches!(self.rust.song_overlay_actor_value(row).unwrap(),Value::Table(_))); }
        self.compare_rows();
        self.rust.release_song_overlay_actors(&mut self.rows).unwrap();
        self.rust.release_actor_scene(self.screen).unwrap();
        self.rust.vm_mut().collect_garbage().unwrap();
    }
}
fn visual() -> (Kind,SongLuaOverlayState) {
    (SongLuaOverlayKind::Model{layers:Vec::new()},SongLuaOverlayState{zoom_y:-1.,..Default::default()})
}
fn factory(numbered: bool, players: usize, arrow_visual: bool) -> String {
    let mut source = String::from("multitaps={Challenge={{lane=1,taps={1,2,3},peak=1.2},{lane=4,taps={3,4}}},Double_Challenge={{lane=8,taps={1,2,3},peak=1.2},{lane=4,taps={3,4}}}}; MYSTERIOUS_VERSION_DEPENDENT_RADIAN_POLTERGEIST=4; saved={}; local root=Def.ActorFrame{}; ");
    for pn in 1..=players {
        source.push_str(&format!("root[#root+1]=Def.ActorFrame{{Name='MultitapFrameP{pn}'}};"));
        source.push_str(&format!("multitap_note_callback_P{pn}=function()end;"));
        for index in 1..=2 {
            let class=if arrow_visual {"Sprite"} else {"ActorFrame"};
            let texture=if arrow_visual {",Texture='_missing.png'"} else {""};
            let deco=if numbered {format!("Def.BitmapText{{Name='MultitapTextP{pn}_{index}',Text='3'}}")} else {format!("Def.ActorFrame{{Name='MultitapDeco{pn}_{index}',Def.Quad{{Name='deco{pn}_{index}',InitCommand=function(self)self:zoom(.5)end}}}}")};
            source.push_str(&format!("root[#root+1]=Def.ActorFrame{{Name='MultitapP{pn}_{index}',InitCommand=function(self)self:y(15)end,Def.{class}{{Name='MultitapArrowP{pn}_{index}'{texture},InitCommand=function(self)saved.arrow=self;self.__songlua_noteskin_name='default' end}},{deco}}};"));
        }
        source.push_str(&format!("root[#root+1]=Def.ActorFrame{{Name='MultitapExplosionP{pn}_1',InitCommand=function(self)saved.explosion=self end,JudgmentCommand=function(self) self:x(10); collectgarbage() end,DimCommand=function(self) self:linear(.1):diffusealpha(.2) end,W1Command=function(self) self:y(11)end,W3Command=function(self)self:zoom(3)end,Def.Quad{{Name='burst{pn}',JudgmentCommand=function(self)self:z(6)end,W2Command=function(self)self:linear(.2):zoom(2)end}}}};"));
    }
    source.push_str("root[#root+1]=Def.ActorFrame{Name='Update',InitCommand=function(self)saved.update=self;self:SetUpdateFunction(function()end)end}; saved.root=root; return root");
    source
}
fn descs(rows: Option<Vec<MultitapDesc>>) -> Option<Vec<(usize,Vec<f32>,Option<f32>)>> {
    rows.map(|rows|rows.into_iter().map(|r|(r.lane,r.taps,r.peak)).collect())
}

#[test]
fn descriptors_preserve_selection_coercion_holes_and_peak_errors() {
    for double in [false,true] {
        for source in [
            "multitaps=nil",
            "multitaps={Challenge={{lane='1.8',taps={3,' 2 ',false,1,0/0,1/0},peak=' 1.4 '},false,{lane=9,taps={1}},{lane=-1,taps={1}},{lane=2,taps={false}},nil,{lane=3,taps={1}}},Double_Challenge={{lane=8,taps={2,1}}}}",
            "multitaps=setmetatable({Challenge={{lane=1,taps={2,1}}}},{__index=function(_,k)collectgarbage();return nil end})",
            "multitaps={Challenge={setmetatable({lane=1,taps={2,1}},{__index=function()error('peak suppressed')end})}}",
            "multitaps={Challenge={setmetatable({},{__index=function(_,k)collectgarbage();if k=='lane' then return 2 elseif k=='taps' then multitaps=nil;return {4,2} end end})}}",
        ] {
            let context_dir=tempfile::tempdir().unwrap();
            let mut context=SongLuaCompileContext::new(context_dir.path(),"descriptor reader");
            if double {context.style_name="double".into();}
            let native=mlua::Lua::new();native.load(source).exec().unwrap();
            let mut rust=RustSongRuntime::new(&context).unwrap();rust.vm_mut().execute(&compile_named(source.as_bytes(),b"=descriptors").unwrap()).unwrap();
            assert_eq!(descs(deadsync_song_lua::read_multitap_descs(&native,&context).unwrap()),descs(rust.read_song_multitap_descs(&context).unwrap()),"{source}, double={double}");
        }
    }
}
#[test]
fn descriptor_typed_errors_match_and_challenge_fallback_suppresses_only_fallback_errors() {
    for source in ["multitaps=false","multitaps={Challenge=false}","multitaps={Challenge={{lane=false,taps={1}}}}","multitaps={Challenge={{lane=1,taps=false}}}"] {
        let dir=tempfile::tempdir().unwrap();let context=SongLuaCompileContext::new(dir.path(),"typed descriptors");
        let native=mlua::Lua::new();native.load(source).exec().unwrap();
        let mut rust=RustSongRuntime::new(&context).unwrap();rust.vm_mut().execute(&compile_named(source.as_bytes(),b"=typed descriptors").unwrap()).unwrap();
        assert!(deadsync_song_lua::read_multitap_descs(&native,&context).is_err());assert!(rust.read_song_multitap_descs(&context).is_err());
    }
    let mut pair=Pair::new(&factory(true,1,false), |_|{});
    pair.context.players[0].difficulty=deadsync_song_lua::SongLuaDifficulty::Easy;
    pair.exec("multitaps=setmetatable({}, {__index=function(_,k) if k=='Challenge' then error('suppressed') end end})");
    assert!(deadsync_song_lua::read_multitap_descs(&pair.native,&pair.context).unwrap().is_none());
    assert!(pair.rust.read_song_multitap_descs(&pair.context).unwrap().is_none());pair.finish();
}
#[test]
fn numbered_factory_curves_and_missing_visuals_match() {
    let mut pair=Pair::new(&factory(true,1,false), |_|{});
    assert!(pair.gate().unwrap());
    assert!(!pair.compile().unwrap().is_empty());
    assert!(pair.hits());
    assert!(pair.rows.actors.iter().any(|r|r.actor.message_commands.iter().any(|c|c.message=="__songlua_tap_1_1_W1")));
    pair.finish();
}
#[test]
fn double_two_player_decorations_and_existing_visuals_match() {
    let mut pair=Pair::new(&factory(false,2,true), |context|{context.style_name="double".into();context.players[1].enabled=true;});
    assert!(!pair.compile().unwrap().is_empty());assert!(pair.hits());pair.finish();
}
#[test]
fn complete_plan_rejects_hidden_root_wrapper_column_and_screen_callbacks() {
    for mutation in [
        "saved.root:SetUpdateFunction(function()end)",
        "saved.arrow:AddWrapperState():SetUpdateFunction(function()end)",
        "SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField'):GetColumnActors()[1]:SetUpdateFunction(function()end)",
        "SCREENMAN:GetTopScreen():SetUpdateFunction(function()end)",
        "saved.arrow.__songlua_recurring_update_command='AgainCommand';saved.arrow.AgainCommand=function()end",
        "saved.arrow:hibernate(2)",
        "saved.arrow.__songlua_command_queue_deferred=true",
    ] {
        let mut pair=Pair::new(&factory(true,1,false), |_|{});pair.exec(mutation);
        assert!(!pair.gate().unwrap(),"{mutation}");assert!(pair.compile().is_none());pair.finish();
    }
}
#[test]
fn plan_cache_retains_traversal_until_update_function_invalidates_it() {
    let mut pair=Pair::new(&factory(true,1,false), |_|{});
    assert!(pair.gate().unwrap());
    pair.exec("local child=Def.ActorFrame{Name='extra',InitCommand=function(self)self:SetUpdateFunction(function()end)end};child:SetUpdateFunction(function()end); saved.root[#saved.root+1]=child");
    // SetUpdateFunction above invalidated the plan before the new raw child.
    assert!(!pair.gate().unwrap());
    pair.exec("saved.root[#saved.root]=nil;collectgarbage()");
    assert!(!pair.gate().unwrap());
    pair.exec("saved.update:SetUpdateFunction(function()end)");
    assert!(pair.gate().unwrap());pair.finish();
}
#[test]
fn malformed_plan_fields_and_optional_rotation_propagate_errors() {
    for mutation in ["saved.arrow.__songlua_command_queue_deferred=2","saved.arrow.__songlua_hibernate_seconds=false","saved.update.__songlua_update_function=false","saved.update.Name=false"] {
        let mut pair=Pair::new(&factory(true,1,false), |_|{});pair.exec(mutation);assert!(pair.gate().is_err(),"{mutation}");pair.finish();
    }
    let mut pair=Pair::new(&factory(true,1,false), |_|{});pair.exec("MYSTERIOUS_VERSION_DEPENDENT_RADIAN_POLTERGEIST=false");assert!(pair.compile().is_none());pair.finish();
}
#[test]
fn precision_collapsed_beat_edges_preserve_replay_fallback() {
    use deadsync_rules::timing::{TimingData,TimingSegments};
    let mut pair=Pair::new(&factory(true,1,false), |context| {
        context.song_timing=Some(TimingData::from_segments(1.25,0.,&TimingSegments{bpms:vec![(0.,120.)],..Default::default()},&[]));
    });
    assert!(pair.compile().is_none());assert!(pair.hits());pair.finish();
}
#[test]
fn hit_command_failures_remove_temporary_commands_and_preserve_actor_state() {
    let mut pair=Pair::new(&factory(true,1,false), |_|{});
    pair.exec("saved.explosion.DimCommand=function(self)self:x(99);collectgarbage();error('hit failure')end");
    assert!(!pair.hits());
    pair.exec("assert(saved.explosion.__songlua_tap_1_1_W1Command==nil)");pair.finish();
}
#[test]
fn hit_command_capture_rewinds_shared_locals_globals_and_rng() {
    let mut pair=Pair::new(&factory(true,1,false), |_|{});
    pair.exec("n=5;local n=3;saved.explosion.JudgmentCommand=function(self) n=n+1;_G.n=_G.n+1; self:x(n):y(_G.n):aux(math.random());collectgarbage()end");
    assert!(pair.hits());pair.exec("assert(n==5);assert(saved.explosion.__songlua_tap_1_1_W5Command==nil)");pair.finish();
}
