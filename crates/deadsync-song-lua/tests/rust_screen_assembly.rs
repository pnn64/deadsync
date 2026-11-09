use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext,
    rust_engine::{ActorScene, RustSongRuntime, test_support},
};
use std::path::Path;

struct Pair {
    rust: RustSongRuntime,
    scene: ActorScene,
}
impl Pair {
    fn new(context: &SongLuaCompileContext, setup: &str) -> Self {
        Self::with_retained_classes(context, setup, false)
    }
    fn with_retained_classes(
        context: &SongLuaCompileContext,
        setup: &str,
        _retained: bool,
    ) -> Self {
        // The production host installs these public classes after its initial
        // screen constructor, before any authored song can inspect the tree.

        let mut rust = RustSongRuntime::new(context).unwrap();
        rust.vm_mut()
            .execute(&compile_named(setup.as_bytes(), b"=screen setup").unwrap())
            .unwrap();
        let scene = test_support::gameplay_screen(&mut rust, context).unwrap();
        let mut pair = Self { rust, scene };
        pair.script(r#"
            TOP=SCREENMAN:GetTopScreen();root=TOP;observations={}
            function log(...)
                local t={};for i=1,select('#',...) do local v=select(i,...)
                    t[i]=type(v)=='number' and string.format('%.9g',v) or tostring(v)
                end;observations[#observations+1]=table.concat(t,',')
            end
            function role(parent,name)
                for _,actor in ipairs(parent) do
                    if actor.__songlua_underlay_role==name or actor.__songlua_screen_actor_role==name then return actor end
                end
            end
        "#);
        pair
    }
    fn script(&mut self, source: &str) {
        self.rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"@/fixture/screen-assembly.lua").unwrap())
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
        let actual = self.rust.vm_mut().string_bytes(actual).unwrap();
        reference_test::assert_debug("song/tests/rust_screen_assembly.rs::observations", &actual);
    }
}

fn context(live: bool, enabled: [bool; 2]) -> SongLuaCompileContext {
    let mut context = SongLuaCompileContext::new(Path::new("."), "screen assembly migration");
    context.live = live;
    for (player, enabled) in context.players.iter_mut().zip(enabled) {
        player.enabled = enabled;
    }
    context
}

