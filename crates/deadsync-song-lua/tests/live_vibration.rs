#![cfg(feature = "test-support")]

use deadlib_lua::compile_named;
use deadsync_song_lua::{SongLuaCompileContext, SongLuaDrawOp, SongLuaDrawSource, SongRuntime};

#[test]
fn live_vibration_uses_destination_x_zoom_including_one() {
    let mut context = SongLuaCompileContext::new(std::path::Path::new("."), "live vibration zoom");
    context.live = true;
    let mut runtime = SongRuntime::new(&context).unwrap();
    let definition = compile_named(b"return Def.Quad{}", b"@/vibration-definition.lua").unwrap();
    let actor = runtime.vm_mut().execute(&definition).unwrap()[0];
    let scene = runtime.instantiate_actor(actor).unwrap();
    let root = runtime.actor_scene_root(scene).unwrap();
    runtime.vm_mut().set_global(b"root", root).unwrap();
    assert!(runtime.start_actor_scene(scene).unwrap());
    runtime.begin_actor_playback();
    let commands = compile_named(
        br#"
        root:zoom(2):basezoom(3):vibrate():effectmagnitude(2,3,4)
        root:linear(2):zoomx(1)
        assert(root:GetZoom()==1 and root:GetZoomX()==1)
        "#,
        b"@/vibration-commands.lua",
    )
    .unwrap();
    runtime.vm_mut().execute(&commands).unwrap();
    assert!(runtime.advance_actor_scene(scene, 0.5).unwrap());
    let current = runtime.actor_scene_snapshot(scene).unwrap()[0].zoom[0];
    assert_eq!(current, 1.75);
    let state = deadsync_song_lua::test_support::read_live_actor_state(&mut runtime, root).unwrap();
    assert!(state.vibrate);
    assert_eq!(state.vibrate_zoom, Some(1.0));
    assert_ne!(state.vibrate_zoom, Some(current));
    assert_eq!(state.basezoom, 1.0);
    assert_eq!([state.basezoom_x, state.basezoom_y, state.basezoom_z], [3.0; 3]);
    assert_eq!(deadsync_song_lua::overlay_state_axis_scale(state), [5.25, 6.0]);
    let draw = compile_named(b"root:Draw()", b"@/vibration-draw.lua").unwrap();
    runtime.vm_mut().execute(&draw).unwrap();
    let frames = runtime
        .take_actor_draw_frames(&[(root, SongLuaDrawSource::Overlay(0))])
        .unwrap();
    assert_eq!(frames.len(), 1);
    assert!(frames[0].1.immediate);
    let SongLuaDrawOp::Draw { state, .. } = frames[0].1.ops[0] else {
        panic!("explicit Draw must retain its live actor state");
    };
    assert!(state.vibrate);
    assert_eq!(state.vibrate_zoom, Some(1.0));
    assert!(runtime.take_actor_diagnostics().is_empty());
    runtime.end_actor_scene(scene).unwrap();
    runtime.release_actor_scene(scene).unwrap();
}

#[test]
fn live_scale_exports_preserve_unit_axes_and_explicit_draw_parents() {
    use deadsync_song_lua::playback::actor_conformance::overlay_sprite_matrix;
    use deadsync_song_lua::{overlay_state_axis_scale, overlay_state_z_scale};

    let mut context = SongLuaCompileContext::new(std::path::Path::new("."), "live axis scales");
    context.live = true;
    let mut runtime = SongRuntime::new(&context).unwrap();
    let definition = compile_named(
        b"return Def.ActorFrame{Def.Quad{}}", b"@/live-scale-definition.lua",
    ).unwrap();
    let actor = runtime.vm_mut().execute(&definition).unwrap()[0];
    let scene = runtime.instantiate_actor(actor).unwrap();
    let root = runtime.actor_scene_root(scene).unwrap();
    runtime.vm_mut().set_global(b"root", root).unwrap();
    assert!(runtime.start_actor_scene(scene).unwrap());
    runtime.begin_actor_playback();
    let commands = compile_named(br#"
        root:zoom(4):zoomz(1):basezoom(3):basezoomx(1)
        root[1]:zoom(2):zoomx(1):zoomy(1):zoomz(1)
            :basezoom(5):basezoomx(1):basezoomy(1):basezoomz(1)
        assert(root:GetZoom()==4 and root:GetZoomX()==4
            and root:GetZoomY()==4 and root:GetZoomZ()==1)
        assert(root[1]:GetZoom()==1 and root[1]:GetZoomX()==1
            and root[1]:GetZoomY()==1 and root[1]:GetZoomZ()==1)
        return root[1]
    "#, b"@/live-scale-commands.lua").unwrap();
    let child = runtime.vm_mut().execute(&commands).unwrap()[0];
    let parent = deadsync_song_lua::test_support::read_live_actor_state(&mut runtime, root).unwrap();
    let leaf = deadsync_song_lua::test_support::read_live_actor_state(&mut runtime, child).unwrap();
    assert_eq!((parent.zoom, parent.basezoom, leaf.zoom, leaf.basezoom), (1., 1., 1., 1.));
    assert_eq!(overlay_state_axis_scale(parent), [4., 12.]);
    assert_eq!(overlay_state_z_scale(parent), 3.);
    assert_eq!(overlay_state_axis_scale(leaf), [1., 1.]);
    assert_eq!(overlay_state_z_scale(leaf), 1.);
    assert_eq!(overlay_sprite_matrix(parent, [2., 2.]), [
        [4., 0., 0., 0.], [0., 12., 0., 0.], [0., 0., 3., 0.], [0., 0., 0., 1.],
    ]);
    let identity = [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.], [0., 0., 0., 1.]];
    assert_eq!(overlay_sprite_matrix(leaf, [2., 2.]), identity);
    let draw = compile_named(br#"
        root:Draw()
        assert(root:GetZoom()==4 and root:GetZoomZ()==1)
        assert(root[1]:GetZoom()==1 and root[1]:GetZoomZ()==1)
    "#, b"@/live-scale-draw.lua").unwrap();
    runtime.vm_mut().execute(&draw).unwrap();
    let frames = runtime.take_actor_draw_frames(&[
        (root, SongLuaDrawSource::Overlay(0)), (child, SongLuaDrawSource::Overlay(1)),
    ]).unwrap();
    assert_eq!(frames.len(), 1);
    assert!(frames[0].1.immediate);
    assert_eq!(frames[0].1.ops.len(), 1);
    let SongLuaDrawOp::Draw { state, parents, .. } = &frames[0].1.ops[0] else {
        panic!("explicit Draw must retain the child and parent states");
    };
    assert_eq!(parents.len(), 1);
    assert_eq!(overlay_sprite_matrix(parents[0], [2., 2.]), overlay_sprite_matrix(parent, [2., 2.]));
    assert_eq!(overlay_sprite_matrix(*state, [2., 2.]), identity);
    assert!(runtime.take_actor_diagnostics().is_empty());
    runtime.end_actor_scene(scene).unwrap();
    runtime.release_actor_scene(scene).unwrap();
}
