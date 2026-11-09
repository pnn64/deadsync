use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, SongLuaLiveRequest,
    rust_engine::{RustSongRuntime, test_support},
};
use std::path::Path;

#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    String(Vec<u8>),
}
fn number(n: f64) -> Scalar {
    Scalar::Number(if n.is_nan() {
        f64::NAN.to_bits()
    } else {
        n.to_bits()
    })
}
struct Pair {
    rust: RustSongRuntime,
}
impl Pair {
    fn new() -> Self {
        let context = SongLuaCompileContext::new(Path::new("."), "screen manager migration");
        let mut rust = RustSongRuntime::new(&context).unwrap();
        test_support::expose_screen_publication(&mut rust).unwrap();
        let mut p = Self { rust };
        p.compare("TOP={Name='ScreenGameplay'};P1={side=1};P2={side=2};__songlua_side_effect_count=0;PUBLISH(TOP,P1,P2)");
        p
    }
    fn compare(&mut self, source: &str) {
        let values = self
            .rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"@/fixture/screen-manager.lua").unwrap())
            .unwrap_or_else(|e| panic!("Rust: {source}\n{e:?}"))
            .to_vec();
        let actual: Vec<_> = values
            .into_iter()
            .map(|v| match v {
                Value::Nil => Scalar::Nil,
                Value::Boolean(v) => Scalar::Boolean(v),
                Value::Number(v) => number(v),
                Value::String(v) => {
                    Scalar::String(self.rust.vm_mut().string_bytes(v).unwrap().to_vec())
                }
                v => panic!("scalar expected {v:?}"),
            })
            .collect();
        reference_test::assert_source_debug(
            "song/tests/rust_screen_manager.rs::Pair::compare",
            source,
            &actual,
        );
    }
    fn capture(&self) {
        self.rust.begin_screen_request_capture();
    }
    fn take(&self) -> (Option<SongLuaLiveRequest>, Vec<(f64, SongLuaLiveRequest)>) {
        let (actual, traces) = self.rust.take_screen_requests();
        reference_test::assert_debug(
            "song/tests/rust_screen_manager.rs::Pair::take",
            &(
                &actual,
                traces
                    .iter()
                    .map(|e| (number(e.music_seconds), &e.request))
                    .collect::<Vec<_>>(),
            ),
        );
        (
            actual,
            traces
                .into_iter()
                .map(|e| (e.music_seconds, e.request))
                .collect(),
        )
    }
}