#[test]
fn screen_tree_preserves_enabled_children_parents_roles_and_stable_draw_order() {
    for live in [false, true] {
        for enabled in [[true, false], [false, true], [true, true]] {
            let mut p = Pair::new(&context(live, enabled), "");
            p.script(r#"
                assert(TOP==__songlua_top_screen)
                local group=TOP:GetChild('');assert(#group==5)
                local roles={Toasty=true,NextSong=true,Ready=true,Go=true,Failed=true}
                for _,actor in ipairs(group) do
                    assert(actor:GetParent()==TOP and roles[actor.__songlua_screen_actor_role])
                    assert(actor.__songlua_transition_state==0)
                end
                assert(not role(TOP,'Toasty'):GetVisible())
                local seen={};local previous=-math.huge
                for i,actor in ipairs(TOP) do
                    assert(not seen[actor] and actor:GetParent()==TOP);seen[actor]=true
                    local order=actor.__songlua_state_draw_order;assert(order>=previous);previous=order
                    log(i,actor:GetName(),actor.__songlua_actor_type,actor.__songlua_screen_actor_role,
                        order,actor.__songlua_theme_hibernating,actor:GetVisible(),actor.GetNumChildren and actor:GetNumChildren() or -1)
                end
                assert(TOP:GetNumChildren()==#TOP)
                for i,pn in ipairs{PLAYER_1,PLAYER_2} do
                    local player=_G['__songlua_top_screen_player_'..i]
                    assert(player:GetParent()==TOP and player:GetChild('NoteField'):GetParent()==player)
                    local meter=player.__songlua_life_meter
                    assert(meter:GetParent()==TOP and meter:GetVisible() and meter.__songlua_theme_hibernating)
                    assert(meter:GetNumChildren()==0 and next(meter:GetChildren())==nil and meter:GetChild('missing')==nil)
                    local count=meter.GetNumChildren;collectgarbage('collect');assert(count()==0)
                    local enabled=GAMESTATE:IsPlayerEnabled(pn)
                    assert((TOP:GetChild('PlayerP'..i)~=nil)==enabled)
                    assert((TOP:GetLifeMeter(pn)~=nil)==enabled)
                    for _,name in ipairs{'ScoreP','StepsDisplayP','StepsDescriptionP','PlayerOptionsP','OniGameOverP'} do
                        assert((TOP:GetChild(name..i)~=nil)==enabled)
                    end
                end
                local underlay=TOP:GetChild('Underlay')
                assert(underlay:GetParent()==TOP and underlay:GetChild('Header'):GetParent()==underlay)
                for i,actor in ipairs(underlay) do
                    assert(actor:GetParent()==underlay)
                    log('underlay',i,actor.__songlua_underlay_role,actor:GetName(),actor.__songlua_actor_type,actor.GetNumChildren and actor:GetNumChildren() or -1)
                end
                for i,pn in ipairs{PLAYER_1,PLAYER_2} do
                    local enabled=GAMESTATE:IsPlayerEnabled(pn)
                    assert((underlay:GetChild('P'..i..'Score')~=nil)==enabled)
                    assert(underlay:GetChild('StepStatsPaneP'..i)==nil)
                end
            "#);
        }
    }
}

#[test]
fn underlay_geometry_text_filters_and_variant_selection_match_production() {
    for setup in [
        "",
        "SL.P1.ActiveModifiers.BackgroundFilter=75;SL.P2.ActiveModifiers.BackgroundFilter='Off'",
        "SL.P1.ActiveModifiers.HideLifebar=true;SL.P2.ActiveModifiers.LifeMeterType='Battery'",
        "SL.P1.ActiveModifiers.NoteFieldOffsetX=31;SL.P2.ActiveModifiers.NoteFieldOffsetX=-17;SL.Global.ActiveModifiers.MusicRate=1.25",
    ] {
        let mut p = Pair::new(&context(true, [true, true]), setup);
        p.script(r#"
            local underlay=TOP:GetChild('Underlay')
            local function pose(a)
                log(a:GetName(),a.__songlua_actor_type,a:GetX(),a:GetY(),a:GetWidth(),a:GetHeight(),
                    a:GetZoomX(),a:GetZoomY(),a:GetDiffuseAlpha(),a:GetVisible(),a.Font,a.Texture)
                if a.GetText then log(a:GetText()) end
            end
            pose(underlay:GetChild('Header'))
            for _,actor in ipairs(underlay) do
                log(actor.__songlua_underlay_role);pose(actor)
                for _,child in ipairs(actor) do
                    assert(child:GetParent()==actor);pose(child)
                    for _,grandchild in ipairs(child) do assert(grandchild:GetParent()==child);pose(grandchild) end
                end
            end
            assert(#TOP:GetChild('')==5)
            log(TOP:GetChild('In').__songlua_state_draw_order,TOP:GetChild('Out').__songlua_state_draw_order,TOP:GetChild('Cancel').__songlua_state_draw_order)
        "#);
    }
}

#[test]
fn first_life_messages_initialize_real_meter_tweens_without_authored_broadcast_records() {
    let mut p = Pair::new(&context(true, [true, true]), "");
    p.script(r#"
        local underlay=TOP:GetChild('Underlay')
        for i,pn in ipairs{PLAYER_1,PLAYER_2} do
            local meter=role(underlay,'life'..(i-1))
            assert(meter and meter:GetVisible())
            local fill=meter:GetChild('MeterFill');local swoosh=meter:GetChild('MeterSwoosh')
            assert(fill==meter[3] and swoosh==meter[4])
            log(fill:GetTweenTimeLeft(),swoosh:GetTweenTimeLeft(),fill:GetX(),swoosh:GetX(),swoosh:GetNumStates())
            assert(fill:GetTweenTimeLeft()>0 and swoosh:GetTweenTimeLeft()>0)
            TOP:GetChild('PlayerP'..i):SetLife(0.75)
            log(TOP:GetLifeMeter(pn):GetLife(),fill:GetTweenTimeLeft(),swoosh:GetTweenTimeLeft())
        end
        local history=__songlua_broadcasts or {};log(#history)
    "#);
}

#[test]
fn screen_scene_retirement_releases_owned_players_underlay_and_transitions() {
    let mut p = Pair::new(&context(true, [true, true]), "");
    p.script(r#"
        WEAK=setmetatable({TOP,TOP:GetChild('PlayerP1'),TOP:GetChild('Underlay'),role(TOP,'Failed')},{__mode='v'})
        SAVED_CONSTANT=TOP.GetScreenType
        TOP=nil;root=nil;__songlua_top_screen=nil;__songlua_top_screen_player_1=nil;__songlua_top_screen_player_2=nil;GetPlayerAF=nil
        MusicRateDisplay=nil;dispP1=nil;dispP2=nil
    "#);
    assert!(p.rust.release_actor_scene(p.scene).unwrap());
    assert!(!p.rust.release_actor_scene(p.scene).unwrap());
    // The native oracle retains its registered actors until VM teardown; the
    // Rust scene's explicit release must unsubscribe and relinquish ownership.
    p.rust
        .vm_mut()
        .execute(
            &compile_named(
                br#"
        collectgarbage('collect');collectgarbage('collect')
        for i=1,4 do assert(WEAK[i]==nil) end
        assert(SAVED_CONSTANT()=='ScreenType_Gameplay')
    "#,
                b"=retired screen",
            )
            .unwrap(),
        )
        .unwrap();
}

#[test]
fn required_theme_helpers_keep_argument_rules_fresh_colors_and_mutable_style_lookup() {
    let mut p = Pair::new(&context(true, [true, false]), "");
    p.script(r#"
        local function color(v) for i=1,4 do log(v[i]) end end
        local values=BackgroundFilterValues();assert(values~=BackgroundFilterValues())
        log(values.Off,values.Dark,values.Darker,values.Darkest,#values)
        for i=0,100 do assert(values[i]==i) end
        for _,v in ipairs{PLAYER_1,PLAYER_2,'p1','P2','0',0,1,0.5,2,false,{}} do
            for _,flag in ipairs{true,false,0,1,'yes','off','NaN',{}} do color(PlayerColor(v,flag)) end
        end
        for _,flag in ipairs{true,false,0,1,'yes','off','NaN',{}} do color(GetCurrentColor(flag)) end
        color(GetCurrentColor());assert(GetCurrentColor()~=GetCurrentColor())
        log(left,center,middle,right,top,bottom,HorizAlign_Left,VertAlign_Bottom)
        assert(type(MonthOfYear())=='number')
        PREFSMAN:SetPreference('EasterEggs',true)
        for _,month in ipairs{0,9,11} do
            MonthOfYear=function()collectgarbage('collect');return month end
            for _,style in ipairs{'Arrows','Spooky'} do
                ThemePrefs.Set('VisualStyle',style);assert(IsSpooky()==(month==9 and style=='Spooky'));log(IsSpooky())
            end
        end
        PREFSMAN:SetPreference('EasterEggs',false);assert(not IsSpooky())
        color(PlayerColor());assert(PlayerColor(0)~=PlayerColor(0))
        for _,v in ipairs{'Difficulty_Easy','Hard','Edit','easy','3',0,2.9,-1,false,{}} do
            color(DifficultyColor(v));color(DifficultyColor(v,'yes'))
        end
        color(DifficultyColor());assert(DifficultyColor(1)~=DifficultyColor(1))
        for _,v in ipairs{1.25,-1.25,-0.5,123.456,'2.5','bad',false,{}} do
            for _,digits in ipairs{0,1,2,-1,'1','bad',false} do log(round(v,digits)) end
        end
        log(round(),round(nil,2));log(IsRoutine())
        GAMESTATE.GetCurrentStyle=function()return nil end;assert(not IsRoutine())
        GAMESTATE.GetCurrentStyle=function()return {GetStyleType=function()collectgarbage('collect');return 'StyleType_TwoPlayersSharedSides' end} end
        assert(IsRoutine());log(IsRoutine())
    "#);
}

#[test]
fn underlay_update_closures_keep_bpm_rate_and_life_owners_across_collection() {
    for enabled in [[true, false], [true, true]] {
        let mut p = Pair::new(&context(true, enabled), "");
        p.script(
            r#"
            local underlay=TOP:GetChild('Underlay')
            UNDERLAY=underlay;BPM=role(underlay,'bpm')
            GAMESTATE:GetSongOptionsObject('ModsLevel_Song'):MusicRate(1.25)
            collectgarbage('collect')
        "#,
        );
        let mut seconds = 0.;
        for delta in [0.05, 0.1] {
            seconds += delta;

            p.rust.advance_actor_scene(p.scene, delta).unwrap();
            p.script(r#"
                log(BPM:GetChild('RatemodDisplay'):GetText())
                local display=BPM:GetChild('')
                for _,child in ipairs(display) do log(child:GetName(),child:GetText()) end
                for _,actor in ipairs(UNDERLAY) do
                    if actor.__songlua_underlay_role=='life0' or actor.__songlua_underlay_role=='life1' then
                        log(actor:GetChild('MeterFill'):GetZoomX(),actor:GetChild('MeterSwoosh'):GetZoomX())
                    end
                end
                collectgarbage('collect')
            "#);
        }
        assert!(p.rust.take_actor_diagnostics().is_empty());
    }
}

#[test]
fn named_underlay_children_restore_names_and_copy_enabled_tags_without_metamethod_writes() {
    let mut p = Pair::with_retained_classes(
        &context(true, [true, false]),
        r#"
        local fonts=GetComboFonts;local installed=false
        GetComboFonts=function()
            if not installed then
                installed=true;local xy=Actor.xy
                Actor.xy=function(self,...)
                    collectgarbage('collect')
                    if self:GetName()=='Header' then
                        local parent=self:GetParent()
                        assert(parent:GetName()=='Underlay' and parent:GetParent()==nil)
                        self:name('temporary');HEADER_DURING=self:GetName()
                        local old=getmetatable(self);local mt={};for k,v in pairs(old) do mt[k]=v end
                        mt.__newindex=function(t,k,v)
                            assert(k~='__songlua_player1_enabled' and k~='__songlua_player2_enabled','enabled flags must use raw writes')
                            if type(old.__newindex)=='function' then return old.__newindex(t,k,v) end
                            rawset(t,k,v)
                        end
                        setmetatable(self,mt)
                    end
                    return xy(self,...)
                end
            end
            return fonts()
        end
    "#,
        true,
    );
    p.script(
        r#"
        local underlay=TOP:GetChild('Underlay');local header=underlay:GetChild('Header')
        log(HEADER_DURING,header:GetName(),header.Name,header.__songlua_native_name)
        assert(header:GetName()=='Header' and header:GetParent()==underlay)
        assert(header.__songlua_player1_enabled and header.__songlua_player2_enabled==false)
    "#,
    );
}

#[test]
fn failed_screen_construction_releases_partial_actors_and_allows_recovery() {
    let context = context(true, [true, false]);
    let setup = r#"
        local fonts=GetComboFonts;local installed=false
        WEAK=setmetatable({},{__mode='v'});CTOR_COUNT=0
        GetComboFonts=function()
            if not installed then
                installed=true;local xy=Actor.xy
                Actor.xy=function(self,...)
                    CTOR_COUNT=CTOR_COUNT+1;WEAK[CTOR_COUNT]=self;collectgarbage('collect')
                    return xy(self,...)
                end
            end
            return fonts()
        end
        SL.P1.ActiveModifiers.BackgroundFilter='Dark'
        WIDTH_ORIGINAL=GetNotefieldWidth
        GetNotefieldWidth=function()error('width failed')end
    "#;
    let mut rust = RustSongRuntime::new(&context).unwrap();
    rust.vm_mut()
        .execute(&compile_named(setup.as_bytes(), b"=failure setup").unwrap())
        .unwrap();
    let error = test_support::gameplay_screen(&mut rust, &context).unwrap_err();
    assert!(error.to_string().contains("width failed"), "{error}");
    let Value::Number(count) = rust.vm_mut().get_global(b"CTOR_COUNT").unwrap() else {
        panic!("constructor count")
    };
    reference_test::assert_debug(
        "song/tests/rust_screen_assembly.rs::failed_constructor_count",
        &count,
    );
    rust.vm_mut()
        .execute(
            &compile_named(
                br#"
        collectgarbage('collect');collectgarbage('collect')
        for i=1,CTOR_COUNT do assert(WEAK[i]==nil) end
        assert(SCREENMAN==nil and __songlua_top_screen==nil)
        GetNotefieldWidth=WIDTH_ORIGINAL
    "#,
                b"=failure cleanup",
            )
            .unwrap(),
        )
        .unwrap();
    let scene = test_support::gameplay_screen(&mut rust, &context).unwrap();
    assert!(rust.actor_scene_root(scene).is_some());
    assert!(rust.release_actor_scene(scene).unwrap());
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
