#![cfg(feature = "test-support")]

use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{ActorScene, SongLuaCompileContext, SongRuntime, test_support};
use serde_json::Value as Json;

struct Movie {
    runtime: SongRuntime,
    scene: ActorScene,
    sprite: Value,
    _directory: tempfile::TempDir,
}

impl Movie {
    fn new(case: &Json) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut context = SongLuaCompileContext::new(directory.path(), "native movie clock");
        context.live = true;
        let mut runtime = SongRuntime::new(&context).unwrap();
        // This fixture observes native Sprite calls, so no decoder or video
        // pixels are needed. The movie extension selects the retained clock.
        let definition = Self::execute(
            &mut runtime,
            "return Def.ActorFrame{Def.Sprite{Texture='clock.mpg'}}",
        )[0];
        let scene = runtime.instantiate_actor(definition).unwrap();
        let root = runtime.actor_scene_root(scene).unwrap();
        runtime.vm_mut().set_global(b"root", root).unwrap();
        let sprite = Self::execute(&mut runtime, "sprite=root[1]; return sprite")[0];
        assert!(runtime.start_actor_scene(scene).unwrap());
        runtime.begin_actor_playback();
        let mut movie = Self {
            runtime,
            scene,
            sprite,
            _directory: directory,
        };
        if let Some(value) = case.get("animate") {
            movie.script(&format!("sprite:animate({value})"));
        }
        if let Some(clock) = case["clock"].as_str() {
            movie.script(&format!("sprite:effectclock({clock:?})"));
        }
        movie
    }

    fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
        runtime
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"@/movie-clock.lua").unwrap())
            .unwrap()
            .to_vec()
    }

    fn script(&mut self, source: &str) {
        Self::execute(&mut self.runtime, source);
    }

    fn update(&mut self, update: &Json, index: usize) {
        for (field, receiver, method) in [
            ("animate", "sprite", "animate"),
            ("decode", "sprite", "SetDecodeMovie"),
            ("visible", "sprite", "visible"),
            ("hibernate", "sprite", "hibernate"),
            ("parent_rate", "root", "SetUpdateRate"),
            ("parent_hibernate", "root", "hibernate"),
            ("seek", "sprite", "SetSecondsIntoAnimation"),
            ("texture_seek", "sprite:GetTexture()", "position"),
            ("texture_rate", "sprite:GetTexture()", "rate"),
            ("texture_looping", "sprite:GetTexture()", "loop"),
        ] {
            if let Some(value) = update.get(field) {
                self.script(&format!("{receiver}:{method}({value})"));
            }
        }
        if let Some(queued) = update.get("queued_animate") {
            let after = queued["after"].as_f64().unwrap_or(0.0);
            let enabled = queued["value"].as_bool().unwrap();
            self.script(&format!(
                "sprite:addcommand('ClockToggle{index}', function(self) self:animate({enabled}) end); \
                 sprite:sleep({after}):queuecommand('ClockToggle{index}')",
            ));
        }
        let seconds = update
            .get("music_seconds")
            .unwrap_or(&update["elapsed"])
            .as_f64()
            .unwrap() as f32;
        self.runtime.set_values(seconds * 2.0, seconds).unwrap();
        assert!(
            self.runtime
                .advance_actor_scene(self.scene, update["delta"].as_f64().unwrap())
                .unwrap()
        );
    }

    fn finish(mut self) {
        assert!(self.runtime.take_actor_diagnostics().is_empty());
        self.runtime.end_actor_scene(self.scene).unwrap();
        self.runtime.release_actor_scene(self.scene).unwrap();
    }
}

#[test]
fn live_movie_clock_matches_native_sprite_updates() {
    let native: Json = serde_json::from_str(include_str!(
        "../../../tests/fixtures/itgmania-song-lua-micro/sprite-movie-clock-native.json",
    ))
    .unwrap();
    assert_eq!(native["oracle"], "itgmania_native_sprite_movie_clock");
    for case in native["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut movie = Movie::new(case);
        let mut position = 0.0_f32;
        let mut rate = 1.0_f32;
        let mut looping = true;
        for (index, update) in case["updates"].as_array().unwrap().iter().enumerate() {
            movie.update(update, index);
            // Preserve native call order. Rate belongs to the movie texture;
            // Sprite passes its raw, nonnegative effect delta to UpdateMovie.
            for event in update["events"].as_array().unwrap() {
                match event["method"].as_str().unwrap() {
                    "update" => position += event["value"].as_f64().unwrap() as f32 * rate,
                    // The recorder observes Sprite's requested seek. Native
                    // MovieTexture_Generic ignores nonzero SetPosition calls.
                    "seek" => {
                        if event["value"].as_f64().unwrap() == 0.0 {
                            position = 0.0;
                        }
                    }
                    "rate" => rate = event["value"].as_f64().unwrap() as f32,
                    "loop" => looping = event["value"].as_bool().unwrap(),
                    other => panic!("unexpected native movie call {other}"),
                }
            }
            let state =
                test_support::read_live_actor_state(&mut movie.runtime, movie.sprite).unwrap();
            assert!(
                (state.movie_position_seconds - position).abs() < 0.00001,
                "{name} update {index}: DeadSync {} native {position}",
                state.movie_position_seconds,
            );
            assert_eq!(state.sprite_playback_rate, rate, "{name} update {index}");
            assert_eq!(state.sprite_loop, looping, "{name} update {index}");
        }
        movie.finish();
    }
}
