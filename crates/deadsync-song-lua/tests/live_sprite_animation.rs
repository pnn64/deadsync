#![cfg(feature = "test-support")]

use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    ActorScene, SongLuaCompileContext, SongLuaDrawOp, SongLuaDrawSource, SongLuaOverlayState,
    SongRuntime, test_support,
};

struct Scene {
    runtime: SongRuntime,
    scene: ActorScene,
    root: Value,
    sprite: Value,
    seconds: f32,
    _directory: tempfile::TempDir,
}

impl Scene {
    fn new(frames: &str) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let texture = directory.path().join("sheet 2x1.png");
        image::RgbaImage::from_pixel(32, 16, image::Rgba([255; 4]))
            .save(&texture)
            .unwrap();
        let mut context = SongLuaCompileContext::new(directory.path(), "live sprite animation");
        context.live = true;
        let mut runtime = SongRuntime::new(&context).unwrap();
        let source = format!(
            "FINISHED=0; return Def.ActorFrame{{Def.Sprite{{Texture={:?}, Frames={frames}, \
             AnimationFinishedCommand=function(self) FINISHED=FINISHED+1 end}}}}",
            deadsync_song_lua::file_path_string(&texture),
        );
        let definition = Self::execute(&mut runtime, &source)[0];
        let scene = runtime.instantiate_actor(definition).unwrap();
        let root = runtime.actor_scene_root(scene).unwrap();
        runtime.vm_mut().set_global(b"root", root).unwrap();
        let sprite = Self::execute(&mut runtime, "sprite=root[1]; return sprite")[0];
        assert!(runtime.start_actor_scene(scene).unwrap());
        runtime.begin_actor_playback();
        Self {
            runtime,
            scene,
            root,
            sprite,
            seconds: 0.0,
            _directory: directory,
        }
    }

    fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
        runtime
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"@/live-sprite-animation.lua").unwrap())
            .unwrap()
            .to_vec()
    }

    fn script(&mut self, source: &str) {
        Self::execute(&mut self.runtime, source);
    }

    fn advance(&mut self, delta: f64) {
        self.seconds += delta as f32;
        self.runtime
            .set_values(self.seconds * 2.0, self.seconds)
            .unwrap();
        assert!(self.runtime.advance_actor_scene(self.scene, delta).unwrap());
    }

    fn state(&mut self, expected: u32) -> SongLuaOverlayState {
        assert_eq!(
            Self::execute(&mut self.runtime, "return sprite:GetState()")[0],
            Value::Number(f64::from(expected)),
        );
        let state = test_support::read_live_actor_state(&mut self.runtime, self.sprite).unwrap();
        assert_eq!(state.sprite_state_index, Some(expected));
        assert!(
            !state.sprite_animate,
            "the renderer must not animate a frozen live state again"
        );
        assert_eq!(state.texcoord_velocity, None);
        state
    }

    fn finish(mut self) {
        assert!(self.runtime.take_actor_diagnostics().is_empty());
        self.runtime.end_actor_scene(self.scene).unwrap();
        self.runtime.release_actor_scene(self.scene).unwrap();
    }
}

const FRAMES: &str = "{{Frame=1,Delay=.125},{Frame=0,Delay=.25},{Frame=1,Delay=.5}}";

#[test]
fn live_custom_frames_keep_logical_indices_and_individual_delays() {
    let mut scene = Scene::new(FRAMES);
    scene.script("assert(sprite:GetNumStates()==3 and sprite:GetAnimationLengthSeconds()==.875)");
    assert_eq!(
        scene.state(0).custom_texture_rect,
        Some([0.5, 0.0, 1.0, 1.0])
    );
    for (delta, expected) in [(0.125, 1), (0.125, 1), (0.125, 2), (0.5, 0)] {
        scene.advance(delta);
        let rect = if expected == 1 {
            [0.0, 0.0, 0.5, 1.0]
        } else {
            [0.5, 0.0, 1.0, 1.0]
        };
        assert_eq!(scene.state(expected).custom_texture_rect, Some(rect));
    }
    scene.script("assert(FINISHED==1)");
    scene.finish();

    // A custom sequence can contain one logical state on a multi-frame sheet.
    let mut single = Scene::new("{{Frame=1,Delay=.125}}");
    single.script("assert(sprite:GetNumStates()==1 and sprite:GetAnimationLengthSeconds()==.125)");
    single.advance(4.0);
    assert_eq!(
        single.state(0).custom_texture_rect,
        Some([0.5, 0.0, 1.0, 1.0])
    );
    single.script("assert(FINISHED==0)");
    single.finish();
}

#[test]
fn live_setstate_resets_elapsed_without_replacing_custom_coordinates() {
    let mut scene = Scene::new(FRAMES);
    scene.advance(0.1875);
    scene.state(1);
    scene.script("sprite:customtexturerect(.125,.25,.875,.75); assert(sprite:GetState()==1); sprite:setstate(1)");
    let rect = Some([0.125, 0.25, 0.875, 0.75]);
    assert_eq!(scene.state(1).custom_texture_rect, rect);
    scene.advance(0.1875);
    assert_eq!(scene.state(1).custom_texture_rect, rect);
    scene.advance(0.0625);
    assert_eq!(scene.state(2).custom_texture_rect, rect);
    scene.finish();
}

