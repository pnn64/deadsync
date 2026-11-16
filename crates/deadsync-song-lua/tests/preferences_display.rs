use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{SongLuaCompileContext, SongRuntime};
use std::path::Path;

fn context() -> SongLuaCompileContext {
    let mut context = SongLuaCompileContext::new(Path::new("."), "preference/display migration");
    context.screen_width = 854.;
    context.screen_height = 480.;
    context.display_width = 1366.;
    context.display_height = 768.;
    context.global_offset_seconds = 0.02;
    context.video_renderers = "null".into();
    context.last_seen_video_driver = "NVIDIA test adapter".into();
    context.event_mode = false;
    context
}

fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/preferences-display.lua").unwrap())
        .unwrap_or_else(|e| panic!("{source}\n{e:?}"))
        .to_vec()
}

#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    String(Vec<u8>),
}
fn numeric(n: f64) -> Scalar {
    Scalar::Number(if n.is_nan() {
        f64::NAN.to_bits()
    } else {
        n.to_bits()
    })
}
struct Pair {
    rust: SongRuntime,
}
impl Pair {
    fn new(context: &SongLuaCompileContext) -> Self {
        Self {
            rust: SongRuntime::new(context).unwrap(),
        }
    }
    fn compare(&mut self, source: &str) {
        let actual: Vec<_> = execute(&mut self.rust, source)
            .into_iter()
            .map(|value| match value {
                Value::Nil => Scalar::Nil,
                Value::Boolean(b) => Scalar::Boolean(b),
                Value::Number(n) => numeric(n),
                Value::String(id) => {
                    Scalar::String(self.rust.vm_mut().string_bytes(id).unwrap().to_vec())
                }
                other => panic!("expected scalar, got {other:?}"),
            })
            .collect();
        reference_test::assert_source_debug(
            "song/tests/rust_preferences_display.rs::Pair::compare",
            source,
            &actual,
        );
    }
}

#[test]
fn preference_defaults_case_and_prefixes_match_production() {
    let mut p = Pair::new(&context());
    for key in [
        "GlobalOffsetSeconds",
        "DisplayAspectRatio",
        "DisplayWidth",
        "DisplayHeight",
        "VideoRenderers",
        "LastSeenVideoDriver",
        "EventMode",
        "VisualDelaySeconds",
        "BackgroundFitMode",
        "BGBrightness",
        "TimingWindowScale",
        "TimingWindowAdd",
        "TimingWindowSeconds",
        "TimingWindowSecondsW1",
        "AutoGenGroupCourses",
        "Center1Player",
        "EasterEggs",
        "HarshHotLifePenalty",
        "MemoryCards",
        "MenuTimer",
        "OnlyDedicatedMenuButtons",
        "RateModsAffectFGChanges",
        "ShowBanners",
        "ShowNativeLanguage",
        "ThreeKeyNavigation",
        "CoinsPerCredit",
        "CustomSongsLoadTimeout",
        "CustomSongsMaxMegabytes",
        "CustomSongsMaxSeconds",
        "LifeDifficultyScale",
        "LongVerSongSeconds",
        "MarathonVerSongSeconds",
        "MaxHighScoresPerListFor",
        "MaxHighScoresPerListForMachine",
        "MaxRegenComboAfterMiss",
        "MinTnsToScoreNotes",
        "MusicWheelSwitchSpeed",
        "RegenComboAfterMiss",
        "SongsPerPlay",
        "SoundVolume",
        "RefreshRate",
        "CoinMode",
        "DefaultModifiers",
        "EditorNoteSkinP1",
        "EditorNoteSkinP2",
        "DisplayMode",
        "DisplayResolution",
        "FullscreenType",
        "HttpAllowHosts",
        "Premium",
        "Theme",
        "DefaultLocalProfileID",
        "DefaultLocalProfileID_P1",
        "unknown",
        "",
        "éventMode",
    ] {
        for key in [
            key.to_owned(),
            key.to_ascii_lowercase(),
            key.to_ascii_uppercase(),
        ] {
            p.compare(&format!(
                "return PREFSMAN:GetPreference({key:?}),PREFSMAN:PreferenceExists({key:?})"
            ));
        }
    }
    p.compare("return PREFSMAN:GetPreference(string.rep('A',200)),PREFSMAN:PreferenceExists(string.rep('A',200))");
}

