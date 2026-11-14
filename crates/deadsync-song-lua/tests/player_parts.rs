use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, {ActorScene, SongRuntime, test_support},
};
use std::path::Path;

struct Pair {
    rust: SongRuntime,
    scenes: [ActorScene; 2],
}
impl Pair {
    fn new(style: &str, live: bool, setup: &str) -> Self {
        let mut context = SongLuaCompileContext::new(Path::new("."), "player parts migration");
        context.live = live;
        context.style_name = style.into();
        context.players[1].enabled = true;
        context.players[0].judgment_texture = Some(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("assets/simply-love/Graphics/Combo 100milestone/explosion.png"),
        );

        let mut rust = SongRuntime::new(&context).unwrap();
        rust.vm_mut()
            .execute(&compile_named(setup.as_bytes(), b"=player setup").unwrap())
            .unwrap();
        let scenes = [0, 1].map(|i| {
            let scene = test_support::gameplay_player(&mut rust, &context, i).unwrap();
            let actor = rust.actor_scene_root(scene).unwrap();
            rust.vm_mut()
                .set_global(if i == 0 { b"p" } else { b"q" }, actor)
                .unwrap();
            scene
        });
        let mut pair = Self { rust, scenes };
        pair.script(
            r#"
            observations={};function log(...)
                local t={};for i=1,select('#',...) do local v=select(i,...)
                    t[i]=type(v)=='number' and string.format('%.9g',v) or tostring(v)
                end;observations[#observations+1]=table.concat(t,',')
            end
            j=p:GetChild('Judgment');s=j:GetChild('JudgmentWithOffsets')
            c=p:GetChild('Combo');nf=p:GetChild('NoteField');holds=p.__songlua_hold_judgments
        "#,
        );
        pair
    }
    fn script(&mut self, source: &str) {
        self.rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"@/fixture/player_parts.lua").unwrap())
            .unwrap_or_else(|e| panic!("Rust: {source}\n{e:?}"));
        self.compare(source);
    }
    fn compare(&mut self, _source: &str) {
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
            "song/tests/rust_player_parts.rs::observations",
            &self.rust.vm_mut().string_bytes(actual).unwrap(),
        );
    }
}

