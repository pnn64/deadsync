//! Existing Lua fixtures through the Rust live VM and VM-free input owners.
use deadsync_song_lua::{self as song, *};
use std::{path::Path, sync::Arc};
type Kind = SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>;

struct Pair {
    rust: song::SongLiveScene,
    _dir: tempfile::TempDir,
}
impl Pair {
    fn new(source: &str, files: &[(&str, &str)]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        reference_test::register_path(dir.path());
        for (name, contents) in files {
            std::fs::write(dir.path().join(name), contents).unwrap();
        }
        let path = dir.path().join("input.lua");
        std::fs::write(&path, source).unwrap();
        let mut context = SongLuaCompileContext::new(dir.path(), "Actor input snapshots");
        context.live = true;
        context.music_length_seconds = 1.;
        context.players[1].enabled = true;
        let slots = |_: &Path, _: &Path, _: &Path| Ok(Arc::<[()]>::from([]));
        let layer = |_: &()| None::<SongLuaOverlayModelLayer<()>>;

        let mut rust = song::compile_song_lua_layers(
            &[&path],
            0,
            &context,
            Default::default(),
            slots,
            layer,
            |_, _| None::<(Kind, SongLuaOverlayState)>,
        )
        .unwrap();
        Self {
            rust: rust[0].live_scene.take().unwrap(),
            _dir: dir,
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
enum Scalar {
    Boolean(bool),
    Number(u64),
    Bytes(Vec<u8>),
}
fn scalars<'a>(
    rows: impl Iterator<Item = (usize, &'a [u8], SongLuaInputScalar<'a>)>,
) -> Vec<(usize, Vec<u8>, Scalar)> {
    let mut result = rows
        .map(|(object, key, value)| {
            (
                object,
                key.to_vec(),
                match value {
                    SongLuaInputScalar::Boolean(v) => Scalar::Boolean(v),
                    SongLuaInputScalar::Number(v) => Scalar::Number(v.to_bits()),
                    SongLuaInputScalar::Bytes(v) => Scalar::Bytes(v.to_vec()),
                },
            )
        })
        .collect::<Vec<_>>();
    result.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    result
}
fn booleans<'a>(rows: impl Iterator<Item = (u32, &'a [u8], bool)>) -> Vec<(u32, Vec<u8>, bool)> {
    let mut result = rows
        .map(|(object, key, value)| (object, key.to_vec(), value))
        .collect::<Vec<_>>();
    result.sort();
    result
}
fn tracked_states(
    rows: impl Iterator<Item = (SongLuaTrackedActorTarget, SongLuaOverlayState)>,
) -> Vec<(u8, usize, SongLuaOverlayState)> {
    rows.map(|(target, state)| {
        let (kind, index) = match target {
            SongLuaTrackedActorTarget::Player(index) => (0, index),
            SongLuaTrackedActorTarget::PlayerJudgment(index) => (1, index),
            SongLuaTrackedActorTarget::PlayerCombo(index) => (2, index),
            SongLuaTrackedActorTarget::SongForeground => (3, 0),
            SongLuaTrackedActorTarget::ScreenLayer(index) => (4, index),
        };
        (kind, index, state)
    })
    .collect()
}
fn event(index: usize) -> SongLuaInputEvent<'static> {
    SongLuaInputEvent {
        player: [Some(0), Some(1), None][index % 3],
        controller: Some(index % 3),
        button: ["Left", "Right", "Up", "Down", "Other"][index % 5],
        game_button: ["Left", "Right", "Other"][index % 3],
        kind: [
            SongLuaInputType::FirstPress,
            SongLuaInputType::Release,
            SongLuaInputType::Repeat,
        ][index % 3],
        device: "InputDevice_Key",
        device_button: "Key_left",
        level: [-0.0, 0.1, 0.5, 1.0][index % 4],
        z: index as i32 - 4,
        down: index % 2 == 0,
        ago: index as f64 / 1024.,
        is_joystick: false,
        is_mouse: false,
    }
}
fn exercise(source: &str, files: &[(&str, &str)], count: usize) {
    let mut pair = Pair::new(source, files);
    let mut rust = pair
        .rust
        .compile_actor_inputs()
        .expect("Rust supported fixture");
    let mut expected = Vec::new();
    for index in 0..count {
        let input = event(index);
        let (handled, output) = pair.rust.input(&input).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        let current = pair.rust.compile_actor_inputs().unwrap();
        let cells = scalars(current.scalar_states());
        let maps = booleans(current.boolean_states());
        let observation = (
            input,
            handled,
            pair.rust.states().to_vec(),
            tracked_states(pair.rust.tracked_states()),
            cells,
            maps,
        );
        reference_test::assert_debug(
            "song/tests/rust_actor_inputs.rs::exercise::live",
            &(index, &observation),
        );
        expected.push(observation);
    }
    pair.rust.end().unwrap();
    assert!(pair.rust.compile_actor_inputs().is_none());
    drop(pair);
    assert!(rust.retained_bytes() > 0);
    for (index, (input, handled, states, tracked, cells, maps)) in expected.into_iter().enumerate()
    {
        let mut actions = Vec::new();
        assert_eq!(
            rust.input_with_actions(&input, |action| actions.push(action)),
            handled
        );
        reference_test::assert_debug(
            "song/tests/rust_actor_inputs.rs::actions",
            &(index, &actions),
        );
        assert_eq!(rust.states(), states, "closed Rust states {index}");
        assert_eq!(tracked_states(rust.tracked_states()), tracked);
        assert_eq!(scalars(rust.scalar_states()), cells);
        assert_eq!(booleans(rust.boolean_states()), maps);
    }
}

