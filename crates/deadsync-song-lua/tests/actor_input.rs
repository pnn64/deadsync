//! Actual scene loading and authored Lua differential; no Song --lib harness.
use deadsync_song_lua::*;
use std::{path::Path, sync::Arc};
type Kind = SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>;

fn scene(source: &str) -> (SongLuaLiveScene, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.lua");
    std::fs::write(&path, source).unwrap();
    let mut context = SongLuaCompileContext::new(directory.path(), "actor input test");
    context.live = true;
    context.music_length_seconds = 1.0;
    let mut compiled = compile_song_lua_layers_with_default_host(
        &[&path],
        0,
        &context,
        SongLuaNoteskinResolver::default(),
        |_: &Path, _: &Path, _: &Path| Ok(Arc::<[()]>::from([])),
        |_: &()| None,
        |_: &SongLuaCompileContext, _: &str| None::<(Kind, SongLuaOverlayState)>,
    )
    .unwrap();
    (compiled[0].live_scene.take().unwrap(), directory)
}

fn tracked(
    rows: impl Iterator<Item = (SongLuaTrackedActorTarget, SongLuaOverlayState)>,
) -> Vec<(usize, usize, SongLuaOverlayState)> {
    rows.map(|(target, state)| {
        use SongLuaTrackedActorTarget::*;
        let (kind, index) = match target {
            Player(p) => (0, p),
            PlayerJudgment(p) => (1, p),
            PlayerCombo(p) => (2, p),
            SongForeground => (3, 0),
            ScreenLayer(p) => (4, p),
        };
        (kind, index, state)
    })
    .collect()
}

fn event(index: usize) -> SongLuaInputEvent<'static> {
    SongLuaInputEvent {
        player: if index % 4 == 0 {
            None
        } else {
            Some(index % 2)
        },
        controller: Some(0),
        button: ["Left", "Right", "Other"][index % 3],
        game_button: ["Left", "Right", "Other"][index % 3],
        kind: [
            SongLuaInputType::FirstPress,
            SongLuaInputType::Repeat,
            SongLuaInputType::Release,
        ][index % 3],
        device: "InputDevice_Key",
        device_button: "DeviceButton_left",
        level: [-0.0, 0.25, 0.5, 1.0, f32::from_bits(0x7fc0_1234)][index % 5],
        z: index as i32 - 30,
        down: index % 4 < 2,
        ago: -0.0,
        is_joystick: false,
        is_mouse: false,
    }
}

#[test]
fn native_actor_actions_match_real_lua_state_and_continue_after_scene_destruction() {
    let (mut scene, _file) = scene(include_str!("fixtures/actor-input.lua"));
    let mut native = scene
        .compile_actor_inputs()
        .expect("authenticated visibility callbacks");
    assert_eq!(native.states(), scene.states());
    let mut expected = Vec::new();
    for index in 0..60 {
        let input = event(index);
        let (handled, output) = scene.input(&input).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        expected.push((
            handled,
            scene.states().to_vec(),
            tracked(scene.tracked_states()),
        ));
    }
    // Native callbacks and state own no Lua handles. All calls below outlive
    // their loading scene, original callbacks, actor tables and temporary file.
    drop(scene);
    for (index, (handled, states, expected_tracked)) in expected.iter().enumerate() {
        let mut actions = Vec::new();
        assert_eq!(
            native.input_with_actions(&event(index), |action| actions.push(action)),
            *handled
        );
        assert_eq!(native.states(), states);
        assert_eq!(tracked(native.tracked_states()), *expected_tracked);
        let expected_values: &[bool] = match index % 3 {
            0 => &[index % 4 < 2, event(index).level >= 0.5],
            1 => &[index % 4 < 2, true, false],
            _ => &[true, false],
        };
        assert_eq!(
            actions
                .iter()
                .map(|action| match action {
                    SongLuaActorInputAction::Visible { visible, .. } => *visible,
                })
                .collect::<Vec<_>>(),
            expected_values
        );
        if index % 3 == 0 {
            assert_eq!(
                actions[0],
                SongLuaActorInputAction::Visible {
                    actor: match actions[1] {
                        SongLuaActorInputAction::Visible { actor, .. } => actor,
                    },
                    visible: index % 4 < 2
                }
            );
        }
    }
}

