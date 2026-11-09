"""Build whole-frame comparisons from the existing native live-scene harness."""
from pathlib import Path
r=Path(__file__).resolve().parents[3]
s=(r/'crates/deadsync-song-lua/tests/rust_live_layers.rs').read_text();s=s[:s.index('const SNAPSHOT:')]
s=s.replace('SongLiveFrames, SongLiveLayers,','SongLiveScene, SongLiveSceneBindings,')
s=s.replace('    rust: RustSongRuntime,','    rust: SongLiveScene,')
a=s.index('    frames: SongLiveFrames,');b=s.index('    _directory:',a);s=s[:a]+s[b:]
s=s.replace('        rust.begin_song_compilation().unwrap();','        rust.begin_live_song_load();\n        rust.begin_song_compilation().unwrap();')
s=s.replace('        rust.initialize_song_layers(screen, &plan).unwrap().unwrap();','        let root=rust.initialize_song_layers(screen, &plan).unwrap().unwrap();')
a=s.index('        rust.activate_song_layers(');b=s.index('        let mut pair = Self {',a)
s=s[:a]+'''        let activation=rust.activate_song_layers(screen,&selected).unwrap().unwrap();
        let mut overlays=rust.read_song_overlay_actors(root,&selected,slots,layer,|_|{}).unwrap();
        let mut tracked=rust.read_song_tracked_actors(screen).unwrap().unwrap();
        let actors=overlays.actors.iter().map(|a|rust.song_overlay_actor_value(a).unwrap()).collect();
        let tracks=tracked.actors.iter().map(|a|(a.target,rust.song_tracked_actor_value(a).unwrap())).collect();
        let mut rust=SongLiveScene::new(rust,&selected,SongLiveSceneBindings{screen,root,actors,tracked:tracks,entry_paths:paths.iter().map(|p|p.to_path_buf()).collect(),primary_index:0,startup_broadcasts:activation.broadcasts}).unwrap();
        rust.runtime_mut().release_song_overlay_actors(&mut overlays).unwrap();
        rust.runtime_mut().release_song_tracked_actors(&mut tracked).unwrap();
''' +s[b:]
s=s.replace('            frames,\n            screen,\n            layers,\n            last_music: -9999.,\n            prepared: false,\n','')
s=s.replace('.rust\n            .vm_mut()', '.rust\n            .runtime_mut().vm_mut()')
a=s.index('    fn prepare(');b=s.index('\n}\nimpl Drop',a)
s=s[:a]+'''    fn prepare(&mut self,frame:&SongLuaLiveFrame){self.native.prepare_gameplay_frame(frame.clone()).unwrap();self.rust.prepare_gameplay_frame(frame.clone()).unwrap();self.compare(SNAPSHOT);}
    fn update(&mut self,frame:&SongLuaLiveFrame){
        let native=self.native.update(frame.clone()).unwrap();let rust=self.rust.update(frame.clone()).unwrap();self.output(native,rust);self.compare(SNAPSHOT);self.states();
    }
    fn draw(&mut self){let native=self.native.draw().unwrap();let rust=self.rust.draw().unwrap();self.output(native,rust);self.compare(SNAPSHOT);self.states();}
    fn states(&mut self){
        assert_eq!(self.rust.states(),self.native.states(),"overlay current render states");
        let expected:Vec<_>=self.native.tracked_states().collect();let actual:Vec<_>=self.rust.tracked_states().collect();assert_eq!(actual,expected,"tracked current render states");
    }
    fn output(&mut self,native:song::SongLuaLiveOutput,rust:song::SongLuaLiveOutput){
        assert_eq!(format!("{:?}",rust.request),format!("{:?}",native.request));
        assert_eq!(format!("{:?}",rust.source_requests),format!("{:?}",native.source_requests));
        assert_eq!(rust.broadcasts,native.broadcasts);assert_eq!(rust.sounds,native.sounds);assert_eq!(rust.player_writes,native.player_writes);
        assert_eq!(rust.errors.len(),native.errors.len(),"native {:?}; Rust {:?}",native.errors,rust.errors);
        assert_eq!(rust.coverage_gaps,native.coverage_gaps);
        let source=|v:Vec<song::SongLuaLiveBroadcast>|v.into_iter().map(|v|(v.beat,v.music_seconds,v.name,v.params)).collect::<Vec<_>>();
        assert_eq!(source(rust.source_broadcasts),source(native.source_broadcasts));
        assert_eq!(rust.draw_frames,native.draw_frames);
        assert_eq!(rust.actor_sounds.len(),native.actor_sounds.len());
    }
''' +s[b:]
a=s.index('    fn drop(&mut self)');s=s[:a]+'''    fn drop(&mut self){self.rust.end().unwrap();self.native.end().unwrap();}
}
'''
p=r/'crates/deadsync-song-lua/tests/rust_live_scene.rs';assert not p.exists();p.write_text(s)