#[test]
fn publication_preserves_manager_identity_and_saved_player_getters() {
    let mut p = Pair::new();
    p.compare(r#"
        local names={};for k in pairs(SCREENMAN)do names[#names+1]=k end;table.sort(names)
        assert(table.concat(names,',')=='AddNewScreenToTop,GetTopScreen,SetNewScreen,SystemMessage,set_input_redirected')
        assert(__songlua_top_screen==TOP and __songlua_top_screen_player_1==P1 and __songlua_top_screen_player_2==P2)
        OLD_MANAGER=SCREENMAN;OLD_AF=GetPlayerAF;OLD_TOP=SCREENMAN.GetTopScreen
        SCREENMAN.marker={};SCREENMAN.SystemMessage='authored override';A={side=3};B={side=4};T={Name='replacement'}
        PUBLISH(T,A,B)
        assert(SCREENMAN==OLD_MANAGER and SCREENMAN.SystemMessage=='authored override')
        assert(OLD_TOP()==T and GetPlayerAF(0)==A and GetPlayerAF(1)==B)
        assert(OLD_AF(0)==P1 and OLD_AF(1)==P2);return true
    "#);
    for argument in [
        "0",
        "-0",
        "1",
        "1.5",
        "2",
        "-1",
        "0/0",
        "1/0",
        "'P1'",
        "'p2'",
        "'PlayerNumber_p1'",
        "'PLAYERnumber_P2'",
        "'0'",
        "'2'",
        "''",
        "'P3'",
        "true",
        "nil",
        "{}",
    ] {
        p.compare(&format!("local a=GetPlayerAF({argument});local b=GetPlayerAF({{}},{argument});return a and a.side,b and b.side"));
    }
}

#[test]
fn loading_and_top_queries_are_raw_while_add_screen_uses_ordinary_lookup() {
    let mut p = Pair::new();
    for value in ["nil", "false", "true", "0", "''", "{}", "function()end"] {
        p.compare(&format!(
            "__songlua_screen_loading={value};return SCREENMAN:GetTopScreen()==TOP"
        ));
    }
    p.compare(r#"
        __songlua_screen_loading=nil;__songlua_top_screen=nil;LOOKUPS=0
        setmetatable(_G,{__index=function(t,k)LOOKUPS=LOOKUPS+1;if k=='__songlua_top_screen' then return TOP end;if k=='__songlua_screen_loading' then return true end end})
        assert(SCREENMAN:GetTopScreen()==nil and LOOKUPS==0)
        SCREENMAN:AddNewScreenToTop('overlay');assert(TOP.Name=='overlay' and LOOKUPS==1)
        setmetatable(_G,nil);__songlua_top_screen=17
        assert(SCREENMAN:GetTopScreen()==17);assert(not pcall(SCREENMAN.AddNewScreenToTop,SCREENMAN,'bad'))
        assert(pcall(SCREENMAN.AddNewScreenToTop,SCREENMAN,17));return __songlua_side_effect_count
    "#);
}

#[test]
fn replacement_recreates_invalid_managers_and_preserves_authored_tables() {
    let mut p = Pair::new();
    for bad in ["nil", "false", "0", "'name'", "function()end"] {
        p.compare(&format!("SCREENMAN={bad};PUBLISH(TOP,P1,P2);assert(type(SCREENMAN)=='table' and SCREENMAN:GetTopScreen()==TOP)"));
    }
    p.compare("SCREENMAN=setmetatable({marker=7},{__index=function()error('no manager lookup')end});local manager=SCREENMAN;PUBLISH(TOP,P1,P2);assert(SCREENMAN==manager and SCREENMAN.marker==7);assert(rawget(SCREENMAN,'GetTopScreen')==nil)");
}

#[test]
fn requests_preserve_duplicates_precision_and_drain_without_changing_top() {
    let mut p = Pair::new();
    p.compare("SCREENMAN:SetNewScreen('first');SCREENMAN:SetNewScreen('first');SCREENMAN:SetNewScreen('last');assert(TOP.Name=='ScreenGameplay')");
    let (request, source) = p.take();
    assert_eq!(request, Some(SongLuaLiveRequest::NewScreen("last".into())));
    assert!(source.is_empty());
    p.capture();
    p.compare("__songlua_compile_song_runtime.__songlua_music_seconds=0.123456789;SCREENMAN:SetNewScreen('ScreenSelectMusic');SCREENMAN.SetNewScreen('ScreenGameplay');SCREENMAN:SetNewScreen('ScreenGameplay');assert(SCREENMAN:GetTopScreen()==TOP)");
    p.capture();
    let (request, source) = p.take();
    assert_eq!(
        request,
        Some(SongLuaLiveRequest::NewScreen("ScreenGameplay".into()))
    );
    assert_eq!(source.len(), 3);
    for ((time, request), name) in
        source
            .into_iter()
            .zip(["ScreenSelectMusic", "ScreenGameplay", "ScreenGameplay"])
    {
        assert_eq!(time, f64::from(0.123456789_f32));
        assert_eq!(request, SongLuaLiveRequest::NewScreen(name.into()));
    }
    assert_eq!(p.take(), (None, vec![]));
    p.compare("SCREENMAN:SetNewScreen('after drain')");
    assert_eq!(p.take().1.len(), 1);
}

#[test]
fn invalid_names_only_count_effects_and_disabled_capture_does_not_read_clock() {
    let mut p = Pair::new();
    p.compare("__songlua_compile_song_runtime=false;SCREENMAN:SetNewScreen('without clock')");
    assert_eq!(
        p.take().0,
        Some(SongLuaLiveRequest::NewScreen("without clock".into()))
    );
    p.capture();
    p.compare(r#"
        local before=__songlua_side_effect_count
        SCREENMAN:SetNewScreen();SCREENMAN:SetNewScreen(3);SCREENMAN:SetNewScreen(true);SCREENMAN:SetNewScreen({});SCREENMAN:SetNewScreen('\255')
        assert(__songlua_side_effect_count==before+5)
        assert(not pcall(SCREENMAN.SetNewScreen,SCREENMAN,'needs clock'));return __songlua_side_effect_count
    "#);
    assert_eq!(p.take(), (None, vec![]));
}

#[test]
fn request_and_name_writes_keep_partial_effects_on_failure() {
    let mut p = Pair::new();
    p.capture();
    p.compare(r#"
        __songlua_side_effect_count={}
        assert(not pcall(SCREENMAN.SetNewScreen,SCREENMAN,'queued before counter error'))
        TOP.Name=nil;setmetatable(TOP,{__newindex=function(t,k,v)rawset(t,k,v);error('name setter error')end})
        assert(not pcall(SCREENMAN.AddNewScreenToTop,SCREENMAN,'written before error'));assert(TOP.Name=='written before error')
        setmetatable(TOP,nil);__songlua_side_effect_count=0
        return TOP.Name
    "#);
    let (request, source) = p.take();
    assert_eq!(
        request,
        Some(SongLuaLiveRequest::NewScreen(
            "queued before counter error".into()
        ))
    );
    assert_eq!(source.len(), 1);
    p.compare("__songlua_compile_song_runtime={__songlua_song_beat='bad',__songlua_music_seconds=7};assert(not pcall(SCREENMAN.SetNewScreen,SCREENMAN,'bad beat'));assert(__songlua_side_effect_count==0)");
    assert_eq!(p.take(), (None, vec![]));
}

#[test]
fn clock_lookup_reentry_preserves_request_order_and_roots_old_runtime() {
    let mut p = Pair::new();
    p.capture();
    p.compare(r#"
        SEEN={};local original=__songlua_compile_song_runtime
        __songlua_compile_song_runtime=setmetatable({}, {__index=function(t,k)
            SEEN[#SEEN+1]=k
            if k=='__songlua_song_beat' then
                if not ENTERED then ENTERED=true;SCREENMAN:SetNewScreen('inner') end
                __songlua_compile_song_runtime=original;collectgarbage('collect');return '3.25'
            end
            if k=='__songlua_music_seconds' then collectgarbage('collect');return '17.123456789' end
        end})
        SCREENMAN:SetNewScreen('outer')
        assert(table.concat(SEEN,',')=='__songlua_song_beat,__songlua_song_beat,__songlua_music_seconds,__songlua_music_seconds')
        return __songlua_side_effect_count
    "#);
    let (request, source) = p.take();
    assert_eq!(request, Some(SongLuaLiveRequest::NewScreen("outer".into())));
    assert_eq!(
        source.iter().map(|v| &v.1).collect::<Vec<_>>(),
        [
            &SongLuaLiveRequest::NewScreen("inner".into()),
            &SongLuaLiveRequest::NewScreen("outer".into())
        ]
    );
    assert!(source.iter().all(|v| v.0 == f64::from(17.123456789_f32)));
}

#[test]
fn publication_roots_all_arguments_across_global_setters_and_partial_errors() {
    let mut p = Pair::new();
    p.compare(r#"
        local a,b,t={side='new1'},{side='new2'},{Name='new'}
        WEAK=setmetatable({a,b,t},{__mode='v'});SEEN={}
        __songlua_top_screen_player_1=nil;__songlua_top_screen_player_2=nil;__songlua_top_screen=nil;GetPlayerAF=nil
        setmetatable(_G,{__newindex=function(g,k,v)
            SEEN[#SEEN+1]=k;collectgarbage('collect')
            assert(WEAK[1] and WEAK[2] and WEAK[3])
            if k=='__songlua_top_screen_player_1' then SCREENMAN={authored=true} end
            rawset(g,k,v)
        end})
        PUBLISH(t,a,b);setmetatable(_G,nil)
        assert(SCREENMAN.authored and GetPlayerAF(0)==a and GetPlayerAF(1)==b and __songlua_top_screen==t)
        assert(table.concat(SEEN,',')=='__songlua_top_screen_player_1,__songlua_top_screen_player_2,__songlua_top_screen,GetPlayerAF')
        __songlua_top_screen_player_1=nil;__songlua_top_screen_player_2=nil
        setmetatable(_G,{__newindex=function(g,k,v)if k=='__songlua_top_screen_player_2' then error('stop publication') end;rawset(g,k,v)end})
        assert(not pcall(PUBLISH,TOP,P1,P2));setmetatable(_G,nil)
        assert(__songlua_top_screen_player_1==P1 and __songlua_top_screen_player_2==nil and __songlua_top_screen==t)
    "#);
}

#[test]
fn saved_getters_keep_only_their_player_pair_and_old_screens_are_collectible() {
    let mut p = Pair::new();
    p.compare(r#"
        WEAK=setmetatable({TOP,P1,P2},{__mode='v'});SAVED=GetPlayerAF;QUERY=SCREENMAN.GetTopScreen
        PUBLISH({Name='new'},{},{});TOP=nil;P1=nil;P2=nil
        collectgarbage('collect');collectgarbage('collect')
        assert(WEAK[1]==nil and WEAK[2] and WEAK[3]);assert(SAVED(0)==WEAK[2] and QUERY().Name=='new')
        SAVED=nil;collectgarbage('collect');collectgarbage('collect');assert(WEAK[2]==nil and WEAK[3]==nil)
    "#);
}

#[test]
fn pending_transition_survives_higher_priority_new_screen_and_runtimes_are_isolated() {
    let mut p = Pair::new();
    let other = Pair::new();
    test_support::seed_rust_transition(&p.rust, "SM_DoNextScreen");
    p.compare("SCREENMAN:SetNewScreen('winner')");
    assert_eq!(
        p.take().0,
        Some(SongLuaLiveRequest::NewScreen("winner".into()))
    );
    assert_eq!(
        p.take().0,
        Some(SongLuaLiveRequest::Transition("SM_DoNextScreen".into()))
    );
    assert_eq!(p.take(), (None, vec![]));
    assert_eq!(other.take(), (None, vec![]));
}

#[test]
fn effect_only_methods_keep_return_arity_and_typed_counter_conversion() {
    let mut p = Pair::new();
    for value in [
        "nil",
        "0",
        "-3.5",
        "'7.5'",
        "'9223372036854775808'",
        "'nan'",
        "'inf'",
        "'-4.5'",
        "0/0",
        "1/0",
        "-1/0",
        "2^63",
        "-2^63",
        "true",
        "{}",
        "'bad'",
    ] {
        p.compare(&format!(
            r#"
            __songlua_side_effect_count={value}
            local ok=pcall(SCREENMAN.SystemMessage,{{}},'anything',{{}})
            local v=__songlua_side_effect_count
            return ok,type(v),type(v)=='number' and v or nil
        "#
        ));
    }
    p.compare("__songlua_side_effect_count=0;assert(select('#',SCREENMAN:SystemMessage('message'))==0);assert(select('#',SCREENMAN:set_input_redirected({},false))==0);assert(__songlua_side_effect_count==2)");
}

#[test]
fn hooks_observe_authored_calls_without_internal_numeric_conversion_calls() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let mut p = Pair::new();
    p.capture();
    p.compare("__songlua_side_effect_count='2';__songlua_compile_song_runtime.__songlua_song_beat='1';__songlua_compile_song_runtime.__songlua_music_seconds='3.25'");
    let rust = Arc::new(AtomicUsize::new(0));
    let hook = p
        .rust
        .vm_mut()
        .new_host_function_with_data(
            |call| {
                call.function_data::<Arc<AtomicUsize>>()
                    .unwrap()
                    .fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            &[],
            rust.clone(),
        )
        .unwrap();
    p.rust
        .vm_mut()
        .with_native_stack(|stack| {
            stack.set_hook(stack.thread_id(), deadlib_lua::NativeHook::Host(hook), 1, 0)
        })
        .unwrap();
    p.compare("SCREENMAN:SystemMessage('message');SCREENMAN:SetNewScreen('hooked');SCREENMAN:AddNewScreenToTop('overlay')");
    reference_test::assert_debug(
        "song/tests/rust_screen_manager.rs::hooks",
        &rust.load(Ordering::SeqCst),
    );
    assert!(rust.load(Ordering::SeqCst) >= 3);
    p.rust
        .vm_mut()
        .with_native_stack(|stack| {
            stack.set_hook(stack.thread_id(), deadlib_lua::NativeHook::None, 0, 0)
        })
        .unwrap();
    p.compare("return __songlua_side_effect_count");
    assert_eq!(
        p.take().0,
        Some(SongLuaLiveRequest::NewScreen("hooked".into()))
    );
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