#[test]
fn preference_mutations_keep_original_receiver_values_and_side_effect_counts() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        local p=PREFSMAN
        assert(p:GetPreference('EventMode')==false and p:GetPreference('eventmode')==true)
        local object={};local fn=function()return 23 end
        assert(p.SetPreference('SoundVolume',object)==p and p.GetPreference({},'SoundVolume')==object)
        p:SetPreference('soundvolume',fn);assert(p:GetPreference('soundvolume')==fn)
        p:SetPreference('Unknown',123);assert(p:GetPreference('Unknown')==nil)
        p.__songlua_preference_values.Unknown=456
        assert(p:GetPreference('Unknown')==456 and not p:PreferenceExists('Unknown'))
        assert(p:SetPreferenceToDefault('Unknown')==p and p:GetPreference('Unknown')==nil)
        p:SetPreference('EventMode',nil);assert(p:GetPreference('EventMode')==true)
        p:SetPreference('SoundVolume',false);assert(p:GetPreference('SoundVolume')==false)
        assert(p.SavePreferences({},'ignored')==p)
        p:SetPreferenceToDefault('SoundVolume');assert(p:GetPreference('SoundVolume')==1)
        return __songlua_side_effect_count
    "#);
    for key in [
        "nil",
        "true",
        "12",
        "{}",
        "function()end",
        "string.char(255)",
    ] {
        p.compare(&format!("local p=PREFSMAN;local key={key};assert(p:SetPreference(key,1)==p);assert(p:SetPreferenceToDefault(key)==p);return p:GetPreference(key),p:PreferenceExists(key),__songlua_side_effect_count"));
    }
    p.compare("__songlua_side_effect_count='12.75';PREFSMAN:SavePreferences();return __songlua_side_effect_count");
    p.compare("__songlua_side_effect_count={};local ok=pcall(PREFSMAN.SetPreference,'SoundVolume',9);__songlua_side_effect_count=0;return ok,PREFSMAN:GetPreference('SoundVolume')");
}

