//! Original screen input unit fixtures over the Rust-owned callback registry.
use deadlib_lua::{NativeHook, Value};
use deadsync_song_lua::{
    SongLuaCompileContext, SongLuaInputEvent, SongLuaInputType, {SongRuntime, test_support},
};
#[path = "support/baked_program.rs"]
mod api;
fn setup() -> SongRuntime {
    let context = SongLuaCompileContext::new(".", "input unit fixtures");
    let mut host = SongRuntime::new(&context).unwrap();
    let screen = api::table(host.vm_mut());
    host.vm_mut().set_global(b"SCREEN", screen).unwrap();
    host.bind_screen_input(screen).unwrap();
    test_support::install_input_controls(&mut host, &event(SongLuaInputType::Repeat)).unwrap();
    host
}
fn execute(host: &mut SongRuntime, source: impl AsRef<[u8]>) -> Vec<Value> {
    api::eval(host.vm_mut(), source)
}
fn number(host: &mut SongRuntime, source: &str) -> f64 {
    api::as_number(execute(host, source)[0])
}
fn boolean(host: &mut SongRuntime, source: &str) -> bool {
    api::as_bool(execute(host, source)[0])
}
fn put(host: &mut SongRuntime, name: &[u8], value: Value) {
    host.vm_mut().set_global(name, value).unwrap();
}
fn identity(host: &mut SongRuntime, value: Value) -> usize {
    host.vm_mut().object_pointer(value).unwrap() as usize
}
fn register(host: &mut SongRuntime, function: Value) {
    let vm = host.vm_mut();
    let screen = vm.get_global(b"SCREEN").unwrap();
    let add = api::get(vm, screen, b"AddInputCallback");
    api::call(vm, add, &[screen, function]);
}
fn interpreted_count(host: &SongRuntime) -> usize {
    test_support::input_functions(host).unwrap().len() - test_support::input_compiled_count(host)
}
fn seen(host: &mut SongRuntime) -> Vec<usize> {
    let vm = host.vm_mut();
    let Value::Table(table) = vm.get_global(b"SEEN").unwrap() else {
        panic!("seen table")
    };
    (1..=vm.raw_table_len(table).unwrap())
        .map(|i| api::as_number(vm.raw_table_get(table, Value::Number(i as f64)).unwrap()) as usize)
        .collect()
}

fn event(kind: SongLuaInputType) -> SongLuaInputEvent<'static> {
    SongLuaInputEvent {
        player: Some(1),
        controller: Some(1),
        button: "Up",
        game_button: "MenuUp",
        kind,
        device: "InputDevice_Key",
        device_button: "DeviceButton_up",
        level: 1.0,
        z: 0,
        down: kind != SongLuaInputType::Release,
        ago: 0.0,
        is_joystick: false,
        is_mouse: false,
    }
}

