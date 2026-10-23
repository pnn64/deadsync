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
