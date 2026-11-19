#![cfg(feature = "test-support")]

use deadlib_lua::compile_named;
use deadsync_song_lua::{
    SongLuaCompileContext, SongLuaDifficulty, SongLuaStep, SongRuntime, test_support,
};
use std::path::Path;

fn chart(kind: &str, difficulty: SongLuaDifficulty, name: &str, meter: u32) -> SongLuaStep {
    SongLuaStep {
        steps_type: kind.into(),
        difficulty,
        description: String::new(),
        chart_name: name.into(),
        credit: String::new(),
        meter,
        music_file: String::new(),
        possible_grade_points: 0,
    }
}

fn context(charts: Vec<SongLuaStep>, live: bool) -> SongLuaCompileContext {
    let mut context =
        SongLuaCompileContext::new(Path::new("/fixture/playable-steps"), "Playable steps");
    context.live = live;
    context.style_name = "versus".into();
    context.song_steps = charts;
    context.player_step_indices = [Some(0), Some(0)];
    context
}

fn execute(context: &SongLuaCompileContext, source: &str) {
    let mut runtime = SongRuntime::new(context).unwrap();
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/playable-steps.lua").unwrap())
        .unwrap_or_else(|error| panic!("{source}\n{error:?}"));
}

#[test]
fn playable_steps_orders_fiji_difficulties_without_replacing_selected_charts() {
    use SongLuaDifficulty::{Challenge, Hard};
    for live in [false, true] {
        let context = context(
            vec![
                chart("StepsType_Dance_Single", Challenge, "challenge", 9),
                chart("StepsType_Dance_Single", Hard, "hard", 8),
            ],
            live,
        );
        execute(
            &context,
            r#"
            local song = GAMESTATE:GetCurrentSong()
            local all = song:GetAllSteps()
            assert(all[1]:GetDifficulty() == 'Difficulty_Challenge')
            assert(all[2]:GetDifficulty() == 'Difficulty_Hard')
            assert(GAMESTATE:GetCurrentSteps(PLAYER_1) == all[1])
            assert(GAMESTATE:GetCurrentSteps(PLAYER_2) == all[1])

            -- The retained Fiji loop selects P2's Hard chart as it visits the list.
            local events = {}
            local playable = SongUtil.GetPlayableSteps(song)
            assert(playable[1] == all[2] and playable[2] == all[1])
            for _, steps in ipairs(playable) do
                local difficulty = steps:GetDifficulty()
                events[#events + 1] = difficulty
                if difficulty == 'Difficulty_Hard' then
                    GAMESTATE:SetCurrentSteps(PLAYER_2, steps)
                    events[#events + 1] = 'set P2 Hard'
                end
            end
            assert(table.concat(events, ',') ==
                'Difficulty_Hard,set P2 Hard,Difficulty_Challenge')
            assert(GAMESTATE:GetCurrentSteps(PLAYER_1) == all[1])
            assert(GAMESTATE:GetCurrentSteps(PLAYER_2) == all[2])
            assert(song:GetAllSteps()[1] == all[1] and song:GetAllSteps()[2] == all[2])
            assert(playable ~= SongUtil.GetPlayableSteps(song))
            playable[1] = false
            assert(SongUtil.GetPlayableSteps(song)[1] == all[2])
        "#,
        );
    }
}

#[test]
fn playable_steps_use_native_enum_type_and_difficulty_priority() {
    use SongLuaDifficulty::*;
    let context = context(
        vec![
            chart("StepsType_Dance_Double", Beginner, "double", 1),
            chart("StepsType_Dance_Couple", Beginner, "couple", 1),
            chart("StepsType_Dance_Single", Edit, "edit", 1),
            chart("StepsType_Dance_Single", Challenge, "challenge", 2),
            chart("StepsType_Dance_Single", Hard, "hard", 20),
            chart("StepsType_Dance_Single", Medium, "medium", 30),
            chart("StepsType_Dance_Single", Easy, "easy", 40),
            chart("StepsType_Dance_Single", Beginner, "beginner", 50),
        ],
        false,
    );
    execute(
        &context,
        r#"
        local song = GAMESTATE:GetCurrentSong()
        local all = song:GetAllSteps()
        local playable = SongUtil.GetPlayableSteps(song)
        -- Native enum ranks, not spelling or meter, determine these primary keys.
        -- This checks the supplied catalog; eligibility is outside this change.
        local expected = {8, 7, 6, 5, 4, 3, 1, 2}
        assert(#playable == #expected)
        for index, original in ipairs(expected) do
            assert(playable[index] == all[original])
        end
        assert(GAMESTATE:GetCurrentSteps(PLAYER_1) == all[1])
        assert(GAMESTATE:GetCurrentSteps(PLAYER_2) == all[1])
        local after = song:GetAllSteps()
        for index, steps in ipairs(all) do assert(after[index] == steps) end
    "#,
    );
}

#[test]
fn playable_steps_keep_equal_key_objects_distinct_and_in_catalog_order() {
    use SongLuaDifficulty::{Edit, Hard};
    let mut context = context(
        vec![
            chart("StepsType_Dance_Single", Edit, "first edit", 7),
            chart("StepsType_Dance_Single", Hard, "hard", 7),
            chart("StepsType_Dance_Single", Edit, "second edit", 7),
            chart("StepsType_Dance_Single", Edit, "third edit", 7),
        ],
        false,
    );
    context.player_step_indices = [Some(2), Some(3)];
    execute(
        &context,
        r#"
        local song = GAMESTATE:GetCurrentSong()
        local all = song:GetAllSteps()
        local playable = SongUtil.GetPlayableSteps(song)
        -- Equal-key preservation is an owned identity contract, not a claim about
        -- native std::sort permutations or unavailable P1 radar tie breakers.
        assert(playable[1] == all[2])
        assert(playable[2] == all[1])
        assert(playable[3] == all[3])
        assert(playable[4] == all[4])
        assert(all[1] ~= all[3] and all[3] ~= all[4])
        assert(GAMESTATE:GetCurrentSteps(PLAYER_1) == all[3])
        assert(GAMESTATE:GetCurrentSteps(PLAYER_2) == all[4])
        assert(song:GetAllSteps()[3] == all[3])
    "#,
    );
}

#[test]
fn playable_steps_preserve_live_getters_without_sorting_queries_or_mutation() {
    use SongLuaDifficulty::{Challenge, Hard};
    let context = context(
        vec![
            chart("StepsType_Dance_Single", Challenge, "challenge", 9),
            chart("StepsType_Dance_Single", Hard, "hard", 8),
        ],
        false,
    );
    execute(
        &context,
        r#"
        local song = GAMESTATE:GetCurrentSong()
        local provided = song:GetAllSteps()
        for _, steps in ipairs(provided) do
            for _, name in ipairs {'GetStepsType', 'GetDifficulty', 'GetMeter',
                'GetDescription', 'GetRadarValues'} do
                steps[name] = function() error('sorting must use captured identity') end
            end
        end
        local calls = 0
        song.GetAllSteps = function(self, ...)
            assert(self == song and select('#', ...) == 0)
            calls = calls + 1
            collectgarbage('collect')
            return provided
        end
        song.__songlua_steps = nil
        local playable = SongUtil.GetPlayableSteps(song)
        assert(calls == 1)
        assert(playable ~= provided)
        assert(playable[1] == provided[2] and playable[2] == provided[1])
        assert(provided[1] == GAMESTATE:GetCurrentSteps(PLAYER_1))
        playable[1] = false
        assert(provided[2] ~= false)

        -- Existing proxy results and the type-filtered compatibility helper keep
        -- their delegated getter behavior and original chart order.
        local proxy = {GetAllSteps=function() return provided end}
        assert(SongUtil.GetPlayableSteps(proxy) == provided)
        local typed = SongUtil:GetPlayableStepsByStepsType(song, 'StepsType_Dance_Single')
        assert(typed[1] == provided[1] and typed[2] == provided[2])
        local foreign = {false, 'authored replacement'}
        song.GetAllSteps = function() return foreign end
        assert(SongUtil.GetPlayableSteps(song) == foreign)
    "#,
    );
}

#[test]
fn saved_playable_utility_does_not_own_retired_songs_or_charts() {
    use SongLuaDifficulty::{Challenge, Hard};
    for live in [false, true] {
        let context = context(
            vec![
                chart("StepsType_Dance_Single", Challenge, "challenge", 9),
                chart("StepsType_Dance_Single", Hard, "hard", 8),
            ],
            live,
        );
        let mut runtime = SongRuntime::new(&context).unwrap();
        test_support::expose_catalog(&mut runtime).unwrap();
        test_support::release_catalog_root(&mut runtime).unwrap();
        test_support::retire_game_state_globals(&mut runtime).unwrap();
        test_support::retire_statistics_globals(&mut runtime).unwrap();
        for source in [
            r#"
            saved_song = catalog.song
            local all = saved_song:GetAllSteps()
            weak = setmetatable({saved_song, all[1], all[2], catalog.current_song_options}, {__mode='v'})
            saved_util = SongUtil.GetPlayableSteps
            catalog = nil; SONGMAN = nil; SongUtil = nil
            local playable = saved_util(saved_song)
            assert(playable[1] == all[2] and playable[2] == all[1])
            "#,
            r#"
            collectgarbage('collect'); collectgarbage('collect')
            assert(saved_util(saved_song)[1] == weak[3])
            saved_steps = saved_song.GetAllSteps
            saved_song = nil
            "#,
            r#"
            collectgarbage('collect'); collectgarbage('collect')
            assert(weak[1] == nil)
            assert(weak[2] ~= nil and weak[3] ~= nil and weak[4] ~= nil)
            assert(saved_steps()[1] == weak[2] and saved_steps()[2] == weak[3])
            saved_steps = nil
            "#,
            r#"
            collectgarbage('collect'); collectgarbage('collect')
            assert(weak[2] == nil and weak[3] == nil and weak[4] == nil)
            assert(type(saved_util) == 'function')
            assert(#saved_util({GetAllSteps=function() return {} end}) == 0)
            saved_util = nil
            "#,
        ] {
            runtime
                .vm_mut()
                .execute(
                    &compile_named(source.as_bytes(), b"@/fixture/playable-ownership.lua").unwrap(),
                )
                .unwrap_or_else(|error| panic!("{source}\n{error:?}"));
        }
    }
}
