use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext,
    rust_engine::{ActorScene, RustSongRuntime},
};

fn runtime() -> RustSongRuntime {
    RustSongRuntime::new(&SongLuaCompileContext::new(
        std::path::Path::new("."),
        "Rust actor instance tests",
    ))
    .unwrap()
}
fn execute(runtime: &mut RustSongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/actors.lua").unwrap())
        .unwrap()
        .to_vec()
}
fn instantiate(runtime: &mut RustSongRuntime, source: &str) -> ActorScene {
    let definition = execute(runtime, source)[0];
    let scene = runtime.instantiate_actor(definition).unwrap();
    let root = runtime.actor_scene_root(scene).unwrap();
    runtime.vm_mut().set_global(b"root", root).unwrap();
    scene
}
fn assert_clean(runtime: &RustSongRuntime) {
    assert_eq!(runtime.take_actor_diagnostics(), Vec::<String>::new());
}

#[derive(Debug, PartialEq)]
enum Observation {
    Nil,
    Boolean(bool),
    Number(f64),
    Bytes(Vec<u8>),
}

// Run the same declaration and probe through the actual production constructor
// and the Rust adapter. Assertions inside probes must pass on both engines;
// the returned observations also catch otherwise unnoticed ordering changes.
fn compare_scene(definition: &str, probe: &str) {
    let mut rust = runtime();
    instantiate(&mut rust, definition);
    let actual: Vec<_> = execute(&mut rust, probe)
        .into_iter()
        .map(|value| match value {
            Value::Nil => Observation::Nil,
            Value::Boolean(value) => Observation::Boolean(value),
            Value::Number(value) => Observation::Number(value),
            Value::String(value) => {
                Observation::Bytes(rust.vm_mut().string_bytes(value).unwrap().to_vec())
            }
            other => panic!("non-scalar Rust observation: {other:?}"),
        })
        .collect();
    reference_test::assert_source_debug(
        "song/tests/rust_actors.rs::compare_scene",
        format!("{definition}\0{probe}"),
        &actual,
    );
}

enum Phase<'a> {
    Start,
    Advance(f64),
    Script(&'a str),
}

