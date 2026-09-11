use super::*;
use std::hint::black_box;

fn actor_with_fields(lua: &Lua, ignored: usize, retained: usize) -> Table {
    let actor = lua.create_table().unwrap();
    for index in 0..ignored {
        actor.raw_set(format!("Method{index}"), index).unwrap();
    }
    for index in 0..retained {
        actor
            .raw_set(format!("__songlua_state_custom_{index}"), index)
            .unwrap();
    }
    actor
}

#[test]
fn actor_state_roundtrip_restores_owned_values_and_preserves_unrelated_fields() {
    let lua = Lua::new();
    let actor = actor_with_fields(&lua, 64, 8);
    actor.set("Text", "before").unwrap();
    actor.set("__songlua_state_\u{03b1}", 42).unwrap();
    actor.raw_set(1, "numeric key").unwrap();
    let nested = lua.create_table().unwrap();
    nested.set("value", 17).unwrap();
    actor.set("__songlua_state_nested", nested.clone()).unwrap();
    actor.set("__songlua_capture_cursor", 1.25).unwrap();
    let snapshot = snapshot_actor_mutable_state(&lua, &actor).unwrap();
    assert_eq!(snapshot.len(), 12);
    assert!(
        snapshot
            .iter()
            .all(|(key, _)| is_actor_mutable_state_key(key))
    );
    nested.set("value", 99).unwrap();
    actor.set("Text", "after").unwrap();
    actor.set("__songlua_state_added", true).unwrap();
    actor.set("__songlua_capture_cursor", 8.0).unwrap();
    actor.set("Method0", "changed method").unwrap();
    restore_actor_mutable_state(&actor, snapshot).unwrap();
    assert_eq!(actor.get::<String>("Text").unwrap(), "before");
    assert_eq!(actor.get::<i32>("__songlua_state_\u{03b1}").unwrap(), 42);
    assert!(matches!(
        actor.get::<Value>("__songlua_state_added").unwrap(),
        Value::Nil
    ));
    assert_eq!(actor.get::<f32>("__songlua_capture_cursor").unwrap(), 1.25);
    assert_eq!(actor.get::<String>("Method0").unwrap(), "changed method");
    assert_eq!(actor.raw_get::<String>(1).unwrap(), "numeric key");
    let restored = actor.get::<Table>("__songlua_state_nested").unwrap();
    assert_eq!(restored.get::<i32>("value").unwrap(), 17);
    assert_ne!(restored.to_pointer(), nested.to_pointer());

    let semantic = snapshot_actor_semantic_state(&lua, &actor).unwrap();
    actor.set("__songlua_capture_cursor", 4.0).unwrap();
    actor.set("Text", "changed again").unwrap();
    restore_actor_semantic_state(&actor, semantic).unwrap();
    assert_eq!(actor.get::<String>("Text").unwrap(), "before");
    assert_eq!(actor.get::<f32>("__songlua_capture_cursor").unwrap(), 4.0);
}

#[test]
fn state_scans_keep_invalid_utf8_key_errors_before_restoration() {
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    actor
        .raw_set(lua.create_string([0xff]).unwrap(), 1)
        .unwrap();
    actor.set("__songlua_state_x", 5).unwrap();
    assert!(snapshot_actor_mutable_state(&lua, &actor).is_err());
    assert!(restore_actor_mutable_state(&actor, vec![]).is_err());
    assert_eq!(actor.get::<i32>("__songlua_state_x").unwrap(), 5);
}

#[test]
fn position_capture_keeps_scheduled_getter_state_and_additive_precedence() {
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    reset_actor_capture(&lua, &actor).unwrap();
    capture_block_set_f32(&lua, &actor, "x", 17.0).unwrap();
    assert_eq!(actor.get::<f32>("__songlua_current_x").unwrap(), 17.0);
    actor.set("__songlua_capture_duration", 1.0).unwrap();
    actor.raw_remove("__songlua_current_x").unwrap();
    capture_block_set_f32(&lua, &actor, "x", 25.0).unwrap();
    assert_eq!(actor.get::<f32>("__songlua_current_x").unwrap(), 17.0);
    assert_eq!(actor.get::<f32>("__songlua_state_x").unwrap(), 25.0);

    let add = make_actor_add_f32_method(&lua, &actor, "x").unwrap();
    let block = actor_current_capture_block(&lua, &actor).unwrap();
    block.set("x", 40.0).unwrap();
    add.call::<Value>((actor.clone(), 2.0)).unwrap();
    assert_eq!(actor.get::<f32>("__songlua_state_x").unwrap(), 42.0);
    assert_eq!(actor.get::<f32>("__songlua_current_x").unwrap(), 17.0);
    // The old eager fallback lookup must still surface conversion errors even
    // when the current block has a value (Lua metamethods can observe reads).
    actor
        .set("__songlua_state_x", lua.create_table().unwrap())
        .unwrap();
    assert!(add.call::<Value>((actor.clone(), 2.0)).is_err());

    capture_block_set_f32(&lua, &actor, "custom_\u{03b1}", 7.0).unwrap();
    let add_custom = make_actor_add_f32_method(&lua, &actor, "custom_\u{03b1}").unwrap();
    add_custom.call::<Value>((actor.clone(), 3.0)).unwrap();
    assert_eq!(
        actor.get::<f32>("__songlua_state_custom_\u{03b1}").unwrap(),
        10.0
    );
}

