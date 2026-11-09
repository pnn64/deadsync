use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    self as song, SongLuaCompileContext, SongLuaLiveFrame, SongLuaLiveScene,
    rust_engine::{RustSongRuntime, SongLayerPlan, SongLiveFrames, SongLiveLayers, test_support},
};
use std::{path::Path, sync::Arc};

fn slots(_: &Path, _: &Path, _: &Path) -> Result<Arc<[()]>, String> {
    Ok(Arc::from([]))
}
fn layer(_: &()) -> Option<song::SongLuaOverlayModelLayer<()>> {
    None
}
#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    String(Vec<u8>),
}
fn numeric(n: f64) -> Scalar {
    Scalar::Number(if n.is_nan() {
        f64::NAN.to_bits()
    } else {
        n.to_bits()
    })
}
struct Pair {
    native: SongLuaLiveScene,
    rust: RustSongRuntime,
    frames: SongLiveFrames,
    screen: song::rust_engine::ActorScene,
    layers: SongLiveLayers,
    last_music: f32,
    prepared: bool,
    _directory: tempfile::TempDir,
}
impl Pair {
    fn new(sources: &[&str], configure: impl FnOnce(&mut SongLuaCompileContext)) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let paths: Vec<_> = sources
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let p = directory.path().join(format!("layer-{i}.lua"));
                std::fs::write(&p, s).unwrap();
                p
            })
            .collect();
        let paths: Vec<_> = paths.iter().map(|p| p.as_path()).collect();
        let mut context = SongLuaCompileContext::new(directory.path(), "live layers");
        context.live = true;
        context.players[1].enabled = true;
        context.music_length_seconds = 2.;
        configure(&mut context);
        let mut outputs = song::compile_song_lua_layers_with_default_host(
            &paths,
            0,
            &context,
            Default::default(),
            slots,
            layer,
            |_, _| None,
        )
        .unwrap();
        let native = outputs[0].live_scene.take().unwrap();
        let mut rust = RustSongRuntime::new(&context).unwrap();
        rust.begin_song_compilation().unwrap();
        let (screen, selected) = rust.construct_gameplay_screen(&context).unwrap();
        rust.load_gameplay_theme(screen, &selected).unwrap();
        let plan = SongLayerPlan::new(&paths, 0, &context).unwrap();
        rust.initialize_song_layers(screen, &plan).unwrap().unwrap();
        rust.activate_song_layers(screen, &selected)
            .unwrap()
            .unwrap();
        let frames = rust.new_song_live_frames(&selected).unwrap();
        let layers = rust
            .new_song_live_layers(screen, &selected)
            .unwrap()
            .unwrap();
        rust.begin_actor_playback();
        let mut pair = Self {
            native,
            rust,
            frames,
            screen,
            layers,
            last_music: -9999.,
            prepared: false,
            _directory: directory,
        };
        pair.run("S=STATSMAN:GetCurStageStats(); P={S:GetPlayerStageStats(0),S:GetPlayerStageStats(1)}; EVENTS={}");
        pair.compare(SNAPSHOT);
        pair
    }
    fn compare(&mut self, source: &str) {
        let expected: mlua::MultiValue = test_support::legacy_live_scene_lua(&self.native)
            .load(source)
            .call(())
            .unwrap();
        let expected: Vec<_> = expected
            .into_iter()
            .map(|v| match v {
                mlua::Value::Nil => Scalar::Nil,
                mlua::Value::Boolean(v) => Scalar::Boolean(v),
                mlua::Value::Integer(v) => numeric(v as f64),
                mlua::Value::Number(v) => numeric(v),
                mlua::Value::String(v) => Scalar::String(v.as_bytes().to_vec()),
                v => panic!("scalar expected: {v:?}"),
            })
            .collect();
        let values = self
            .rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"@/live-frame-check.lua").unwrap())
            .unwrap()
            .to_vec();
        let actual: Vec<_> = values
            .into_iter()
            .map(|v| match v {
                Value::Nil => Scalar::Nil,
                Value::Boolean(v) => Scalar::Boolean(v),
                Value::Number(v) => numeric(v),
                Value::String(id) => {
                    Scalar::String(self.rust.vm_mut().string_bytes(id).unwrap().to_vec())
                }
                v => panic!("scalar expected: {v:?}"),
            })
            .collect();
        assert_eq!(actual, expected, "{source}");
    }
    fn run(&mut self, source: &str) {
        self.compare(source)
    }
    fn prepare(&mut self, frame: &SongLuaLiveFrame) {
        self.native.prepare_gameplay_frame(frame.clone()).unwrap();
        if !self.prepared {
            self.frames
                .prepare_gameplay_frame(&mut self.rust, frame, None, false)
                .unwrap();
            let beat = self.rust.values().unwrap().0;
            self.layers
                .prepare_backgrounds(
                    &mut self.rust,
                    beat as f64,
                    frame.music_seconds,
                    self.last_music,
                )
                .unwrap();
            self.prepared = true;
        }
        self.compare(SNAPSHOT);
    }
    fn rust_update(&mut self, frame: &SongLuaLiveFrame) -> Result<(), deadlib_lua::Error> {
        let beat = self.frames.update(&mut self.rust, frame, None, false)?;
        if !self.prepared {
            self.layers.prepare_backgrounds(
                &mut self.rust,
                beat,
                frame.music_seconds,
                self.last_music,
            )?;
        }
        self.prepared = false;
        self.layers.advance(
            &mut self.rust,
            beat,
            frame.music_seconds,
            self.last_music,
            frame.delta_seconds,
        )?;
        self.last_music = frame.music_seconds as f32;
        self.frames.finish_health_update(&mut self.rust)
    }
    fn update(&mut self, frame: &SongLuaLiveFrame) {
        let output = self.native.update(frame.clone()).unwrap();
        assert!(
            output.errors.is_empty(),
            "native callback errors: {:?}",
            output.errors
        );
        self.rust_update(frame).unwrap();
        assert!(
            self.rust.take_actor_diagnostics().is_empty(),
            "Rust callback errors"
        );
        self.compare(SNAPSHOT);
        self.render();
    }
    fn render(&mut self) {
        let snapshots = self.rust.actor_scene_snapshot(self.screen).unwrap();
        let lua = test_support::legacy_live_scene_lua(&self.native);
        let actors: mlua::Table = lua.globals().get("ACTORS").unwrap();
        for pair in actors.pairs::<String, mlua::Table>() {
            let (name, actor) = pair.unwrap();
            lua.globals().set("root", actor).unwrap();
            let expected = test_support::legacy_actor_render_states(lua).unwrap();
            let actual: Vec<_> = snapshots
                .iter()
                .filter(|a| {
                    a.name == name.as_bytes() || a.name == format!("{name}_leaf").as_bytes()
                })
                .collect();
            assert_eq!(actual.len(), expected.len(), "render tree {name}");
            for (a, e) in actual.into_iter().zip(expected) {
                assert_eq!(a.render, e, "rendered {name}");
            }
        }
    }
}
impl Drop for Pair {
    fn drop(&mut self) {
        self.layers.release(&mut self.rust).unwrap();
        self.frames.release(&mut self.rust).unwrap();
        self.rust.end_actor_scene(self.screen).unwrap();
        self.rust.release_actor_scene(self.screen).unwrap();
        self.native.end().unwrap();
    }
}

