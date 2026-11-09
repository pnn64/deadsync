use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, SongLuaOverlayState as State, SongLuaOverlayUpdateTarget as Target,
    SongLuaOverlayUpdateValue as Update,
    {
        ActorScene, SongFrameCaptureClock as Clock, SongFrameCaptureStates as States,
        SongLayerPlan, SongOverlayFrameCapture as Capture, SongRuntime,
    },
};

struct Fixture {
    _dir: tempfile::TempDir,
    context: SongLuaCompileContext,
    rust: SongRuntime,
    scene: ActorScene,
    actors: Vec<Value>,
    b: Capture,
    baseline: Vec<State>,
    b_update: Vec<State>,
    b_to: Vec<State>,
}
impl Fixture {
    fn new(clocked: bool) -> Self {
        Self::new_source(
            clocked,
            r#"
            saved={};events={}
            return Def.ActorFrame{
                Def.ActorFrame{Name='first',InitCommand=function(self)saved.first=self end},
                Def.Quad{Name='second',InitCommand=function(self)saved.second=self end}}
        "#,
        )
    }
    fn new_source(clocked: bool, source: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        reference_test::register_path(dir.path());
        let path = dir.path().join("default.lua");
        std::fs::write(&path, source).unwrap();
        let context = SongLuaCompileContext::new(dir.path(), "frame output compiler");

        let mut rust = SongRuntime::new(&context).unwrap();
        rust.begin_song_compilation().unwrap();
        let (scene, selected) = rust.construct_gameplay_screen(&context).unwrap();
        rust.load_gameplay_theme(scene, &selected).unwrap();
        rust.initialize_song_layers(scene, &SongLayerPlan::new(&[&path], 0, &context).unwrap())
            .unwrap()
            .unwrap();
        let rust_activation = rust.activate_song_layers(scene, &context).unwrap().unwrap();

        let Value::Table(saved) = rust.vm_mut().get_global(b"saved").unwrap() else {
            panic!()
        };
        let actors: Vec<Value> = ["first", "second"]
            .iter()
            .map(|key| {
                let key = rust.vm_mut().try_intern(key.as_bytes()).unwrap();
                rust.vm_mut().raw_table_get(saved, key).unwrap()
            })
            .collect();

        rust.discover_song_startup_commands(scene, &rust_activation)
            .unwrap();
        let baseline = vec![State::default(); 2];

        rust.begin_song_replay(
            scene,
            &actors
                .iter()
                .map(|v| {
                    let Value::Table(id) = v else { panic!() };
                    (*id, State::default())
                })
                .collect::<Vec<_>>(),
            &rust_activation,
        )
        .unwrap();
        if clocked {
            let frames: Vec<_> = (0..17)
                .map(|i| (i as f64 / 4., if i == 0 { 0. } else { 0.125 }))
                .collect();

            assert!(rust.set_song_replay_frames(scene, &frames, None));
        }
        let mut f = Self {
            _dir: dir,
            context,

            rust,
            scene,

            actors,
            b: Default::default(),
            b_update: baseline.clone(),
            b_to: baseline.clone(),
            baseline,
        };
        f.script("first=saved.first;second=saved.second;function mark(s)events[#events+1]=s end");
        f
    }
    fn script(&mut self, source: &str) {
        self.rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"=frame fixture").unwrap())
            .unwrap();
        self.events();
    }
    fn events(&mut self) {
        let Value::String(id) = self
            .rust
            .vm_mut()
            .execute(&compile_named(b"return table.concat(events,'|')", b"=frame events").unwrap())
            .unwrap()[0]
        else {
            panic!()
        };
        reference_test::assert_debug(
            "song/tests/rust_song_frame_capture.rs::events",
            &self.rust.vm_mut().string_bytes(id).unwrap(),
        );
    }
    fn capture(&mut self, clock: Clock, restored: &[usize], error: bool) {
        let from = self.b_to.clone();
        self.capture_from(clock, restored, error, &from);
    }
    fn capture_from(&mut self, clock: Clock, restored: &[usize], error: bool, b_from: &[State]) {
        let b = self.rust.capture_song_overlay_frame(
            self.scene,
            &mut self.b,
            &self.actors,
            &self.context,
            States {
                baseline: &self.baseline,
                from: b_from,
                update: &mut self.b_update,
                to: &mut self.b_to,
                restored_indices: restored,
            },
            clock,
        );
        assert_eq!(b.is_err(), error, "Rust={b:?}");
        reference_test::assert_debug(
            "song/tests/rust_song_frame_capture.rs::capture",
            &(
                &self.b_update,
                &self.b_to,
                self.b.stopped_indices(),
                self.b.retargeted_values(),
            ),
        );
        compare_tracks(&self.b);
        self.events();
    }
    fn tick(&mut self, prior: f32, next: f32) {
        self.capture(
            Clock {
                prior_time: prior,
                next_time: next,
                next_seconds: f64::from(next),
            },
            &[],
            false,
        )
    }
    fn sample(&self, second: f64) -> (Vec<State>, Vec<(usize, Target, Update)>) {
        let mut states = self.b_to.clone();
        let mut writes = Vec::new();
        self.b
            .sample_schedule(&mut states, second, |index, target, value| {
                writes.push((index, target, value.clone()));
                Ok(())
            })
            .unwrap();
        reference_test::assert_debug(
            "song/tests/rust_song_frame_capture.rs::sample",
            &(second, &states, &writes, self.b.pending_updates(second)),
        );
        (states, writes)
    }

    fn getters(&mut self, values: &[(usize, Target, Update)], error: bool) {
        let b = self
            .rust
            .set_song_overlay_getter_values(self.scene, &self.actors, values);
        assert_eq!(b.is_err(), error, "Rust={b:?}");
        self.events();
    }
    fn query(&mut self, expression: &str) -> String {
        let source = format!("return {expression}");
        let Value::String(id) = self
            .rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"=handoff query").unwrap())
            .unwrap()[0]
        else {
            panic!()
        };
        let actual = self.rust.vm_mut().string_bytes(id).unwrap().to_vec();
        reference_test::assert_debug(
            "song/tests/rust_song_frame_capture.rs::query",
            &(expression, &actual),
        );
        String::from_utf8(actual).unwrap()
    }
    fn publish(&mut self) {
        self.rust
            .publish_song_tween_frame(self.scene, &self.actors)
            .unwrap();
    }
    fn advance(&mut self, frame: usize) {
        let second = frame as f32 * 0.125;

        self.rust.set_values(frame as f32 / 4., second).unwrap();

        self.rust
            .advance_song_replay_at_frame(self.scene, frame, 0.125)
            .unwrap();
        self.events();
    }
}
fn compare_tracks(actual: &Capture) {
    reference_test::assert_debug(
        "song/tests/rust_song_frame_capture.rs::tracks",
        &frame_snapshot::tracks(actual.tracks()),
    );
}

