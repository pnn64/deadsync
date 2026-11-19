#![cfg(feature = "test-support")]

use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, SongLuaDrawOp, SongLuaDrawSource, SongRuntime, test_support,
};

fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/sprite-texture-wrapping.lua").unwrap())
        .unwrap()
        .to_vec()
}

#[test]
fn custom_texture_coordinates_enable_wrapping_immediately_and_capture_it() {
    let directory = tempfile::tempdir().unwrap();
    let texture = directory.path().join("texture.png");
    image::RgbaImage::from_pixel(16, 16, image::Rgba([255; 4]))
        .save(&texture)
        .unwrap();
    for live in [false, true] {
        let mut context = SongLuaCompileContext::new(directory.path(), "custom texture wrapping");
        context.live = live;
        let mut runtime = SongRuntime::new(&context).unwrap();
        if !live {
            assert!(runtime.begin_actor_preparation().unwrap());
        }
        let definition = execute(
            &mut runtime,
            &format!(
                "return Def.Sprite{{Texture={:?}}}",
                deadsync_song_lua::file_path_string(&texture),
            ),
        )[0];
        let scene = runtime.instantiate_actor(definition).unwrap();
        let root = runtime.actor_scene_root(scene).unwrap();
        runtime.vm_mut().set_global(b"root", root).unwrap();
        if live {
            assert!(runtime.start_actor_scene(scene).unwrap());
            runtime.begin_actor_playback();
        }
        for command in [
            "customtexturerect(.1,.2,.8,.9)",
            "SetCustomImageRect(0,0,1,1)",
            "stretchtexcoords(.1,-.2)",
            "addimagecoords(2,3)",
            "CropTo(8,12)",
        ] {
            execute(
                &mut runtime,
                &format!("root:stoptweening():linear(2):x(100):texturewrapping(false):{command}",),
            );
            assert!(
                runtime.actor_scene_snapshot(scene).unwrap()[0]
                    .render
                    .texture_wrapping,
                "{command}, live={live}"
            );
            assert!(
                test_support::read_live_actor_state(&mut runtime, root)
                    .unwrap()
                    .texture_wrapping,
                "{command}, live={live}"
            );
            if live {
                execute(&mut runtime, "root:Draw()");
                let frames = runtime
                    .take_actor_draw_frames(&[(root, SongLuaDrawSource::Overlay(0))])
                    .unwrap();
                let SongLuaDrawOp::Draw { state, .. } = frames[0].1.ops[0] else {
                    panic!("expected explicit sprite draw");
                };
                assert!(state.texture_wrapping, "{command}");
            } else {
                let capture = runtime.actor_command_capture(root).unwrap();
                assert_eq!(
                    capture
                        .blocks
                        .iter()
                        .filter_map(|block| block.delta.texture_wrapping)
                        .last(),
                    Some(true),
                    "{command}"
                );
            }
            // Setting the animation state does not undo wrapping; an explicit
            // subsequent texturewrapping call still controls the sampler.
            execute(&mut runtime, "root:setstate(0)");
            assert!(
                test_support::read_live_actor_state(&mut runtime, root)
                    .unwrap()
                    .texture_wrapping
            );
            execute(&mut runtime, "root:texturewrapping(false)");
            assert!(
                !test_support::read_live_actor_state(&mut runtime, root)
                    .unwrap()
                    .texture_wrapping
            );
        }
        assert!(runtime.take_actor_diagnostics().is_empty());
        runtime.end_actor_scene(scene).unwrap();
        runtime.release_actor_scene(scene).unwrap();
    }
}
