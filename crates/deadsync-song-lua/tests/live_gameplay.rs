use deadlib_lua::{Value, compile_named};
use deadsync_gameplay::{self as game, GameplayNoteFieldTap as Tap};
use deadsync_rules::{
    judgment::{JudgeGrade, Judgment},
    note::HoldResult,
};
use deadsync_song_lua as song;
use std::{path::Path, sync::Arc};
#[path = "support/paths.rs"]
mod paths;
type Game = game::GameplayRuntimeState<deadsync_profile_gameplay::GameplayProfile, (), (), ()>;
struct NoLua;
impl game::SongLuaRuntimeBuilder<(), (), ()> for NoLua {
    fn build_song_lua_runtime(
        self,
        p: game::SongLuaRuntimeWindowBuild<'_>,
    ) -> game::SongLuaRuntimeBuildOutput<(), (), ()> {
        (
            Default::default(),
            Default::default(),
            game::SongLuaRuntimeVisuals {
                overlays: vec![],
                screen_overlay_index: None,
                overlay_eases: vec![],
                overlay_ease_ranges: vec![],
                overlay_events: vec![],
                background_visual_layers: vec![],
                foreground_visual_layers: vec![],
                player_actors: [(); 2],
                player_events: Default::default(),
                player_judgment_events: Default::default(),
                player_combo_events: Default::default(),
                song_foreground: (),
                song_foreground_events: vec![],
                hidden_players: [false; 2],
                screen_layers: [(); 4],
                screen_layer_events: Default::default(),
                note_hides: Default::default(),
                column_offsets: Default::default(),
                column_splines: Default::default(),
                screen_width: p.screen_width,
                screen_height: p.screen_height,
            },
        )
    }
}
fn game(dir: &Path) -> Game {
    paths::init_paths();
    let p = dir.join("callbacks.ssc");
    std::fs::write(&p,"#TITLE:Callbacks;\n#BPMS:0=120;\n#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Challenge;\n#METER:1;\n#NOTES:\n2000\n0100\n0010\n3001\n,\n000M\n0000\n0000\n0000\n;\n").unwrap();
    let song = Arc::new(deadsync_simfile::app_runtime::parse_song_for_test(&p, 0.).unwrap());
    let chart = Arc::new(song.charts[0].clone());
    let data = Arc::new(
        deadsync_simfile::app_runtime::load_gameplay_charts(&song, &[0], 0.)
            .unwrap()
            .remove(0),
    );
    game::init_gameplay_runtime(
        song,
        [chart.clone(), chart],
        [data.clone(), data],
        game::GameplayViewport::new(640., 480.),
        game::GameplaySession {
            play_style: game::GameplayInputPlayStyle::Versus,
            ..Default::default()
        },
        game::GameplayConfig::default(),
        deadsync_chart::SyncPref::Default,
        Default::default(),
        Default::default(),
        NoLua,
        |_, _, _, _, _| vec![],
        5,
        1.,
        [deadsync_rules::scroll::ScrollSpeedSetting::XMod(1.); 2],
        std::array::from_fn(|_| {
            deadsync_profile_gameplay::GameplayProfile::from(deadsync_profile::Profile::default())
        }),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        [game::CourseLifeConfig::Bar; 2],
        false,
        [0; 2],
    )
}
fn slots(_: &Path, _: &Path, _: &Path) -> Result<Arc<[()]>, String> {
    Ok(Arc::from([]))
}
fn layer(_: &()) -> Option<song::SongLuaOverlayModelLayer<()>> {
    None
}
struct Pair {
    rust: Option<song::SongLiveScene>,
    r: Game,
    _dir: tempfile::TempDir,
}
impl Drop for Pair {
    fn drop(&mut self) {
        if let Some(s) = &mut self.rust {
            let _ = s.end();
        }
    }
}
const LUA: &str = r#"
Log={};SavedNotes={}
function add(...) local a={...};for i=1,select('#',...) do a[i]=tostring(a[i]) end;Log[#Log+1]=table.concat(a,'|') end
function note(n)
 local r=n:GetTapNoteResult()
 add('note',n:GetTapNoteType(),n:GetTapNoteSubType(),n:GetTapNoteSource(),n:GetPlayerNumber(),n:GetHoldDuration(),n:GetKeysoundIndex(),r:GetTapNoteScore(),r:GetTapNoteOffset(),r:GetHeld(),r:GetHidden(),n:GetTapNoteResult()==n:GetHoldNoteResult())
 SavedNotes[#SavedNotes+1]=n
end
return Def.ActorFrame{
 InitCommand=function(self)
  Root=self;Player=SCREENMAN:GetTopScreen():GetChild('PlayerP1');Stage=STATSMAN:GetCurStageStats();Stats=Stage:GetPlayerStageStats('PlayerNumber_P1')
  Fields={SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField'),SCREENMAN:GetTopScreen():GetChild('PlayerP2'):GetChild('NoteField')}
  for i,f in ipairs(Fields) do f:SetDidTapNoteCallback(function(c,s,b) add('tap',i,c,s,b); if Tap then return Tap(c,s,b) end end) end
 end,
 StepMessageCommand=function(self,p)
  add('Step',p.PlayerNumber,p.Column,p.MultiPlayer,Stats:GetCurrentCombo(),Stats:GetCurrentLife(),Stage:GetGameplaySeconds(),GAMESTATE:GetSongBeat(),GAMESTATE:GetSongPosition():GetMusicSeconds(),GetTimeSinceStart())
  MESSAGEMAN:Broadcast('Nested',{value=p.Column})
  if Step then Step(p) end
 end,
 StepP1MessageCommand=function(self,p)add('StepP1',next(p)==nil)end,
 StepP2MessageCommand=function(self,p)add('StepP2',next(p)==nil)end,
 NestedMessageCommand=function(self,p)add('nested',p.value)end,
 JudgmentMessageCommand=function(self,p)
  add('Judgment',p.Player,p.FirstTrack,p.NumTracks,p.TapNoteScore,p.HoldNoteScore,p.TapNoteOffset,p.Early,p.EarlyTapNoteScore,p.EarlyTapNoteOffset,p.HeldMiss)
  if p.Notes then for i=1,4 do if p.Notes[i] then add('track',i);note(p.Notes[i]) end end end
  if p.Holds then for i=1,4 do if p.Holds[i] then add('hold',i);note(p.Holds[i]) end end end
  if p.TapNote then note(p.TapNote) end
  if Judge then Judge(p) end
 end,
 EndCommand=function(self)add('End')end,
}
"#;
impl Pair {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        reference_test::register_path(dir.path());

        let r = game(dir.path());
        let path = dir.path().join("callbacks.lua");
        std::fs::write(&path, LUA).unwrap();
        let mut c = song::SongLuaCompileContext::new(dir.path(), "gameplay callbacks");
        c.live = true;
        c.players[1].enabled = true;
        c.song_timing = Some((*r.timing_runtime.timing).clone());

        let mut ro = song::compile_song_lua_layers(
            &[&path],
            0,
            &c,
            Default::default(),
            slots,
            layer,
            |_, _| None,
        )
        .unwrap();
        let mut s = Self {
            rust: ro.iter_mut().find_map(|o| o.live_scene.take()),

            r,
            _dir: dir,
        };

        let b = s.rust.as_mut().unwrap().take_startup_output();
        s.output(b);
        s.install();
        s
    }
    fn install(&mut self) {
        let r = self.rust.as_ref().unwrap();

        r.install_note_field_tap_callback(&mut self.r, 10).unwrap();

        r.install_step_callback(&mut self.r).unwrap();

        r.install_tap_row_judgment_callback(&mut self.r).unwrap();

        r.install_hold_judgment_callback(&mut self.r).unwrap();

        r.install_avoided_mine_judgment_callback(&mut self.r)
            .unwrap();
    }
    fn run(&mut self, source: &str) {
        let mut runtime = self.rust.as_mut().unwrap().runtime_mut();
        let vm = runtime.vm_mut();
        let v = vm
            .execute(&compile_named(source.as_bytes(), b"@/gameplay-check.lua").unwrap())
            .unwrap()
            .to_vec();
        let r = v
            .into_iter()
            .map(|v| match v {
                Value::Nil => "nil".into(),
                Value::Boolean(b) => b.to_string(),
                Value::Number(n) => format!("{:016x}", n.to_bits()),
                Value::String(id) => format!("{:?}", vm.string_bytes(id).unwrap()),
                v => panic!("scalar {v:?}"),
            })
            .collect::<Vec<String>>();
        reference_test::assert_debug("song/tests/rust_live_gameplay.rs::observations", &r);
    }
    fn logs(&mut self) {
        self.run("return table.concat(Log,'\\n')")
    }
    fn output(&self, output: song::SongLuaLiveOutput) {
        reference_test::assert_debug(
            "song/tests/rust_live_gameplay.rs::output",
            &live_snapshot::output(&output),
        );
        assert!(output.actor_sounds.is_empty());
    }
    fn flush(&mut self) {
        let frame = song::SongLuaLiveFrame::from_gameplay(&self.r, 0.);

        let r = self.rust.as_mut().unwrap().update(frame).unwrap();
        self.output(r);
        self.logs();
        self.game_state();
    }
    fn step(&mut self, column: usize) {
        for g in [&mut self.r] {
            let mut cb = g.step_callback.take().unwrap();
            cb(g, column);
            g.step_callback = Some(cb);
        }
        self.logs();
        self.game_state();
    }
    fn tap(&mut self, tap: Tap) -> Tap {
        let mut callback = self.r.note_field_tap_callback.take().unwrap();
        let result = callback(&mut self.r, tap);
        self.r.note_field_tap_callback = Some(callback);
        reference_test::assert_debug("song/tests/rust_live_gameplay.rs::tap", &result);
        self.logs();
        self.game_state();
        result
    }
    fn row(&mut self, player: usize, row: usize, j: Judgment) {
        for g in [&mut self.r] {
            let mut cb = g.tap_row_judgment_callback.take().unwrap();
            cb(g, player, row, j);
            g.tap_row_judgment_callback = Some(cb);
        }
        self.logs();
        self.game_state();
    }
    fn hold(&mut self, player: usize, note: usize, result: HoldResult) {
        for g in [&mut self.r] {
            let mut cb = g.hold_judgment_callback.take().unwrap();
            cb(g, player, note, result);
            g.hold_judgment_callback = Some(cb);
        }
        self.logs();
        self.game_state();
    }
    fn mine(&mut self, player: usize, column: usize) {
        for g in [&mut self.r] {
            let mut cb = g.avoided_mine_judgment_callback.take().unwrap();
            cb(g, player, column);
            g.avoided_mine_judgment_callback = Some(cb);
        }
        self.logs();
        self.game_state();
    }
    fn game_state(&self) {
        for index in 0..2 {
            let player = &self.r.players_runtime.players[index];
            reference_test::assert_debug(
                "song/tests/rust_live_gameplay.rs::player_state",
                &(index, player.life.to_bits(), player),
            );
        }
        reference_test::assert_debug(
            "song/tests/rust_live_gameplay.rs::fail_types",
            &self.r.mods.song_lua_fail_types,
        );
    }
}
fn judgment(grade: JudgeGrade, ms: f32, held: bool) -> Judgment {
    Judgment {
        grade,
        time_error_ms: ms,
        time_error_music_ns: (ms as f64 * 1_000_000.) as i64,
        window: None,
        miss_because_held: held,
    }
}
fn tap(player: usize) -> Tap {
    Tap {
        player,
        column: 1,
        score: "W3",
        bright: false,
    }
}

#[test]
fn step_messages_refresh_stats_clocks_and_reenter_lua_without_advancing_uptime() {
    let mut p = Pair::new();
    p.run("assert(GetTimeSinceStart()==0);Step=function() Player:SetLife(0.375) end");
    for g in [&mut p.r] {
        g.players_runtime.players[0].combo = 27;
        g.clock.song_position.current_music_time_ns = 1_250_000_000;
        g.boundary.total_elapsed_in_screen = 3.25;
    }
    p.step(2);
    p.step(7);
    assert_eq!(p.r.players_runtime.players[0].life, 0.375);
    p.run("assert(#Log==6)");
    p.run("assert(GetTimeSinceStart()==0);return Stats:GetCurrentCombo(),Stats:GetCurrentLife(),table.concat(Log,'\\n')");
    p.flush();
}
#[test]
fn row_judgments_include_current_notes_preceding_holds_and_retained_results() {
    let mut p = Pair::new();
    let j = judgment(JudgeGrade::Great, -24., false);
    let early = judgment(JudgeGrade::WayOff, -100., true);
    for g in [&mut p.r] {
        for n in &mut g.chart_runtime.notes {
            if n.column % 4 == 1 {
                n.result = Some(j);
                n.early_result = Some(early);
            }
        }
    }
    for player in 0..2 {
        let row =
            p.r.chart_runtime
                .row_entries
                .iter()
                .position(|r| {
                    r.note_indices().iter().any(|i| {
                        let n = &p.r.chart_runtime.notes[i.get()];
                        n.column == player * 4 + 1
                    })
                })
                .unwrap();
        p.row(player, row, j);
    }
    p.run("assert(#SavedNotes>=4);collectgarbage('collect');for i=1,#SavedNotes do note(SavedNotes[i]) end;return #SavedNotes");
    p.flush();
}
#[test]
fn held_misses_and_all_hold_results_keep_native_unormalized_parameter_shapes() {
    let mut p = Pair::new();
    let j = judgment(JudgeGrade::Miss, 180., true);
    let row = 0;
    p.row(0, row, j);
    for player in 0..2 {
        let ni =
            p.r.chart_runtime
                .notes
                .iter()
                .position(|n| n.column == player * 4 && n.hold.is_some())
                .unwrap();
        for g in [&mut p.r] {
            g.chart_runtime.notes[ni].result = Some(j);
            g.chart_runtime.notes[ni].early_result =
                Some(judgment(JudgeGrade::Decent, -80., false));
        }
        for result in [HoldResult::Held, HoldResult::LetGo, HoldResult::Missed] {
            p.hold(player, ni, result);
        }
    }
    p.mine(0, 3);
    p.mine(1, 6);
    p.run("assert(#SavedNotes>=7);return table.concat(Log,'\\n')");
    p.flush();
}
#[test]
fn did_tap_note_preserves_independent_returns_brightness_and_error_suppression() {
    let mut p = Pair::new();
    for g in [&mut p.r] {
        g.players_runtime.players[0].combo = 11;
    }
    assert!(p.tap(tap(0)).bright);
    let mine = Tap {
        score: "HitMine",
        bright: true,
        ..tap(0)
    };
    assert!(!p.tap(mine).bright);
    for ret in [
        "return 3.9,'TapNoteScore_W1',true",
        "return '2.75','w2',false",
        "return nil,9,nil",
        "return 0,'no score',17",
        "return 5,99,true",
        "return {},{},false",
        "return 0/0,nil,true",
        "return 1/0,nil,false",
        "return 0.5,nil,nil",
        "return 4.99,nil,nil",
    ] {
        p.run(&format!("Tap=function(c,s,b) {ret} end"));
        p.tap(tap(0));
    }
    p.run("Tap=function() Player:SetLife(0.25);error('tap boom') end");
    assert_eq!(p.tap(tap(0)), tap(0));
    assert_eq!(p.tap(tap(0)), tap(0));
    p.run("assert(Fields[1].__songlua_did_tap_note_column_error_reported and Fields[1].__songlua_did_tap_note_score_error_reported and Fields[1].__songlua_did_tap_note_error_reported)");
    p.flush();
}
#[test]
fn callback_failures_preserve_prior_writes_and_prepared_frame() {
    let mut p = Pair::new();
    let frame = song::SongLuaLiveFrame::from_gameplay(&p.r, 0.125);

    p.rust
        .as_mut()
        .unwrap()
        .prepare_gameplay_frame(frame.clone())
        .unwrap();
    p.run("Step=function()Player:SetLife(0.2);error('step boom')end;Judge=function()Player:SetLife(0.3);error('judge boom')end");
    p.step(0);
    p.step(4);
    p.mine(0, 0);
    p.mine(0, 1);
    p.run("return GetTimeSinceStart()");
    p.flush();
}
#[test]
fn pending_output_writes_and_song_fail_policy_follow_current_only_precedence() {
    let mut p = Pair::new();
    for script in [
        "",
        "GAMESTATE:GetPlayerState('PlayerNumber_P1'):GetPlayerOptions('ModsLevel_Current'):FailSetting('FailType_Off')",
        "GAMESTATE:GetPlayerState('PlayerNumber_P1'):GetPlayerOptions('ModsLevel_Song'):FailSetting('FailType_EndOfSong')",
        "GAMESTATE:GetPlayerState('PlayerNumber_P2'):GetPlayerOptions('ModsLevel_Song'):FailSetting('FailType_Off')",
    ] {
        p.run(script);

        p.rust
            .as_ref()
            .unwrap()
            .prepare_gameplay_player_options(&mut p.r)
            .unwrap();
        p.game_state();
    }
    p.run("Player:SetLife(0.45);Stats:FailPlayer()");

    p.rust
        .as_ref()
        .unwrap()
        .apply_pending_player_writes(&mut p.r);
    p.game_state();
    let output = song::SongLuaLiveOutput {
        player_writes: vec![
            song::SongLuaLivePlayerWrite::RecordLife {
                player: 0,
                life: 0.7,
            },
            song::SongLuaLivePlayerWrite::SetLife {
                player: 1,
                life: 0.6,
            },
        ],
        ..Default::default()
    };

    p.rust
        .as_ref()
        .unwrap()
        .apply_output_player_writes(&output, &mut p.r);
    p.game_state();
    p.flush();
}
#[test]
fn installed_callbacks_survive_collection_reload_and_loading_thread_handoff() {
    let mut p = Pair::new();
    p.run("OLD_STATS=Stats;OLD_FIELD=Fields[1];collectgarbage('collect')");
    p.step(0);
    p.tap(tap(1));

    let r = p.rust.take().unwrap();

    let mut ro = song::reload_song_lua(r, slots, layer).unwrap();

    p.rust = ro.iter_mut().find_map(|o| o.live_scene.take());

    let r = p.rust.as_mut().unwrap().take_startup_output();
    p.output(r);
    // The old callbacks still own the original selected statistics and fields.
    p.run("collectgarbage('collect');OLD_FIELD:SetDidTapNoteCallback(function(c,s,b)add('old',c,s,b);return 4,'Miss',true end)");
    assert_eq!(p.tap(tap(0)).column, 3);
    p.step(5);
    p.flush();
    p.install();
    p.run("collectgarbage('collect')");
    p.step(1);
    p.tap(tap(0));
    p.flush();
    let mut r = p.rust.take().unwrap();
    let mut game = std::mem::replace(&mut p.r, game(p._dir.path()));
    (r, game) = std::thread::spawn(move || {
        let mut cb = game.step_callback.take().unwrap();
        cb(&mut game, 1);
        game.step_callback = Some(cb);
        r.runtime_mut()
            .vm_mut()
            .execute(&compile_named(b"collectgarbage('collect')", b"=thread GC").unwrap())
            .unwrap();
        (r, game)
    })
    .join()
    .unwrap();
    let mut cb = game.step_callback.take().unwrap();
    cb(&mut game, 2);
    game.step_callback = Some(cb);
    r.end().unwrap();
}
#[test]
fn gameplay_judgment_pipeline_uses_installed_callbacks() {
    let mut p = Pair::new();
    for column in [0, 1, 4, 5, 2, 6] {
        let (i, _) =
            p.r.chart_runtime
                .notes
                .iter()
                .enumerate()
                .find(|(_, n)| n.column == column)
                .unwrap();
        let ns = p.r.chart_runtime.note_time_cache_ns[i];

        let r = p.r.judge_a_tap(column, ns);
        reference_test::assert_debug("song/tests/rust_live_gameplay.rs::judge", &r);
        p.logs();
        p.game_state();
    }
    p.flush();
}

#[test]
fn retained_callbacks_own_the_vm_after_scene_drop_and_release_it_on_teardown() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Sentinel(Arc<AtomicUsize>);
    impl Drop for Sentinel {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let mut p = Pair::new();
    let drops = Arc::new(AtomicUsize::new(0));
    {
        let mut runtime = p.rust.as_mut().unwrap().runtime_mut();
        let vm = runtime.vm_mut();
        let f = vm
            .new_host_function_with_data(|_| Ok(()), &[], Sentinel(drops.clone()))
            .unwrap();
        vm.set_global(b"SENTINEL", f).unwrap();
    }

    p.rust.as_mut().unwrap().end().unwrap();

    drop(p.rust.take());
    for g in [&mut p.r] {
        let mut cb = g.step_callback.take().unwrap();
        cb(g, 0);
        g.step_callback = Some(cb);
    }
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    p.r.step_callback = None;
    p.r.tap_row_judgment_callback = None;
    p.r.hold_judgment_callback = None;
    p.r.avoided_mine_judgment_callback = None;
    assert_eq!(
        drops.load(Ordering::SeqCst),
        0,
        "note field callback still owns the same VM"
    );
    p.r.note_field_tap_callback = None;
    assert_eq!(
        drops.load(Ordering::SeqCst),
        1,
        "last callback releases the VM and all remaining roots"
    );
}

#[test]
fn missing_tap_handlers_and_invalid_players_keep_native_write_drain_boundaries() {
    let mut p = Pair::new();
    p.run("Player:SetLife(0.125)");
    let invalid = Tap {
        player: usize::MAX,
        ..tap(0)
    };
    assert_eq!(p.tap(invalid), invalid);
    assert_ne!(p.r.players_runtime.players[0].life, 0.125);
    p.run("Fields[1]:SetDidTapNoteCallback(nil)");
    assert_eq!(p.tap(tap(0)), tap(0));
    assert_eq!(p.r.players_runtime.players[0].life, 0.125);
    p.flush();
}
#[test]
fn event_clock_reads_owned_music_rate_and_contains_malformed_frame_errors() {
    let mut p = Pair::new();
    p.run("Options=GAMESTATE:GetSongOptionsObject('ModsLevel_Current');Options:MusicRate(2);Options.MusicRate=function()error('public getter must not run')end");
    for g in [&mut p.r] {
        g.clock.song_position.current_music_time_ns = 1_750_000_000;
        g.clock.song_position.current_beat = 99.;
    }
    p.step(0);
    p.run("Options.__songlua_music_rate='bad owned rate'");
    p.step(0);
    p.step(0);
    p.run("Options.__songlua_music_rate=2;return SCREENMAN:GetTopScreen().__songlua_step_error_reported");
    p.flush();
}

#[path = "support/live_snapshot.rs"]
mod live_snapshot;
#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
