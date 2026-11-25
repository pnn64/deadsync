#![cfg(feature = "test-support")]

use deadlib_lua::compile_named;
use deadsync_gameplay::SongLuaColumnSplineTrack as Track;
use deadsync_song_lua::{
    SongColumnSplineCapture, SongLayerPlan, SongLuaColumnTransformSample as Sample,
    SongLuaColumnTransformTarget as Target, SongLuaCompileContext, SongRuntime,
};

const IDENTITY: [usize; 4] = [0, 1, 2, 3];
const MIRROR: [usize; 4] = [3, 2, 1, 0];
const LEFT: [usize; 4] = [2, 0, 3, 1];
const RIGHT: [usize; 4] = [1, 3, 0, 2];

struct Fixture {
    _dir: tempfile::TempDir,
    runtime: SongRuntime,
}

impl Fixture {
    fn new(maps: [[usize; 4]; 2], live: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("default.lua");
        std::fs::write(&path, "return Def.ActorFrame{}").unwrap();
        let mut context = SongLuaCompileContext::new(dir.path(), "column turn routing");
        context.live = live;
        for (player, map) in context.players.iter_mut().zip(maps) {
            player.enabled = true;
            player.column_take_from[..4].copy_from_slice(&map);
        }
        let mut runtime = SongRuntime::new(&context).unwrap();
        runtime.begin_song_compilation().unwrap();
        let (scene, selected) = runtime.construct_gameplay_screen(&context).unwrap();
        runtime.load_gameplay_theme(scene, &selected).unwrap();
        let plan = SongLayerPlan::new(&[&path], 0, &context).unwrap();
        runtime
            .initialize_song_layers(scene, &plan)
            .unwrap()
            .unwrap();
        runtime
            .activate_song_layers(scene, &context)
            .unwrap()
            .unwrap();
        let mut fixture = Self { _dir: dir, runtime };
        fixture.script(
            r#"
            columns={}
            for p=1,2 do
                columns[p]=SCREENMAN:GetTopScreen():GetChild('PlayerP'..p)
                    :GetChild('NoteField'):GetColumnActors()
            end
            function configure(h,mode,relative,points,beats,receptor)
                h:SetSplineMode('NoteColumnSplineMode_'..mode)
                h:SetSubtractSongBeat(relative)
                h:SetBeatsPerT(beats or 1)
                h:SetReceptorT(receptor or 0)
                local s=h:GetSpline()
                s:SetSize(#points)
                for i,point in ipairs(points) do s:SetPoint(i,point) end
            end
            "#,
        );
        fixture
    }

    fn script(&mut self, source: &str) {
        self.runtime
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"=column turn regression").unwrap())
            .unwrap();
    }

    fn transforms(&mut self) -> Vec<Sample> {
        self.runtime
            .read_song_note_column_transform_samples()
            .unwrap()
    }

    fn capture(&mut self, capture: &mut SongColumnSplineCapture, second: f32) {
        self.runtime
            .capture_song_column_splines(capture, second)
            .unwrap();
    }
}

fn sample(samples: &[Sample], player: usize, column: usize, target: Target) -> f32 {
    samples
        .iter()
        .find(|sample| {
            sample.player == player && sample.column == column && sample.target == target
        })
        .unwrap_or_else(|| panic!("missing ({player}, {column}, {target:?}) in {samples:?}"))
        .value
}

fn track(tracks: &[Track], player: usize, column: usize) -> &Track {
    tracks
        .iter()
        .find(|track| track.player == player && track.column == column)
        .unwrap_or_else(|| panic!("missing track ({player}, {column}) in {tracks:?}"))
}

