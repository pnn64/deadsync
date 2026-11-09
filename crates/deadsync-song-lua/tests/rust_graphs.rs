use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext,
    rust_engine::{ActorScene, RustSongRuntime},
};

fn execute(runtime: &mut RustSongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/graphs.lua").unwrap())
        .unwrap_or_else(|e| panic!("{source}\n{e:?}"))
        .to_vec()
}
struct Pair {
    rust: RustSongRuntime,
    scene: ActorScene,
    seconds: f32,
    _files: tempfile::TempDir,
}
impl Pair {
    fn new(live: bool, players: usize) -> Self {
        let files = tempfile::tempdir().unwrap();
        let mut context = SongLuaCompileContext::new(files.path(), "graph migration");
        context.live = live;
        for (i, p) in context.players.iter_mut().enumerate() {
            p.enabled = i < players;
        }
        let source = r#"
            observations={}
            function log(...) for i=1,select('#',...) do
                local v=select(i,...);observations[#observations+1]=type(v)=='number' and string.format('%.9g',v) or tostring(v)
            end end
            return Def.ActorFrame{Def.GraphDisplay{Name='graph',
                Def.Actor{Name='authored',InitCommand=function(self) log('init',self:GetParent():GetName()) end},
                InitCommand=function(self)
                    g=self;body=self:GetChild('');line=self:GetChild('Line');authored=self:GetChild('authored')
                    log('graph init',#body,self:GetNumChildren(),self:GetWidth(),self:GetHeight(),line:GetName())
                    assert(self[1]==authored and self[2]==nil)
                    assert(body[1]:GetParent()==self and body[2]:GetParent()==self and line:GetParent()==self)
                end
            }}
        "#;

        let mut rust = RustSongRuntime::new(&context).unwrap();
        let definition = execute(&mut rust, source)[0];
        let scene = rust.instantiate_actor(definition).unwrap();
        let root = rust.actor_scene_root(scene).unwrap();
        rust.vm_mut().set_global(b"root", root).unwrap();
        let mut pair = Self {
            rust,
            scene,
            seconds: 0.0,
            _files: files,
        };
        pair.compare("construction", false);
        pair
    }
    fn compare(&mut self, phase: &str, error: bool) {
        let actor = self.rust.vm_mut().get_global(b"g").unwrap();
        let actual = self.rust.actor_graph_display(actor);
        if error {
            assert!(actual.is_err(), "{phase}: Rust");
        } else {
            reference_test::assert_debug(
                "song/tests/rust_graphs.rs::payload",
                &(phase, &actual.unwrap()),
            );
        }
        let source = "return table.concat(observations,'|')";
        let Value::String(actual) = execute(&mut self.rust, source)[0] else {
            panic!("observations")
        };
        reference_test::assert_debug(
            "song/tests/rust_graphs.rs::observations",
            &(phase, &self.rust.vm_mut().string_bytes(actual).unwrap()),
        );
    }
    fn script(&mut self, source: &str) {
        execute(&mut self.rust, source);
        self.compare(source, false);
    }
    fn advance(&mut self, delta: f64) {
        self.seconds += delta as f32;

        self.rust
            .set_values(self.seconds * 2.0, self.seconds)
            .unwrap();

        self.rust.advance_actor_scene(self.scene, delta).unwrap();
        self.compare("advance", false);
    }
}

#[test]
fn construction_children_sizes_and_private_receiver_binding() {
    for live in [false, true] {
        for players in [0, 1, 2] {
            let mut p = Pair::new(live, players);
            p.script(r#"
            assert(GraphDisplay==nil and GraphDisplayBody==nil and GraphDisplayLine==nil)
            assert(g.SetUpdateFunction~=nil and body[1].SetUpdateFunction==nil)
            assert(g.SetDrawFunction~=nil and line.SetDrawFunction==nil)
            assert(g:Load('GraphDisplay2')==g and g.__songlua_graph_display_metric=='GraphDisplay2')
            local load=g.Load;assert(load('metric')==g and g.__songlua_graph_display_metric=='metric')
            load({},'bound');load(false);load(42);load();load(string.char(255))
            assert(g.__songlua_graph_display_metric=='bound')
            g:SetWidth(120);body[2]:visible(false);line:addy(1)
            assert(g:GetWidth()==120 and g:GetHeight()==64 and line:GetY()==1)
            body:diffusealpha(.6);assert(body[1]:GetDiffuseAlpha()==1)
            local previous=ActorFrame.Load;ActorFrame.Load=function(self)assert(self==g);return 'public' end
            assert(g:Load()=='public');ActorFrame.Load=previous
            body[2]:name('renamed');line:name('renamed-line')
            assert(g:GetChild('Line')==line and g:GetChild('') [2]==body[2])
            assert(g:GetChild('renamed-line')==nil)
            g:SortByDrawOrder();assert(g[1]==authored and g[2]==nil)
        "#);
        }
    }
}

#[test]
fn set_retains_stats_calls_life_record_and_normalizes_owned_samples() {
    for live in [false, true] {
        let mut p = Pair::new(live, 2);
        p.script(r#"
            stage={__songlua_total_steps_seconds='0x20'}
            player={GetLifeRecord=function(self,seconds,count)
                assert(self==player and seconds==32 and count==100)
                assert(g.__songlua_graph_display_set and g.__songlua_graph_display_stage_stats==stage and g.__songlua_graph_display_player_stats==player)
                collectgarbage('collect');g:Load('reentrant')
                record={-1,2,'0.75',false,'nan','inf','-inf','0x10',0/0,math.huge,1e100}
                return record,'ignored'
            end}
            assert(g:Set(stage,player)==g)
            assert(g.__songlua_graph_display_values==record and g.__songlua_graph_display_metric=='reentrant')
        "#);
        let g = p.rust.vm_mut().get_global(b"g").unwrap();
        let held = p.rust.actor_graph_display(g).unwrap();
        p.script("record[1]=.25;record[2]=.125;collectgarbage('collect')");
        assert_eq!(held.body_values[0], 0.0);
        assert_eq!(held.body_values[1], 1.0);
        p.script("local f=g.Set;assert(f({},stage,player)==g);g:Set();assert(g.__songlua_graph_display_stage_stats==stage and g.__songlua_graph_display_player_stats==player)");
        p.script("g:Set(nil,nil);assert(g.__songlua_graph_display_stage_stats==nil and g.__songlua_graph_display_player_stats==nil)");
    }
}

#[test]
fn failed_stage_or_player_queries_publish_fallback_and_keep_prior_mutations() {
    let mut p = Pair::new(true, 1);
    p.script(
        r#"
        stage=setmetatable({}, {__index=function()collectgarbage('collect');error('stage')end})
        player=setmetatable({}, {__index=function(self,key)
            assert(key=='GetLifeRecord');collectgarbage('collect')
            return function(self,seconds,count)
                assert(self==player and seconds==0 and count==100)
                g:x(17);collectgarbage('collect');return {[1]=.1,[100]=.9}
            end
        end})
        g:Set(stage,player);assert(g:GetX()==17)
    "#,
    );
    for value in [
        "{}",
        "{GetLifeRecord=false}",
        "{GetLifeRecord=setmetatable({},{__call=function()error('not a function')end})}",
        "{GetLifeRecord=function()g:y(13);error('record error')end}",
        "{GetLifeRecord=function()return false end}",
    ] {
        p.script(&format!("player={value};assert(g:Set({{}},player)==g);assert(g.__songlua_graph_display_player_stats==player);log(g:GetY())"));
    }
    p.script(r#"
        local old=g.__songlua_graph_display_values
        local set=g.Set
        g.__songlua_actor_type='Actor';assert(set(g,{}, {})==g and g.__songlua_graph_display_values==old)
        g.__songlua_actor_type='GraphDisplay'
        g:Load('saved');g:Set(false,false);assert(g.__songlua_graph_display_stage_stats==false)
        assert(#g.__songlua_graph_display_values==100)
    "#);
}

#[test]
fn named_child_aliases_groups_and_read_errors_recover() {
    let mut p = Pair::new(true, 2);
    p.script(r#"
        body[1]:x(3);body[2]:x(7);line:x(11)
        children=g:GetChildren();assert(children[''][1]==body[1] and children.Line==line and children.authored==authored)
        children[''][1]=line;assert(g:GetChild('')[1]==body[1])
        g.__songlua_children.alias=line;assert(g:GetChild('alias')==line and g:GetNumChildren()==4)
        g.__songlua_children['']=nil;assert(g:GetChild('')==nil)
    "#);
    p.script("g.__songlua_children['']=body;g.__songlua_children.Line=nil");
    p.script("g.__songlua_children.Line=line;authored:name('Line');assert(g:GetChild('Line')[1]==line and g:GetChild('Line')[2]==authored)");
    p.script("authored:name('authored')");
    for source in [
        "g.__songlua_graph_display_values=false",
        "g.__songlua_graph_display_values=nil;g.__songlua_children.Line=false",
        "g.__songlua_children.Line=line;g.__songlua_children['']=false",
    ] {
        execute(&mut p.rust, source);
        p.compare(source, true);
    }
    p.script("g.__songlua_children['']=body;g.__songlua_graph_display_values=setmetatable({[1]=.9},{__index=function()error('raw only')end})");
}

#[test]
fn auxiliary_children_keep_native_startup_update_and_command_rules() {
    let mut p = Pair::new(true, 1);
    p.script(
        r#"
        for _,a in ipairs{g,body[1],body[2],line,authored} do
            local name=a==body[1] and 'body1' or a==body[2] and 'body2' or a:GetName()
            a:addcommand('Begin',function(self)log('Begin',name)end)
            a:addcommand('On',function(self)log('On',name)end)
            a:addcommand('End',function(self)log('End',name)end)
            a:addcommand('Probe',function(self)log('Probe',name)end)
        end
        g:playcommandonchildren('Probe')
        line:linear(1):x(20);authored:linear(1):x(30)
        g:SetUpdateFunction(function(self,dt)
            collectgarbage('collect');log('update',line:GetX(),authored:GetX())
            self:Set({}, {GetLifeRecord=function()return {[1]=dt}end})
        end)
    "#,
    );

    p.rust.start_actor_scene(p.scene).unwrap();
    p.compare("start", false);
    p.advance(0.25);
    p.script("g:hurrytweening(.5);log(line:GetTweenTimeLeft(),authored:GetTweenTimeLeft(),g:GetTweenTimeLeft())");
    p.script("log(line:GetTweenTimeLeft(),authored:GetTweenTimeLeft(),g:GetTweenTimeLeft());g:finishtweening();log(line:GetTweenTimeLeft(),authored:GetTweenTimeLeft());g:zwrite(true);assert(line:GetVisible())");
    p.script("line:linear(1):x(33);for i=1,52 do g:linear(1) end;assert(line:GetTweenTimeLeft()==0);log(g:GetTweenTimeLeft())");

    p.rust.end_actor_scene(p.scene).unwrap();
    p.compare("end", false);
}

#[test]
fn graph_snapshots_and_retained_callbacks_survive_collection_and_retirement() {
    let mut p = Pair::new(true, 1);
    p.script(
        r#"
        weak=setmetatable({}, {__mode='v'})
        g:Set({}, {GetLifeRecord=function()
            local values={[1]=.25};weak[1]=values;collectgarbage('collect');return values
        end})
        retained=g.Set;collectgarbage('collect');assert(weak[1]~=nil)
        line:diffuse({.2,.3,.4,.5});body[2]:visible(false)
    "#,
    );
    let g = p.rust.vm_mut().get_global(b"g").unwrap();
    let held = p.rust.actor_graph_display(g).unwrap();
    p.rust.release_actor_scene(p.scene).unwrap();
    execute(
        &mut p.rust,
        "collectgarbage('collect');assert(not pcall(retained,g,{},{}));root=nil;g=nil;body=nil;line=nil;authored=nil;retained=nil;collectgarbage('collect');collectgarbage('collect');assert(weak[1]==nil)",
    );
    assert_eq!(held.body_values[0], 0.25);
    assert!(!held.body_state.visible);
    assert_eq!(held.line_state.diffuse[3], 0.5);
    assert!(p.rust.actor_graph_display(g).is_err());
}

#[test]
fn native_child_message_subscriptions_precede_ordinary_scene_registration() {
    let mut p = Pair::new(true, 2);
    // Registration order controls the discovery reads. Message delivery uses
    // identities, represented by independently captured dense delivery ranks.
    p.script(
        r#"
        for i,a in ipairs{root,g,body[1],body[2],line,authored} do
            local label=i
            local mt={};for k,v in pairs(getmetatable(a)) do mt[k]=v end
            local index=mt.__index
            mt.__index=function(self,key)
                if key=='ScanMessageCommand' then log('scan',label) end
                if type(index)=='function' then return index(self,key) end
                return index and index[key]
            end
            setmetatable(a,mt)
        end
        MESSAGEMAN:Broadcast('Scan')
    "#,
    );
    let source = "return {root,g,body[1],body[2],line,authored}";
    let ranks = reference_test::source_indices("song/tests/rust_graphs.rs::message_ranks", source);
    let mut ordered = ranks.clone();
    ordered.sort();
    assert_eq!(ordered, (1..=6).collect::<Vec<_>>());
    let Value::Table(actors) = execute(&mut p.rust, source)[0] else {
        panic!("actor identities")
    };
    for (i, rank) in (1..=6).zip(ranks) {
        let actor = p
            .rust
            .vm_mut()
            .raw_table_get(actors, Value::Number(i as f64))
            .unwrap();
        assert!(p.rust.set_actor_message_identity(actor, rank));
    }
    p.script(
        r#"
        for i,a in ipairs{root,g,body[1],body[2],line,authored} do
            local index=i
            a:addcommand('ProbeMessage',function(self) log('message',index) end)
        end
        MESSAGEMAN:Broadcast('Probe');collectgarbage('collect');MESSAGEMAN:Broadcast('Probe')
    "#,
    );
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
