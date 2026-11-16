use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, {ActorScene, SongRuntime},
};

fn runtime() -> SongRuntime {
    SongRuntime::new(&SongLuaCompileContext::new(
        std::path::Path::new("."),
        "message migration tests",
    ))
    .unwrap()
}
fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/messages.lua").unwrap())
        .unwrap()
        .to_vec()
}
fn instantiate(runtime: &mut SongRuntime, source: &str) -> ActorScene {
    let definition = execute(runtime, source)[0];
    let scene = runtime.instantiate_actor(definition).unwrap();
    let root = runtime.actor_scene_root(scene).unwrap();
    runtime.vm_mut().set_global(b"root", root).unwrap();
    scene
}

struct Pair {
    rust: SongRuntime,
    scene: ActorScene,
    seconds: f64,
}
impl Pair {
    fn new(definition: &str) -> Self {
        let source = format!(
            r#"
            seen={{}}
            function log(...) for i=1,select('#',...) do
                local v=select(i,...)
                seen[#seen+1]=type(v)=='number' and string.format('%.17g',v) or tostring(v)
            end end
            function observe() return table.concat(seen,'|') end
            {definition}
        "#
        );
        let mut rust = runtime();
        let scene = instantiate(&mut rust, &source);
        // Preserve the reference fixture's relative delivery order with dense
        // ranks. No address from the reference process enters the Rust VM.
        let identities = reference_test::source_indices(
            "song/tests/rust_messages.rs::Pair::new::ranks",
            &source,
        );
        let mut ordered = identities.clone();
        ordered.sort_unstable();
        assert_eq!(ordered, (1..=identities.len()).collect::<Vec<_>>());
        assert_eq!(
            identities.len(),
            rust.actor_scene_snapshot(scene).unwrap().len() + 1
        );
        let null = rust.vm_mut().get_global(b"NullActor").unwrap();
        assert!(rust.set_actor_message_identity(null, identities[0]));
        assert!(rust.set_actor_message_ranks(scene, &identities[1..]));
        Self {
            rust,
            scene,
            seconds: 0.0,
        }
    }
    fn assert_observations(&mut self) {
        let Value::String(actual) = execute(&mut self.rust, "return observe()")[0] else {
            panic!("observation string");
        };
        let actual = std::str::from_utf8(self.rust.vm_mut().string_bytes(actual).unwrap()).unwrap();
        let actual: Vec<_> = actual.split('|').collect();
        reference_test::assert_debug(
            "song/tests/rust_messages.rs::Pair::observations",
            &(self.seconds.to_bits(), &actual),
        );
    }
    fn script(&mut self, source: &str) {
        execute(&mut self.rust, source);
        self.assert_observations();
    }
    fn advance(&mut self, delta: f64) {
        self.seconds += delta;
        assert!(self.rust.advance_actor_scene(self.scene, delta).unwrap());
        self.assert_observations();
    }
}

#[test]
fn broadcast_arity_identity_dynamic_fields_and_manager_call_styles() {
    let mut pair = Pair::new(
        r#"
        local function receive(self,...) local p=...;log(self:GetName(),select('#',...),p==shared,__songlua_active_broadcast) end
        return Def.ActorFrame{Name='root',ProbeMessageCommand=receive,
            Def.Quad{Name='child',ProbeMessageCommand=receive},Def.Actor{Name='plain'}}
    "#,
    );
    pair.script(r#"
        shared={value=4};__songlua_active_broadcast='outer';__songlua_broadcast_messages={}
        assert(MESSAGEMAN:Broadcast('Probe',shared)==MESSAGEMAN)
        assert(MESSAGEMAN.Broadcast('Probe')==MESSAGEMAN)
        assert(MESSAGEMAN.Broadcast({},'Probe',nil)==MESSAGEMAN)
        assert(__songlua_active_broadcast=='outer')
        root:GetChild('plain'):addcommand('ProbeMessage',function(self,p) log('added',p==shared)end)
        root.ProbeMessageCommand=nil
        MESSAGEMAN:Broadcast('Probe',shared)
        for _,entry in ipairs(__songlua_broadcast_messages) do log(entry.message,entry.has_params) end
        assert(MESSAGEMAN:SetLogging(false)==MESSAGEMAN)
        log(__songlua_side_effect_count,root.SubscribeToMessage==nil,root.UnsubscribeFromMessage==nil)
        local saved=MESSAGEMAN;MESSAGEMAN={};assert(saved.Broadcast('Probe',shared)==saved);MESSAGEMAN=saved
    "#);
}

#[test]
fn same_message_reentry_preserves_direct_command_guard_and_scopes() {
    let mut pair = Pair::new(
        r#"
        local function receive(self,p)
            log('enter',self:GetName(),p.depth,__songlua_active_broadcast)
            self:playcommand('Pulse',p)
            if p.depth==0 then MESSAGEMAN:Broadcast('Pulse',{depth=1}) end
            log('leave',self:GetName(),p.depth,__songlua_active_broadcast)
        end
        return Def.ActorFrame{Name='root',PulseMessageCommand=receive,
            Def.Quad{Name='child',PulseMessageCommand=function(self,p) log('child',p.depth) end}}
    "#,
    );
    pair.script("__songlua_active_broadcast='outside';MESSAGEMAN:Broadcast('Pulse',{depth=0});assert(__songlua_active_broadcast=='outside')");
}

#[test]
fn listener_snapshot_reloads_handlers_and_defers_new_subscribers() {
    let mut pair = Pair::new(
        r#"
        local function receive(self,p)
            log('old',self:GetName())
            if not changed then
                changed=true
                local top=self:GetParent() or self
                top.PulseMessageCommand=function(a)log('new',a:GetName())end
                top:GetChild('child').PulseMessageCommand=function(a)log('new',a:GetName())end
                top:GetChild('late').PulseMessageCommand=function(a)log('late',a:GetName())end
                collectgarbage('collect')
            end
        end
        return Def.ActorFrame{Name='root',PulseMessageCommand=receive,
            Def.Quad{Name='child',PulseMessageCommand=receive},Def.Quad{Name='late'}}
    "#,
    );
    pair.script("MESSAGEMAN:Broadcast('Pulse');log('next');MESSAGEMAN:Broadcast('Pulse')");
}

#[test]
fn broadcast_validation_suppression_history_and_logging_match_production() {
    let mut pair = Pair::new(
        "return Def.Quad{ProbeMessageCommand=function(self,...)log('probe',select('#',...))end}",
    );
    pair.script(r#"
        __songlua_broadcast_messages={}
        for _,bad in ipairs{false,3,'bad',function()end} do
            log((pcall(function()MESSAGEMAN:Broadcast('Probe',bad)end)))
        end
        for _,name in ipairs{false,8,{},function()end,string.char(255)} do
            log(MESSAGEMAN:Broadcast(name,false)==MESSAGEMAN)
        end
        MESSAGEMAN:Broadcast('');MESSAGEMAN:Broadcast('  ')
        __songlua_suppress_broadcast_dispatch=true
        MESSAGEMAN:Broadcast('Probe');MESSAGEMAN:Broadcast('Probe',nil);MESSAGEMAN:Broadcast('Probe',{})
        __songlua_suppress_broadcast_dispatch=false
        MESSAGEMAN:Broadcast('Probe');MESSAGEMAN:SetLogging()
        log(__songlua_side_effect_count)
        for _,entry in ipairs(__songlua_broadcast_messages) do log(entry.message,entry.has_params) end
    "#);
}

#[test]
fn queued_messages_use_empty_shared_params_and_frame_boundaries() {
    let mut pair = Pair::new(
        r#"
        local function receive(self,p) log(self:GetName(),type(p),next(p)==nil,__songlua_active_broadcast);p[self:GetName()]=true end
        return Def.ActorFrame{Name='root',PulseMessageCommand=receive,
            Def.Quad{Name='child',PulseMessageCommand=receive}}
    "#,
    );
    pair.script("__songlua_broadcast_messages={};root:sleep(0.5):queuemessage('Pulse'):queuecommand('!Pulse');__songlua_suppress_broadcast_dispatch=true");
    pair.advance(0.0);
    pair.advance(0.5);
    pair.advance(0.0);
    pair.advance(0.125);
    pair.script("log(__songlua_side_effect_count);for _,e in ipairs(__songlua_broadcast_messages)do log(e.message,e.has_params)end");
}

#[test]
fn authored_and_theme_message_errors_restore_state_and_recover() {
    let mut pair = Pair::new(
        "return Def.Quad{ProbeMessageCommand=function(self)self:addx(1);error('authored')end}",
    );
    pair.script(r#"
        __songlua_active_broadcast='outer';__songlua_script_dir='outside';root.__songlua_script_dir='/fixture/nested/'
        log((pcall(function() MESSAGEMAN:Broadcast('Probe') end)))
        log(root:GetX(),__songlua_active_broadcast,__songlua_script_dir)
        root.ProbeMessageCommand=assert(loadstring("return function(self)self:addx(2);error('theme failure')end",'@theme:message'))()
        log((pcall(function()MESSAGEMAN:Broadcast('Probe')end)))
        root.ProbeMessageCommand=function(self)self:addx(4)end;MESSAGEMAN:Broadcast('Probe')
        log(root:GetX(),__songlua_active_broadcast,__songlua_script_dir)
    "#);
    assert_eq!(pair.rust.take_actor_diagnostics().len(), 1);
}

#[test]
fn judgment_normalization_retains_note_and_result_objects_across_gc() {
    let mut pair = Pair::new(
        r#"
        return Def.Quad{JudgmentMessageCommand=function(self,p)
            assert(p==shared);retained=p.Notes;log(p.Player,p.TapNoteScore,p.HoldNoteScore,p.TapNoteOffset)
            local keys={};for k in pairs(p.Notes)do keys[#keys+1]=k end;table.sort(keys)
            for _,k in ipairs(keys)do local n=p.Notes[k];local r=n:GetTapNoteResult()
                log(k,n:GetTapNoteType(),n:GetTapNoteSource(),n:GetTapNoteSubType(),n:GetPlayerNumber(),
                    n:GetAttackModifiers(),n:GetHoldDuration(),n:GetAttackDuration(),n:GetKeysoundIndex(),
                    r==n:GetHoldNoteResult(),r:GetHeld(),r:GetHidden(),r:GetTapNoteOffset(),r:GetTapNoteScore())
            end
        end}
    "#,
    );
    pair.script(r#"
        shared={Column=2};MESSAGEMAN:Broadcast('Judgment',shared)
        shared={Player='PlayerNumber_P2',TapNoteScore='TapNoteScore_W1',TapNoteOffset=0.017,
            Notes={[1]='TapNoteType_Mine',[4]={TapNoteType='TapNoteType_HoldHead',Source='custom',SubType='roll',
                Held='yes',Hidden='0',Offset='0.1',Score='specific',HoldDuration='1.25',AttackDuration=0.2,
                AttackModifiers='dizzy',KeysoundIndex='2.6',TapNoteResult={}}}}
        MESSAGEMAN:Broadcast('Judgment',shared)
        result=retained[4]:GetTapNoteResult();note=retained[4];note.TapNoteResult=nil;note.HoldNoteResult=nil
        shared=nil;retained=nil;collectgarbage('collect')
        log(note:GetTapNoteResult()==result,note:GetHoldNoteResult()==result,result:GetHeld(),result:GetTapNoteScore())
        note.Held=false;note.TapNoteType='changed';log(result:GetHeld(),note:GetTapNoteType())
        shared={Notes={}};MESSAGEMAN:Broadcast('Judgment',shared)
    "#);
    assert!(pair.rust.release_actor_scene(pair.scene).unwrap());
    execute(
        &mut pair.rust,
        "root=nil;shared=nil;retained=nil;collectgarbage('collect');assert(note:GetTapNoteResult()==result and result:GetHeld()==true)",
    );
}

#[test]
fn native_messages_preserve_engine_params_and_omitted_arguments() {
    let mut pair = Pair::new(
        r#"return Def.Quad{JudgmentMessageCommand=function(self,...)
        local p=...;log(select('#',...),p and p.Player,p and p.Notes)
    end}"#,
    );
    let rust_params = Value::Table(pair.rust.vm_mut().try_new_table().unwrap());
    pair.rust
        .vm_mut()
        .set_global(b"engine_params", rust_params)
        .unwrap();
    pair.rust
        .broadcast_native_message("Judgment", Some(rust_params))
        .unwrap();
    pair.assert_observations();
    for present in [false, true] {
        pair.rust
            .broadcast_native_message("Judgment", present.then_some(Value::Nil))
            .unwrap();
        pair.assert_observations();
    }
    pair.script("assert(next(engine_params)==nil)");
}

#[test]
fn natural_delivery_uses_vm_identities_and_release_removes_subscribers() {
    let mut rust = runtime();
    let scene = instantiate(
        &mut rust,
        r#"seen={};local function receive(self)seen[#seen+1]=self end
        return Def.ActorFrame{PulseMessageCommand=receive,Def.Quad{PulseMessageCommand=receive},Def.Actor{PulseMessageCommand=receive}}"#,
    );
    let Value::Table(root) = rust.actor_scene_root(scene).unwrap() else {
        panic!("actor root")
    };
    let children = [
        Value::Table(root),
        rust.vm_mut()
            .raw_table_get(root, Value::Number(1.0))
            .unwrap(),
        rust.vm_mut()
            .raw_table_get(root, Value::Number(2.0))
            .unwrap(),
    ];
    let mut expected = children.map(|actor| (rust.vm_mut().object_pointer(actor).unwrap(), actor));
    expected.sort_by_key(|&(identity, _)| identity);
    execute(
        &mut rust,
        "collectgarbage('collect');MESSAGEMAN:Broadcast('Pulse')",
    );
    let Value::Table(seen) = rust.vm_mut().get_global(b"seen").unwrap() else {
        panic!("seen")
    };
    for (index, &(_, actor)) in expected.iter().enumerate() {
        assert_eq!(
            rust.vm_mut()
                .raw_table_get(seen, Value::Number((index + 1) as f64))
                .unwrap(),
            actor
        );
    }
    assert!(rust.release_actor_scene(scene).unwrap());
    execute(
        &mut rust,
        "seen={};collectgarbage('collect');MESSAGEMAN:Broadcast('Pulse');assert(#seen==0)",
    );
}

#[test]
fn null_actor_is_an_independent_live_template_and_subscriber() {
    let mut pair = Pair::new(
        r#"
        assert(NullActor:getaux()==0)
        NullActor:addcommand('PulseMessage',function(self)log('pulse',self==NullActor,self:getaux())end)
        return Def.ActorFrame{NullActor,NullActor,InitCommand=function(self)
            self[1]:aux(4);self[2]:aux(7);assert(self[1]~=self[2] and self[1]~=NullActor)
        end}
    "#,
    );
    pair.script(
        "MESSAGEMAN:Broadcast('Pulse');assert(NullActor:getaux()==0);collectgarbage('collect')",
    );
    assert!(pair.rust.release_actor_scene(pair.scene).unwrap());
    execute(
        &mut pair.rust,
        "seen={};MESSAGEMAN:Broadcast('Pulse');assert(observe()=='pulse|true|0')",
    );
}

#[test]
fn judgment_metamethods_gc_and_existing_native_objects_match_production() {
    let mut pair = Pair::new(
        r#"return Def.Quad{JudgmentMessageCommand=function(self,p)
        log('received',p.Notes[1]:GetTapNoteResult()==p.Notes[1]:GetHoldNoteResult())
    end}"#,
    );
    pair.script(r#"
        local function metatable(label)
            return {__index=function(t,k)log(label..'.get',k)end,
                __newindex=function(t,k,v)log(label..'.set',k);rawset(t,k,v);collectgarbage('collect')end}
        end
        local result=setmetatable({},metatable('result'))
        local note=setmetatable({TapNoteResult=result},metatable('note'))
        local params=setmetatable({Notes={[1]=note}},metatable('params'))
        MESSAGEMAN:Broadcast('Judgment',params)
        assert(note:GetTapNoteResult()==result)
        local before=#seen;MESSAGEMAN:Broadcast('Judgment',params);log('repeat',#seen-before)
        local native={GetTapNoteType=false}
        params.Notes[1]=native
        root.JudgmentMessageCommand=function(self,p)log('native',p.Notes[1]==native,p.Notes[1].TapNoteResult==nil)end
        MESSAGEMAN:Broadcast('Judgment',params)
    "#);
}

#[test]
fn null_actor_instances_copy_template_native_state_without_advancing_template() {
    let mut pair = Pair::new(
        r#"
        NullActor:x(3):aux(9):zoom(2):setsize(5,6):visible(false):hibernate(0.25):linear(2):x(11)
        function sample()
            for _,a in ipairs{NullActor,root[1],root[2]}do
                log(a:GetX(),a:GetDestX(),a:getaux(),a:GetZoomX(),a:GetWidth(),a:GetHeight(),a:GetVisible(),a:GetTweenTimeLeft())
            end
        end
        return Def.ActorFrame{NullActor,NullActor}
    "#,
    );
    pair.script("sample()");
    pair.advance(0.125);
    pair.script("sample()");
    pair.advance(0.25);
    pair.script("sample();root[1]:aux(18);assert(root[2]:getaux()==9 and NullActor:getaux()==9)");
}

#[test]
fn fatal_broadcast_restores_scopes_and_guard_after_gc() {
    let mut rust = runtime();
    instantiate(
        &mut rust,
        r#"__songlua_active_broadcast='outer';__songlua_script_dir='outside';return Def.Quad{
        _Dir='/fixture/inside/',PulseMessageCommand=function(self)self:x(3);while true do end end}"#,
    );
    rust.vm_mut().set_instruction_limit(Some(400));
    let error = rust.broadcast_message("Pulse", None).unwrap_err();
    assert_eq!(error.kind(), deadlib_lua::ErrorKind::Limit);
    rust.vm_mut().set_instruction_limit(None);
    execute(
        &mut rust,
        "assert(__songlua_active_broadcast=='outer' and __songlua_script_dir=='outside');root.PulseMessageCommand=function(self)self:addx(7)end;collectgarbage('collect');MESSAGEMAN:Broadcast('Pulse');assert(root:GetX()==10)",
    );
    assert!(rust.take_actor_diagnostics().is_empty());
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
