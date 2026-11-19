//! Public child, layout, message and lifetime behavior of the native shared pane.
use deadlib_lua::compile_named;
use deadsync_song_lua::{ActorScene, SongLuaCompileContext, SongRuntime, test_support};
use std::path::Path;

fn script(runtime: &mut SongRuntime, source: &str) {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/versus-statistics.lua").unwrap())
        .unwrap_or_else(|error| panic!("{source}\n{error:?}"));
}

fn load(
    live: bool,
    style: &str,
    width: f32,
    height: f32,
    statistics: [bool; 2],
) -> (SongRuntime, ActorScene) {
    let mut context = SongLuaCompileContext::new(Path::new("."), "shared versus statistics");
    context.live = live;
    context.style_name = style.into();
    context.screen_width = width;
    context.screen_height = height;
    context.display_aspect_ratio = width / height;
    for player in &mut context.players {
        player.enabled = true;
        player.autoplay = 0.;
    }
    let mut runtime = SongRuntime::new(&context).unwrap();
    script(
        &mut runtime,
        r#"
        SL.Global.GameMode='ITG'
        ThemePrefs.Set('EnableTournamentMode',false)
        ThemePrefs.Set('StepStats','Hide')
        for _,pn in ipairs{'P1','P2'} do
            local m=SL[pn].ActiveModifiers
            m.BackgroundFilter='Darker';m.NPSGraphAtTop=false
            m.ShowFaPlusWindow=false;m.HideDanger=false;m.HideLifebar=false
            m.LifeMeterType='Standard'
        end
    "#,
    );
    for (index, enabled) in statistics.into_iter().enumerate() {
        script(
            &mut runtime,
            &format!(
                "SL.P{}.ActiveModifiers.DataVisualizations='{}'",
                index + 1,
                if enabled { "Step Statistics" } else { "None" },
            ),
        );
    }
    let scene = test_support::gameplay_screen(&mut runtime, &context).unwrap();
    assert_eq!(runtime.begin_actor_preparation().unwrap(), !live);
    let loaded = test_support::gameplay_underlay(&mut runtime, scene);
    let diagnostics = runtime.take_actor_diagnostics();
    assert!(loaded.is_ok(), "{loaded:?}\n{diagnostics:#?}");
    assert!(loaded.unwrap());
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    script(
        &mut runtime,
        r#"
        function near(actual,expected)
            assert(math.abs(actual-expected)<0.00001,tostring(actual)..' ~= '..tostring(expected))
        end
        function shared_statistics()
            local u=SCREENMAN:GetTopScreen():GetChild('Underlay')
            local found
            for _,a in ipairs(u:GetChildren()[''] or {}) do
                local ok,children=pcall(function() return a:GetChildren() end)
                if ok and children.TapNoteJudgments then
                    assert(not found,'duplicate shared statistics frame')
                    found=a
                end
            end
            return found
        end
    "#,
    );
    (runtime, scene)
}

#[test]
fn shared_versus_statistics_keep_native_children_messages_and_layout() {
    for live in [false, true] {
        let (mut runtime, _) = load(live, "versus", 854., 480., [true, true]);
        script(
            &mut runtime,
            r#"
            local u=SCREENMAN:GetTopScreen():GetChild('Underlay')
            assert(u:GetNumChildren()==16)
            local children=u:GetChildren()
            assert(#children['']==8)
            local shared=assert(shared_statistics())
            assert(shared==children[''][3] and shared:GetName()=='' and shared:GetParent()==u)
            assert(not children.StepStatsPaneP1 and not children.StepStatsPaneP2)
            near(shared:GetX(),427);near(shared:GetY(),240)
            assert(shared:GetNumChildren()==4)
            local grouped=shared:GetChildren()
            assert(#grouped['']==2 and #grouped.TapNoteJudgments==2)
            local quad,banner=grouped[''][1],grouped[''][2]
            assert(quad:GetParent()==shared and banner:GetParent()==shared)
            near(quad:GetZoomedWidth(),150);near(quad:GetZoomedHeight(),480)
            near(banner:GetWidth(),418);near(banner:GetHeight(),164)
            near(banner:GetZoomX(),0.3);near(banner:GetY(),70)
            for i,pane in ipairs(grouped.TapNoteJudgments) do
                assert(pane:GetParent()==shared and pane:GetNumChildren()==6)
                assert(pane:GetName()=='TapNoteJudgments')
                near(pane:GetX(),i==1 and -64 or 66);near(pane:GetY(),100)
                near(pane:GetZoomX(),0.8);near(pane:GetZoomY(),0.8)
                for _,value in ipairs(pane:GetChildren()['']) do
                    assert(value:GetParent()==pane and value:GetText()=='0000')
                end
            end
            collectgarbage('collect');collectgarbage('collect')
            assert(shared_statistics()==shared)
            MESSAGEMAN:Broadcast('Judgment',{Player=PLAYER_1,TapNoteScore='TapNoteScore_W1'})
            MESSAGEMAN:Broadcast('Judgment',{Player=PLAYER_2,TapNoteScore='TapNoteScore_Miss'})
            for player,pane in ipairs(grouped.TapNoteJudgments) do
                for row,value in ipairs(pane:GetChildren()['']) do
                    local incremented=(player==1 and row==1) or (player==2 and row==6)
                    assert(value:GetText()==(incremented and '0001' or '0000'))
                end
            end
        "#,
        );
        assert!(runtime.take_actor_diagnostics().is_empty());
    }
}

#[test]
fn shared_versus_statistics_follow_native_style_screen_and_player_options() {
    for live in [false, true] {
        for (style, width, height, statistics) in [
            ("single", 854., 480., [true, true]),
            ("versus", 640., 480., [true, true]),
            ("versus", 2560., 1080., [true, true]),
            ("versus", 854., 480., [false, false]),
        ] {
            let (mut runtime, _) = load(live, style, width, height, statistics);
            script(&mut runtime, "assert(shared_statistics()==nil)");
        }
        let (mut runtime, _) = load(live, "versus", 854., 480., [true, false]);
        script(
            &mut runtime,
            r#"
            local shared=assert(shared_statistics())
            assert(shared:GetNumChildren()==3)
            local pane=shared:GetChild('TapNoteJudgments')
            assert(pane:GetNumChildren()==6 and pane:GetParent()==shared)
            near(pane:GetX(),-64)
        "#,
        );
    }
}

#[test]
fn shared_versus_statistics_release_with_their_screen() {
    for live in [false, true] {
        let (mut runtime, scene) = load(live, "versus", 854., 480., [true, true]);
        script(
            &mut runtime,
            r#"
            local shared=assert(shared_statistics())
            local children=shared:GetChildren()
            WEAK=setmetatable({shared,children[''][2],children.TapNoteJudgments[1],
                children.TapNoteJudgments[1]:GetChildren()[''][1]},{__mode='v'})
            __songlua_top_screen=nil;__songlua_top_screen_player_1=nil;__songlua_top_screen_player_2=nil
            GetPlayerAF=nil;MusicRateDisplay=nil;dispP1=nil;dispP2=nil
        "#,
        );
        assert!(runtime.release_actor_scene(scene).unwrap());
        script(
            &mut runtime,
            "collectgarbage('collect');collectgarbage('collect');for i=1,4 do assert(WEAK[i]==nil,'retained shared statistics actor '..i)end",
        );
    }
}
