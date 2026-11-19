#![cfg(feature = "test-support")]

use deadlib_lua::compile_named;
use deadlib_render_core::{DrawOp, RenderFrame};
use deadsync_assets::noteskin::SpriteSlot;
use deadsync_assets::song_lua::{CompiledSongLua, SongLuaOverlayActor};
use deadsync_gameplay::{SongLuaRuntimeBuilder, SongLuaRuntimeWindowBuild};
use deadsync_rules::timing::{TimingData, TimingSegments};
use deadsync_song_lua::{self as song, SongLuaCompileContext, SongLuaLiveFrame};
use std::{path::Path, sync::Arc};

type Visuals = deadsync_gameplay::SongLuaRuntimeVisuals<
    SongLuaOverlayActor,
    song::SongLuaCapturedActor,
    song::gameplay::SongLuaRuntimeOverlayStateDelta,
>;

fn compile_layers(
    paths: &[&Path],
    primary: usize,
    context: &SongLuaCompileContext,
) -> Result<Vec<CompiledSongLua>, String> {
    song::compile_song_lua_layers(
        paths, primary, context, Default::default(),
        |_, _, _| Ok(Arc::<[SpriteSlot]>::from([])),
        |_| None, |_, _| None,
    )
}

fn source(name: &str) -> String {
    format!(r#"
COUNTS=COUNTS or {{}}; OWNERS=OWNERS or {{}}; LEAVES=LEAVES or {{}}
COUNTS.{name}={{init=0,on=0,gain=0,lose=0}}
return Def.ActorFrame{{
    Name='{name}',
    InitCommand=function(self)
        OWNERS.{name}=self; COUNTS.{name}.init=COUNTS.{name}.init+1
    end,
    OnCommand=function(self) COUNTS.{name}.on=COUNTS.{name}.on+1 end,
    GainFocusCommand=function(self) COUNTS.{name}.gain=COUNTS.{name}.gain+1 end,
    LoseFocusCommand=function(self) COUNTS.{name}.lose=COUNTS.{name}.lose+1 end,
    Def.Quad{{Name='{name}_leaf',
        InitCommand=function(self) LEAVES.{name}=self end,
        OnCommand=function(self) self:zoomto(32,24):diffuse(1,1,1,1) end,
    }},
}}
"#)
}

struct Fixture {
    scene: song::SongLiveScene,
    visuals: Visuals,
    last_seconds: f32,
    _directory: tempfile::TempDir,
}

impl Fixture {
    fn new(background: &str, layer2: &str) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::create_dir(root.join("first")).unwrap();
        std::fs::create_dir(root.join("second")).unwrap();
        std::fs::write(root.join("first/default.lua"), source("first")).unwrap();
        std::fs::write(root.join("second/default.lua"), source("second")).unwrap();
        std::fs::write(root.join("direct.lua"), source("direct")).unwrap();
        image::RgbaImage::new(2, 2).save(root.join("still.png")).unwrap();
        image::RgbaImage::new(2, 2).save(root.join("later.png")).unwrap();
        // This control schedules the incoming movie; it never decodes or draws it.
        std::fs::write(root.join("movie.mp4"), b"").unwrap();
        let path = root.join("song.ssc");
        std::fs::write(&path, format!(
            "#TITLE:Outgoing Lua background;\n#BPMS:0=120;\n\
             #BGCHANGES:{background};\n#BGCHANGES2:{layer2};\n\
             #NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Challenge;\n\
             #METER:1;\n#NOTES:\n1000\n0000\n0100\n0000\n;\n"
        )).unwrap();
        let parsed = deadsync_simfile::song::parse_song_meta_file(
            &path,
            &deadsync_simfile::song::ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
            0., |_| 16.,
        ).unwrap();
        let timing = TimingData::from_segments(0., 0., &TimingSegments {
            bpms: vec![(0., 120.)], ..Default::default()
        }, &[]);
        let mut context = SongLuaCompileContext::new(root, &parsed.title);
        context.live = true;
        context.defer_initial_live_update = true;
        context.music_length_seconds = 16.;
        context.song_timing = Some(timing.clone());
        context.players[1].enabled = false;
        let mut prepared = song::playback::prepare_song_lua(
            &parsed, &context,
            |path, context| compile_layers(&[path], 0, context).map(|mut layers| layers.remove(0)),
            compile_layers,
        );
        assert!(prepared.compile_succeeded());
        let actual_paths = prepared.compiled_layers().map(|compiled| compiled.entry_path.clone())
            .collect::<Vec<_>>();
        let expected_paths = parsed.background_lua_changes.iter().map(|owner| owner.path.clone())
            .collect::<Vec<_>>();
        assert_eq!(actual_paths, expected_paths, "media must not create extra compiled owners");
        let scene = prepared.take_live_scene().unwrap().expect("retained preparation");
        let (_, _, visuals) = prepared.build_song_lua_runtime(SongLuaRuntimeWindowBuild {
            song_title: &parsed.title,
            timing_players: [&timing, &timing],
            num_players: 1,
            machine_global_offset_seconds: 0.,
            player_global_offset_shift_seconds: &[0.; 2],
            screen_width: 854., screen_height: 480.,
            player_actor_defaults: [deadsync_gameplay::SongLuaPlayerActorDefault {x: 0., y: 0.}; 2],
        });
        Self { scene, visuals, last_seconds: 0., _directory: directory }
    }

    fn check(&mut self, source: &str) {
        let program = compile_named(source.as_bytes(), b"@/background-media-check.lua").unwrap();
        // Release the runtime guard before reporting an assertion failure so
        // normal scene teardown does not encounter a poisoned VM mutex.
        let result = self.scene.runtime_mut().vm_mut().execute(&program).map(|_| ());
        result.unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    }

    fn update(&mut self, seconds: f32) {
        let output = self.scene.update(SongLuaLiveFrame {
            beat: f64::from(seconds * 2.), music_seconds: f64::from(seconds),
            screen_seconds: f64::from(seconds), delta_seconds: f64::from(seconds - self.last_seconds),
            ..Default::default()
        }).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        self.last_seconds = seconds;
        self.scene.write_visuals(&mut self.visuals).unwrap();
    }

    fn draw(&self, name: &str) -> RenderFrame {
        let (layer, index) = self.visuals.background_visual_layers.iter().find_map(|layer| {
            layer.overlays.iter().position(|a| a.name.as_deref() == Some(name))
                .map(|index| (layer, index))
        }).expect("prepared background leaf");
        let states = layer.overlays.iter().map(|actor| actor.initial_state).collect::<Vec<_>>();
        let mut composer = song::playback::actor_conformance::WholeSongComposer::new(&layer.overlays);
        composer.render_overlay(&layer.overlays, &states, index, [854., 480.],
            self.last_seconds, self.last_seconds * 2.)
    }

    fn assert_alpha(&mut self, name: &str, expected: f32) {
        // Native GetDiffuseAlpha reads the destination tween state (Actor.h
        // 524-526). The current interpolated alpha is observed in submitted draws.
        let frame = self.draw(&format!("{name}_leaf"));
        if expected == 0. {
            assert!(frame.ops.is_empty(), "zero-alpha leaf: {:?}", frame.ops);
            return;
        }
        assert_eq!(frame.ops.len(), 1, "one outgoing Quad draw");
        let packed = (expected * 256.).floor().min(255.) / 255.;
        match &frame.ops[0] {
            DrawOp::Sprite(run) => {
                assert_eq!(run.instance_count, 1);
                let instance = &frame.sprite_instances[run.instance_start as usize];
                assert_eq!(instance.size, [32., 24.]);
                assert_eq!(instance.tint[3], packed);
            }
            DrawOp::TexturedMesh(run) => {
                assert_eq!(run.instance_count, 1);
                let instance = &frame.tmesh_instances[run.instance_start as usize];
                let vertices = &frame.tmesh_geometries[run.geometry as usize].vertices;
                assert_eq!(vertices.len(), 6);
                for vertex in vertices.iter() {
                    assert_eq!(vertex.color[3] * instance.tint[3], packed);
                }
            }
            other => panic!("unexpected Quad output: {other:?}"),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) { self.scene.end().unwrap(); }
}

#[test]
fn prepared_image_and_movie_changes_fade_the_real_outgoing_lua_leaf() {
    for incoming in ["still.png", "movie.mp4"] {
        let mut f = Fixture::new(&format!("0=first=1=0=0=1,2={incoming}=3=0=0=1===CrossFade"), "");
        let media = &f.scene.context().background_media_activations;
        assert_eq!(media.len(), 1);
        assert_eq!(media[0].0, 0);
        assert_eq!((media[0].1.start_beat, media[0].1.rate), (2., 3.));
        assert_eq!(media[0].1.transition, "CrossFade");
        f.update(0.);
        f.assert_alpha("first", 1.);
        f.check("assert(COUNTS.first.init==1 and COUNTS.first.on==1 and COUNTS.first.gain==1 and COUNTS.first.lose==0)");
        // The first observed update overshoots the image/movie's beat-2 start
        // by .25 seconds. Native CrossFade acts on the outgoing leaves now.
        for (seconds, alpha) in [(1.25, 0.75), (1.5, 0.5), (1.75, 0.25), (2., 0.)] {
            f.update(seconds);
            f.check(&format!("assert(LEAVES.first:GetDiffuseAlpha()==0); assert(LEAVES.first:GetTweenTimeLeft()=={alpha})"));
            f.assert_alpha("first", alpha);
        }
        f.check("assert(COUNTS.first.lose==1 and COUNTS.first.on==1)");
        f.update(2.25);
        f.check("local a=OWNERS.first:GetParent(); assert(a.__songlua_live_inactive==true)");
    }
}

#[test]
fn media_activation_preserves_layer_rate_and_cached_lua_reactivation() {
    let mut f = Fixture::new(
        "0=first=0.5=0=0=1,2=still.png=3=0=0=1===CrossFade,4=first=1=0=0=1,6=still.png=1=0=0=1===CrossFade,7=later.png=1=0=0=1===CrossFade",
        "0=second=1=0=0=1",
    );
    f.update(0.);
    f.update(1.25);
    f.assert_alpha("first", 0.875); // Old .5 rate, not the incoming media's 3.
    f.assert_alpha("second", 1.);
    f.check("assert(LEAVES.first:GetDiffuseAlpha()==0 and LEAVES.first:GetTweenTimeLeft()==.875)");
    f.check("assert(COUNTS.first.lose==1 and COUNTS.second.lose==0)");
    f.update(2.);
    f.assert_alpha("first", 1.);
    f.check("assert(COUNTS.first.init==1 and COUNTS.first.on==2 and COUNTS.first.gain==2)");
    f.update(3.25);
    f.assert_alpha("first", 0.75);
    f.update(3.5);
    f.check("assert(LEAVES.first:GetDiffuseAlpha()==0 and LEAVES.first:GetTweenTimeLeft()==.75); assert(OWNERS.first:GetParent().__songlua_live_inactive==true); assert(COUNTS.first.lose==2)");
    f.assert_alpha("second", 1.);
}

#[test]
fn preparation_excludes_direct_lua_duplicates_and_same_layer_beat_collisions() {
    let mut f = Fixture::new(
        "0=direct.lua=1=0=0=1,2=direct.lua=1=0=0=1,2=still.png=1=0=0=1===CrossFade,4=movie.mp4=1=0=0=1===CrossFade",
        "0=second=1=0=0=1,2=later.png=1=0=0=1===CrossFade",
    );
    let schedule = f.scene.context().background_media_activations.iter()
        .map(|(layer, change)| (*layer, change.start_beat)).collect::<Vec<_>>();
    assert_eq!(schedule, [(0, 4.), (1, 2.)]);
    // The parsed Lua/media lists lack mixed-kind authored tie order. Keep the
    // existing Lua activation at a colliding beat; do not invent native order.
    f.update(0.);
    f.update(1.25);
    f.assert_alpha("direct", 1.);
    f.assert_alpha("second", 0.75);
    f.check("assert(COUNTS.direct.on==2 and COUNTS.direct.lose==0); assert(COUNTS.second.lose==1)");
    f.update(2.25);
    f.assert_alpha("direct", 0.75);
    f.check("assert(COUNTS.direct.init==1 and COUNTS.direct.lose==1)");
}
