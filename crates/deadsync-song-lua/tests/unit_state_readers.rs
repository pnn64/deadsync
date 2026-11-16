//! Original state-reader fixtures, with independent fixed native observations.
use deadlib_lua::{Error, HostCall, NativeCallStatus, Root, Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, SongLuaOverlayState, {SongRuntime, test_support},
};
use std::sync::{Arc, Mutex};
#[path = "support/baked_program.rs"]
mod api;
#[path = "support/state_reader_errors.rs"]
mod errors;
#[expect(
    dead_code,
    reason = "this fixture uses a subset of the shared snapshot API"
)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
fn runtime() -> SongRuntime {
    SongRuntime::new(&SongLuaCompileContext::new(".", "state reader unit")).unwrap()
}
struct StateReader {
    function: Root,
    output: test_support::FixtureStateOutput,
}
fn state_reader(host: &mut SongRuntime, actor: Value, live: bool) -> StateReader {
    let (function, output) = test_support::prepare_actor_state_reader(host, actor, live).unwrap();
    StateReader { function, output }
}
fn read(host: &mut SongRuntime, reader: &StateReader) -> Result<SongLuaOverlayState, String> {
    let vm = host.vm_mut();
    let function = vm.rooted(reader.function).unwrap();
    vm.call(function, &[]).unwrap();
    reader.output.lock().unwrap().take().unwrap()
}
fn set(host: &mut SongRuntime, actor: Value, key: &str, value: Value) {
    api::set(host.vm_mut(), actor, key.as_bytes(), value)
}
type TextOutput = Arc<Mutex<Option<Result<Option<String>, String>>>>;
fn text_reader(call: &mut HostCall<'_>) -> Result<(), Error> {
    let output = call.function_data::<TextOutput>().unwrap().clone();
    let Value::Table(actor) = call.captures()[2] else {
        unreachable!()
    };
    let key = call.try_intern(b"__songlua_actor_type")?;
    let value = call.raw_table_get(actor, key)?;
    let result = test_support::fixture_field_text(call, value);
    let result = match result {
        Err(error)
            if matches!(
                error.kind(),
                deadlib_lua::ErrorKind::Limit | deadlib_lua::ErrorKind::Memory
            ) =>
        {
            return Err(error);
        }
        result => result,
    };
    *output.lock().unwrap() = Some(result.map_err(|error| error.to_string()));
    Ok(())
}
fn named_actor(host: &mut SongRuntime, source: &str) -> Value {
    let code = compile_named(source.as_bytes(), b"texture-mutation").unwrap();
    let actor = host.vm_mut().execute(&code).unwrap()[0];
    host.vm_mut().pin(actor).unwrap();
    actor
}
fn continuation(host: &mut SongRuntime) -> u32 {
    api::as_number(api::eval(host.vm_mut(), "return 6*7")[0]) as u32
}

