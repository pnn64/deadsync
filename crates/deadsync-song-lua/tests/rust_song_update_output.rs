use deadlib_lua::{Value, compile_named};
use deadsync_rules::timing::{TimingData, TimingSegments};
use deadsync_song_lua::{
    SongLuaCompileContext, SongLuaEaseTarget, SongLuaTimeUnit, SongLuaUpdateModState,
    rust_engine::{
        RustSongRuntime, SongColumnSplineCapture, SongLayerPlan, SongStaticUpdateOutput,
        test_support::RustUpdateOptionsProbe,
    },
};

struct Fixture {
    _dir: tempfile::TempDir,
    context: SongLuaCompileContext,
    rust: RustSongRuntime,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        reference_test::register_path(dir.path());
        let path = dir.path().join("default.lua");
        std::fs::write(&path, "return Def.ActorFrame{}").unwrap();
        let mut context = SongLuaCompileContext::new(dir.path(), "static update compiler");
        context.music_length_seconds = 12.345;

        let mut rust = RustSongRuntime::new(&context).unwrap();
        rust.begin_song_compilation().unwrap();
        let (scene, selected) = rust.construct_gameplay_screen(&context).unwrap();
        rust.load_gameplay_theme(scene, &selected).unwrap();
        let plan = SongLayerPlan::new(&[&path], 0, &context).unwrap();
        rust.initialize_song_layers(scene, &plan).unwrap().unwrap();
        rust.activate_song_layers(scene, &context).unwrap().unwrap();
        let mut result = Self {
            _dir: dir,
            context,
            rust,
        };
        result.script(
            r#"
            events={}
            function mark(s)events[#events+1]=s end
            first=__songlua_player_options_1
            second=__songlua_player_options_2
        "#,
        );
        result
    }
    fn script(&mut self, source: &str) {
        self.rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"=static update fixture").unwrap())
            .unwrap();
        self.events();
    }
    fn events(&mut self) -> String {
        let Value::String(id) = self
            .rust
            .vm_mut()
            .execute(&compile_named(b"return table.concat(events,'|')", b"=events").unwrap())
            .unwrap()[0]
        else {
            panic!()
        };
        let actual = self.rust.vm_mut().string_bytes(id).unwrap().to_vec();
        reference_test::assert_debug("song/tests/rust_song_update_output.rs::events", &actual);
        String::from_utf8(actual).unwrap()
    }
    fn output(&mut self, error: bool) -> Option<SongStaticUpdateOutput> {
        let rust = (|| {
            let mut splines = SongColumnSplineCapture::default();
            self.rust.capture_song_column_splines(&mut splines, 0.)?;
            self.rust
                .compile_song_static_update_output(&self.context, splines)
        })();
        assert_eq!(rust.is_err(), error, "Rust: {:?}", rust.as_ref().err());
        self.events();
        rust.ok().map(|output| {
            compare_output(&output);
            output
        })
    }
}
fn compare_output(actual: &SongStaticUpdateOutput) {
    reference_test::assert_debug(
        "song/tests/rust_song_update_output.rs::update_output",
        &spline_snapshot::update_output(actual),
    );
}
fn compare_states(actual: &[SongLuaUpdateModState; 2]) {
    reference_test::assert_debug(
        "song/tests/rust_song_update_output.rs::states",
        &spline_snapshot::states(actual),
    );
}

#[test]
fn static_update_output_keeps_public_option_targets_and_approach_speeds() {
    let mut f = Fixture::new();
    f.script(
        r#"
        first:Mini(.37,2.5);first:XMod(2.75,1.25);first:Reverse(.8,4)
        first:FromString('75% drunk, 20% tornado, *3 40% tipsy')
        second:CMod(321,5);second:Mini(.125,0)
    "#,
    );
    let out = f.output(false).unwrap();
    assert!(
        out.eases
            .iter()
            .any(|w| w.target == SongLuaEaseTarget::Mod("xmod".into())
                && w.to == 2.75
                && w.approach_speed == Some(1.25))
    );
    assert!(
        out.eases
            .iter()
            .any(|w| w.target == SongLuaEaseTarget::Mod("cmod".into())
                && w.to == 321.
                && w.approach_speed == Some(5.))
    );
    assert!(
        out.eases
            .iter()
            .all(|w| w.unit == SongLuaTimeUnit::BeatClock)
    );
}

