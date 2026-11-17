#![cfg(feature = "test-support")]

use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    ActorScene, SongLuaCompileContext, SongLuaOverlayState, SongRuntime, test_support,
};

fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/live-sprite-scrolling.lua").unwrap())
        .unwrap()
        .to_vec()
}

fn scene(size: [u32; 2], definition: &str) -> (tempfile::TempDir, SongRuntime, ActorScene, Value) {
    let directory = tempfile::tempdir().unwrap();
    let texture = directory.path().join("texture.png");
    image::RgbaImage::from_pixel(size[0], size[1], image::Rgba([255; 4]))
        .save(&texture)
        .unwrap();
    let mut context = SongLuaCompileContext::new(directory.path(), "live sprite scrolling");
    context.live = true;
    let mut runtime = SongRuntime::new(&context).unwrap();
    let source = format!(
        "texture={:?}; return {definition}",
        deadsync_song_lua::file_path_string(&texture)
    );
    let definition = execute(&mut runtime, &source)[0];
    let scene = runtime.instantiate_actor(definition).unwrap();
    let root = runtime.actor_scene_root(scene).unwrap();
    runtime.vm_mut().set_global(b"root", root).unwrap();
    assert!(runtime.start_actor_scene(scene).unwrap());
    runtime.begin_actor_playback();
    (directory, runtime, scene, root)
}

fn state(runtime: &mut SongRuntime, actor: Value) -> SongLuaOverlayState {
    let state = test_support::read_live_actor_state(runtime, actor).unwrap();
    assert_eq!(state.texcoord_velocity, None);
    assert!(!state.sprite_animate);
    state
}

fn rect(actual: SongLuaOverlayState, expected: [f32; 4]) {
    let actual = actual
        .custom_texture_rect
        .expect("live Sprite exports its active coordinates");
    for (a, b) in actual.into_iter().zip(expected) {
        assert!((a - b).abs() < 0.000002, "{actual:?} != {expected:?}");
    }
}

fn finish(mut runtime: SongRuntime, scene: ActorScene) {
    assert!(runtime.take_actor_diagnostics().is_empty());
    runtime.end_actor_scene(scene).unwrap();
    runtime.release_actor_scene(scene).unwrap();
}

#[test]
fn scrolling_accumulates_and_wraps_in_backing_texture_units() {
    let (_directory, mut runtime, scene, root) = scene([24, 12], "Def.Sprite{Texture=texture}");
    // Native allocation is 32x16. Wrapping happens before converting those
    // coordinates to the image UVs used by the presentation layer.
    execute(
        &mut runtime,
        "root:customtexturerect(1.25,-.5,2.75,2):texturewrapping(false):texcoordvelocity(.125,-.25)",
    );
    runtime.advance_actor_scene(scene, 0.5).unwrap();
    let first = state(&mut runtime, root);
    assert!(first.texture_wrapping);
    rect(first, [1.75, -5.0 / 6.0, 3.75, 2.5]);
    runtime.advance_actor_scene(scene, 0.5).unwrap();
    rect(state(&mut runtime, root), [0.5, 1.0 / 3.0, 2.5, 11.0 / 3.0]);
    let paused = state(&mut runtime, root);
    execute(&mut runtime, "root:animate(false)");
    runtime.set_values(500.0, 250.0).unwrap();
    runtime.advance_actor_scene(scene, 3.0).unwrap();
    assert_eq!(
        state(&mut runtime, root).custom_texture_rect,
        paused.custom_texture_rect
    );
    execute(
        &mut runtime,
        "root:animate(true):texcoordvelocity(0,0):SetCustomImageRect(0,0,1,1):addimagecoords(3,3)",
    );
    rect(state(&mut runtime, root), [0.125, 0.25, 1.125, 1.25]);
    execute(&mut runtime, "root:stretchtexcoords(.125,.125)");
    rect(
        state(&mut runtime, root),
        [7.0 / 24.0, 5.0 / 12.0, 31.0 / 24.0, 17.0 / 12.0],
    );
    finish(runtime, scene);
}

#[test]
fn scrolling_selects_wall_or_effect_delta_and_keeps_snapshot_reads_pure() {
    let (_directory, mut runtime, scene, root) = scene(
        [16, 16],
        "Def.ActorFrame{Def.Sprite{Texture=texture},Def.Sprite{Texture=texture}}",
    );
    let actors = execute(
        &mut runtime,
        "a=root[1]; b=root[2]; a:effectclock('music'):texcoordvelocity(.125,0); b:effectclock('music'):texcoordvelocity(.125,0):set_use_effect_clock_for_texcoords(true); assert(not a:get_use_effect_clock_for_texcoords() and b:get_use_effect_clock_for_texcoords()); return a,b",
    );
    runtime.set_values(8.0, 4.0).unwrap();
    runtime.advance_actor_scene(scene, 0.25).unwrap();
    rect(state(&mut runtime, actors[0]), [0.03125, 0.0, 1.03125, 1.0]);
    rect(state(&mut runtime, actors[1]), [0.5, 0.0, 1.5, 1.0]);
    runtime.advance_actor_scene(scene, 0.25).unwrap();
    rect(state(&mut runtime, actors[0]), [0.0625, 0.0, 1.0625, 1.0]);
    rect(state(&mut runtime, actors[1]), [0.5, 0.0, 1.5, 1.0]);
    // Changing the public song clock without Update cannot move either sprite.
    runtime.set_values(200.0, 100.0).unwrap();
    rect(state(&mut runtime, actors[1]), [0.5, 0.0, 1.5, 1.0]);
    let _ = test_support::read_live_actor_state(&mut runtime, root).unwrap();
    runtime.set_values(6.0, 3.0).unwrap();
    runtime.advance_actor_scene(scene, 0.0).unwrap();
    rect(state(&mut runtime, actors[1]), [0.375, 0.0, 1.375, 1.0]);
    finish(runtime, scene);
}

#[test]
fn sprite_hibernation_retains_effect_delta_but_uv_scrolling_uses_incoming_delta() {
    let (_directory, mut runtime, scene, root) = scene(
        [16, 16],
        "Def.Sprite{Texture=texture,Frames={{Frame=0,Delay=.125},{Frame=0,Delay=.125}}}",
    );
    execute(&mut runtime, "root:texcoordvelocity(1,0)");
    runtime.advance_actor_scene(scene, 0.0625).unwrap();
    execute(
        &mut runtime,
        "assert(root:GetState()==0); root:hibernate(1)",
    );
    runtime.advance_actor_scene(scene, 0.125).unwrap();
    execute(&mut runtime, "assert(root:GetState()==1)");
    // Sprite::Update continues after its Actor base returns from hibernation.
    rect(state(&mut runtime, root), [0.1875, 0.0, 1.1875, 1.0]);
    finish(runtime, scene);
}