fn compare_lifecycle(definition: &str, phases: &[Phase<'_>]) {
    let source = format!(
        r#"
        events={{}}
        function log(...) for i=1,select('#',...) do
            local value=select(i,...)
            events[#events+1]=type(value)=='number' and string.format('%.17g',value) or tostring(value)
        end end
        function observe()
            local result={{table.concat(events,':')}}
            local function visit(a)
                result[#result+1]=a:GetName()
                for _,v in ipairs{{a:GetX(),a:GetY(),a:GetZ(),a:GetDestX(),a:GetDestY(),a:GetDestZ(),
                    a:GetZoomX(),a:GetZoomY(),a:GetRotationZ(),a:getaux(),a:GetTweenTimeLeft()}} do
                    result[#result+1]=v~=v and 'nan' or string.format('%.17g',v)
                end
                for i=1,#a do visit(a[i]) end
            end
            visit(root)
            return table.concat(result,'|')
        end
        {definition}
    "#
    );
    let mut rust = runtime();
    let scene = instantiate(&mut rust, &source);
    let mut seconds: f64 = 0.0;
    for (index, phase) in phases.iter().enumerate() {
        match phase {
            Phase::Start => {
                assert!(rust.start_actor_scene(scene).unwrap());
            }
            Phase::Advance(delta) => {
                seconds += delta;
                assert!(rust.advance_actor_scene(scene, *delta).unwrap());
            }
            Phase::Script(source) => {
                execute(&mut rust, source);
            }
        }
        let Value::String(actual) = execute(&mut rust, "return observe()")[0] else {
            panic!("expected observation string");
        };
        let actual = std::str::from_utf8(rust.vm_mut().string_bytes(actual).unwrap()).unwrap();
        reference_test::assert_source_debug(
            "song/tests/rust_actors.rs::compare_lifecycle",
            &source,
            &(index, seconds.to_bits(), actual),
        );
    }
}

#[test]
fn lifecycle_startup_and_exact_queue_boundaries_match_production() {
    use Phase::*;
    compare_lifecycle(
        r#"
        local function init(self) log('init',self:GetName());self:queuecommand('Queued') end
        local function begin(self) log('begin',self:GetName()) end
        local function on(self) log('on',self:GetName());self:sleep(0.5):queuecommand('After') end
        local function queued(self,...) log('queued',self:GetName(),select('#',...));self:addx(2) end
        local function after(self) log('after',self:GetName());self:linear(0.25):x(12) end
        return Def.ActorFrame{Name='parent',InitCommand=init,BeginCommand=begin,OnCommand=on,
            QueuedCommand=queued,AfterCommand=after,
            Def.Quad{Name='child',InitCommand=init,BeginCommand=begin,OnCommand=on,
                QueuedCommand=queued,AfterCommand=after}}
    "#,
        &[
            Script("assert(#events==4)"),
            Start,
            Start,
            Advance(0.0),
            Advance(0.5),
            Script("collectgarbage('collect')"),
            Advance(0.0),
            Advance(0.125),
            Advance(0.125),
            Advance(0.01),
        ],
    );
}

#[test]
fn lifecycle_easing_destinations_and_native_bezier_match_production() {
    use Phase::*;
    compare_lifecycle(
        r#"return Def.Quad{Name='quad'}"#,
        &[
            Script(
                "root:x(3):linear(0.5):x(11):y(7):zoomx(3):rotationz(90):aux(6):accelerate(0.25):x(-9)",
            ),
            Advance(0.0),
            Advance(0.125),
            Advance(0.375),
            Advance(0.125),
            Advance(0.125),
            Script(
                "root:decelerate(0.5):x(5):smooth(0.5):x(20):spring(0.5):x(2):bouncebegin(0.5):x(40):bounceend(0.5):x(5)",
            ),
            Advance(0.3),
            Advance(0.3),
            Advance(0.3),
            Advance(0.3),
            Advance(0.3),
            Advance(0.3),
            Advance(0.3),
            Advance(0.4),
            Script("root:tween('0.5','TweenType_Bezier',{0,0.2,0.8,1}):x(50)"),
            Advance(0.1),
            Advance(0.2),
            Advance(0.2),
            Script("root:tween(0.5,4,{0,0,0.25,0.1,0.75,0.9,1,1}):x(-20)"),
            Advance(0.1),
            Advance(0.4),
            Script("root:tween(0.25,'accelerate'):x(8)"),
            Advance(0.125),
            Advance(0.125),
        ],
    );
}

#[test]
fn lifecycle_queue_reentry_stop_finish_hurry_and_overflow_match_production() {
    use Phase::*;
    compare_lifecycle(
        r#"
        return Def.ActorFrame{Name='parent',
            ReplaceCommand=function(self) log('replace');self:stoptweening():linear(0.5):x(12):queuecommand('Done') end,
            DoneCommand=function(self) log('done',self:GetName()) end,
            Def.Quad{Name='child',DoneCommand=function(self)log('done',self:GetName())end}}
    "#,
        &[
            Script(
                "root:sleep(0.25):queuecommand('Replace'):linear(4):x(900);root[1]:linear(2):x(8):queuecommand('Done')",
            ),
            Advance(0.375),
            Script("root:hurrytweening(0.5)"),
            Advance(0.125),
            Script("root:stoptweening();assert(root[1]:GetTweenTimeLeft()>0)"),
            Advance(0.25),
            Script("root:linear(4):x(99):queuecommand('Done');root:finishtweening()"),
            Advance(0.1),
            Script(
                "root[1]:linear(10):x(77):queuecommand('Done');for i=1,52 do root:linear(1):x(i) end",
            ),
            Advance(0.5),
            Script("root:hurrytweening(0)"),
            Advance(0.0),
            Advance(0.1),
        ],
    );
}

#[test]
fn lifecycle_hibernation_rates_update_order_and_replacement_match_production() {
    use Phase::*;
    compare_lifecycle(
        r#"
        local function update(self,delta) log('update',self:GetName(),delta,self:GetX()) end
        return Def.ActorFrame{Name='root',InitCommand=function(self)
            self:linear(2):x(8):SetUpdateRate(2):SetUpdateFunction(update)
        end,Def.ActorFrame{Name='first',InitCommand=function(self)
            self:linear(1):x(16):SetUpdateRate(0.5):hibernate(0.25):SetUpdateFunction(function(self,delta)
                log('first',delta,self:GetX())
                self:GetParent():SetUpdateFunction(function(a,d)log('new',d,a:GetX())end)
            end)
        end},Def.ActorFrame{Name='last',InitCommand=function(self)
            self:SetUpdateFunction(update)
        end}}
    "#,
        &[
            Start,
            Advance(0.0),
            Advance(0.125),
            Advance(0.125),
            Script("root:hibernate(0.5);root[1]:hibernate(0.25)"),
            Advance(0.25),
            Advance(0.25),
            Advance(0.125),
            Script(
                "root:SetUpdateRate('0.5');root[1]:SetUpdateFunction(nil);collectgarbage('collect')",
            ),
            Advance(0.2),
            Script("root:hibernate(-1);root[2]:hibernate(math.huge)"),
            Advance(0.5),
            Script("root[2]:hibernate(0/0)"),
            Advance(0.0),
            Advance(0.1),
        ],
    );
}

#[test]
fn lifecycle_queued_callback_replacement_gc_and_partial_errors_match_production() {
    use Phase::*;
    compare_lifecycle(
        r#"
        local payload={amount=7}
        return Def.ActorFrame{Name='root',OnCommand=function(self)
            self:SetUpdateFunction(function(a,d) log('old',d) end):queuecommand('Install')
        end,InstallCommand=function(self)
            self.InstallCommand=nil
            collectgarbage('collect')
            self:SetUpdateFunction(function(a,d)
                a:addx(payload.amount); log('new',d); error('update failed after write')
            end)
        end}
    "#,
        &[
            Start,
            Advance(0.0),
            Advance(0.125),
            Script("collectgarbage('collect')"),
            Advance(0.125),
            Advance(0.25),
            Script("root:SetUpdateFunction(nil)"),
            Advance(0.1),
        ],
    );
}

#[test]
fn lifecycle_numeric_coercions_and_invalid_calls_match_production() {
    compare_lifecycle(
        r#"return Def.ActorFrame{Name='root',Def.Quad{Name='child'}}"#,
        &[
            Phase::Script(
                r#"
            for _,v in ipairs{false,{},'bad',-2,0,0.1,'0.1','1e100','inf',math.huge,0/0} do
                root:stoptweening():linear(v):x(5);log('linear',root:GetTweenTimeLeft())
                root:stoptweening():sleep(v):x(10);log('sleep',root:GetTweenTimeLeft())
                root:stoptweening():hibernate(v);log('hibernate',root:GetTweenTimeLeft())
                root:hibernate(0)
                log('rate',pcall(function()root:SetUpdateRate(v)end),root:GetUpdateRate())
                log('hurry',(pcall(function()root:hurrytweening(v)end)))
            end
            for _,v in ipairs{false,{},'bad',-1,5,0.1,'0','linear','TweenType_Linear'} do
                log('tween',(pcall(function()root:tween(0.5,v)end)))
            end
            root:stoptweening():SetUpdateRate(1)
            log((pcall(function()root:SetUpdateFunction({})end)))
            root:queuecommand(5):queuecommand(false):queuecommand({})
        "#,
            ),
            Phase::Advance(0.125),
        ],
    );
}

#[test]
fn lifecycle_scene_ownership_release_and_update_diagnostic_lifetime() {
    let mut rust = runtime();
    let mut other = runtime();
    let scene = instantiate(
        &mut rust,
        r#"return Def.ActorFrame{InitCommand=function(self)
        self:SetUpdateFunction(function(a) a:addx(1);error('partial')end)
    end}"#,
    );
    assert!(!other.start_actor_scene(scene).unwrap());
    assert!(!other.advance_actor_scene(scene, 0.1).unwrap());
    for _ in 0..3 {
        assert!(rust.advance_actor_scene(scene, 0.1).unwrap());
    }
    assert_eq!(
        rust.actor_scene_snapshot(scene).unwrap()[0].position[0],
        3.0
    );
    assert_eq!(rust.take_actor_diagnostics().len(), 1);
    assert!(rust.release_actor_scene(scene).unwrap());
    assert!(!rust.start_actor_scene(scene).unwrap());
    assert!(!rust.advance_actor_scene(scene, 0.1).unwrap());
    execute(
        &mut rust,
        "collectgarbage('collect');assert(not pcall(function()root:queuecommand('Lost')end))",
    );
}

#[test]
fn lifecycle_preserves_production_traversal_cache_and_callback_lookup() {
    use Phase::*;
    compare_lifecycle(
        r#"
        local function update(self,delta) log(self:GetName(),delta) end
        return Def.ActorFrame{Name='root',InitCommand=function(self)self:SetUpdateFunction(update)end,
            Def.ActorFrame{Name='first',InitCommand=function(self)
                self:SetUpdateFunction(function(a,d)
                    log(a:GetName(),d)
                    if not a.sorted then
                        a.sorted=true;a:draworder(5);a:GetParent():SortByDrawOrder()
                    end
                end)
            end},Def.ActorFrame{Name='second',InitCommand=function(self)self:SetUpdateFunction(update)end}}
    "#,
        &[
            Start,
            Advance(0.1),
            Advance(0.1),
            Script("assert(root[1]:GetName()=='second');root:SetUpdateRate(1)"),
            Advance(0.1),
            Script(
                "root:GetChild('second'):SetUpdateFunction(function(a,d) log('second new',d);a:GetParent():SetUpdateFunction(function(a,d)log('parent new',d)end)end)",
            ),
            Advance(0.1),
        ],
    );
}

#[test]
fn lifecycle_nonterminating_queue_limit_and_recovery_match_production() {
    let source = r#"count=0;return Def.Quad{InitCommand=function(self)self:queuecommand('Again')end,
        AgainCommand=function(self)count=count+1;self:queuecommand('Again')end}"#;
    let mut rust = runtime();
    let scene = instantiate(&mut rust, source);
    let actual = rust.advance_actor_scene(scene, 0.1).unwrap_err();
    assert!(
        actual
            .to_string()
            .contains("nonterminating zero-time actor command queue")
    );
    assert_eq!(
        execute(&mut rust, "return count"),
        vec![Value::Number(10000.)]
    );
    execute(&mut rust, "root:stoptweening():linear(1):x(10)");
    assert!(rust.advance_actor_scene(scene, 0.5).unwrap());
    assert_eq!(
        rust.actor_scene_snapshot(scene).unwrap()[0].position[0],
        5.0
    );
    assert_clean(&rust);
}

#[test]
fn lifecycle_fatal_callback_limits_restore_scope_and_allow_recovery() {
    let mut rust = runtime();
    let scene = instantiate(
        &mut rust,
        r#"__songlua_script_dir='outer';return Def.ActorFrame{
        _Dir='/fixture/inner/', InitCommand=function(self)
            self:SetUpdateFunction(function(a) a:x(7);while true do end end)
        end}"#,
    );
    rust.vm_mut().set_instruction_limit(Some(400));
    let error = rust.advance_actor_scene(scene, 0.1).unwrap_err();
    assert_eq!(error.kind(), deadlib_lua::ErrorKind::Limit);
    rust.vm_mut().set_instruction_limit(None);
    execute(
        &mut rust,
        "assert(__songlua_script_dir=='outer');root:SetUpdateFunction(function(a,d)a:addx(d)end);collectgarbage('collect')",
    );
    assert!(rust.advance_actor_scene(scene, 0.5).unwrap());
    assert_eq!(
        rust.actor_scene_snapshot(scene).unwrap()[0].position[0],
        7.5
    );
    assert_clean(&rust);
}

#[test]
fn production_baseline_actor_loading_and_instance_fields() {
    let mut rust = runtime();
    instantiate(
        &mut rust,
        r#"
        seen={}
        local function observe(self,params)
            assert(params == nil)
            local parent=self:GetParent()
            seen[#seen+1]=self:GetName()..':'..(parent and parent:GetName() or '-')..':'..(parent and parent:GetNumChildren() or self:GetNumChildren())
        end
        shared=Def.Quad{Name='same',InitCommand=function(self,p)
            observe(self,p); self:GetParent():name('temporary'); self:draworder(-1)
        end}
        definition=Def.ActorFrame{Name='root', InitCommand=observe,
            Def.Quad{Name='first', InitCommand=observe},shared,shared,
            Def.Actor{Condition=false,InitCommand=function()error('excluded')end}}
        return definition
    "#,
    );
    let values = execute(
        &mut rust,
        r#"
        local group=root:GetChild('same')
        return table.concat(seen,'|'),root.Class,root.Name,root.InitCommand==definition.InitCommand,
            type(getmetatable(root)), group[1].ctx==nil, group[1]==group[2]
    "#,
    );
    let actual = values
        .into_iter()
        .map(|value| match value {
            Value::String(id) => {
                Observation::Bytes(rust.vm_mut().string_bytes(id).unwrap().to_vec())
            }
            Value::Boolean(value) => Observation::Boolean(value),
            other => panic!("unexpected baseline value: {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        vec![
            Observation::Bytes(b"first::3|same::3|same:temporary:3|root:-:3".to_vec()),
            Observation::Bytes(b"ActorFrame".to_vec()),
            Observation::Bytes(b"root".to_vec()),
            Observation::Boolean(true),
            Observation::Bytes(b"table".to_vec()),
            Observation::Boolean(true),
            Observation::Boolean(false),
        ]
    );
}

#[test]
fn instances_are_distinct_and_child_first_init_observes_native_loading_order() {
    let mut r = runtime();
    let scene = instantiate(
        &mut r,
        r#"
        seen={}
        local function observe(self,params)
            assert(params == nil)
            local parent=self:GetParent()
            seen[#seen+1]=self:GetName()..':'..(parent and parent:GetName() or '-')..':'..(parent and parent:GetNumChildren() or self:GetNumChildren())
        end
        shared=Def.Quad{Name='same',InitCommand=function(self,p)
            observe(self,p); self:GetParent():name('temporary'); self:draworder(-1)
        end}
        definition=Def.ActorFrame{Name='root', InitCommand=observe,
            Def.Quad{Name='first', InitCommand=observe},shared,shared,
            Def.Actor{Condition=false,InitCommand=function()error('excluded')end}}
        return definition
    "#,
    );
    execute(
        &mut r,
        r#"
        assert(table.concat(seen,'|')=='first::3|same::3|same:temporary:3|root:-:3')
        assert(root~=definition and root.Class=='ActorFrame' and root.Name=='root' and root.InitCommand==definition.InitCommand)
        assert(root:GetCommand('Init')==definition.InitCommand)
        local group=root:GetChild('same')
        assert(#group==2 and group[1]~=group[2] and group[1]~=shared)
        assert(group[1].ctx==nil and group[2].ctx==nil)
        root.ctx={answer={42}}; assert(group[1].ctx==nil and group[2].ctx==nil)
        root.Name='field'; assert(root:GetName()=='root')
        assert(type(getmetatable(root))=='table' and pcall(setmetatable,root,getmetatable(root)))
    "#,
    );
    let state = r.actor_scene_snapshot(scene).unwrap();
    assert_eq!(state.len(), 4);
    assert_eq!(state[0].children, vec![2, 3, 1]);
    assert_eq!(state[2].parent, Some(0));
    assert_clean(&r);
}

#[test]
fn full_declaration_snapshot_precedes_init_and_preserves_shared_callback_upvalues() {
    let mut r = runtime();
    instantiate(
        &mut r,
        r#"
        local count=0
        local next=Def.Quad{Name='before',InitCommand=function(self) count=count+1; self:aux(count) end}
        return Def.ActorFrame{Def.Quad{InitCommand=function(self)
            next.Name='after'; next.InitCommand=function()error('mutated declaration')end
            collectgarbage()
        end},next,next}
    "#,
    );
    execute(
        &mut r,
        "local g=root:GetChild('before');assert(#g==2 and g[1]:getaux()==1 and g[2]:getaux()==2 and root:GetChild('after')==nil)",
    );
    assert_clean(&r);
}

#[test]
fn commands_propagate_in_sorted_order_and_preserve_parameter_identity() {
    let mut r = runtime();
    instantiate(
        &mut r,
        r#"
        log={}; params={}
        local function command(self,p) assert(p==params); log[#log+1]=self:GetName() end
        return Def.ActorFrame{Name='root',ProbeCommand=command,
            Def.ActorFrame{Name='frame',ProbeCommand=command,Def.Quad{Name='leaf',ProbeCommand=command}},
            Def.Quad{Name='first',InitCommand=function(self)self:draworder(-1)end,ProbeCommand=command}}
    "#,
    );
    execute(
        &mut r,
        r#"
        assert(root:playcommand('Probe',params)==root)
        assert(table.concat(log,',')=='root,first,frame,leaf')
        log={}; local ok,err=pcall(root.RunCommandsRecursively,root,function(self,p)assert(p==params);log[#log+1]=self:GetName()end,params)
        assert(not ok and tostring(err):find('has no semantic implementation',1,true) and #log==0)
        log={}; root:RunCommandsOnChildren(function(self,p)assert(p==params);log[#log+1]=self:GetName()end,params)
        assert(table.concat(log,',')=='first,frame')
        log={}; root:runcommandsonleaves(function(self,p)assert(p==nil);log[#log+1]=self:GetName()end)
        assert(table.concat(log,',')=='first,leaf')
        local frame=root:GetChild('frame');frame:propagate(true)
        log={};frame:playcommand('Probe',params)
        assert(table.concat(log,',')=='leaf')
    "#,
    );
    assert_clean(&r);
}

#[test]
fn named_children_commands_use_each_childs_handler_and_include_empty_frames_as_leaves() {
    let mut r = runtime();
    instantiate(
        &mut r,
        r#"
        log={}
        local function own(self,p) assert(p==nil);log[#log+1]=self:GetName()end
        return Def.ActorFrame{CustomCommand=function()error('parent handler')end,
            Def.ActorFrame{Name='empty',CustomCommand=own},Def.Quad{Name='leaf',CustomCommand=own}}
    "#,
    );
    execute(
        &mut r,
        r#"
        root:playcommandonchildren('Custom'); assert(table.concat(log,',')=='empty,leaf')
        log={}; root:playcommandonleaves('Custom'); assert(table.concat(log,',')=='empty,leaf')
        log={};root:GetChild('empty'):runcommandsonleaves(function(self)log[#log+1]=self:GetName()end)
        assert(table.concat(log,',')=='empty')
        assert(not pcall(ActorFrame.GetChild,root:GetChild('leaf'),'x'))
    "#,
    );
    assert_clean(&r);
}

#[test]
fn duplicate_child_groups_are_snapshots_with_instance_lookup_and_open_results() {
    let mut r = runtime();
    instantiate(
        &mut r,
        "return Def.ActorFrame{Def.Quad{Name='same'},Def.Quad{Name='same'}}",
    );
    execute(
        &mut r,
        r#"
        local g=root:GetChild('same'); assert(#g==2 and g~=root:GetChild('same'))
        assert(g:x(17)==g[2] and g[1]:GetX()==0 and g[2]:GetX()==17)
        local getx=g.GetX; g[3]=g[1]; assert(getx(g)==0 and g['3']==nil)
        g[2].GetX=function()return 999 end
        assert(root:GetChild('same'):GetX()==999)
        Actor.multi=function(self,...)return self,... end
        local t={}; local a,b,c,d=root:GetChildren().same:multi(t,nil,42)
        assert(a==g[2] and b==t and c==nil and d==42)
        assert(select('#',root:GetChild('same'):multi(nil))==2)
        g[1]:name('changed');assert(root:GetChild('changed')==g[1] and root:GetChild('same')==g[2])
    "#,
    );
    assert_clean(&r);
}

#[test]
fn scene_ownership_survives_gc_and_release_invalidates_methods_without_leaking_cycles() {
    let mut r = runtime();
    let scene = instantiate(
        &mut r,
        r#"
        weak=setmetatable({},{__mode='v'})
        return Def.ActorFrame{InitCommand=function(self)weak[1]=self end,
            Def.Quad{InitCommand=function(self)weak[2]=self;local cycle={self};self:addcommand('Cycle',function()return cycle end)end}}
    "#,
    );
    execute(
        &mut r,
        "root=nil;collectgarbage();collectgarbage();assert(weak[1] and weak[2]);saved=weak[2]",
    );
    assert!(r.release_actor_scene(scene).unwrap());
    assert!(!r.release_actor_scene(scene).unwrap());
    assert!(r.actor_scene_root(scene).is_none());
    execute(
        &mut r,
        "assert(not pcall(saved.GetName,saved));saved=nil;collectgarbage();collectgarbage();assert(weak[1]==nil and weak[2]==nil)",
    );
    assert_clean(&r);
}

#[test]
fn failed_build_releases_partial_instances_and_later_scenes_remain_usable() {
    let mut r = runtime();
    let d = execute(
        &mut r,
        r#"
        weak=setmetatable({},{__mode='v'})
        return Def.ActorFrame{Def.Quad{InitCommand=function(self)weak[1]=self;weak[2]=self:GetParent()end},{Class="UnimplementedActor"}}
    "#,
    )[0];
    assert!(
        r.instantiate_actor(d)
            .unwrap_err()
            .to_string()
            .contains("no Rust instance implementation")
    );
    execute(
        &mut r,
        "collectgarbage();collectgarbage();assert(weak[1]==nil and weak[2]==nil)",
    );
    let scene = instantiate(&mut r, "return Def.Actor{Condition=false}");
    // MakeActor tests Condition only when loading a child, not the root.
    assert!(matches!(r.actor_scene_root(scene), Some(Value::Table(_))));
    assert_eq!(r.actor_scene_snapshot(scene).unwrap().len(), 1);
    compare_scene(
        "return Def.Actor{Condition=false,InitCommand=function(self)self:x(23)end}",
        "assert(root:GetX()==23);return root:GetX()",
    );
    let scene = instantiate(&mut r, "return Def.Quad{Name='healthy'}");
    let mut other = runtime();
    assert!(!other.release_actor_scene(scene).unwrap());
    execute(&mut r, "assert(root:GetName()=='healthy')");
    assert_clean(&r);
}

#[test]
fn authored_command_errors_propagate_theme_errors_report_and_fatal_limits_abort() {
    let mut r = runtime();
    instantiate(
        &mut r,
        "return Def.ActorFrame{ProbeCommand=function()error({reason='stop'})end,Def.Quad{ProbeCommand=function(self)self:x(42)end}}",
    );
    execute(
        &mut r,
        "assert(not pcall(root.playcommand,root,'Probe'));assert(root:GetChildren()['']:GetX()==0)",
    );
    assert_clean(&r);
    execute(
        &mut r,
        r#"
        root.ProbeCommand=assert(loadstring("return function()error('theme failure')end",'@theme:control'))()
        assert(root:playcommand('Probe')==root);assert(root:GetChildren()['']:GetX()==42)
    "#,
    );
    assert_eq!(r.take_actor_diagnostics().len(), 1);
    execute(
        &mut r,
        "__songlua_script_dir='caller';root:addcommand('Forever',function()while true do end end)",
    );
    r.vm_mut().set_instruction_limit(Some(400));
    let error = r
        .vm_mut()
        .execute(&compile_named(b"root:playcommand('Forever')", b"=limit").unwrap())
        .unwrap_err();
    assert_eq!(error.kind(), deadlib_lua::ErrorKind::Limit);
    r.vm_mut().set_instruction_limit(None);
    execute(
        &mut r,
        "assert(__songlua_script_dir=='caller');assert(root:GetNumChildren()==1);root.ForeverCommand=function(self)self:aux(17)end;root:playcommand('Forever');assert(root:getaux()==17)",
    );
    assert_clean(&r);
}

#[test]
fn production_command_fields_reentry_parameters_and_failure_recovery() {
    compare_scene(
        r#"
        log={}
        return Def.Quad{ProbeCommand=function()log[#log+1]='direct'end,
            ProbeMessageCommand=function()log[#log+1]='message'end}
        "#,
        r#"
        assert(root:GetCommand('Probe')==root.ProbeCommand)
        root:playcommand('Probe')
        root.ProbeCommand=function(self,...) log[#log+1]='new:'..select('#',...)..':'..tostring((...)) end
        collectgarbage();root:playcommand('Probe');root:playcommand('Probe',nil);root:playcommand('Probe',false)
        root.ProbeCommand=nil;assert(root:GetCommand('Probe')==root.ProbeMessageCommand);root:playcommand('Probe')
        root.ProbeMessageCommand=nil;assert(root:GetCommand('Probe')==nil);root:playcommand('Probe')
        local fn=function(self,p) log[#log+1]='added:'..tostring(p) end
        assert(root:addcommand('PingMessage',fn,'ignored')==root)
        assert(root:GetCommand('Ping')==fn and root:GetCommand('PingMessage')==fn and root.PingMessageCommand==fn)
        root:playcommand('Ping',23)
        assert(not pcall(root.addcommand,root,'Bad',23))
        root.BadCommand=false;assert(not pcall(root.GetCommand,root,'Bad'));assert(not pcall(root.playcommand,root,'Bad'))
        assert(root:GetCommand(false)==nil and root:GetCommand()==nil and root:playcommand({})==root)
        root:addcommand(123,function()error('numeric command names are ignored by lookup')end)
        assert(root:GetCommand(123)==nil and root:GetCommand('123')==root['123Command'])
        assert(root:playcommand(123)==root and root:GetCommand(string.char(255))==nil)
        root.LoopCommand=function(self)
            log[#log+1]='loop';self:playcommand('Loop');self.LoopCommand=function()log[#log+1]='replacement'end
            self:playcommand('Loop');collectgarbage()
        end
        root:playcommand('Loop');root:playcommand('Loop')
        root.FailCommand=function()log[#log+1]='failure';error('authored failure')end
        assert(not pcall(root.playcommand,root,'Fail'));assert(not pcall(root.playcommand,root,'Fail'))
        root.FailCommand=function()log[#log+1]='recovered'end;root:playcommand('Fail')
        assert(root.removecommand==nil)
        return table.concat(log,'|')
        "#,
    );
}

#[test]
fn production_command_walk_order_handlers_and_callback_errors() {
    compare_scene(
        r#"
        log={};params={}
        local function own(self,p) assert(p==params);log[#log+1]=self:GetName()end
        return Def.ActorFrame{Name='root',ProbeCommand=own,
            Def.ActorFrame{Name='branch',ProbeCommand=own,Def.Quad{Name='leaf',ProbeCommand=own}},
            Def.ActorFrame{Name='empty',ProbeCommand=own},
            Def.Quad{Name='first',InitCommand=function(self)self:draworder(-1)end,ProbeCommand=own}}
        "#,
        r#"
        root:playcommand('Probe',params);log[#log+1]='/'
        root:playcommandonchildren('Probe',params);log[#log+1]='/'
        root:playcommandonleaves('Probe',params);log[#log+1]='/'
        root:propagatecommand('Probe',params);log[#log+1]='/'
        root:propagate(0):playcommand('Probe',params);log[#log+1]='/'
        root:propagate(false):playcommand('Probe',params);log[#log+1]='/'
        local branch=root:GetChild('branch');branch:propagate(true):playcommand('Probe',params);log[#log+1]='/'
        root:RunCommandsOnChildren(function(self,p)assert(p==params);log[#log+1]=self:GetName()end,params)
        log[#log+1]='/'
        root:runcommandsonleaves(function(self,...)assert(select('#',...)==0);log[#log+1]=self:GetName()end,params)
        log[#log+1]='/'
        assert(root:RunCommandsOnChildren(false)==root and root:runcommandsonleaves(false)==root)
        assert(not pcall(root.RunCommandsRecursively,root,function()error('should not run')end))
        assert(not pcall(root.RunCommandsOnChildren,root,function(self)log[#log+1]='failed:'..self:GetName();error('stop')end))
        assert(not pcall(root.runcommandsonleaves,root,function(self)log[#log+1]='leaf-failed:'..self:GetName();error('stop')end))
        return table.concat(log,'|')
        "#,
    );
}

#[test]
fn production_child_groups_class_hooks_and_visibility_coercions() {
    compare_scene(
        "return Def.ActorFrame{Def.Quad{Name='same'},Def.Quad{Name='same'}}",
        r#"
        local g=root:GetChild('same');assert(#g==2 and g~=root:GetChild('same'))
        assert(getmetatable(g[1])~=getmetatable(g[2]))
        local log={}
        assert(g:x(17)==g[2] and g[1]:GetX()==0 and g[2]:GetX()==17)
        local getx=g.GetX;g[3]=g[1];assert(getx(g)==0 and g['3']==nil)
        g[2].GetX=function()return 999 end;assert(root:GetChild('same'):GetX()==999)
        Actor.multi=function(self,...)return self,...end
        local t={};local a,b,c,d=root:GetChildren().same:multi(t,nil,42)
        assert(a==g[2] and b==t and c==nil and d==42)
        for _,v in ipairs({false,0,0.5,'1','no',' on ','off','bad',true,{}}) do
            g[1]:visible(v);log[#log+1]=tostring(g[1]:GetVisible())
        end
        local old=Sprite
        Sprite=setmetatable({}, {__index=function(_,key)
            if key=='Marker' then return 42 end
            if key=='HookCommand' then return function(self,p)log[#log+1]='hook:'..tostring(p)end end
            if key=='ErrorCommand' then error('lookup failure')end
            return old[key]
        end})
        assert(g[1].Marker==42 and g[1][17]==nil)
        g[1]:playcommand('Hook','parameter');assert(not pcall(g[1].GetCommand,g[1],'Error'))
        Sprite=old;g[1]:name('changed');assert(root:GetChild('changed')==g[1] and root:GetChild('same')==g[2])
        return table.concat(log,'|')
        "#,
    );
}

#[test]
fn scalar_methods_preserve_native_float_rounding_and_lua_fields_do_not_alias_state() {
    let mut r = runtime();
    let scene = instantiate(&mut r, "return Def.Quad{}");
    execute(
        &mut r,
        r#"
        assert(root:x('16777217')==root and root:GetX()==16777216)
        root:addx(1); assert(root:GetX()==16777216)
        root:xy(0.1,0.2):z(-3):zoom(2):zoomy(-4):rotationz(12):addrotationz(0.5):setsize(5,6):aux(7)
        assert(root:GetZoomedWidth()==10 and root:GetZoomedHeight()==-24 and root:GetRotationZ()==12.5 and root:getaux()==7)
        root.x='authored';assert(root:GetX()>0 and root.x=='authored');Actor.x(root,3);assert(root:GetX()==3)
        root:visible(0.5);assert(root:GetVisible());root:visible(false);assert(not root:GetVisible())
        assert(pcall(root.visible,root,'1') and root:GetVisible())
        root:draworder(16777217)
    "#,
    );
    let s = &r.actor_scene_snapshot(scene).unwrap()[0];
    assert_eq!(s.draw_order, 16777217);
    assert_eq!(s.position[0], 3.);
    assert_clean(&r);
}

#[test]
fn production_callback_directories_restore_across_nested_calls_errors_and_gc() {
    compare_scene(
        r#"
        log={}
        local function record()log[#log+1]=tostring(__songlua_script_dir)end
        return Def.ActorFrame{OuterCommand=function(self)
            record();local child=self:GetChild('child');child:playcommand('Probe');record()
            assert(not pcall(child.playcommand,child,'Fail'));record()
        end,ProbeCommand=record,FailCommand=function()record();error('root failure')end,
            Def.Quad{Name='child',ProbeCommand=function()record();collectgarbage()end,
                FailCommand=function()record();error('child failure')end}}
        "#,
        r#"
        root.__songlua_script_dir='/outer/';root:GetChild('child').__songlua_script_dir='/inner/'
        __songlua_script_dir='caller';root:playcommand('Outer');assert(__songlua_script_dir=='caller')
        assert(not pcall(root.playcommand,root,'Fail'));assert(__songlua_script_dir=='caller')
        root:RunCommandsOnChildren(function()log[#log+1]=__songlua_script_dir end)
        root.__songlua_script_dir=' ';root:playcommand('Probe');assert(__songlua_script_dir=='caller')
        local prior={};local weak=setmetatable({prior},{__mode='v'})
        __songlua_script_dir=prior;prior=nil;root.__songlua_script_dir='/outer/'
        root:playcommand('Outer');assert(__songlua_script_dir==weak[1] and weak[1]~=nil)
        root.__songlua_script_dir=false;assert(not pcall(root.playcommand,root,'Probe'))
        assert(__songlua_script_dir==weak[1])
        return table.concat(log,'|')
        "#,
    );
}

#[test]
fn production_scalar_defaults_coercions_nonfinite_values_and_size_signs() {
    for class in ["Actor", "ActorFrame", "Quad"] {
        compare_scene(
            &format!("return Def.{class}{{}}"),
            r#"
            return root:GetX(),root:GetY(),root:GetZ(),root:GetZoom(),root:GetZoomY(),root:GetZoomZ(),
                root:GetRotationX(),root:GetRotationY(),root:GetRotationZ(),root:GetWidth(),root:GetHeight(),
                root:GetVisible(),root:getaux()
        "#,
        );
    }
    compare_scene(
        "return Def.Quad{}",
        r#"
        local log={}
        local function number(x)
            if x~=x then return 'nan' end
            if x==math.huge then return 'inf' end
            if x==-math.huge then return '-inf' end
            if x==0 and 1/x<0 then return '-0' end
            return string.format('%.9g',x)
        end
        local function record()
            local values={root:GetX(),root:GetY(),root:GetZ(),root:GetZoomX(),root:GetZoomY(),root:GetZoomZ(),
                root:GetRotationX(),root:GetRotationY(),root:GetRotationZ(),root:GetWidth(),root:GetHeight(),
                root:GetZoomedWidth(),root:GetZoomedHeight(),root:getaux()}
            for i,v in ipairs(values) do values[i]=number(v) end
            log[#log+1]=table.concat(values,',')
        end
        local inputs={{},{false},{true},{'bad'},{'0x10'},{' 1.25 '},{-3.5},{0/0},{math.huge},{1e100},{'nan'},{'inf'},{string.char(255)},{ {}}}
        for _,method in ipairs({'x','y','z','addx','addy','addz','zoom','zoomx','zoomy','zoomz',
            'rotationx','rotationy','rotationz','addrotationx','addrotationy','addrotationz','SetWidth','SetHeight','aux'}) do
            for _,input in ipairs(inputs) do
                root:x(1):y(2):z(3):zoom(2):rotationx(4):rotationy(5):rotationz(6):setsize(7,8):aux(9)
                local ok=pcall(root[method],root,unpack(input));log[#log+1]=method..':'..tostring(ok);record()
            end
        end
        root:xy(false,17);record();root:xy(19,false);record()
        root:setsize(-5,-6);record();root:SetWidth(-9);record();root:SetHeight(-10);record()
        root:setsize(4,false);record();root:setsize(false,4);record()
        return table.concat(log,'|')
    "#,
    );
}

#[test]
fn production_draw_order_rounding_and_invalid_arguments_preserve_stable_sort() {
    compare_scene(
        r#"
        return Def.ActorFrame{Def.Quad{Name='first'},Def.Quad{Name='second'}}
    "#,
        r#"
        local first,second=root:GetChild('first'),root:GetChild('second')
        local log={}
        for _,input in ipairs({{}, {false}, {'bad'}, {0.5}, {1.5}, {-0.5}, {'0.5'}, {'1.5'}, {'nan'}, {'inf'}, {math.huge}}) do
            first:draworder(-10);second:draworder(0);root:SortByDrawOrder()
            first:draworder(0);local ok=pcall(first.draworder,first,unpack(input));log[#log+1]=tostring(ok)
            root:SortByDrawOrder();root:RunCommandsOnChildren(function(self)log[#log+1]=self:GetName()end)
        end
        return table.concat(log,'|')
    "#,
    );
}

#[test]
fn actor_strings_preserve_public_class_and_stable_distinct_identity() {
    compare_scene(
        "return Def.ActorFrame{Def.Quad{},Def.Quad{},Def.Actor{}}",
        r#"
        local a,b,plain=root[1],root[2],root[3]
        local first=tostring(a)
        assert(first:match('^Sprite %(') and first:match('%)$'))
        assert(first == tostring(a) and first ~= tostring(b))
        local wrapper = a:AddWrapperState():GetWrapperState(1)
        local function class(actor) return tostring(actor):match('^(.-) %(') end
        collectgarbage('collect')
        assert(tostring(a) == first)
        return class(root),class(a),class(plain),class(wrapper)
        "#,
    );
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