#[test]
fn compact_option_snapshots_preserve_raw_state_and_numeric_conversion_rules() {
    let mut f = Fixture::new();
    f.script(
        r#"
        first.__songlua_player_option_state={
            mini=' .25 ',reverse=true,drunk=false,tipsy='-inf',tornado='NaN',
            bumpy=1/0,unknown={},[5]='.2',['5']='.2',['signed\0name']=-1/math.huge,
            nohex='0x10',bad=string.char(255),nan_number=0/0}
        first.__songlua_player_option_speeds={mini='0x1.8p1',reverse=-0.0,drunk=1/0}
        first.__songlua_speedmod_active=nil
        first.__songlua_speedmod_xmod='0x1.4p1'
        first.__songlua_speedmod_cmod=' 345 '
        first.__songlua_speedmod_mmod=-0.0
        second.__songlua_player_option_state=nil
        second.__songlua_player_option_speeds=nil
    "#,
    );

    let mut b = RustUpdateOptionsProbe::new(&mut f.rust).unwrap();
    for speeds in [false, true] {
        let right = b.sample(&mut f.rust, speeds).unwrap();
        compare_states(&right);
        if !speeds {
            assert_eq!(right[0]["xmod"], 2.5);
            assert!(!right[0].contains_key("nohex") && !right[0].contains_key("bumpy"));
            assert_eq!(right[0]["signed\0name"].to_bits(), (-0.0f32).to_bits());
        }
    }
    b.release(&mut f.rust).unwrap();
}

#[test]
fn option_capture_retains_original_tables_across_gc_and_replaced_globals() {
    let mut f = Fixture::new();
    f.script(
        r#"
        weak=setmetatable({},{__mode='v'})
        first=nil;second=nil
        __songlua_player_options_1=nil;__songlua_player_options_2=nil
        setmetatable(_G,{__index=function(_,key)
            if key=='__songlua_player_options_1' then
                mark('global1')
                local v=setmetatable({__songlua_player_option_state={mini=.1}},
                    {__index=function(_,key)
                        mark('first:'..key)
                        __songlua_player_options_1={}
                        __songlua_player_options_2={}
                        collectgarbage('collect');assert(weak[1] and weak[2])
                        if key=='__songlua_speedmod_xmod' then return 2 end
                    end})
                weak[1]=v;return v
            elseif key=='__songlua_player_options_2' then
                mark('global2');collectgarbage('collect');assert(weak[1])
                local v={__songlua_player_option_state={reverse=.2}}
                weak[2]=v;return v
            end
        end})
    "#,
    );

    let mut b = RustUpdateOptionsProbe::new(&mut f.rust).unwrap();
    f.events();
    for _ in 0..3 {
        compare_states(&b.sample(&mut f.rust, false).unwrap());
        compare_states(&b.sample(&mut f.rust, true).unwrap());
        f.script("collectgarbage('collect');assert(weak[1] and weak[2])");
    }

    b.release(&mut f.rust).unwrap();
    f.script(
        "collectgarbage('collect');collectgarbage('collect');assert(not weak[1] and not weak[2])",
    );
}

#[test]
fn option_snapshots_recover_from_typed_errors_without_losing_roots_or_state() {
    let mut f = Fixture::new();

    let mut b = RustUpdateOptionsProbe::new(&mut f.rust).unwrap();
    for (setup, speeds) in [
        ("first.__songlua_player_option_state=false", false),
        ("first.__songlua_player_option_state={[true]=1}", false),
        (
            "first.__songlua_player_option_state={[string.char(255)]=1}",
            false,
        ),
        (
            "first.__songlua_player_option_state={};first.__songlua_speedmod_active=false",
            false,
        ),
        (
            "first.__songlua_speedmod_active=nil;first.__songlua_speedmod_xmod=false",
            false,
        ),
        ("first.__songlua_player_option_speeds=false", true),
        ("first.__songlua_player_option_speeds={mini=false}", true),
        ("first.__songlua_player_option_speeds={[{}]=1}", true),
    ] {
        f.script(setup);

        assert!(b.sample(&mut f.rust, speeds).is_err(), "Rust {setup}");
        f.script("first.__songlua_player_option_state={mini=.3};first.__songlua_player_option_speeds={mini=2};first.__songlua_speedmod_active='xmod';first.__songlua_speedmod_xmod=2;collectgarbage('collect')");
        for speeds in [false, true] {
            compare_states(&b.sample(&mut f.rust, speeds).unwrap());
        }
    }
    let mut other = RustSongRuntime::new(&f.context).unwrap();
    assert!(b.sample(&mut other, false).is_err());
    assert!(b.release(&mut other).is_err());
    compare_states(&b.sample(&mut f.rust, false).unwrap());
    b.release(&mut f.rust).unwrap();
    b.release(&mut f.rust).unwrap();
}

