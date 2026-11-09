use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext,
    rust_engine::{ActorScene, RustSongRuntime, test_support},
};
use std::path::Path;

struct Pair {
    rust: RustSongRuntime,
    scenes: Vec<ActorScene>,
}
impl Pair {
    fn new(live: bool, enabled: [bool; 2]) -> Self {
        let mut context = SongLuaCompileContext::new(Path::new("."), "screen control migration");
        context.live = live;
        for (player, enabled) in context.players.iter_mut().zip(enabled) {
            player.enabled = enabled;
        }

        let mut rust = RustSongRuntime::new(&context).unwrap();
        let scenes = test_support::top_screen_controls(&mut rust, &context).unwrap();
        let mut pair = Self { rust, scenes };
        pair.script(
            r#"
            TOP=__songlua_top_screen; observations={}; __songlua_side_effect_count=0
            function log(...)
                local t={};for i=1,select('#',...) do local v=select(i,...)
                    t[i]=type(v)=='number' and string.format('%.9g',v) or tostring(v)
                end;observations[#observations+1]=table.concat(t,',')
            end
        "#,
        );
        pair
    }
    fn script(&mut self, source: &str) {
        self.rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"@/fixture/top-screen.lua").unwrap())
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
            "song/tests/rust_top_screen.rs::observations",
            &self.rust.vm_mut().string_bytes(actual).unwrap(),
        );
    }
    fn subtree(&mut self, source: &str) {
        let value = self
            .rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"=subtree").unwrap())
            .unwrap()[0];
        let scene = self.rust.instantiate_actor(value).unwrap();
        self.scenes.push(scene);
        let value = self.rust.actor_scene_root(scene).unwrap();
        self.rust.vm_mut().set_global(b"SUB", value).unwrap();
    }
}

