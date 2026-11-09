use deadlib_lua::{ErrorKind, NativeHook, Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, SongLuaInputEvent, SongLuaInputType,
    rust_engine::{CompiledInput, RustSongRuntime, test_support},
};
use std::path::Path;

fn event(kind: SongLuaInputType) -> SongLuaInputEvent<'static> {
    SongLuaInputEvent {
        player: Some(0),
        controller: Some(1),
        button: "Up",
        game_button: "MenuUp",
        kind,
        device: "Keyboard",
        device_button: "Up",
        level: 0.3,
        z: -9,
        down: true,
        ago: 0.125,
        is_joystick: false,
        is_mouse: false,
    }
}
fn execute(runtime: &mut RustSongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/input.lua").unwrap())
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
struct Pair {
    rust: RustSongRuntime,
}
impl Pair {
    fn new() -> Self {
        let context = SongLuaCompileContext::new(Path::new("."), "input migration");
        let mut rust = RustSongRuntime::new(&context).unwrap();
        let screen = rust.vm_mut().try_new_table().unwrap();
        rust.vm_mut()
            .set_global(b"SCREEN", Value::Table(screen))
            .unwrap();
        rust.bind_screen_input(Value::Table(screen)).unwrap();
        let mut pair = Self { rust };
        test_support::install_input_controls(&mut pair.rust, &event(SongLuaInputType::Repeat))
            .unwrap();
        pair
    }
    fn compare(&mut self, source: &str) {
        let actual: Vec<_> = execute(&mut self.rust, source)
            .into_iter()
            .map(|v| match v {
                Value::Nil => Scalar::Nil,
                Value::Boolean(v) => Scalar::Boolean(v),
                Value::Number(v) => Scalar::Number(v.to_bits()),
                Value::String(v) => {
                    Scalar::String(self.rust.vm_mut().string_bytes(v).unwrap().to_vec())
                }
                v => panic!("scalar expected {v:?}"),
            })
            .collect();
        reference_test::assert_source_debug(
            "song/tests/rust_input.rs::Pair::compare",
            source,
            &actual,
        );
    }
    fn dispatch(&mut self, input: &SongLuaInputEvent<'_>, handled: bool, errors: &[&str]) {
        let (actual, messages) = self.rust.dispatch_input(input).unwrap();
        let name = "Rust";
        {
            assert_eq!(actual, handled, "{name}");
            assert_eq!(messages.len(), errors.len(), "{name}: {messages:?}");
            for (message, needle) in messages.iter().zip(errors) {
                assert!(
                    message.starts_with("Error running input callback: ")
                        && message.contains(needle),
                    "{name}: {message}"
                );
            }
        }
    }
    fn count(&self, count: usize, compiled: usize) {
        assert_eq!(
            test_support::input_functions(&self.rust).unwrap().len(),
            count
        );
        assert_eq!(test_support::input_compiled_count(&self.rust), compiled);
    }
    fn offline(&self, compatible: bool) {
        let b = self.rust.require_input_offline_compatible();
        reference_test::assert_debug("song/tests/rust_input.rs::offline", &b);
        assert_eq!(b.is_ok(), compatible);
    }
    // Order is opaque function identity, not insertion order or source order.
    // Assign logical roles separately in each heap so both test the same trace.
    fn ordered(&mut self, body: &str) {
        self.compare(&format!("SEEN={{}};FUNCTIONS={{}};ROLES={{}};for i=1,12 do local slot={{}};slot.f=function(e) local role=ROLES[slot.f];SEEN[#SEEN+1]=role;{body} end;FUNCTIONS[i]=slot.f end"));
        let vm = self.rust.vm_mut();
        let Value::Table(functions) = vm.get_global(b"FUNCTIONS").unwrap() else {
            panic!()
        };
        let mut values = (1..=12)
            .map(|i| {
                vm.raw_table_get(functions, Value::Number(i as f64))
                    .unwrap()
            })
            .collect::<Vec<_>>();
        values.sort_by_key(|&f| vm.object_pointer(f).unwrap());
        let Value::Table(roles) = vm.get_global(b"ROLES").unwrap() else {
            panic!()
        };
        for (i, name, role) in [(0, "LOW", "low"), (6, "MID", "mid"), (11, "HIGH", "high")] {
            let role = vm.try_intern(role.as_bytes()).unwrap();
            vm.raw_table_set(roles, values[i], role).unwrap();
            vm.set_global(name.as_bytes(), values[i]).unwrap();
        }
    }
}

#[test]
fn event_shape_conversions_shared_mutation_and_fresh_dispatch_tables() {
    let mut p = Pair::new();
    p.compare("EVENTS={};SCREEN:AddInputCallback(function(e) EVENTS[#EVENTS+1]=e end)");
    for (i, kind) in [
        SongLuaInputType::FirstPress,
        SongLuaInputType::Repeat,
        SongLuaInputType::Release,
    ]
    .into_iter()
    .enumerate()
    {
        let mut e = event(kind);
        e.player = [Some(0), None, Some(8)][i];
        e.controller = [Some(1), Some(3), None][i];
        e.down = i == 0;
        e.is_mouse = i == 1;
        e.is_joystick = i == 2;
        p.dispatch(&e, false, &[]);
        p.compare("local e=EVENTS[#EVENTS];local d=e.DeviceInput;local n,m=0,0;for _ in pairs(e)do n=n+1 end;for _ in pairs(d)do m=m+1 end;return n,m,e.button,e.GameButton,e.type,e.PlayerNumber,e.controller,e.MultiPlayer,d.device,d.button,d.level,d.z,d.down,d.ago,d.is_joystick,d.is_mouse");
    }
    p.compare("assert(EVENTS[1]~=EVENTS[2] and EVENTS[1].DeviceInput~=EVENTS[2].DeviceInput);return #EVENTS");
}

#[test]
fn strict_registration_chaining_dedup_and_sticky_offline_rejection() {
    let mut p = Pair::new();
    p.offline(true);
    p.dispatch(&event(SongLuaInputType::FirstPress), false, &[]);
    p.compare(r#"local add=SCREEN.AddInputCallback;local remove=SCREEN.RemoveInputCallback
        local callable=setmetatable({}, {__call=function()end})
        for _,f in ipairs{add,remove}do
            assert(not pcall(f));assert(not pcall(f,{},nil));assert(not pcall(f,{},callable));assert(not pcall(f,1,function()end))
        end
        F=function(e)return e.button=='Down' end;OTHER={}
        assert(add(OTHER,F,'ignored')==OTHER);assert(remove(OTHER,F)==OTHER)
        SCREEN:AddInputCallback(F):AddInputCallback(F)
        return true"#);
    p.count(1, 1);
    p.offline(false);
    p.compare("SCREEN:RemoveInputCallback(F);old=SCREEN;SCREEN={};RESET(SCREEN)");
    p.count(0, 0);
    p.offline(false);
    p.compare("old:AddInputCallback(F);assert(old:RemoveInputCallback(F)==old)");
    p.count(0, 0);
}

#[test]
fn errors_truthiness_and_deferred_removals_preserve_identity_order() {
    let mut p = Pair::new();
    p.ordered("if role=='low' then SCREEN:RemoveInputCallback(HIGH);error('input failure') elseif role=='mid' then return HANDLED end");
    p.compare(
        "SCREEN:AddInputCallback(HIGH):AddInputCallback(MID):AddInputCallback(LOW);HANDLED=false",
    );
    p.dispatch(
        &event(SongLuaInputType::FirstPress),
        false,
        &["input failure"],
    );
    p.compare("assert(table.concat(SEEN,',')=='low,mid,high')");
    p.count(2, 0);
    for truthy in ["0", "''", "{}", "function()end"] {
        p.compare(&format!(
            "SEEN={{}};HANDLED={truthy};SCREEN:AddInputCallback(HIGH)"
        ));
        p.dispatch(&event(SongLuaInputType::Repeat), true, &["input failure"]);
        p.compare("assert(table.concat(SEEN,',')=='low,mid')");
        p.count(2, 0);
    }
}

#[test]
fn additions_and_nested_removals_repair_the_dispatch_cursor() {
    let mut p = Pair::new();
    p.ordered("if role=='mid' and not ADDED then ADDED=true;SCREEN:AddInputCallback(LOW):AddInputCallback(HIGH) end;return false");
    p.compare("SCREEN:AddInputCallback(MID)");
    p.dispatch(&event(SongLuaInputType::FirstPress), false, &[]);
    p.compare("assert(table.concat(SEEN,',')=='mid,high');SEEN={}");
    p.dispatch(&event(SongLuaInputType::Repeat), false, &[]);
    p.compare("assert(table.concat(SEEN,',')=='low,mid,high');RESET(SCREEN)");
    p.ordered("if role=='mid' and not IN_NEST then IN_NEST=true;SCREEN:RemoveInputCallback(LOW);local handled,n=NESTED();assert(not handled and n==0) end;return false");
    p.compare("SCREEN:AddInputCallback(LOW):AddInputCallback(MID):AddInputCallback(HIGH)");
    p.dispatch(&event(SongLuaInputType::FirstPress), false, &[]);
    p.compare("assert(table.concat(SEEN,',')=='low,mid,low,mid,high,high')");
    p.count(2, 0);
}

#[test]
fn screen_replacement_keeps_active_callbacks_live_and_saved_methods_use_new_registry() {
    let mut p = Pair::new();
    p.ordered("if role=='mid' then old=SCREEN;SCREEN={};RESET(SCREEN);old:AddInputCallback(LOW):AddInputCallback(HIGH);collectgarbage('collect') end;return false");
    p.compare("SCREEN:AddInputCallback(MID)");
    p.dispatch(&event(SongLuaInputType::FirstPress), false, &[]);
    p.compare(
        "assert(table.concat(SEEN,',')=='mid,high');SEEN={};old=nil;collectgarbage('collect')",
    );
    p.dispatch(&event(SongLuaInputType::Repeat), false, &[]);
    p.compare("assert(table.concat(SEEN,',')=='low,high')");
    p.count(2, 0);
    p.offline(false);
}

#[test]
fn collected_registry_callbacks_and_screens_release_at_remove_and_rebind() {
    let mut p = Pair::new();
    p.compare(r#"WEAK=setmetatable({}, {__mode='v'});local captured={};local f=function(e) collectgarbage('collect');return captured and false end
        WEAK[1]=f;WEAK[2]=captured;WEAK[3]=SCREEN;SCREEN:AddInputCallback(f)
        SCREEN=nil;collectgarbage('collect');collectgarbage('collect');assert(WEAK[1] and WEAK[2] and not WEAK[3])"#);
    p.dispatch(&event(SongLuaInputType::Repeat), false, &[]);
    p.compare("SCREEN={};RESET(SCREEN);collectgarbage('collect');collectgarbage('collect');assert(WEAK[1]==nil and WEAK[2]==nil)");
    p.count(0, 0);
    p.compare("local f=function(e)return e.button=='Up' end;WEAK[1]=f;SCREEN:AddInputCallback(f);f=nil;collectgarbage('collect');assert(WEAK[1])");
    p.count(1, 1);
    p.compare("SCREEN:RemoveInputCallback(WEAK[1]);collectgarbage('collect');collectgarbage('collect');assert(WEAK[1]==nil)");
    p.count(0, 0);
}

#[test]
fn compiled_predicates_match_lua_for_all_event_fields_and_reject_side_effects() {
    let mut p = Pair::new();
    for source in [
        "return e.GameButton=='MenuUp'",
        "return e.button=='Down' or e.type=='InputEventType_Release'",
        "return e.controller=='GameController_4' and e.PlayerNumber=='PlayerNumber_P9'",
        "return e.MultiPlayer==nil and e.DeviceInput.down",
        "return e.DeviceInput.level*2>0.5 and e.DeviceInput.z<0",
        "return e.DeviceInput.ago<=0.125 and not e.DeviceInput.is_mouse",
        "return e.DeviceInput.device=='Keyboard' and e.DeviceInput.button=='Up'",
        "return e.DeviceInput.is_joystick or e.PlayerNumber==nil",
        "return 0",
        "return ''",
        "return nil,false",
    ] {
        p.compare(&format!(
            "RESET(SCREEN);F=function(e){source} end;SCREEN:AddInputCallback(F)"
        ));
        p.count(1, 1);
        // Fixed observations come from direct Lua execution. The detached
        // predicate also compiles bytecode authored by the Rust interpreter.
        for i in 0..4 {
            let mut e = event(if i == 2 {
                SongLuaInputType::Release
            } else {
                SongLuaInputType::FirstPress
            });
            e.player = [Some(0), None, Some(8), Some(1)][i];
            e.controller = Some(i);
            e.is_joystick = i == 3;
            e.is_mouse = i == 1;
            let expected =
                reference_test::source_bool("song/tests/rust_input.rs::predicate_result", source);
            p.dispatch(&e, expected, &[]);
            let bytes = dump_callback(&mut p.rust);
            let mut moved = CompiledInput::compile(&bytes).unwrap();
            assert_eq!(moved.evaluate(&e), expected);
        }
    }
    for source in [
        "e.button='changed';return false",
        "return GLOBAL",
        "return tostring(e.button)",
        "local t={};return t",
        "while true do return false end",
    ] {
        p.compare(&format!(
            "RESET(SCREEN);F=function(e){source} end;SCREEN:AddInputCallback(F)"
        ));
        let supported =
            reference_test::source_bool("song/tests/rust_input.rs::predicate_compilation", source);
        assert_eq!(
            test_support::input_compiled_count(&p.rust),
            usize::from(supported),
            "{source}"
        );
    }
    p.compare(
        "RESET(SCREEN);local capture=false;SCREEN:AddInputCallback(function(e)return capture end)",
    );
    p.count(1, 0);
}

#[test]
fn fully_compiled_dispatch_allocates_no_lua_event_and_hooks_force_fallback() {
    let mut p = Pair::new();
    p.compare("SCREEN:AddInputCallback(function(e)return e.GameButton=='MenuUp' end)");
    p.count(1, 1);
    p.dispatch(&event(SongLuaInputType::Repeat), true, &[]);
    let before = p.rust.vm_mut().heap_stats();
    for _ in 0..128 {
        p.dispatch(&event(SongLuaInputType::Repeat), true, &[]);
    }
    let after = p.rust.vm_mut().heap_stats();
    assert_eq!(before.tables, after.tables);
    assert_eq!(before.live_bytes, after.live_bytes);
    let hook = p
        .rust
        .vm_mut()
        .new_host_function(|call| Err(call.error(b"observed input hook")), &[])
        .unwrap();
    p.rust
        .vm_mut()
        .with_native_stack(|stack| stack.set_hook(stack.thread_id(), NativeHook::Host(hook), 1, 0))
        .unwrap();
    p.dispatch(
        &event(SongLuaInputType::Repeat),
        false,
        &["observed input hook"],
    );
    p.rust
        .vm_mut()
        .with_native_stack(|stack| stack.set_hook(stack.thread_id(), NativeHook::None, 0, 0))
        .unwrap();
    p.dispatch(&event(SongLuaInputType::Repeat), true, &[]);
}

#[test]
fn binding_a_screen_does_not_introduce_an_authored_call_for_hooks() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let mut p = Pair::new();
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
        .with_native_stack(|stack| stack.set_hook(stack.thread_id(), NativeHook::Host(hook), 1, 0))
        .unwrap();
    let screen = p.rust.vm_mut().try_new_table().unwrap();
    p.rust.bind_screen_input(Value::Table(screen)).unwrap();
    assert_eq!(rust.load(Ordering::SeqCst), 0);
}

#[test]
fn a_false_compiled_prefix_enters_lua_and_later_predicates_see_mutation() {
    let mut p = Pair::new();
    p.compare("POOL={};for i=1,64 do POOL[#POOL+1]=function(e)return e.button=='never' end;POOL[#POOL+1]=function(e)e.button='changed' end;POOL[#POOL+1]=function(e)return e.button=='changed' end end");
    // Choose the same semantic order using each heap's actual pointer order.
    fn choose<T: Copy>(mut entries: Vec<(usize, usize, T)>) -> [T; 3] {
        entries.sort_by_key(|e| e.0);
        let mut chosen = Vec::new();
        for (_, kind, value) in entries {
            if kind == chosen.len() {
                chosen.push(value);
                if chosen.len() == 3 {
                    break;
                }
            }
        }
        chosen
            .try_into()
            .unwrap_or_else(|_| panic!("interleaved identity candidates"))
    }
    let vm = p.rust.vm_mut();
    let Value::Table(pool) = vm.get_global(b"POOL").unwrap() else {
        panic!()
    };
    let mut entries = Vec::new();
    for i in 0..192 {
        let f = vm
            .raw_table_get(pool, Value::Number((i + 1) as f64))
            .unwrap();
        entries.push((vm.object_pointer(f).unwrap(), i % 3, f));
    }
    for (name, f) in [b"FIRST".as_slice(), b"MIDDLE", b"LAST"]
        .into_iter()
        .zip(choose(entries))
    {
        vm.set_global(name, f).unwrap();
    }
    p.compare("SCREEN:AddInputCallback(LAST):AddInputCallback(MIDDLE):AddInputCallback(FIRST)");
    p.count(3, 2);
    p.dispatch(&event(SongLuaInputType::Repeat), true, &[]);
    p.compare("SCREEN:RemoveInputCallback(MIDDLE)");
    p.count(2, 2);
    p.dispatch(&event(SongLuaInputType::Repeat), false, &[]);
}

#[test]
fn mixed_callbacks_share_mutations_and_removal_restores_compiled_dispatch() {
    let mut p = Pair::new();
    p.compare("READER=function(e)return e.button=='changed' end;MUTATOR=function(e)e.button='changed';SCREEN:RemoveInputCallback(MUTATOR) end;SCREEN:AddInputCallback(READER):AddInputCallback(MUTATOR)");
    p.count(2, 1);
    let vm = p.rust.vm_mut();
    let reader = vm.get_global(b"READER").unwrap();
    let mutator = vm.get_global(b"MUTATOR").unwrap();
    let expected = vm.object_pointer(mutator) < vm.object_pointer(reader);
    assert_eq!(
        p.rust
            .dispatch_input(&event(SongLuaInputType::Repeat))
            .unwrap(),
        (expected, vec![])
    );
    p.count(1, 1);
    p.dispatch(&event(SongLuaInputType::Repeat), false, &[]);
}

#[test]
fn resource_limit_unwinds_dispatch_and_applies_deferred_cleanup() {
    let mut p = Pair::new();
    p.compare("F=function(e) SCREEN:RemoveInputCallback(F);while true do end end;SCREEN:AddInputCallback(F)");
    p.rust.vm_mut().set_instruction_limit(Some(500));
    let error = p
        .rust
        .dispatch_input(&event(SongLuaInputType::Repeat))
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
    p.rust.vm_mut().set_instruction_limit(None);
    assert!(test_support::input_functions(&p.rust).unwrap().is_empty());
    assert_eq!(
        p.rust
            .dispatch_input(&event(SongLuaInputType::Repeat))
            .unwrap(),
        (false, vec![])
    );
}

fn dump_callback(runtime: &mut RustSongRuntime) -> Vec<u8> {
    let value = execute(runtime, "return string.dump(F)");
    let [Value::String(id)] = value.as_slice() else {
        panic!("callback bytecode expected")
    };
    runtime.vm_mut().string_bytes(*id).unwrap().to_vec()
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