#[test]
fn changed_methods_metamethods_shared_cells_and_unknown_calls_reject_without_running() {
    let template = include_str!("fixtures/actor-input.lua");
    for mutation in [
        "local original=left.visible; left.visible=function(...) return original(...) end",
        "local original=Actor.visible; Actor.visible=function(...) return original(...) end",
        "local original=Sprite.visible; Sprite.visible=function(...) return original(...) end",
        "local mt=getmetatable(left); local original=mt.__index; mt.__index=function(...) return original(...) end",
        "local mt=getmetatable(left); local original=mt.__newindex; mt.__newindex=function(...) return original(...) end",
        "local methods=left.__songlua_bound_methods; local original=methods.visible; methods.visible=function(...) return original(...) end",
    ] {
        // Mutate after callback registration, before the snapshot is compiled.
        let source = template.replacen(
            "local alias = left",
            &format!("local alias = left; {mutation}"),
            1,
        );
        let (scene, _file) = scene(&source);
        assert!(scene.compile_actor_inputs().is_none(), "{mutation}");
    }
    for replacement in [
        "local shared = 0; SCREENMAN:GetTopScreen():AddInputCallback(function(event) shared=shared+1; return shared end)",
        "SCREENMAN:GetTopScreen():AddInputCallback(function(event) left:zoom(2) end)",
        "SCREENMAN:GetTopScreen():AddInputCallback(function(event) event.button='Changed' end)",
        "SCREENMAN:GetTopScreen():AddInputCallback(function(event) return _G.changed end)",
    ] {
        let source = template.replacen(
            "local alias = left",
            &format!("local alias = left; {replacement}"),
            1,
        );
        let (scene, _file) = scene(&source);
        assert!(scene.compile_actor_inputs().is_none(), "{replacement}");
    }
}

#[test]
fn copied_bound_method_keeps_its_captured_target_and_chain_return_identity() {
    let source = include_str!("fixtures/actor-input.lua").replacen(
        "local alias = left",
        "local alias = left; rawset(left, 'visible', right.__songlua_bound_methods.visible)",
        1,
    );
    let source = source.replace(
        "left:visible(event.DeviceInput.down)",
        "left:visible(event.DeviceInput.down):visible(not event.DeviceInput.down)",
    );
    let (mut scene, _file) = scene(&source);
    let mut native = scene
        .compile_actor_inputs()
        .expect("copied authenticated bound method");
    for index in 0..24 {
        assert_eq!(
            native.input(&event(index)),
            scene.input(&event(index)).unwrap().0
        );
        assert_eq!(native.states(), scene.states());
        assert_eq!(
            tracked(native.tracked_states()),
            tracked(scene.tracked_states())
        );
    }
}

#[test]
fn copied_setter_rejects_custom_assignment_on_its_actual_mutation_target() {
    let mut source = include_str!("fixtures/actor-input.lua").replacen(
        "local alias = left",
        r#"
        local alias = left
        rawset(left, 'visible', right.__songlua_bound_methods.visible)
        local mt = getmetatable(right)
        local original = mt.__newindex
        mt.__newindex = function(actor, key, value)
            if key == '__songlua_visible' then actor:x(13) end
            return original(actor, key, value)
        end
        rawset(right, '__songlua_visible', nil)
    "#,
        1,
    );
    let start = source
        .find("        SCREENMAN:GetTopScreen():AddInputCallback")
        .unwrap();
    let end = start + source[start..].find("        end)").unwrap() + "        end)".len();
    source.replace_range(
        start..end,
        r#"        SCREENMAN:GetTopScreen():AddInputCallback(function(event)
            left:visible(event.DeviceInput.down)
            return false
        end)"#,
    );
    let (mut scene, _directory) = scene(&source);
    assert!(scene.compile_actor_inputs().is_none());
    let (_, output) = scene.input(&event(0)).unwrap();
    assert!(output.errors.is_empty());
    assert_eq!(
        scene.states()[2].x,
        13.0,
        "Lua fallback preserves target assignment side effects"
    );
}

#[derive(Debug, PartialEq, Eq)]
enum ScalarBits {
    Boolean(bool),
    Number(u64),
    Bytes(Vec<u8>),
}

fn scalars(native: &SongLuaCompiledActorInputs) -> Vec<(usize, Vec<u8>, ScalarBits)> {
    scalar_values(native.scalar_states())
}

fn scalar_values<'a>(
    rows: impl Iterator<Item = (usize, &'a [u8], SongLuaInputScalar<'a>)>,
) -> Vec<(usize, Vec<u8>, ScalarBits)> {
    let mut values = rows
        .map(|(table, key, value)| {
            (
                table,
                key.to_vec(),
                match value {
                    SongLuaInputScalar::Boolean(value) => ScalarBits::Boolean(value),
                    SongLuaInputScalar::Number(value) => ScalarBits::Number(value.to_bits()),
                    SongLuaInputScalar::Bytes(value) => ScalarBits::Bytes(value.to_vec()),
                },
            )
        })
        .collect::<Vec<_>>();
    values.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    values
}