#[test]
fn screen_controls_preserve_bound_receivers_defaults_and_argument_rules() {
    for live in [false, true] {
        let mut p = Pair::new(live, [true, true]);
        p.script(r#"
            assert(TOP==SCREENMAN:GetTopScreen())
            log(TOP:GetName(),TOP.__songlua_actor_type,TOP:GetScreenType(),TOP:GetEditState(),TOP:GetGoToOptions(),TOP:AllAreOnLastRow())
            log(TOP.__songlua_main_title,TOP.__songlua_bpm_text,TOP:GetNumRows(),TOP:GetCurrentRowIndex(),TOP:IsPaused())
            assert(TopScreen==nil and OptionRow==nil and OptionRowFrame==nil)
            local next=TOP.SetNextScreenName;assert(next('next')==TOP and next({},'again')==TOP)
            log(TOP:GetNextScreenName());next(123);log(TOP:GetNextScreenName())
            TOP:SetPrevScreenName('prev');log(TOP:GetPrevScreenName());TOP:SetPrevScreenName(false);log(TOP:GetPrevScreenName())
            for _,n in ipairs{2.5,-1.6,'4.6','not a row',false} do
                assert(TOP:SetOptionRowIndex(PLAYER_1,n)==TOP);log(TOP:GetCurrentRowIndex())
            end
            TOP:SetOptionRowIndex(7);log(TOP:GetCurrentRowIndex());TOP:SetOptionRowIndex(9,nil);log(TOP:GetCurrentRowIndex())
            TOP.__songlua_current_row_index='2.9';log(TOP:GetCurrentRowIndex())
            TOP:PauseGame(0);log(TOP:IsPaused());TOP:PauseGame(false);log(TOP:IsPaused())
            for _,name in ipairs{'RedrawOptions','PostScreenMessage','SetProfileIndex','Cancel','Continue','Finish','Load','begin_backing_out'} do
                assert(TOP[name]({},'ignored')==TOP)
            end
            log(__songlua_side_effect_count)
        "#);
    }
}

#[test]
fn screen_lead_in_uses_typed_receiver_range_and_no_side_effect() {
    let mut p = Pair::new(true, [true, false]);
    p.script(r#"
        local set=TOP.SetMinSecondsToMusic;local other={}
        assert(set(other,'2.75')==other and other.__songlua_min_seconds_to_music==2.75)
        assert(TOP.__songlua_min_seconds_to_music==nil)
        for _,n in ipairs{0,60,0/0,1/0,-0.1,60.1,'5','bad',false,{}} do
            local ok=pcall(set,TOP,n);log(ok,TOP.__songlua_min_seconds_to_music)
        end
        log((pcall(function()set()end)),(pcall(function()set(TOP)end)),(pcall(function()set(3,4)end)))
        log(__songlua_side_effect_count)
    "#);
}

#[test]
fn player_and_life_lookups_capture_enabled_owners_without_creating_missing_children() {
    for enabled in [[true, false], [false, true], [true, true]] {
        let mut p = Pair::new(true, enabled);
        p.script(
            r#"
            local detached=TOP.GetChild;assert(detached==ActorFrame.GetChild)
            log((pcall(function()detached()end)),(pcall(function()detached(12)end)))
            local child=TOP.__songlua_bound_methods.GetChild;local meter=TOP.GetLifeMeter
            for _,name in ipairs{'PlayerP1','playerp2','LifeP1','lifep2','missing'} do
                local a=child(name);log(name,a and a:GetName())
            end
            for _,pn in ipairs{PLAYER_1,PLAYER_2,'p1','P2',0,1,0.5,2,false} do
                local a=meter(pn);log(a and a:GetName())
            end
            log(child(),child(12),meter(),meter('0'))
            local old=child('PlayerP1');local life=meter(PLAYER_1)
            TOP.__songlua_player1_enabled=false
            __songlua_top_screen={};GAMESTATE={GetEnabledPlayers=function()return{}end}
            collectgarbage('collect');assert(child({},'PlayerP1')==old and meter({},PLAYER_1)==life)
            log(__songlua_side_effect_count)
        "#,
        );
    }
}

#[test]
fn option_rows_preserve_names_generated_groups_defaults_and_fresh_identity() {
    let mut p = Pair::new(false, [true, true]);
    p.script(r#"
        for i=0,TOP:GetNumRows()+1 do
            local row=TOP:GetOptionRow(i);local frame=row:GetChild('');local items=frame:GetChild('Item')
            assert(row:GetParent()==nil and frame:GetParent()==row and items[1]:GetParent()==frame)
            assert(row~=TOP:GetOptionRow(i) and items[1]~=items[2])
            log(i,row:GetName(),row.__songlua_actor_type,frame.__songlua_actor_type,row:GetChoiceInRowWithFocus(),row:GetNumChildren(),frame:GetNumChildren())
            log(frame:GetChild('Title'):GetText(),items[1]:GetText(),items[2]:GetText(),type(row.SetUpdateFunction),type(frame.SetUpdateFunction))
        end
        for _,key in ipairs{'SpeedMod','speedmod','2',-1,1.5,false,string.char(255)} do
            log(TOP:GetOptionRow(key):GetName())
        end
        local row=TOP:GetOptionRow('SpeedMod');local frame=row:GetChild('')
        row:name('renamed');row.Name='authored';assert(row:GetName()=='SpeedMod')
        frame:GetChild('Item'):settext('C400');assert(frame:GetChild('Item')[1]:GetText()=='1')
        assert(frame:GetChild('Item')[2]:GetText()=='C400');frame:GetChild('Title'):settext('Title')
        log(row:GetChild(''):GetChild('Title'):GetText(),TOP:GetOptionRow('SpeedMod'):GetChild(''):GetChild('Item')[1]:GetText())
        collectgarbage('collect');assert(row:GetChild('')==frame);log(__songlua_side_effect_count)
    "#);
}

#[test]
fn music_wheel_retains_shared_backing_and_counts_only_existing_side_effects() {
    let mut p = Pair::new(true, [true, false]);
    p.script(r#"
        local wheel=TOP:GetMusicWheel();assert(wheel==TOP.GetMusicWheel({},1))
        log(wheel:IsLocked(),wheel:GetSelectedSection(),wheel:GetSelectedSong(),wheel:GetSelectedType(),wheel:GetSortOrder())
        assert(wheel:ChangeSort('SortOrder_Title')==wheel);log(wheel:GetSortOrder(),GAMESTATE:GetSortOrder())
        wheel.ChangeSort({},'custom');log(wheel:GetSortOrder(),GAMESTATE:GetSortOrder())
        wheel:ChangeSort(12);wheel:ChangeSort(false);log(wheel:GetSortOrder())
        assert(wheel.Move()==wheel and wheel:SetOpenSection('section')==wheel)
        local song=GAMESTATE:GetCurrentSong();assert(TOP:GetNextCourseSong()==song)
        GAMESTATE:SetCurrentSong(nil);assert(TOP:GetNextCourseSong()==song);
        local replacement={};GAMESTATE:SetCurrentSong(replacement);assert(TOP:GetNextCourseSong()==replacement);GAMESTATE:SetCurrentSong(song)
        local current=TOP.GetNextCourseSong;local sort=wheel.GetSortOrder
        GAMESTATE={};collectgarbage('collect');assert(current()==song);log(sort())
        log(__songlua_side_effect_count)
    "#);
}

#[test]
fn transitions_keep_off_order_out_activation_and_request_commit_boundaries() {
    for live in [false, true] {
        let mut p = Pair::new(live, [true, false]);
        p.subtree(r#"return Def.ActorFrame{Name='A',OffCommand=function(self)log('offA');collectgarbage('collect')end,
            Def.Actor{Name='B',OffCommand=function(self)log('offB')end},
            Def.ActorFrame{Name='Out',OffCommand=function(self)log('offOut')end,
                Def.Actor{Name='visual',StartTransitioningCommand=function(self)log('startVisual')end}}
        }"#);
        p.script(
            r#"
            for i=#TOP,1,-1 do TOP[i]=nil end
            TOP.__songlua_children={};TOP.__songlua_owned_child_lookup=true
            TOP[1]=SUB;TOP[2]=SUB:GetChild('Out');TOP[2].__songlua_transition_state=0
            TOP[2].__songlua_transition_child=TOP[2][1]
            TOP.OffCommand=function()error('screen itself must not receive Off')end
        "#,
        );

        p.rust.begin_screen_request_capture();
        p.script("assert(TOP:StartTransitioningScreen('SM_GoToNextScreen')==TOP);log(__songlua_side_effect_count,TOP[2].__songlua_transition_state)");
        let actual = p.rust.take_screen_requests();
        reference_test::assert_debug("song/tests/rust_top_screen.rs::requests", &actual);

        p.script("TOP.__songlua_children.Out=TOP[2];assert(TOP:StartTransitioningScreen('activate')==TOP);log(TOP[2].__songlua_transition_state)");
        let actual = p.rust.take_screen_requests();
        reference_test::assert_debug("song/tests/rust_top_screen.rs::requests", &actual);
        p.script("local ok=pcall(function()TOP:StartTransitioningScreen(false)end);log(ok,__songlua_side_effect_count)");
        p.script("SUB.OffCommand=function()error('off failure')end;local ok=pcall(function()TOP:StartTransitioningScreen('bad')end);log(ok,__songlua_side_effect_count)");
        let actual = p.rust.take_screen_requests();
        reference_test::assert_debug("song/tests/rust_top_screen.rs::requests", &actual);
    }
}

#[test]
fn screen_getters_and_setters_allow_reentry_and_keep_partial_writes_on_errors() {
    let mut p = Pair::new(true, [true, false]);
    p.script(r#"
        local gets=0;local sets=0;local next=TOP.GetNextScreenName;local set=TOP.SetNextScreenName
        TOP.__songlua_next_screen_name=nil
        setmetatable(TOP,{__index=function(t,k)
            if k=='__songlua_next_screen_name' then gets=gets+1;collectgarbage('collect');return 12 end
        end,__newindex=function(t,k,v)sets=sets+1;collectgarbage('collect');rawset(t,k,v)end})
        log(next(),gets);assert(set('target')==TOP);log(next(),sets,__songlua_side_effect_count)
        __songlua_side_effect_count='broken';local ok=pcall(function()set('written')end)
        log(ok,next());__songlua_side_effect_count=0
        TOP.__songlua_next_screen_name={};log((pcall(function()return next()end)))
    "#);
}

#[test]
fn retired_screen_is_collectible_while_saved_independent_controls_remain_usable() {
    let mut p = Pair::new(true, [true, false]);
    p.script(r#"
        SAVED_WHEEL=TOP:GetMusicWheel();SAVED_ROW=TOP.GetOptionRow;SAVED_CONSTANT=TOP.GetScreenType
        WEAK=setmetatable({TOP},{__mode='v'})
        TOP=nil;__songlua_top_screen=nil;__songlua_top_screen_player_1=nil;__songlua_top_screen_player_2=nil;GetPlayerAF=nil
    "#);
    for scene in std::mem::take(&mut p.scenes) {
        assert!(p.rust.release_actor_scene(scene).unwrap());
    }
    // Unlike the legacy oracle's process-long registry, the new host explicitly
    // retires scenes. Saved independent controls must not keep retired tables.
    p.rust
        .vm_mut()
        .execute(
            &compile_named(
                br#"
        collectgarbage('collect');collectgarbage('collect');assert(WEAK[1]==nil)
    "#,
                b"=retired screen ownership",
            )
            .unwrap(),
        )
        .unwrap();
    p.script(r#"
        assert(SAVED_CONSTANT()=='ScreenType_Gameplay')
        SAVED_WHEEL:ChangeSort('SortOrder_Artist');log(SAVED_WHEEL:GetSortOrder(),GAMESTATE:GetSortOrder())
        local row=SAVED_ROW('Mini');collectgarbage('collect')
        assert(row:GetChild(''):GetChild('Item')[1]:GetText()=='0%');log(row:GetName())
    "#);
}

#[test]
fn preparation_row_capture_and_ownership_survive_screen_retirement() {
    let mut p = Pair::new(false, [true, false]);
    p.script(r#"
        ROW=TOP:GetOptionRow('MusicRate');ITEM=ROW:GetChild(''):GetChild('Item')[2]
        ITEM:x(2):linear(0.5):x(15):diffusealpha(0.25)
        log(ITEM:GetX(),ITEM:GetDestX(),ITEM:GetTweenTimeLeft())
        WEAK=setmetatable({ROW,ITEM},{__mode='v'});TOP=nil
        __songlua_top_screen=nil;__songlua_top_screen_player_1=nil;__songlua_top_screen_player_2=nil;GetPlayerAF=nil
    "#);

    let item = p.rust.vm_mut().get_global(b"ITEM").unwrap();
    reference_test::assert_debug(
        "song/tests/rust_top_screen.rs::blocks",
        &p.rust.actor_command_capture(item).unwrap().blocks,
    );
    for scene in std::mem::take(&mut p.scenes) {
        assert!(p.rust.release_actor_scene(scene).unwrap());
    }
    p.script(
        r#"
        ROW=nil;ITEM=nil;collectgarbage('collect');collectgarbage('collect')
        assert(WEAK[1]~=nil and WEAK[2]~=nil and WEAK[1]:GetChild(''):GetChild('Item')[2]==WEAK[2])
        log(WEAK[1]:GetName(),WEAK[2]:GetX(),WEAK[2]:GetTweenTimeLeft())
    "#,
    );
}

#[test]
fn default_screen_fixture_constructs_complete_players_with_real_font_helper() {
    for live in [false, true] {
        let mut p = Pair::new(live, [true, true]);
        p.script(r#"
            assert(#GetComboFonts()==0)
            for _,name in ipairs{'PlayerP1','PlayerP2'} do
                local player=TOP:GetChild(name)
                local combo=player:GetChild('Combo')
                local judgment=player:GetChild('Judgment')
                local field=player:GetChild('NoteField')
                local holds=player.__songlua_hold_judgments
                local columns=#field:GetColumnActors()
                assert(#holds==columns and #player==columns+4)
                assert(player[2]==combo and player[3]==judgment and player[#player]==field)
                assert(combo:GetParent()==player and judgment:GetParent()==player and field:GetParent()==player)
                assert(combo.__songlua_actor_type=='Actor' and not combo:GetVisible())
                assert(judgment:GetChild('JudgmentWithOffsets')==judgment[1])
                for i,hold in ipairs(holds) do
                    assert(hold==player[i+3] and hold:GetParent()==player)
                end
                log(player:GetNumChildren(),judgment:GetNumChildren(),field:GetNumChildren(),columns)
            end
        "#);
    }
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
