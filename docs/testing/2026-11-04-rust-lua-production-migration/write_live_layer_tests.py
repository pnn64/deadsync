"""Create a layer-phase harness using the actual complete native live scene."""
from pathlib import Path
r=Path(__file__).resolve().parents[3]
s=(r/'crates/deadsync-song-lua/tests/rust_live_actors.rs').read_text();s=s[:s.index('// This compares the actual player animation')]
s=s.replace('SongLiveFrames, test_support','SongLiveFrames, SongLiveLayers, test_support')
s=s.replace('    screen: song::rust_engine::ActorScene,','    screen: song::rust_engine::ActorScene,\n    layers: SongLiveLayers,\n    last_music: f32,\n    prepared: bool,')
a=s.index('    fn new(');b=s.index('        context.live = true;',a)
s=s[:a]+'''    fn new(sources: &[&str], configure: impl FnOnce(&mut SongLuaCompileContext)) -> Self {
        let directory=tempfile::tempdir().unwrap();
        let paths: Vec<_>=sources.iter().enumerate().map(|(i,s)|{let p=directory.path().join(format!("layer-{i}.lua"));std::fs::write(&p,s).unwrap();p}).collect();
        let paths:Vec<_>=paths.iter().map(|p|p.as_path()).collect();
        let mut context=SongLuaCompileContext::new(directory.path(),"live layers");
''' +s[b:]
s=s.replace('&[&path]','&paths')
s=s.replace('        rust.begin_actor_playback();','        let layers=rust.new_song_live_layers(screen,&selected).unwrap().unwrap();\n        rust.begin_actor_playback();')
s=s.replace('            screen,\n            _directory:', '            screen,\n            layers,\n            last_music: -9999.,\n            prepared: false,\n            _directory:')
a=s.index('    fn prepare(');b=s.index('\n}\nimpl Drop',a)
s=s[:a]+'''    fn prepare(&mut self, frame: &SongLuaLiveFrame) {
        self.native.prepare_gameplay_frame(frame.clone()).unwrap();
        if !self.prepared {
            self.frames.prepare_gameplay_frame(&mut self.rust,frame,None,false).unwrap();
            let beat=self.rust.values().unwrap()[0];
            self.layers.prepare_backgrounds(&mut self.rust,beat as f64,frame.music_seconds,self.last_music).unwrap();
            self.prepared=true;
        }
        self.compare(SNAPSHOT);
    }
    fn rust_update(&mut self,frame:&SongLuaLiveFrame)->Result<(),deadlib_lua::Error>{
        let beat=self.frames.update(&mut self.rust,frame,None,false)?;
        if !self.prepared {self.layers.prepare_backgrounds(&mut self.rust,beat,frame.music_seconds,self.last_music)?;}
        self.prepared=false;
        self.layers.advance(&mut self.rust,beat,frame.music_seconds,self.last_music,frame.delta_seconds)?;
        self.last_music=frame.music_seconds as f32;
        self.frames.finish_health_update(&mut self.rust)
    }
    fn update(&mut self,frame:&SongLuaLiveFrame){
        self.native.update(frame.clone()).unwrap();self.rust_update(frame).unwrap();self.compare(SNAPSHOT);self.render();
    }
    fn render(&mut self){
        let snapshots=self.rust.actor_scene_snapshot(self.screen).unwrap();
        let lua=test_support::legacy_live_scene_lua(&self.native);
        let actors:mlua::Table=lua.globals().get("ACTORS").unwrap();
        for pair in actors.pairs::<String,mlua::Table>() {
            let (name,actor)=pair.unwrap();lua.globals().set("root",actor).unwrap();
            let expected=test_support::legacy_actor_render_states(lua).unwrap();
            let actual:Vec<_>=snapshots.iter().filter(|a|a.name==name.as_bytes()||a.name==format!("{name}_leaf").as_bytes()).collect();
            assert_eq!(actual.len(),expected.len(),"render tree {name}");
            for (a,e) in actual.into_iter().zip(expected){assert_eq!(a.render,e,"rendered {name}");}
        }
    }
''' +s[b:]
s=s.replace('        self.frames.release(&mut self.rust).unwrap();','        self.layers.release(&mut self.rust).unwrap();\n        self.frames.release(&mut self.rust).unwrap();')
p=r/'crates/deadsync-song-lua/tests/rust_live_layers.rs';assert not p.exists();p.write_text(s)
