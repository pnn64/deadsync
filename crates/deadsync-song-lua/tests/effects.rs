use deadlib_lua::{Value, compile_named};
use deadsync_rules::timing::{TimingData, TimingSegments};
use deadsync_song_lua::{
    SongLuaCompileContext, {ActorScene, SongRuntime},
};

struct Pair {
    rust: SongRuntime,
    scene: ActorScene,
    frame_clock: [f32; 2],
}

fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/effects.lua").unwrap())
        .unwrap()
        .to_vec()
}

impl Pair {
    fn new(definition: &str) -> Self {
        let mut context = SongLuaCompileContext::new(".", "effect migration");
        context.live = true;
        Self::with_context(definition, &context)
    }
    fn with_context(definition: &str, context: &SongLuaCompileContext) -> Self {
        let source = format!(
            r#"
            observations={{}}
            function log(...) for i=1,select('#',...) do
                local v=select(i,...)
                observations[#observations+1]=type(v)=='number' and string.format('%.17g',v) or tostring(v)
            end end
            function observe()
                local function visit(a)
                    log(a:GetSecsIntoEffect(),a:GetEffectDelta(),a:GetTweenTimeLeft())
                    log(a:geteffectmagnitude());log(a:getrotation())
                    log(a:GetDiffuseAlpha(),a:GetX(),a:GetDestX())
                    for i=1,#a do visit(a[i]) end
                end
                visit(root)
            end
            {definition}
        "#
        );
        let mut rust = SongRuntime::new(context).unwrap();
        let definition = execute(&mut rust, &source)[0];
        let scene = rust.instantiate_actor(definition).unwrap();
        let root = rust.actor_scene_root(scene).unwrap();
        rust.vm_mut().set_global(b"root", root).unwrap();
        let mut pair = Self {
            rust,
            scene,
            frame_clock: [0.0; 2],
        };
        pair.compare("construction");
        pair
    }
    fn compare(&mut self, phase: &str) {
        let actual = self
            .rust
            .actor_scene_render_states(self.scene, self.frame_clock)
            .unwrap();
        let actual = actual
            .iter()
            .map(|state| format!("{state:?}"))
            .collect::<Vec<_>>();
        reference_test::assert_source_debug(
            "song/tests/rust_effects.rs::Pair::compare::render",
            phase,
            &(self.frame_clock.map(f32::to_bits), actual),
        );
        let Value::String(actual) =
            execute(&mut self.rust, "return table.concat(observations,'|')")[0]
        else {
            panic!("string")
        };
        let actual = self.rust.vm_mut().string_bytes(actual).unwrap();
        let actual = String::from_utf8_lossy(actual);
        let actual: Vec<_> = actual.split('|').collect();
        reference_test::assert_source_debug(
            "song/tests/rust_effects.rs::Pair::compare::getters",
            phase,
            &actual,
        );
    }
    fn script(&mut self, script: &str) {
        execute(&mut self.rust, script);
        self.compare(script);
    }
    fn at(&mut self, seconds: f32, beat: f32, delta: f64) {
        let delta_beat = beat - self.frame_clock[1];
        self.rust.set_values(beat, seconds).unwrap();
        self.rust
            .set_delta_values(delta_beat, delta as f32)
            .unwrap();
        self.frame_clock = [seconds, beat];
        self.rust.advance_actor_scene(self.scene, delta).unwrap();
        self.script("observe()");
    }
    fn advance(&mut self, delta: f64) {
        self.at(
            self.frame_clock[0] + delta as f32,
            self.frame_clock[1] + delta as f32 * 2.0,
            delta,
        );
    }
}

#[test]
fn effect_defaults_reselection_and_phase_resets_match_production() {
    let mut pair = Pair::new("return Def.Quad{}");
    pair.script("observe()");
    pair.advance(0.375);
    for name in [
        "pulse",
        "bob",
        "bounce",
        "wag",
        "diffuseblink",
        "diffuseshift",
        "diffuseramp",
        "glowblink",
        "glowshift",
        "glowramp",
        "rainbow",
        "vibrate",
        "spin",
        "stopeffect",
    ] {
        pair.script(&format!("assert(root:{name}()==root); observe()"));
        pair.advance(0.125);
        pair.script(&format!("root:{name}(); observe()"));
        pair.advance(0.25);
    }
    pair.script("root:bob():effectperiod(3); observe()");
    pair.advance(0.25);
    pair.script("root:bob(); observe()");
}

#[test]
fn effect_timing_colors_magnitude_and_invalid_inputs_match_production() {
    let mut pair = Pair::new("return Def.Quad{}");
    for script in [
        "root:effectcolor1(.1,.2,.3,.4):effectcolor2('#abcdef01'):effectmagnitude(1,2,3):effectoffset(.25):effectperiod(4); observe()",
        "root:effecttiming(1,2,3,4,5); observe()",
        "root:effect_hold_at_full(2); observe()",
        "root:effectperiod(3); observe()",
        "root:effecttiming(-1,2,-3,4):effectmagnitude(false,'8',nil):effectcolor1(false):effectcolor2({1,2}); observe()",
        "root:effecttiming(0,0,0,0):effecttiming(1,2,false,4):effectperiod(false):effectoffset(false); observe()",
        "assert(not pcall(root.effect_hold_at_full,root,-1)); assert(not pcall(root.effect_hold_at_full,root)); observe()",
        "root:effect_hold_at_full('0x2'):effectcolor1({1,'0x2',3}):effectcolor2('invalid'); observe()",
        "root:effectperiod(0); assert(not pcall(root.effect_hold_at_full,root,0)); observe()",
        "root:effect_hold_at_full(0/0):effectmagnitude(1/0,'inf',0/0):effectoffset('NaN'); observe()",
    ] {
        pair.script(script);
    }
}

#[test]
fn timers_exact_boundaries_hibernation_and_update_rates_match_production() {
    let mut pair = Pair::new("return Def.ActorFrame{Def.Quad{},Def.ActorFrame{Def.Quad{}}}");
    for delta in [0.5, 0.5, 0.0, 2.5, -0.5] {
        pair.advance(delta);
    }
    pair.script("root:effectperiod(0); root[1]:effectperiod(-1); root[2]:SetUpdateRate(3):hibernate(.25); root:SetUpdateRate(2); observe()");
    for delta in [0.125, 0.0, 0.25, 1.0, -0.5] {
        pair.advance(delta);
    }
    pair.script("root:hibernate(.5):SetUpdateFunction(function(self,dt) log('update',dt,self:GetSecsIntoEffect()); self[1]:pulse() end)");
    for delta in [0.25, 0.25, 0.0, 0.25] {
        pair.advance(delta);
    }
}

#[test]
fn spin_precedes_tweens_and_queued_commands_match_production() {
    for live in [false, true] {
        let mut context = SongLuaCompileContext::new(".", "spin migration");
        context.live = live;
        let mut pair = Pair::with_context(
            "return Def.Quad{InitCommand=function(self) self:rotationx(15):rotationy(25):rotationz(35):spin():effectmagnitude(20,-30,40) end}",
            &context,
        );
        pair.advance(0.25);
        pair.script("root:linear(.5):diffusealpha(.25):queuecommand('Switch'); root.SwitchCommand=function(self) log('switch',self:GetSecsIntoEffect(),self:GetRotationZ()); self:effectmagnitude(100,200,300):linear(.25):rotationz(70) end; observe()");
        for delta in [0.0, 0.125, 0.375, 0.125, 0.125, 0.1] {
            pair.advance(delta);
        }
        pair.script("root:linear(.5):rotationz(240); observe()");
        pair.advance(0.2);
        pair.script("root:stoptweening():stopeffect(); observe()");
        pair.advance(0.25);
        pair.script("root:spin():linear(.25):rotationx(180):finishtweening(); observe()");
        pair.advance(0.1);
    }
}

#[test]
fn global_clocks_aliases_rewinds_and_switches_match_production() {
    let mut pair = Pair::new("return Def.ActorFrame{Def.Quad{}}");
    for clock in [
        "music",
        "beat",
        "timer",
        "beatnooffset",
        "timerGlobal",
        "bgm",
        "musicnooffset",
        "seconds",
        "unknown",
        "'beat'",
        " timer ",
        "",
        "TiMeR",
    ] {
        pair.script(&format!(
            "root:effectclock({clock:?}); root[1]:effectclock({clock:?}):bob(); observe()"
        ));
        pair.at(4.25, 8.75, 0.125);
        pair.at(-1.25, -3.25, 0.0);
        pair.at(2.5, 6.75, 0.25);
    }
    pair.script("root:effectclock(false):effectclock(12); observe()");
}

#[test]
fn preparation_rate_music_origin_and_no_offset_timing_match_production() {
    for live in [false, true] {
        let mut context = SongLuaCompileContext::new(".", "effect timing migration");
        context.live = live;
        context.song_music_rate = 1.7;
        context.song_timing = Some(TimingData::from_segments(
            1.25,
            0.125,
            &TimingSegments {
                bpms: vec![(0.0, 120.0), (4.0, 90.0)],
                ..Default::default()
            },
            &[],
        ));
        let mut pair = Pair::with_context("return Def.Quad{}", &context);
        for clock in ["music", "beatnooffset", "beat", "timer"] {
            pair.script(&format!(
                "root:effectclock({clock:?}):pulse():effectmagnitude(2,3,4); observe()"
            ));
            for (seconds, beat, delta) in [
                (-0.0, 0.0, 0.0),
                (0.125, 7.0, 0.125),
                (1.5, 12.0, 0.25),
                (-0.5, -2.0, 0.0),
            ] {
                pair.at(seconds, beat, delta);
            }
        }
    }
}

#[test]
fn queued_resets_and_reentrant_callbacks_keep_clocks_and_gc_roots() {
    let mut pair = Pair::new(
        r#"
        NullActor:bob():effecttiming(.5,.25,.5,.75):effectcolor1(.2,.3,.4,.5)
        return Def.ActorFrame{NullActor,NullActor,
            PulseMessageCommand=function(self,p)
                log('message',self:GetSecsIntoEffect(),self:GetEffectDelta())
                self:pulse(); self[1]:bob(); collectgarbage('collect')
                if not p.again then p.again=true; MESSAGEMAN:Broadcast('Pulse',p) end
            end}
    "#,
    );
    pair.script("root:bob():sleep(.25):queuecommand('Reset'); root.ResetCommand=function(self) self:bounce(); self[1]:wag(); MESSAGEMAN:Broadcast('Pulse',{}) end; root:SetUpdateFunction(function(self,dt) log('update',self:GetSecsIntoEffect(),self[1]:GetSecsIntoEffect()); self[2]:spin():effectmagnitude(0,0,12); collectgarbage('collect') end)");
    for delta in [0.0, 0.25, 0.125, 0.25] {
        pair.advance(delta);
    }
}

#[test]
fn failed_first_clock_read_retains_partial_state_and_recovers() {
    let mut pair = Pair::new("return Def.Quad{}");
    pair.script(
        r#"
        r=__songlua_compile_song_runtime
        root:effectclock('music')
        r.__songlua_music_seconds=8
        r.__songlua_music_rate=nil
        setmetatable(r,{__index=function() collectgarbage('collect'); error('clock fault') end})
    "#,
    );
    assert!(pair.rust.advance_actor_scene(pair.scene, 0.25).is_err());
    pair.script(
        r#"
        setmetatable(r,nil); r.__songlua_music_rate=1
        local ok=pcall(root.GetSecsIntoEffect,root)
        assert(not ok); log(ok,root:GetEffectDelta())
    "#,
    );
    pair.at(9.0, 18.0, 0.25);
    pair.script("assert(root:GetSecsIntoEffect()==9 and root:GetEffectDelta()==9)");
}

#[test]
fn rotation_getter_publication_preserves_the_playback_boundary() {
    let mut pair = Pair::new("return Def.Quad{InitCommand=function(self) self:spin() end}");
    pair.advance(0.25);
    pair.script("root:diffusealpha(.5); observe(); assert(root:GetRotationZ()==45)");
    pair.advance(0.25);
    pair.rust.begin_actor_playback();
    pair.compare("playback activation");
    pair.script("root:diffuse(1,1,1,.25):diffusecolor(.2,.3,.4):diffusealpha(.75); observe(); assert(root:GetRotationZ()==45)");
    pair.script("root:addrotationz(5):rotationx(15); observe(); assert(root:GetRotationZ()==95)");
    pair.script("root:linear(.5):diffusealpha(.1); observe()");
    pair.advance(0.25);
    pair.script("root:stoptweening(); observe()");
    pair.advance(0.125);
    pair.script("root:linear(.25):rotationy(60):finishtweening(); observe()");
    pair.advance(0.125);
}

#[test]
fn detached_runtime_tables_stay_rooted_across_clock_reads_and_reentry() {
    let mut context = SongLuaCompileContext::new(".", "effect clock rooting");
    context.song_music_rate = 1.7;
    let mut pair = Pair::with_context("return Def.Quad{}", &context);
    pair.script(
        r#"
        original_runtime=__songlua_compile_song_runtime
        root:effectclock('music')
        __songlua_compile_song_runtime=setmetatable({}, {__index=function(_,key)
            if key=='__songlua_music_seconds' then
                __songlua_compile_song_runtime=nil
                collectgarbage('collect'); root:effectmagnitude(7,8,9)
                return '2.5'
            elseif key=='__songlua_music_rate' then
                collectgarbage('collect'); return '1.7'
            end
            error('unexpected clock key '..key)
        end})
    "#,
    );
    pair.rust.advance_actor_scene(pair.scene, 0.25).unwrap();
    pair.script("__songlua_compile_song_runtime=original_runtime; observe(); assert(root:GetSecsIntoEffect()==4.25)");
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
