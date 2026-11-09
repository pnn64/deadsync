use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, {ActorScene, SongRuntime},
};

fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/meters.lua").unwrap())
        .unwrap_or_else(|e| panic!("{source}\n{e:?}"))
        .to_vec()
}
struct Pair {
    rust: SongRuntime,
    scene: ActorScene,
    seconds: f32,
}
impl Pair {
    fn new(live: bool, meter: &str) -> Self {
        let mut context = SongLuaCompileContext::new(std::path::Path::new("."), "meter migration");
        context.live = live;
        let source = format!(
            r#"
            seen={{}}
            function log(...) for i=1,select('#',...) do
                local v=select(i,...);seen[#seen+1]=type(v)=='number' and string.format('%.9g',v) or tostring(v)
            end end
            return Def.ActorFrame{{{meter}, Def.ActorFrame{{Name='other',InitCommand=function(self)other=self end}}}}
        "#
        );

        let mut rust = SongRuntime::new(&context).unwrap();
        let definition = execute(&mut rust, &source)[0];
        let scene = rust.instantiate_actor(definition).unwrap();
        let root = rust.actor_scene_root(scene).unwrap();
        rust.vm_mut().set_global(b"root", root).unwrap();
        let mut p = Self {
            rust,
            scene,
            seconds: 0.0,
        };
        p.compare("construction", false);
        p
    }
    fn compare(&mut self, phase: &str, error: bool) {
        let actor = self.rust.vm_mut().get_global(b"m").unwrap();
        let actual = self.rust.actor_song_meter_display(actor);
        if error {
            assert!(actual.is_err(), "{phase}: Rust");
        } else {
            reference_test::assert_debug(
                "song/tests/rust_song_meters.rs::payload",
                &(phase, &actual.unwrap()),
            );
        }
        let source = "return table.concat(seen,'|')";
        let Value::String(actual) = execute(&mut self.rust, source)[0] else {
            panic!("observations")
        };
        reference_test::assert_debug(
            "song/tests/rust_song_meters.rs::observations",
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
const METER: &str = r#"Def.SongMeterDisplay{
    Name='meter', StreamWidth='120',
    Stream=Def.ActorFrame{Name='authored-stream',
        Def.Quad{Name='tip',InitCommand=function(self)log('tip init',self:GetParent():GetName())end},
        InitCommand=function(self)log('stream init',self:GetName(),self:GetParent():GetName())end},
    Def.Actor{Name='authored',InitCommand=function(self)log('authored init',self:GetParent():GetName())end},
    InitCommand=function(self)
        m=self;s=self.Stream;a=self:GetChild('authored')
        log('meter init',self:GetName(),s:GetName(),self:GetStreamWidth(),self:GetNumChildren())
        assert(self[1]==a and self[2]==nil and self:GetChild('Stream')==s)
    end}"#;

#[test]
fn construction_private_binding_and_stream_aliases() {
    for live in [false, true] {
        let mut p = Pair::new(live, METER);
        p.script(
            r#"
            assert(SongMeterDisplay==nil and m.SetUpdateFunction~=nil)
            assert(s:GetName()=='Stream' and s:GetParent()==m)
            s:name('renamed');assert(m:GetChild('Stream')==s and m:GetChild('renamed')==nil)
            local children=m:GetChildren();children.Stream=nil;assert(m:GetChild('Stream')==s)
            m.__songlua_children.alias=s;assert(m:GetNumChildren()==2)
            local f=m.SetStreamWidth;assert(f({},'25.5')==m and m:GetStreamWidth()==25.5)
            local previous=ActorFrame.SetStreamWidth
            ActorFrame.SetStreamWidth=function(self)assert(self==m);return 'override'end
            assert(m:SetStreamWidth(99)=='override');ActorFrame.SetStreamWidth=previous
            f(false);f('invalid');f();assert(m:GetStreamWidth()==25.5)
            local saved=m.SetStreamWidth;m.__songlua_actor_type='Actor';saved(m,99)
            m.__songlua_actor_type='SongMeterDisplay';assert(m:GetStreamWidth()==25.5)
        "#,
        );
    }
}

#[test]
fn absent_streams_width_coercion_and_render_thresholds() {
    let mut absent = Pair::new(
        true,
        "Def.SongMeterDisplay{StreamWidth=50,InitCommand=function(self)m=self end}",
    );
    absent.script("assert(m:GetNumChildren()==0);m:SetStreamWidth(100)");
    let mut p = Pair::new(true, METER);
    for v in [
        "0",
        "-5",
        "1e-8",
        "1e-6",
        "'0x10'",
        "'nan'",
        "'inf'",
        "math.huge",
        "0/0",
    ] {
        p.script(&format!("m:SetStreamWidth({v});log(m:GetStreamWidth())"));
    }
    p.script("m.__songlua_stream_width=nil;m.StreamWidth='0.75';assert(m:GetStreamWidth()==0)");
    p.script("m.__songlua_stream_width='0x20';s:xy(7,9):zoom(.5):diffusealpha(.25)");
}

#[test]
fn stream_startup_frame_updates_and_finish_keep_distinct_traversals() {
    let mut p = Pair::new(true, METER);
    p.script(
        r#"
        for _,actor in ipairs{m,s,a} do
            local name=actor==s and 'stream' or actor:GetName()
            for _,cmd in ipairs{'Begin','On','End','Probe'} do
                local command=cmd;actor:addcommand(cmd,function(self)log(command,name)end)
            end
        end
        m:playcommandonchildren('Probe')
        s:linear(1):x(20);a:linear(1):x(40)
        s:SetUpdateRate(.5);m:SetUpdateRate(2)
        s:SetUpdateFunction(function(self,dt)log('stream update',dt,self:GetX())end)
        m:SetUpdateFunction(function(self,dt)log('meter update',dt,s:GetX(),a:GetX())end)
    "#,
    );

    p.rust.start_actor_scene(p.scene).unwrap();
    p.compare("start", false);
    p.advance(0.25);
    p.script(
        "m:hurrytweening(.5);log(s:GetTweenTimeLeft(),a:GetTweenTimeLeft(),m:GetTweenTimeLeft())",
    );
    p.advance(0.125);
    p.script("m:finishtweening();log(s:GetTweenTimeLeft(),a:GetTweenTimeLeft());m:zwrite(true)");

    p.rust.end_actor_scene(p.scene).unwrap();
    p.compare("end", false);
}

#[test]
fn stream_replacement_obeys_cached_update_plan_and_invalidation() {
    let mut p = Pair::new(true, METER);
    p.script("s:SetUpdateFunction(function(self,dt)log('original',dt,self:GetParent():GetName())end);other:SetUpdateFunction(function(self,dt)log('other',dt,self:GetParent():GetName())end)");
    p.advance(0.125);
    p.script("m.__songlua_children.Stream=other;assert(m:GetChild('Stream')==other)");
    p.advance(0.125);
    p.script("m:SetUpdateRate(2)");
    p.advance(0.125);
    p.script("m.__songlua_children.Stream=s;m:SetUpdateRate(1)");
    p.advance(0.125);
}

#[test]
fn getters_reenter_collect_and_failed_reads_do_not_publish_stale_payloads() {
    let mut p = Pair::new(true, METER);
    p.script(r#"
        local original=getmetatable(m).__index
        m.__songlua_stream_width=nil
        getmetatable(m).__index=function(self,key)
            if key=='__songlua_stream_width' then collectgarbage('collect');s:x(12);return '30.5' end
            return original(self,key)
        end
        assert(m:GetStreamWidth()==30.5)
    "#);
    for source in [
        "m.__songlua_stream_width=false",
        "m.__songlua_stream_width=40;m.__songlua_children.Stream=false",
    ] {
        execute(&mut p.rust, source);
        p.compare(source, true);
    }
    p.script("m.__songlua_children.Stream=s;m.__songlua_stream_width=40");
    let m = p.rust.vm_mut().get_global(b"m").unwrap();
    let held = p.rust.actor_song_meter_display(m).unwrap().unwrap();
    p.script("weak=setmetatable({s},{__mode='v'});retained=m.SetStreamWidth;s:x(99);collectgarbage('collect')");
    p.rust.release_actor_scene(p.scene).unwrap();
    execute(
        &mut p.rust,
        "assert(not pcall(retained,m,5));root=nil;m=nil;s=nil;a=nil;other=nil;retained=nil;collectgarbage('collect');collectgarbage('collect');assert(weak[1]==nil)",
    );
    assert_eq!(held.stream_state.x, 12.0);
    assert!(p.rust.actor_song_meter_display(m).is_err());
}

#[test]
fn malformed_stream_declarations_fail_without_retaining_partial_scenes() {
    for stream in ["false", "42", "'bad'", "Def.UnknownClass{}"] {
        let source = format!("return Def.SongMeterDisplay{{Stream={stream}}}");

        let context = SongLuaCompileContext::new(std::path::Path::new("."), "invalid stream");
        let mut rust = SongRuntime::new(&context).unwrap();
        // Unknown declarations may fail while evaluating Def itself.
        let program = compile_named(source.as_bytes(), b"=invalid stream").unwrap();
        if let Ok(values) = rust.vm_mut().execute(&program) {
            let definition = values[0];
            assert!(rust.instantiate_actor(definition).is_err());
        }
        execute(&mut rust, "collectgarbage('collect')");
        let def = execute(&mut rust, "return Def.SongMeterDisplay{}")[0];
        let scene = rust.instantiate_actor(def).unwrap();
        rust.release_actor_scene(scene).unwrap();
    }
}

#[test]
fn stream_subscription_begins_after_parent_init_and_keeps_native_loading_name() {
    let mut p = Pair::new(
        true,
        r#"Def.SongMeterDisplay{
        Name='meter', StreamWidth=100,
        Stream=Def.ActorFrame{
            ProbeMessageCommand=function(self)log('message',self:GetName())end,
            Def.Actor{InitCommand=function()MESSAGEMAN:Broadcast('Probe')end},
            InitCommand=function(self)log('stream init',self:GetName());MESSAGEMAN:Broadcast('Probe')end},
        InitCommand=function(self)
            m=self;s=self.Stream;log('meter init',s:GetName());MESSAGEMAN:Broadcast('Probe');s.Name='changed'
        end}"#,
    );
    p.script(
        "assert(s:GetName()=='changed' and m:GetChild('Stream')==s);MESSAGEMAN:Broadcast('Probe')",
    );
}

#[test]
fn duplicate_stream_name_uses_native_group_lifecycle_and_payload() {
    let mut p = Pair::new(
        true,
        r#"Def.SongMeterDisplay{
        StreamWidth=100,
        Stream=Def.ActorFrame{InitCommand=function()log('stream init')end},
        Def.ActorFrame{Name='Stream',InitCommand=function()log('authored init')end},
        InitCommand=function(self)m=self;s=self.Stream;a=self[1];log('meter init')end}"#,
    );
    p.script("assert(m:GetChild('Stream')[1]==s and m:GetChild('Stream')[2]==a);s:x(12);log('immediate',s:GetX(),s:GetTweenTimeLeft());s:linear(1):x(20);log('queued',s:GetX(),s:GetTweenTimeLeft());a:linear(1):x(40);s:SetUpdateFunction(function(self,dt)collectgarbage('collect');log('s',dt,self:GetX())end);a:SetUpdateFunction(function(self,dt)collectgarbage('collect');log('a',dt,self:GetX());if not changed then changed=true;m:SetUpdateRate(1);collectgarbage('collect')end end)");

    p.rust.start_actor_scene(p.scene).unwrap();
    p.compare("start", false);
    p.advance(0.125);
    p.advance(0.125);
    p.script(
        "log('parents',s:GetParent()==m,a:GetParent()==m);weak=setmetatable({m,s,a},{__mode='v'})",
    );
    p.rust.release_actor_scene(p.scene).unwrap();
    execute(
        &mut p.rust,
        "root=nil;m=nil;s=nil;a=nil;other=nil;collectgarbage('collect');collectgarbage('collect');assert(next(weak)==nil)",
    );
}

#[test]
fn bundled_song_info_meter_declaration_preserves_its_quad_stream() {
    let source = include_str!(
        "../assets/simply-love/BGAnimations/ScreenGameplay underlay/Shared/SongInfoBar.lua"
    );
    let begin = source.find("Def.SongMeterDisplay{").unwrap();
    let end = source[begin..].find("-- Song Title").unwrap() + begin;
    let declaration = source[begin..end].trim().trim_end_matches(',');
    for width in [310, 417] {
        let definition = format!(
            r#"(function()
            local w={width}
            local GetCurrentColor=function(bright)assert(bright==true);return {{.2,.3,.4,1}}end
            return Def.ActorFrame{{{declaration},InitCommand=function(self)m=self[1];s=m.Stream end}}
        end)()"#
        );
        let mut p = Pair::new(true, &definition);
        p.script(&format!(
            "assert(m:GetStreamWidth()=={} and s:GetZoomY()==18 and s:GetTexture()~=nil)",
            width - 4
        ));
    }
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
