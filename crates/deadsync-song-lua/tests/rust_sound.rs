use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    self as song, SongLuaActorSound, SongLuaCompileContext,
    rust_engine::{ActorScene, RustSongRuntime},
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

fn wav(path: &Path, frames: u32) {
    let bytes = frames * 2;
    let mut data = Vec::new();
    data.extend(b"RIFF");
    data.extend((36 + bytes).to_le_bytes());
    data.extend(b"WAVEfmt ");
    data.extend(16u32.to_le_bytes());
    data.extend(1u16.to_le_bytes());
    data.extend(1u16.to_le_bytes());
    data.extend(16000u32.to_le_bytes());
    data.extend(32000u32.to_le_bytes());
    data.extend(2u16.to_le_bytes());
    data.extend(16u16.to_le_bytes());
    data.extend(b"data");
    data.extend(bytes.to_le_bytes());
    data.resize(data.len() + bytes as usize, 0);
    std::fs::write(path, data).unwrap();
}
struct Files {
    _temp: tempfile::TempDir,
    dir: String,
    a: PathBuf,
    b: PathBuf,
}
impl Files {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        reference_test::register_path(temp.path());
        let a = temp.path().join("choice a.wav");
        let b = temp.path().join("choice B.wav");
        wav(&a, 1975);
        wav(&b, 7311);
        Self {
            dir: song::file_path_string(temp.path()) + "/",
            _temp: temp,
            a,
            b,
        }
    }
    fn source(&self) -> String {
        format!(
            r#"
            directory={dir:?}; first={a:?}; second={b:?}
            return Def.ActorFrame{{
                Def.Sound{{Name='a',File=first,_Dir=directory}},
                Def.Sound{{Name='b',File=first,_Dir=directory}}
            }}
        "#,
            dir = self.dir,
            a = song::file_path_string(&self.a),
            b = song::file_path_string(&self.b)
        )
    }
}
fn execute(runtime: &mut RustSongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/sounds.lua").unwrap())
        .unwrap()
        .to_vec()
}
struct Pair {
    rust: RustSongRuntime,
    scene: ActorScene,
    rust_ids: HashMap<u64, String>,
    seconds: f32,
}
fn events(events: Vec<SongLuaActorSound>, ids: &HashMap<u64, String>) -> Vec<String> {
    events
        .into_iter()
        .map(|event| match event {
            SongLuaActorSound::Play { owner, path } => {
                format!("{} play {}", ids[&owner], path.display())
            }
            SongLuaActorSound::Pause { owner, paused } => format!("{} pause {paused}", ids[&owner]),
            SongLuaActorSound::Stop { owner } => format!("{} stop", ids[&owner]),
        })
        .collect()
}
impl Pair {
    fn new(files: &Files, live: bool) -> Self {
        let source = format!(
            r#"
            observations={{}}
            function log(...) for i=1,select('#',...) do
                local v=select(i,...)
                observations[#observations+1]=type(v)=='number' and string.format('%.17g',v) or tostring(v)
            end end
            {}
        "#,
            files.source()
        );
        let mut context = SongLuaCompileContext::new(&files.dir, "sound migration");
        context.live = live;
        let mut rust = RustSongRuntime::new(&context).unwrap();
        let definition = execute(&mut rust, &source)[0];
        let scene = rust.instantiate_actor(definition).unwrap();
        let root = rust.actor_scene_root(scene).unwrap();
        rust.vm_mut().set_global(b"root", root).unwrap();
        execute(&mut rust, "a=root[1]; b=root[2]");
        let mut rust_ids = HashMap::new();
        for name in ["root", "a", "b"] {
            let actor = rust.vm_mut().get_global(name.as_bytes()).unwrap();
            rust_ids.insert(
                rust.vm_mut().object_pointer(actor).unwrap() as u64,
                name.to_owned(),
            );
        }
        Self {
            rust,
            scene,
            rust_ids,
            seconds: 0.0,
        }
    }
    fn compare(&mut self, phase: &str) -> Vec<String> {
        let Value::String(actual) =
            execute(&mut self.rust, "return table.concat(observations,'|')")[0]
        else {
            panic!("string")
        };
        let actual = String::from_utf8_lossy(self.rust.vm_mut().string_bytes(actual).unwrap());
        reference_test::assert_source_debug(
            "song/tests/rust_sound.rs::compare::observations",
            phase,
            &actual,
        );
        let actual = events(self.rust.take_actor_sounds(), &self.rust_ids);
        reference_test::assert_source_debug(
            "song/tests/rust_sound.rs::compare::immediate",
            phase,
            &actual,
        );
        reference_test::assert_source_debug(
            "song/tests/rust_sound.rs::compare::prepared",
            phase,
            &self.rust.take_prepared_sounds(),
        );
        actual
    }
    fn script(&mut self, source: &str) -> Vec<String> {
        execute(&mut self.rust, source);
        self.compare(source)
    }
    fn advance(&mut self, delta: f64) -> Vec<String> {
        self.seconds += delta as f32;
        self.rust
            .set_values(self.seconds * 2.0, self.seconds)
            .unwrap();
        self.rust.advance_actor_scene(self.scene, delta).unwrap();
        self.compare("advance")
    }
    fn capture(&mut self, queued: bool) {
        self.rust.begin_actor_capture(queued);
    }
}

#[test]
fn sound_handles_parameters_and_borrowed_methods_match_production() {
    let files = Files::new();
    let mut pair = Pair::new(&files, true);
    pair.script(
        r#"
        h=a:get(); other=b:get(); assert(h==a:get() and h~=other)
        assert(h.__songlua_volume==1 and h.__songlua_pitch==1)
        assert(h:volume(-2.5):pitch(' 1.25 ')==h)
        log(h.__songlua_volume,h.__songlua_pitch,other.__songlua_volume,other.__songlua_pitch)
        local volume=h.volume; assert(volume(3.5)==h and volume({},4.5)==h)
        local get=a.get; assert(get()==h and get(b)==h)
        local load=a.Load; assert(load(second)==a and a.File==second and b.File==first)
        assert(load(false)==a and a.File==second)
        for _,value in ipairs{false,{},'bad','0x2',0/0,1/0,-1/0,1e300} do
            assert(not pcall(h.volume,h,value)); assert(not pcall(h.pitch,h,value))
        end
        h:pitch('NaN'):volume('inf'); log(h.__songlua_pitch,h.__songlua_volume)
        assert(h:SetProperty('Speed',2)==false)
        log(h:get_length(),other:get_length())
        assert(a:Load()==a and a.File==nil); log(h:get_length())
        assert(a:Load(first)==a); log(h:get_length())
        collectgarbage('collect'); assert(a:get()==h)
    "#,
    );
}

#[test]
fn immediate_sound_order_is_independent_of_tweens_and_keeps_actor_identity() {
    let files = Files::new();
    let mut pair = Pair::new(&files, true);
    let emitted=pair.script(r#"
        a:sleep(.5):play(); b:playforplayer(2); a:Load(second):play():pause(true):pause('off'):stop()
        a.GoCommand=function(self) self:play(); b:play(); collectgarbage('collect') end
        a:queuecommand('Go')
    "#);
    assert_eq!(emitted.len(), 6);
    assert!(pair.advance(0.5).is_empty());
    assert_eq!(pair.advance(0.125).len(), 2);
    pair.script(r#"
        for _,value in ipairs{true,false,0,1,'yes','no','on','off','true','false','1','0','bad',{},0/0} do a:pause(value) end
        a:pause(); local stop=a.stop; assert(stop()==a)
        Actor.play=function(self) log('override',self:GetName()); return self end
        assert(a:play()==a)
        Actor.Load=function(self,path) log('class load',self:GetName(),path); return self end
        a:Load('class hook')
    "#);
}

#[test]
fn sound_paths_prefix_selection_and_metadata_cache_match_production() {
    let files = Files::new();
    let mut pair = Pair::new(&files, true);
    pair.script(
        r#"
        a:Load('choice'); a:play(); log(a:get():get_length())
        a:Load('CHOICE'); a:play(); log(a:get():get_length())
        a:Load('choice B.wav'); a:play(); log(a:get():get_length())
        a:Load('missing.wav'):play(); log(a:get():get_length())
        a.__songlua_script_dir=nil; a.__songlua_song_dir=directory
        a:Load('choice a.wav'):play(); log(a:get():get_length())
        a:Load(first):play(); log(a:get():get_length())
        a:Load(''):play(); log(a:get():get_length())
        a:Load(false); assert(a.File=='')
    "#,
    );
}

#[test]
fn sound_length_cache_uses_file_text_and_retains_failed_prefix_probes() {
    let files = Files::new();
    let mut pair = Pair::new(&files, true);
    pair.script("log(a:get():get_length()); a:Load('later'); log(a:get():get_length()); b:Load('missing.wav'); log(b:get():get_length())");
    wav(&files._temp.path().join("later.wav"), 16000);
    wav(&files._temp.path().join("missing.wav"), 8000);
    pair.script("log(a:get():get_length(),b:get():get_length()); a:Load('LATER'):play(); log(a:get():get_length()); a:Load('later.wav'):play(); log(a:get():get_length()); b:Load(second); log(b:get():get_length()); b:Load('missing.wav'); log(b:get():get_length())");
    pair.script("a:Load(first); log(a:get():get_length())");
    std::fs::remove_file(&files.a).unwrap();
    pair.script("a:play(); log(a:get():get_length())");
    wav(&files.a, 32000);
    pair.script("a:play(); log(a:get():get_length()); a:Load(second); log(a:get():get_length()); a:Load(first); log(a:get():get_length())");
}

#[test]
fn sound_file_getters_reenter_collect_and_preserve_error_recovery() {
    let files = Files::new();
    let mut pair = Pair::new(&files, true);
    pair.script(r#"
        h=a:get(); a.File=nil
        local mt=getmetatable(a); local lookup=mt.__index
        mt.__index=function(self,key)
            if key=='File' then
                rawset(self,'File',second); log('nested',h:get_length()); rawset(self,'File',nil)
                collectgarbage('collect'); return first
            end
            return lookup(self,key)
        end
        log('outer',h:get_length()); a:play(); mt.__index=lookup
        a.File=false; assert(not pcall(h.get_length,h)); assert(not pcall(a.play,a))
        a.File=string.char(255); assert(not pcall(h.get_length,h)); assert(not pcall(a.play,a))
        a.File='choice a.wav'; a.__songlua_script_dir=nil
        mt.__index=function(self,key) if key=='__songlua_script_dir' then collectgarbage('collect'); error('path fault') end; return lookup(self,key) end
        log(h:get_length()); assert(pcall(a.play,a)); mt.__index=lookup
        a.__songlua_script_dir=directory; a:Load(second); log(h:get_length()); a:play()
    "#);
}

#[test]
fn preparation_capture_preserves_beat_message_scope_and_startup_phase() {
    let files = Files::new();
    let mut pair = Pair::new(&files, false);
    pair.capture(false);
    pair.advance(0.25);
    pair.script(r#"
        __songlua_sound_calls={}
        a:sleep(5):play():pause(true):stop()
        root.InnerMessageCommand=function(self) b:Load(second):play(); collectgarbage('collect') end
        root.OuterMessageCommand=function(self) a:play(); MESSAGEMAN:Broadcast('Inner'); a:play() end
        MESSAGEMAN:Broadcast('Outer'); a:play()
        log(table.concat(__songlua_sound_calls,'|'))
    "#);
    pair.capture(true);
    pair.script("a:Load(second):playforplayer(1); MESSAGEMAN:Broadcast('Outer')");

    pair.rust.end_actor_capture();
    pair.script("a:play(); log(table.concat(__songlua_sound_calls,'|'))");
}

#[test]
fn preparation_errors_retain_emitted_events_and_restore_broadcast_scope() {
    let files = Files::new();
    let mut pair = Pair::new(&files, false);
    pair.capture(false);
    pair.script(
        r#"
        __songlua_sound_calls=false
        root.FaultMessageCommand=function(self) a:play() end
        assert(not pcall(MESSAGEMAN.Broadcast,MESSAGEMAN,'Fault'))
        __songlua_sound_calls={}; a:play()
        __songlua_compile_song_runtime.__songlua_music_beat=nil
        a:play(); log(table.concat(__songlua_sound_calls,'|'))
    "#,
    );
}

#[test]
fn retiring_scene_stops_sound_owners_and_retained_handles_keep_metadata() {
    let files = Files::new();
    let mut pair = Pair::new(&files, true);
    pair.script("h=a:get(); h:volume(.25):pitch(1.5); a:play(); b:play()");
    assert!(pair.rust.release_actor_scene(pair.scene).unwrap());
    assert_eq!(
        events(pair.rust.take_actor_sounds(), &pair.rust_ids),
        ["a stop", "b stop"]
    );
    execute(
        &mut pair.rust,
        r#"
        collectgarbage('collect'); assert(h:volume(.5)==h and h.__songlua_pitch==1.5)
        assert(h:get_length()>0 and h:SetProperty('x',1)==false)
        assert(not pcall(a.play,a) and not pcall(a.get,a))
        a=nil; b=nil; root=nil; collectgarbage('collect'); assert(h:get_length()>0)
    "#,
    );
    assert!(!pair.rust.release_actor_scene(pair.scene).unwrap());
    assert!(pair.rust.take_actor_sounds().is_empty());
}

#[test]
fn end_callbacks_precede_owner_stops_without_advancing_queued_work() {
    let files = Files::new();
    let mut pair = Pair::new(&files, true);
    pair.script(
        r#"
        __songlua_top_screen=root
        root.EndCommand=function() error('synthetic root must not receive End') end
        local wrapper=root:AddWrapperState()
        wrapper.EndCommand=function() error('wrapper End') end
        a.LaterCommand=function() error('queued command ran during End') end
        a:sleep(3):queuecommand('Later')
        a.EndCommand=function(self,...)
            log('end a',select('#',...),__songlua_top_screen==nil)
            self:Load(second):play():sleep(1):queuecommand('Later')
            collectgarbage('collect')
        end
        b.EndCommand=function(self,...)
            log('end b',select('#',...),__songlua_top_screen==nil)
            self:pause(true):play()
        end
    "#,
    );
    assert!(pair.rust.end_actor_scene(pair.scene).unwrap());
    reference_test::assert_debug(
        "song/tests/rust_sound.rs::end",
        &events(pair.rust.take_actor_sounds(), &pair.rust_ids),
    );
    pair.compare("End callbacks");
    pair.script("log(a:GetTweenTimeLeft()); h=a:get(); assert(h:get_length()>0)");
    assert!(!pair.rust.end_actor_scene(pair.scene).unwrap());
    assert!(pair.rust.advance_actor_scene(pair.scene, 10.0).is_err());
    assert!(pair.rust.release_actor_scene(pair.scene).unwrap());
    assert!(!pair.rust.end_actor_scene(pair.scene).unwrap());
    assert!(
        pair.rust.take_actor_sounds().is_empty(),
        "release must not repeat owner stops"
    );
}

#[test]
fn failed_end_keeps_partial_effects_and_is_not_retried() {
    let files = Files::new();
    let mut pair = Pair::new(&files, true);
    pair.script(
        r#"
        __songlua_top_screen=root
        a.EndCommand=function(self)
            log('end fault',__songlua_top_screen==nil)
            self:play(); collectgarbage('collect'); error('end fault')
        end
        b.EndCommand=function() error('later End must not run') end
    "#,
    );
    assert!(pair.rust.end_actor_scene(pair.scene).is_err());
    assert_eq!(pair.compare("failed End").len(), 1);
    assert!(!pair.rust.end_actor_scene(pair.scene).unwrap());
    assert!(pair.rust.advance_actor_scene(pair.scene, 0.0).is_err());
    assert!(pair.rust.take_actor_sounds().is_empty());
    assert!(pair.rust.release_actor_scene(pair.scene).unwrap());
    assert_eq!(
        events(pair.rust.take_actor_sounds(), &pair.rust_ids),
        ["a stop", "b stop"]
    );
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
