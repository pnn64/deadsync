use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext,
    rust_engine::{ActorScene, RustSongRuntime, test_support},
};
use std::fs;

struct Pair {
    rust: RustSongRuntime,
    scene: ActorScene,
    _directory: tempfile::TempDir,
}

impl Pair {
    fn new(live: bool, enabled: [bool; 2], setup: &str) -> Self {
        Self::with_fonts(live, enabled, setup, true)
    }

    fn with_fonts(live: bool, enabled: [bool; 2], setup: &str, fonts: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        if fonts {
            let path = directory.path().join("theme/Fonts/_Combo Fonts/Fixture");
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("Fixture.ini"), b"[main]").unwrap();
            fs::write(
                path.join("Fixture 10x2.png"),
                include_bytes!("../assets/simply-love/Graphics/Combo 100milestone/explosion.png"),
            )
            .unwrap();
        }
        let mut context = SongLuaCompileContext::new(directory.path(), "loaded gameplay theme");
        context.live = live;
        context.screen_width = 854.;
        context.screen_height = 480.;
        for (player, enabled) in context.players.iter_mut().zip(enabled) {
            player.enabled = enabled;
        }
        // Exercise the actual GetComboFonts/FILEMAN scan, with a temporary
        // current-theme directory instead of replacing the font helper.
        let theme = "THEME.GetCurrentThemeDirectory=function()return 'theme/' end";

        let mut rust = RustSongRuntime::new(&context).unwrap();
        rust.vm_mut()
            .execute(&compile_named(theme.as_bytes(), b"=fixture theme directory").unwrap())
            .unwrap();
        let scene = test_support::gameplay_screen(&mut rust, &context).unwrap();
        assert_eq!(rust.begin_actor_preparation().unwrap(), !live);
        let mut pair = Self {
            rust,
            scene,
            _directory: directory,
        };
        pair.script(
            r#"
            TOP=SCREENMAN:GetTopScreen();root=TOP;observations={}
            function log(...)
                local t={};for i=1,select('#',...) do local v=select(i,...)
                    t[i]=type(v)=='number' and string.format('%.9g',v) or tostring(v)
                end;observations[#observations+1]=table.concat(t,',')
            end
            function role(name)
                for _,actor in ipairs(TOP) do
                    if actor.__songlua_screen_actor_role==name then return actor end
                end
            end
            function combo(p) return _G['__songlua_top_screen_player_'..p]:GetChild('Combo') end
            function inspect(a)
                log(a.__songlua_actor_type,a:GetName(),a:GetVisible(),a:GetX(),a:GetY(),
                    a:GetWidth(),a:GetHeight(),a:GetZoomX(),a:GetZoomY(),a:GetDiffuseAlpha(),
                    a:GetTweenTimeLeft(),type(a.InitCommand))
                if a.GetTexture and a:GetTexture() then
                    local t=a:GetTexture();log(t:GetPath(),t:GetImageWidth(),t:GetImageHeight())
                end
                for _,child in ipairs(a) do assert(child:GetParent()==a);inspect(child) end
            end
        "#,
        );
        pair.script(setup);
        pair
    }

    fn script(&mut self, source: &str) {
        self.rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"@/fixture/gameplay-theme.lua").unwrap())
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
        reference_test::assert_debug("song/tests/rust_gameplay_theme.rs::observations", &actual);
    }

    fn combo(&mut self) {
        assert!(test_support::gameplay_combo(&mut self.rust, self.scene).unwrap());
    }

    fn transitions(&mut self) {
        assert!(test_support::gameplay_transitions(&mut self.rust, self.scene).unwrap());
    }
}