#[test]
fn original_actor_visibility_fixture_preserves_writes_after_teardown() {
    exercise(include_str!("fixtures/actor-input.lua"), &[], 36);
}
#[test]
fn original_fof_handler_preserves_dense_aliased_boolean_tables_after_teardown() {
    exercise(
        "local controller=LoadActor('controller.lua');return Def.ActorFrame{OnCommand=function()SCREENMAN:GetTopScreen():AddInputCallback(controller.handle_input)end}",
        &[(
            "controller.lua",
            include_str!("fixtures/fof-input-system.lua"),
        )],
        96,
    );
}
#[test]
fn original_movement_fixture_preserves_position_bits_and_scalar_cells() {
    exercise(include_str!("fixtures/movement-input.lua"), &[], 32);
}
#[test]
fn copied_and_chained_setter_actions_use_the_captured_target() {
    let source = include_str!("fixtures/actor-input.lua").replace(
        "local alias = left",
        "rawset(left,'visible',right.__songlua_bound_methods.visible);local alias = left",
    );
    exercise(&source, &[], 32);
}
#[test]
fn mutable_byte_cells_share_the_existing_specialization_retry() {
    exercise(
        r#"return Def.ActorFrame{OnCommand=function(self)
        local actor=self:GetChild('lamp');local state={word='left',count=0}
        SCREENMAN:GetTopScreen():AddInputCallback(function(event)
            state.count=state.count+1
            if event.GameButton=='Left' then state.word='left' else state.word='right' end
            actor:visible(state.word=='left')
            return state.count==2
        end)
    end,Def.Quad{Name='lamp'}}"#,
        &[],
        24,
    );
}
#[test]
fn unsupported_body_rejects_without_applying_its_earlier_action() {
    let source = include_str!("fixtures/actor-input.lua").replace(
        "if event.GameButton == \"Left\" then",
        "left:visible(true);_G.snapshot_probe=true;if event.GameButton == \"Left\" then",
    );
    let mut pair = Pair::new(&source, &[]);

    assert!(pair.rust.compile_actor_inputs().is_none());
    let before = pair
        .rust
        .states()
        .iter()
        .map(|s| (s.x.to_bits(), s.y.to_bits(), s.visible))
        .collect::<Vec<_>>();

    pair.rust.update(SongLuaLiveFrame::default()).unwrap();
    reference_test::assert_debug("song/tests/rust_actor_inputs.rs::before_rejection", &before);
    for states in [pair.rust.states()] {
        assert_eq!(
            states
                .iter()
                .map(|s| (s.x.to_bits(), s.y.to_bits(), s.visible))
                .collect::<Vec<_>>(),
            before
        );
    }

    pair.rust.end().unwrap();
}