#[test]
fn immediate_frame_tracks_keep_target_order_compound_writes_and_idle_gaps() {
    let mut f = Fixture::new(true);
    f.script("first:basealpha(.25):halign(.2):visible(false);second:draworder(7):basezoomx(2)");
    f.tick(0., 0.25);
    assert!(
        f.b.tracks().iter().any(|t| t.target == Target::BaseAlpha
            && t.samples.last().unwrap().value == Update::F32(0.25))
    );
    f.tick(0.25, 0.5);
    f.script("first:basealpha(.75);second:visible(false);collectgarbage('collect')");
    f.tick(0.5, 0.75);
    assert!(
        f.b.tracks()
            .iter()
            .find(|t| t.target == Target::BaseAlpha)
            .unwrap()
            .samples
            .len()
            >= 3
    );
}

#[test]
fn scheduled_frames_preserve_clocks_interpolation_pending_values_and_completion() {
    let mut f = Fixture::new(true);
    f.script(
        "first:linear(1):x(20):basealpha(.2):draworder(4);second:sleep(.25):linear(.5):basealpha(.4)",
    );
    f.tick(0., 0.);
    assert!(!f.b.pending_updates(0.).is_empty());
    for second in [0., 0.125, 0.25, 0.5, 0.75, 1., 1.25] {
        f.sample(second);
    }
    let (states, _) = f.sample(0.5);
    assert_eq!(states[0].base_alpha, 0.2);
    assert!((states[0].x - 10.).abs() < 0.000001, "{:?}", states[0]);
    f.tick(0., 1.25);
    assert!(f.b.pending_updates(1.25).is_empty());
    assert_eq!(f.b_to[0].base_alpha, 0.2);
}