const SNAPSHOT: &str = r#"
local out={};local n=0;local function add(v)n=n+1;out[n]=v==nil and 'nil' or v end
local names={};for name in pairs(ACTORS or {})do names[#names+1]=name end;table.sort(names)
for _,name in ipairs(names)do local a=ACTORS[name];local owner=a
 if a:GetParent() and a:GetParent().__songlua_background_start_beat then owner=a:GetParent()end
 add(name);for _,key in ipairs{'__songlua_live_delta','__songlua_live_inactive','__songlua_background_rate','__songlua_defer_on'}do add(owner[key])end
 for _,key in ipairs{'GetX','GetY','GetZoomX','GetZoomY','GetZoomZ','GetVisible','GetTweenTimeLeft','GetUpdateRate'}do add(a[key](a))end
 local leaf=a:GetChild(name..'_leaf');if leaf then add(leaf:GetDiffuseAlpha());add(leaf:GetZoomX())end
end
add(table.concat(EVENTS or {},';'));return unpack(out,1,n)
"#;
fn source(name: &str, on: &str) -> String {
    format!(
        r#"
    ACTORS=ACTORS or{{}};EVENTS=EVENTS or{{}}
    local function log(action)EVENTS[#EVENTS+1]='{name}:'..action end
    return Def.ActorFrame{{Name='{name}',
      InitCommand=function(self)ACTORS['{name}']=self;self.custom=12;self:x(37):SetUpdateRate(1.25);self:SetUpdateFunction(function(_,d)log('update='..string.format('%0.9g',d))end)end,
      OnCommand=function(self)log('on');{on} end,
      GainFocusCommand=function(self)log('gain')end,LoseFocusCommand=function(self)log('lose')end,
      Def.Quad{{Name='{name}_leaf',InitCommand=function(self)self:zoom(2):diffusealpha(0.25):setsize(8,12)end}}
    }}
    "#
    )
}
fn frame(beat: f64, music: f64) -> SongLuaLiveFrame {
    let mut f = SongLuaLiveFrame {
        beat,
        music_seconds: music,
        screen_seconds: music.max(0.),
        delta_seconds: 0.1,
        ..Default::default()
    };
    f.players[0].life = 0.5;
    f.players[1].life = 0.5;
    f
}
fn activation(
    start: f32,
    rate: f32,
    transition: &str,
) -> deadsync_chart::song::SongBackgroundLuaActivation {
    deadsync_chart::song::SongBackgroundLuaActivation {
        start_beat: start,
        rate,
        transition: transition.into(),
    }
}

#[test]
fn backgrounds_activate_repeat_reset_and_fade_in_native_order() {
    let a = source("a", "self:linear(0.3):x(91)");
    let b = source("b", "self:linear(0.2):x(62)");
    let mut p = Pair::new(&[&a, &b], |c| {
        c.background_layer_count = 2;
        c.layer_start_beats = vec![0., 0.];
        c.background_activations = vec![
            (
                0,
                vec![
                    activation(0., 1., ""),
                    activation(2., 0.75, "CrossFade_Faster"),
                ],
            ),
            (0, vec![activation(1., 2., "CrossFade")]),
        ];
    });
    for (beat, music) in [
        (-1., -0.1),
        (0., 0.),
        (0.5, 0.25),
        (1., 0.5),
        (1.5, 0.75),
        (2., 1.),
        (3., 1.5),
        (4., 2.),
    ] {
        p.update(&frame(beat, music));
    }
    p.run("assert(ACTORS.a.custom==12)");
}
#[test]
fn two_background_layers_ties_and_rate_wrappers_preserve_callback_order() {
    let a = source("a", "");
    let b = source("b", "");
    let c = source("c", "");
    let mut p = Pair::new(&[&a, &b, &c], |ctx| {
        ctx.background_layer_count = 3;
        ctx.layer_start_beats = vec![-1., 0., 0.];
        ctx.background_activations = vec![
            (1, vec![activation(-1., 0.5, "")]),
            (0, vec![activation(0., 2., "")]),
            (0, vec![activation(0., 3., "CrossFade_Fastest")]),
        ];
    });
    for beat in [-1., 0., 0.5, 1.] {
        p.update(&frame(beat, beat * 0.5));
    }
}
#[test]
fn foreground_crossing_start_runs_on_after_players_then_uses_music_delta() {
    let a = source("front", "self:sleep(0.4):linear(0.2):x(80)");
    let mut p = Pair::new(&[&a], |c| {
        c.layer_start_beats = vec![1.];
    });
    p.run("local p=SCREENMAN:GetTopScreen():GetChild('PlayerP1');p:SetUpdateFunction(function()EVENTS[#EVENTS+1]='player';ACTORS.front.seen_player=true end)");
    for (beat, music) in [
        (0., 0.),
        (0.9, 0.45),
        (1.2, 0.6),
        (1.5, 0.75),
        (2., 1.),
        (3., 1.5),
    ] {
        p.update(&frame(beat, music));
    }
    p.run("assert(ACTORS.front.seen_player and not ACTORS.front:GetVisible())");
}
#[test]
fn foreground_visible_override_restarts_on_without_repeating_child_startup() {
    let a = source("front", "self:sleep(2)");
    let mut p = Pair::new(&[&a], |c| {
        c.layer_start_beats = vec![0.];
    });
    p.update(&frame(0., 0.));
    p.run("ACTORS.front:visible(false)");
    p.update(&frame(0.5, 0.25));
    p.update(&frame(1., 0.5));
}
#[test]
fn prepared_frame_does_not_repeat_background_focus_or_player_options() {
    let a = source("a", "self:sleep(0.5)");
    let mut p = Pair::new(&[&a], |c| {
        c.background_layer_count = 1;
        c.layer_start_beats = vec![0.];
    });
    let f = frame(0., 0.);
    p.prepare(&f);
    p.prepare(&frame(1., 0.5));
    p.update(&f);
    p.update(&frame(1., 0.5));
}
#[test]
fn foreground_reads_music_rate_again_after_on_and_keeps_selected_options_rooted() {
    let a = source(
        "front",
        "local o=GAMESTATE:GetSongOptionsObject('ModsLevel_Current');o:MusicRate(2);self:sleep(0.5);collectgarbage('collect')",
    );
    let mut p = Pair::new(&[&a], |c| {
        c.layer_start_beats = vec![1.];
        use deadsync_rules::timing::{TimingData, TimingSegments};
        c.song_timing = Some(TimingData::from_segments(
            0.,
            0.,
            &TimingSegments {
                bpms: vec![(0., 120.)],
                ..Default::default()
            },
            &[],
        ));
    });
    for t in [0., 0.6, 0.8, 1., 1.5, 2.] {
        p.update(&frame(t * 2., t));
    }
}
#[test]
fn rate_preference_changes_background_and_foreground_tween_deltas() {
    let a = source("a", "self:sleep(4)");
    let b = source("front", "self:sleep(4)");
    let mut p = Pair::new(&[&a, &b], |c| {
        c.background_layer_count = 1;
        c.layer_start_beats = vec![0., 0.];
    });
    p.run("local old=PREFSMAN.GetPreference;PREFSMAN.GetPreference=function(self,k)if k=='RateModsAffectFGChanges'then return RATE_PREF end;return old(self,k)end;GAMESTATE:GetSongOptionsObject('ModsLevel_Preferred'):MusicRate(1.5)");
    for (i, pref) in ["false", "true", "'truthy'", "nil"].into_iter().enumerate() {
        p.run(&format!("RATE_PREF={pref}"));
        p.update(&frame(i as f64 * 0.5, i as f64 * 0.25));
    }
}
#[test]
fn background_reset_preserves_resources_masks_wrappers_and_authored_fields() {
    let a = source(
        "a",
        "self:basezoom(2):zoom(3):MaskSource():SetDrawByZPosition(true);self:GetChild('a_leaf'):diffusealpha(0.75);self:linear(0.5):x(120)",
    );
    let b = source("b", "self:sleep(0.5)");
    let mut p = Pair::new(&[&a, &b], |c| {
        c.background_layer_count = 2;
        c.layer_start_beats = vec![0., 0.];
        c.background_activations = vec![
            (0, vec![activation(0., 1., ""), activation(2., 1., "")]),
            (0, vec![activation(1., 1., "")]),
        ];
    });
    p.update(&frame(0., 0.));
    p.run("ACTORS.a:AddWrapperState():zoom(4);ACTORS.a.custom={kept=true}");
    p.update(&frame(1., 0.5));
    p.update(&frame(2., 1.));
    p.run("assert(ACTORS.a.custom.kept and ACTORS.a:GetWrapperState(1):GetZoomX()==4)");
}
#[test]
fn unsupported_transition_keeps_partial_focus_state_and_recovers() {
    let a = source("a", "");
    let b = source("b", "");
    let mut p = Pair::new(&[&a, &b], |c| {
        c.background_layer_count = 2;
        c.layer_start_beats = vec![0., 0.];
        c.background_activations = vec![
            (0, vec![activation(0., 1., "")]),
            (0, vec![activation(1., 1., "unsupported-transition")]),
        ];
    });
    p.update(&frame(0., 0.));
    let f = frame(1., 0.5);
    let native = p.native.update(f.clone()).unwrap_err();
    let rust = p.rust_update(&f).unwrap_err();
    assert!(
        native
            .to_string()
            .contains("unsupported background transition")
    );
    assert!(
        rust.to_string()
            .contains("unsupported background transition")
    );
    p.compare(SNAPSHOT);
    // Rewinding to the old activation does not retry the rejected transition.
    p.update(&frame(0., 0.));
}
#[test]
fn absent_layers_do_not_query_layer_preferences_or_song_options() {
    let a = source("plain", "");
    let mut p = Pair::new(&[&a], |_| {});
    p.run("PREFSMAN.GetPreference=function()error('unused-layer-preference')end;GAMESTATE.GetSongOptionsObject=function()error('unused-layer-options')end");
    p.update(&frame(0., 0.));
}
#[test]
fn layer_owner_rejects_foreign_and_retired_screens_without_consuming_release() {
    let a = source("a", "");
    let mut p = Pair::new(&[&a], |c| {
        c.background_layer_count = 1;
        c.layer_start_beats = vec![0.];
    });
    let mut other =
        RustSongRuntime::new(&SongLuaCompileContext::new(Path::new("."), "foreign")).unwrap();
    assert!(p.layers.release(&mut other).is_err());
    assert!(
        p.layers
            .prepare_backgrounds(&mut other, 0., 0., -9999.)
            .is_err()
    );
    p.update(&frame(0., 0.));
    p.rust.retire_actor_scene(p.screen).unwrap();
    assert!(
        p.layers
            .prepare_backgrounds(&mut p.rust, 0., 0., 0.)
            .is_err()
    );
}
