use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    self as song, SongLuaCompileContext, SongLuaLiveFrame,
    rust_engine::{RustSongRuntime, SongLayerPlan, SongLiveFrames},
};
use std::sync::Arc;

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
    rust: RustSongRuntime,
    frames: SongLiveFrames,
    context: SongLuaCompileContext,
    screen: song::rust_engine::ActorScene,
    _directory: tempfile::TempDir,
}
impl Pair {
    fn new(configure: impl FnOnce(&mut SongLuaCompileContext)) -> Self {
        Self::with_source("return Def.ActorFrame{}", configure)
    }
    fn with_source(source: &str, configure: impl FnOnce(&mut SongLuaCompileContext)) -> Self {
        let directory = tempfile::tempdir().unwrap();
        reference_test::register_path(directory.path());
        let path = directory.path().join("default.lua");
        std::fs::write(&path, source).unwrap();
        let mut context = SongLuaCompileContext::new(directory.path(), "live frame phases");
        context.live = true;
        context.players[1].enabled = true;
        context.music_length_seconds = 2.;
        configure(&mut context);

        let mut rust = RustSongRuntime::new(&context).unwrap();
        rust.begin_song_compilation().unwrap();
        let (screen, selected) = rust.construct_gameplay_screen(&context).unwrap();
        rust.load_gameplay_theme(screen, &selected).unwrap();
        let plan = SongLayerPlan::new(&[&path], 0, &context).unwrap();
        rust.initialize_song_layers(screen, &plan).unwrap().unwrap();
        rust.activate_song_layers(screen, &selected)
            .unwrap()
            .unwrap();
        let frames = rust.new_song_live_frames(&selected).unwrap();
        rust.begin_actor_playback();
        let mut pair = Self {
            rust,
            frames,
            context: selected,
            screen,
            _directory: directory,
        };
        pair.run("S=STATSMAN:GetCurStageStats(); P={S:GetPlayerStageStats(0),S:GetPlayerStageStats(1)}; EVENTS={}");
        pair.compare(SNAPSHOT);
        pair
    }
    fn compare(&mut self, source: &str) {
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
        reference_test::assert_debug("song/tests/rust_live_frames.rs::observations", &actual);
    }
    fn run(&mut self, source: &str) {
        self.compare(source)
    }
    fn prepare(&mut self, frame: &SongLuaLiveFrame, deltas: Option<[f32; 3]>) {
        self.frames
            .prepare_gameplay_frame(&mut self.rust, frame, deltas, false)
            .unwrap();
        self.compare(SNAPSHOT);
    }
    fn update(&mut self, frame: &SongLuaLiveFrame, deltas: Option<[f32; 3]>) {
        self.frames
            .update(&mut self.rust, frame, deltas, false)
            .unwrap();
        self.frames.finish_health_update(&mut self.rust).unwrap();
        self.compare(SNAPSHOT);
    }
    fn health(&mut self) {
        let rust = self.frames.finish_health_update(&mut self.rust);
        reference_test::assert_debug(
            "song/tests/rust_live_frames.rs::health_error",
            &rust.is_err(),
        );
        self.compare(SNAPSHOT);
    }
}
impl Drop for Pair {
    fn drop(&mut self) {
        self.frames.release(&mut self.rust).unwrap();
        self.rust.end_actor_scene(self.screen).unwrap();
        self.rust.release_actor_scene(self.screen).unwrap();
    }
}
const SNAPSHOT: &str = r#"
local out={};local n=0
local function add(v)n=n+1;out[n]=v==nil and 'nil' or v end
local r=__songlua_compile_song_runtime
for _,k in ipairs{'__songlua_song_beat','__songlua_music_seconds','__songlua_song_bps','__songlua_song_delta_beat','__songlua_music_delta_seconds','__songlua_uptime_seconds','__songlua_freeze','__songlua_delay'}do add(r[k])end
local p=r.__songlua_player_positions
for i=1,2 do
 for _,k in ipairs{'__songlua_song_beat','__songlua_music_seconds','__songlua_song_bps','__songlua_freeze','__songlua_delay'}do add(p[i][k])end
 local ps=GAMESTATE:GetPlayerState(i-1)
 add(ps:GetPlayerController());add(ps:GetHealthState())
 for _,level in ipairs{'ModsLevel_Stage','ModsLevel_Song','ModsLevel_Current'}do
  local po=ps:GetPlayerOptions(level);add(po:Mini());add(po:Mirror())
 end
 local stats=P[i]
 for _,key in ipairs{'__songlua_possible_dance_points','__songlua_actual_dance_points','__songlua_current_possible_dance_points','__songlua_failed','__songlua_life','__songlua_combo','__songlua_miss_combo'}do add(stats[key])end
 for _,name in ipairs{'W1','W2','Miss'}do add(stats.__songlua_tap_scores[name])end
 add(stats.__songlua_life_meter and stats.__songlua_life_meter.__songlua_life)