#[test]
fn input_and_update_callbacks_share_timers_rates_hibernation_and_postorder_without_a_vm() {
    for (rate, child_rate, sleep) in [
        ("1.25", "0.5", "0.0625"),
        ("0.25", "2", "0"),
        ("0.3", "0.7", "0.2"),
    ] {
        let source = include_str!("fixtures/timed-input.lua")
            .replace("SetUpdateRate(1.25)", &format!("SetUpdateRate({rate})"))
            .replace(
                "SetUpdateRate(0.5)",
                &format!("SetUpdateRate({child_rate})"),
            )
            .replace("hibernate(0.0625)", &format!("hibernate({sleep})"));
        let (mut scene, _directory) = scene(&source);
        let mut native = scene
            .compile_actor_interactions()
            .expect("input and update unit");
        let mut expected = Vec::new();
        let active = |snapshot: &SongLuaCompiledActorInteractions| {
            snapshot.scalar_states().any(|(_, key, value)| {
                key == b"active" && value == SongLuaInputScalar::Boolean(true)
            })
        };
        for index in 0..90 {
            let input = stateful_event(index);
            let (handled, output) = scene.input(&input).unwrap();
            assert!(output.errors.is_empty());
            let snapshot = scene.compile_actor_interactions().unwrap();
            let was_active = active(&snapshot);
            expected.push((
                Some(handled),
                0.0,
                scene.states().to_vec(),
                scalar_values(snapshot.scalar_states()),
                match input.game_button {
                    "Right" => vec![SongLuaActorInputAction::Visible {
                        actor: 2,
                        visible: true,
                    }],
                    "Left" => vec![SongLuaActorInputAction::Visible {
                        actor: 2,
                        visible: false,
                    }],
                    _ => vec![],
                },
            ));
            let delta = [0.03125, 1.0 / 60.0, 0.0, -0.0, 0.125, 0.001][index % 6];
            let output = scene
                .update(SongLuaLiveFrame {
                    delta_seconds: delta,
                    beat: index as f64 / 30.0,
                    music_seconds: index as f64 / 60.0,
                    screen_seconds: index as f64 / 60.0,
                    ..Default::default()
                })
                .unwrap();
            assert!(output.errors.is_empty(), "{:?}", output.errors);
            let snapshot = scene.compile_actor_interactions().unwrap();
            expected.push((
                None,
                delta,
                scene.states().to_vec(),
                scalar_values(snapshot.scalar_states()),
                if was_active && !active(&snapshot) {
                    vec![SongLuaActorInputAction::Visible {
                        actor: 2,
                        visible: false,
                    }]
                } else {
                    vec![]
                },
            ));
        }
        drop(scene);
        for (step, (handled, delta, states, cells, expected_actions)) in
            expected.into_iter().enumerate()
        {
            let mut actions = Vec::new();
            if let Some(handled) = handled {
                assert_eq!(
                    native.input_with_actions(&stateful_event(step / 2), |a| actions.push(a)),
                    handled
                );
            } else {
                native.update_with_actions(delta, |a| actions.push(a));
            }
            assert_eq!(
                actions, expected_actions,
                "ordered writes after step {step}"
            );
            assert_eq!(native.states(), states, "authored states after step {step}");
            assert_eq!(
                scalar_values(native.scalar_states()),
                cells,
                "scalar bits after step {step}"
            );
        }
    }
}

#[test]
fn update_unit_rejects_unlowered_queues_effects_globals_and_callback_changes() {
    let template = include_str!("fixtures/timed-input.lua");
    for statement in ["self:linear(10):x(100)", "self:rainbow()", "self:spin()"] {
        let source = template.replace(
            "self:SetUpdateRate(1.25)",
            &format!("self:SetUpdateRate(1.25);{statement}"),
        );
        let (scene, _directory) = scene(&source);
        assert!(scene.compile_actor_interactions().is_none(), "{statement}");
    }
    for statement in [
        "shared.root_delta = _G.changed",
        "if delta > 0 then self:SetUpdateFunction(nil) end",
        "lamp:zoom(delta)",
    ] {
        let source = template.replace("shared.root_delta = delta", statement);
        let (scene, _directory) = scene(&source);
        assert!(scene.compile_actor_interactions().is_none(), "{statement}");
    }
}

