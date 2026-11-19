use deadlib_lua::compile_named;
use deadsync_song_lua::{
    SongLiveScene, SongLuaCompileContext, SongLuaLiveFrame, SongLuaOverlayKind,
    SongLuaOverlayState, compile_song_lua,
};
use std::{collections::BTreeMap, path::Path, sync::Arc};

type Kind = SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>;

const SOURCE: &str = r#"
return Def.ActorFrame{
    Name='Container',
    InitCommand=function(self) PARENT=self end,
    OnCommand=function(self)
        self:SetUpdateFunction(function() OBSERVED=ANIM:GetState() end)
    end,
    Def.Sprite{
        Name='Animated', Texture=texture_sheet,
        Frames={{Frame=0,Delay=.125},{Frame=1,Delay=.125}},
        InitCommand=function(self) ANIM=self end,
        ResetCommand=function(self) RESET=true; self:setstate(0) end,
    },
    Def.Sprite{
        Name='Scrolling', Texture=texture_image,
        InitCommand=function(self) SCROLL=self end,
        OnCommand=function(self) self:texcoordvelocity(.25,.125) end,
    },
}
"#;

struct Scene {
    live: SongLiveScene,
    indices: BTreeMap<String, usize>,
    elapsed: f64,
    _directory: tempfile::TempDir,
}

impl Scene {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let sheet = directory.path().join("sheet 2x1.png");
        let image = directory.path().join("image.png");
        for (path, width, height) in [(&sheet, 32, 16), (&image, 24, 12)] {
            image::RgbaImage::from_pixel(width, height, image::Rgba([255; 4]))
                .save(path)
                .unwrap();
        }
        let entry = directory.path().join("default.lua");
        std::fs::write(
            &entry,
            format!(
                "texture_sheet={:?}; texture_image={:?}; {SOURCE}",
                deadsync_song_lua::file_path_string(&sheet),
                deadsync_song_lua::file_path_string(&image),
            ),
        )
        .unwrap();
        let mut context = SongLuaCompileContext::new(directory.path(), "live Sprite scene");
        context.live = true;
        context.defer_initial_live_update = true;
        context.music_length_seconds = 60.0;
        let mut compiled = compile_song_lua(
            &entry,
            &context,
            Default::default(),
            |_: &Path, _: &Path, _: &Path| Ok(Arc::<[()]>::from([])),
            |_: &()| None,
            |_, _| None::<(Kind, SongLuaOverlayState)>,
        )
        .unwrap();
        let indices = compiled
            .overlays
            .iter()
            .enumerate()
            .filter_map(|(index, actor)| actor.name.clone().map(|name| (name, index)))
            .collect();
        let mut live = compiled.live_scene.take().expect("production live scene");
        assert!(live.take_startup_output().errors.is_empty());
        let mut scene = Self {
            live,
            indices,
            elapsed: 0.0,
            _directory: directory,
        };
        scene.update(0.0);
        scene.script("assert(ANIM:GetState()==0 and ANIM:GetNumStates()==2)");
        scene
    }

    fn script(&self, source: &str) {
        self.live
            .runtime()
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"@/live-sprite-scene-check.lua").unwrap())
            .unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    }

    fn update(&mut self, delta: f64) {
        self.elapsed += delta;
        // Exercise the production screen/layer traversal, including its texture
        // phase; advance_actor_scene uses a separate low-level update path.
        let output = self
            .live
            .update(SongLuaLiveFrame {
                beat: self.elapsed * 2.0,
                music_seconds: self.elapsed,
                screen_seconds: self.elapsed,
                delta_seconds: delta,
                ..Default::default()
            })
            .unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
    }

    fn state(&self, name: &str) -> SongLuaOverlayState {
        self.live.states()[self.indices[name]]
    }

    fn animated(&self, frame: u32) {
        let state = self.state("Animated");
        assert_eq!(state.sprite_state_index, Some(frame));
        rect(
            state,
            if frame == 0 {
                [0.0, 0.0, 0.5, 1.0]
            } else {
                [0.5, 0.0, 1.0, 1.0]
            },
        );
        self.script(&format!(
            "assert(ANIM:GetState()=={frame} and OBSERVED=={frame})"
        ));
    }

    fn finish(mut self) {
        assert!(self.live.end().unwrap().errors.is_empty());
    }
}

fn rect(state: SongLuaOverlayState, expected: [f32; 4]) {
    assert!(
        !state.sprite_animate,
        "published live state must not animate twice"
    );
    assert_eq!(state.texcoord_velocity, None);
    let actual = state
        .custom_texture_rect
        .expect("published active Sprite UVs");
    for (a, b) in actual.into_iter().zip(expected) {
        assert!((a - b).abs() < 0.000002, "{actual:?} != {expected:?}");
    }
}

#[test]
fn production_live_scene_advances_sprite_frames_and_uvs_before_parent_callback() {
    let mut scene = Scene::new();
    scene.animated(0);
    rect(scene.state("Scrolling"), [0.0, 0.0, 1.0, 1.0]);
    scene.update(0.125);
    scene.animated(1);
    rect(
        scene.state("Scrolling"),
        [1.0 / 24.0, 1.0 / 48.0, 25.0 / 24.0, 49.0 / 48.0],
    );
    scene.update(0.125);
    scene.animated(0);
    rect(
        scene.state("Scrolling"),
        [1.0 / 12.0, 1.0 / 24.0, 13.0 / 12.0, 25.0 / 24.0],
    );
    scene.finish();
}

#[test]
fn production_live_scene_sprite_phase_respects_queued_reset_pause_and_parent_hibernation() {
    let mut scene = Scene::new();
    scene.update(0.1875);
    scene.animated(1);
    scene.script("ANIM:sleep(.0625):queuecommand('Reset')");
    scene.update(0.0625);
    scene.animated(0);
    scene.script("assert(RESET~=true)");
    // Finishing sleep exactly consumes the Actor update's delta. Its following
    // zero-length command runs only on the next positive update; that reset is
    // then followed by Sprite::Update adding the current frame's effect delta.
    scene.update(0.0625);
    scene.animated(0);
    scene.script("assert(RESET==true)");
    scene.update(0.0625);
    scene.animated(1);

    scene.script("ANIM:animate(false); SCROLL:animate(false)");
    let paused = [scene.state("Animated"), scene.state("Scrolling")];
    scene.update(0.5);
    scene.animated(1);
    for (name, before) in ["Animated", "Scrolling"].into_iter().zip(paused) {
        assert_eq!(
            scene.state(name).custom_texture_rect,
            before.custom_texture_rect
        );
    }

    scene.script("ANIM:animate(true); SCROLL:animate(true); PARENT:hibernate(.25)");
    scene.update(0.125);
    scene.update(0.125);
    for (name, before) in ["Animated", "Scrolling"].into_iter().zip(paused) {
        assert_eq!(
            scene.state(name).custom_texture_rect,
            before.custom_texture_rect
        );
    }
    scene.update(0.125);
    scene.animated(0);
    assert_ne!(
        scene.state("Scrolling").custom_texture_rect,
        paused[1].custom_texture_rect
    );
    scene.finish();
}
