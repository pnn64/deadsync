use super::*;
#[cfg(windows)]
use std::hint::black_box;

#[path = "live_extract_before.rs"]
mod before;

fn fixture(labels: usize, label_bytes: usize, normal: usize, immediate: usize) -> Lua {
    let lua = Lua::new();
    let mut capture = Capture {
        bytes: 12345,
        frame_ops: 17,
        ..Capture::default()
    };
    for pointer in 1..=labels {
        capture.labels.insert(
            pointer,
            format!("Quad '{pointer}:{}'", "x".repeat(label_bytes)),
        );
    }
    for index in 0..normal + immediate {
        let frame = RawFrame {
            second: index as f32 * 0.125,
            immediate: index >= normal,
            owners: vec![1],
            calls: vec![RawCall {
                owner: 1,
                op: RawOp::Draw(
                    1,
                    SongLuaOverlayState {
                        x: index as f32,
                        ..Default::default()
                    },
                    Context {
                        owner: 1,
                        ..Default::default()
                    },
                    None,
                ),
            }],
        };
        if frame.immediate {
            let position = (index * 3) % (capture.frames.len() + 1);
            capture.frames.insert(position, frame);
        } else {
            capture.frames.push(frame);
        }
    }
    lua.set_app_data(capture);
    lua.set_app_data(LiveSources(
        [(1, DrawSource::Overlay(0)), (2, DrawSource::Player(1))]
            .into_iter()
            .collect(),
    ));
    lua.gc_stop();
    lua
}