#[test]
fn chart_handlers_follow_destination_to_source_maps_for_both_players_and_capture_modes() {
    for first in [IDENTITY, MIRROR, LEFT, RIGHT] {
        for live in [false, true] {
            let maps = [first, RIGHT];
            let mut fixture = Fixture::new(maps, live);
            fixture.script(
                r#"
                for p,cols in ipairs(columns) do
                    for i,c in ipairs(cols) do
                        local n=p*100+i*10
                        configure(c:GetPosHandler(),'Position',false,
                            {{n,n+1,n+2},{n,n+1,n+2}},2,0.5)
                        configure(c:GetZoomHandler(),'Position',false,
                            {{n+3,n+4,n+5},{n+3,n+4,n+5}},3,-0.25)
                        configure(c:GetRotHandler(),'Offset',false,
                            {{0,0,n+6},{0,0,n+6}})
                    end
                end
                "#,
            );
            let samples = fixture.transforms();
            let mut capture = SongColumnSplineCapture::default();
            fixture.capture(&mut capture, 0.);
            let tracks = capture.finish().unwrap();
            assert_eq!(tracks.len(), 8);
            for (player, map) in maps.into_iter().enumerate() {
                for (column, source) in map.into_iter().enumerate() {
                    let n = ((player + 1) * 100 + (source + 1) * 10) as f32;
                    let frame = track(&tracks, player, column).at_second(0.).unwrap();
                    let position = frame.position.as_ref().unwrap();
                    assert_eq!(
                        position.coefficients.view().get(0).unwrap().map(|axis| axis[0]),
                        [n, n + 1., n + 2.]
                    );
                    assert_eq!((position.beats_per_t, position.receptor_t), (2., 0.5));
                    assert!(!position.subtract_song_beat);
                    let zoom = frame.zoom.as_ref().unwrap();
                    assert_eq!(
                        zoom.coefficients.view().get(0).unwrap().map(|axis| axis[0]),
                        [n + 3., n + 4., n + 5.]
                    );
                    assert_eq!((zoom.beats_per_t, zoom.receptor_t), (3., -0.25));
                    assert!(!zoom.subtract_song_beat);
                    assert_eq!(sample(&samples, player, column, Target::RotationZ), n + 6.);
                }
            }
        }
    }
}

#[test]
fn native_handler_precedence_is_independent_and_leaves_raw_handles_and_actor_state_physical() {
    let mut fixture = Fixture::new([MIRROR, IDENTITY], false);
    fixture.script(
        r#"
        local own,source=columns[1][1],columns[1][4]
        original=own:GetPosHandler()
        configure(original,'Offset',true,{{11,12,13},{11,12,13}})
        configure(source:GetPosHandler(),'Offset',false,{{44,45,46},{44,45,46}})
        configure(own:GetRotHandler(),'Offset',true,{{0,0,17},{0,0,17}})
        configure(source:GetRotHandler(),'Offset',true,{{0,0,47},{0,0,47}})
        configure(own:GetZoomHandler(),'Offset',true,{{9,9,9},{9,9,9}})
        configure(source:GetZoomHandler(),'Disabled',false,{})
        own:rotationz(71)
        assert(columns[1][1]==own and columns[1][4]==source)
        assert(own:GetPosHandler()==original)
        assert(original:GetSpline():Evaluate(0)[1]==11)
        assert(own:GetRotationZ()==71 and source:GetRotationZ()==0)
        "#,
    );
    let samples = fixture.transforms();
    assert_eq!(sample(&samples, 0, 0, Target::PositionEnabled), 1.);
    assert_eq!(
        sample(&samples, 0, 0, Target::PositionPoint { point: 0, axis: 0 }),
        44.
    );
    assert_eq!(sample(&samples, 0, 0, Target::PositionSubtractBeat), 0.);
    assert_eq!(sample(&samples, 0, 0, Target::RotationZ), 17.);
    assert_eq!(sample(&samples, 0, 0, Target::Zoom), 1.);
    // The outgoing chart position must reset to a real disabled baseline.
    assert_eq!(sample(&samples, 0, 3, Target::PositionEnabled), 0.);
    assert_eq!(sample(&samples, 0, 3, Target::OffsetX), 0.);
    assert_eq!(sample(&samples, 0, 3, Target::OffsetY), 0.);
    assert_eq!(sample(&samples, 0, 3, Target::RotationZ), 47.);
    assert_eq!(sample(&samples, 0, 3, Target::Zoom), 1.);
    fixture.script(
        "assert(columns[1][1]:GetPosHandler()==original and columns[1][1]:GetRotationZ()==71)",
    );
}