#[test]
fn bundled_combo_instances_keep_native_names_parents_textures_and_disabled_player() {
    for live in [false, true] {
        for enabled in [[true, false], [false, true], [true, true]] {
            let mut p = Pair::new(live, enabled, "");
            p.combo();
            p.script(
                r#"
                for i=1,2 do
                    local c=combo(i);assert(#c==3)
                    assert(c[1]==c:GetChild('OneHundredMilestone') and #c[1]==4)
                    assert(c[2]==c:GetChild('OneThousandMilestone') and #c[2]==3)
                    assert(c[3]==c:GetChild('Number') and #c[2][1]==4)
                    inspect(c)
                end
            "#,
            );
        }
    }
}

#[test]
fn bundled_combo_visual_styles_and_truthy_hide_setting() {
    for live in [false, true] {
        for style in [
            "Arrows",
            "Bears",
            "Cats",
            "Ducks",
            "Gay",
            "Hearts",
            "Spooky",
            "SRPG10",
            "Stars",
            "Technique",
            "Thonk",
        ] {
            let mut p = Pair::new(
                live,
                [true, false],
                &format!("ThemePrefs.Set('VisualStyle','{style}')"),
            );
            p.combo();
            p.script("inspect(combo(1));inspect(combo(2))");
        }
        for value in ["true", "0", "''", "{}"] {
            let mut p = Pair::new(
                live,
                [true, false],
                &format!("SL.P1.ActiveModifiers.HideComboExplosions={value}"),
            );
            p.combo();
            p.script(
                r#"
                assert(#combo(1)==1 and combo(1)[1]==combo(1):GetChild('Number'))
                assert(#combo(2)==3);inspect(combo(1));inspect(combo(2))
            "#,
            );
        }
    }
}

#[test]
fn combo_init_order_environment_and_collection_match_the_production_loader() {
    for live in [false, true] {
        let mut p = Pair::new(
            live,
            [true, true],
            r#"
            local frame=Def.ActorFrame
            Def.ActorFrame=function(t)
                local init=t.InitCommand
                t.InitCommand=function(self)
                    log('init',self.Name,self:GetParent() and self:GetParent().Name,
                        self.__songlua_owned_child_lookup,#self)
                    local env=getfenv(init)
                    log(env~=getfenv(),rawget(env,'LoadActor')~=nil,rawget(env,'Def')==nil)
                    env.theme_private=(env.theme_private or 0)+1
                    log(env.theme_private,_G.theme_private)
                    collectgarbage('collect');init(self)
                end
                return frame(t)
            end
        "#,
        );
        p.combo();
        p.script(
            "assert(theme_private==nil and #observations==18);inspect(combo(1));inspect(combo(2))",
        );
    }
}

#[test]
fn retained_combo_commands_survive_gc_and_keep_their_capture_or_live_tweens() {
    for live in [false, true] {
        let mut p = Pair::new(live, [true, true], "");
        p.combo();
        p.script(
            r#"
            collectgarbage('collect');collectgarbage('collect')
            combo(1)[1]:playcommand('HundredMilestone')
            combo(2)[2]:playcommand('ThousandMilestone')
            inspect(combo(1));inspect(combo(2))
            combo(1)[1]:finishtweening():playcommand('Hide')
            combo(2)[2]:finishtweening():playcommand('Hide')
            inspect(combo(1));inspect(combo(2))
        "#,
        );
    }
}

#[test]
fn loaded_transition_children_keep_original_sources_geometry_and_owner_state() {
    for live in [false, true] {
        let mut p = Pair::new(live, [true, true], "");
        p.transitions();
        p.script(
            r#"
            for _,name in ipairs{'Toasty','NextSong','Ready','Go','Failed'} do
                local a=role(name)
                assert(#a==1 and a.__songlua_transition_child==a[1])
                assert(a:GetChild('')==a[1] and a[1]:GetParent()==a)
                log(name,a:GetVisible(),a.__songlua_transition_state)
                inspect(a)
            end
            assert(not role('Toasty')[1]:GetVisible())
            assert(#role('NextSong')[1]==2)
            local init=role('NextSong')[1][1].InitCommand
            log(getfenv(init)==getfenv())
        "#,
        );
    }
}

#[test]
fn loaded_transition_commands_retain_rate_dependent_timing_across_gc() {
    for live in [false, true] {
        let mut p = Pair::new(live, [true, true], "");
        p.transitions();
        p.script(
            r#"
            local a=role('NextSong')[1]
            collectgarbage('collect')
            a:playcommand('Start');log(a:GetTweenTimeLeft(),a:GetVisible(),a:GetDiffuseAlpha())
            a:finishtweening();SL.Global.ActiveModifiers.MusicRate=2
            a:playcommand('Finish');log(a:GetTweenTimeLeft(),a:GetVisible(),a:GetDiffuseAlpha())
            a:finishtweening():playcommand('Hide');log(a:GetVisible())
        "#,
        );
    }
}

#[test]
fn loaded_theme_scene_owns_callback_roots_until_release() {
    for live in [false, true] {
        let mut p = Pair::new(live, [true, true], "");
        p.combo();
        p.transitions();
        let original = p.rust.actor_scene_snapshot(p.scene).unwrap().len();
        assert!(original > 60);
        p.script(r#"
            assert(combo(1)[1] and combo(2)[2] and role('NextSong')[1] and role('Toasty')[1])
            WEAK=setmetatable({combo(1)[1],combo(2)[2],role('NextSong')[1],role('Toasty')[1]}, {__mode='v'})
            root=nil;TOP=nil;__songlua_top_screen=nil;__songlua_top_screen_player_1=nil;__songlua_top_screen_player_2=nil
            GetPlayerAF=nil;MusicRateDisplay=nil;dispP1=nil;dispP2=nil
        "#);
        assert!(p.rust.release_actor_scene(p.scene).unwrap());
        assert!(!test_support::gameplay_combo(&mut p.rust, p.scene).unwrap());
        p.rust
            .vm_mut()
            .execute(
                &compile_named(
                    br#"
            collectgarbage('collect');collectgarbage('collect')
            for i=1,4 do assert(WEAK[i]==nil) end
        "#,
                    b"=retired loaded theme",
                )
                .unwrap(),
            )
            .unwrap();
    }
}

#[test]
fn failed_milestone_init_is_owned_by_the_screen_and_preserves_the_source_error() {
    for live in [false, true] {
        let mut p = Pair::new(
            live,
            [true, false],
            r#"
            local frame=Def.ActorFrame
            Def.ActorFrame=function(t)
                t.InitCommand=function(self) FAILED=self;error('milestone init sentinel') end
                return frame(t)
            end
        "#,
        );

        let rust = test_support::gameplay_combo(&mut p.rust, p.scene).unwrap_err();

        assert!(
            rust.to_string().contains("milestone init sentinel"),
            "{rust}"
        );
        p.script("WEAK=setmetatable({FAILED},{__mode='v'});FAILED=nil");
        p.rust.release_actor_scene(p.scene).unwrap();
        p.rust
            .vm_mut()
            .execute(
                &compile_named(
                    br#"
            collectgarbage('collect');collectgarbage('collect');assert(WEAK[1]==nil)
        "#,
                    b"=failed load cleanup",
                )
                .unwrap(),
            )
            .unwrap();
    }
}

#[test]
fn combo_without_theme_fonts_remains_an_invisible_leaf() {
    for live in [false, true] {
        let mut p = Pair::with_fonts(live, [true, false], "", false);
        p.combo();
        p.script("for i=1,2 do local c=combo(i);assert(c.__songlua_actor_type=='Actor' and #c==0 and not c:GetVisible());inspect(c) end");
    }
}

#[test]
fn bundled_sources_keep_native_chunk_names_and_ignore_replaced_lua_loaders() {
    for live in [false, true] {
        let mut p = Pair::new(live, [true, false], "");

        p.rust
            .vm_mut()
            .execute(
                &compile_named(
                    b"function inspect_source(f)return debug.getinfo(f,'S').source end",
                    b"=source inspector",
                )
                .unwrap(),
            )
            .unwrap();
        p.script(
            r#"
            loadstring=function()error('replaced Lua loader must not handle bundled sources')end
            setfenv=function()error('replaced Lua setter must not handle bundled environments')end
        "#,
        );
        p.combo();
        p.transitions();
        p.script(r#"
            assert(inspect_source(combo(1)[1].InitCommand)=='@theme:/Graphics/Combo 100milestone/default.lua')
            assert(inspect_source(combo(1)[2].ThousandMilestoneCommand)=='@theme:/Graphics/Combo 1000milestone/default.lua')
            assert(inspect_source(role('Toasty')[1].InitCommand)=='@theme:/BGAnimations/ScreenGameplay toasty.lua')
            assert(inspect_source(role('NextSong')[1].StartCommand)=='@theme:/BGAnimations/ScreenGameplay next course song/default.lua')
            inspect(combo(1));inspect(role('NextSong'))
        "#);
    }
}

#[test]
fn transition_load_writes_native_registry_without_authored_newindex_callbacks() {
    for live in [false, true] {
        let mut p = Pair::new(
            live,
            [true, false],
            r#"
            for _,name in ipairs{'Toasty','NextSong','Ready','Go','Failed'} do
                role(name).__songlua_children=setmetatable({}, {__newindex=function() error('native registry must use raw writes') end})
            end
        "#,
        );
        p.transitions();
        p.script("for _,name in ipairs{'Toasty','NextSong','Ready','Go','Failed'} do local a=role(name);assert(rawget(a.__songlua_children,'')==a[1]);inspect(a) end");
    }
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
