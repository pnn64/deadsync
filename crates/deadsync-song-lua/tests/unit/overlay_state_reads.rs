#[test]
fn overlay_reads_preserve_conversions_errors_and_stack() {
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    // Exercise every property used by render-state extraction, including
    // invalid UTF-8, coercions, short/aliased vectors and foreign userdata.
    let keys = &[
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
    let values = [
        Value::Nil, Value::Boolean(false), Value::Boolean(true),
        Value::Integer(7), Value::Number(-0.0), Value::Number(f64::INFINITY),
        Value::String(lua.create_string(" 2.5 ").unwrap()),
        Value::String(lua.create_string("x".repeat(200)).unwrap()),
        Value::String(lua.create_string([0xff, 0xfe]).unwrap()),
        lua.load("return {1,'2',3,4,5}").eval().unwrap(),
        lua.load("return {{1,2,3,4},{1,2,3,4},{1,2,3,4},{1,2,3,4}}").eval().unwrap(),
        Value::UserData(lua.create_any_userdata(7_u32).unwrap()),
    ];
    lua.exec_raw_lua(|raw| unsafe {
        assert_ne!(ffi::lua_checkstack(raw.state(), 8), 0);
        raw.push(42).unwrap();
        let top = ffi::lua_gettop(raw.state());
        for key in keys {
            for value in &values {
                actor.raw_set(*key, value.clone()).unwrap();
                let expected = actor_overlay_initial_state(&actor);
                let actual = actor_overlay_state_batched(&lua, &actor);
                assert_eq!(actual, expected, "{key}: {value:?}");
                assert_eq!(ffi::lua_gettop(raw.state()), top, "{key}");
            }
            actor.raw_set(*key, Value::Nil).unwrap();
        }
        assert_eq!(raw.pop::<i32>().unwrap(), 42);
    });
    // Both fields are bad: preserve the first error in decoder order.
    actor.raw_set("__songlua_state_draw_order", false).unwrap();
    actor.raw_set("__songlua_state_shadow_len", 7).unwrap();
    assert_eq!(actor_overlay_state_batched(&lua, &actor), actor_overlay_initial_state(&actor));
    assert_eq!(lua.load("return 6*7").eval::<i32>().unwrap(), 42);
}

#[test]
fn overlay_reads_follow_authored_texture_mutations() {
    let lua = Lua::new();
    let source = r#"
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
    for fail in [false, true] {
        lua.globals().set("FAIL", fail).unwrap();
        lua.globals().set("READS", 0).unwrap();
        let before = lua.load(source).set_name("texture-mutation").eval::<Table>().unwrap();
        let after = lua.load(source).set_name("texture-mutation").eval::<Table>().unwrap();
        let expected = draw_state_baseline::batched(&lua, &before);
        let actual = actor_overlay_state_batched(&lua, &after);
        assert_eq!(actual, expected);
        assert_eq!(lua.globals().get::<i32>("READS").unwrap(), 2);
        if !fail {
            let state = actual.unwrap();
            assert_eq!(state.x, 7.0, "scalar snapshot precedes authored lookup");
            assert_eq!(state.draw_order, 17);
            assert_eq!(state.shadow_len, [3.0, 5.0]);
            assert_eq!(state.size, Some([12.0, 24.0]));
        }
    }
}

#[test]
fn overlay_reads_keep_native_vectors_without_heap_churn() {
    let lua = Lua::new();
    begin_live_runtime(&lua).unwrap();
    let actor = lua.load(r#"return {
        __songlua_actor_type='Quad',
        __songlua_state_text_align='left',
        __songlua_state_effect_mode='pulse',
        __songlua_state_effect_clock='beat',
        __songlua_state_blend='add',
        __songlua_state_stretch_rect={0,0,10,20},
    }"#).eval::<Table>().unwrap();
    crate::native_vector::store(&lua, &actor, "__songlua_state_size", [12.0,24.0]).unwrap();
    crate::native_vector::store(&lua, &actor, "__songlua_state_shadow_len", [3.0,5.0]).unwrap();
    crate::native_vector::store(&lua, &actor, "__songlua_state_base_rotation", [7.0,9.0,11.0]).unwrap();
    ensure_tween_replay(&lua, &actor).unwrap();
    begin_live_actor_playback(&lua, &[actor.clone()]).unwrap();
    let expected = live_actor_overlay_state(&lua, &actor).unwrap();
    assert_eq!(expected.size, Some([12.0,24.0]));
    assert_eq!(expected.shadow_len, [3.0,5.0]);
    crate::draw_capture::perf::allocations::assert_no_churn(|| {
        for _ in 0..128 {
            assert_eq!(live_actor_overlay_state(&lua, &actor).unwrap(), expected);
        }
    });
}