#[test]
fn receptor_relative_offsets_zoom_and_rotation_stay_on_physical_columns() {
    let mut fixture = Fixture::new([LEFT, MIRROR], true);
    fixture.script(
        r#"
        for p,cols in ipairs(columns) do
            for i,c in ipairs(cols) do
                local n=p*100+i*10
                configure(c:GetPosHandler(),'Offset',true,
                    {{n,n+1,0},{n,n+1,0},{n,n+1,0}})
                configure(c:GetZoomHandler(),'Offset',true,{{0.25,0.25,0.25}})
                configure(c:GetRotHandler(),'Offset',true,{{0,0,n+2}})
            end
        end
        "#,
    );
    let samples = fixture.transforms();
    for player in 0..2 {
        for column in 0..4 {
            let n = ((player + 1) * 100 + (column + 1) * 10) as f32;
            assert_eq!(sample(&samples, player, column, Target::OffsetX), n);
            assert_eq!(sample(&samples, player, column, Target::OffsetY), n + 1.);
            assert_eq!(sample(&samples, player, column, Target::Zoom), 1.25);
            assert_eq!(sample(&samples, player, column, Target::RotationZ), n + 2.);
            assert_eq!(
                sample(&samples, player, column, Target::PositionEnabled),
                0.
            );
        }
    }
}

#[test]
fn chart_zoom_hide_windows_move_with_notes_and_stop_when_handler_becomes_relative() {
    let mut fixture = Fixture::new([MIRROR, LEFT], false);
    fixture.script(
        r#"
        hide=columns[1][2]:GetZoomHandler()
        configure(hide,'Offset',false,{{0,0,0},{-1,-1,-1},{-1,-1,-1},{0,0,0}},2)
        configure(columns[1][3]:GetZoomHandler(),'Offset',true,{{0.25,0.25,0.25}})
        "#,
    );
    let hides = fixture.runtime.read_song_note_column_zoom_hides().unwrap();
    assert_eq!(hides.len(), 1);
    assert_eq!((hides[0].player, hides[0].column), (0, 2));
    assert_eq!((hides[0].start_beat, hides[0].end_beat), (2., 4.));
    assert_eq!((hides[0].spline_beats_per_t, hides[0].spline_size), (2., 4));
    assert_eq!(sample(&fixture.transforms(), 0, 1, Target::Zoom), 1.);

    fixture.script("hide:SetSubtractSongBeat(true)");
    assert!(
        fixture
            .runtime
            .read_song_note_column_zoom_hides()
            .unwrap()
            .is_empty()
    );
    assert_eq!(sample(&fixture.transforms(), 0, 2, Target::Zoom), 1.25);

    fixture.script(
        "hide:SetSubtractSongBeat(false); hide:SetSplineMode('NoteColumnSplineMode_Disabled')",
    );
    assert!(
        fixture
            .runtime
            .read_song_note_column_zoom_hides()
            .unwrap()
            .is_empty()
    );
    assert_eq!(sample(&fixture.transforms(), 0, 2, Target::Zoom), 1.);
}

#[test]
fn changing_raw_handler_timing_reselects_tracks_without_rebinding_public_columns() {
    let mut fixture = Fixture::new([MIRROR, IDENTITY], true);
    fixture.script(
        r#"
        h=columns[1][2]:GetPosHandler()
        configure(h,'Position',false,{{22,0,0},{22,0,0}})
        configure(columns[1][3]:GetPosHandler(),'Position',true,{{33,0,0},{33,0,0}})
        "#,
    );
    let mut capture = SongColumnSplineCapture::default();
    fixture.capture(&mut capture, 0.);
    fixture.script("h:SetSubtractSongBeat(true); assert(columns[1][2]:GetPosHandler()==h)");
    fixture.capture(&mut capture, 1.);
    let tracks = capture.finish().unwrap();
    assert_eq!(tracks.len(), 2);
    let down = track(&tracks, 0, 1);
    assert!(down.at_second(0.).unwrap().position.is_none());
    let down = down.at_second(1.).unwrap().position.as_ref().unwrap();
    assert_eq!(down.coefficients[0][0][0], 22.);
    assert!(down.subtract_song_beat);
    let up = track(&tracks, 0, 2);
    let incoming = up.at_second(0.).unwrap().position.as_ref().unwrap();
    assert_eq!(incoming.coefficients[0][0][0], 22.);
    assert!(!incoming.subtract_song_beat);
    let own = up.at_second(1.).unwrap().position.as_ref().unwrap();
    assert_eq!(own.coefficients[0][0][0], 33.);
    assert!(own.subtract_song_beat);
}