#[test]
fn player_init_owns_ordered_children_and_shared_named_aliases() {
    for style in ["single", "double", "solo", "pump-single", "pump-double"] {
        for live in [false, true] {
            let mut p = Pair::new(
                style,
                live,
                "GetComboFonts=function() return {'fixture'} end",
            );
            p.script(r#"
                assert(HoldJudgment==nil)
                for pi,player in ipairs{p,q} do
                    local combo=player:GetChild('Combo');local judgment=player:GetChild('Judgment')
                    local holds=player.__songlua_hold_judgments;local nf=player:GetChild('NoteField')
                    local count=#nf:GetColumnActors();assert(#holds==count and #player==count+4)
                    assert(player[1]==player.__songlua_attack_display)
                    assert(player[2]==combo and player[3]==judgment and player[#player]==nf)
                    assert(player:GetNumChildren()==count+4)
                    local unnamed=player:GetChild('');assert(#unnamed==count+1)
                    assert(unnamed[1]==player[1] and unnamed:GetParent()==player)
                    for i,hold in ipairs(holds) do
                        assert(hold==player[i+3] and hold==unnamed[i+1])
                        assert(hold.__songlua_actor_type=='HoldJudgment' and hold:GetName()=='')
                        assert(hold:GetParent()==player and hold:GetNumChildren()==1)
                        local sprite=hold:GetChild('HoldJudgment')
                        assert(sprite==hold[1] and sprite:GetParent()==hold)
                        assert(sprite.__songlua_sprite_animate==false)
                        assert(sprite:GetDiffuseAlpha()==0)
                        log(sprite:GetVisible(),sprite:GetX(),sprite:GetY(),sprite.Texture)
                    end
                    assert(unnamed:GetChild('HoldJudgment')==holds[count][1])
                    assert(combo:GetChild('Number')==combo[1] and combo[1]:GetParent()==combo)
                    -- Native's combo is an ActorFrame registered as Combo.
                    assert(tostring(combo):find('^Combo %(') and combo.__songlua_actor_type=='ActorFrame')
                    assert(combo.GetNumChildren and tostring(combo[1]):find('^BitmapText %('))
                    assert(judgment:GetChild('JudgmentWithOffsets')==judgment[1])
                    assert(judgment[1].__songlua_judgment_sprite_player==pi-1)
                    assert(not judgment[1]:GetVisible() and not judgment[1].__songlua_sprite_animate)
                    for _,part in ipairs{combo,judgment,nf} do
                        assert(part:GetParent()==player and part.__songlua_player_index==pi-1)
                        assert(part.__songlua_style_name==player.__songlua_style_name)
                        assert(part.__songlua_player_child_name==part:GetName())
                        assert(player:GetChildren()[part:GetName()]==part)
                    end
                    log(nf:GetNumChildren(),combo:GetNumChildren(),judgment:GetNumChildren())
                end
                log(__songlua_side_effect_count,s:GetWidth(),s:GetHeight(),s:GetNumStates())
            "#);
        }
    }
}

#[test]
fn hidden_or_empty_font_combo_is_the_existing_invisible_leaf() {
    for setup in [
        "GetComboFonts=function()return {} end",
        "GetComboFonts=function()return {'fixture'} end;SL.P1.ActiveModifiers.HideCombo=true",
        "GetComboFonts=function()return {'fixture'} end;SL.P1.ActiveModifiers.HideCombo=0",
        "GetComboFonts=function()return setmetatable({}, {__index=function() error('raw fonts only') end}) end",
    ] {
        let mut p = Pair::new("single", true, setup);
        p.script(
            r#"
            assert(c.__songlua_actor_type=='Actor' and not c:GetVisible())
            assert(tostring(c):find('^Actor %('))
            assert(c.GetChild==nil and c.SetUpdateFunction==nil and c[1]==nil)
            assert(c:GetParent()==p and p[2]==c)
            log(q:GetChild('Combo').__songlua_actor_type,q:GetChild('Combo'):GetVisible())
            assert(j:GetNumChildren()==1 and nf:GetNumChildren()==1 and #holds==4)
        "#,
        );
    }
}

#[test]
fn hold_texture_method_reenters_gc_once_per_player_and_uses_raw_modifiers() {
    let mut p = Pair::new(
        "double",
        true,
        r#"
        calls={};GetComboFonts=function()collectgarbage('collect');return {'fixture'}end
        SL.P1.ActiveModifiers.HoldJudgment='  custom 2x3.png  '
        SL.P2.ActiveModifiers.HoldJudgment=123
        local old=THEME.GetPathG
        THEME.GetPathG=function(self,group,name)
            collectgarbage('collect');assert(self==THEME and group=='')
            calls[#calls+1]=name
            return old(self,group,name)
        end
    "#,
    );
    p.script(r#"
        assert(#calls==2 and calls[1]=='_HoldJudgments/  custom 2x3.png  ' and calls[2]=='_HoldJudgments/123')
        for _,player in ipairs{p,q} do
            local texture
            for _,h in ipairs(player.__songlua_hold_judgments) do
                assert(not texture or texture==h[1].Texture);texture=h[1].Texture
                log(h[1]:GetNumStates(),h[1]:GetState(),h[1]:GetDiffuseAlpha())
            end
            log(texture)
        end
        log(s.Texture,s:GetTexture():GetPath(),s:GetWidth(),s:GetHeight())
    "#);
}

#[test]
fn generated_child_lookup_keeps_aliases_and_group_forwarding_after_renames() {
    let mut p = Pair::new(
        "single",
        true,
        "GetComboFonts=function()return {'fixture'}end",
    );
    p.script(
        r#"
        local last=holds[#holds];local group=p:GetChild('')
        local saved=group.x;assert(saved(group,7)==last and last:GetX()==7)
        last.Name='instance-only';assert(last:GetName()=='')
        s:name('renamed');assert(j:GetChild('JudgmentWithOffsets')==s and j:GetChild('renamed')==s)
        local snapshot=j:GetChildren();assert(snapshot.renamed==snapshot.JudgmentWithOffsets)
        snapshot.renamed=nil;assert(j:GetChild('renamed')==s)
        local number=c:GetChild('Number');number:name('changed')
        assert(c:GetChild('Number')==number and c:GetChild('changed')==number)
        last[1]:name('also-changed')
        assert(last:GetChild('HoldJudgment')==last[1] and last:GetChild('also-changed')==last[1])
        p:GetChild(''):y(17);assert(last:GetY()==17 and holds[1]:GetY()==0)
        log(last:GetX(),last:GetY(),p:GetNumChildren(),j:GetNumChildren(),c:GetNumChildren())
    "#,
    );
}

#[test]
fn player_update_traversal_visits_each_part_once_and_columns_last() {
    let mut p = Pair::new(
        "single",
        true,
        "GetComboFonts=function()return {'fixture'}end",
    );
    p.script(
        r#"
        root=p
        p[1]:SetUpdateFunction(function(self,dt)log('attack',dt)end)
        c:SetUpdateFunction(function(self,dt)log('combo',dt)end)
        j:SetUpdateFunction(function(self,dt)log('judgment',dt)end)
        for i,h in ipairs(holds) do
            h:SetUpdateFunction(function(self,dt)log('hold'..i,dt);collectgarbage('collect')end)
            h[1]:linear(1):x(100)
        end
        nf:SetUpdateFunction(function(self,dt)log('field',dt)end)
        p:SetUpdateFunction(function(self,dt)log('player',dt)end)
        local column=nf:GetColumnActors()[1];column:linear(1):y(80)
        p:SetUpdateRate(2);holds[1]:SetUpdateRate(.5)
    "#,
    );
    for dt in [0.125, 0.25] {
        p.rust.advance_actor_scene(p.scenes[0], dt).unwrap();
        p.compare("update traversal");
        p.script("log(holds[1][1]:GetX(),holds[2][1]:GetX(),nf:GetColumnActors()[1]:GetY())");
    }
    assert!(p.rust.take_actor_diagnostics().is_empty());
}

#[test]
fn failed_hold_construction_releases_partial_scene_and_allows_later_construction() {
    let context = SongLuaCompileContext::new(Path::new("."), "failed player construction");
    let mut rust = SongRuntime::new(&context).unwrap();
    rust.vm_mut()
        .execute(
            &compile_named(
                br#"
        GetComboFonts=function()return {'fixture'}end
        weak=setmetatable({}, {__mode='v'});count=0
        old_alpha=Actor.diffusealpha
        Actor.diffusealpha=function(self,value)
            count=count+1;weak[count]=self;collectgarbage('collect')
            if count==3 then error('hold construction failure') end
            return old_alpha(self,value)
        end
    "#,
                b"=fail player constructor",
            )
            .unwrap(),
        )
        .unwrap();
    let error = test_support::gameplay_player(&mut rust, &context, 0).unwrap_err();
    assert!(
        error.to_string().contains("hold construction failure"),
        "{error}"
    );
    rust.vm_mut()
        .execute(
            &compile_named(
                br#"
        Actor.diffusealpha=old_alpha;old_alpha=nil
        assert(count==3);collectgarbage('collect');collectgarbage('collect');assert(next(weak)==nil)
    "#,
                b"=check partial scene release",
            )
            .unwrap(),
        )
        .unwrap();
    let scene = test_support::gameplay_player(&mut rust, &context, 0).unwrap();
    let actor = rust.actor_scene_root(scene).unwrap();
    rust.vm_mut().set_global(b"p", actor).unwrap();
    rust.vm_mut()
        .execute(
            &compile_named(
                br#"
        assert(#p.__songlua_hold_judgments==4 and p:GetChild('NoteField')~=nil)
        p:GetChild('Judgment'):SetUpdateFunction(function()collectgarbage('collect')end)
    "#,
                b"=recovered player constructor",
            )
            .unwrap(),
        )
        .unwrap();
    rust.advance_actor_scene(scene, 0.125).unwrap();
    assert!(rust.take_actor_diagnostics().is_empty());
    rust.release_actor_scene(scene).unwrap();
}

#[test]
fn scene_release_collects_the_entire_generated_tree_and_keeps_other_player_alive() {
    let mut p = Pair::new(
        "double",
        true,
        "GetComboFonts=function()return {'fixture'}end",
    );
    p.script(
        r#"
        weak=setmetatable({}, {__mode='v'})
        local function remember(a)
            weak[#weak+1]=a
            for _,child in ipairs(a) do remember(child) end
        end
        remember(p)
        for _,col in ipairs(nf:GetColumnActors())do weak[#weak+1]=col end
        weak[#weak+1]=p.__songlua_life_meter
        saved_spline=nf:GetColumnActors()[1]:GetPosHandler():GetSpline()
        saved_spline:SetSize(1);saved_spline:SetPoint(1,{3,4,5})
        p=nil;j=nil;s=nil;c=nil;nf=nil;holds=nil;collectgarbage('collect')
        assert(next(weak)~=nil)
    "#,
    );
    assert!(p.rust.release_actor_scene(p.scenes[0]).unwrap());
    assert!(!p.rust.release_actor_scene(p.scenes[0]).unwrap());
    p.rust
        .vm_mut()
        .execute(
            &compile_named(
                br#"
        collectgarbage('collect');collectgarbage('collect');assert(next(weak)==nil)
        local v=saved_spline:evaluate(0);assert(v[1]==3 and v[2]==4 and v[3]==5)
        assert(q:GetChild('NoteField'):GetColumnActors()[1]:GetParent()==q:GetChild('NoteField'))
        assert(q:SetLife(.75)==q)
    "#,
                b"=release player",
            )
            .unwrap(),
        )
        .unwrap();
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
