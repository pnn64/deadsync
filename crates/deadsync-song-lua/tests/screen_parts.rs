use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, {ActorScene, SongRuntime, test_support},
};
use std::path::Path;

struct Pair {
    rust: SongRuntime,
    scene: ActorScene,
}
impl Pair {
    fn new(name: &str, live: bool, setup: &str) -> Self {
        let mut context = SongLuaCompileContext::new(Path::new("."), "generated screen parts");
        context.live = live;
        context.screen_width = 854.;
        context.screen_height = 480.;
        context.players[1].enabled = true;

        let mut rust = SongRuntime::new(&context).unwrap();
        rust.vm_mut()
            .execute(&compile_named(setup.as_bytes(), b"=screen part setup").unwrap())
            .unwrap();
        let scene = test_support::screen_part(&mut rust, name).unwrap();
        let actor = rust.actor_scene_root(scene).unwrap();
        rust.vm_mut().set_global(b"part", actor).unwrap();
        let mut pair = Self { rust, scene };
        pair.script(r#"
            root=part;observations={};function log(...)
                local t={};for i=1,select('#',...) do local v=select(i,...)
                    t[i]=type(v)=='number' and string.format('%.9g',v) or tostring(v)
                end;observations[#observations+1]=table.concat(t,',')
            end
            function pose(a)log(a:GetX(),a:GetY(),a:GetWidth(),a:GetHeight(),a:GetZoomX(),a:GetZoomY(),a:GetVisible(),a:GetDiffuseAlpha())end
        "#);
        pair
    }
    fn script(&mut self, source: &str) {
        self.rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"@/fixture/screen_parts.lua").unwrap())
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
            "song/tests/rust_screen_parts.rs::observations",
            &self.rust.vm_mut().string_bytes(actual).unwrap(),
        );
    }
}

