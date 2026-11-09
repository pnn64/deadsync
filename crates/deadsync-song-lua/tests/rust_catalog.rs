use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    self as song, SongLuaCompileContext,
    rust_engine::{RustSongRuntime, test_support},
};
use std::path::Path;

fn context() -> SongLuaCompileContext {
    // Metadata probes use a virtual song directory so snapshots do not depend
    // on Cargo's package working directory or the checkout location.
    let mut context = SongLuaCompileContext::new(Path::new("/fixture/catalog"), "Catalog migration");
    context.song_dir = Path::new("/fixture/catalog").to_owned();
    context
}
fn execute(runtime: &mut RustSongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/catalog.lua").unwrap())
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
        let mut rust = RustSongRuntime::new(context).unwrap();
        test_support::expose_catalog(&mut rust).unwrap();
        Self { rust }
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
                other => panic!("scalar expected: {other:?}"),
            })
            .collect();
        reference_test::assert_source_debug(
            "song/tests/rust_catalog.rs::Pair::compare",
            source,
            &actual,
        );
    }
}

const SNAPSHOT: &str = r#"
function snapshot(v)
    local kind=type(v)
    if kind=='table' then
        local entries={}
        for k,value in pairs(v) do entries[#entries+1]=snapshot(k)..'='..snapshot(value) end
        table.sort(entries);return '{'..table.concat(entries,';')..'}'
    elseif kind=='string' then return 's'..#v..':'..v
    elseif kind=='number' then return v~=v and 'nan' or string.format('%.17g',v)
    elseif kind=='boolean' then return tostring(v)
    else return kind end
end
"#;

fn populated() -> SongLuaCompileContext {
    use song::SongLuaDifficulty::*;
    let mut context = context();
    context.main_title = "Catalog / title".into();
    context.song_display_bpms = [90.0, 180.0];
    context.song_music_rate = 1.25;
    context.music_length_seconds = 127.5;
    context.song_note_bounds = Some([-0.125, 101.25]);
    context.song_timing_bpms = vec![(0.0, 90.0), (8.0, 180.0)];
    context.players[0].display_bpms = [60.0, 240.0];
    context.song_steps = [Easy, Hard, Edit, Edit]
        .into_iter()
        .enumerate()
        .map(|(i, difficulty)| song::SongLuaStep {
            steps_type: if i == 3 {
                "StepsType_Dance_Double"
            } else {
                "StepsType_Dance_Single"
            }
            .into(),
            difficulty,
            description: format!("description {i}"),
            chart_name: format!("chart {i}"),
            credit: format!("author {i}"),
            meter: 7 + i as u32,
            music_file: "overridden.ogg".into(),
            possible_grade_points: 100,
        })
        .collect();
    context.player_step_indices = [Some(1), Some(2)];
    context
}

#[test]
fn song_steps_course_and_trail_metadata_match_production() {
    for context in [context(), populated()] {
        let mut pair = Pair::new(&context);
        pair.compare(SNAPSHOT);
        pair.compare(r#"
            local values={}
            for _,object in ipairs{catalog.song,catalog.steps[1],catalog.steps[2],catalog.course,catalog.trails[1],catalog.trails[2]} do
                local out={}
                for name,method in pairs(object) do
                    if type(method)=='function' and name~='GetTrailEntry' and name~='GetOneSteps' and name~='GetStepsByStepsType' and name~='HasStepsTypeAndDifficulty' and name~='HasStepsType' and name~='HasEdits' then
                        out[name]={method(object)}
                    end
                end
                values[#values+1]=out
            end
            return snapshot(values),snapshot(SongUtil),snapshot(SONGMAN)
        "#);
        pair.compare(r#"
            local s=catalog.song
            local t=catalog.trails[1]
            local c=catalog.course
            assert(s:GetTimingData()==s:GetTimingData() and s:GetDisplayBpms()==s:GetDisplayBpms())
            assert(t:GetTrailEntries()==t:GetTrailEntries() and t:GetTrailEntry(0)==t:GetTrailEntry(800))
            assert(t:GetTrailEntry(0):GetSong()==s and t:GetTrailEntry(0):GetSteps()==catalog.steps[1])
            assert(c:GetTrail()==t and c:GetAllTrails()[1]==t and c:GetCourseEntries()[1]~=t:GetTrailEntry(0))
            assert(t:GetRadarValues()~=t:GetRadarValues())
            assert(catalog.steps[1]:GetRadarValues()==catalog.steps[1]:GetRadarValues())
            t:GetTrailEntries()[1]=nil
            assert(t:GetTrailEntry(0):GetSong()==s)
            t:GetTrailEntries()[1]=false
            return pcall(t.GetTrailEntry,t,0)
        "#.replace("return pcall(t.GetTrailEntry,t,0)","local ok=pcall(t.GetTrailEntry,t,0);return ok").as_str());
    }
}

#[test]
fn chart_selectors_keep_native_identity_and_numeric_enum_rules() {
    let mut pair = Pair::new(&populated());
    pair.compare(SNAPSHOT);
    pair.compare(r#"
        local s=catalog.song
        original=s:GetAllSteps()
        for _,v in ipairs(original) do v.GetStepsType=function() error('overridden type') end;v.GetDifficulty=function() error('overridden difficulty') end end
        s.__songlua_steps[1]=false;s.__songlua_steps=nil
        local result={}
        for _,kind in ipairs{'StepsType_Dance_Single',' dance single ','DANCE-SINGLE','StepsType_Dance_Double','missing',0,1,37,-1,38,.5,0/0,true} do
            local row={}
            for _,name in ipairs{'GetAllSteps','GetStepsByStepsType','GetOneSteps','HasStepsType','HasStepsTypeAndDifficulty','HasEdits'} do
                local ok,value=pcall(s[name],s,kind,'Expert')
                row[name]=ok and (type(value)=='table' and #value or value) or 'error'
            end
            result[#result+1]=row
        end
        assert(s:GetAllSteps()~=s:GetAllSteps() and s:GetAllSteps()[1]==original[1])
        assert(s:GetOneSteps(0,'Easy')==original[1] and s:GetOneSteps(0,5)==original[3])
        assert(catalog.steps[1]==original[2] and catalog.steps[2]==original[3])
        for _,diff in ipairs{-1,6,1.5,0/0,math.huge} do assert(not pcall(s.GetOneSteps,s,0,diff)) end
        assert(s:GetOneSteps(0,'garbage')==nil)
        return snapshot(result)
    "#);
}

#[test]
fn song_util_uses_live_getters_and_preserves_arguments_and_reentry() {
    let mut pair = Pair::new(&populated());
    pair.compare(r#"
        local calls={}
        local proxy=setmetatable({}, {__index=function(self,k)
            collectgarbage('collect');calls[#calls+1]=k
            if k=='GetAllSteps' then return function(self,...) calls[#calls+1]=select('#',...);return 'all',99 end end
            if k=='GetStepsByStepsType' then return function(self,...) calls[#calls+1]=select('#',...);return ... end end
        end})
        assert(SongUtil.GetPlayableSteps(false,{},proxy)=='all')
        assert(SongUtil:GetPlayableStepsByStepsType(proxy)=='StepsType_Dance_Single')
        assert(SongUtil.GetPlayableStepsByStepsType(proxy,0)==0)
        assert(SongUtil.GetPlayableStepsByStepsType(proxy,nil)==nil)
        assert(#SongUtil.GetPlayableSteps({GetAllSteps=setmetatable({}, {__call=function() error('callable table') end})})==0)
        local fake={GetAllSteps=function()end,GetStepsByStepsType=false}
        assert(#SongUtil.GetPlayableStepsByStepsType(fake)==0)
        fake.GetStepsByStepsType=function() return SongUtil.GetPlayableSteps(catalog.song)[2] end
        assert(SongUtil.GetPlayableStepsByStepsType(fake)==catalog.steps[1])
        return table.concat(calls,','),pcall(function() SongUtil.GetPlayableSteps(setmetatable({},{__index=function() error('propagate') end})) end) and true or false
    "#);
}

#[test]
fn timing_uses_owned_rate_and_float_traversal_with_fresh_arrays() {
    use deadsync_rules::timing::{
        DelaySegment, StopSegment, TimingData, TimingSegments, WarpSegment,
    };
    let mut context = populated();
    context.song_timing = Some(TimingData::from_segments(
        0.75,
        0.125,
        &TimingSegments {
            bpms: vec![(0.0, 120.0), (8.0, 90.0)],
            stops: vec![StopSegment {
                beat: 4.0,
                duration: 0.75,
            }],
            delays: vec![DelaySegment {
                beat: 6.0,
                duration: 0.25,
            }],
            warps: vec![WarpSegment {
                beat: 10.0,
                length: 2.0,
            }],
            ..Default::default()
        },
        &[],
    ));
    let mut pair = Pair::new(&context);
    pair.compare(SNAPSHOT);
    pair.compare(r#"
        local t=catalog.song:GetTimingData()
        assert(t:GetActualBPM()~=t:GetActualBPM() and t:GetBPMs()~=t:GetBPMs())
        assert(type(t:GetBPMsAndTimes(0)[1])=='table' and type(t:GetBPMsAndTimes('')[1])=='table')
        assert(type(t:GetBPMsAndTimes(false)[1])=='string')
        local out={}
        for _,rate in ipairs{.75,1,1.25,2,'1.5','0x2'} do
            catalog.current_song_options.__songlua_music_rate=rate
            for _,beat in ipairs{-4,0,3.9999,4,4.0001,6,8,10,11,12,20,'3.5'} do
                out[#out+1]={t:GetBPMAtBeat(beat),t:GetElapsedTimeFromBeat(beat),t:GetBeatFromElapsedTime(beat)}
            end
        end
        catalog.current_song_options.__songlua_music_rate={}
        local ok=pcall(t.GetElapsedTimeFromBeat,t,3)
        assert(not ok and t:GetBPMAtBeat(3)==120)
        catalog.current_song_options.__songlua_music_rate=1.25
        tonumber=function() error('global replacement') end
        return snapshot(out),t:GetElapsedTimeFromBeat(4),snapshot(t:GetBPMsAndTimes()),snapshot(t:GetBPMsAndTimes(true))
    "#);
    pair.compare(
        r#"
        local out={};local t=catalog.song:GetTimingData()
        for _,v in ipairs{false,{},'bad','0x10',math.huge,0/0,'inf','nan',' 2.5 '} do
            local ok,value=pcall(t.GetBPMAtBeat,t,v);out[#out+1]={ok,ok and value or 'error'}
        end
        return snapshot(out)
    "#,
    );
}

fn chart_notes() -> SongLuaCompileContext {
    let mut context = populated();
    context.song_steps.truncate(1);
    context.player_step_indices = [Some(0), Some(0)];
    context.song_step_notes.push(std::sync::Arc::from(
        b"1000\n0100\n00M0\n000L\n,\n2000\n0100\n3000\n000F\n,\n0004\n0000\n0003\n1000\n"
            .as_slice(),
    ));
    context.song_step_row_beats.push(std::sync::Arc::from([
        0., 1., 2., 3., 4., 5., 6., 7., 8., 9., 10., 11.,
    ]));
    context
}

#[test]
fn density_radar_and_column_cues_match_and_keep_fresh_mutable_results() {
    let mut pair = Pair::new(&chart_notes());
    pair.compare(SNAPSHOT);
    pair.compare(r#"
        local s=catalog.steps[1];local out={}
        for _,pn in ipairs{0,1,'P1','playernumber_p2','P2'} do
            out[#out+1]={s:GetNotesPerMeasure(pn),s:GetNpsPerMeasure(pn),s:GetPeakNps(pn)}
        end
        for _,pn in ipairs{2,-1,.5,{},false,'p3'} do assert(not pcall(s.GetPeakNps,s,pn)) end
        assert(s:GetNotesPerMeasure()~=s:GetNotesPerMeasure() and s:GetNpsPerMeasure()~=s:GetNpsPerMeasure())
        local before=s:GetNpsPerMeasure();catalog.current_song_options.__songlua_music_rate=2
        assert(snapshot(before)==snapshot(s:GetNpsPerMeasure()))
        for _,name in ipairs{'Notes','Holds','Mines','Rolls','RadarCategory_Notes','notes','Jumps'} do out[#out+1]=s:GetRadarValues():GetValue(name) end
        for _,minimum in ipairs{-1,0,.5,1.5,'2',false,'bad',math.huge} do out[#out+1]=s:GetColumnCues(minimum) end
        local first=s:GetColumnCues();first[1].columns[1].noteType=nil;first[1].columns[1].isMine=true
        assert(s:GetColumnCues()[1].columns[1].noteType~=nil)
        return snapshot(out)
    "#);
}

#[test]
fn column_cues_root_timing_callbacks_across_collection_and_reentry() {
    let mut pair = Pair::new(&chart_notes());
    pair.compare(SNAPSHOT);
    pair.compare(r#"
        local steps=catalog.steps[1];local original=steps.GetColumnCues;local log={};local depth=false
        steps.GetTimingData=function(self)
            assert(self==steps);log[#log+1]='get'
            return setmetatable({}, {__index=function(t,k)
                assert(k=='GetElapsedTimeFromBeat');collectgarbage('collect');log[#log+1]='elapsed'
                return function(receiver,beat)
                    assert(receiver==t);collectgarbage('collect');log[#log+1]=beat
                    if not depth then depth=true;assert(#original(steps,100)>0);depth=false end
                    return tostring(beat*.75)
                end
            end})
        end
        local result=steps:GetColumnCues(.5)
        steps.GetTimingData=false
        assert(not pcall(original,steps))
        steps.GetTimingData=function() return {GetElapsedTimeFromBeat=setmetatable({}, {__call=function() return 1 end})} end
        assert(not pcall(original,steps))
        steps.GetTimingData=function() return {GetElapsedTimeFromBeat=function() return {} end} end
        assert(not pcall(original,steps))
        return snapshot(result),table.concat(log,',')
    "#);
}

#[test]
fn manager_shared_arrays_lookup_errors_and_side_effects_match() {
    let mut pair = Pair::new(&populated());
    pair.compare(SNAPSHOT);
    pair.compare(r#"
        local m=SONGMAN;local s=catalog.song;local c=catalog.course;local group=s:GetGroupName()
        assert(m:GetRandomSong()==s and m:GetRandomCourse()==c and m:GetSongFromSteps(false)==s)
        local song,steps=m:GetExtraStageInfo();assert(song==s and steps==catalog.steps[1])
        assert(m:GetAllSongs()==m:GetAllSongs() and m:GetAllSongs()~=m:GetPopularSongs())
        assert(m:GetPopularSongs()==m:GetPreferredSortSongs() and m:GetPopularSongs()==m:GetSongsInGroup(group))
        assert(m:GetPopularCourses()==m:GetPreferredSortCourses() and m:GetPopularCourses()==m:GetCoursesInGroup(group))
        assert(m:GetSongGroupNames()~=m:GetCourseGroupNames())
        assert(m:GetSongsInGroup('missing')~=m:GetSongsInGroup('missing'))
        assert(m:GetSongColor()~=m:GetSongColor())
        local out={}
        for _,query in ipairs{s:GetSongDir(),s:GetMainTitle(),group..'/'..s:GetMainTitle(),c:GetCourseDir(),'missing',false,123} do
            out[#out+1]={m:FindSong(query)==s,m:FindCourse(query)==c,m:DoesSongGroupExist(query),m:GetSongGroupBannerPath(query),m:ShortenGroupName(query)}
        end
        assert(m:SetPreferredSongs('ignored')==m and m:SetPreferredCourses()==m)
        assert(m:GetSongRank(s)==1 and m:GetSongRank(false)==nil)
        return snapshot(out),__songlua_side_effect_count,m:SongToPreferredSortSectionName(s),m:GetGroup():GetSyncOffset()
    "#);
}

#[test]
fn retained_selectors_survive_gc_and_runtimes_are_isolated() {
    let context = populated();
    let mut pair = Pair::new(&context);
    pair.compare(
        r#"
        local song=catalog.song;local steps=song:GetAllSteps()[1]
        saved=song.GetAllSteps;savedTiming=steps:GetTimingData().GetElapsedTimeFromBeat
        weak=setmetatable({song,steps,SONGMAN}, {__mode='v'})
        catalog.song=nil;catalog.steps=nil;catalog.course=nil;catalog.trails=nil;SONGMAN=nil
        collectgarbage('collect');collectgarbage('collect')
        return saved()[1]==weak[2],savedTiming(8)
    "#,
    );
    let mut second = Pair::new(&context);
    second.compare("return catalog.song:GetMainTitle(),saved==nil");
}

#[test]
fn malformed_chart_rows_fail_initialization() {
    let mut context = chart_notes();
    context.song_step_row_beats.clear();
    assert!(RustSongRuntime::new(&context).is_err());
    let mut context = populated();
    context.player_step_indices[1] = Some(100);
    assert!(RustSongRuntime::new(&context).is_err());
}

#[test]
fn frozen_song_steps_metadata_matches_selected_and_catalog_objects() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/itgmania-song-lua-micro");
    let source = deadsync_simfile::song::parse_song_meta_file(
        &root.join("song-steps.sm"),
        &deadsync_simfile::song::ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
        0.,
        |_| 0.,
    )
    .unwrap();
    let expected: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("song-steps-native.json")).unwrap())
            .unwrap();
    let mut context = populated();
    context.song_steps = source.charts.iter().map(song::SongLuaStep::from).collect();
    let mut pair = Pair::new(&context);
    for (i, chart) in expected["charts"].as_array().unwrap().iter().enumerate() {
        let script = format!(
            "local s=catalog.song:GetAllSteps()[{}];return s:GetDescription(),s:GetCredit(),s:GetMeter()",
            i + 1
        );
        pair.compare(&script);
        let values = execute(&mut pair.rust, &script);
        let Value::String(description) = values[0] else {
            panic!()
        };
        assert_eq!(
            std::str::from_utf8(pair.rust.vm_mut().string_bytes(description).unwrap()).unwrap(),
            chart["description"].as_str().unwrap()
        );
        let Value::String(credit) = values[1] else {
            panic!()
        };
        assert_eq!(
            std::str::from_utf8(pair.rust.vm_mut().string_bytes(credit).unwrap()).unwrap(),
            chart["credit"].as_str().unwrap()
        );
        assert_eq!(values[2], Value::Number(chart["meter"].as_f64().unwrap()));
    }
}

#[test]
fn frozen_split_timing_uses_each_loaded_chart_owner() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/itgmania-song-lua-micro");
    let options = deadsync_simfile::song::ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
    let source = deadsync_simfile::song::parse_song_meta_file(
        &root.join("timing-data.ssc"),
        &options,
        0.,
        |_| 0.,
    )
    .unwrap();
    let payload = deadsync_simfile::cache::load_gameplay_charts_with_options(
        &source,
        &[0, 1],
        &deadsync_simfile::cache::GameplayChartLoadOptions {
            cache_dir: &root,
            parse_options: &options,
            allow_cache_read: false,
            allow_cache_write: false,
            verify_cache_freshness: true,
            global_offset_seconds: 0.,
        },
        |_| 0.,
    )
    .unwrap();
    let expected: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("timing-data-native.json")).unwrap())
            .unwrap();
    let mut context = populated();
    context.song_music_rate = 1.;
    context.song_steps = source.charts.iter().map(song::SongLuaStep::from).collect();
    context.song_timing = source.song_timing.clone();
    context.song_step_timing = payload.charts.iter().map(|c| c.timing.clone()).collect();
    context.player_step_indices = [Some(0), Some(0)];
    let mut pair = Pair::new(&context);
    for (name, owner) in [
        ("Song", "catalog.song"),
        ("Selected", "catalog.steps[1]"),
        ("Chart1", "catalog.song:GetAllSteps()[1]"),
        ("Chart2", "catalog.song:GetAllSteps()[2]"),
    ] {
        for (method, arg, key) in [
            ("GetElapsedTimeFromBeat", 0., "Actor.x"),
            ("GetElapsedTimeFromBeat", 6., "Actor.y"),
            ("GetBeatFromElapsedTime", 3., "Actor.z"),
            ("GetBPMAtBeat", 6., "Actor.aux"),
            ("GetBPMAtBeat", 3.995, "Actor.rotationx"),
        ] {
            let script = format!("return {owner}:GetTimingData():{method}({arg})");
            pair.compare(&script);
            let result = execute(&mut pair.rust, &script);
            assert_eq!(
                result,
                [Value::Number(expected[name][key].as_f64().unwrap())],
                "{name} {method}"
            );
        }
        let script = format!(
            "local t={owner}:GetTimingData();local a=t:GetBPMsAndTimes(0);return #a,a[1][2],t:HasBPMChanges()"
        );
        pair.compare(&script);
        assert_eq!(
            execute(&mut pair.rust, &script),
            [
                Value::Number(expected[name]["Actor.zoomy"].as_f64().unwrap()),
                Value::Number(expected[name]["Actor.zoomx"].as_f64().unwrap()),
                Value::Boolean(true)
            ]
        );
    }
}

#[test]
fn saved_chart_callbacks_retain_only_their_data_after_catalog_release() {
    let mut runtime = RustSongRuntime::new(&populated()).unwrap();
    test_support::expose_catalog(&mut runtime).unwrap();
    test_support::release_catalog_root(&mut runtime).unwrap();
    test_support::retire_game_state_globals(&mut runtime).unwrap();
    test_support::retire_statistics_globals(&mut runtime).unwrap();
    execute(
        &mut runtime,
        r#"
        local song=catalog.song
        weak=setmetatable({song,song:GetAllSteps()[1],SONGMAN,catalog.current_song_options},{__mode='v'})
        saved=song.GetAllSteps
        catalog=nil;SONGMAN=nil
    "#,
    );
    execute(
        &mut runtime,
        r#"
        collectgarbage('collect');collectgarbage('collect')
        assert(weak[1]==nil and weak[3]==nil and weak[2]~=nil and weak[4]~=nil)
        assert(saved()[1]==weak[2]);saved=nil
    "#,
    );
    execute(
        &mut runtime,
        r#"collectgarbage('collect');collectgarbage('collect');assert(weak[2]==nil and weak[4]==nil)"#,
    );
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