fn exercise_combined(source: &str) {
    let mut pair = Pair::new(source, &[]);
    let mut rust = pair
        .rust
        .compile_actor_interactions()
        .expect("Rust combined fixture");
    let mut music = 0.;
    let mut expected = Vec::new();
    for index in 0..32 {
        let input = event(index);
        let (handled, output) = pair.rust.input(&input).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        let delta = if index == 0 {
            0.
        } else {
            [0.015625, 0.03125, 0.0625][index % 3]
        };
        music += delta;
        let frame = SongLuaLiveFrame {
            beat: music * 2.,
            music_seconds: music,
            screen_seconds: music,
            delta_seconds: delta,
            ..Default::default()
        };
        let output = pair.rust.update(frame).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        let current = pair
            .rust
            .compile_actor_interactions()
            .expect("Rust current cells");
        let cells = scalars(current.scalar_states());
        let observation = (
            input,
            handled,
            delta,
            pair.rust.states().to_vec(),
            tracked_states(pair.rust.tracked_states()),
            cells,
        );
        reference_test::assert_debug(
            "song/tests/rust_actor_inputs.rs::exercise_combined::live",
            &(index, &observation),
        );
        expected.push(observation);
    }
    pair.rust.end().unwrap();
    drop(pair);
    for (index, (input, handled, delta, states, tracked, cells)) in expected.into_iter().enumerate()
    {
        let mut actions = Vec::new();
        assert_eq!(
            rust.input_with_actions(&input, |action| actions.push(action)),
            handled
        );
        reference_test::assert_debug(
            "song/tests/rust_actor_inputs.rs::combined_input_actions",
            &(index, &actions),
        );
        actions.clear();
        rust.update_with_actions(delta, |action| actions.push(action));
        reference_test::assert_debug(
            "song/tests/rust_actor_inputs.rs::combined_update_actions",
            &(index, &actions),
        );
        assert_eq!(rust.states(), states, "Rust closed combined state {index}");
        assert_eq!(tracked_states(rust.tracked_states()), tracked);
        assert_eq!(
            scalars(rust.scalar_states()),
            cells,
            "Rust closed cells {index}"
        );
    }
}

#[test]
fn timed_input_updates_share_cells_rates_and_hibernation_after_teardown() {
    exercise_combined(include_str!("fixtures/timed-input.lua"));
}
#[test]
fn movement_input_updates_preserve_hierarchical_position_action_order() {
    exercise_combined(include_str!("fixtures/movement-input.lua"));
}
#[test]
fn combined_units_keep_existing_queue_effect_and_unknown_operation_rejections() {
    let template = include_str!("fixtures/timed-input.lua");
    for statement in ["self:linear(10):x(100)", "self:rainbow()", "self:spin()"] {
        let source = template.replace(
            "self:SetUpdateRate(1.25)",
            &format!("self:SetUpdateRate(1.25);{statement}"),
        );
        let mut pair = Pair::new(&source, &[]);

        assert!(
            pair.rust.compile_actor_interactions().is_none(),
            "Rust {statement}"
        );

        pair.rust.end().unwrap();
    }
    let source = template.replace(
        "shared.root_delta = delta",
        "shared.root_delta = _G.changed",
    );
    let mut pair = Pair::new(&source, &[]);

    assert!(pair.rust.compile_actor_interactions().is_none());

    pair.rust.end().unwrap();
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
