use deadlib_lua::{Value, compile_named};
use deadsync_gameplay::SongLuaColumnSplineTrack as Track;
use deadsync_song_lua::{
    SongLuaCompileContext, {SongColumnSplineCapture, SongLayerPlan, SongRuntime},
};
use std::sync::Arc;

struct Fixture {
    _dir: tempfile::TempDir,
    rust: SongRuntime,
    rust_capture: SongColumnSplineCapture,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        reference_test::register_path(dir.path());
        let path = dir.path().join("default.lua");
        std::fs::write(&path, "return Def.ActorFrame{}").unwrap();
        let context = SongLuaCompileContext::new(dir.path(), "column compiler output");

        let mut rust = SongRuntime::new(&context).unwrap();
        rust.begin_song_compilation().unwrap();
        let (scene, selected) = rust.construct_gameplay_screen(&context).unwrap();
        rust.load_gameplay_theme(scene, &selected).unwrap();
        let plan = SongLayerPlan::new(&[&path], 0, &context).unwrap();
        rust.initialize_song_layers(scene, &plan).unwrap().unwrap();
        rust.activate_song_layers(scene, &context).unwrap().unwrap();
        let mut this = Self {
            _dir: dir,
            rust,
            rust_capture: Default::default(),
        };
        this.script(r#"
            events={}
            function mark(key)events[#events+1]=key end
            function handler(points,mode)
                return {__songlua_spline_mode=mode or 'NoteColumnSplineMode_Position',
                    __songlua_beats_per_t=1,__songlua_receptor_t=0,__songlua_subtract_song_beat=true,
                    __songlua_spline={__songlua_spline_size=#points,__songlua_spline_points=points,__songlua_spline_loop=false}}
            end
            function column(player,index,pos,zoom)
                return {__songlua_player_index=player,__songlua_column_index=index,
                    __songlua_pos_handler=pos,__songlua_zoom_handler=zoom}
            end
            function install(first,second)
                __songlua_top_screen_player_1=first and {__songlua_children={NoteField={__songlua_note_columns=first}}}
                __songlua_top_screen_player_2=second and {__songlua_children={NoteField={__songlua_note_columns=second}}}
            end
        "#);
        this
    }
    fn script(&mut self, source: &str) {
        self.rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"=column output fixture").unwrap())
            .unwrap();
        self.events();
    }
    fn events(&mut self) -> String {
        let Value::String(id) = self
            .rust
            .vm_mut()
            .execute(&compile_named(b"return table.concat(events,'|')", b"=column events").unwrap())
            .unwrap()[0]
        else {
            panic!()
        };
        let actual = self.rust.vm_mut().string_bytes(id).unwrap().to_vec();
        reference_test::assert_debug("song/tests/rust_song_column_output.rs::events", &actual);
        String::from_utf8(actual).unwrap()
    }
    fn sample(&mut self, second: f32, error: Option<&str>) {
        let result = self
            .rust
            .capture_song_column_splines(&mut self.rust_capture, second);
        assert_eq!(result.is_err(), error.is_some(), "Rust={result:?}");
        if let Some(message) = error.filter(|message| !message.is_empty()) {
            assert!(result.unwrap_err().contains(message));
        }
        self.events();
    }
    fn hides(&mut self, error: bool) -> Vec<deadsync_song_lua::SongLuaNoteHideWindow> {
        let result = self.rust.read_song_note_column_zoom_hides();
        assert_eq!(result.is_err(), error, "Rust={result:?}");
        self.events();
        match result {
            Ok(value) => {
                reference_test::assert_debug(
                    "song/tests/rust_song_column_output.rs::hides",
                    &value,
                );
                value
            }
            Err(_) => Vec::new(),
        }
    }
    fn finish(&mut self) -> Vec<Track> {
        let b = std::mem::take(&mut self.rust_capture).finish().unwrap();
        assert_tracks(&b);
        b
    }
}
fn assert_tracks(actual: &[Track]) {
    reference_test::assert_debug(
        "song/tests/rust_song_column_output.rs::tracks",
        &spline_snapshot::tracks(actual),
    );
}

#[test]
fn spline_tracks_capture_real_handlers_mutations_metadata_and_reenabled_geometry() {
    let mut f = Fixture::new();
    f.script(r#"
        local c=SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField'):GetColumnActors()[1]
        h=c:GetPosHandler();s=h:GetSpline()
        h:SetSplineMode('NoteColumnSplineMode_Position');h:SetBeatsPerT(2);h:SetReceptorT(0.5);h:SetSubtractSongBeat(false)
        s:SetSize(3);s:SetPoint(1,{1,2,3});s:SetPoint(2,{4,5,6});s:SetPoint(3,{7,8,9})
    "#);
    f.sample(0., None);
    f.sample(0.2, None);
    f.script("s.__songlua_spline_points[1][1]=2");
    f.sample(0.4, None);
    f.script("h:SetBeatsPerT(3)");
    f.sample(0.6, None);
    f.script("s:SetLoop(true)");
    f.sample(0.8, None);
    f.script("h:SetSplineMode('NoteColumnSplineMode_Disabled')");
    f.sample(1., None);
    f.script("h:SetSplineMode('NoteColumnSplineMode_Position')");
    f.sample(1.2, None);
    let tracks = f.finish();
    drop(f);
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].frames.len(), 6);
    assert!(Arc::ptr_eq(
        &tracks[0].frames[1].position.as_ref().unwrap().coefficients,
        &tracks[0].frames[2].position.as_ref().unwrap().coefficients
    ));
}