#[test]
fn state_class_and_texture_reads_match_frozen_decoder() {
    let mut host = runtime();
    for field in ["__songlua_actor_type", "Texture"] {
        let vm = host.vm_mut();
        let values = [
            Value::Nil,
            api::string(vm, "Model"),
            api::string(vm, "Sprite"),
            api::string(vm, "x".repeat(200)),
            api::string(vm, [0xff, 0xfe]),
            Value::Number(42.),
            Value::Number(1.5),
            Value::Boolean(false),
            api::table(vm),
        ];
        for value in values {
            let actor = api::table(host.vm_mut());
            let kind = api::string(host.vm_mut(), "Sprite");
            set(&mut host, actor, "__songlua_actor_type", kind);
            set(&mut host, actor, field, value);
            let reader = state_reader(&mut host, actor, false);
            let actual = read(&mut host, &reader);
            reference_test::assert_debug(
                "song/unit/state_readers::state_class_and_texture_reads_match_frozen_decoder::assert_0",
                &errors::result(&actual),
            );
        }
    }
}
#[test]
fn short_class_fields_keep_string_coercions_spills_and_errors() {
    let mut host = runtime();
    let actor = api::table(host.vm_mut());
    let output: TextOutput = Arc::new(Mutex::new(None));
    let function =
        test_support::prepare_actor_reader(&mut host, actor, text_reader, output.clone()).unwrap();
    let vm = host.vm_mut();
    let values = [
        Value::Nil,
        api::string(vm, "ActorFrame"),
        api::string(vm, "x".repeat(200)),
        api::string(vm, [0xff, 0xfe]),
        Value::Number(42.),
        Value::Number(1.5),
        Value::Boolean(false),
        api::table(vm),
    ];
    for value in values {
        set(&mut host, actor, "__songlua_actor_type", value);
        let vm = host.vm_mut();
        let function = vm.rooted(function).unwrap();
        vm.call(function, &[]).unwrap();
        let actual = output.lock().unwrap().take().unwrap();
        reference_test::assert_debug(
            "song/unit/state_readers::short_class_fields_keep_string_coercions_spills_and_errors::assert_0",
            &errors::result(&actual),
        );
    }
}
#[test]
fn live_scalar_read_preserves_defaults_coercions_and_errors() {
    let mut host = runtime();
    let actor = api::table(host.vm_mut());
    let reader = state_reader(&mut host, actor, false);
    reference_test::assert_debug(
        "song/unit/state_readers::live_scalar_read_preserves_defaults_coercions_and_errors::assert_0",
        &read(&mut host, &reader).unwrap(),
    );
    let numeric_string = api::string(host.vm_mut(), " 1.25 ");
    for numeric in [Value::Number(0.75), numeric_string] {
        for key in NUMBER_KEYS {
            set(&mut host, actor, key, numeric);
        }
        let empty = api::string(host.vm_mut(), "");
        for truthy in [
            Value::Boolean(false),
            Value::Boolean(true),
            Value::Number(0.0),
            empty,
        ] {
            for key in FLAG_KEYS {
                set(&mut host, actor, key, truthy);
            }
            let actual = read(&mut host, &reader).unwrap();
            reference_test::assert_debug(
                "song/unit/state_readers::live_scalar_read_preserves_defaults_coercions_and_errors::assert_1",
                &actual,
            );
            if matches!(truthy, Value::Number(_)) {
                assert!(actual.visible, "Lua zero is truthy");
            }
            assert_eq!(continuation(&mut host), 42);
        }
    }
    for key in NUMBER_KEYS {
        set(&mut host, actor, key, Value::Nil);
    }
    set(
        &mut host,
        actor,
        "__songlua_state_x",
        Value::Number(f32::NAN as f64),
    );
    set(
        &mut host,
        actor,
        "__songlua_state_z",
        Value::Number(f32::INFINITY as f64),
    );
    let mut actual = read(&mut host, &reader).unwrap();
    reference_test::assert_debug(
        "song/unit/state_readers::live_scalar_read_preserves_defaults_coercions_and_errors::assert_3",
        &actual.x.to_bits(),
    );
    actual.x = 0.0;
    reference_test::assert_debug(
        "song/unit/state_readers::live_scalar_read_preserves_defaults_coercions_and_errors::assert_4",
        &actual,
    );
    let bad = api::table(host.vm_mut());
    set(&mut host, actor, "__songlua_state_x", bad);
    let error = read(&mut host, &reader).unwrap_err();
    reference_test::assert_debug(
        "song/unit/state_readers::live_scalar_read_preserves_defaults_coercions_and_errors::assert_5",
        &errors::error_cause(&error),
    );
    assert_eq!(continuation(&mut host), 42);
}
#[test]
fn overlay_reads_preserve_conversions_errors_and_stack() {
    let mut host = runtime();
    let actor = api::table(host.vm_mut());
    let reader = state_reader(&mut host, actor, false);
    let vm = host.vm_mut();
    let foreign = vm.try_new_userdata(std::mem::size_of::<u32>()).unwrap();
    let Value::Userdata(id) = foreign else {
        unreachable!()
    };
    vm.userdata_bytes_mut(id)
        .unwrap()
        .copy_from_slice(&7_u32.to_ne_bytes());
    vm.pin(foreign).unwrap();
    let values = [
        Value::Nil,
        Value::Boolean(false),
        Value::Boolean(true),
        Value::Number(7.),
        Value::Number(-0.0),
        Value::Number(f64::INFINITY),
        api::string(vm, " 2.5 "),
        api::string(vm, "x".repeat(200)),
        api::string(vm, [0xff, 0xfe]),
        api::eval(vm, "return {1,'2',3,4,5}")[0],
        api::eval(vm, "return {{1,2,3,4},{1,2,3,4},{1,2,3,4},{1,2,3,4}}")[0],
        foreign,
    ];
    let keys = OVERLAY_KEYS
        .iter()
        .map(|key| api::string(vm, key))
        .collect::<Vec<_>>();
    let function = vm.rooted(reader.function).unwrap();
    vm.with_native_stack(|stack|{
        stack.set_top(0)?;assert!(stack.check_stack(8));stack.push(Value::Number(42.))?;let top=stack.top();
        for(key_index,key)in keys.iter().copied().enumerate(){
            for value in values{
                stack.push(actor)?;stack.push(key)?;stack.push(value)?;stack.set_table(-3,true)?;stack.set_top(top)?;
                stack.push(function)?;assert_eq!(stack.protected_call(0,Some(0),None)?,NativeCallStatus::Ok);let actual=reader.output.lock().unwrap().take().unwrap();
                reference_test::assert_debug("song/unit/state_readers::overlay_reads_preserve_conversions_errors_and_stack::assert_0",&errors::result(&actual));assert_eq!(stack.top(),top,"{}",OVERLAY_KEYS[key_index]);
            }
            stack.push(actor)?;stack.push(key)?;stack.push(Value::Nil)?;stack.set_table(-3,true)?;stack.set_top(top)?;
        }
        assert_eq!(stack.value(-1),Some(Value::Number(42.)));stack.set_top(top-1)?;Ok(())
    }).unwrap();
    set(
        &mut host,
        actor,
        "__songlua_state_draw_order",
        Value::Boolean(false),
    );
    set(
        &mut host,
        actor,
        "__songlua_state_shadow_len",
        Value::Number(7.),
    );
    let actual = read(&mut host, &reader);
    reference_test::assert_debug(
        "song/unit/state_readers::overlay_reads_preserve_conversions_errors_and_stack::assert_3",
        &errors::result(&actual),
    );
    assert_eq!(continuation(&mut host), 42);
}
#[test]
fn overlay_reads_follow_authored_texture_mutations() {
    let mut host = runtime();
    for fail in [false, true] {
        host.vm_mut()
            .set_global(b"FAIL", Value::Boolean(fail))
            .unwrap();
        host.vm_mut()
            .set_global(b"READS", Value::Number(0.))
            .unwrap();
        let before = owned_mutation_actor(&mut host, TEXTURE_MUTATION_SOURCE);
        let after = owned_mutation_actor(&mut host, TEXTURE_MUTATION_SOURCE);
        let before_reader = state_reader(&mut host, before, true);
        let after_reader = state_reader(&mut host, after, true);
        let control = read(&mut host, &before_reader);
        reference_test::assert_debug(
            "song/unit/state_readers::overlay_reads_follow_authored_texture_mutations::control_actor",
            &errors::result(&control),
        );
        let actual = read(&mut host, &after_reader);
        reference_test::assert_debug(
            "song/unit/state_readers::overlay_reads_follow_authored_texture_mutations::assert_0",
            &errors::result(&actual),
        );
        assert_eq!(
            api::as_number(host.vm_mut().get_global(b"READS").unwrap()) as i32,
            2
        );
        if !fail {
            let state = actual.unwrap();
            assert_eq!(state.x, 7.0, "scalar snapshot precedes authored lookup");
            assert_eq!(state.draw_order, 17);
            assert_eq!(state.shadow_len, [3.0, 5.0]);
            assert_eq!(state.size, Some([12.0, 24.0]));
        }
    }
}

