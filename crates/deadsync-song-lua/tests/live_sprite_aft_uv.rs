#![cfg(feature = "test-support")]

use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, SongLuaDrawOp, SongLuaDrawSource, SongLuaOverlayState, SongRuntime,
    test_support,
};

fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/live-sprite-aft-uv.lua").unwrap())
        .unwrap_or_else(|error| panic!("{source}\n{error:?}"))
        .to_vec()
}

fn rect(state: SongLuaOverlayState, expected: [f32; 4]) {
    assert!(state.sprite_texture);
    assert!(!state.sprite_animate);
    assert_eq!(state.texcoord_velocity, None);
    let actual = state
        .custom_texture_rect
        .expect("retained active rectangle");
    for (a, b) in actual.into_iter().zip(expected) {
        assert!((a - b).abs() < 0.000002, "{actual:?} != {expected:?}");
    }
}

#[test]
fn live_aft_sprite_keeps_backing_coordinates_and_retained_viewport() {
    let directory = tempfile::tempdir().unwrap();
    for [width, height] in [[300u32, 190], [3, 5]] {
        let mut context = SongLuaCompileContext::new(directory.path(), "live AFT Sprite UV");
        context.live = true;
        let mut runtime = SongRuntime::new(&context).unwrap();
        let definition = execute(
            &mut runtime,
            "return Def.ActorFrame{Def.ActorFrameTexture{}, Def.Sprite{Frames={{Frame=7,Delay=.125}}}}",
        )[0];
        let scene = runtime.instantiate_actor(definition).unwrap();
        let root = runtime.actor_scene_root(scene).unwrap();
        runtime.vm_mut().set_global(b"root", root).unwrap();
        assert!(runtime.start_actor_scene(scene).unwrap());
        runtime.begin_actor_playback();
        let sprite = execute(
            &mut runtime,
            "aft=root[1]; sprite=root[2]; assert(sprite:GetNumStates()==0 and sprite:GetAnimationLengthSeconds()==0); return sprite",
        )[0];
        let untextured = test_support::read_live_actor_state(&mut runtime, sprite).unwrap();
        assert!(!untextured.sprite_texture);
        assert_eq!(untextured.custom_texture_rect, None);

        execute(
            &mut runtime,
            &format!(
                "aft:SetWidth({width}.9):SetHeight({height}.8):Create(); saved=aft:GetTexture(); \
                 aft:SetWidth(999):SetHeight(777); sprite:SetTexture(saved); \
                 assert(sprite:GetNumStates()==1 and sprite:GetWidth()=={width} and sprite:GetHeight()=={height})",
            ),
        );
        let allocation = [
            width.next_power_of_two() as f32,
            height.next_power_of_two() as f32,
        ];
        let default = [
            0.0,
            0.0,
            width as f32 / allocation[0],
            height as f32 / allocation[1],
        ];
        rect(
            test_support::read_live_actor_state(&mut runtime, sprite).unwrap(),
            default,
        );

        // Custom texture coordinates already name the AFT's backing texture.
        execute(&mut runtime, "sprite:customtexturerect(.125,.25,.75,.875)");
        rect(
            test_support::read_live_actor_state(&mut runtime, sprite).unwrap(),
            [0.125, 0.25, 0.75, 0.875],
        );
        execute(
            &mut runtime,
            "sprite:SetCustomImageRect(0,0,1,1):addimagecoords(1,2)",
        );
        let translated = [
            1.0 / allocation[0],
            2.0 / allocation[1],
            default[2] + 1.0 / allocation[0],
            default[3] + 2.0 / allocation[1],
        ];
        rect(
            test_support::read_live_actor_state(&mut runtime, sprite).unwrap(),
            translated,
        );

        // Draw captures use the same frozen coordinate convention as live reads.
        execute(&mut runtime, "sprite:Draw()");
        let frames = runtime
            .take_actor_draw_frames(&[(sprite, SongLuaDrawSource::Overlay(0))])
            .unwrap();
        let SongLuaDrawOp::Draw { state, .. } = frames[0].1.ops[0] else {
            panic!("expected explicit AFT sprite draw");
        };
        rect(state, translated);
        execute(
            &mut runtime,
            "sprite:SetCustomImageRect(0,0,1,1):texcoordvelocity(.125,-.25)",
        );
        runtime.advance_actor_scene(scene, 0.5).unwrap();
        rect(
            test_support::read_live_actor_state(&mut runtime, sprite).unwrap(),
            [0.0625, 0.875, default[2] + 0.0625, default[3] + 0.875],
        );

        assert!(runtime.take_actor_diagnostics().is_empty());
        runtime.end_actor_scene(scene).unwrap();
        runtime.release_actor_scene(scene).unwrap();
    }
}

#[test]
fn first_settexture_on_an_empty_sprite_uses_the_image_allocation() {
    let directory = tempfile::tempdir().unwrap();
    let texture = directory.path().join("texture.png");
    image::RgbaImage::from_pixel(24, 12, image::Rgba([255; 4]))
        .save(&texture)
        .unwrap();
    let mut context = SongLuaCompileContext::new(directory.path(), "first Sprite texture UV");
    context.live = true;
    let mut runtime = SongRuntime::new(&context).unwrap();
    let definition = execute(
        &mut runtime,
        &format!(
            "return Def.ActorFrame{{Def.Sprite{{}}, Def.Sprite{{Texture={:?}}}}}",
            deadsync_song_lua::file_path_string(&texture)
        ),
    )[0];
    let scene = runtime.instantiate_actor(definition).unwrap();
    let root = runtime.actor_scene_root(scene).unwrap();
    runtime.vm_mut().set_global(b"root", root).unwrap();
    assert!(runtime.start_actor_scene(scene).unwrap());
    runtime.begin_actor_playback();
    let sprite = execute(
        &mut runtime,
        "sprite=root[1]; assert(sprite:GetNumStates()==0); sprite:SetTexture(root[2]:GetTexture()); assert(sprite:GetNumStates()==1); return sprite",
    )[0];
    // Private UVs cover 24/32 by 12/16, then the image renderer receives 0..1.
    rect(
        test_support::read_live_actor_state(&mut runtime, sprite).unwrap(),
        [0.0, 0.0, 1.0, 1.0],
    );
    assert!(runtime.take_actor_diagnostics().is_empty());
    runtime.end_actor_scene(scene).unwrap();
    runtime.release_actor_scene(scene).unwrap();
}