#[test]
fn cancelling_and_finishing_tweens_preserve_sampled_values_and_stop_indices() {
    let mut f = Fixture::new(true);
    f.script("first:linear(1):basealpha(.2);second:linear(2):basealpha(.4)");
    f.tick(0., 0.);
    f.script(
        "first:stoptweening():basealpha(.7);second:finishtweening();collectgarbage('collect')",
    );
    f.tick(0., 0.5);
    assert!(!f.b.stopped_indices().is_empty());
    assert!(!f.b.tracks().is_empty());
    f.sample(1.5);
}

#[test]
fn message_state_changes_synchronize_existing_tracks_without_new_publications() {
    let mut f = Fixture::new(true);
    f.script("first:basealpha(.3):visible(false)");
    f.tick(0., 0.25);
    let b_from = f.b_to.clone();
    f.b_to[0].base_alpha = 0.8;
    f.capture_from(
        Clock {
            prior_time: 0.25,
            next_time: 0.5,
            next_seconds: 0.5,
        },
        &[0],
        false,
        &b_from,
    );
    assert!(
        f.b.tracks().iter().any(|t| t.target == Target::BaseAlpha
            && t.samples.last().unwrap().value == Update::F32(0.8))
    );
}

#[test]
fn metadata_errors_retain_the_whole_ordered_batch_and_allow_recovery() {
    let mut f = Fixture::new(true);
    for setup in [
        "second.__songlua_player_index=false",
        "second.__songlua_player_index='1';second.__songlua_player_child_name=false",
    ] {
        f.script(&format!("first:basealpha(.3);second:basealpha(.4);{setup}"));
        f.capture(
            Clock {
                prior_time: 0.,
                next_time: 0.25,
                next_seconds: 0.25,
            },
            &[],
            true,
        );
        f.script("second.__songlua_player_index=nil;second.__songlua_player_child_name=nil;collectgarbage('collect')");
        f.tick(0., 0.25);
        assert!(f.b.tracks().iter().any(|t| t.overlay_index == 1
            && t.target == Target::BaseAlpha
            && t.samples.last().unwrap().value == Update::F32(0.4)));
    }
}

#[test]
fn legacy_unclocked_schedule_merging_preserves_ordered_endpoint_tracks() {
    let mut f = Fixture::new(false);
    f.script("first:linear(.5):basealpha(.2);second:sleep(.25):linear(.5):basealpha(.4)");
    f.tick(0., 0.);
    f.sample(0.5);
    f.tick(0., 1.);
    assert!(f.b.pending_updates(1.).is_empty());
    assert!(f.b.tracks().iter().all(|track| {
        track
            .samples
            .windows(2)
            .all(|pair| pair[0].time <= pair[1].time)
    }));
}

#[test]
fn grouped_spring_and_elastic_tweens_keep_full_renderer_property_output() {
    for easing in ["spring", "outElastic"] {
        let mut f = Fixture::new(true);
        let start = if easing == "spring" {
            "first:spring(1)"
        } else {
            "first:linear(1);first.__songlua_capture_easing='outElastic'"
        };
        f.script(&format!("{start};first:x(20):y(30):basezoomx(2):basezoomy(3):basealpha(.2):fov(60);second:effectperiod(3):effectoffset(.125):effectmagnitude(2,3,4):diffuseblink()"));
        f.tick(0., 0.);
        for second in [0., 0.125, 0.5, 0.999999, 1., 1.5] {
            let (_, writes) = f.sample(second);
            assert!(writes.iter().any(|(_, target, _)| *target == Target::X));
            assert!(writes.iter().any(|(_, target, _)| *target == Target::Y));
        }
        f.tick(0., 1.5);
        assert!(
            f.b.tracks()
                .iter()
                .any(|track| track.target == Target::EffectMagnitude)
        );
    }
}

#[test]
fn frame_tracks_outlive_gc_and_scene_teardown_and_foreign_screens_are_rejected() {
    let mut f = Fixture::new(true);
    f.script("first:basealpha(.25);second:visible(false);collectgarbage('collect')");
    f.tick(0., 0.25);
    let mut other = SongRuntime::new(&f.context).unwrap();
    let from = f.b_to.clone();
    assert!(
        other
            .capture_song_overlay_frame(
                f.scene,
                &mut f.b,
                &f.actors,
                &f.context,
                States {
                    baseline: &f.baseline,
                    from: &from,
                    update: &mut f.b_update,
                    to: &mut f.b_to,
                    restored_indices: &[]
                },
                Clock {
                    prior_time: 0.25,
                    next_time: 0.5,
                    next_seconds: 0.5
                }
            )
            .is_err()
    );
    assert!(f.rust.retire_actor_scene(f.scene).unwrap());
    assert!(
        f.rust
            .capture_song_overlay_frame(
                f.scene,
                &mut f.b,
                &f.actors,
                &f.context,
                States {
                    baseline: &f.baseline,
                    from: &from,
                    update: &mut f.b_update,
                    to: &mut f.b_to,
                    restored_indices: &[]
                },
                Clock {
                    prior_time: 0.25,
                    next_time: 0.5,
                    next_seconds: 0.5
                }
            )
            .is_err()
    );

    let b = std::mem::take(&mut f.b).finish();
    reference_test::assert_debug("song/tests/rust_song_frame_capture.rs::finished", &b);
    drop(f);
    assert!(b.iter().any(|t| t.target == Target::BaseAlpha));
}