end
add(S.__songlua_gameplay_seconds)
add(table.concat(EVENTS,';'))
return unpack(out,1,n)
"#;
fn frame(seconds: f64) -> SongLuaLiveFrame {
    SongLuaLiveFrame {
        beat: seconds * 2.,
        music_seconds: seconds,
        screen_seconds: seconds.max(0.),
        delta_seconds: 1. / 60.,
        players: [song::SongLuaLivePlayer {
            possible_points: 100,
            actual_points: 75,
            current_possible_points: 80,
            life: 0.5,
            fail_enabled: true,
            ..Default::default()
        }; 2],
        ..Default::default()
    }
}

#[test]
fn live_frame_clock_statistics_feedback_and_uptime_match() {
    let mut p = Pair::new(|_| {});
    for (i, seconds) in [0., 0.016666666667, 0.123456789123, 0.5, 1.]
        .into_iter()
        .enumerate()
    {
        let mut f = frame(seconds);
        f.players[0].actual_points += i as i32;
        f.players[0].combo = i as u32;
        f.players[1].tap_scores[9] = i as u32;
        f.players[1].autoplay = i % 2 == 1;
        f.players[0].life_record_latest = Some(0.9);
        f.uptime_seconds = (i == 3).then_some(50.);
        p.update(&f, None);
    }
}
#[test]
fn prepared_frame_refreshes_feedback_without_reapplying_options_or_uptime() {
    let mut p = Pair::new(|_| {});
    p.run("GAMESTATE:GetPlayerState(0):GetPlayerOptions('ModsLevel_Song'):Mini(1,0.5)");
    let mut f = frame(0.3);
    p.prepare(&f, None);
    p.prepare(&frame(0.7), Some([0.9; 3]));
    f.players[0].combo = 27;
    f.players[0].life = 0.8;
    f.uptime_seconds = Some(999.);
    p.update(&f, None);
    p.run("assert(__songlua_compile_song_runtime.__songlua_uptime_seconds~=999)");
}
#[test]
fn frozen_music_keeps_independent_uptime_and_option_clock_deltas() {
    let mut p = Pair::new(|_| {});
    p.run("GAMESTATE:GetPlayerState(0):GetPlayerOptions('ModsLevel_Song'):Mini(1,0.5)");
    for i in 0..4 {
        let mut f = frame(0.);
        f.screen_seconds = i as f64 / 10.;
        p.update(&f, Some([0.1, 0.2, 0.3]));
    }
}
#[test]
fn song_and_selected_player_timings_use_the_same_audio_position() {
    use deadsync_rules::timing::{TimingData, TimingSegments};
    let mut p = Pair::new(|c| {
        c.song_music_rate = 1.7;
        c.song_timing = Some(TimingData::from_segments(
            1.25,
            0.,
            &TimingSegments {
                bpms: vec![(0., 120.), (0.5, 180.)],
                ..Default::default()
            },
            &[],
        ));
        c.player_timing[0] = Some(TimingData::from_segments(
            -0.3,
            0.,
            &TimingSegments {
                bpms: vec![(0., 90.)],
                ..Default::default()
            },
            &[],
        ));
    });
    for seconds in [0., 0.1, 0.5, 1., 2.] {
        p.update(&frame(seconds), None);
    }
}
#[test]
fn attack_activation_uses_prior_player_position_and_strict_endpoints() {
    let mut p = Pair::new(|_| {});
    let windows: Arc<[deadsync_gameplay::ChartAttackWindow]> =
        Arc::from([deadsync_gameplay::ChartAttackWindow {
            start_second: 0.1,
            len_seconds: 0.3,
            mods: "50% Mini, Mirror".into(),
        }]);
    for seconds in [0., 0.1, 0.2, 0.4, 0.5] {
        let mut f = frame(seconds);
        f.chart_attacks = Some([windows.clone(), Arc::from([])]);
        p.update(&f, None);
    }
}
#[test]
fn replacement_attack_schedule_rebuilds_even_when_active_indices_match() {
    let mut p = Pair::new(|_| {});
    for (i, mods) in ["25% Mini", "75% Mini", ""].into_iter().enumerate() {
        let mut f = frame(0.1 + i as f64 * 0.1);
        f.chart_attacks = Some([
            Arc::from([deadsync_gameplay::ChartAttackWindow {
                start_second: -1.,
                len_seconds: 3.,
                mods: mods.into(),
            }]),
            Arc::from([]),
        ]);
        p.update(&f, None);
    }
}
#[test]
fn retained_statistics_survive_global_replacement_and_gc() {
    let mut p = Pair::new(|_| {});
    p.run("STATSMAN={GetCurStageStats=function()error('must retain stage')end};collectgarbage()");
    p.update(&frame(0.2), None);
}
#[test]
fn disabled_player_position_stays_owned_while_statistics_still_feed() {
    let mut p = Pair::new(|c| c.players[1].enabled = false);
    p.update(&frame(0.2), None);
}
const HEALTH: &str = r#"
return Def.ActorFrame{HealthStateChangedMessageCommand=function(self,p)
 table.insert(EVENTS,p.PlayerNumber..':'..p.OldHealthState..':'..p.HealthState)
 if p.PlayerNumber=='PlayerNumber_P1' and TOUCH_P2 then
  SCREENMAN:GetTopScreen():GetChild('PlayerP2'):SetLife(0.05)
 end
end}
"#;
#[test]
fn health_thresholds_and_fail_off_match_native_phase() {
    let mut p = Pair::with_source(HEALTH, |_| {});
    for (i, life) in [1., 0.5, 0.1, 0., 0.4].into_iter().enumerate() {
        let mut f = frame(i as f64 / 10.);
        f.players[0].life = life;
        p.update(&f, None);
    }
    p.run("GAMESTATE:GetPlayerState(0):GetPlayerOptions('ModsLevel_Current'):FailSetting('FailType_Off')");
    let mut players = frame(0.6).players;
    players[0].life = 0.;

    p.rust.publish_live_players(players);
    p.health();
    p.run("assert(GAMESTATE:GetPlayerState(0):GetHealthState()=='HealthState_Alive')");
}
#[test]
fn health_notification_can_write_the_following_player_before_its_health_read() {
    let mut p = Pair::with_source(HEALTH, |_| {});
    p.run("TOUCH_P2=true");
    let mut f = frame(0.1);
    f.players[0].life = 1.;
    p.prepare(&f, None);
    p.health();

    let b = p.rust.take_player_writes();
    reference_test::assert_debug("song/tests/rust_live_frames.rs::player_writes", &b);
    p.run("assert(#EVENTS==2)");
}
#[test]
fn health_partial_errors_keep_prior_status_and_recover_without_duplicate_notifications() {
    let mut p = Pair::with_source(HEALTH, |_| {});
    let mut f = frame(0.1);
    f.players[0].life = 1.;
    f.players[1].life = 0.1;
    p.prepare(&f, None);
    p.run("GAMESTATE:GetPlayerState(1):GetPlayerOptions('ModsLevel_Current').__songlua_player_option_state.failsetting=999");
    p.health();
    p.run("assert(#EVENTS==1)");
    p.run("GAMESTATE:GetPlayerState(1):GetPlayerOptions('ModsLevel_Current').__songlua_player_option_state.failsetting=0;collectgarbage()");
    p.health();
    p.run("assert(#EVENTS==2)");
}
#[test]
fn foreign_runtime_is_rejected_without_consuming_live_owners() {
    let mut p = Pair::new(|_| {});
    let mut foreign = RustSongRuntime::new(&SongLuaCompileContext::new(".", "foreign")).unwrap();
    assert!(
        p.frames
            .update(&mut foreign, &frame(0.1), None, false)
            .is_err()
    );
    assert!(p.frames.release(&mut foreign).is_err());
    p.update(&frame(0.1), None);
}

