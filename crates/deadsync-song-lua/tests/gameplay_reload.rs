use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, SongLuaDifficulty, SongLuaStep, {ActorScene, SongRuntime, test_support},
};
use std::path::Path;

fn context(live: bool, enabled: [bool; 2]) -> SongLuaCompileContext {
    let mut c = SongLuaCompileContext::new(Path::new("."), "gameplay reconstruction");
    c.live = live;
    c.screen_width = 854.;
    c.screen_height = 480.;
    for (p, enabled) in c.players.iter_mut().zip(enabled) {
        p.enabled = enabled;
    }
    c.song_steps = [SongLuaDifficulty::Easy, SongLuaDifficulty::Hard]
        .into_iter()
        .enumerate()
        .map(|(i, difficulty)| SongLuaStep {
            steps_type: "StepsType_Dance_Single".into(),
            difficulty,
            description: format!("chart-{i}"),
            chart_name: String::new(),
            credit: String::new(),
            meter: 5 + i as u32,
            music_file: "same.ogg".into(),
            possible_grade_points: 100 + i as i32,
        })
        .collect();
    c.player_step_indices = [Some(0), Some(1)];
    c
}

struct Pair {
    rust: SongRuntime,
    context: SongLuaCompileContext,
    scenes: Vec<ActorScene>,
}
impl Pair {
    fn new(context: SongLuaCompileContext, full: bool) -> Self {
        let mut rust = SongRuntime::new(&context).unwrap();
        let mut scenes = Vec::new();
        if full {
            let (scene, selected) = rust.construct_gameplay_screen(&context).unwrap();
            assert!(rust.load_gameplay_theme(scene, &selected).unwrap());
            scenes.push(scene);
        }
        let mut p = Self {
            rust,
            context,
            scenes,
        };
        p.script(r#"
            observations={}
            function log(...)
                local t={};for i=1,select('#',...)do local v=select(i,...)
                    t[i]=type(v)=='number' and string.format('%.17g',v) or tostring(v)
                end;observations[#observations+1]=table.concat(t,',')
            end
            function screen_snapshot()
                local top=SCREENMAN:GetTopScreen();log(top:GetName(),#top)
                for _,child in ipairs(top)do
                    assert(child:GetParent()==top)
                    log(child.__songlua_actor_type,child:GetName(),child:GetX(),child:GetY(),
                        child:GetWidth(),child:GetHeight(),child:GetVisible(),
                        child.__songlua_state_draw_order,child.__songlua_hibernate_seconds)
                end
                local pos=GAMESTATE:GetSongPosition();log(pos:GetMusicSeconds(),pos:GetCurBPS())
                for _,pn in ipairs{PLAYER_1,PLAYER_2}do
                    local stats=STATSMAN:GetCurStageStats():GetPlayerStageStats(pn)
                    log(GAMESTATE:GetCurrentSteps(pn):GetDifficulty(),stats:GetScore(),stats:GetPossibleDancePoints())
                end
            end
        "#);
        p
    }
    fn script(&mut self, source: &str) {
        self.rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"@/fixture/gameplay-reload.lua").unwrap())
            .unwrap_or_else(|e| panic!("Rust: {source}\n{e:?}"));
        let Value::String(actual) = self
            .rust
            .vm_mut()
            .execute(
                &compile_named(b"return table.concat(observations,'|')", b"=observations").unwrap(),
            )
            .unwrap()[0]
        else {
            panic!()
        };
        reference_test::assert_debug(
            "song/tests/rust_gameplay_reload.rs::observations",
            &self.rust.vm_mut().string_bytes(actual).unwrap(),
        );
    }
    fn selected(&mut self) {
        let actual = self.rust.selected_gameplay_context(&self.context).unwrap();
        reference_test::assert_debug(
            "song/tests/rust_gameplay_reload.rs::selected",
            &(
                &actual.player_step_indices,
                actual.players.each_ref().map(|p| p.difficulty),
                &actual.style_name,
                &actual.player_timing,
                &actual.main_title,
            ),
        );
    }
    fn rebuild(&mut self) {
        let (scene, selected) = self
            .rust
            .reconstruct_gameplay_screen(&self.context)
            .unwrap();
        self.scenes.push(scene);
        assert!(self.rust.load_gameplay_theme(scene, &selected).unwrap());
        let errors = self.rust.take_actor_diagnostics();
        assert!(errors.is_empty(), "{errors:#?}");
        self.selected();
    }
}

#[test]
fn context_uses_retained_chart_identity_metadata_and_missing_selection_fallbacks() {
    for live in [false, true] {
        let mut p = Pair::new(context(live, [true, true]), false);
        p.selected();
        p.script(
            r#"
            local all=GAMESTATE:GetCurrentSong():GetAllSteps()
            all[2].GetDifficulty=function()error('mutable difficulty must not be read')end
            GAMESTATE:SetCurrentSteps(PLAYER_1,all[2])
            GAMESTATE.GetCurrentSteps=function()error('public getter must not be read')end
            collectgarbage('collect')
        "#,
        );
        p.selected();
        p.script("GAMESTATE:SetCurrentSteps(PLAYER_1,nil)");
        p.selected();
        // Missing song-list entries still retain their original constructor metadata.
        let mut c = context(live, [true, true]);
        c.song_steps.clear();
        c.player_step_indices = [None, None];
        c.players[0].difficulty = SongLuaDifficulty::Medium;
        let mut p = Pair::new(c, false);
        p.context.players[0].difficulty = SongLuaDifficulty::Edit;
        p.selected();
    }
}

#[test]
fn style_projection_keeps_native_fallback_reentry_and_read_order() {
    let mut p = Pair::new(context(true, [true, true]), false);
    p.script(
        r#"
        local all=GAMESTATE:GetCurrentSong():GetAllSteps()
        local old=GAMESTATE:GetCurrentStyle()
        style_reads=0;GAMESTATE.__songlua_current_style[1]={GetName=function()
            style_reads=style_reads+1;collectgarbage('collect');GAMESTATE:SetCurrentSteps(PLAYER_1,all[2]);return 'double'
        end,GetStepsType=old.GetStepsType}
    "#,
    );
    p.selected();
    p.selected();
    p.script("assert(style_reads==2)");
    for source in [
        "GAMESTATE.__songlua_current_style[1]={__songlua_style_key='couple',GetName=function()error('not used')end}",
        "GAMESTATE.__songlua_current_style[1]={GetName=function()error('style getter')end}",
        "GAMESTATE.__songlua_current_style=nil;GAMESTATE.GetCurrentStyle=function()collectgarbage('collect');return {GetName=function()return 'routine'end}end",
    ] {
        p.script(source);
        p.selected();
    }
}

#[test]
fn initial_screen_seeds_nonhuman_chart_and_loads_all_original_phases() {
    for live in [false, true] {
        for enabled in [[true, false], [false, true], [true, true], [false, false]] {
            let mut p = Pair::new(context(live, enabled), true);
            p.selected();
            p.script("screen_snapshot()");
            assert!(p.rust.take_actor_diagnostics().is_empty());
        }
    }
}

#[test]
fn rebuild_preserves_process_state_and_rng_but_resets_options_statistics_clock_and_subscriptions() {
    for live in [false, true] {
        let mut p = Pair::new(context(live, [true, false]), true);
        p.script(r#"
            OLD=SCREENMAN:GetTopScreen();OLD_SL=SL;OLD_GAMESTATE=GAMESTATE
            GET_OLD_PLAYER=GetPlayerAF;old_hits=0
            local h=OLD:GetChild('Header');h:addcommand('ReloadPingMessage',function()old_hits=old_hits+1 end)
            GAMESTATE:Env().Retained=73;arbitrary_global={value=19}
            local state=GAMESTATE:GetPlayerState(PLAYER_1)
            state:GetPlayerOptions('ModsLevel_Preferred'):Reverse(0.25)
            state:GetPlayerOptions('ModsLevel_Stage'):Reverse(0.75)
            state:GetPlayerOptions('ModsLevel_Song'):Reverse(0.1)
            state:GetPlayerOptions('ModsLevel_Current'):Reverse(0.3)
            STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1):SetScore(123)
            __songlua_compile_song_runtime.__songlua_music_seconds=9.5
            __songlua_compile_song_runtime.__songlua_song_bps=3
            GAMESTATE:SetCurrentSteps(PLAYER_1,GAMESTATE:GetCurrentSong():GetAllSteps()[2])
            log(math.random(),math.random());collectgarbage('collect')
        "#);
        p.rebuild();
        p.script(r#"
            assert(SCREENMAN:GetTopScreen()~=OLD and SL==OLD_SL and GAMESTATE==OLD_GAMESTATE)
            assert(GAMESTATE:Env().Retained==73 and arbitrary_global.value==19)
            assert(GET_OLD_PLAYER(PLAYER_1)~=GetPlayerAF(PLAYER_1))
            assert(OLD:GetChild('Header'):GetName()=='Header')
            MESSAGEMAN:Broadcast('ReloadPing');assert(old_hits==0)
            log(math.random(),math.random())
            local state=GAMESTATE:GetPlayerState(PLAYER_1)
            for _,level in ipairs{'Preferred','Stage','Song','Current'}do log(state:GetPlayerOptions('ModsLevel_'..level):Reverse())end
            screen_snapshot()
        "#);
    }
}

#[test]
fn replacement_resets_screen_input_map_and_keeps_saved_methods_bound_to_new_map() {
    let mut p = Pair::new(context(true, [true, false]), true);
    p.script(
        r#"
        OLD=SCREENMAN:GetTopScreen();SAVE_ADD=OLD.AddInputCallback
        callback=function()return true end;OLD:AddInputCallback(callback)
    "#,
    );

    assert_eq!(test_support::input_functions(&p.rust).unwrap().len(), 1);
    p.rebuild();

    assert!(
        test_support::input_functions(&p.rust)
            .unwrap_or_default()
            .is_empty()
    );
    p.script("SAVE_ADD(OLD,callback)");

    assert_eq!(test_support::input_functions(&p.rust).unwrap().len(), 1);
}

#[test]
fn failed_theme_load_keeps_published_partial_screen_owned_until_explicit_release() {
    for live in [false, true] {
        let mut p = Pair::new(context(live, [true, false]), true);
        p.script(
            r#"
            local ctor=Def.ActorFrame
            Def.ActorFrame=function(t)
                t.OnCommand=function(self)FAILED=self;error('rebuild On failure')end
                return ctor(t)
            end
        "#,
        );

        let (scene, selected) = p.rust.reconstruct_gameplay_screen(&p.context).unwrap();
        p.scenes.push(scene);
        let rust = p.rust.load_gameplay_theme(scene, &selected).unwrap_err();

        assert!(rust.to_string().contains("rebuild On failure"));
        p.script(r#"
            assert(FAILED and SCREENMAN:GetTopScreen())
            log(SCREENMAN:GetTopScreen():GetName(),GAMESTATE:GetSongPosition():GetMusicSeconds())
            WEAK=setmetatable({SCREENMAN:GetTopScreen()},{__mode='v'})
            FAILED=nil;__songlua_top_screen=nil;__songlua_top_screen_player_1=nil;__songlua_top_screen_player_2=nil
            GetPlayerAF=nil;MusicRateDisplay=nil;dispP1=nil;dispP2=nil
        "#);
        for scene in p.scenes.drain(..) {
            assert!(p.rust.release_actor_scene(scene).unwrap());
        }
        p.rust
            .vm_mut()
            .execute(
                &compile_named(
                    b"collectgarbage('collect');collectgarbage('collect');assert(WEAK[1]==nil)",
                    b"=rebuild release",
                )
                .unwrap(),
            )
            .unwrap();
    }
}

#[test]
fn selected_context_uses_chart_timing_and_keeps_supplied_fallback_when_timing_is_missing() {
    use deadsync_rules::timing::{TimingData, TimingSegments};
    let timing = |offset, bpm| {
        TimingData::from_segments(
            offset,
            0.,
            &TimingSegments {
                bpms: vec![(0., bpm)],
                ..Default::default()
            },
            &[],
        )
    };
    let mut c = context(true, [true, true]);
    c.song_step_timing = vec![timing(1., 90.), timing(2., 180.)];
    c.player_timing = [Some(timing(3., 240.)), None];
    let mut p = Pair::new(c, false);
    p.selected();
    p.script("GAMESTATE:SetCurrentSteps(PLAYER_1,GAMESTATE:GetCurrentSong():GetAllSteps()[2])");
    p.selected();
    p.context.song_step_timing.clear();
    p.selected();
    p.script("GAMESTATE:SetCurrentSteps(PLAYER_1,nil)");
    p.selected();
}

#[test]
fn repeated_reconstruction_releases_outgoing_scene_roots_after_collection() {
    for live in [false, true] {
        let mut p = Pair::new(context(live, [true, false]), true);
        for _ in 0..2 {
            p.script(r#"
                root=SCREENMAN:GetTopScreen();WEAK=setmetatable({root},{__mode='v'})
                __songlua_compile_song_runtime.__songlua_music_seconds=12.5
                root:GetChild('Overlay'):addcommand('End',function(self)
                    log('End',GAMESTATE:GetSongPosition():GetMusicSeconds(),SCREENMAN:GetTopScreen()==nil)
                end)
            "#);
            let outgoing = *p.scenes.last().unwrap();

            assert!(p.rust.end_actor_scene(outgoing).unwrap());
            p.script("root=nil;assert(observations[#observations]=='End,12.5,true')");
            p.rebuild();
            assert!(p.rust.release_actor_scene(outgoing).unwrap());

            p.rust
                .vm_mut()
                .execute(
                    &compile_named(
                        b"collectgarbage('collect');collectgarbage('collect');assert(WEAK[1]==nil)",
                        b"=retired Rust screen",
                    )
                    .unwrap(),
                )
                .unwrap();
            p.script("log('released outgoing screen')");
        }
    }
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