#[test]
fn handoff_scalar_boolean_vector_and_ignored_targets_match_native_fields() {
    let mut f = Fixture::new(true);
    let targets = [
        Target::Aux,
        Target::X,
        Target::Y,
        Target::Z,
        Target::Zoom,
        Target::ZoomX,
        Target::ZoomY,
        Target::ZoomZ,
        Target::BaseZoom,
        Target::BaseZoomX,
        Target::BaseZoomY,
        Target::BaseZoomZ,
        Target::BaseAlpha,
        Target::RotationX,
        Target::RotationY,
        Target::RotationZ,
        Target::SkewX,
        Target::SkewY,
        Target::CropLeft,
        Target::CropRight,
        Target::CropTop,
        Target::CropBottom,
        Target::FadeLeft,
        Target::FadeRight,
        Target::FadeTop,
        Target::FadeBottom,
        Target::EffectPeriod,
        Target::EffectOffset,
    ];
    let mut values: Vec<_> = targets
        .into_iter()
        .enumerate()
        .map(|(i, t)| (0, t, Update::F32(i as f32 * 0.125 + 1.)))
        .collect();
    values.extend([
        (0, Target::Z, Update::F32(-0.)),
        (0, Target::Visible, Update::Bool(false)),
        (0, Target::Hibernating, Update::Bool(true)),
        (0, Target::Diffuse, Update::Vec4([0.2, 0.4, 0.6, 0.8])),
        (0, Target::EffectMagnitude, Update::Vec3([2., 3., 4.])),
        (0, Target::Quaternion, Update::Quaternion([0., 0., 0., 1.])),
        (0, Target::Fov, Update::F32(99.)),
        (0, Target::BaseRotation, Update::Vec3([9., 8., 7.])),
        (0, Target::Glow, Update::Vec4([1., 0., 0., 1.])),
    ]);
    f.getters(&values, false);
    f.script(r#"
        assert(first.__songlua_diffuse==first.__songlua_state_diffuse)
        assert(first.__songlua_visible==false and first.__songlua_state_visible==false)
        assert(first.__songlua_state_hibernating==true)
        assert(1/first.__songlua_current_z==-math.huge)
        assert(first:GetX()==1.125 and first:GetY()==1.25)
        function dump(v)
            if type(v)=='table' then return table.concat(v,',') end
            return tostring(v)
        end
        function fields(a)
            local names={'aux','current_x','current_y','current_z','state_zoom','state_zoom_x','state_zoom_y','state_zoom_z',
                'state_basezoom','state_basezoom_x','state_basezoom_y','state_basezoom_z','state_base_alpha',
                'state_rot_x_deg','state_rot_y_deg','state_rot_z_deg','state_skew_x','state_skew_y',
                'state_cropleft','state_cropright','state_croptop','state_cropbottom','state_fadeleft','state_faderight',
                'state_fadetop','state_fadebottom','state_effect_period','state_effect_offset','state_diffuse',
                'state_quaternion','state_effect_magnitude','state_glow','state_fov','state_base_rotation'}
            local out={};for _,k in ipairs(names)do out[#out+1]=k..'='..dump(rawget(a,'__songlua_'..k))end
            return table.concat(out,'|')
        end
    "#);
    f.query("fields(first)");
}

#[test]
fn handoff_player_destination_suppression_and_typed_raw_metadata_keep_error_order() {
    let mut f = Fixture::new(true);
    f.script("first.__songlua_player_index='1';first.__songlua_player_child_name=nil");
    f.getters(
        &[
            (0, Target::ZoomX, Update::F32(3.)),
            (0, Target::RotationZ, Update::F32(60.)),
            (0, Target::X, Update::F32(4.)),
            (0, Target::BaseZoom, Update::F32(2.)),
        ],
        false,
    );
    f.script("assert(first.__songlua_state_zoom_x==1 and first.__songlua_state_rot_z_deg==0);assert(first:GetX()==4);assert(first.__songlua_state_basezoom==2)");
    f.script("first.__songlua_player_child_name='Decoration'");
    f.getters(&[(0, Target::ZoomX, Update::F32(3.))], false);
    f.script("assert(first.__songlua_state_zoom_x==3);first.__songlua_player_index=false;first.__songlua_player_child_name=nil");
    f.getters(
        &[
            (0, Target::BaseAlpha, Update::F32(0.25)),
            (0, Target::ZoomX, Update::F32(4.)),
        ],
        true,
    );
    f.script("assert(first.__songlua_state_base_alpha==.25 and first.__songlua_state_zoom_x==3);first.__songlua_player_index=nil;first.__songlua_player_child_name=false");
    f.getters(&[(0, Target::ZoomX, Update::F32(4.))], false);
    f.script("assert(first.__songlua_state_zoom_x==4)");
}

#[test]
fn handoff_batch_roots_and_shared_color_alias_survive_authored_assignment_gc() {
    let mut f = Fixture::new(true);
    f.script(
        r#"
        weakactors=setmetatable({},{__mode='v'});weakcolor=setmetatable({},{__mode='v'})
        first=setmetatable({},{__newindex=function(t,k,v)
            mark(k..':'..type(v));first=nil;second=nil;saved={};collectgarbage('collect')
            assert(weakactors[1] and weakactors[2])
            if k=='__songlua_state_diffuse' then weakcolor[1]=v;return end
            if k=='__songlua_diffuse' then assert(weakcolor[1]==v)end
            rawset(t,k,v)
        end})
        second=setmetatable({},{__newindex=function(t,k,v)mark('second:'..k);rawset(t,k,v)end})
        weakactors[1]=first;weakactors[2]=second
    "#,
    );

    f.actors = vec![
        f.rust.vm_mut().get_global(b"first").unwrap(),
        f.rust.vm_mut().get_global(b"second").unwrap(),
    ];
    f.getters(
        &[
            (0, Target::Diffuse, Update::Vec4([0.1, 0.2, 0.3, 0.4])),
            (0, Target::Quaternion, Update::Quaternion([0., 0., 0., 1.])),
            (1, Target::X, Update::F32(7.)),
        ],
        false,
    );
    assert_eq!(
        f.query("table.concat(events,'|')"),
        "__songlua_state_diffuse:table|__songlua_diffuse:table|second:__songlua_current_x"
    );

    f.actors.clear();
    f.script("collectgarbage('collect');collectgarbage('collect');assert(not weakactors[1] and not weakactors[2] and not weakcolor[1])");
}

#[test]
fn handoff_partial_write_errors_keep_reentrant_getters_and_nested_setter_effects() {
    let mut f = Fixture::new(true);
    f.script(r#"
        getx=first.GetX;first.__songlua_current_x=nil;first.__songlua_visible=nil;first.__songlua_state_visible=nil
        setmetatable(first,{__newindex=function(t,k,v)
            mark(k)
            if k=='__songlua_current_x' then assert(getx(t)==0);second:basealpha(.6);collectgarbage('collect')end
            if k=='__songlua_state_visible' then error('getter handoff failure')end
            rawset(t,k,v)
        end})
    "#);
    f.getters(
        &[
            (0, Target::X, Update::F32(12.)),
            (0, Target::Visible, Update::Bool(false)),
            (0, Target::Z, Update::F32(99.)),
        ],
        true,
    );
    f.script("assert(getx(first)==12 and rawget(first,'__songlua_visible')==false);assert(rawget(first,'__songlua_state_visible')==nil and first.__songlua_current_z==0)");
    f.tick(0., 0.25);
    assert!(f.b.tracks().iter().any(|t| t.overlay_index == 1
        && t.target == Target::BaseAlpha
        && t.samples.last().unwrap().value == Update::F32(0.6)));
    f.script("setmetatable(first,nil)");
    f.getters(&[(0, Target::Visible, Update::Bool(true))], false);
}

#[test]
fn handoff_schedule_sampler_updates_real_getters_before_following_callbacks() {
    let mut f = Fixture::new(true);
    f.script("first:linear(1):x(20):y(30)");
    f.tick(0., 0.);

    let mut b = f.b_to.clone();

    f.rust
        .sample_song_overlay_schedule(f.scene, &f.b, &f.actors, &mut b, 0.5)
        .unwrap();
    reference_test::assert_debug("song/tests/rust_song_frame_capture.rs::sampler_getters", &b);
    f.script("assert(first:GetX()==10 and first:GetY()==15);second:x(first:GetX()+1);collectgarbage('collect')");
    f.tick(0., 0.5);
    assert!(f.b.tracks().iter().any(|t| t.overlay_index == 1
        && t.target == Target::X
        && t.samples.last().unwrap().value == Update::F32(11.)));
}

const HANDOFF_STARTUP: &str = r#"
    saved={};events={}
    return Def.ActorFrame{
        Def.ActorFrame{Name='first',InitCommand=function(self)saved.first=self end,
            OnCommand=function(self)self:linear(1):x(20):y(30):diffuse(.2,.4,.6,.8)end},
        Def.Quad{Name='second',InitCommand=function(self)saved.second=self end,
            OnCommand=function(self)self:sleep(.5)end},
        Def.ActorFrame{InitCommand=function(self)saved.hidden=self end,
            OnCommand=function(self)self:sleep(.25):linear(.5):z(8)end}}
"#;

#[test]
fn handoff_native_tween_publications_keep_progress_and_queue_ownership_across_frames() {
    let mut f = Fixture::new_source(true, HANDOFF_STARTUP);
    f.publish();
    f.publish();
    f.tick(0., 0.);
    for i in 1..=8 {
        f.advance(i);
        f.publish();
        f.tick((i - 1) as f32 * 0.125, i as f32 * 0.125);
        let replay = f.rust.capture_song_tween_replay(f.scene).unwrap();
        let actual = f
            .actors
            .iter()
            .map(|actor| {
                let Value::Table(id) = actor else { panic!() };
                &replay[id]
            })
            .collect::<Vec<_>>();
        reference_test::assert_debug(
            "song/tests/rust_song_frame_capture.rs::tween_replay",
            &(i, actual),
        );
    }
    assert_eq!(
        f.b.tracks()
            .iter()
            .find(|t| t.overlay_index == 0 && t.target == Target::X)
            .unwrap()
            .samples
            .last()
            .unwrap()
            .value,
        Update::F32(20.)
    );
}

#[test]
fn handoff_empty_touched_rows_keep_metadata_errors_and_batch_retry_semantics() {
    let mut f = Fixture::new_source(true, HANDOFF_STARTUP);
    f.script("second.__songlua_player_index=false");
    f.publish();
    f.capture(
        Clock {
            prior_time: 0.,
            next_time: 0.,
            next_seconds: 0.,
        },
        &[],
        true,
    );
    f.script("second.__songlua_player_index=nil;collectgarbage('collect')");
    f.tick(0., 0.);
    f.advance(1);
    f.publish();
    f.tick(0., 0.125);
    assert!(
        f.b.tracks()
            .iter()
            .any(|t| t.overlay_index == 0 && t.target == Target::X)
    );
}

#[test]
fn handoff_fallbacks_are_not_published_as_native_poses_and_scene_guards_hold() {
    let mut f = Fixture::new(true);
    f.script("first:x(10)");
    f.publish();
    f.tick(0., 0.);
    assert_eq!(
        f.b.tracks()
            .iter()
            .find(|t| t.target == Target::X)
            .unwrap()
            .samples
            .last()
            .unwrap()
            .value,
        Update::F32(10.)
    );
    let mut other = SongRuntime::new(&f.context).unwrap();
    assert!(other.publish_song_tween_frame(f.scene, &f.actors).is_err());
    assert!(
        other
            .set_song_overlay_getter_values(f.scene, &f.actors, &[])
            .is_err()
    );
    let mut states = f.b_to.clone();
    assert!(
        other
            .sample_song_overlay_schedule(f.scene, &f.b, &f.actors, &mut states, 0.)
            .is_err()
    );
    f.rust.retire_actor_scene(f.scene).unwrap();
    assert!(f.rust.publish_song_tween_frame(f.scene, &f.actors).is_err());
    assert!(
        f.rust
            .set_song_overlay_getter_values(f.scene, &f.actors, &[])
            .is_err()
    );
}

#[path = "support/frame_snapshot.rs"]
mod frame_snapshot;
#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