#[test]
fn spline_tracks_keep_sparse_points_typed_indices_signed_zero_and_lane_order() {
    let mut f = Fixture::new();
    f.script(r#"
        h=handler({{1,'0x4'},nil,{3,2}});h.__songlua_spline.__songlua_spline_size='3'
        h.__songlua_spline.__songlua_spline_loop='';h.__songlua_beats_per_t='2';h.__songlua_receptor_t='0.5';h.__songlua_subtract_song_beat=0
        other=handler({{1,'0x4'},nil,{3,2}});other.__songlua_spline.__songlua_spline_size=3
        other.__songlua_spline.__songlua_spline_loop=true
        install({column('7',2.9,h,nil)},{column(0,0,other,nil)})
    "#);
    f.sample(0., None);
    f.script("h.__songlua_spline.__songlua_spline_points[1][1]=-0;other.__songlua_spline.__songlua_spline_points[1][1]=-0");
    f.sample(1., None);
    f.script("h.__songlua_spline.__songlua_spline_points[1][1]=0;other.__songlua_spline.__songlua_spline_points[1][1]=0");
    f.sample(2., None);
    let tracks = f.finish();
    assert_eq!(
        tracks
            .iter()
            .map(|t| (t.player, t.column))
            .collect::<Vec<_>>(),
        [(0, 0), (7, 2)]
    );
}

#[test]
fn spline_tracks_retain_getter_temporaries_future_fields_and_release_processed_fields() {
    let mut f = Fixture::new();
    f.script(r#"
        weak=setmetatable({}, {__mode='v'})
        local c={__songlua_player_index=0,__songlua_column_index=0}
        setmetatable(c,{__index=function(_,key)
            if key~='__songlua_pos_handler'then return nil end
            local h=setmetatable({}, {__index=function(_,key)
                mark(key)
                if key=='__songlua_spline_mode'then return 'NoteColumnSplineMode_Position' end
                if key=='__songlua_beats_per_t'then collectgarbage();assert(weak.handler);return 1 end
                if key=='__songlua_receptor_t'then return 0 end
                if key=='__songlua_subtract_song_beat'then collectgarbage();assert(weak.handler and weak.spline and weak.points);return false end
                if key=='__songlua_spline'then
                    local s=setmetatable({}, {__index=function(_,key)
                        mark(key)
                        if key=='__songlua_spline_size'then return 2 end
                        if key=='__songlua_spline_points'then local p={{1,2,3},{4,5,6}};weak.points=p;return p end
                        if key=='__songlua_spline_loop'then collectgarbage();assert(weak.handler and weak.spline and weak.points);return false end
                    end});weak.spline=s;return s
                end
            end});weak.handler=h;return h
        end})
        local columns={c}
        local first=setmetatable({}, {__index=function(_,key)
            if key=='__songlua_note_columns'then
                __songlua_top_screen_player_1=nil;__songlua_top_screen_player_2=nil;collectgarbage()
                assert(weak.first and weak.second);mark('first');return columns
            end
        end})
        local second=setmetatable({}, {__index=function(_,key)
            if key=='__songlua_note_columns'then collectgarbage();assert(weak.first==nil and weak.second);mark('second');return {} end
        end})
        weak.first=first;weak.second=second
        __songlua_top_screen_player_1={__songlua_children={NoteField=first}}
        __songlua_top_screen_player_2={__songlua_children={NoteField=second}}
    "#);
    f.sample(0., None);
    assert!(f.events().starts_with("first|__songlua_spline_mode|"));
    f.script("collectgarbage();collectgarbage();assert(weak.first==nil and weak.second==nil and weak.handler==nil and weak.spline==nil and weak.points==nil)");
    assert_eq!(f.finish().len(), 1);
}

#[test]
fn spline_tracks_keep_error_boundaries_partial_buffers_and_recovery() {
    let mut f = Fixture::new();
    f.script("h=handler({{1,2,3},{4,5,6}});install({column(0,0,h,nil)},nil)");
    f.sample(0., None);
    for (bad, repair, message) in [
        (
            "h.__songlua_spline_mode=false",
            "h.__songlua_spline_mode='NoteColumnSplineMode_Position'",
            "",
        ),
        (
            "h.__songlua_spline.__songlua_spline_size=65537",
            "h.__songlua_spline.__songlua_spline_size=2",
            "65536 points",
        ),
        (
            "h.__songlua_beats_per_t=0",
            "h.__songlua_beats_per_t=1",
            "invalid beat conversion",
        ),
        (
            "h.__songlua_spline.__songlua_spline_points[1][1]=math.huge",
            "h.__songlua_spline.__songlua_spline_points[1][1]=1",
            "nonfinite point",
        ),
        (
            "h.__songlua_spline.__songlua_spline_points[1]=false",
            "h.__songlua_spline.__songlua_spline_points[1]={1,2,3}",
            "",
        ),
    ] {
        f.script(bad);
        f.sample(1., Some(message));
        f.script(repair);
        f.sample(2., None);
    }
    assert_eq!(f.finish()[0].frames.len(), 1);
}

#[test]
fn spline_tracks_keep_nonreflexive_coefficients_and_the_logical_byte_limit() {
    let mut f = Fixture::new();
    f.script("h=handler({{3e38,3e38,3e38},{-3e38,-3e38,-3e38},{3e38,3e38,3e38}});install({column(0,0,h,nil)},nil)");
    f.sample(0., None);
    f.sample(1., None);
    f.sample(2., None);
    assert_eq!(f.finish()[0].frames.len(), 3);
    f.script("h=handler({});h.__songlua_spline.__songlua_spline_size=65536;install({column(0,0,h,nil)},nil)");
    for i in 0..43 {
        f.script(&format!("h.__songlua_beats_per_t={}", i + 1));
        f.sample(
            i as f32,
            if i == 42 {
                Some("128 MiB per layer")
            } else {
                None
            },
        );
    }
    let tracks = f.finish();
    assert_eq!(tracks[0].frames.len(), 42);
    assert_eq!(
        tracks[0].frames[0]
            .position
            .as_ref()
            .unwrap()
            .coefficients
            .len(),
        65536
    );
}

#[test]
fn zoom_hides_keep_raw_thresholds_sparse_points_modes_and_outer_player_indices() {
    let mut f = Fixture::new();
    f.script(r#"
        h=handler({{-1,-1,-1},{'-inf','-inf','-inf'},{-1,-1,0},{-1,-1,-1}},'NoteColumnSplineMode_Offset')
        h.__songlua_subtract_song_beat=false;h.__songlua_beats_per_t=2
        z=handler({{-1,-1,-1},{-1,-1,-1}},'NoteColumnSplineMode_Offset');z.__songlua_subtract_song_beat=false
        install({column(99,3,nil,h)},{column(0,1,nil,z)})
    "#);
    let hides = f.hides(false);
    assert!(!hides.is_empty());
    assert!(hides.iter().all(|w| w.player < 2));
    for change in [
        "h.__songlua_subtract_song_beat=true",
        "h.__songlua_subtract_song_beat=false;h.__songlua_beats_per_t=0",
        "h.__songlua_beats_per_t=1;h.__songlua_spline_mode='NoteColumnSplineMode_Position'",
        "h.__songlua_spline_mode=nil",
    ] {
        f.script(change);
        f.hides(false);
    }
}

#[test]
fn zoom_hides_retain_the_current_player_and_read_later_globals_after_processing() {
    let mut f = Fixture::new();
    f.script(r#"
        weak=setmetatable({}, {__mode='v'})
        h=handler({{-1,-1,-1}},'NoteColumnSplineMode_Offset');h.__songlua_subtract_song_beat=false
        local columns={column(0,0,nil,h)}
        local field=setmetatable({}, {__index=function(_,key)
            if key=='__songlua_note_columns'then
                __songlua_top_screen_player_1=nil;__songlua_top_screen_player_2=nil;collectgarbage()
                assert(weak.player);mark('first columns');return columns
            end
        end})
        local player={__songlua_children={NoteField=field}};weak.player=player
        __songlua_top_screen_player_1=player
        setmetatable(_G,{__index=function(_,key)
            if key=='__songlua_top_screen_player_2'then collectgarbage();assert(weak.player==nil);mark('second global')end
        end})
        __songlua_top_screen_player_2=nil
    "#);
    assert!(!f.hides(false).is_empty());
    assert_eq!(f.events(), "first columns|second global");
}

#[test]
fn zoom_hides_keep_typed_errors_and_recover_without_changing_spline_capture() {
    let mut f = Fixture::new();
    f.script("h=handler({{-1,-1,-1}},'NoteColumnSplineMode_Offset');h.__songlua_subtract_song_beat=false;install({column(0,0,nil,h)},nil)");
    for (bad, repair) in [
        (
            "h.__songlua_spline_mode=false",
            "h.__songlua_spline_mode='NoteColumnSplineMode_Offset'",
        ),
        ("h.__songlua_beats_per_t=false", "h.__songlua_beats_per_t=1"),
        (
            "h.__songlua_spline.__songlua_spline_size=false",
            "h.__songlua_spline.__songlua_spline_size=1",
        ),
        (
            "h.__songlua_spline.__songlua_spline_points[1]=false",
            "h.__songlua_spline.__songlua_spline_points[1]={-1,-1,-1}",
        ),
    ] {
        f.script(bad);
        f.hides(true);
        f.script(repair);
        assert!(!f.hides(false).is_empty());
    }
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
#[path = "support/spline_snapshot.rs"]
mod spline_snapshot;