#[test]
fn live_state_properties_reset_phase_and_delay_changes_preserve_it() {
    let mut scene = Scene::new(FRAMES);
    scene.advance(0.1875);
    scene.state(1);
    scene.script("sprite:SetStateProperties{{Frame=1,Delay=.125},{Delay=.25},{Frame=1,Delay=-1}}; assert(sprite:GetNumStates()==3 and sprite:GetAnimationLengthSeconds()==-.625)");
    assert_eq!(
        scene.state(0).custom_texture_rect,
        Some([0.5, 0.0, 1.0, 1.0])
    );
    // The old state's .0625 remainder is discarded with the old state list.
    scene.advance(0.0625);
    scene.state(0);
    scene.advance(0.0625);
    // Omitted Frame starts at zero independently of the preceding entry.
    assert_eq!(
        scene.state(1).custom_texture_rect,
        Some([0.0, 0.0, 0.5, 1.0])
    );
    scene.advance(0.25);
    scene.state(2);
    scene.advance(1.0);
    scene.state(2);
    scene.script("assert(FINISHED==0)");

    let custom = Some([0.125, 0.25, 0.875, 0.75]);
    scene.script("sprite:customtexturerect(.125,.25,.875,.75):SetStateProperties{{Frame=1,Delay=.25},{Delay=.5},{Frame=1,Delay=.75}}");
    assert_eq!(scene.state(0).custom_texture_rect, custom);
    scene.advance(0.375);
    assert_eq!(scene.state(1).custom_texture_rect, custom);
    scene.script(
        "sprite:SetAllStateDelays(.1875); assert(sprite:GetAnimationLengthSeconds()==.5625)",
    );
    assert_eq!(scene.state(1).custom_texture_rect, custom);
    // The current frame keeps its .125 elapsed; the new delay needs only .0625.
    scene.advance(0.0625);
    assert_eq!(scene.state(2).custom_texture_rect, custom);
    scene.advance(0.1875);
    assert_eq!(scene.state(0).custom_texture_rect, custom);
    scene.advance(0.1875);
    assert_eq!(scene.state(1).custom_texture_rect, custom);
    scene.script("assert(FINISHED==1)");
    scene.finish();
}

#[test]
fn live_animation_pause_rate_and_loop_follow_native_frame_timing() {
    let mut scene = Scene::new(FRAMES);
    scene.advance(0.0625);
    scene.script("sprite:animate(false)");
    scene.advance(2.0);
    scene.state(0);
    scene.script("sprite:animate(true):rate(8):loop(false)");
    scene.advance(0.0625);
    scene.state(1);
    scene.advance(0.25);
    scene.state(2);
    // Texture/movie rate and looping flags do not change Sprite's state clock.
    scene.advance(0.5);
    scene.state(0);
    scene.script("assert(FINISHED==1)");
    scene.finish();
}

#[test]
fn live_sprite_animation_uses_parent_update_rate_and_hibernation() {
    let mut scene = Scene::new(FRAMES);
    scene.script("root:SetUpdateRate(2)");
    scene.advance(0.0625);
    scene.state(1);
    scene.script("root:hibernate(.25)");
    scene.advance(0.125);
    scene.state(1);
    // Only .0625 remains after the parent's sleep; its rate doubles that span.
    scene.advance(0.1875);
    scene.state(1);
    scene.advance(0.0625);
    scene.state(2);
    scene.script("root:SetUpdateRate(.5); sprite:setstate(0)");
    scene.advance(0.125);
    scene.state(0);
    scene.advance(0.125);
    scene.state(1);
    scene.finish();
}

#[test]
fn animation_finished_can_reset_state_and_explicit_draw_freezes_the_result() {
    let mut scene = Scene::new("{{Frame=1,Delay=.125},{Frame=0,Delay=.125}}");
    scene.script("sprite.AnimationFinishedCommand=function(self) FINISHED=FINISHED+1; self:setstate(1):animate(false) end");
    scene.advance(0.375);
    let expected = scene.state(1);
    scene.script("assert(FINISHED==1); root:Draw()");
    let frames = scene
        .runtime
        .take_actor_draw_frames(&[
            (scene.root, SongLuaDrawSource::Overlay(0)),
            (scene.sprite, SongLuaDrawSource::Overlay(1)),
        ])
        .unwrap();
    assert_eq!(frames.len(), 1);
    assert!(frames[0].1.immediate);
    assert_eq!(frames[0].1.ops.len(), 1);
    let SongLuaDrawOp::Draw { state, .. } = &frames[0].1.ops.get(0).unwrap() else {
        panic!("explicit Draw must retain the selected Sprite state");
    };
    assert_eq!(state.sprite_state_index, expected.sprite_state_index);
    assert_eq!(state.custom_texture_rect, expected.custom_texture_rect);
    assert!(!state.sprite_animate);
    assert_eq!(state.texcoord_velocity, None);
    scene.advance(1.0);
    scene.state(1);
    scene.script("assert(FINISHED==1); sprite:animate(true)");
    scene.advance(0.0625);
    scene.state(1);
    scene.script("assert(FINISHED==1)");
    scene.advance(0.0625);
    scene.state(1);
    scene.script("assert(FINISHED==2)");
    assert_eq!(state.sprite_state_index, Some(1));
    scene.finish();
}