#[test]
fn static_output_uses_song_clock_origin_rate_and_exact_tail_limits() {
    let mut f = Fixture::new();
    f.script("first:Mini(.4,2);first:MMod(450,3);second:Reverse(.5,1)");
    for (origin, rate, length) in [
        (0., 1., 12.345),
        (1.25, 1.7, 4.2),
        (-2., 0.5, 1.),
        (0.5, 2., 0.),
        (-0.25, 1., -3.),
    ] {
        f.context.song_music_rate = rate;
        f.context.music_length_seconds = length;
        f.context.song_timing = Some(TimingData::from_segments(
            origin,
            0.,
            &TimingSegments {
                bpms: vec![(0., 120.), (4., 90.)],
                ..Default::default()
            },
            &[],
        ));
        let out = f.output(false).unwrap();
        assert!(!out.eases.is_empty());
        assert!(
            out.eases
                .iter()
                .all(|w| w.unit == SongLuaTimeUnit::Second && w.limit > 0.)
        );
    }
}

#[test]
fn static_output_keeps_column_transforms_and_owned_spline_coefficients() {
    let mut f = Fixture::new();
    f.script(
        r#"
        local player=SCREENMAN:GetTopScreen():GetChild('PlayerP1')
        local columns=player:GetChild('NoteField'):GetColumnActors()
        columns[1]:GetRotHandler():SetSplineMode('NoteColumnSplineMode_Offset')
        local rot=columns[1]:GetRotHandler():GetSpline()
        rot:SetSize(1);rot:SetPoint(1,{0,0,37})
        columns[2]:GetPosHandler():SetSplineMode('NoteColumnSplineMode_Position')
        local pos=columns[2]:GetPosHandler():GetSpline()
        pos:SetSize(3);pos:SetPoint(1,{1,2,3});pos:SetPoint(2,{4,5,6});pos:SetPoint(3,{7,8,9})
        first:Mini(.25,1)
    "#,
    );
    let out = f.output(false).unwrap();
    assert!(!out.column_transforms.is_empty() && !out.column_splines.is_empty());
    drop(f);
    assert!(
        out.column_splines
            .iter()
            .flat_map(|track| track.frames.iter())
            .any(|frame| frame
                .position
                .as_ref()
                .is_some_and(|s| !s.coefficients.is_empty()))
    );
}

#[test]
fn static_output_preserves_phase_order_and_releases_option_roots_on_failure() {
    let mut f = Fixture::new();
    f.script(r#"
        first={__songlua_player_option_state={mini=.2},__songlua_player_option_speeds={mini=2}}
        second={__songlua_player_option_state={reverse=.3},__songlua_player_option_speeds={reverse=false}}
        weak=setmetatable({first,second},{__mode='v'})
        __songlua_player_options_1=nil;__songlua_player_options_2=nil
        setmetatable(_G,{__index=function(_,key)
            if key=='__songlua_player_options_1' then mark('table1');return first end
            if key=='__songlua_player_options_2' then mark('table2');return second end
        end})
        setmetatable(first,{__index=function(_,key)mark('p1:'..key)end})
        setmetatable(second,{__index=function(_,key)mark('p2:'..key)end})
    "#);
    f.output(true);
    assert_eq!(
        f.events(),
        "table1|table2|p1:__songlua_speedmod_xmod|p1:__songlua_speedmod_cmod|p1:__songlua_speedmod_mmod|p2:__songlua_speedmod_xmod|p2:__songlua_speedmod_cmod|p2:__songlua_speedmod_mmod"
    );
    f.script("first=nil;second=nil;collectgarbage('collect');collectgarbage('collect');assert(not weak[1] and not weak[2])");
    f.script("setmetatable(_G,nil);__songlua_player_options_1={};__songlua_player_options_2={};events={}");
    f.output(false);
}

#[test]
fn option_capture_construction_validates_both_globals_and_keeps_lookup_order() {
    let mut f = Fixture::new();
    for body in [
        "__songlua_player_options_1=false",
        "__songlua_player_options_1={};__songlua_player_options_2=false",
        "__songlua_player_options_1={};__songlua_player_options_2=nil",
    ] {
        f.script(body);
        f.output(true);
    }
    f.script("__songlua_player_options_1={};__songlua_player_options_2={}");
    f.output(false);
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
#[path = "support/spline_snapshot.rs"]
mod spline_snapshot;