#[test]
fn screen_callbacks_receive_press_repeat_release_and_deduplicate_registration() {
    let mut host = setup();
    execute(
        &mut host,
        r#"
SEEN = {}
CALLBACK = function(event)
    assert(event.button == 'Up' and event.GameButton == 'MenuUp')
    assert(event.PlayerNumber == 'PlayerNumber_P2' and event.controller == 'GameController_2')
    assert(event.DeviceInput.button == 'DeviceButton_up')
    assert(event.DeviceInput.down == (event.type ~= 'InputEventType_Release'))
    SEEN[#SEEN+1] = event.type
end
SCREEN:AddInputCallback(CALLBACK):AddInputCallback(CALLBACK)
"#,
    );
    for kind in [
        SongLuaInputType::FirstPress,
        SongLuaInputType::Repeat,
        SongLuaInputType::Release,
    ] {
        let (handled, errors) = host.dispatch_input(&event(kind)).unwrap();
        assert!(!handled && errors.is_empty(), "{errors:?}");
    }
    assert_eq!(number(&mut host, "return #SEEN"), 3.);
    execute(&mut host, "SCREEN:RemoveInputCallback(CALLBACK)");
    host.dispatch_input(&event(SongLuaInputType::FirstPress))
        .unwrap();
    assert_eq!(number(&mut host, "return #SEEN"), 3.);
    let next = api::table(host.vm_mut());
    host.bind_screen_input(next).unwrap();
    assert!(
        test_support::input_functions(&host).unwrap().is_empty(),
        "new screen replaces callbacks"
    );
}

#[test]
fn screen_callbacks_defer_removals_share_event_and_use_lua_truthiness() {
    let mut host = setup();
    let create = execute(
        &mut host,
        r#"return function()
    local callback
    callback = function(e)
        if callback == FIRST then
            COUNT=COUNT+1; e.mark=true
            SCREEN:RemoveInputCallback(SECOND)
            return HANDLED
        end
        assert(e.mark); COUNT=COUNT+10
    end
    return callback
end"#,
    )[0];
    let first = api::call_one(host.vm_mut(), create, &[]);
    let second = api::call_one(host.vm_mut(), create, &[]);
    let (low, high) = if identity(&mut host, first) < identity(&mut host, second) {
        (first, second)
    } else {
        (second, first)
    };
    put(&mut host, b"FIRST", low);
    put(&mut host, b"SECOND", high);
    put(&mut host, b"COUNT", Value::Number(0.));
    put(&mut host, b"HANDLED", Value::Boolean(false));
    execute(
        &mut host,
        "SCREEN:AddInputCallback(SECOND):AddInputCallback(FIRST)",
    );
    let (handled, errors) = host
        .dispatch_input(&event(SongLuaInputType::FirstPress))
        .unwrap();
    assert!(!handled && errors.is_empty(), "{errors:?}");
    assert_eq!(number(&mut host, "return COUNT"), 11.);
    put(&mut host, b"HANDLED", Value::Number(0.));
    assert!(
        host.dispatch_input(&event(SongLuaInputType::Repeat))
            .unwrap()
            .0,
        "zero is truthy in Lua"
    );
    assert_eq!(number(&mut host, "return COUNT"), 12.);
}

#[test]
fn closed_callbacks_use_native_dispatch_and_debug_hooks_keep_lua_execution() {
    let mut host = setup();
    execute(
        &mut host,
        "SCREEN:AddInputCallback(function(e) return e.GameButton=='MenuUp' end)",
    );
    assert_eq!(interpreted_count(&host), 0);
    assert_eq!(
        test_support::input_compiled_count(&host),
        test_support::input_functions(&host).unwrap().len()
    );
    for _ in 0..3 {
        let (handled, errors) = host
            .dispatch_input(&event(SongLuaInputType::Repeat))
            .unwrap();
        assert!(handled && errors.is_empty());
    }
    let hook = host
        .vm_mut()
        .new_host_function(|call| Err(call.error(b"observed callback hook")), &[])
        .unwrap();
    host.vm_mut()
        .with_native_stack(|stack| stack.set_hook(stack.thread_id(), NativeHook::Host(hook), 1, 0))
        .unwrap();
    let (handled, errors) = host
        .dispatch_input(&event(SongLuaInputType::Repeat))
        .unwrap();
    assert!(!handled);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("observed callback hook"));
    host.vm_mut()
        .with_native_stack(|stack| stack.set_hook(stack.thread_id(), NativeHook::None, 0, 0))
        .unwrap();
    assert!(
        host.dispatch_input(&event(SongLuaInputType::Repeat))
            .unwrap()
            .0
    );
}

#[test]
fn mixed_callbacks_see_mutated_events_and_removals_restore_native_eligibility() {
    let mut host = setup();
    let reader = execute(
        &mut host,
        "return function(e) return e.button=='changed' end",
    )[0];
    let mutator = execute(
        &mut host,
        "return function(e) e.button='changed';SCREEN:RemoveInputCallback(MUTATOR) end",
    )[0];
    let mutated_before_read = identity(&mut host, mutator) < identity(&mut host, reader);
    put(&mut host, b"MUTATOR", mutator);
    register(&mut host, reader);
    register(&mut host, mutator);
    assert_eq!(interpreted_count(&host), 1);
    let (handled, errors) = host
        .dispatch_input(&event(SongLuaInputType::FirstPress))
        .unwrap();
    assert_eq!(handled, mutated_before_read);
    assert!(errors.is_empty());
    assert_eq!(interpreted_count(&host), 0);
    assert!(
        !host
            .dispatch_input(&event(SongLuaInputType::Repeat))
            .unwrap()
            .0
    );
}