#[test]
fn disabled_following_player_keeps_health_fallback_after_nested_life_write() {
    let mut p = Pair::with_source(HEALTH, |c| c.players[1].enabled = false);
    p.run("TOUCH_P2=true");
    let mut f = frame(0.1);
    f.players[0].life = 1.;
    p.prepare(&f, None);
    p.health();
    p.run("assert(#EVENTS==1)");
}

#[test]
fn rejected_option_clock_keeps_prior_raw_writes_and_next_frame_recovers() {
    let mut p = Pair::new(|_| {});
    p.update(&frame(0.2), None);
    let f = frame(0.1);

    let rust = p
        .frames
        .prepare_gameplay_frame(&mut p.rust, &f, None, false);
    assert!(rust.is_err());
    p.compare(SNAPSHOT);
    p.update(&frame(0.3), None);
}

#[test]
fn explicit_option_return_values_use_the_same_chunk_execution_mode() {
    let mut p = Pair::new(|_| {});
    p.compare("return GAMESTATE:GetPlayerState(0):GetPlayerOptions('ModsLevel_Song'):Mini(1,.5)");
    p.compare("return GAMESTATE:GetPlayerState(0):GetPlayerOptions('ModsLevel_Current'):FailSetting('FailType_Off')");
}

#[test]
fn original_life_meter_call_keeps_prior_health_event_and_reports_unsupported_method() {
    let source = HEALTH.replace("'PlayerP2'", "'LifeP2'");
    let mut p = Pair::with_source(&source, |_| {});
    p.run("TOUCH_P2=true");
    let mut f = frame(0.1);
    f.players[0].life = 1.;
    p.prepare(&f, None);

    let b = p.frames.finish_health_update(&mut p.rust);

    assert!(b.unwrap_err().to_string().contains("SetLife"));
    p.compare(SNAPSHOT);
    p.run("assert(#EVENTS==1)");
}

#[test]
fn released_live_owners_reject_use_and_new_owner_resets_health_publication() {
    let mut p = Pair::with_source(HEALTH, |_| {});
    let mut f = frame(0.1);
    f.players[0].life = 1.;
    p.update(&f, None);
    p.frames.release(&mut p.rust).unwrap();
    assert!(p.frames.update(&mut p.rust, &f, None, false).is_err());

    p.frames = p.rust.new_song_live_frames(&p.context).unwrap();
    p.compare(SNAPSHOT);
    p.health();
    p.run("assert(#EVENTS==2)");
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