#[test]
fn timer_and_engine_labels_keep_types_visibility_and_bound_methods() {
    for live in [false, true] {
        for name in [
            "Timer",
            "Debug",
            "Overlay",
            "Header",
            "Footer",
            "SongForeground",
            "StepsDescriptionP1",
            "StepsDescriptionP2",
            "PlayerOptionsP1",
            "ActiveAttackListP2",
            "SongOptions",
            "OniGameOverP1",
        ] {
            let mut p = Pair::new(name, live, "");
            p.script(r#"
                assert(part:GetParent()==nil);assert(Timer==nil and PercentageDisplay==nil)
                log(part:GetName(),part.__songlua_actor_type,part.Text,__songlua_side_effect_count);pose(part)
                if part.GetSeconds then
                    local get=part.GetSeconds;assert(get()==0 and get({},1,2)==0)
                    Actor.GetSeconds=function()return 12 end;assert(part:GetSeconds()==12);Actor.GetSeconds=nil
                end
                if part.GetNumChildren then assert(part:GetNumChildren()==0 and part:GetChild('missing')==nil)end
                log(type(part.GetTexture))
                local texture=part.GetTexture and part:GetTexture()
                if texture then log(texture:GetPath(),texture:GetImageWidth())else log(nil)end
                part:x(19);assert(part:GetX()==19)
            "#);
        }
    }
}

#[test]
fn percentage_score_owns_named_only_children_and_text_aliases() {
    for name in ["ScoreP1", "ScoreP2"] {
        for live in [false, true] {
            let mut p = Pair::new(name, live, "");
            p.script(r#"
                assert(part[1]==nil and part:GetNumChildren()==2)
                local frame=part:GetChild('');local percent=part:GetChild('ScoreDisplayPercentage Percent')
                assert(frame:GetParent()==part and percent:GetParent()==part)
                assert(percent.__songlua_actor_type=='PercentageDisplay' and percent:GetNumChildren()==1)
                assert(percent[1]==nil and percent.SetUpdateFunction~=nil)
                log(type(percent.GetTexture))
                local pn=part:GetName():sub(-1);local text=percent:GetChild('PercentP'..pn)
                assert(text:GetParent()==percent and text:GetText()=='0.00%')
                text:name('renamed');assert(percent:GetChild('PercentP'..pn)==text)
                assert(percent:GetChild('renamed')==nil)
                local texture=frame:GetTexture()
                log(texture:GetImageWidth(),texture:GetImageHeight(),texture:GetTextureWidth(),texture:GetTextureHeight())
                local copy=part:GetChildren();copy['']=nil;assert(part:GetChild('')==frame)
                log(part:GetName(),percent:GetName(),text:GetName(),text:GetText(),__songlua_side_effect_count)
                pose(frame);pose(percent);pose(text)
                text:settext('12.34%');log(text:GetText(),text:GetWidth(),text:GetHeight())
                assert(part:GetChild('missing')==nil and percent:GetChild('missing')==nil)
            "#);
        }
    }
}

#[test]
fn internal_texture_queries_keep_their_owner_and_public_class_overrides() {
    for name in ["Timer", "ScoreP1", "ScoreP2"] {
        for live in [false, true] {
            let mut p = Pair::new(name, live, "");
            p.script(r#"
                local owner=part.__songlua_actor_type=='Timer' and part or part:GetChild('ScoreDisplayPercentage Percent')
                local get=owner.GetTexture;assert(type(get)=='function' and rawget(owner,'GetTexture')==nil)
                assert(get({Texture='foreign.png'})==nil and get({},1,2)==nil)
                assert(not pcall(get) and not pcall(get,nil) and not pcall(get,42) and not pcall(get,'wrong'))
                Actor.GetTexture=function(self)assert(self==owner);return 'override'end
                assert(owner:GetTexture()=='override' and get({})==nil);Actor.GetTexture=nil
                owner.Texture='screen-owned-missing 2x1.png'
                collectgarbage('collect');local first=get({Texture='foreign.png'})
                local second=owner:GetTexture();assert(first~=second)
                log(first:GetPath(),first:GetNumFrames(),first:GetSourceWidth(),first:GetSourceHeight())
                assert(first:GetPath()==second:GetPath())
                owner.Texture=nil;collectgarbage('collect')
                assert(get({})==nil);log(first:GetPath(),first:GetNumFrames())
                if part~=owner then assert(part.GetTexture==nil)end
            "#);
        }
    }
}

#[test]
fn lyrics_follow_current_reverse_selection_and_keep_real_layer_commands() {
    for setup in [
        "",
        "GAMESTATE:GetPlayerState(0):GetPlayerOptions('ModsLevel_Current'):Reverse(1)",
        "GAMESTATE:GetPlayerState(0):GetPlayerOptions('ModsLevel_Current'):Reverse(1);GAMESTATE:GetPlayerState(1):GetPlayerOptions('ModsLevel_Current'):Reverse(1)",
        "GAMESTATE.GetEnabledPlayers=function() collectgarbage('collect');return {} end",
    ] {
        let mut p = Pair::new("LyricDisplay", true, setup);
        p.script(r#"
            assert(part:GetNumChildren()==2 and part[1]==part:GetChild('LyricBack') and part[2]==part:GetChild('LyricFront'))
            for _,text in ipairs(part) do
                assert(text:GetParent()==part and text:GetText()=='' and text.Font=='Common Normal')
                assert(type(text.ChangedCommand)=='function')
            end
            pose(part);log(__songlua_side_effect_count)
            for _,command in ipairs{'SetNoReverse','SetReverse','SetOneReverse'} do
                part:playcommand(command);pose(part)
            end
            local old=Actor.LyricCommand;calls={}
            Actor.LyricCommand=function(self,side)calls[#calls+1]=side;assert(self:GetParent()==part);collectgarbage('collect')end
            part[1]:playcommand('Changed');part[2]:playcommand('Changed')
            assert(table.concat(calls,',')=='Back,Front');Actor.LyricCommand=old
        "#);
    }
}

#[test]
fn inherited_lyric_helper_uses_current_vars_and_animates_both_layers() {
    let mut p = Pair::new("LyricDisplay", true, "");
    p.script(r#"
        local vars={LyricText='a short lyric',LyricColor={.8,.6,.4,1},LyricDuration=2}
        Var=function(key)return vars[key]end
        part[1]:playcommand('Changed');part[2]:playcommand('Changed')
        for _,text in ipairs(part)do assert(text:GetText()==vars.LyricText);log(text:GetTweenTimeLeft());pose(text)end
    "#);
    for dt in [0.1, 0.2, 1., 1.25] {
        p.rust.advance_actor_scene(p.scene, dt).unwrap();
        p.script("for _,text in ipairs(part)do pose(text);log(text:GetTweenTimeLeft())end");
    }
    assert!(p.rust.take_actor_diagnostics().is_empty());
}

#[test]
fn background_preserves_border_geometry_brightness_groups_and_owned_lookups() {
    for live in [false, true] {
        let mut p = Pair::new(
            "SongBackground",
            live,
            r#"
            calls=0;SCREEN_WIDTH='801.125';SCREEN_HEIGHT='479.75'
            PREFSMAN.GetPreference=function(self,name)
                assert(self==PREFSMAN and name=='BGBrightness');calls=calls+1;collectgarbage('collect');return '0.375'
            end
        "#,
        );
        p.script(r#"
            assert(calls==1 and part:GetNumChildren()==1 and part:GetChild('')==part[1])
            local impl=part[1];assert(impl:GetParent()==part and #impl==5 and impl:GetNumChildren()==5)
            local group=impl:GetChild('');assert(#group==5)
            local brightness=impl[5];assert(brightness:GetParent()==impl and #brightness==3)
            assert(brightness:GetNumChildren()==3)
            local overlays=brightness:GetChild('BrightnessOverlay');assert(#overlays==3)
            for i=1,4 do assert(impl[i]:GetParent()==impl);pose(impl[i])end
            for i,q in ipairs(brightness)do
                assert(q==overlays[i] and q:GetParent()==brightness)
                assert(q:GetDiffuseAlpha()==.625);pose(q)
            end
            assert(part.__songlua_owned_child_lookup and impl.__songlua_owned_child_lookup and brightness.__songlua_owned_child_lookup)
            brightness.__songlua_children={Alias=brightness[1]}
            assert(brightness:GetChild('Alias')==nil and brightness:GetChildren().Alias==nil)
            brightness[1]:name('renamed')
            assert(brightness:GetChild('renamed')==brightness[1])
            assert(#brightness:GetChild('BrightnessOverlay')==2)
            log(__songlua_side_effect_count)
        "#);
    }
}

#[test]
fn generated_named_score_children_advance_and_all_release_with_the_scene() {
    let mut p = Pair::new("ScoreP1", true, "");
    p.script(r#"
        weak=setmetatable({}, {__mode='v'})
        local percent=part:GetChild('ScoreDisplayPercentage Percent');local text=percent:GetChild('PercentP1')
        weak[1]=part;weak[2]=percent;weak[3]=text;weak[4]=part:GetChild('')
        text:linear(1):x(100)
        percent:SetUpdateFunction(function(self,dt)log('percent',dt);collectgarbage('collect')end)
        part:SetUpdateFunction(function(self,dt)log('score',dt)end)
    "#);

    p.rust.advance_actor_scene(p.scene, 0.25).unwrap();
    p.script(
        "log(weak[3]:GetX());root=nil;part=nil;collectgarbage('collect');assert(next(weak)~=nil)",
    );
    p.rust.release_actor_scene(p.scene).unwrap();
    p.rust
        .vm_mut()
        .execute(
            &compile_named(
                b"collectgarbage('collect');collectgarbage('collect');assert(next(weak)==nil)",
                b"=release screen part",
            )
            .unwrap(),
        )
        .unwrap();
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