#[test]
fn preference_store_metamethods_reenter_collect_and_preserve_failure_order() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        trace={};local store=PREFSMAN.__songlua_preference_values
        setmetatable(store,{__index=function(t,k)
            collectgarbage('collect');trace[#trace+1]='get:'..k
            if k=='Loop' then PREFSMAN:SetPreference('SoundVolume',.5);return PREFSMAN:GetPreference('SoundVolume') end
            if k=='bad' then error('read failure') end
        end,__newindex=function(t,k,v)
            collectgarbage('collect');trace[#trace+1]='set:'..k..':'..__songlua_side_effect_count
            if k=='BGBrightness' then error('write failure') end
            rawset(t,k,v)
        end})
        local value=PREFSMAN:GetPreference('Loop')
        local write=pcall(PREFSMAN.SetPreference,'BGBrightness',.1)
        local read=pcall(PREFSMAN.GetPreference,'bad')
        PREFSMAN:SetPreference('unknown',10)
        PREFSMAN:SetPreferenceToDefault('other')
        return value,write,read,__songlua_side_effect_count,table.concat(trace,'|')
    "#);
    p.compare("setmetatable(PREFSMAN.__songlua_preference_values,nil);PREFSMAN:SetPreference('BGBrightness',.8);return PREFSMAN:GetPreference('BGBrightness')");
}

#[test]
fn screen_dimensions_and_display_modes_remain_independent_of_preference_mutation() {
    for (screen_w, screen_h, display_w, display_h, texture) in [
        (854., 480., 0., 0., false),
        (700.25, 500.75, 1919.5, 1079.5, true),
        (-1., 0., -10., -1., false),
        (1280., 720., 3840., 2160., true),
    ] {
        let mut context = context();
        context.screen_width = screen_w;
        context.screen_height = screen_h;
        context.display_width = display_w;
        context.display_height = display_h;
        context.render_to_texture = texture;
        let mut p = Pair::new(&context);
        p.compare(r#"return SCREEN_WIDTH,SCREEN_HEIGHT,SCREEN_CENTER_X,SCREEN_CENTER_Y,scx,scy,SCREEN_LEFT,SCREEN_TOP,SCREEN_RIGHT,SCREEN_BOTTOM,
            _screen.w,_screen.h,_screen.cx,_screen.cy,_screen.l,_screen.t,_screen.r,_screen.b,ASPECT_SCALE_FACTOR,
            DISPLAY:GetDisplayWidth(),DISPLAY:GetDisplayHeight(),DISPLAY:SupportsRenderToTexture(),GetScreenAspectRatio(),IsUsingWideScreen(),
            WideScale('2','10'),SL_WideScale(-4,12)"#);
        p.compare(r#"PREFSMAN:SetPreference('DisplayWidth',1024);PREFSMAN:SetPreference('DisplayHeight',768);PREFSMAN:SetPreference('DisplayAspectRatio',4/3)
            return SCREEN_WIDTH,SCREEN_HEIGHT,_screen.w,_screen.h,DISPLAY:GetDisplayWidth(),DISPLAY:GetDisplayHeight(),GetScreenAspectRatio(),WideScale(2,10)"#);
    }
}

#[test]
fn display_spec_identity_mutation_and_saved_methods_survive_collection() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        local display=DISPLAY;local specs=display:GetDisplaySpecs();local spec=specs[1]
        local mode=spec:GetCurrentMode();local modes=spec:GetSupportedModes()
        assert(display.GetDisplaySpecs(false)==specs and modes[1]==mode and spec:GetCurrentMode()==mode)
        assert(spec:GetSupportedModes()==modes and spec.GetId()=='Default' and spec:GetName()=='Default Display')
        local get=spec.GetCurrentMode;local getspecs=display.GetDisplaySpecs
        specs.marker='same';modes[1]={};specs[1]=nil;DISPLAY={}
        collectgarbage('collect')
        assert(get()==mode and getspecs()==specs and specs.marker=='same')
        return mode:GetWidth(),mode:GetHeight(),mode:GetRefreshRate(),#modes,
            tostring(specs),spec:IsVirtual(),display:GetFPS(),display:GetVPF(),display:GetCumFPS(),display:SupportsFullscreenBorderlessWindow()
    "#);
}

#[test]
fn aspect_reads_current_public_store_while_preferences_keep_the_original_store() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        old=PREFSMAN;old:SetPreference('DisplayAspectRatio',2)
        old.__songlua_preference_values={DisplayAspectRatio=1.25}
        assert(old:GetPreference('DisplayAspectRatio')==2 and GetScreenAspectRatio()==1.25)
        PREFSMAN={__songlua_preference_values=setmetatable({}, {__index=function()error('must be raw')end})}
        return GetScreenAspectRatio(),old:GetPreference('DisplayAspectRatio')
    "#);
    p.compare("PREFSMAN.__songlua_preference_values.DisplayAspectRatio='0x1.8';return GetScreenAspectRatio()");
    for value in ["false", "{}", "'nonnumeric'"] {
        p.compare(&format!("PREFSMAN.__songlua_preference_values.DisplayAspectRatio={value};local ok=pcall(GetScreenAspectRatio);return ok"));
    }
    p.compare(r#"
        rawset(_G,'PREFSMAN',nil)
        local count=0
        setmetatable(_G,{__index=function(t,k)
            if k=='PREFSMAN' then count=count+1;collectgarbage('collect');return {__songlua_preference_values={DisplayAspectRatio=3}} end
        end})
        local aspect=GetScreenAspectRatio();setmetatable(_G,nil)
        return aspect,count,old:GetPreference('DisplayAspectRatio')
    "#);
}

#[test]
fn manager_captures_release_cycles_without_losing_saved_values() {
    let mut runtime = SongRuntime::new(&context()).unwrap();
    execute(
        &mut runtime,
        r#"
        weak=setmetatable({}, {__mode='v'})
        weak.manager=PREFSMAN;weak.store=PREFSMAN.__songlua_preference_values
        getter=PREFSMAN.GetPreference
        local payload={value=37};weak.payload=payload
        PREFSMAN:SetPreference('SoundVolume',payload)
        PREFSMAN=nil;payload=nil
        collectgarbage('collect');collectgarbage('collect')
        assert(weak.manager==nil and weak.store~=nil and getter('SoundVolume').value==37)
        getter=nil;collectgarbage('collect');collectgarbage('collect')
        assert(weak.store==nil and weak.payload==nil)
        weak.display=DISPLAY;local specs=DISPLAY:GetDisplaySpecs();weak.specs=specs
        getspecs=DISPLAY.GetDisplaySpecs;DISPLAY=nil;specs=nil
        collectgarbage('collect');collectgarbage('collect')
        assert(weak.display==nil and weak.specs==getspecs())
        getspecs=nil;collectgarbage('collect');collectgarbage('collect')
        assert(weak.specs==nil)
    "#,
    );
    let mut other = SongRuntime::new(&context()).unwrap();
    execute(
        &mut other,
        "assert(PREFSMAN:GetPreference('SoundVolume')==1 and DISPLAY:GetDisplayWidth()==1366)",
    );
}

#[test]
fn side_effect_global_getters_and_setters_preserve_callback_order() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        local trace={};rawset(_G,'__songlua_side_effect_count',nil)
        setmetatable(_G,{__index=function(t,k)
            if k=='__songlua_side_effect_count' then collectgarbage('collect');trace[#trace+1]='read';return '4' end
        end,__newindex=function(t,k,v)
            if k=='__songlua_side_effect_count' then trace[#trace+1]='write:'..v;error('counter rejected') end
            rawset(t,k,v)
        end})
        local ok=pcall(PREFSMAN.SetPreference,'SoundVolume',10)
        setmetatable(_G,nil)
        return ok,PREFSMAN:GetPreference('SoundVolume'),table.concat(trace,'|')
    "#);
    p.compare("tonumber=function()error('replacement primitive')end;PREFSMAN:SetPreference('SoundVolume',.25);return PREFSMAN:GetPreference('SoundVolume'),__songlua_side_effect_count");
}

#[test]
fn existing_display_size_fixture_preserves_framebuffer_dimensions_and_render_state() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/song_lua/display-size.lua");
    let source = std::fs::read_to_string(&path).unwrap();
    for live in [false, true] {
        let mut context = context();
        context.live = live;
        let mut rust = SongRuntime::new(&context).unwrap();
        let value = execute(&mut rust, &source)[0];
        let scene = rust.instantiate_actor(value).unwrap();
        let root = rust.actor_scene_root(scene).unwrap();
        rust.vm_mut().set_global(b"root", root).unwrap();
        let actual: Vec<_> = rust
            .actor_scene_snapshot(scene)
            .unwrap()
            .into_iter()
            .map(|actor| format!("{:?}", actor.render))
            .collect();
        reference_test::assert_debug("song/tests/rust_preferences_display.rs::render", &actual);
        for name in ["ScreenTex", "VStripsTex", "HStripsTex"] {
            let script = format!("a=root:GetChild({name:?})");
            execute(&mut rust, &script);
            let actor = rust.vm_mut().get_global(b"a").unwrap();
            reference_test::assert_debug(
                "song/tests/rust_preferences_display.rs::texture",
                &rust.actor_frame_texture(actor).unwrap(),
            );
        }
        rust.release_actor_scene(scene).unwrap();
    }
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
