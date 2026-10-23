//! Test the production input module without building the Song --lib harness.
#[path = "../src/input.rs"]
mod screen_input;

use deadsync_song_lua::{SongLuaCompiledInput, SongLuaInputEvent, SongLuaInputType};
use mlua::{Function, Lua, Value};
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
            let lua = Lua::new();
            lua.set_app_data(Dropped(dropped.clone()));
            let f: Function = lua
                .load(format!("return function(e) {body} end"))
                .eval()
                .unwrap();
            let program = SongLuaCompiledInput::compile(&f).expect(body);
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
                    let native_input = screen_input::SongLuaInputEvent {
                        player: input.player,
                        controller: input.controller,
                        button: input.button,
                        game_button: input.game_button,
                        kind: match input.kind {
                            SongLuaInputType::FirstPress => {
                                screen_input::SongLuaInputType::FirstPress
                            }
                            SongLuaInputType::Repeat => screen_input::SongLuaInputType::Repeat,
                            SongLuaInputType::Release => screen_input::SongLuaInputType::Release,
                        },
                        device: input.device,
                        device_button: input.device_button,
                        level: input.level,
                        z: input.z,
                        down: input.down,
                        ago: input.ago,
                        is_joystick: input.is_joystick,
                        is_mouse: input.is_mouse,
                    };
                    let event = screen_input::create_event(&lua, &native_input).unwrap();
                    let value = f.call::<Value>(event).unwrap();
                    expected.push(!matches!(value, Value::Nil | Value::Boolean(false)));
                    sequence.push(input);
                }
            }
            (program, sequence, expected)
        };
        assert!(
            dropped.load(Relaxed),
            "native input binding retained its loading VM"
        );
        for (input, expected) in sequence.iter().zip(expected) {
            assert_eq!(program.evaluate(input), expected, "{body}");
        }
    }
}

#[test]
fn unsupported_state_calls_coercions_and_event_mutation_keep_fallback() {
    let lua = Lua::new();
    for source in [
        "local held=false;return function(e) held=not held;return held end",
        "return function(e) e.button='other';return true end",
        "return function(e) return e.DeviceInput.level+'1' end",
        "return function(e) return type(e.button) end",
        "return function(e) return e.DeviceInput==e.DeviceInput end",
        "return function(e) while e.button=='Right' do end end",
    ] {
        let f: Function = lua.load(source).eval().unwrap();
        assert!(SongLuaCompiledInput::compile(&f).is_none(), "{source}");
    }
}