#[test]
fn typed_state_setters_keep_capture_values_and_property_names() {
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    reset_actor_capture(&lua, &actor).unwrap();
    capture_block_set_vec2(&lua, &actor, "shadow_len", [2.0, 3.0]).unwrap();
    capture_block_set_vec3(&lua, &actor, "effect_magnitude", [4.0, 5.0, 6.0]).unwrap();
    capture_block_set_vec4(&lua, &actor, "glow", [0.1, 0.2, 0.3, 0.4]).unwrap();
    capture_block_set_vec5(&lua, &actor, "effect_timing", [1.0, 2.0, 3.0, 4.0, 5.0]).unwrap();
    capture_block_set_string(&lua, &actor, "blend", "add").unwrap();
    capture_block_set_u32(&lua, &actor, "sprite_state_index", 7).unwrap();
    capture_block_set_i32(&lua, &actor, "wrap_width_pixels", 128).unwrap();
    let state = actor_overlay_initial_state(&actor).unwrap();
    assert_eq!(state.shadow_len, [2.0, 3.0]);
    assert_eq!(state.effect_magnitude, [4.0, 5.0, 6.0]);
    assert_eq!(state.glow, [0.1, 0.2, 0.3, 0.4]);
    assert_eq!(state.effect_timing, Some([1.0, 2.0, 3.0, 4.0, 5.0]));
    assert_eq!(state.blend, crate::SongLuaOverlayBlendMode::Add);
    assert_eq!(state.sprite_state_index, Some(7));
    assert_eq!(state.wrap_width_pixels, Some(128));
    let block = actor_current_capture_block(&lua, &actor).unwrap();
    assert_eq!(block.get::<u32>("sprite_state_index").unwrap(), 7);
    assert_eq!(block.get::<String>("blend").unwrap(), "add");
}

#[test]
#[ignore = "manual release benchmark; run serially"]
fn actor_state_hot_path_bench() {
    for ignored in [32, 256, 1024] {
        let lua = Lua::new();
        let actor = actor_with_fields(&lua, ignored, 8);
        crate::perf::measure_sampled(
            &format!("snapshot_keys_{ignored}"),
            256,
            ignored + 8,
            || {
                black_box(snapshot_actor_mutable_state(&lua, black_box(&actor)).unwrap());
            },
        );
        // Cloning the eight-entry input snapshot is included in both versions.
        let snapshot = snapshot_actor_mutable_state(&lua, &actor).unwrap();
        crate::perf::measure_sampled(&format!("restore_keys_{ignored}"), 256, ignored + 8, || {
            restore_actor_mutable_state(black_box(&actor), snapshot.clone()).unwrap();
        });
    }
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    reset_actor_capture(&lua, &actor).unwrap();
    crate::perf::measure_sampled("set_xyz_96", 256, 96, || {
        for index in 0..32 {
            for key in ["x", "y", "z"] {
                capture_block_set_f32(&lua, &actor, black_box(key), black_box(index as f32))
                    .unwrap();
            }
        }
    });
    let add = make_actor_add_f32_method(&lua, &actor, "x").unwrap();
    crate::perf::measure_sampled("add_x_32", 256, 32, || {
        for _ in 0..32 {
            black_box(
                add.call::<Value>((actor.clone(), black_box(0.125)))
                    .unwrap(),
            );
        }
    });
}

#[test]
fn ignored_actor_fields_only_pay_lua_handle_churn() {
    let lua = Lua::new();
    let actor = actor_with_fields(&lua, 512, 0);
    // Warm Lua's reference stack before measuring Rust allocation traffic.
    assert!(
        snapshot_actor_mutable_state(&lua, &actor)
            .unwrap()
            .is_empty()
    );
    restore_actor_mutable_state(&actor, vec![]).unwrap();
    // mlua 0.12 shares each visited string handle through an Rc allocation
    // (two machine words). Neither scan should also copy ignored key text.
    let visited = 2 * 512;
    crate::perf::assert_churn_budget(visited, visited * 2 * size_of::<usize>(), || {
        assert!(
            snapshot_actor_mutable_state(&lua, &actor)
                .unwrap()
                .is_empty()
        );
        restore_actor_mutable_state(&actor, vec![]).unwrap();
    });
}

#[test]
fn warmed_position_and_additive_updates_do_not_allocate() {
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    reset_actor_capture(&lua, &actor).unwrap();
    let add = make_actor_add_f32_method(&lua, &actor, "x").unwrap();
    for key in ["x", "y", "z"] {
        capture_block_set_f32(&lua, &actor, key, 1.0).unwrap();
    }
    add.call::<Value>((actor.clone(), 1.0)).unwrap();
    crate::perf::assert_no_churn(|| {
        for key in ["x", "y", "z"] {
            capture_block_set_f32(&lua, &actor, key, 4.0).unwrap();
        }
        add.call::<Value>((actor.clone(), 2.0)).unwrap();
    });
    assert_eq!(actor.get::<f32>("__songlua_state_x").unwrap(), 6.0);
}
