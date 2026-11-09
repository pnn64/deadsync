//! Compiled predicates against ordinary authored Lua in the Rust interpreter.
use deadlib_lua::{HostCall, TableId, Value, Vm, compile};
use deadsync_song_lua::{SongLuaInputEvent, SongLuaInputType, rust_engine::CompiledInput};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering::Relaxed},
};

fn event() -> SongLuaInputEvent<'static> {
    SongLuaInputEvent {
        player: Some(0),
        controller: Some(0),
        button: "Right",
        game_button: "Right",
        kind: SongLuaInputType::FirstPress,
        device: "InputDevice_Key",
        device_button: "DeviceButton_right",
        level: 1.0,
        z: 0,
        down: true,
        ago: 0.0,
        is_joystick: false,
        is_mouse: false,
    }
}

struct Dropped(Arc<AtomicBool>);
impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.store(true, Relaxed);
    }
}

fn callback(vm: &mut Vm, source: &str) -> Value {
    vm.execute(&compile(source.as_bytes()).unwrap()).unwrap()[0]
}

fn dump(vm: &mut Vm, function: Value) -> Vec<u8> {
    vm.set_global(b"fixture_callback", function).unwrap();
    let Value::String(bytes) = callback(vm, "return string.dump(fixture_callback)") else {
        panic!("dumped callback bytes")
    };
    vm.string_bytes(bytes).unwrap().to_vec()
}

// Construct the authored-Lua reference event independently of the predicate's
// flattened input bindings. In particular, nested DeviceInput is an actual
// table, optional player/controller values remain nil, and strings are bytes.
fn authored_event(vm: &mut Vm, input: &SongLuaInputEvent<'_>) -> Value {
    fn put(vm: &mut Vm, owner: TableId, name: &[u8], value: Value) {
        let key = vm.intern(name);
        vm.raw_table_set(owner, key, value).unwrap();
    }
    fn text(vm: &mut Vm, owner: TableId, name: &[u8], value: &str) {
        let value = vm.intern(value.as_bytes());
        put(vm, owner, name, value);
    }
    let event = vm.try_new_table().unwrap();
    let device = vm.try_new_table().unwrap();
    text(vm, device, b"device", input.device);
    text(vm, device, b"button", input.device_button);
    put(vm, device, b"level", Value::Number(input.level.into()));
    put(vm, device, b"z", Value::Number(input.z.into()));
    put(vm, device, b"down", Value::Boolean(input.down));
    put(vm, device, b"ago", Value::Number(input.ago));
    put(
        vm,
        device,
        b"is_joystick",
        Value::Boolean(input.is_joystick),
    );
    put(vm, device, b"is_mouse", Value::Boolean(input.is_mouse));
    put(vm, event, b"DeviceInput", Value::Table(device));
    text(vm, event, b"button", input.button);
    text(vm, event, b"GameButton", input.game_button);
    text(
        vm,
        event,
        b"type",
        match input.kind {
            SongLuaInputType::FirstPress => "InputEventType_FirstPress",
            SongLuaInputType::Repeat => "InputEventType_Repeat",
            SongLuaInputType::Release => "InputEventType_Release",
        },
    );
    if let Some(player) = input.player {
        text(
            vm,
            event,
            b"PlayerNumber",
            &format!("PlayerNumber_P{}", player + 1),
        );
    }
    if let Some(controller) = input.controller {
        text(
            vm,
            event,
            b"controller",
            &format!("GameController_{}", controller + 1),
        );
    }
    Value::Table(event)
}

#[test]
fn automatic_input_binding_matches_authored_lua_then_runs_without_its_vm() {
    let bodies = [
        "return e.GameButton=='Right' and e.type~='InputEventType_Release'",
        "return e.PlayerNumber=='PlayerNumber_P1' and e.controller=='GameController_1'",
        "return e.controller or e.PlayerNumber or false",
        "return e.DeviceInput.down and e.DeviceInput.level>=0.5",
        "return e.DeviceInput.z < 0 or e.DeviceInput.ago > 0",
        "return e.DeviceInput.device=='InputDevice_Key' and e.DeviceInput.button=='DeviceButton_right'",
        "return e.DeviceInput.is_mouse or e.DeviceInput.is_joystick",
        "return e.MultiPlayer==nil",
        "local value;if e.DeviceInput.down then value=e.button end;return value",
        "return e.button=='\\0\\255'", // comparison is bytes, never UTF-8 decoding
        "return e.DeviceInput",
        "return e",
        "return 0",
        "return ''",
        "return nil",
    ];
    for body in bodies {
        let dropped = Arc::new(AtomicBool::new(false));
        let (mut program, sequence, expected) = {
            let mut vm = Vm::new();
            let marker = vm
                .new_host_function_with_data(
                    |_: &mut HostCall<'_>| Ok(()),
                    &[],
                    Dropped(dropped.clone()),
                )
                .unwrap();
            vm.set_global(b"fixture_lifetime", marker).unwrap();
            let f = callback(&mut vm, &format!("return function(e) {body} end"));
            let bytes = dump(&mut vm, f);
            let program = CompiledInput::compile(&bytes).expect(body);
            let mut sequence = Vec::new();
            let mut expected = Vec::new();
            for repeat in 0..3 {
                for (index, player) in [None, Some(0), Some(1), Some(usize::MAX - 1)]
                    .into_iter()
                    .enumerate()
                {
                    let mut input = event();
                    input.player = player;
                    input.controller = player;
                    input.kind = match (repeat + index) % 3 {
                        0 => SongLuaInputType::FirstPress,
                        1 => SongLuaInputType::Repeat,
                        _ => SongLuaInputType::Release,
                    };
                    input.down = input.kind != SongLuaInputType::Release;
                    input.button = if index % 2 == 0 { "Left" } else { "Right" };
                    input.game_button = input.button;
                    input.level = [-0.0, 0.25, 1.0, f32::NAN][index];
                    input.ago = [-0.0, 0.0, 0.25, f64::NAN][index];
                    input.z = index as i32 - 2;
                    input.is_joystick = index == 1;
                    input.is_mouse = index == 2;
                    let event = authored_event(&mut vm, &input);
                    let value = vm
                        .call(f, &[event])
                        .unwrap()
                        .first()
                        .copied()
                        .unwrap_or(Value::Nil);
                    expected.push(!matches!(value, Value::Nil | Value::Boolean(false)));
                    sequence.push(input);
                }
            }
            (program, sequence, expected)
        };
        assert!(
            dropped.load(Relaxed),
            "compiled input binding retained its loading VM"
        );
        for (input, expected) in sequence.iter().zip(expected) {
            assert_eq!(program.evaluate(input), expected, "{body}");
        }
    }
}

#[test]
fn unsupported_state_calls_coercions_and_event_mutation_keep_fallback() {
    let mut vm = Vm::new();
    for source in [
        "local held=false;return function(e) held=not held;return held end",
        "return function(e) e.button='other';return true end",
        "return function(e) return e.DeviceInput.level+'1' end",
        "return function(e) return type(e.button) end",
        "return function(e) return e.DeviceInput==e.DeviceInput end",
        "return function(e) while e.button=='Right' do end end",
    ] {
        let f = callback(&mut vm, source);
        let bytes = dump(&mut vm, f);
        assert!(CompiledInput::compile(&bytes).is_none(), "{source}");
    }
}