fn snapshot(lua: &Lua) -> String {
    fn context(value: &Context) -> String {
        format!("{}/{:?}/{:?}", value.owner, value.parents, value.camera)
    }
    fn call(value: &RawCall) -> String {
        let op = match &value.op {
            RawOp::Begin(pointer, preserve) => format!("begin {pointer} {preserve}"),
            RawOp::Finish(pointer) => format!("finish {pointer}"),
            RawOp::Draw(pointer, state, scope, vertices) => {
                format!("draw {pointer} {state:?} {} {vertices:?}", context(scope))
            }
        };
        format!("{} {op}", value.owner)
    }
    let Some(capture) = lua.app_data_ref::<Capture>() else {
        return "absent".into();
    };
    let labels: std::collections::BTreeMap<_, _> = capture.labels.iter().collect();
    format!(
        "{:?}/{:?}/{:?}/{:?}/{:?}/{labels:?}/{}/{}",
        capture.context.as_ref().map(context),
        capture
            .targets
            .iter()
            .map(|(pointer, scope)| (*pointer, context(scope)))
            .collect::<Vec<_>>(),
        capture.calls.iter().map(call).collect::<Vec<_>>(),
        capture.owners,
        capture
            .frames
            .iter()
            .map(|frame| (
                frame.second.to_bits(),
                frame.immediate,
                &frame.owners,
                frame.calls.iter().map(call).collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>(),
        capture.bytes,
        capture.frame_ops
    )
}

#[test]
fn live_extraction_preserves_frames_labels_errors_and_capture_state() {
    for (normal, immediate) in [(0, 0), (3, 0), (0, 1), (1, 1), (1, 3), (3, 2), (64, 3)] {
        for case in 0..12 {
            let mut outputs = Vec::new();
            let mut states = Vec::new();
            for variant in 0..3 {
                let lua = fixture(8, 17, normal, immediate);
                {
                    let mut capture = lua.app_data_mut::<Capture>().unwrap();
                    match case {
                        1 => capture.context = Some(Context::default()),
                        2 => capture.calls.push(RawCall {
                            owner: 1,
                            op: RawOp::Finish(1),
                        }),
                        3 => capture.targets.push((1, Context::default())),
                        4 if immediate > 0 => capture
                            .frames
                            .iter_mut()
                            .find(|f| f.immediate)
                            .unwrap()
                            .owners
                            .push(1),
                        5 if immediate > 0 => {
                            capture
                                .frames
                                .iter_mut()
                                .find(|f| f.immediate)
                                .unwrap()
                                .calls[0]
                                .owner = 900
                        }
                        6 if immediate > 0 => {
                            capture
                                .frames
                                .iter_mut()
                                .find(|f| f.immediate)
                                .unwrap()
                                .calls[0]
                                .op = RawOp::Begin(900, true)
                        }
                        7 if immediate > 0 => {
                            capture
                                .frames
                                .iter_mut()
                                .find(|f| f.immediate)
                                .unwrap()
                                .calls[0]
                                .op = RawOp::Finish(900)
                        }
                        8 if immediate > 0 => {
                            capture.labels.insert(900, "Quad 'missing texture'".into());
                            capture
                                .frames
                                .iter_mut()
                                .find(|f| f.immediate)
                                .unwrap()
                                .calls[0]
                                .op = RawOp::Draw(
                                900,
                                SongLuaOverlayState::default(),
                                Context::default(),
                                None,
                            );
                        }
                        9 if immediate > 0 => capture
                            .frames
                            .iter_mut()
                            .find(|f| f.immediate)
                            .unwrap()
                            .owners
                            .clear(),
                        _ => {}
                    }
                }
                if case == 10 {
                    lua.remove_app_data::<LiveSources>();
                }
                if case == 11 {
                    lua.remove_app_data::<Capture>();
                }
                let output = match variant {
                    0 => before::take_live(&lua),
                    1 => before_partition(&lua),
                    _ => take_live(&lua),
                };
                outputs.push(format!("{output:?}"));
                states.push(snapshot(&lua));
            }
            assert_eq!(
                outputs[0], outputs[2],
                "final output: normal={normal} immediate={immediate} case={case}"
            );
            assert_eq!(
                states[0], states[2],
                "final capture state: normal={normal} immediate={immediate} case={case}"
            );
            assert_eq!(
                outputs[0], outputs[1],
                "normal={normal} immediate={immediate} case={case}"
            );
            assert_eq!(
                states[0], states[1],
                "capture state: normal={normal} immediate={immediate} case={case}"
            );
        }
    }
}

fn tostring_pair(lua: &Lua, kind: Option<mlua::Value>) -> (Table, Function, Function) {
    let actor = lua.create_table().unwrap();
    if let Some(kind) = kind {
        actor.raw_set("__songlua_actor_type", kind).unwrap();
    }
    crate::lua_util::install_actor_metatable(lua, &actor, |_, _| Ok(())).unwrap();
    let current = actor
        .metatable()
        .unwrap()
        .get::<Function>("__tostring")
        .unwrap();
    let original = lua.create_function(before::actor_tostring).unwrap();
    (actor, original, current)
}

fn error_text(error: mlua::Error) -> String {
    if let mlua::Error::CallbackError { cause, .. } = error {
        cause.to_string()
    } else {
        error.to_string()
    }
}

#[test]
fn actor_tostring_preserves_type_aliases_identity_coercions_and_errors() {
    let lua = Lua::new();
    for name in [
        "Actor",
        "Quad",
        "WrapperState",
        "TopScreen",
        "Sprite",
        "",
        "\0Type",
        "型別",
        &"x".repeat(4096),
    ] {
        let (actor, old, new) = tostring_pair(
            &lua,
            Some(mlua::Value::String(lua.create_string(name).unwrap())),
        );
        assert_eq!(
            old.call::<String>(&actor).unwrap(),
            new.call::<String>(&actor).unwrap()
        );
    }
    for value in [
        None,
        Some(mlua::Value::Number(1.25)),
        Some(mlua::Value::Boolean(true)),
        Some(mlua::Value::Table(lua.create_table().unwrap())),
        Some(mlua::Value::String(
            lua.create_string([0xff, 0xfe]).unwrap(),
        )),
    ] {
        let (actor, old, new) = tostring_pair(&lua, value);
        assert_eq!(
            old.call::<String>(&actor).map_err(error_text),
            new.call::<String>(&actor).map_err(error_text)
        );
    }
}

#[cfg(windows)]
fn compare<T>(
    name: &str,
    count: usize,
    mut prepare: impl FnMut() -> T,
    mut work: impl FnMut(&T, bool),
) {
    use crate::draw_capture::perf::{allocations, cycles};
    use std::time::Instant;
    let mut samples = Vec::new();
    for round in 0..10 {
        for variant in [round % 2, 1 - round % 2] {
            let inputs: Vec<_> = (0..count).map(|_| prepare()).collect();
            let start_cycles = cycles();
            let start = Instant::now();
            let (_, churn) = allocations::measure(|| {
                for input in &inputs {
                    work(input, black_box(variant == 1));
                }
            });
            let nanos = start.elapsed().as_nanos();
            let used = cycles() - start_cycles;
            if round > 0 {
                samples.push(serde_json::json!({"round":round, "current":variant == 1,
                    "iterations":count,"nanoseconds":nanos,"cycles":used,
                    "allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes}));
            }
        }
    }
    println!(
        "LIVE_EXTRACT_METRIC {}",
        serde_json::json!({"workload":name,"samples":samples})
    );
}

#[cfg(windows)]
#[test]
#[ignore = "paired release benchmark; run alone with --nocapture"]
fn benchmark_live_extract() {
    let _affinity = crate::draw_capture::perf::pin_cpu2();
    let filter = std::env::var("DEADSYNC_LIVE_EXTRACT_BENCH").unwrap_or_default();
    let variant = std::env::var("DEADSYNC_LIVE_EXTRACT_VARIANT").unwrap_or_default();
    let partition = variant == "partition";
    let labels_only = variant == "labels";
    if filter.is_empty() || filter == "draw" {
        for (name, labels, bytes, normal, immediate) in [
            ("draw_quiet", 128, 16, 3, 0),
            ("draw_one", 1, 16, 0, 1),
            ("draw_normal_one", 1, 16, 1, 1),
            ("draw_normal_one_multi", 128, 16, 1, 3),
            ("draw_labels128", 128, 16, 0, 1),
            ("draw_long_labels128", 128, 4096, 0, 1),
            ("draw_mixed", 128, 16, 4, 3),
            ("draw_retained128", 1, 16, 128, 1),
        ] {
            compare(
                name,
                32,
                || fixture(labels, bytes, normal, immediate),
                |lua, current| {
                    black_box(if current {
                        if labels_only {
                            before_partition(lua)
                        } else {
                            take_live(lua)
                        }
                    } else {
                        if partition {
                            before_partition(lua)
                        } else {
                            before::take_live(lua)
                        }
                    })
                    .unwrap();
                },
            );
        }
    }
    if filter.is_empty() || filter == "text" {
        for name in ["Actor", "Quad", "WrapperState", "TopScreen", "Sprite", ""] {
            compare(
                &format!("tostring_{name}"),
                16,
                || {
                    let lua = Lua::new();
                    let kind = lua.create_string(name).unwrap();
                    let (actor, old, new) = tostring_pair(&lua, Some(mlua::Value::String(kind)));
                    lua.gc_stop();
                    (lua, actor, old, new)
                },
                |(_, actor, old, new), current| {
                    let function = if current { new } else { old };
                    for _ in 0..128 {
                        black_box(function.call::<String>(actor).unwrap());
                    }
                },
            );
        }
    }
}

// Frozen after label borrowing, before retaining the frame buffer.
fn before_partition(lua: &Lua) -> Result<Vec<DrawFrame>, String> {
    if lua.app_data_ref::<Capture>().is_none_or(|capture| {
        !capture.frames.iter().any(|frame| frame.immediate) || capture.context.is_some()
    }) {
        return Ok(Vec::new());
    }
    if lua.app_data_ref::<LiveSources>().is_none() {
        return Ok(Vec::new());
    }
    let mut capture = lua
        .remove_app_data::<Capture>()
        .expect("complete capture frames");
    let (immediate, normal) = std::mem::take(&mut capture.frames)
        .into_iter()
        .partition::<Vec<_>, _>(|frame| frame.immediate);
    capture.frames = normal;
    // Conversion reads labels only for errors and invokes no Lua callbacks.
    // Borrow them before restoring capture, including when conversion fails.
    let result = {
        let sources = lua
            .app_data_ref::<LiveSources>()
            .expect("live source bindings");
        convert(immediate, &capture.labels, &sources.0)
    };
    if capture.frames.is_empty() && capture.calls.is_empty() && capture.targets.is_empty() {
        capture.bytes = 0;
        capture.frame_ops = 0;
        capture.labels.clear();
    }
    lua.set_app_data(capture);
    result.map(|frames| frames.into_iter().map(|(_, frame)| frame).collect())
}