const TEXTURE_MUTATION_SOURCE: &str = r#"
return setmetatable({__songlua_actor_type='Sprite', __songlua_state_x=7}, {
    __index = function(self, key)
        assert(key == 'Texture', 'unexpected authored property read: '..key)
        READS = READS + 1
        self.__songlua_state_x = 99
        self.__songlua_state_draw_order = 17
        self.__songlua_state_shadow_len = {3, 5}
        self.__songlua_state_size = {12, 24}
        self.__songlua_state_blend = 'add'
        if FAIL then error('authored texture failure') end
        return 'texture.png'
    end,
})
"#;
const NUMBER_KEYS: &[&str] = &[
    "__songlua_hibernate_seconds",
    "__songlua_state_aux",
    "__songlua_state_x",
    "__songlua_state_y",
    "__songlua_state_z",
    "__songlua_state_z_bias",
    "__songlua_state_halign",
    "__songlua_state_valign",
    "__songlua_state_fov",
    "FOV",
    "__songlua_state_cropleft",
    "__songlua_state_cropright",
    "__songlua_state_croptop",
    "__songlua_state_cropbottom",
    "__songlua_state_fadeleft",
    "__songlua_state_faderight",
    "__songlua_state_fadetop",
    "__songlua_state_fadebottom",
    "__songlua_state_zoom",
    "__songlua_state_zoom_x",
    "__songlua_state_zoom_y",
    "__songlua_state_zoom_z",
    "__songlua_state_basezoom",
    "__songlua_state_basezoom_x",
    "__songlua_state_basezoom_y",
    "__songlua_state_basezoom_z",
    "__songlua_state_base_alpha",
    "__songlua_state_rot_x_deg",
    "__songlua_state_rot_y_deg",
    "__songlua_state_rot_z_deg",
    "__songlua_state_skew_x",
    "__songlua_state_skew_y",
    "__songlua_state_effect_period",
    "__songlua_state_effect_offset",
    "__songlua_state_text_distortion",
    "__songlua_state_sprite_playback_rate",
    "__songlua_state_sprite_state_delay",
    "__songlua_state_max_width",
    "__songlua_state_max_height",
];
const FLAG_KEYS: &[&str] = &[
    "__songlua_theme_hibernating",
    "__songlua_state_sprite_texture",
    "__songlua_visible",
    "__songlua_state_draw_by_z_position",
    "__songlua_state_aft_preserve",
    "__songlua_state_aft_created",
    "__songlua_state_uppercase",
    "__songlua_state_mask_source",
    "__songlua_state_mask_dest",
    "__songlua_state_depth_test",
    "__songlua_state_depth_write",
    "__songlua_state_depth_test_on_fail",
    "__songlua_state_vibrate",
    "__songlua_state_effect_timer",
    "__songlua_state_effect_no_offset",
    "__songlua_state_rainbow",
    "__songlua_state_rainbow_scroll",
    "__songlua_state_text_jitter",
    "__songlua_state_mult_attrs_with_diffuse",
    "__songlua_state_sprite_animate",
    "__songlua_state_sprite_loop",
    "__songlua_state_max_w_pre_zoom",
    "__songlua_state_max_h_pre_zoom",
    "__songlua_state_max_dimension_uses_zoom",
    "__songlua_state_decode_movie",
    "__songlua_state_texture_filtering",
    "__songlua_state_texture_wrapping",
];
const OVERLAY_KEYS: &[&str] = &[
    "__songlua_state_diffuse",
    "__songlua_state_vertex_colors",
    "__songlua_state_draw_order",
    "__songlua_state_text_align",
    "__songlua_state_shadow_len",
    "__songlua_state_shadow_color",
    "__songlua_state_glow",
    "__songlua_state_vanishpoint",
    "__songlua_state_base_rotation",
    "__songlua_state_effect_magnitude",
    "__songlua_state_effect_clock",
    "__songlua_state_effect_color1",
    "__songlua_state_effect_color2",
    "__songlua_state_effect_timing",
    "__songlua_state_text_glow_mode",
    "__songlua_state_vert_spacing",
    "__songlua_state_wrap_width_pixels",
    "__songlua_state_sprite_state_index",
    "__songlua_state_texcoord_offset",
    "__songlua_state_custom_texture_rect",
    "__songlua_state_texcoord_velocity",
    "__songlua_state_size",
    "__songlua_state_stretch_rect",
    "__songlua_state_x",
    "__songlua_state_y",
    "__songlua_state_blend",
    "__songlua_state_effect_mode",
];

// The old raw C decoder accepted loading tables. The Rust live reader uses
// owned actors, so initialize native pose state before attaching the exact
// authored getter and its raw metadata. Each original actor still reads once.
fn owned_mutation_actor(host: &mut SongRuntime, source: &str) -> Value {
    let declaration = api::eval(
        host.vm_mut(),
        "return Def.Actor{InitCommand=function(self) self:x(7) end}",
    )[0];
    let scene = host.instantiate_actor(declaration).unwrap();
    let actor = host.actor_scene_root(scene).unwrap();
    let authored = named_actor(host, source);
    let attach = api::eval(
        host.vm_mut(),
        "return function(actor,authored) for key,value in pairs(authored) do rawset(actor,key,value) end;setmetatable(actor,getmetatable(authored));return actor end",
    )[0];
    api::call_one(host.vm_mut(), attach, &[actor, authored])
}
