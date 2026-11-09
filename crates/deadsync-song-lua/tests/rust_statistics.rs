use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    self as song, SongLuaCompileContext, SongLuaLiveFrame, SongLuaLivePlayer,
    rust_engine::{RustSongRuntime, SongLuaLivePlayerWrite, test_support},
};
use std::path::Path;

fn context() -> SongLuaCompileContext {
    let mut context = SongLuaCompileContext::new(Path::new("."), "statistics migration");
    context.players[1].enabled = true;
    context.music_length_seconds = 125.25;
    context.song_steps = [
        song::SongLuaDifficulty::Easy,
        song::SongLuaDifficulty::Hard,
        song::SongLuaDifficulty::Challenge,
    ]
    .into_iter()
    .enumerate()
    .map(|(i, difficulty)| song::SongLuaStep {
        steps_type: "StepsType_Dance_Single".into(),
        difficulty,
        description: format!("chart-{i}"),
        chart_name: String::new(),
        credit: String::new(),
        meter: 8,
        music_file: "same.ogg".into(),
        possible_grade_points: (i as i32 + 1) * 123,
    })
    .collect();
    context.player_step_indices = [Some(0), Some(1)];
    context
}
fn execute(runtime: &mut RustSongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/statistics.lua").unwrap())
        .unwrap_or_else(|e| panic!("{source}\n{e:?}"))
        .to_vec()
}
#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    String(Vec<u8>),
}
fn number(n: f64) -> Scalar {
    Scalar::Number(if n.is_nan() {
        f64::NAN.to_bits()
    } else {
        n.to_bits()
    })
}
struct Pair {
    rust: RustSongRuntime,
}
impl Pair {
    fn new(context: &SongLuaCompileContext) -> Self {
        let rust = RustSongRuntime::new(context).unwrap();
        let mut pair = Self { rust };
        pair.compare(r#"
            __songlua_side_effect_count=0
            s=STATSMAN:GetCurStageStats();p=s:GetPlayerStageStats(0);q=s:GetPlayerStageStats(1)
            m=s:GetMultiPlayerStageStats()
            function keys(t)local a={};for k in pairs(t)do a[#a+1]=k end;table.sort(a);return table.concat(a,',')end
        "#);
        pair
    }
    fn compare(&mut self, source: &str) {
        let actual: Vec<_> = execute(&mut self.rust, source)
            .into_iter()
            .map(|v| match v {
                Value::Nil => Scalar::Nil,
                Value::Boolean(b) => Scalar::Boolean(b),
                Value::Number(n) => number(n),
                Value::String(id) => {
                    Scalar::String(self.rust.vm_mut().string_bytes(id).unwrap().to_vec())
                }
                v => panic!("scalar expected {v:?}"),
            })
            .collect();
        reference_test::assert_source_debug(
            "song/tests/rust_statistics.rs::Pair::compare",
            source,
            &actual,
        );
    }
    fn reset(&mut self, context: &SongLuaCompileContext, success: bool) {
        assert_eq!(
            self.rust.reset_stage_statistics(context).is_ok(),
            success,
            "Rust reset"
        );
    }
    fn feed(&mut self, frame: &SongLuaLiveFrame, success: bool) {
        assert_eq!(
            self.rust.feed_stage_statistics(frame).is_ok(),
            success,
            "Rust feed"
        );
    }
    fn publish(&mut self, players: [SongLuaLivePlayer; 2]) {
        self.rust.publish_live_players(players);
    }
    fn writes(&mut self, expected: &[usize]) {
        fn failures(writes: Vec<SongLuaLivePlayerWrite>) -> Vec<usize> {
            writes
                .into_iter()
                .map(|write| match write {
                    SongLuaLivePlayerWrite::Fail { player } => player,
                    other => panic!("unexpected {other:?}"),
                })
                .collect()
        }

        assert_eq!(failures(self.rust.take_player_writes()), expected);

        assert!(self.rust.take_player_writes().is_empty());

        reference_test::assert_debug(
            "song/tests/rust_statistics.rs::Pair::writes::players",
            &test_support::rust_live_players(&self.rust).map(|p| p.map(|p| p.failed)),
        );
    }
}
const VALUES: &[&str] = &[
    "nil",
    "false",
    "true",
    "0",
    "-0",
    "-0.75",
    "17.75",
    "-3",
    "0/0",
    "1/0",
    "-1/0",
    "2147483648",
    "4294967296",
    "9223372036854775808",
    "'17.75'",
    "'0x10'",
    "'NaN'",
    "'inf'",
    "' nope '",
    "{}",
    "function()end",
];

#[test]
fn complete_manager_stage_and_player_surfaces_defaults_and_identities() {
    let mut pair = Pair::new(&context());
    pair.compare(r#"
        assert(s==STATSMAN:GetAccumPlayedStageStats() and s==STATSMAN:GetFinalEvalStageStats())
        assert(s==STATSMAN:GetPlayedStageStats(1) and STATSMAN:GetPlayedStageStats(0)==nil)
        assert(p~=q and p~=m and q~=m and m==s:GetMultiPlayerStageStats('anything'))
        assert(p:GetPlayedSteps()==p:GetPlayedSteps() and p:GetPlayedSteps()~=p:GetPossibleSteps())
        assert(p:GetPlayedSteps()[1]==GAMESTATE:GetCurrentSteps(0))
        assert(q:GetPossibleSteps()[1]==GAMESTATE:GetCurrentSteps(1))
        assert(m:GetPlayedSteps()[1]~=p:GetPlayedSteps()[1])
        local a,b=s:GetPlayedSongs(),s:GetPossibleSongs()
        assert(a~=b and a[1]==b[1] and a[1]~=GAMESTATE:GetCurrentSong())
        assert(p:GetHighScore()==p:GetHighScore() and p:GetHighScore()~=q:GetHighScore())
        return keys(STATSMAN),keys(s),keys(p),keys(q),keys(m),STATSMAN:GetStagesPlayed(),
            STATSMAN:GetFinalGrade(),STATSMAN:GetBestGrade(),STATSMAN:GetWorstGrade(),STATSMAN:GetBestFinalGrade(),
            s:GetStageIndex(),s:GetStage(),s:AllFailed(),s:OnePassed(),s:PlayerHasHighScore(),s:GetEarnedExtraStage(),s:GaveUp(),
            s:GetGameplaySeconds(),s:GetStepsSeconds(),s:GetTotalPossibleStepsSeconds(),
            p:GetPossibleDancePoints(),q:GetPossibleDancePoints(),m:GetPossibleDancePoints(),
            p:GetHighScore():GetName(),q:GetHighScore():GetName(),m:GetHighScore():GetName()
    "#);
    for selector in VALUES
        .iter()
        .copied()
        .chain(["1", "2", "'P1'", "'p2'", "'PlayerNumber_P2'"])
    {
        pair.compare(&format!(
            r#"
            return s:GetPlayerStageStats({selector})==p,s.GetPlayerStageStats({selector})==q,
                STATSMAN:GetPlayedStageStats({selector})==s
        "#
        ));
    }
    pair.compare(r#"
        local names={'GetCaloriesBurned','GetNumControllerSteps','GetSurvivalSeconds','GetCurrentScoreMultiplier',
            'GetCurMaxScore','GetGrade','GetFailed','GetScore','GetPercentDancePoints','GetActualDancePoints',
            'GetCurrentPossibleDancePoints','GetCurrentLife','GetCurrentCombo','GetCurrentMissCombo','GetLifeRemainingSeconds',
            'GetAliveSeconds','GetHoldNoteScores','FullCombo','MaxCombo','GetLessonScoreActual','GetLessonScoreNeeded',
            'GetMachineHighScoreIndex','GetPersonalHighScoreIndex','GetStageAward','GetPeakComboAward','IsDisqualified',
            'GetPercentageOfTaps','GetBestFullComboTapNoteScore','FullComboOfScore','GetSongsPassed','GetSongsPlayed'}
        local out={};for _,v in ipairs({p,q,m})do for _,name in ipairs(names)do out[#out+1]=tostring(v[name](v)) end end
        assert(p:GetComboList()~=p:GetComboList() and next(p:GetComboList())==nil)
        assert(p:GetRadarActual()~=p:GetRadarActual())
        return table.concat(out,','),p:GetRadarActual():GetValue('x'),p:GetRadarPossible():GetValue('x'),p:GetRadarValues():GetValue('x'),
            #p:GetLifeRecord(),#p:GetLifeRecord(10,0),#p:GetLifeRecord(10,-2),#p:GetLifeRecord(10,'3'),p:GetLifeRecord()[1]
    "#);
}

#[test]
fn score_and_limit_argument_conversions_return_values_and_effect_order() {
    let mut pair = Pair::new(&context());
    for value in VALUES {
        pair.compare(&format!(
            r#"
            p.__songlua_score=10;p.__songlua_cur_max_score=20;__songlua_side_effect_count=0
            local a=p:SetScore({value});local b=p.SetCurMaxScore({value})
            return a==p,b==p,p:GetScore(),p:GetCurMaxScore(),__songlua_side_effect_count
        "#
        ));
        for possible in ["100", "0", "-1", "'7.75'", "nil", "1/0", "0/0"] {
            pair.compare(&format!(r#"
                p.__songlua_actual_dance_points=2;p.__songlua_possible_dance_points=3;__songlua_side_effect_count=0
                local ok,value=pcall(p.SetDancePointLimits,p,{value},{possible},'last')
                return ok,ok and value==p,ok and value=='last',p:GetActualDancePoints(),p:GetPossibleDancePoints(),__songlua_side_effect_count
            "#));
        }
    }
    pair.compare(r#"
        local ok,err=pcall(p.SetDancePointLimits,p,nil,2)
        assert(not ok and string.find(tostring(err),'SetDancePointLimits requires two numbers',1,true))
        assert(p:SetDancePointLimits(8,7)==7 and p:GetActualDancePoints()==7)
        assert(p.SetDancePointLimits(1,7,'tail')=='tail')
        p.__songlua_score=nil;p.__songlua_possible_dance_points=nil;p.__songlua_actual_dance_points=nil
        local log={};setmetatable(p,{__newindex=function(t,k,v)
            collectgarbage('collect');log[#log+1]=k..'='..v
            if k=='__songlua_actual_dance_points' then error('write failed')end;rawset(t,k,v)
        end})
        assert(p:SetScore(45)==p);local before=__songlua_side_effect_count
        assert(not pcall(p.SetDancePointLimits,p,4,9));assert(__songlua_side_effect_count==before)
        return table.concat(log,','),p:GetScore(),p:GetPossibleDancePoints(),p:GetActualDancePoints(),__songlua_side_effect_count
    "#);
}

#[test]
fn typed_getters_and_raw_receiver_fields_match_native_coercions() {
    let mut pair = Pair::new(&context());
    for value in VALUES {
        pair.compare(&format!(r#"
            p.__songlua_score={value};p.__songlua_life={value};p.__songlua_combo={value};p.__songlua_tap_scores.W1={value}
            local out={{}}
            for _,fn in ipairs({{p.GetScore,p.GetCurrentLife,p.GetCurrentCombo,function(self)return self:GetTapNoteScores('W1')end}}) do
                local ok,v=pcall(fn,p);out[#out+1]=ok;out[#out+1]=ok and v or false
            end
            p.__songlua_failed={value};out[#out+1]=p:GetFailed();out[#out+1]=p:GetGrade()
            return unpack(out)
        "#));
    }
    pair.compare(r#"
        p.__songlua_score=nil;p.__songlua_failed=nil
        setmetatable(p,{__index=function(t,k)collectgarbage('collect');if k=='__songlua_score'then return '27.75'end;return false end})
        local fake={__songlua_life='0.75',__songlua_combo='12.5',__songlua_miss_combo=3,__songlua_tap_scores={W1='7.5'}}
        assert(not pcall(p.GetCurrentLife));assert(not pcall(p.GetCurrentCombo,{}))
        for _,name in ipairs({'w1','TapNoteScore_w1','bogus',3,false})do assert(not pcall(p.GetTapNoteScores,p,name))end
        return p:GetScore(),p:GetFailed(),p.GetScore(fake),p.GetCurrentLife(fake),p.GetCurrentCombo(fake),
            p.GetCurrentMissCombo(fake),p.GetTapNoteScores(fake,'TapNoteScore_W1')
    "#);
}

#[test]
fn frozen_native_dance_percent_fixture_matches_rust_statistics() {
    let native: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/itgmania-song-lua-micro/native-dance-percent.json"
    )))
    .unwrap();
    let mut pair = Pair::new(&context());
    for case in native["cases"].as_array().unwrap() {
        let source = format!(
            "p.__songlua_actual_dance_points={};p.__songlua_possible_dance_points={};return p:GetPercentDancePoints()",
            case["actual"], case["possible"]
        );
        pair.compare(&source);
        assert_eq!(
            execute(&mut pair.rust, &source),
            vec![Value::Number(case["percent"].as_f64().unwrap())],
            "case {}",
            case["index"]
        );
    }
    pair.compare("p.__songlua_actual_dance_points=0;p.__songlua_possible_dance_points=0;assert(p:GetPercentDancePoints()==0)");
}

#[test]
fn reset_preserves_selected_objects_and_array_identity_across_collecting_callbacks() {
    let mut context = context();
    let mut pair = Pair::new(&context);
    pair.compare(r#"
        played=p:GetPlayedSteps();possible=p:GetPossibleSteps();played[2]='tail';possible[2]='other'
        selected=GAMESTATE:GetCurrentSong():GetAllSteps();GAMESTATE:SetCurrentSteps(0,selected[3])
        p:SetScore(9);p:FailPlayer();assert(select('#',STATSMAN:Reset())==0 and p:GetScore()==9 and p:GetFailed())
        p.GetPlayedSteps=function(self)
            GAMESTATE:SetCurrentSteps(0,selected[2]);collectgarbage('collect');return played
        end
        p.GetPossibleSteps=function(self)collectgarbage('collect');return possible end
    "#);
    context.player_step_indices = [Some(2), None];
    pair.reset(&context, true);
    pair.compare(r#"
        assert(played==p:GetPlayedSteps() and possible==p:GetPossibleSteps())
        assert(played[1]==selected[3] and possible[1]==selected[3] and played[2]=='tail' and possible[2]=='other')
        assert(GAMESTATE:GetCurrentSteps(0)==selected[2])
        return p:GetScore(),p:GetCurMaxScore(),p:GetFailed(),p:GetCurrentLife(),p:GetCurrentCombo(),p:GetTapNoteScores('W1'),
            p:GetPossibleDancePoints(),q:GetPossibleDancePoints(),s:GetGameplaySeconds()
    "#);
    pair.compare(
        r#"
        p:SetScore(88);q:SetScore(99);p.GetPlayedSteps=function()error('reset getter failed')end
    "#,
    );
    pair.reset(&context, false);
    pair.compare("assert(p:GetScore()==0 and q:GetScore()==99);p.GetPlayedSteps=function()collectgarbage('collect');return played end");
    pair.reset(&context, true);
    pair.compare("assert(q:GetScore()==0 and played[1]==selected[2]);return played[2],possible[2]");
}

fn frame() -> SongLuaLiveFrame {
    SongLuaLiveFrame {
        screen_seconds: 23.75,
        players: [
            SongLuaLivePlayer {
                possible_points: 400,
                actual_points: 123,
                current_possible_points: 246,
                life: 0.8125,
                life_record_latest: Some(0.375),
                failed: true,
                combo: 17,
                miss_combo: 4,
                tap_scores: std::array::from_fn(|i| i as u32 * 3),
                ..Default::default()
            },
            SongLuaLivePlayer {
                possible_points: 800,
                actual_points: 456,
                current_possible_points: 567,
                life: 0.25,
                combo: 31,
                miss_combo: 7,
                tap_scores: [9; 11],
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}
const FED: &str = r#"
    return s:GetGameplaySeconds(),p:GetPossibleDancePoints(),p:GetActualDancePoints(),p:GetCurrentPossibleDancePoints(),
        p:GetCurrentLife(),p:GetFailed(),p:GetGrade(),p:GetCurrentCombo(),p:GetCurrentMissCombo(),
        p:GetTapNoteScores('W1'),p:GetTapNoteScores('CheckpointHit'),p:GetScore(),p:GetCurMaxScore(),
        q:GetPossibleDancePoints(),q:GetActualDancePoints(),q:GetCurrentPossibleDancePoints(),q:GetCurrentLife(),q:GetCurrentCombo()
"#;
#[test]
fn frame_feed_retains_original_players_and_distinguishes_recorded_and_meter_life() {
    let mut pair = Pair::new(&context());
    pair.compare(
        r#"
        p:SetScore(77);p:SetCurMaxScore(88);meter={};p.__songlua_life_meter=meter
        s.__songlua_player_stats={q,p};STATSMAN={};collectgarbage('collect')
        for _,v in ipairs({p,q,meter,p.__songlua_tap_scores})do
            setmetatable(v,{__newindex=function()error('feed must write raw')end})
        end
        p.__songlua_combo=nil;q.__songlua_life=nil
    "#,
    );
    pair.feed(&frame(), true);
    pair.compare(FED);
    pair.compare("assert(p:GetCurrentLife()==0.375 and meter.__songlua_life==0.8125);assert(p:GetScore()==77 and p:GetCurMaxScore()==88)");
    // Reset deliberately follows the replaced raw array; feed deliberately does not.
    pair.reset(&context(), true);
    pair.compare("assert(q:GetPossibleDancePoints()==123 and p:GetPossibleDancePoints()==246)");
    pair.feed(&frame(), true);
    pair.compare(FED);
}

#[test]
fn frame_errors_preserve_prior_raw_writes_and_next_frame_recovers() {
    let mut pair = Pair::new(&context());
    pair.compare(
        "p.__songlua_life_meter=false;p.__songlua_combo=91;q.__songlua_actual_dance_points=92",
    );
    pair.feed(&frame(), false);
    pair.compare("assert(p:GetActualDancePoints()==123 and p:GetCurrentLife()==0.375 and p:GetCurrentCombo()==91 and q:GetActualDancePoints()==92);return s:GetGameplaySeconds()");
    pair.compare(
        "p.__songlua_life_meter={};counts=p.__songlua_tap_scores;p.__songlua_tap_scores=false",
    );
    pair.feed(&frame(), false);
    pair.compare("assert(p:GetCurrentCombo()==17 and p.__songlua_life_meter.__songlua_life==0.8125 and q:GetActualDancePoints()==92);p.__songlua_tap_scores=counts");
    pair.feed(&frame(), true);
    pair.compare(FED);
}

#[test]
fn fail_player_publishes_and_queues_in_native_order_only_in_live_mode() {
    let mut pair = Pair::new(&context());
    pair.publish([SongLuaLivePlayer::default(); 2]);
    pair.compare("assert(p:FailPlayer()==p);assert(m:FailPlayer()==m)");
    pair.writes(&[]);
    assert!(test_support::rust_live_players(&pair.rust).unwrap()[0].failed);
    pair.rust.set_live(true);
    pair.publish([SongLuaLivePlayer::default(); 2]);
    pair.compare("q:FailPlayer();p:FailPlayer();q:FailPlayer();m:FailPlayer()");
    pair.writes(&[1, 0, 1]);
    pair.compare(r#"
        p.__songlua_player_index=-1;p.__songlua_failed=false;local before=__songlua_side_effect_count
        assert(not pcall(p.FailPlayer,p));assert(p:GetFailed() and __songlua_side_effect_count==before)
        p.__songlua_player_index=8;p:FailPlayer()
        p.__songlua_player_index=0;__songlua_side_effect_count=false
        assert(not pcall(p.FailPlayer,p));assert(p:GetFailed())
    "#);
    pair.writes(&[8, 0]);
    pair.compare("__songlua_side_effect_count=0;p.__songlua_player_index=nil;p:FailPlayer()");
    pair.writes(&[]);
    // Feed and publication remain separate phases: feed alone must not replace live players.
    pair.feed(&SongLuaLiveFrame::default(), true);
    assert!(test_support::rust_live_players(&pair.rust).unwrap()[0].failed);
    pair.publish([SongLuaLivePlayer::default(); 2]);
    assert!(!test_support::rust_live_players(&pair.rust).unwrap()[0].failed);
}

#[test]
fn statistics_and_saved_methods_survive_gc_then_release_their_owners() {
    let mut runtime = RustSongRuntime::new(&context()).unwrap();
    test_support::expose_catalog(&mut runtime).unwrap();
    execute(
        &mut runtime,
        r#"
        local s=STATSMAN:GetCurStageStats();local p=s:GetPlayerStageStats(0)
        weak=setmetatable({STATSMAN,s,p,p:GetHighScore(),p:GetPlayedSteps()[1]},{__mode='v'})
        saved=p.GetScore;catalog=nil;STATSMAN=nil;SONGMAN=nil
        collectgarbage('collect');assert(weak[2] and weak[3] and weak[4] and weak[5])
    "#,
    );
    runtime.feed_stage_statistics(&frame()).unwrap();
    execute(
        &mut runtime,
        "assert(weak[3]:GetActualDancePoints()==123);collectgarbage('collect')",
    );
    test_support::release_catalog_root(&mut runtime).unwrap();
    test_support::retire_game_state_globals(&mut runtime).unwrap();
    execute(
        &mut runtime,
        r#"
        collectgarbage('collect');collectgarbage('collect');assert(weak[1]==nil and weak[2]==nil)
        assert(weak[3] and weak[4] and weak[5] and saved()==0)
        saved=nil;collectgarbage('collect');collectgarbage('collect')
        for i=1,5 do assert(weak[i]==nil,'unreleased statistics owner '..i)end
    "#,
    );
}

#[test]
fn receiver_based_queries_and_fresh_list_factories_do_not_retain_original_player() {
    let mut runtime = RustSongRuntime::new(&context()).unwrap();
    test_support::release_catalog_root(&mut runtime).unwrap();
    test_support::retire_game_state_globals(&mut runtime).unwrap();
    execute(
        &mut runtime,
        r#"
        local p=STATSMAN:GetCurStageStats():GetPlayerStageStats(0)
        weak=setmetatable({p},{__mode='v'})
        life,combo,taps,record,radar=p.GetCurrentLife,p.GetCurrentCombo,p.GetTapNoteScores,p.GetLifeRecord,p.GetRadarPossible
        STATSMAN=nil;SONGMAN=nil;p=nil
        collectgarbage('collect');collectgarbage('collect');assert(weak[1]==nil)
        local fake={__songlua_life=0.75,__songlua_combo=17,__songlua_tap_scores={W1=8}}
        assert(life(fake)==0.75 and combo(fake)==17 and taps(fake,'W1')==8)
        assert(#record({},10,3)==3 and radar():GetValue()==1)
    "#,
    );
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