#[test]
fn traversal_tables_outside_render_inventory_cannot_alias_mutable_scalar_cells() {
    let source = r#"return Def.ActorFrame {
        OnCommand=function(self)
            local state={__songlua_update_rate=1}
            local screen=SCREENMAN:GetTopScreen()
            local prepare
            prepare=function(event)
                if event.GameButton=='Install' then
                    rawset(self,1,state)
                    self:SetUpdateRate(1)
                elseif event.GameButton=='Freeze' then
                    for key in pairs(state) do state[key]=nil end
                    state.__songlua_update_rate=1
                    screen:RemoveInputCallback(prepare)
                end
                return false
            end
            screen:AddInputCallback(prepare)
            screen:AddInputCallback(function(event)
                if event.GameButton=='Change' then
                    state.__songlua_update_rate=state.__songlua_update_rate+1
                end
                return false
            end)
        end
    }"#;
    let (mut scene, _directory) = scene(source);
    let mut input = event(0);
    input.game_button = "Install";
    assert!(scene.input(&input).unwrap().1.errors.is_empty());
    let output = scene
        .update(SongLuaLiveFrame {
            delta_seconds: 0.1,
            screen_seconds: 0.1,
            ..Default::default()
        })
        .unwrap();
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    input.game_button = "Freeze";
    assert!(scene.input(&input).unwrap().1.errors.is_empty());
    assert!(
        scene.compile_actor_inputs().is_some(),
        "input-only copying remains supported"
    );
    assert!(
        scene.compile_actor_interactions().is_none(),
        "the same table is owned by the timing traversal"
    );
}

fn stateful_event(index: usize) -> SongLuaInputEvent<'static> {
    let mut input = event(index);
    input.kind = [
        SongLuaInputType::FirstPress,
        SongLuaInputType::Repeat,
        SongLuaInputType::Release,
    ][(index / 3) % 3];
    input.level = if index < 30 {
        [-0.0, 0.25, 0.5, 1.0][index % 4]
    } else {
        [-0.0, 0.25, 0.5, 1.0, f32::from_bits(0x7fc0_1234)][index % 5]
    };
    input
}

#[test]
fn string_modes_share_transitions_across_inputs_updates_and_distinct_equal_cells_without_lua() {
    let (mut scene, _directory) = scene(include_str!("fixtures/state-machine-input.lua"));
    let mut native = scene
        .compile_actor_interactions()
        .expect("finite byte-state interaction");
    assert_eq!(native.scalar_states().count(), 6);
    let mode = |unit: &SongLuaCompiledActorInteractions| {
        unit.scalar_states()
            .find_map(|(_, key, v)| {
                if key == b"mode" {
                    match v {
                        SongLuaInputScalar::Bytes(v) => Some(v.to_vec()),
                        _ => None,
                    }
                } else {
                    None
                }
            })
            .unwrap()
    };
    let mut expected = Vec::new();
    let mut seconds = 0.0;
    for index in 0..120 {
        let input = stateful_event(index);
        let (handled, out) = scene.input(&input).unwrap();
        assert!(out.errors.is_empty());
        let snapshot = scene.compile_actor_interactions().unwrap();
        let before = mode(&snapshot);
        let actions = match input.game_button {
            "Right" => vec![SongLuaActorInputAction::Visible {
                actor: 2,
                visible: before == b"armed",
            }],
            "Left" => vec![SongLuaActorInputAction::Visible {
                actor: 2,
                visible: false,
            }],
            _ => vec![],
        };
        expected.push((
            Some(handled),
            0.0,
            scene.states().to_vec(),
            scalar_values(snapshot.scalar_states()),
            actions,
        ));
        let delta = [0.03125, 1.0 / 60.0, 0.0, -0.0, 0.125, 0.001][index % 6];
        seconds += delta;
        let out = scene
            .update(SongLuaLiveFrame {
                delta_seconds: delta,
                screen_seconds: seconds,
                music_seconds: seconds,
                beat: seconds * 2.0,
                ..Default::default()
            })
            .unwrap();
        assert!(out.errors.is_empty());
        let snapshot = scene.compile_actor_interactions().unwrap();
        let actions = if before == b"armed" && mode(&snapshot) == b"cooldown" {
            vec![SongLuaActorInputAction::Visible {
                actor: 2,
                visible: false,
            }]
        } else {
            vec![]
        };
        expected.push((
            None,
            delta,
            scene.states().to_vec(),
            scalar_values(snapshot.scalar_states()),
            actions,
        ));
    }
    drop(scene);
    for (step, (handled, delta, states, cells, expected_actions)) in
        expected.into_iter().enumerate()
    {
        let mut actions = Vec::new();
        if let Some(handled) = handled {
            assert_eq!(
                native.input_with_actions(&stateful_event(step / 2), |a| actions.push(a)),
                handled
            );
        } else {
            native.update_with_actions(delta, |a| actions.push(a));
        }
        assert_eq!(native.states(), states, "states at{step}");
        assert_eq!(
            scalar_values(native.scalar_states()),
            cells,
            "cells at{step}"
        );
        assert_eq!(actions, expected_actions, "actions at{step}");
        assert!(
            native
                .scalar_states()
                .any(|(_, key, value)| key == b"decoy"
                    && value == SongLuaInputScalar::Bytes(b"idle"))
        );
    }
}