#[test]
fn native_prefix_stopping_preserves_later_lua_action_order() {
    let mut host = setup();
    execute(&mut host, "TARGET={visible=true}");
    let predicate = execute(
        &mut host,
        "return function(e) return e.GameButton=='MenuUp' end",
    )[0];
    let action = execute(&mut host, "return function(e) TARGET.visible=false end")[0];
    let predicate_first = identity(&mut host, predicate) < identity(&mut host, action);
    register(&mut host, predicate);
    register(&mut host, action);
    let (handled, errors) = host
        .dispatch_input(&event(SongLuaInputType::FirstPress))
        .unwrap();
    assert!(handled && errors.is_empty());
    assert_eq!(boolean(&mut host, "return TARGET.visible"), predicate_first);
    let mut alternate = event(SongLuaInputType::Release);
    alternate.game_button = "MenuLeft";
    let (handled, errors) = host.dispatch_input(&alternate).unwrap();
    assert!(!handled && errors.is_empty());
    assert!(!boolean(&mut host, "return TARGET.visible"));
}

#[test]
fn reentrant_dispatch_keeps_existing_pending_removal_protocol() {
    let mut host = setup();
    let f = execute(&mut host, "return function(e) return true end")[0];
    register(&mut host, f);
    test_support::seed_pending_input_removal(&mut host, f);
    assert!(
        host.dispatch_input(&event(SongLuaInputType::FirstPress))
            .unwrap()
            .0
    );
    let (dispatching, removals, functions) = test_support::input_dispatch_state(&host);
    assert!(!dispatching && removals == 0);
    assert_eq!(functions, 0);
}

#[test]
fn additions_on_both_sides_of_current_callback_keep_native_identity_order() {
    let mut host = setup();
    let empty = api::table(host.vm_mut());
    put(&mut host, b"SEEN", empty);
    let mut functions=(0..12).map(|id|{let f=execute(&mut host,format!("return function(e) SEEN[#SEEN+1]={id};if START_ID=={id} and not ADDED then ADDED=true;SCREEN:AddInputCallback(LOW):AddInputCallback(HIGH) end;return false end"))[0];(id,f)}).collect::<Vec<_>>();
    functions.sort_by_key(|(_, f)| identity(&mut host, *f));
    let (low_id, low) = functions[0];
    let (mid_id, mid) = functions[6];
    let (high_id, high) = functions[11];
    put(&mut host, b"LOW", low);
    put(&mut host, b"START_ID", Value::Number(mid_id as f64));
    put(&mut host, b"HIGH", high);
    register(&mut host, mid);
    assert!(
        !host
            .dispatch_input(&event(SongLuaInputType::FirstPress))
            .unwrap()
            .0
    );
    assert_eq!(seen(&mut host), [mid_id, high_id]);
    let empty = api::table(host.vm_mut());
    put(&mut host, b"SEEN", empty);
    assert!(
        !host
            .dispatch_input(&event(SongLuaInputType::Repeat))
            .unwrap()
            .0
    );
    assert_eq!(seen(&mut host), [low_id, mid_id, high_id]);
}

#[test]
fn nested_dispatch_removing_earlier_entries_repairs_outer_position() {
    let mut host = setup();
    let empty = api::table(host.vm_mut());
    put(&mut host, b"SEEN", empty);
    let mut functions=(0..12).map(|id|{let f=execute(&mut host,format!("return function(e) SEEN[#SEEN+1]={id};if START_ID=={id} and not IN_NEST then IN_NEST=true;SCREEN:RemoveInputCallback(LOW);NESTED() end;return false end"))[0];(id,f)}).collect::<Vec<_>>();
    functions.sort_by_key(|(_, f)| identity(&mut host, *f));
    let (low_id, low) = functions[0];
    let (mid_id, mid) = functions[6];
    let (high_id, high) = functions[11];
    put(&mut host, b"LOW", low);
    put(&mut host, b"START_ID", Value::Number(mid_id as f64));
    for f in [low, mid, high] {
        register(&mut host, f);
    }
    assert!(
        !host
            .dispatch_input(&event(SongLuaInputType::FirstPress))
            .unwrap()
            .0
    );
    assert_eq!(
        seen(&mut host),
        [low_id, mid_id, low_id, mid_id, high_id, high_id]
    );
}

#[test]
fn compiled_registry_keeps_function_identity_rooted_until_removal() {
    let mut host = setup();
    execute(
        &mut host,
        "WEAK=setmetatable({}, {__mode='v'});local f=function(e) return e.button=='Up' end;WEAK[1]=f;SCREEN:AddInputCallback(f)",
    );
    execute(&mut host, "collectgarbage('collect')");
    assert!(boolean(&mut host, "return WEAK[1]~=nil"));
    execute(&mut host, "SCREEN:RemoveInputCallback(WEAK[1])");
    execute(&mut host, "collectgarbage('collect')");
    assert!(boolean(&mut host, "return WEAK[1]==nil"));
    assert!(test_support::input_functions(&host).unwrap().is_empty());
}
