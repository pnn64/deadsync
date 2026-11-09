use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    self as song, SongLuaCompileContext, SongLuaLivePlayer,
    rust_engine::{ActorScene, RustSongRuntime, test_support},
};
use std::path::Path;

struct Pair {
    rust: RustSongRuntime,
    scenes: [ActorScene; 2],
}
impl Pair {
    fn new(live: bool) -> Self {
        let mut context = SongLuaCompileContext::new(Path::new("."), "generated player migration");
        context.live = live;
        context.players[0].screen_x = 144.5;
        context.players[0].screen_y = -37.25;
        context.players[1].screen_x = 432.25;
        context.players[1].screen_y = 120.;
        context.players[1].enabled = true;
        let mut rust = RustSongRuntime::new(&context).unwrap();
        let scenes = [0, 1].map(|i| {
            let scene = test_support::gameplay_player_base(&mut rust, &context, i).unwrap();
            let actor = rust.actor_scene_root(scene).unwrap();
            rust.vm_mut()
                .set_global(if i == 0 { b"p" } else { b"q" }, actor)
                .unwrap();
            scene
        });
        let mut pair = Self { rust, scenes };
        pair.script(
            r#"
            m=p.__songlua_life_meter;n=q.__songlua_life_meter
            s=STATSMAN:GetCurStageStats();ps=s:GetPlayerStageStats(0);qs=s:GetPlayerStageStats(1)
            observations={}
            function log(...) local t={};for i=1,select('#',...) do t[i]=tostring(select(i,...)) end
                observations[#observations+1]=table.concat(t,',') end
        "#,
        );
        pair
    }
    fn script(&mut self, source: &str) {
        let code = compile_named(source.as_bytes(), b"@/fixture/generated_players.lua").unwrap();
        self.rust
            .vm_mut()
            .execute(&code)
            .unwrap_or_else(|e| panic!("Rust: {source}\n{e:?}"));
        let code =
            compile_named(b"return table.concat(observations,'|')", b"=observations").unwrap();
        let Value::String(actual) = self.rust.vm_mut().execute(&code).unwrap()[0] else {
            panic!()
        };
        reference_test::assert_source_debug(
            "song/tests/rust_generated_players.rs::observations",
            source,
            &self.rust.vm_mut().string_bytes(actual).unwrap(),
        );
        reference_test::assert_source_debug(
            "song/tests/rust_generated_players.rs::writes",
            source,
            &self.rust.take_player_writes(),
        );
        reference_test::assert_source_debug(
            "song/tests/rust_generated_players.rs::players",
            source,
            &test_support::rust_live_players(&self.rust),
        );
    }
    fn publish(&mut self) {
        let mut players = [SongLuaLivePlayer::default(); 2];
        players[0].life = 0.75;
        players[1].life = 0.25;
        self.rust.publish_live_players(players);
    }
}

#[test]
fn constructors_own_attack_display_and_preserve_actor_methods() {
    for live in [false, true] {
        let mut pair = Pair::new(live);
        pair.script(
            r#"
            assert(PlayerActor==nil and LifeMeter==nil)
            for i,a in ipairs{p,q} do
                assert(a:GetName()=='PlayerP'..i and a:GetParent()==nil)
                assert(a.__songlua_player_index==i-1 and a.__songlua_visible)
                assert(a.__songlua_style_name=='single')
                local child=a:GetChild('')
                assert(child==a[1] and child==a.__songlua_attack_display and a[2]==nil)
                assert(child:GetParent()==a and child:GetName()=='' and child:GetX()==-40)
                assert(a:GetNumChildren()==1 and a:GetChild('missing')==nil)
                log(a:GetX(),a:GetY(),a:GetDestX(),a:GetDestY(),child:GetY())
            end
            assert(p:GetX()==144.5 and p:GetY()==-37.25)
            assert(q:GetX()==432.25 and q:GetY()==120)
            assert(m:GetName()=='LifeP1' and n:GetName()=='LifeP2')
            assert(m:GetParent()==nil and m.SetUpdateFunction==nil and p.SetUpdateFunction~=nil)
            assert(not p:get_oitg_zoom_mode())
            assert(p:set_oitg_zoom_mode(0)==p and p:get_oitg_zoom_mode())
            assert(p:set_oitg_zoom_mode(false)==p and not p:get_oitg_zoom_mode())
            local f=p.set_oitg_zoom_mode;local t={};assert(f(t,'yes')==t)
            assert(p.get_oitg_zoom_mode(t));t.__songlua_oitg_zoom_mode='invalid'
            assert(p.get_oitg_zoom_mode(t));assert(not pcall(f,false,true))
            for _,v in ipairs{false,0,'',{},true} do
                t.__songlua_oitg_zoom_mode=v;log(p.get_oitg_zoom_mode(t))
            end
            p:x(21);p[1]:y(11);assert(p:GetX()==21 and p[1]:GetY()==11)
            ActorFrame.SetLife=function(self)return self,'override' end
            local a,b=p:SetLife(.3);assert(a==p and b=='override');ActorFrame.SetLife=nil
            Actor.GetLife=function()return 12 end;assert(m:GetLife()==12);Actor.GetLife=nil
            log(m:GetLife(),m:IsFailing(),m:IsHot(),m:IsInDanger())
        "#,
        );
    }
}

#[test]
fn life_queries_capture_meter_and_distinguish_live_value_from_local_flags() {
    for live in [false, true] {
        let mut pair = Pair::new(live);
        pair.script(
            r#"
            local get=m.GetLife
            assert(get()==m:GetLife() and get(q,1,2)==m:GetLife())
            for _,value in ipairs{0,.1,.2,1,2,-1,'0.125'} do
                m.__songlua_life=value;log(get(),m:IsFailing(),m:IsHot(),m:IsInDanger())
            end
            m.__songlua_life=nil;log(get(),m:IsFailing())
            m.__songlua_life=false;assert(not pcall(get))
            m.__songlua_life=.1
        "#,
        );
        pair.publish();
        pair.script(
            r#"
            assert(m:GetLife()==.75 and n:GetLife()==.25)
            assert(m:IsInDanger() and not m:IsHot())
            m.__songlua_life=false;assert(m:GetLife()==.75 and not pcall(m.IsHot))
            m.__songlua_life=1;assert(m:IsHot() and m:GetLife()==.75)
        "#,
        );
    }
}

#[test]
fn set_life_updates_shared_stats_and_notifies_before_side_effect() {
    for live in [false, true] {
        let mut pair = Pair::new(live);
        pair.publish();
        pair.script(
            r#"
            p.LifeChangedMessageCommand=function(self,params)
                assert(self==p and params.Player=='PlayerNumber_P1' and params.LifeMeter==m)
                assert(p.__songlua_inside_set_life)
                assert(ps.__songlua_life==m.__songlua_life and m:GetLife()==ps.__songlua_life)
                log('message',m:GetLife(),__songlua_side_effect_count)
                assert(not pcall(p.SetLife,p,.8));collectgarbage('collect')
            end
            __songlua_side_effect_count=0
            assert(p:SetLife('0.2')==p and p.__songlua_inside_set_life==false)
            assert(__songlua_side_effect_count==1)
            p:SetLife(5);p:SetLife(-1)
            p.LifeChangedMessageCommand=nil
            local original=ps
            s.__songlua_player_stats={};STATSMAN=nil;ps=nil;collectgarbage('collect')
            p:SetLife(.625);assert(original.__songlua_life==.625 and m:GetLife()==.625)
            assert(select('#',p:SetLife(.5))==1)
            local saved=p.SetLife
            assert(saved(q,.375)==q and n.__songlua_life==.375)
            assert(original.__songlua_life==.375 and qs.__songlua_life~= .375)
            log(__songlua_side_effect_count,m:GetLife(),n:GetLife(),n.__songlua_life)
        "#,
        );
    }
}

#[test]
fn errors_keep_native_partial_writes_and_clear_recursion_guard() {
    for live in [false, true] {
        let mut pair = Pair::new(live);
        pair.publish();
        pair.script(r#"
            __songlua_side_effect_count=0
            p.LifeChangedMessageCommand=function()collectgarbage('collect');error('authored failure')end
            assert(not pcall(p.SetLife,p,.125))
            assert(ps.__songlua_life==.125 and m:GetLife()==.125 and not p.__songlua_inside_set_life)
            assert(__songlua_side_effect_count==0)
            p.LifeChangedMessageCommand=nil
            p.__songlua_life_meter=false
            assert(not pcall(p.SetLife,p,.25) and ps.__songlua_life==.25)
            assert(m:GetLife()==.25 and m.__songlua_life==.125 and not p.__songlua_inside_set_life)
            p.__songlua_life_meter=m
            for _,bad in ipairs{false,{},'invalid'} do
                assert(not pcall(p.SetLife,p,bad) and ps.__songlua_life==.25)
            end
            p.__songlua_inside_set_life='wrong';assert(not pcall(p.SetLife,p,.5))
            p.__songlua_inside_set_life=nil
            __songlua_side_effect_count='invalid'
            assert(not pcall(p.SetLife,p,.375) and ps.__songlua_life==.375)
            assert(m.__songlua_life==.375 and not p.__songlua_inside_set_life)
            __songlua_side_effect_count='1.5';p:SetLife(.5);assert(__songlua_side_effect_count==2)
            log(ps.__songlua_life,m:GetLife(),p.__songlua_inside_set_life)
        "#);
    }
}

#[test]
fn meter_getters_can_reenter_collect_and_replace_their_receiver_fields() {
    let mut pair = Pair::new(false);
    pair.script(
        r#"
        local get=m.GetLife;local mt=getmetatable(m);local previous=mt.__index
        m.__songlua_life=nil
        mt.__index=function(t,k)
            if k=='__songlua_life' then
                p:SetLife(.25);collectgarbage('collect');return '.625'
            end
            return previous(t,k)
        end
        assert(get()==.625 and m.__songlua_life==.25 and get()==.25)
        mt.__index=previous
        log(m:GetLife(),ps.__songlua_life,__songlua_side_effect_count)
    "#,
    );
}

#[test]
fn released_player_scene_unsubscribes_and_releases_generated_owners() {
    let mut pair = Pair::new(true);
    pair.script(
        r#"
        weak=setmetatable({p,m,p[1]}, {__mode='v'})
        p.LifeChangedMessageCommand=function()error('retired callback')end
        saved=m.GetLife;setter=p.SetLife;zoom=p.get_oitg_zoom_mode
        p=nil;m=nil;collectgarbage('collect')
        assert(weak[1] and weak[2] and weak[3])
    "#,
    );
    assert!(pair.rust.release_actor_scene(pair.scenes[0]).unwrap());
    let code = compile_named(
        br#"
        collectgarbage('collect')
        assert(weak[1]==nil and weak[3]==nil and weak[2]~=nil)
        assert(saved()==.5)
        saved=nil;collectgarbage('collect');assert(weak[2]==nil)
        assert(not zoom(q));assert(setter(q,.25)==q)
        q:SetLife(.25)
    "#,
        b"=retire generated player",
    )
    .unwrap();
    pair.rust.vm_mut().execute(&code).unwrap();
    assert!(!pair.rust.release_actor_scene(pair.scenes[0]).unwrap());
}

#[test]
fn execution_limit_unwinds_life_notification_and_allows_later_updates() {
    let mut pair = Pair::new(true);
    pair.publish();
    pair.script("p.LifeChangedMessageCommand=function()while true do end end");
    let code = compile_named(b"p:SetLife(.25)", b"=limited life notification").unwrap();
    pair.rust.vm_mut().set_instruction_limit(Some(1000));
    let error = pair.rust.vm_mut().execute(&code).unwrap_err();
    assert_eq!(error.kind(), deadlib_lua::ErrorKind::Limit);
    pair.rust.vm_mut().set_instruction_limit(None);
    let code = compile_named(
        br#"
        assert(p.__songlua_inside_set_life==false)
        assert(ps.__songlua_life==.25 and m:GetLife()==.25)
        p.LifeChangedMessageCommand=nil
        p:SetLife(.375)
        assert(m:GetLife()==.375 and ps.__songlua_life==.375)
    "#,
        b"=recovered life notification",
    )
    .unwrap();
    pair.rust.vm_mut().execute(&code).unwrap();
    let writes = pair.rust.take_player_writes();
    assert_eq!(
        writes,
        vec![
            song::rust_engine::SongLuaLivePlayerWrite::SetLife {
                player: 0,
                life: 0.25
            },
            song::rust_engine::SongLuaLivePlayerWrite::SetLife {
                player: 0,
                life: 0.375
            },
        ]
    );
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
