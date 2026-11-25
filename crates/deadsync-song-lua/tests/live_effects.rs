#![cfg(feature = "test-support")]

use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::playback::actor_conformance::effect_sample;
use deadsync_song_lua::{
    ActorScene, SongLuaCompileContext, SongLuaDrawOp, SongLuaDrawSource, SongLuaOverlayState,
    SongRuntime,
};

fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/live-effects.lua").unwrap())
        .unwrap()
        .to_vec()
}

fn scene() -> (SongRuntime, ActorScene, Value, Value) {
    let mut context = SongLuaCompileContext::new(".", "live effect snapshots");
    context.live = true;
    let mut runtime = SongRuntime::new(&context).unwrap();
    let definition = execute(&mut runtime, "return Def.ActorFrame{Def.Quad{}}")[0];
    let scene = runtime.instantiate_actor(definition).unwrap();
    let root = runtime.actor_scene_root(scene).unwrap();
    runtime.vm_mut().set_global(b"root", root).unwrap();
    let child = execute(&mut runtime, "return root[1]")[0];
    assert!(runtime.start_actor_scene(scene).unwrap());
    runtime.begin_actor_playback();
    execute(
        &mut runtime,
        "root:bob():effectmagnitude(0,1.5,0):effectperiod(.5):effectoffset(-.25); \
         root[1]:diffuseshift():effectperiod(.5):effectcolor1(1,1,1,.5):effectcolor2(1,1,1,.75)",
    );
    (runtime, scene, root, child)
}

fn state(runtime: &mut SongRuntime, actor: Value) -> SongLuaOverlayState {
    deadsync_song_lua::test_support::read_live_actor_state(runtime, actor).unwrap()
}

fn close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.00001,
        "{actual} != {expected}"
    );
}

#[test]
fn live_effect_snapshots_preserve_actor_timer_phase() {
    let (mut runtime, scene, root, child) = scene();
    runtime.set_values(100.25, 12.03125).unwrap();
    assert!(runtime.advance_actor_scene(scene, 0.125).unwrap());
    let parent = state(&mut runtime, root);
    let glow = state(&mut runtime, child);
    assert_eq!(parent.effect_time, Some([12.03125, 0.125]));
    assert_eq!(glow.effect_time, Some([12.03125, 0.125]));
    // Actor.cpp's quarter-period Bob is at its negative extremum after the
    // authored -quarter-second offset; DiffuseShift is halfway between colors.
    close(effect_sample(parent, 12.03125, 100.25).position[1], -1.5);
    close(effect_sample(glow, 12.03125, 100.25).tint[3], 0.625);

    execute(
        &mut runtime,
        "root:SetUpdateRate(2); root[1]:hibernate(.25)",
    );
    runtime.set_values(102.75, 12.5625).unwrap();
    assert!(runtime.advance_actor_scene(scene, 0.0625).unwrap());
    assert_eq!(state(&mut runtime, root).effect_time, Some([12.5625, 0.25]));
    assert_eq!(
        state(&mut runtime, child).effect_time,
        Some([12.5625, 0.125])
    );
    close(
        effect_sample(state(&mut runtime, root), 12.5625, 102.75).position[1],
        0.0,
    );

    // A discontinuity in music time with no actor update cannot move a timer.
    runtime.set_values(56.0, 7.0).unwrap();
    assert!(runtime.advance_actor_scene(scene, 0.0).unwrap());
    assert_eq!(state(&mut runtime, root).effect_time, Some([7.0, 0.25]));
    execute(&mut runtime, "root:stopeffect():bob():effectperiod(.5)");
    assert_eq!(state(&mut runtime, root).effect_time, Some([7.0, 0.0]));
    assert!(runtime.take_actor_diagnostics().is_empty());
    runtime.end_actor_scene(scene).unwrap();
    runtime.release_actor_scene(scene).unwrap();
}

#[test]
fn explicit_draw_preserves_parent_and_child_effect_clocks() {
    let (mut runtime, scene, root, child) = scene();
    runtime.set_values(100.25, 12.03125).unwrap();
    assert!(runtime.advance_actor_scene(scene, 0.125).unwrap());
    let parent = state(&mut runtime, root);
    let glow = state(&mut runtime, child);
    execute(&mut runtime, "root:Draw()");
    let frames = runtime
        .take_actor_draw_frames(&[
            (root, SongLuaDrawSource::Overlay(0)),
            (child, SongLuaDrawSource::Overlay(1)),
        ])
        .unwrap();
    assert_eq!(frames.len(), 1);
    assert!(frames[0].1.immediate);
    assert_eq!(frames[0].1.ops.len(), 1);
    let SongLuaDrawOp::Draw { state, parents, .. } = &frames[0].1.ops.get(0).unwrap() else {
        panic!("the child Quad must retain its draw state");
    };
    assert_eq!(state.effect_time, glow.effect_time);
    assert_eq!(parents.len(), 1);
    assert_eq!(parents[0].effect_time, parent.effect_time);
    close(
        effect_sample(parents[0], 12.03125, 100.25).position[1],
        -1.5,
    );
    close(effect_sample(*state, 12.03125, 100.25).tint[3], 0.625);

    // A beat-clock snapshot records the beat axis without reusing music time.
    execute(&mut runtime, "root[1]:effectclock('beat')");
    runtime.set_values(100.375, 12.09375).unwrap();
    assert!(runtime.advance_actor_scene(scene, 0.0).unwrap());
    let current =
        deadsync_song_lua::test_support::read_live_actor_state(&mut runtime, child).unwrap();
    assert_eq!(current.effect_time, Some([100.375, 100.375]));
    assert_eq!(state.effect_time, Some([12.03125, 0.125]));
    assert!(runtime.take_actor_diagnostics().is_empty());
    runtime.end_actor_scene(scene).unwrap();
    runtime.release_actor_scene(scene).unwrap();
}