#[test]
fn readonly_byte_specialization_survives_and_transient_byte_writes_keep_fallback() {
    let template = include_str!("fixtures/actor-input.lua");
    let source = template
        .replace(
            "local alias = left",
            "local alias = left;local descriptor={method='visible'}",
        )
        .replace(
            "left:visible(event.DeviceInput.down)",
            "left[descriptor.method](left,event.DeviceInput.down)",
        );
    let (mut scene, _directory) = scene(&source);
    let mut native = scene
        .compile_actor_inputs()
        .expect("constant byte field remains specialized");
    for index in 0..12 {
        assert_eq!(
            native.input(&event(index)),
            scene.input(&event(index)).unwrap().0
        );
        assert_eq!(native.states(), scene.states());
    }
    for replacement in ["shared.mode = event.GameButton", "shared.mode = 17"] {
        let source = include_str!("fixtures/state-machine-input.lua").replacen(
            "shared.mode = \"armed\"",
            replacement,
            1,
        );
        let (scene, _directory) = self::scene(&source);
        assert!(
            scene.compile_actor_interactions().is_none(),
            "{replacement}"
        );
    }
}

#[test]
fn shared_tables_cross_callbacks_without_merging_equal_independent_state() {
    let (mut scene, _directory) = scene(include_str!("fixtures/stateful-input.lua"));
    let mut native = scene
        .compile_actor_inputs()
        .expect("raw shared scalar tables");
    assert_eq!(native.scalar_states().count(), 3);
    let mut expected = Vec::new();
    for index in 0..90 {
        let input = stateful_event(index);
        let (handled, output) = scene.input(&input).unwrap();
        assert!(output.errors.is_empty());
        // A fresh snapshot reads actual Lua upvalue tables after the call.
        // This observes every scalar's bits without running another callback.
        let snapshot = scene.compile_actor_inputs().unwrap();
        expected.push((
            handled,
            scene.states().to_vec(),
            tracked(scene.tracked_states()),
            scalars(&snapshot),
        ));
    }
    drop(scene);
    for (index, (handled, states, tracked_states, cells)) in expected.into_iter().enumerate() {
        let input = stateful_event(index);
        let mut actions = Vec::new();
        assert_eq!(
            native.input_with_actions(&input, |a| actions.push(a)),
            handled
        );
        let mut expected_actions = vec![SongLuaActorInputAction::Visible {
            actor: if input.game_button == "Left" { 1 } else { 2 },
            visible: states[if input.game_button == "Left" { 1 } else { 2 }].visible,
        }];
        if input.game_button == "Right" && input.kind == SongLuaInputType::Release {
            expected_actions.push(SongLuaActorInputAction::Visible {
                actor: 1,
                visible: false,
            });
        }
        assert_eq!(
            actions, expected_actions,
            "ordered writes after input {index}"
        );
        assert_eq!(native.states(), states);
        assert_eq!(tracked(native.tracked_states()), tracked_states);
        assert_eq!(scalars(&native), cells, "scalar bits after input {index}");
    }
}

#[test]
fn state_table_metamethods_unknown_fields_and_type_changes_keep_lua_fallback() {
    let template = include_str!("fixtures/stateful-input.lua");
    for replacement in [
        "local shared=setmetatable({held=false,count=0},{__index=function() error('must not run') end})",
        "local shared={held=false,count=0,unexpected=function() error('must not run') end}",
    ] {
        let source = template.replace("local shared = { held = false, count = -0.0 }", replacement);
        let (scene, _directory) = scene(&source);
        assert!(scene.compile_actor_inputs().is_none());
    }
    for statement in [
        "shared.new_field = true",
        "shared.held = 0",
        "shared.count = false",
    ] {
        let source = template.replace("shared.held = not alias.held", statement);
        let (scene, _directory) = scene(&source);
        assert!(scene.compile_actor_inputs().is_none(), "{statement}");
    }
}
