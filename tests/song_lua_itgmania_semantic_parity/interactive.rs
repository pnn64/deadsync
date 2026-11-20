//! Independent input and final draw-stream checks for executable captures.
use super::{gameplay, native_creation_order_with_coverage_for_entries};
use deadlib_present::render::{DrawOp, RenderFrame};
use deadsync_assets::song_lua::{SongLuaOverlayActor, SongLuaOverlayKind};
use deadsync_song_lua::playback::actor_conformance::{
    WholeSongComposer, compose_layer_overlay_states, matrix_rows, project_world,
};
use deadsync_song_lua::{SongLuaInputEvent, SongLuaInputType, SongLuaLiveTarget, SongLuaOverlayState};
use serde_json::{Value, json};
use std::{collections::{BTreeMap, VecDeque}, path::Path};

#[path = "render_random.rs"]
mod render_random;

pub(super) fn validate(trace: &Value) -> Result<(), String> {
    render_random::validate(trace)?;
    let injected = trace["input"]["injected_inputs"] == true;
    let inputs = trace["capabilities"]["native_input_events"] == true;
    let draws = trace["capabilities"]["native_drawable_vertices"] == true;
    if !inputs && !draws {
        return Ok(());
    }
    if !trace["interactive_observer_error"].is_null() {
        return Err("native interactive observer failed".into());
    }
    let frames = trace["update_frames"]
        .as_array()
        .ok_or("missing interactive frame anchors")?;
    if inputs {
        let events = trace["input_events"]
            .as_array()
            .ok_or("missing native input events")?;
        if trace["dropped_input_events"] != 0 {
            return Err("native input capture dropped events".into());
        }
        let mut previous = 0;
        for (i, event) in events.iter().enumerate() {
            let at = event["before_update_sequence"].as_u64().unwrap_or(0);
            if at == 0
                || at < previous
                || at > frames.len() as u64
                || event["sequence"].as_u64() != Some(i as u64 + 1)
                || event["scene"] != frames[at as usize - 1]["scene"]
                || !matches!(
                    event["event"]["type"].as_str(),
                    Some(
                        "InputEventType_FirstPress"
                            | "InputEventType_Repeat"
                            | "InputEventType_Release"
                    )
                )
            {
                return Err("invalid native input event or frame anchor".into());
            }
            if trace["capabilities"]["native_input_step_clocks"] == true {
                let clocks = event["step_clock_inputs"].as_array().ok_or("missing native input step clocks")?;
                if clocks.len() > 2 || (event["event"]["type"] == "InputEventType_Repeat" && !clocks.is_empty()) {
                    return Err("invalid native input step clock count".into());
                }
            }
            previous = at;
        }
        if injected {
            let script = trace["input"]["script"]
                .as_array()
                .filter(|s| !s.is_empty())
                .ok_or("missing injected input script")?;
            let commands = trace["injected_commands"]
                .as_array()
                .ok_or("missing native injection records")?;
            if commands.len() != script.len() {
                return Err("incomplete input injection".into());
            }
            for (command, expected) in commands.iter().zip(script) {
                let player = format!("PlayerNumber_P{}", expected["player"].as_u64().unwrap_or(0));
                let kind = if expected["down"] == true {
                    "InputEventType_FirstPress"
                } else {
                    "InputEventType_Release"
                };
                if command["command"] != *expected
                    || !events.iter().any(|event| {
                        event["scene"] == expected["scene"]
                            && event["event"]["button"] == expected["button"]
                            && event["event"]["PlayerNumber"] == player
                            && event["event"]["type"] == kind
                            && event["before_update_sequence"].as_u64()
                                >= command["before_update_sequence"].as_u64()
                    })
                {
                    return Err("scripted input did not reach a native callback".into());
                }
            }
        }
    }
    if draws {
        if trace["dropped_drawable_samples"] != 0 {
            return Err("native drawable capture dropped observations".into());
        }
        let samples = trace["drawable_samples"]
            .as_array()
            .ok_or("native drawable samples missing")?;
        let boundaries = trace["drawable_frames"]
            .as_array()
            .filter(|s| !s.is_empty())
            .ok_or("native draw boundaries missing")?;
        let mut consumed = 0;
        let mut previous = 0;
        for boundary in boundaries {
            let at = boundary["update_sequence"].as_u64().unwrap_or(0);
            let count = boundary["samples"].as_u64().unwrap_or(u64::MAX);
            if at == 0
                || at < previous
                || at > frames.len() as u64
                || count < consumed
                || count > samples.len() as u64
            {
                return Err("invalid native draw boundary".into());
            }
            for sample in &samples[consumed as usize..count as usize] {
                if sample["coordinate_space"] != "native-projection-before-display-centering"
                    || sample["update_sequence"] != at
                    || sample["scene"] != frames[at as usize - 1]["scene"]
                {
                    return Err("native drawable has no complete frame anchor".into());
                }
                if sample["state"]["texture"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty())
                    && [
                        "image_width",
                        "image_height",
                        "texture_width",
                        "texture_height",
                    ]
                    .iter()
                    .any(|key| {
                        !sample["state"][key]
                            .as_f64()
                            .is_some_and(|x| x.is_finite() && x > 0.0)
                    })
                {
                    return Err("native texture allocation dimensions are missing".into());
                }
                let rows = sample["vertices"]
                    .as_array()
                    .filter(|s| !s.is_empty())
                    .ok_or("missing native draw vertices")?;
                for row in rows {
                    let values = row
                        .as_array()
                        .filter(|r| r.len() == 13)
                        .ok_or("invalid native vertex row")?;
                    if values.iter().any(|v| {
                        !v.is_number()
                            && !(v["type"] == "number"
                                && matches!(v["value"].as_str(), Some("nan" | "inf" | "-inf")))
                    }) || values[7..].iter().any(|v| !v.is_number())
                        || values[..3].iter().all(Value::is_number)
                            && values[3..7].iter().any(|v| !v.is_number())
                    {
                        return Err("invalid native draw projection or attributes".into());
                    }
                }
            }
            consumed = count;
            previous = at;
        }
        if consumed != samples.len() as u64 {
            return Err("native drawable tail is incomplete".into());
        }
    }
    Ok(())
}

pub(super) struct InputReplay {
    slots: BTreeMap<(String, String), u32>,
    origin: std::time::Instant,
}

impl Default for InputReplay {
    fn default() -> Self { Self { slots: BTreeMap::new(), origin: std::time::Instant::now() } }
}

impl InputReplay {
    fn gameplay(
        &mut self, state: &mut gameplay::State, input: &SongLuaInputEvent<'_>,
        clocks: &Value, snapshot: deadsync_gameplay::GameplayAudioSnapshot,
    ) -> Result<(), String> {
        use deadsync_input::{InputEvent, VirtualAction as A};
        use deadsync_gameplay::{GameplayInputPlayerSide, current_song_clock_snapshot, handle_core_input, process_input_edges};
        use std::time::Duration;
        // Native ScreenGameplay sends only first presses and releases to Step.
        // Recorded repeat events already exist and must not enter held timers.
        let no_step = || {
            if clocks.as_array().is_some_and(|clocks| !clocks.is_empty()) {
                Err("native stepped an input that has no DeadSync gameplay route".into())
            } else { Ok(()) }
        };
        if input.kind == SongLuaInputType::Repeat { return no_step(); }
        let Some(controller) = input.controller else { return no_step(); };
        let pump = state.gameplay.setup.session.play_style.is_pump();
        let pair = match input.button {
            "Left" if !pump => [A::p1_left, A::p2_left],
            "Down" if !pump => [A::p1_down, A::p2_down],
            "Up" if !pump => [A::p1_up, A::p2_up],
            "Right" if !pump => [A::p1_right, A::p2_right],
            "DownLeft" if pump => [A::p1_left, A::p2_left],
            "UpLeft" if pump => [A::p1_down, A::p2_down],
            "Center" if pump => [A::p1_center, A::p2_center],
            "UpRight" if pump => [A::p1_up, A::p2_up],
            "DownRight" if pump => [A::p1_right, A::p2_right],
            "Start" => [A::p1_start, A::p2_start],
            "Back" => [A::p1_back, A::p2_back],
            "Select" => [A::p1_select, A::p2_select],
            "MenuLeft" => [A::p1_menu_left, A::p2_menu_left],
            "MenuRight" => [A::p1_menu_right, A::p2_menu_right],
            "MenuUp" => [A::p1_menu_up, A::p2_menu_up],
            "MenuDown" => [A::p1_menu_down, A::p2_menu_down],
            _ => return no_step(),
        };
        let next_slot = self.slots.len() as u32;
        let slot = *self.slots.entry((input.device.into(), input.device_button.into())).or_insert(next_slot);
        let host = snapshot.stream_clock.valid_at_host_nanos;
        let ago = Duration::try_from_secs_f64(input.ago).map_err(|_| "invalid native input age")?;
        let timestamp = self.origin.checked_add(Duration::from_nanos(host)).and_then(|t| t.checked_sub(ago))
            .ok_or("native input timestamp out of range")?;
        let event = InputEvent {
            action: pair[controller], input_slot: slot, pressed: input.kind == SongLuaInputType::FirstPress,
            source: if input.is_joystick { deadsync_core::input::InputSource::Gamepad } else { deadsync_core::input::InputSource::Keyboard },
            timestamp, timestamp_host_nanos: host.saturating_sub(ago.as_nanos().min(u64::MAX as u128) as u64),
            stored_at: timestamp, emitted_at: timestamp,
        };
        if !state.gameplay.pending_input_is_empty() { return Err("recorded input phase has undelivered gameplay edges".into()); }
        handle_core_input(&mut state.gameplay, &event);
        let mut edges = std::mem::take(&mut state.gameplay.pending_input.edges);
        if !edges.is_empty() || clocks.is_array() {
            let clocks = clocks.as_array().ok_or("manual gameplay input requires native step clock observations; recapture with harness 0.1.103 or newer")?;
            if clocks.len() != edges.len() {
                return Err(format!("input routing differs: DeadSync queued {} lanes; native stepped {} players", edges.len(), clocks.len()));
            }
            for (edge, clock) in edges.iter_mut().zip(clocks) {
                let player = state.gameplay.player_for_col(edge.lane.index());
                let native_player = usize::from(state.gameplay.runtime_player_side(player) == GameplayInputPlayerSide::P2);
                let column = edge.lane.index() - player * state.gameplay.cols_per_player();
                if clock["player"].as_u64() != Some(native_player as u64) || clock["column"].as_u64() != Some(column as u64) {
                    return Err(format!("input routing differs for player {native_player}, column {column}: {clock}"));
                }
                let number = |key: &str| clock[key].as_f64().filter(|n| n.is_finite() && (*n as f32).is_finite())
                    .map(|n| n as f32).ok_or_else(|| format!("missing native input clock {key}"));
                let rate = number("music_rate")?;
                if rate <= 0. { return Err("invalid native input music rate".into()); }
                // Player::Step's f32 arithmetic, using only raw clock inputs.
                // Native player/column observations above check our routing;
                // they never select a lane, note, judgment, or score for us.
                let current = number("music_seconds")? + number("last_beat_update_ago")? * rate;
                edge.event_music_time_ns = deadsync_core::song_time::song_time_ns_from_seconds(current - number("step_ago")? * rate);
                edge.record_replay = false;
            }
        }
        state.gameplay.pending_input.edges = edges;
        let clock = current_song_clock_snapshot(snapshot, state.gameplay.music_rate(), 0.);
        process_input_edges(&mut state.gameplay, false, &mut Default::default(), clock);
        Ok(())
    }
}

impl InputReplay {
pub(super) fn dispatch(
    &mut self, state: &mut gameplay::State, record: &Value,
    snapshot: deadsync_gameplay::GameplayAudioSnapshot,
) -> Result<(), String> {
    let event = &record["event"];
    let text = |key| event[key].as_str().unwrap_or("");
    let device = &event["DeviceInput"];
    let input = SongLuaInputEvent {
        player: match text("PlayerNumber") {
            "PlayerNumber_P1" => Some(0),
            "PlayerNumber_P2" => Some(1),
            _ => None,
        },
        controller: match text("controller") {
            "GameController_1" => Some(0),
            "GameController_2" => Some(1),
            _ => None,
        },
        button: text("button"),
        game_button: text("GameButton"),
        kind: match text("type") {
            "InputEventType_FirstPress" => SongLuaInputType::FirstPress,
            "InputEventType_Repeat" => SongLuaInputType::Repeat,
            "InputEventType_Release" => SongLuaInputType::Release,
            _ => return Err("invalid recorded input type".into()),
        },
        device: device["device"].as_str().unwrap_or("InputDevice_Invalid"),
        device_button: device["button"].as_str().unwrap_or("DeviceButton_Invalid"),
        level: device["level"].as_f64().unwrap_or(0.0) as f32,
        z: device["z"].as_i64().unwrap_or(0) as i32,
        down: device["down"].as_bool().unwrap_or(false),
        ago: device["ago"].as_f64().unwrap_or(0.0),
        is_joystick: device["is_joystick"].as_bool().unwrap_or(false),
        is_mouse: device["is_mouse"].as_bool().unwrap_or(false),
    };
    self.gameplay(state, &input, &record["step_clock_inputs"], snapshot)?;
    gameplay::handle_song_lua_input(state, &input);
    Ok(())
}
}

#[derive(Default)]
pub(super) struct DrawChecks {
    bindings: BTreeMap<String, SongLuaLiveTarget>,
    binding_paths: BTreeMap<String, String>,
    targets: BTreeMap<String, SongLuaLiveTarget>,
    random: Option<render_random::Replay>,
    composers: BTreeMap<i32, WholeSongComposer>,
    native_aft_resources: BTreeMap<String, NativeAftResource>,
    aft_backing_uv_samples: usize,
    pub samples: usize,
    pub vertices: usize,
    pub text_samples: usize,
    pub failures: usize,
    pub culled_samples: usize,
    pub degenerate_samples: usize,
    pub batched_samples: usize,
    pub uncompared_immediate_draws: usize,
    pub uncompared_normal_draws: usize,
    pub missing_random_inputs: usize,
    failure_counts: BTreeMap<String, usize>,
    failure_examples: BTreeMap<(String, String), Value>,
    last_failure: Option<Value>,
}

struct RecordedDraw {
    frame: RenderFrame,
    texture: Option<String>,
    aft_backing: Option<AftBacking>,
    immediate: bool,
}

#[derive(Default)]
struct RecordedDraws {
    draws: BTreeMap<(i32, usize), VecDeque<RecordedDraw>>,
    normal_owned: std::collections::BTreeSet<(i32, usize)>,
}

fn normal_draw_subtree(
    overlays: &[SongLuaOverlayActor], owners: &std::collections::BTreeSet<usize>,
) -> std::collections::BTreeSet<usize> {
    (0..overlays.len()).filter(|&index| {
        std::iter::successors(Some(index), |&index| overlays[index].parent_index)
            .any(|index| owners.contains(&index))
    }).collect()
}

fn native_draw_vertex_count(sample: &Value) -> Result<usize, String> {
    let count = sample["vertices"].as_array().ok_or("draw without vertices")?.len();
    match sample["primitive"].as_str() {
        Some("Quads") if count > 0 && count % 4 == 0 => Ok(count / 4 * 6),
        Some("QuadStrip") if count >= 4 && count % 2 == 0 => Ok((count - 2) / 2 * 6),
        Some("Triangles") if count > 0 && count % 3 == 0 => Ok(count),
        _ => Err(format!("unsupported native primitive in draw comparison: {:?} with {count} vertices",
            sample["primitive"].as_str())),
    }
}

fn native_draw_geometry_matches(
    primitive: &str,
    rows: &[[f32; 10]],
    reference: &[Value],
    matches: impl Fn(&[f32; 10], &Value) -> bool,
) -> bool {
    let (stride, order) = match primitive {
        "Quads" => (4, [0, 1, 2, 3]),
        // GL_QUAD_STRIP shares the last pair with the next quad. Its perimeter
        // is 0,1,3,2, unlike four independent GL_QUADS vertices. Preserve every
        // pair boundary and compare both complete triangles, including attributes.
        "QuadStrip" => (2, [0, 1, 3, 2]),
        "Triangles" => return !rows.is_empty() && rows.len() % 3 == 0
            && rows.len() == reference.len()
            && rows.iter().zip(reference).all(|(actual, native)| matches(actual, native)),
        _ => return false,
    };
    if reference.len() < 4 || (reference.len() - 4) % stride != 0
        || rows.len() != ((reference.len() - 4) / stride + 1) * 6
    { return false; }
    rows.chunks_exact(6).zip(reference.windows(4).step_by(stride)).all(|(actual, native)| {
        // Native quads permit either diagonal. Retain both original triangles
        // and their attributes; merely finding each corner somewhere is insufficient.
        let triangle = |actual: &[[f32; 10]], indices: [usize; 3]| {
            [[0,1,2],[0,2,1],[1,0,2],[1,2,0],[2,0,1],[2,1,0]].iter().any(|p|
                (0..3).all(|i| matches(&actual[p[i]], &native[order[indices[i]]])))
        };
        [([0,1,2],[0,2,3]),([0,1,3],[1,2,3])].iter().any(|&(a,b)|
            triangle(&actual[..3], a) && triangle(&actual[3..], b)
                || triangle(&actual[..3], b) && triangle(&actual[3..], a))
    })
}

fn native_sprite_piece_state_matches(first: &Value, previous: &Value, next: &Value) -> bool {
    let (first, previous, next) = (&first["state"], &previous["state"], &next["state"]);
    if first.get("actor_draw_sequence").is_none()
        && previous.get("actor_draw_sequence").is_none()
        && next.get("actor_draw_sequence").is_none()
    {
        return first == next;
    }
    // The native hook increments geometry submission IDs for every DrawQuad,
    // but keeps Actor::Draw's scope ID across the faded pieces of one sprite.
    let Some(scope) = first["actor_draw_sequence"].as_u64().filter(|&id| id > 0) else { return false; };
    if previous["actor_draw_sequence"].as_u64() != Some(scope)
        || next["actor_draw_sequence"].as_u64() != Some(scope)
    { return false; }
    let Some(first_id) = first["native_draw_sequence"].as_u64().filter(|&id| id > 0) else { return false; };
    let Some(previous_id) = previous["native_draw_sequence"].as_u64().filter(|&id| id >= first_id) else { return false; };
    if !next["native_draw_sequence"].as_u64().is_some_and(|id| id > previous_id) { return false; }
    let (Some(first), Some(next)) = (first.as_object(), next.as_object()) else { return false; };
    first.len() == next.len() && first.iter().all(|(key, value)|
        key == "native_draw_sequence" || next.get(key) == Some(value))
}

fn native_draw_batch<'a>(
    samples: &'a [Value],
    owned_vertices: usize,
    sprite_pieces: bool,
) -> Result<(usize, usize, std::borrow::Cow<'a, [Value]>), String> {
    let first = samples.first().ok_or("empty native draw batch")?;
    let mut count = native_draw_vertex_count(first)?;
    let mut reference = std::borrow::Cow::Borrowed(first["vertices"].as_array().unwrap().as_slice());
    let mut consumed = 1;
    // Native Sprite::DrawPrimitives submits each faded piece through a separate
    // DrawQuad call. One owned draw may batch those quads in a single mesh. Its
    // independently emitted vertex count bounds this occurrence: never pool all
    // samples for an actor/frame or consume the next recorded ImmediateDraw.
    if sprite_pieces && first["primitive"] == "Quads" {
        for next in &samples[1..] {
            if count >= owned_vertices || ["actor", "definition_id", "scene", "update_sequence",
                "primitive", "coordinate_space", "render_random_sequences", "note_context",
                "native_draw_sequence", "draw_ordinal"]
                .into_iter().any(|key| next[key] != first[key])
                || !native_sprite_piece_state_matches(first, &samples[consumed - 1], next)
            {
                break;
            }
            let next_count = native_draw_vertex_count(next)?;
            if count + next_count > owned_vertices {
                break;
            }
            reference.to_mut().extend_from_slice(next["vertices"].as_array().unwrap());
            count += next_count;
            consumed += 1;
        }
    }
    Ok((consumed, count, reference))
}

#[test]
fn native_quad_strip_counts_keep_complete_pairs_and_submission_boundaries() {
    let point = json!([0,0,0, 0,0,0,1, 0,0, 1,1,1,1]);
    for (count, expected) in [
        (0, None), (1, None), (2, None), (3, None), (4, Some(6)),
        (5, None), (6, Some(12)), (7, None), (8, Some(18)),
    ] {
        let sample = json!({"primitive":"QuadStrip", "vertices":vec![point.clone();count]});
        assert_eq!(native_draw_vertex_count(&sample).ok(), expected, "{count} source vertices");
    }
    let sample = json!({"actor":"mesh", "primitive":"QuadStrip",
        "vertices":vec![point;6], "state":{"texture":"song:/capture"}});
    let samples = [sample.clone(), sample];
    for sprite_pieces in [false, true] {
        let (used, count, vertices) = native_draw_batch(&samples, 24, sprite_pieces).unwrap();
        assert_eq!((used, count, vertices.len()), (1, 12, 6),
            "adjacent strip submissions cannot be pooled into one owned draw");
    }
    let error = native_draw_vertex_count(&json!({"primitive":"QuadStrip", "vertices":vec![0;5]})).unwrap_err();
    assert!(error.contains("QuadStrip") && error.contains("5 vertices"));
}

#[test]
fn native_quad_strip_geometry_checks_topology_shared_pairs_and_all_attributes() {
    let positions = [[0.0,0.0], [0.0,1.0], [1.0,0.25], [1.0,1.5], [2.0,-0.5], [2.0,1.25]];
    let reference = positions.into_iter().enumerate().map(|(i, [x,y])| {
        let value = i as f32 / 10.0;
        json!([0,0,0, x,y,value,1, value,1.0-value, 1.0,value,0.5,1.0])
    }).collect::<Vec<_>>();
    let actual = |indices: &[usize]| indices.iter().map(|&index|
        std::array::from_fn(|axis| reference[index][axis+3].as_f64().unwrap() as f32)
    ).collect::<Vec<[f32;10]>>();
    let matches = |actual: &[f32;10], row: &Value| row_matches(actual, row, [1.0;2], false, [1.0,0.0]);
    let first = [[0,1,3,3,2,0], [0,1,2,1,3,2]];
    let second = [[2,3,5,5,4,2], [2,3,4,3,5,4]];
    for a in first {
        for b in second {
            let indices = [a,b].concat();
            assert!(native_draw_geometry_matches("QuadStrip", &actual(&indices), &reference, matches));
        }
    }
    let valid = actual(&[0,1,3,3,2,0,2,3,5,5,4,2]);
    for axis in 0..10 {
        let mut changed = valid.clone();
        changed[4][axis] += 0.125;
        assert!(!native_draw_geometry_matches("QuadStrip", &changed, &reference, matches),
            "changing clip/UV/RGBA axis {axis} must remain a mismatch");
    }
    for indices in [
        [0,1,2,2,3,0,2,3,4,4,5,2], // Treating each pair window as an ordinary quad twists it.
        [0,1,3,3,2,0,0,1,3,3,2,0], // Repeating a cell cannot replace its neighbor.
        [0,1,5,5,4,0,2,3,5,5,4,2], // Corners from a different cell cannot bridge a missing pair.
    ] {
        assert!(!native_draw_geometry_matches("QuadStrip", &actual(&indices), &reference, matches));
    }
    assert!(!native_draw_geometry_matches("QuadStrip", &valid[..9], &reference, matches));
    assert!(!native_draw_geometry_matches("QuadStrip", &valid, &reference[..5], matches));
    assert!(native_draw_geometry_matches("Quads", &actual(&[0,1,2,2,3,0]), &reference[..4], matches));
    assert!(native_draw_geometry_matches("Triangles", &actual(&[0,1,2]), &reference[..3], matches));
}

#[test]
fn native_draw_batches_preserve_occurrences_and_all_piece_boundaries() {
    let samples = (0..6).map(|piece| json!({
        "actor":"sprite", "definition_id":"def-1", "scene":1, "update_sequence":2,
        "primitive":"Quads", "coordinate_space":"native", "state":{"texture":"song:/a.png"},
        "render_random_sequences":[7], "vertices":vec![json!([piece]);4],
    })).collect::<Vec<_>>();
    let (used, count, vertices) = native_draw_batch(&samples, 18, true).unwrap();
    assert_eq!((used, count, vertices.len()), (3, 18, 12));
    assert_eq!([&vertices[0][0], &vertices[4][0], &vertices[8][0]], [&json!(0), &json!(1), &json!(2)]);
    let (used, count, vertices) = native_draw_batch(&samples[used..], 18, true).unwrap();
    assert_eq!((used, count, vertices.len()), (3, 18, 12));
    assert_eq!(vertices[0][0], 3, "the next immediate occurrence keeps its own first piece");
    assert_eq!(native_draw_batch(&samples, 6, true).unwrap().0, 1);
    assert_eq!(native_draw_batch(&samples, 18, false).unwrap().0, 1);
    let (used, count, _) = native_draw_batch(&samples[..2], 18, true).unwrap();
    assert_eq!((used, count), (2, 12), "missing pieces must leave a vertex-count mismatch");

    for (field, value) in [
        ("actor", json!("other")), ("definition_id", json!("def-2")),
        ("scene", json!(2)), ("update_sequence", json!(3)), ("primitive", json!("Triangles")),
        ("coordinate_space", json!("other")), ("state", json!({"texture":"song:/b.png"})),
        ("render_random_sequences", json!([8])), ("native_draw_sequence", json!(1)),
        ("draw_ordinal", json!(1)), ("note_context", json!({"player":1})),
    ] {
        let mut changed = samples[..2].to_vec();
        changed[1][field] = value;
        assert_eq!(native_draw_batch(&changed, 12, true).unwrap().0, 1, "crossed {field}");
    }
    let mut oversized = samples[..2].to_vec();
    oversized[1]["vertices"] = json!(vec![json!([1]);8]);
    assert_eq!(native_draw_batch(&oversized, 12, true).unwrap().0, 1,
        "a native submission cannot be split across owned occurrences");
}

#[test]
fn native_sprite_batches_use_actor_scope_and_ordered_geometry_submissions() {
    let samples = (0..6).map(|piece| json!({
        "actor":"sprite", "definition_id":"def-1", "scene":1, "update_sequence":2,
        "primitive":"Quads", "coordinate_space":"native",
        "state":{"texture":"song:/fire.png", "blend":1, "actor_draw_sequence":77 + piece / 3,
            "native_draw_sequence":501 + piece},
        "render_random_sequences":[], "vertices":vec![json!([piece]);4],
    })).collect::<Vec<_>>();
    let (used, count, vertices) = native_draw_batch(&samples, 18, true).unwrap();
    assert_eq!((used, count, vertices.len()), (3, 18, 12));
    assert_eq!(native_draw_batch(&samples[used..], 18, true).unwrap().0, 3);
    assert_eq!(native_draw_batch(&samples, 36, true).unwrap().0, 3,
        "a larger owned mesh cannot absorb the next Actor::Draw occurrence");
    for (field, value) in [
        ("actor_draw_sequence", json!(78)), ("actor_draw_sequence", Value::Null),
        ("native_draw_sequence", json!(501)), ("native_draw_sequence", Value::Null),
        ("texture", json!("song:/different.png")), ("blend", json!(2)),
    ] {
        let mut changed = samples[..3].to_vec();
        changed[1]["state"][field] = value;
        assert_eq!(native_draw_batch(&changed, 18, true).unwrap().0, 1, "crossed {field}");
    }
    let mut reordered = samples[..3].to_vec();
    reordered[2]["state"]["native_draw_sequence"] = json!(501);
    assert_eq!(native_draw_batch(&reordered, 18, true).unwrap().0, 2,
        "geometry IDs must remain increasing across every piece");
    let mut missing_scope = samples[..3].to_vec();
    missing_scope[1]["state"].as_object_mut().unwrap().remove("actor_draw_sequence");
    assert_eq!(native_draw_batch(&missing_scope, 18, true).unwrap().0, 1);
    let mut new_state = samples[..3].to_vec();
    new_state[1]["state"]["z_bias"] = json!(0);
    assert_eq!(native_draw_batch(&new_state, 18, true).unwrap().0, 1,
        "only the proved geometry counter may differ within one actor draw");
}

fn target_slot(target: SongLuaLiveTarget) -> (i32, usize) {
    match target {
        SongLuaLiveTarget::Root(i) => (0, i),
        SongLuaLiveTarget::Background(l, i) => (-(l as i32) - 1, i),
        SongLuaLiveTarget::Foreground(l, i) => (l as i32 + 1, i),
    }
}

fn layer(state: &gameplay::State, group: i32) -> &[SongLuaOverlayActor] {
    let v = &state.gameplay.mods.song_lua_visuals;
    if group == 0 {
        &v.overlays
    } else if group < 0 {
        &v.background_visual_layers[(-group - 1) as usize].overlays
    } else {
        &v.foreground_visual_layers[(group - 1) as usize].overlays
    }
}

fn overlay_screen_state(
    overlays: &[SongLuaOverlayActor],
    index: usize,
    screen: Option<SongLuaOverlayState>,
) -> Option<SongLuaOverlayState> {
    let mut parent = overlays[index].parent_index;
    while let Some(index) = parent {
        let actor = &overlays[index];
        // RageTextureRenderTarget::BeginRenderingTo resets both the model
        // matrix and camera. The displayed AFT sprite still inherits its own
        // screen parent unless it is itself inside another capture.
        if matches!(actor.kind, SongLuaOverlayKind::ActorFrameTexture { .. }) {
            return None;
        }
        parent = actor.parent_index;
    }
    screen
}

#[test]
fn native_draw_screen_projection_stops_at_texture_capture() {
    let actor = |kind, parent_index| SongLuaOverlayActor {
        kind, parent_index, name: None, initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    };
    let overlays = vec![
        actor(SongLuaOverlayKind::ActorFrame, None),
        actor(SongLuaOverlayKind::ActorFrameTexture {
            capture_name: "capture".into(), alpha_buffer: true, depth_buffer: false,
            float_buffer: false, capture_size: Some([320.0, 240.0]),
        }, Some(0)),
        actor(SongLuaOverlayKind::ActorFrame, Some(1)),
        actor(SongLuaOverlayKind::Actor, Some(2)),
        actor(SongLuaOverlayKind::AftSprite { capture_name: "capture".into() }, Some(0)),
    ];
    let screen = SongLuaOverlayState { x: 17.0, fov: Some(60.0), ..SongLuaOverlayState::default() };
    let inherited = overlay_screen_state(&overlays, 4, Some(screen)).expect("displayed AFT sprite inherits screen");
    assert_eq!(inherited.x, 17.0);
    assert_eq!(inherited.fov, Some(60.0));
    assert!(overlay_screen_state(&overlays, 3, Some(screen)).is_none(), "capture descendants keep their own viewport");
    assert!(overlay_screen_state(&overlays, 0, None).is_none(), "missing screen state remains absent");
}


#[test]
fn normal_draw_callbacks_suppress_only_their_owned_subtree() {
    let actor = |parent_index| SongLuaOverlayActor {
        kind: SongLuaOverlayKind::ActorFrame, parent_index, name: None,
        initial_state: SongLuaOverlayState::default(), message_commands: Vec::new(),
    };
    let overlays = vec![actor(None), actor(Some(0)), actor(Some(1)), actor(Some(0)), actor(None)];
    assert!(normal_draw_subtree(&overlays, &std::collections::BTreeSet::new()).is_empty(),
        "immediate draws add no normal owner and do not suppress automatic drawing");
    assert_eq!(normal_draw_subtree(&overlays, &std::collections::BTreeSet::from([1])),
        std::collections::BTreeSet::from([1,2]), "a quiet normal callback still owns its descendants");
    assert_eq!(normal_draw_subtree(&overlays, &std::collections::BTreeSet::from([0])),
        std::collections::BTreeSet::from([0,1,2,3]), "unrelated roots remain automatic");
}

impl DrawChecks {
    fn recorded(
        &self, state: &gameplay::State, screen: [f32; 2], clock: [f32; 2],
    ) -> Result<RecordedDraws, String> {
        let frames = state.song_lua_draw_frames();
        if frames.is_empty() { return Ok(RecordedDraws::default()); }
        let scene = state.song_lua.as_ref().ok_or("draw submissions have no owned scene")?;
        let bindings = scene.visual_draw_bindings();
        let globals: BTreeMap<_, _> = bindings.iter().enumerate().map(|(dense, &(global, _))| (global, dense)).collect();
        let slots: BTreeMap<_, _> = bindings.iter().enumerate().map(|(dense, &(_, target))| (target_slot(target), dense)).collect();
        let mut overlays = Vec::with_capacity(bindings.len());
        for &(_, target) in bindings {
            let (group, index) = target_slot(target);
            let mut overlay = layer(state, group)[index].clone();
            overlay.parent_index = overlay.parent_index.and_then(|parent| slots.get(&(group, parent)).copied());
            if let SongLuaOverlayKind::ActorProxy { target: deadsync_song_lua::SongLuaProxyTarget::Actor { overlay_index } } = &mut overlay.kind {
                // ActorProxy::EarlyAbortDraw skips an unset native target.
                // Keep the owned sentinel so the composer skips it too.
                if *overlay_index != usize::MAX {
                    *overlay_index = *slots.get(&(group, *overlay_index)).ok_or("draw proxy has no prepared target")?;
                }
            }
            overlays.push(overlay);
        }
        let states = overlays.iter().map(|actor| actor.initial_state).collect::<Vec<_>>();
        let mut composer = WholeSongComposer::new(&overlays);
        let mut recorded = RecordedDraws::default();
        let mut normal_owners = std::collections::BTreeSet::new();
        for original in frames {
            let mut frame = original.clone();
            let remap = |index: &mut usize| -> Result<(), String> {
                *index = *globals.get(index).ok_or("recorded draw has no prepared visual binding")?;
                Ok(())
            };
            for owner in &mut frame.owners { remap(owner)?; }
            if !frame.immediate { normal_owners.extend(frame.owners.iter().copied()); }
            for op in &mut frame.ops {
                match op {
                    deadsync_song_lua::SongLuaDrawOp::Begin { capture, .. }
                        | deadsync_song_lua::SongLuaDrawOp::Finish { capture } => remap(capture)?,
                    deadsync_song_lua::SongLuaDrawOp::Draw { source: deadsync_song_lua::SongLuaDrawSource::Overlay(index), .. } => remap(index)?,
                    _ => {}
                }
            }
            for (index, frame) in composer.render_recorded_frame(&overlays, &states, &frame, screen, clock[0], clock[1]) {
                let texture = frame.ops.first().and_then(|op| match op {
                    DrawOp::TexturedMesh(r) => Some(composer.texture_key(r.texture_handle).to_owned()),
                    DrawOp::Sprite(r) => Some(composer.texture_key(r.texture_handle).to_owned()),
                    _ => None,
                });
                let aft_backing = owned_aft_backing(&frame, &composer, &overlays);
                recorded.draws.entry(target_slot(bindings[index].1)).or_default()
                    .push_back(RecordedDraw { frame, texture, aft_backing, immediate: original.immediate });
            }
        }
        recorded.normal_owned = normal_draw_subtree(&overlays, &normal_owners).into_iter()
            .map(|index| target_slot(bindings[index].1)).collect();
        Ok(recorded)
    }

    pub fn bind(
        &mut self,
        state: &gameplay::State,
        trace: &Value,
        scene: u64,
    ) -> Result<(), String> {
        let owned = state.song_lua.as_ref();
        let (order, _) = native_creation_order_with_coverage_for_entries(
            trace,
            scene,
            owned.map_or(0, |scene| scene.context().background_layer_count),
        )?;
        let paths: BTreeMap<_, _> = order
            .into_iter()
            .map(|(path, pointer)| (pointer, path))
            .collect();
        let bindings = owned.ok_or("missing Lua scene")?
            .visual_bindings().map_err(|e| e.to_string())?;
        let targets: BTreeMap<_, _> = bindings.into_iter().collect();
        self.bindings.clear();
        self.binding_paths.clear();
        self.targets = targets.clone();
        self.composers.clear();
        self.native_aft_resources = native_aft_resources(trace, scene);
        if self.random.is_none() && trace["capabilities"]["native_render_random_inputs"] == true {
            self.random = Some(render_random::Replay::parse(trace)?);
        }
        for creation in trace["actor_creation_inputs"]
            .as_array()
            .ok_or("missing native factory inputs")?
        {
            if creation["scene"].as_u64() != Some(scene) {
                continue;
            }
            let pointer = usize::from_str_radix(
                creation["pointer_hex"].as_str().ok_or("missing pointer")?,
                16,
            )
            .map_err(|e| e.to_string())?;
            if let Some(target) = paths
                .get(&pointer)
                .and_then(|path| targets.get(path))
                .copied()
            {
                self.bindings.insert(
                    creation["actor"].as_str().ok_or("missing actor ID")?.into(),
                    target,
                );
                self.binding_paths.insert(
                    creation["actor"].as_str().unwrap().into(), paths[&pointer].clone(),
                );
            }
        }
        for actor in trace["runtime_actors"].as_array().into_iter().flatten() {
            if !actor["definition_id"].is_null() { continue; }
            if let (Some(id), Some(path)) = (actor["id"].as_str(), actor["path"].as_str())
                && let Some(&target) = targets.get(path)
            {
                self.bindings.insert(id.into(), target);
                self.binding_paths.insert(id.into(), path.into());
            }
        }
        if let Some(sample) = trace["drawable_samples"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|sample| {
                sample["scene"].as_u64() == Some(scene)
                    && sample["actor"]
                        .as_str()
                        .is_some_and(|id| !self.bindings.contains_key(id))
            })
        {
            return Err(format!(
                "native drawable {} has no independently identified production visual slot",
                sample["actor"]
            ));
        }
        Ok(())
    }

    pub fn compare(
        &mut self,
        state: &gameplay::State,
        samples: &[Value],
        screen: [f32; 2],
        clock: [f32; 2],
        simfile: &Path,
        mismatches: &mut Vec<Value>,
    ) -> Result<(), String> {
        if let Some(scene) = state.song_lua.as_ref()
            && scene.visual_draw_bindings().len() != self.targets.len()
        {
            // Late AddWrapperState appends independently owned visual slots.
            // Original actor indexes remain stable; raw random source paths must
            // resolve against the newly published wrapper identities too.
            self.targets = scene.visual_bindings()?.into_iter().collect();
        }
        let mut states = BTreeMap::new();
        let mut recorded = self.recorded(state, screen, clock)?;
        let mut ordinary_draws = std::collections::BTreeSet::new();
        // ScreenGameplay is retained outside the independent song layer trees.
        // Use its live local pose for the same outer projection and fallback
        // camera as production playback; recorded draws already carry theirs.
        let visuals = &state.gameplay.mods.song_lua_visuals;
        let screen_state = visuals.screen_overlay_index
            .and_then(|index| visuals.overlays.get(index))
            .map(|actor| actor.initial_state);
        let mut sample_index = 0;
        while sample_index < samples.len() {
            let batch_start = sample_index;
            let sample = &samples[sample_index];
            sample_index += 1;
            let actor = sample["actor"]
                .as_str()
                .ok_or("draw without native actor")?;
            let target = self.bindings.get(actor).ok_or_else(|| {
                format!("native drawable {actor} has no production visual binding")
            })?;
            let (group, index) = match *target {
                SongLuaLiveTarget::Root(i) => (0, i),
                SongLuaLiveTarget::Background(l, i) => (-(l as i32) - 1, i),
                SongLuaLiveTarget::Foreground(l, i) => (l as i32 + 1, i),
            };
            let overlays = layer(state, group);
            let mut local = overlays.iter().map(|a| a.initial_state).collect::<Vec<_>>();
            let mut screen_state = screen_state;
            let mut ancestry = std::iter::successors(Some(index), |&index| overlays[index].parent_index)
                .collect::<Vec<_>>();
            ancestry.reverse();
            let mut vibrating = ancestry.into_iter().filter(|&index| local[index].vibrate)
                .map(|index| (group, index)).collect::<Vec<_>>();
            if let Some(index) = visuals.screen_overlay_index
                && screen_state.is_some_and(|state| state.vibrate)
                && !vibrating.contains(&(0, index))
            { vibrating.insert(0, (0, index)); }
            let random_ids = if let Some(random) = self.random.as_mut() {
                random.apply(sample, &self.bindings, &self.binding_paths, &self.targets,
                    &vibrating, group, &mut local,
                    visuals.screen_overlay_index.zip(screen_state.as_mut()))?
            } else {
                self.missing_random_inputs += vibrating.len();
                Vec::new()
            };
            let composed = states.entry((group, random_ids.clone())).or_insert_with(|| {
                let parents = if group < 0 {
                    [visuals.screen_layers[2].initial_state,
                        visuals.background_visual_layers[(-group - 1) as usize].song_foreground.initial_state]
                } else if group > 0 {
                    [visuals.song_foreground.initial_state,
                        visuals.foreground_visual_layers[(group - 1) as usize].song_foreground.initial_state]
                } else {
                    [visuals.song_foreground.initial_state, SongLuaOverlayState::default()]
                };
                compose_layer_overlay_states(overlays, &local, screen, clock, parents)
            });
            self.samples += 1;
            // This adapter compares content only: it does not load font assets
            // or retain gameplay's text caches. Report that limit explicitly;
            // glyph colors need the final-frame regression with native samples.
            if let SongLuaOverlayKind::BitmapText { text, .. } = &overlays[index].kind {
                self.text_samples += 1;
                if sample["state"]["text"].as_str() != Some(text.as_ref()) {
                    self.fail(sample, "text", json!(text.as_ref()), mismatches);
                }
                continue;
            }
            // A normal DrawFunction replaces automatic drawing of its owned
            // subtree, even when it makes no submission for this actor.
            if recorded.normal_owned.contains(&(group, index))
                && recorded.draws.get(&(group, index)).is_none_or(VecDeque::is_empty)
            {
                self.fail(sample, "normal_draw_callback_did_not_submit_actor",
                    json!({"group":group,"index":index}), mismatches);
                continue;
            }
            let composer = self
                .composers
                .entry(group)
                .or_insert_with(|| WholeSongComposer::new(overlays));
            let (frame, recorded_texture, recorded_aft) = match recorded.draws.get_mut(&(group, index)).and_then(VecDeque::pop_front) {
                Some(draw) => {
                    if !random_ids.is_empty() {
                        return Err("recorded Draw needs raw random scope binding".into());
                    }
                    (draw.frame, Some(draw.texture), Some(draw.aft_backing))
                },
                None => (composer.render_screen_overlay(overlays, composed, index, screen, clock[0], clock[1],
                    overlay_screen_state(overlays, index, screen_state)), None, None),
            };
            let rows = submitted_rows(&frame);
            let reference = sample["vertices"]
                .as_array()
                .ok_or("draw without vertices")?;
            if rows.is_empty() && fully_clipped(reference) {
                self.culled_samples += 1;
                continue;
            }
            if rows.is_empty() && fully_degenerate(sample["primitive"].as_str().unwrap_or(""), reference) {
                self.degenerate_samples += 1;
                continue;
            }

            let extra_ordinary_draw = recorded_texture.is_none() && !ordinary_draws.insert((group, index));
            let (consumed, expected_count, reference) = native_draw_batch(
                &samples[batch_start..], rows.len(),
                matches!(overlays[index].kind,
                    SongLuaOverlayKind::Sprite { .. } | SongLuaOverlayKind::Quad | SongLuaOverlayKind::AftSprite { .. }),
            )?;
            // Context equality above also requires equal raw-input references;
            // still validate every consumed native sample against owned ancestry.
            for piece in &samples[batch_start + 1..batch_start + consumed] {
                if let Some(random) = self.random.as_mut() {
                    let ids = random.apply(piece, &self.bindings, &self.binding_paths, &self.targets,
                        &vibrating, group, &mut local,
                        visuals.screen_overlay_index.zip(screen_state.as_mut()))?;
                    if ids != random_ids { return Err("native sprite batch changed random inputs".into()); }
                } else {
                    self.missing_random_inputs += vibrating.len();
                }
            }
            sample_index += consumed - 1;
            self.samples += consumed - 1;
            self.batched_samples += consumed - 1;

            let native_aft = self.native_aft_resources.get(actor);
            let actual_aft = native_aft.and_then(|_| {
                recorded_aft.unwrap_or_else(|| owned_aft_backing(&frame, composer, overlays))
            });
            let aft_backing = equal_aft_backing(&sample["state"], native_aft, actual_aft);
            let coordinates = NativeDrawCoordinates::from_state(&sample["state"])
                .with_equal_aft_backing(aft_backing);
            let expected_texture = sample["state"]["texture"].as_str().unwrap_or("");
            let expected_texture = expected_texture.strip_prefix("song:/").map(|p| {
                deadsync_assets::textures::canonical_texture_key(simfile.parent().unwrap().join(p))
            });
            let actual_texture = recorded_texture.as_ref().map(Option::as_deref).unwrap_or_else(|| frame.ops.first().and_then(|op| match op {
                DrawOp::TexturedMesh(r) => Some(composer.texture_key(r.texture_handle)),
                DrawOp::Sprite(r) => Some(composer.texture_key(r.texture_handle)),
                _ => None,
            }));
            let texture_ok = expected_texture
                .as_deref()
                .is_none_or(|key| actual_texture.is_some_and(|actual| same_texture_key(key, actual)));
            let native_depth = sample["state"]["z_test"] != 0;
            let native_write = sample["state"]["z_write"] == true;
            let native_fail = sample["state"]["z_test"] == 2;
            let depth_ok = frame.ops.iter().all(|op| match op {
                DrawOp::TexturedMesh(r) => r.depth_test == native_depth
                    && r.depth_write == native_write && r.depth_test_on_fail == native_fail,
                _ => !native_depth && !native_write,
            });
            let sampler_ok = frame.ops.iter().all(|op| match op {
                DrawOp::TexturedMesh(r) => r.sampler.is_some_and(|s| {
                    let deadlib_present::render::MeshSampler::Override { filter, wrap } = s else {
                        return false;
                    };
                    (filter == deadlib_present::render::SamplerFilter::Linear)
                        == (sample["state"]["texture_filtering"] == true)
                        && (wrap == deadlib_present::render::SamplerWrap::Repeat)
                            == (sample["state"]["texture_wrapping"] == true)
                }),
                _ => true,
            });
            if rows.len() != expected_count || !texture_ok || !depth_ok || !sampler_ok || extra_ordinary_draw {
                let detail = json!({"vertices":rows.len(),"expected_vertices":expected_count,
                    "texture_ok":texture_ok,"expected_texture":expected_texture,"actual_texture":actual_texture,
                    "depth_ok":depth_ok,"sampler_ok":sampler_ok,"native_submissions":consumed,
                    "extra_ordinary_draw":extra_ordinary_draw,"ops":format!("{:?}",frame.ops)});
                self.fail(sample, "draw_shape_resource_or_depth", detail, mismatches);
                continue;
            }
            self.vertices += rows.len();
            if aft_backing { self.aft_backing_uv_samples += consumed; }
            let matches = |actual: &[f32;10], row: &Value| coordinates.row_matches(actual, row);
            let primitive = sample["primitive"].as_str().unwrap_or("");
            let geometry_ok = native_draw_geometry_matches(primitive, &rows, &reference, matches);
            if !geometry_ok {
                let example=rows.iter().enumerate().find(|(vertex, actual)| {
                    let candidates = match primitive {
                        "Quads" => &reference[vertex/6*4..vertex/6*4+4],
                        "QuadStrip" => &reference[vertex/6*2..vertex/6*2+4],
                        _ => &reference[*vertex..*vertex+1],
                    };
                    !candidates.iter().any(|row|matches(actual,row))
                });
                self.fail(sample,"submitted_clip_uv_color_or_topology",json!({"example_vertex":example,
                    "first_actual_triangle":&rows[..rows.len().min(3)],"first_native_quad":&reference[..reference.len().min(4)]}),mismatches);
            }
        }
        for draw in recorded.draws.values().flatten() {
            if draw.immediate { self.uncompared_immediate_draws += 1; }
            else { self.uncompared_normal_draws += 1; }
        }
        Ok(())
    }

    fn fail(&mut self, sample: &Value, field: &str, actual: Value, mismatches: &mut Vec<Value>) {
        self.failures += 1;
        let failure = json!({"field":field,"actor":sample["actor"],
            "frame":sample["update_sequence"],"deadsync":actual});
        *self.failure_counts.entry(field.into()).or_default() += 1;
        if self.failure_examples.len() < 64 {
            self.failure_examples.entry((field.into(), sample["actor"].as_str().unwrap_or("").into()))
                .or_insert_with(|| failure.clone());
        }
        if self.failures <= 100 {
            mismatches.push(failure.clone());
        }
        self.last_failure = Some(failure);
    }

    pub fn report(&self, inputs: usize) -> Value {
        json!({"input_events_replayed":inputs,"draw_samples_compared":self.samples,
            "submitted_vertices_compared":self.vertices,"live_text_samples_compared":self.text_samples,
            "typed_equal_backing_aft_uv_samples_compared":self.aft_backing_uv_samples,
            "live_text_comparison":"content only; submitted glyph geometry, textures and RGBA are not compared",
            "draw_mismatches":self.failures,"equivalent_offscreen_culls":self.culled_samples,
            "batched_native_draw_samples":self.batched_samples,
            "draw_mismatch_counts":self.failure_counts,
            "draw_mismatch_examples":self.failure_examples.values().collect::<Vec<_>>(),
            "equivalent_degenerate_culls":self.degenerate_samples,
            "uncompared_owned_immediate_draws":self.uncompared_immediate_draws,
            "uncompared_owned_normal_draws":self.uncompared_normal_draws,
            "render_random_inputs_replayed":self.random.as_ref().map_or(0, render_random::Replay::used),
            "render_random_inputs_captured":self.random.as_ref().map_or(0, render_random::Replay::total),
            "unreplayed_render_random_inputs":self.random.as_ref().map_or(0, |random| random.total() - random.used()),
            "draw_vibration_scopes_without_native_inputs":self.missing_random_inputs,
            "last_draw_mismatch":self.last_failure,"pixel_parity_proven":false})
    }
}

// Native texture IDs retain the requested spelling, while FilenameDB resolves
// the source without case sensitivity. Compare the resolved source identity;
// parent aliases still follow filesystem traversal at a symlink.
fn same_texture_key(expected: &str, actual: &str) -> bool {
    if expected.split(['/', '\\']).filter(|part| *part != ".")
        .eq(actual.split(['/', '\\']).filter(|part| *part != ".")) {
        return true;
    }
    let expected_source = deadsync_assets::textures::texture_source_key(expected);
    let actual_source = deadsync_assets::textures::texture_source_key(actual);
    if expected[expected_source.len()..] != actual[actual_source.len()..] {
        return false;
    }
    let expected_source = expected_source.replace('\\', "/");
    let actual_source = actual_source.replace('\\', "/");
    let expected_path = Path::new(&expected_source);
    let actual_path = Path::new(&actual_source);
    // Filename hints affect texture preparation even when an alias happens to
    // reach the same file. Retain that spelling and the resource suffix above.
    if !expected_path.file_name().and_then(|name| name.to_str()).zip(
        actual_path.file_name().and_then(|name| name.to_str()))
        .is_some_and(|(expected, actual)| expected.eq_ignore_ascii_case(actual)) {
        return false;
    }
    match (native_texture_source_path(expected_path), native_texture_source_path(actual_path)) {
        (Some(expected), Some(actual)) => expected == actual,
        _ => false,
    }
}

fn native_texture_source_path(path: &Path) -> Option<std::path::PathBuf> {
    if let Ok(path) = path.canonicalize() { return Some(path); }
    let parent = path.parent()?;
    let parent = if parent.as_os_str().is_empty() {
        std::env::current_dir().ok()?
    } else {
        native_texture_source_path(parent)?
    };
    if path.components().next_back() == Some(std::path::Component::ParentDir) {
        return parent.join("..").canonicalize().ok();
    }
    let name = path.file_name()?.to_str()?;
    let mut matches = std::fs::read_dir(parent).ok()?.filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_str().is_some_and(|entry| entry.eq_ignore_ascii_case(name)));
    let source = matches.next()?;
    // An archive with distinct case-colliding entries does not establish which
    // file native FilenameDB selected. Never guess or merge those resources.
    if matches.next().is_some() { return None; }
    source.path().canonicalize().ok()
}

#[test]
fn native_texture_keys_compare_platform_separators_without_merging_resources() {
    let expected = "C:/trace/song/bg/background.png";
    assert!(same_texture_key(expected, r"C:\trace\song\bg\background.png"));
    assert!(same_texture_key(expected, r"C:/trace/song/bg\background.png"));
    assert!(same_texture_key(expected, r"C:\trace\song\bg\./background.png"));
    assert!(same_texture_key(expected, r"C:\trace\.\song\bg\.\background.png"));
    assert!(!same_texture_key(expected, r"C:\trace\song\background.png"));
    assert!(!same_texture_key(expected, r"C:\trace\song\bg\background.png#itg-model"));
    assert!(!same_texture_key(expected, r"C:\trace\other-song\bg\background.png"));
    assert!(!same_texture_key(expected, r"C:\trace\song\bg\..\background.png"));
    assert!(!same_texture_key(expected, r"C:\trace\song\bg\.background.png"));
}

#[test]
fn native_texture_keys_resolve_parent_aliases_without_merging_resources() {
    let song = tempfile::tempdir().unwrap();
    for directory in ["bg", "lua/inner", "other"] {
        std::fs::create_dir_all(song.path().join(directory)).unwrap();
    }
    for file in ["bg/sky#grad.png", "other/sky#grad.png", "bg/other.png"] {
        std::fs::write(song.path().join(file), b"same contents").unwrap();
    }
    let expected = song.path().join("bg/sky#grad.png").to_string_lossy().into_owned();
    let alias = song.path().join("lua/inner/../../bg/sky#grad.png").to_string_lossy().into_owned();
    assert!(same_texture_key(&expected, &alias));
    assert!(same_texture_key(&format!("{expected}#itg-model"), &format!("{alias}#itg-model")));
    assert!(!same_texture_key(&expected, &format!("{alias}#itg-model")));
    assert!(!same_texture_key(&format!("{expected}#itg-model"), &alias));
    for different in ["lua/../other/sky#grad.png", "lua/../bg/other.png", "lua/../missing/sky#grad.png"] {
        assert!(!same_texture_key(&expected, &song.path().join(different).to_string_lossy()));
    }
}

#[cfg(unix)]
#[test]
fn native_texture_keys_reject_distinct_case_collisions() {
    let song = tempfile::tempdir().unwrap();
    std::fs::write(song.path().join("bg.mp4"), b"lowercase source").unwrap();
    std::fs::write(song.path().join("BG.mp4"), b"different uppercase source").unwrap();
    let lower = song.path().join("bg.mp4").to_string_lossy().into_owned();
    assert!(!same_texture_key(&song.path().join("BG.mp4").to_string_lossy(), &lower));
    assert!(!same_texture_key(&song.path().join("Bg.mp4").to_string_lossy(), &lower));
}

#[test]
fn native_texture_keys_resolve_unique_case_aliases_without_merging_hints() {
    let song = tempfile::tempdir().unwrap();
    for directory in ["lua", "other"] {
        std::fs::create_dir(song.path().join(directory)).unwrap();
        std::fs::write(song.path().join(directory).join("bg.mp4"), b"same bytes").unwrap();
    }
    let actual = song.path().join("lua/bg.mp4").to_string_lossy().into_owned();
    let expected = song.path().join("LUA/BG.mp4").to_string_lossy().into_owned();
    assert!(same_texture_key(&expected, &actual));
    assert!(same_texture_key(&format!("{expected}#itg-model"), &format!("{actual}#itg-model")));
    assert!(!same_texture_key(&expected, &format!("{actual}#itg-model")));
    for different in ["other/bg.mp4", "lua/bg (doubleres).mp4", "missing/BG.mp4"] {
        assert!(!same_texture_key(&song.path().join(different).to_string_lossy(), &actual));
    }
    assert!(!same_texture_key(&song.path().join("missing/BG.mp4").to_string_lossy(),
        &song.path().join("missing/bg.mp4").to_string_lossy()));
}

#[cfg(unix)]
#[test]
fn native_texture_parent_aliases_follow_symlinks_without_erasing_filename_hints() {
    use std::os::unix::fs::symlink;

    let song = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(song.path().join("real/nested")).unwrap();
    std::fs::create_dir_all(song.path().join("local")).unwrap();
    for file in ["real/sky.png", "local/sky.png"] {
        std::fs::write(song.path().join(file), b"same contents").unwrap();
    }
    symlink(song.path().join("real/nested"), song.path().join("local/link")).unwrap();
    let alias = song.path().join("local/link/../sky.png").to_string_lossy().into_owned();
    assert!(same_texture_key(&song.path().join("real/sky.png").to_string_lossy(), &alias));
    assert!(!same_texture_key(&song.path().join("local/sky.png").to_string_lossy(), &alias));
    symlink(song.path().join("real/sky.png"), song.path().join("real/sky (doubleres).png")).unwrap();
    let hinted = song.path().join("real/nested/../sky (doubleres).png").to_string_lossy().into_owned();
    assert!(!same_texture_key(&song.path().join("real/sky.png").to_string_lossy(), &hinted));
}

pub(crate) fn native_depth_range(state: &Value) -> [f32; 2] {
    if state["z_test"] == 0 && state["z_write"] != true { return [1.0, 0.0]; }
    // The native hook records projection coordinates BEFORE glDepthRange.
    // DeadSync represents that range in its submitted homogeneous matrix.
    // Derive the common comparison space from RageDisplay_Legacy::SetZBias,
    // including Actor::SetGlobalRenderStates' implicit NoEffect mask bias.
    let mut bias = state["z_bias"].as_f64().unwrap_or(0.0) as f32;
    if bias == 0.0 && state["blend"] == 10 { bias = 1.0; }
    let near = 0.05 * (1.0 - bias);
    let far = 1.0 - 0.05 * bias;
    [far - near, far + near - 1.0]
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AftBacking {
    image: [u32; 2],
    texture: [u32; 2],
}

impl AftBacking {
    fn native(state: &Value) -> Option<Self> {
        let dimension = |key: &str| {
            u32::try_from(state[key].as_u64()?).ok().filter(|&value| value > 0)
        };
        let image = [dimension("image_width")?, dimension("image_height")?];
        let texture = [dimension("texture_width")?, dimension("texture_height")?];
        image.iter().zip(texture).all(|(&image, texture)| image <= texture)
            .then_some(Self { image, texture })
    }
}

#[derive(Debug)]
struct NativeAftResource {
    texture: String,
    backing: AftBacking,
}

// The capture has no per-draw texture-object identity. Restrict this basis
// correction to one stable, explicitly typed AMV binding in the whole scene.
// Rebinding, loading, unmatched calls, or missing sample anchors cannot prove
// which resource was sampled. None of those cases is inferred from its name.
fn native_aft_resources(trace: &Value, scene: u64) -> BTreeMap<String, NativeAftResource> {
    let mut out = BTreeMap::new();
    if trace["dropped_events"] != 0 { return out; }
    let Some(events) = trace["events"].as_array() else { return out; };
    let Some(frames) = trace["update_frames"].as_array() else { return out; };
    let Some(samples) = trace["drawable_samples"].as_array() else { return out; };
    let mut previous = 0;
    for event in events {
        let Some(sequence) = event["seq"].as_u64() else { return out; };
        if sequence <= previous { return out; }
        previous = sequence;
    }
    let mut changes: BTreeMap<&str, Vec<&Value>> = BTreeMap::new();
    for event in events.iter().filter(|event| event["scene"].as_u64() == Some(scene)) {
        if matches!(event["operation"].as_str(),
            Some("ActorMultiVertex.SetTexture" | "ActorMultiVertex.LoadTexture"))
            && let Some(actor) = event["actor"].as_str()
        { changes.entry(actor).or_default().push(event); }
    }
    for (actor, changes) in changes {
        let [event] = changes.as_slice() else { continue; };
        if event["kind"] != "call" || event["operation"] != "ActorMultiVertex.SetTexture"
            || event["args"].as_array().map(Vec::len) != Some(1)
            || event["args"][0]["type"] != "RageTextureRenderTarget"
        { continue; }
        if !event["seconds"].as_f64().is_some_and(|time| time.is_finite()) { continue; }
        let draws = samples.iter().filter(|sample| sample["scene"].as_u64() == Some(scene)
            && sample["actor"].as_str() == Some(actor)).collect::<Vec<_>>();
        let calls = events.iter().filter(|call| call["scene"].as_u64() == Some(scene)
            && call["actor"].as_str() == Some(actor) && call["operation"] == "Actor.Draw")
            .collect::<Vec<_>>();
        // Event sequence orders binding versus Draw; music time may legitimately
        // move backward. Only the explicit manual one-call/one-sample subset is
        // proved here. Preserve repeated same-clock calls and their multiplicity.
        if calls.len() != draws.len() { continue; }
        let Some(first) = draws.first() else { continue; };
        let Some(texture) = first["state"]["texture"].as_str().filter(|value| !value.is_empty())
            else { continue; };
        let Some(backing) = AftBacking::native(&first["state"]) else { continue; };
        if !draws.iter().zip(calls).all(|(sample, call)| {
            let frame = sample["update_sequence"].as_u64()
                .and_then(|sequence| sequence.checked_sub(1))
                .and_then(|index| frames.get(index as usize));
            call["kind"] == "call" && call["callback"] == "SetDrawFunction"
                && call["seq"].as_u64().unwrap() > event["seq"].as_u64().unwrap()
                && frame.is_some_and(|frame| frame["scene"].as_u64() == Some(scene)
                    && frame["sequence"] == sample["update_sequence"]
                    && frame["seconds"] == call["seconds"]
                    && frame["seconds"].as_f64().is_some_and(|time| time.is_finite()))
                && sample["state"]["texture"].as_str() == Some(texture)
                && AftBacking::native(&sample["state"]) == Some(backing)
        }) { continue; }
        out.insert(actor.to_owned(), NativeAftResource { texture: texture.to_owned(), backing });
    }
    out
}

fn owned_aft_backing(
    frame: &RenderFrame, composer: &WholeSongComposer, overlays: &[SongLuaOverlayActor],
) -> Option<AftBacking> {
    let mut sampled = None;
    for op in &frame.ops {
        let handle = match op {
            DrawOp::TexturedMesh(run) => run.texture_handle,
            DrawOp::Sprite(run) => run.texture_handle,
            _ => return None,
        };
        if !deadlib_present::render::is_render_target_texture(handle) { return None; }
        let handle = deadlib_present::render::render_target_base_handle(handle);
        if sampled.is_some_and(|previous| previous != handle) { return None; }
        sampled = Some(handle);
    }
    let sampled = sampled?;
    let mut allocation = None;
    for (index, actor) in overlays.iter().enumerate() {
        if composer.capture_handle(index) != Some(sampled) { continue; }
        let SongLuaOverlayKind::ActorFrameTexture { capture_size: Some(size), .. } = actor.kind
            else { return None; };
        if allocation.is_some() || size.iter().any(|value| !value.is_finite()
            || *value < 1.0 || *value >= u32::MAX as f32 || value.fract() != 0.0)
        { return None; }
        let image = size.map(|value| value as u32);
        // The frozen allocation uses the same padded backing for both manual
        // and automatic captures (song_lua_capture_size), not an image upload.
        let texture = [image[0].checked_next_power_of_two()?, image[1].checked_next_power_of_two()?];
        allocation = Some(AftBacking { image, texture });
    }
    allocation
}

fn equal_aft_backing(
    state: &Value, native: Option<&NativeAftResource>, owned: Option<AftBacking>,
) -> bool {
    native.is_some_and(|native| state["texture"].as_str() == Some(native.texture.as_str())
        && AftBacking::native(state) == Some(native.backing) && owned == Some(native.backing))
}

struct NativeDrawCoordinates {
    image_uv_scale: [f32; 2],
    untextured: bool,
    depth_range: [f32; 2],
}

impl NativeDrawCoordinates {
    fn from_state(state: &Value) -> Self {
        let image_uv_scale = std::array::from_fn(|axis| {
            let (image, backing) = if axis == 0 {
                ("image_width", "texture_width")
            } else {
                ("image_height", "texture_height")
            };
            let image = state[image].as_f64().unwrap_or(1.0);
            state[backing].as_f64().unwrap_or(image) as f32 / image as f32
        });
        Self {
            image_uv_scale,
            // The native hook adds a string whenever a Sprite/AMV has a
            // texture resource, including generated AFTs and an empty ID.
            // Resolving that ID to a song file is a separate resource check.
            untextured: !state["texture"].is_string(),
            depth_range: native_depth_range(state),
        }
    }

    fn with_equal_aft_backing(mut self, proved: bool) -> Self {
        // Render targets retain padded backing coordinates in both renderers.
        // Keep every UV comparison; only remove the image-upload conversion.
        if proved { self.image_uv_scale = [1.0; 2]; }
        self
    }

    fn row_matches(&self, actual: &[f32; 10], row: &Value) -> bool {
        row_matches(actual, row, self.image_uv_scale, self.untextured, self.depth_range)
    }
}

pub(crate) fn row_matches(actual: &[f32;10], row: &Value, image_uv_scale: [f32;2], untextured: bool, depth_range: [f32;2]) -> bool {
    actual.iter().enumerate().all(|(axis,&actual)| {
        if untextured && (4..6).contains(&axis) { return true; }
        let mut expected = native_number(&row[axis+3]);
        if axis == 2 { expected = expected * depth_range[0] + native_number(&row[6]) * depth_range[1]; }
        if (4..6).contains(&axis) { expected *= image_uv_scale[axis-4]; }
        close(actual, expected)
    })
}

#[test]
fn native_draw_coordinates_compare_uvs_for_every_bound_resource() {
    // An unresolved resource name, even one resembling an AFT name, is
    // not type proof. Preserve ordinary image normalization without proof.
    let row = json!([0, 0, 0, -0.5, 0.25, 0.2, 1, 0.4169921875, 0.46875, 1, 0.5, 0.25, 0.75]);
    let actual = [-0.5, 0.25, 0.2, 1.0, 0.5, 0.5, 1.0, 0.5, 0.25, 0.75];
    for texture in ["ActorFrameTexture 1", "theme:/Graphics/pixel.png", "native:/pixel.png", "song:/sky.png", ""] {
        let state = json!({"texture":texture, "image_width":854, "texture_width":1024,
            "image_height":480, "texture_height":512, "z_test":0, "z_write":false});
        let coordinates = NativeDrawCoordinates::from_state(&state);
        assert!(coordinates.row_matches(&actual, &row), "normalized UVs for {texture:?}");
        for axis in [4, 5] {
            let mut changed = actual;
            changed[axis] += 0.03125;
            assert!(!coordinates.row_matches(&changed, &row),
                "bound resource {texture:?} must compare UV axis {axis}");
        }
    }
}


#[test]
fn native_aft_uv_proof_requires_one_typed_binding_before_every_draw() {
    let state = json!({"texture":"chosen resource", "image_width":854, "image_height":480,
        "texture_width":1024, "texture_height":512});
    let trace = json!({"dropped_events":0,
        "events":[{"seq":1,"scene":1,"actor":"mesh","kind":"call","seconds":1,
            "operation":"ActorMultiVertex.SetTexture", "args":[{"type":"RageTextureRenderTarget"}]},
            {"seq":2,"scene":1,"actor":"mesh","kind":"call","seconds":2,
                "callback":"SetDrawFunction","operation":"Actor.Draw"}],
        "update_frames":[{"scene":1,"sequence":1,"seconds":2}],
        "drawable_samples":[{"scene":1,"actor":"mesh","update_sequence":1,"state":state}]});
    let proof = native_aft_resources(&trace, 1);
    assert_eq!(proof["mesh"].texture, "chosen resource", "no texture-name convention is used");
    assert!(native_aft_resources(&trace, 2).is_empty(), "scene identity is required");
    for time in [0, 2, 3] {
        let mut changed = trace.clone();
        changed["events"][0]["seconds"] = json!(time);
        assert!(native_aft_resources(&changed, 1).contains_key("mesh"),
            "an earlier binding event remains active at equal or rewound music time");
    }
    for time in [Value::Null, json!("1")] {
        let mut changed = trace.clone();
        changed["events"][0]["seconds"] = time.clone();
        assert!(native_aft_resources(&changed, 1).is_empty(), "the binding needs a finite clock");
        let mut changed = trace.clone();
        changed["events"][1]["seconds"] = time.clone();
        changed["update_frames"][0]["seconds"] = time;
        assert!(native_aft_resources(&changed, 1).is_empty(), "a matching invalid draw clock is not proof");
    }
    for arg in [Value::Null, json!({"type":"nil"}), json!({"type":"RageTexture"}),
        json!({"type":"unknown"}), json!("ActorFrameTexture 1")] {
        let mut changed = trace.clone();
        changed["events"][0]["args"][0] = arg;
        assert!(native_aft_resources(&changed, 1).is_empty());
    }
    for operation in ["ActorMultiVertex.SetTexture", "ActorMultiVertex.LoadTexture"] {
        let mut changed = trace.clone();
        let mut later = changed["events"][0].clone();
        later["seq"] = json!(3);
        later["seconds"] = json!(3);
        later["operation"] = json!(operation);
        changed["events"].as_array_mut().unwrap().push(later);
        assert!(native_aft_resources(&changed, 1).is_empty(), "a changed binding stays unproved");
    }
    for changed_state in [json!({}), json!({"texture":null}), json!({"texture":""})] {
        let mut changed = trace.clone(); changed["drawable_samples"][0]["state"] = changed_state;
        assert!(native_aft_resources(&changed, 1).is_empty());
    }
    let mut changed = trace.clone();
    let mut second = changed["drawable_samples"][0].clone(); second["state"]["texture"] = json!("other");
    changed["drawable_samples"].as_array_mut().unwrap().push(second);
    let mut matching_call = changed["events"][1].clone(); matching_call["seq"] = json!(3);
    changed["events"].as_array_mut().unwrap().push(matching_call);
    assert!(native_aft_resources(&changed, 1).is_empty(), "one actor cannot prove two resource IDs");
    let mut changed = trace.clone(); changed["dropped_events"] = json!(1);
    assert!(native_aft_resources(&changed, 1).is_empty());
    let mut changed = trace.clone(); changed["events"][1]["seq"] = json!(1);
    assert!(native_aft_resources(&changed, 1).is_empty(), "event order must be complete and strict");
    for dimension in [json!(0), json!(-1), json!(480.5), json!(1025)] {
        let mut changed = trace.clone();
        changed["drawable_samples"][0]["state"]["image_height"] = dimension;
        assert!(native_aft_resources(&changed, 1).is_empty(), "invalid allocation cannot prove a UV basis");
    }
    let mut changed = trace.clone();
    changed["events"].as_array_mut().unwrap().swap(0, 1);
    changed["events"][0]["seq"] = json!(1); changed["events"][1]["seq"] = json!(2);
    assert!(native_aft_resources(&changed, 1).is_empty(), "later binding cannot bless earlier Draw despite its earlier music time");
    let mut changed = trace.clone(); changed["events"][1]["seconds"] = json!(2.1);
    assert!(native_aft_resources(&changed, 1).is_empty(), "the exact sample clock must match");
    let mut changed = trace.clone(); changed["events"][1]["callback"] = json!("SetUpdateFunction");
    assert!(native_aft_resources(&changed, 1).is_empty(), "unproved callback scope stays unchanged");
    let mut changed = trace.clone(); changed["events"].as_array_mut().unwrap().pop();
    assert!(native_aft_resources(&changed, 1).is_empty(), "automatic or missing calls are not inferred");
    let mut changed = trace.clone();
    let mut extra = changed["events"][1].clone(); extra["seq"] = json!(3);
    changed["events"].as_array_mut().unwrap().push(extra.clone());
    assert!(native_aft_resources(&changed, 1).is_empty(), "extra calls cannot be silently deduplicated");
    changed["drawable_samples"].as_array_mut().unwrap().push(trace["drawable_samples"][0].clone());
    assert!(native_aft_resources(&changed, 1).contains_key("mesh"), "same-clock multiplicity is retained exactly");
    // Captured music seconds may regress even while event and native update
    // sequences advance; a stable binding remains typed through that clock drift.
    extra["seconds"] = json!(0.5); changed["events"][2] = extra;
    changed["update_frames"].as_array_mut().unwrap().push(json!({"scene":1,"sequence":2,"seconds":0.5}));
    changed["drawable_samples"][1]["update_sequence"] = json!(2);
    assert!(native_aft_resources(&changed, 1).contains_key("mesh"), "clock rewind before binding time preserves event causality");
}

#[test]
fn owned_aft_uv_proof_requires_the_composers_actual_capture_handle() {
    use deadlib_present::render::{BlendMode, SpriteRun, render_target_sample_handle};
    let capture = |name: &str| SongLuaOverlayActor {
        kind: SongLuaOverlayKind::ActorFrameTexture {
            capture_name: name.into(), capture_size: Some([854.0, 480.0]),
            alpha_buffer: true, depth_buffer: false, float_buffer: false,
        },
        name: None, parent_index: None,
        initial_state: SongLuaOverlayState { aft_created: true, ..Default::default() },
        message_commands: Vec::new(),
    };
    let overlays = vec![capture("first"), capture("second")];
    let composer = WholeSongComposer::new(&overlays);
    let handle = composer.capture_handle(0).unwrap();
    let sprite = |texture_handle| DrawOp::Sprite(SpriteRun {
        texture_handle, instance_start: 0, instance_count: 1,
        blend: BlendMode::Alpha, camera: 0,
    });
    let mut frame = RenderFrame {
        clear_color: [0.0; 4], render_targets: Vec::new(), cameras: Vec::new(),
        sprite_instances: Vec::new(), mesh_vertices: Vec::new(),
        tmesh_instances: Vec::new(), tmesh_geometries: Vec::new(), ops: vec![sprite(handle)],
    };
    let expected = Some(AftBacking { image: [854, 480], texture: [1024, 512] });
    assert_eq!(owned_aft_backing(&frame, &composer, &overlays), expected);
    frame.ops.push(sprite(render_target_sample_handle(handle, true)));
    assert_eq!(owned_aft_backing(&frame, &composer, &overlays), expected,
        "sampler flags do not change the allocated target");
    frame.ops.push(sprite(composer.capture_handle(1).unwrap()));
    assert_eq!(owned_aft_backing(&frame, &composer, &overlays), None,
        "a mixed-resource submission has no single coordinate basis");
    frame.ops = vec![sprite(1)];
    assert_eq!(owned_aft_backing(&frame, &composer, &overlays), None,
        "an ordinary image handle is not a render-target proof");
    frame.ops = vec![sprite(handle)];
    let other_composer = WholeSongComposer::new(&overlays);
    assert_eq!(owned_aft_backing(&frame, &other_composer, &overlays), None,
        "a recorded draw must retain its own composer allocation");
    let mut unallocated = overlays.clone();
    if let SongLuaOverlayKind::ActorFrameTexture { capture_size, .. } = &mut unallocated[0].kind {
        *capture_size = None;
    }
    assert_eq!(owned_aft_backing(&frame, &composer, &unallocated), None);
    frame.ops.clear();
    assert_eq!(owned_aft_backing(&frame, &composer, &overlays), None);
}

#[test]
fn native_aft_uvs_keep_equal_backing_coordinates_and_reject_wrong_axes() {
    let state = json!({"texture":"chosen resource", "image_width":854, "texture_width":1024,
        "image_height":480, "texture_height":512, "z_test":0, "z_write":false});
    let backing = AftBacking::native(&state).unwrap();
    let native = NativeAftResource { texture: "chosen resource".into(), backing };
    let raw = [-0.9755428, -1.0022018, 0.0, 1.0, 0.0, 0.9375, 1.0, 0.0, 0.0, 1.0];
    let row = json!([0,480,0,-0.9755427837371826,-1.002201795578003,0,1,0,0.9375,1,0,0,1]);
    let proved = equal_aft_backing(&state, Some(&native), Some(backing));
    assert!(proved);
    let coordinates = NativeDrawCoordinates::from_state(&state).with_equal_aft_backing(proved);
    assert!(coordinates.row_matches(&raw, &row));
    for axis in 0..10 {
        let mut wrong = raw; wrong[axis] += 0.03125;
        assert!(!coordinates.row_matches(&wrong, &row), "typed AFT still compares axis {axis}");
    }
    for (proof, allocation) in [(None, Some(backing)), (Some(&native), None),
        (Some(&native), Some(AftBacking { image: [800,480], ..backing })),
        (Some(&native), Some(AftBacking { texture: [2048,512], ..backing }))] {
        assert!(!equal_aft_backing(&state, proof, allocation));
        assert!(!NativeDrawCoordinates::from_state(&state).row_matches(&raw, &row),
            "unproved targets retain ordinary image normalization");
    }
    let mut renamed = state.clone(); renamed["texture"] = json!("another target");
    assert!(!equal_aft_backing(&renamed, Some(&native), Some(backing)));
}

#[test]
fn native_draw_coordinates_without_resource_ignore_only_uv_axes() {
    // A missing texture field is how the native hook records no resource.
    let row = json!([0, 0, 0, -0.5, 0.25, 0.2, 1, 0, 0, 1, 0.5, 0.25, 0.75]);
    let actual = [-0.5, 0.25, 0.2, 1.0, 5.0, -3.0, 1.0, 0.5, 0.25, 0.75];
    let coordinates = NativeDrawCoordinates::from_state(&json!({"z_test":0, "z_write":false}));
    assert!(coordinates.row_matches(&actual, &row));
    for axis in [0, 1, 2, 3, 6, 7, 8, 9] {
        let mut changed = actual;
        changed[axis] += 0.03125;
        assert!(!coordinates.row_matches(&changed, &row),
            "untextured draw must still compare position/color axis {axis}");
    }
}

#[test]
fn native_depth_comparison_keeps_bias_and_homogeneous_w() {
    let row = json!([0,0,0, 1,2,0.4,2, 0.2,0.4, 1,1,1,1]);
    let mut actual = [1.0,2.0,0.48,2.0,0.2,0.4,1.0,1.0,1.0,1.0];
    let mut state = json!({"z_test":1,"z_write":true,"z_bias":0,"blend":0});
    assert!(row_matches(&actual, &row, [1.0;2], false, native_depth_range(&state)));
    actual[2] = 0.4;
    assert!(!row_matches(&actual, &row, [1.0;2], false, native_depth_range(&state)),
        "comparing the unadjusted clip depth must still fail");
    state["z_bias"] = json!(1);
    actual[2] = 0.28;
    assert!(row_matches(&actual, &row, [1.0;2], false, native_depth_range(&state)));
    state["z_bias"] = json!(0);
    state["blend"] = json!(10);
    assert!(row_matches(&actual, &row, [1.0;2], false, native_depth_range(&state)));
    state["z_test"] = json!(0);
    state["z_write"] = json!(false);
    actual[2] = 0.4;
    assert!(row_matches(&actual, &row, [1.0;2], false, native_depth_range(&state)));
}

// Reject a missing production draw only when native geometry can contribute
// pixels. All corners outside one homogeneous clip plane prove a safe cull.
pub(crate) fn fully_clipped(rows: &[Value]) -> bool {
    if rows.is_empty() || rows.iter().any(|r| (3..7).any(|i| !native_number(&r[i]).is_finite())) { return false; }
    (0..3).any(|axis| rows.iter().all(|r| native_number(&r[axis+3]) > native_number(&r[6])+0.0001)
        || rows.iter().all(|r| native_number(&r[axis+3]) < -native_number(&r[6])-0.0001))
}

// Native Sprite::DrawTexture submits zero-scale quads; production composition
// can omit them because they cover no pixels. Prove a conservative subset of
// exact projected degeneracy, without an area tolerance or perspective divide.
// Products of two finite f32 values are exact in f64, so these comparisons cannot
// round a merely thin triangle down to zero area. Other diagonal collinearity
// remains a mismatch rather than relying on a rounded determinant.
pub(crate) fn fully_degenerate(primitive: &str, rows: &[Value]) -> bool {
    let (count, stride) = match primitive {
        "Triangles" => (3, 3),
        "Quads" => (4, 4),
        "QuadStrip" => (4, 2),
        _ => return false,
    };
    if rows.len() < count || (rows.len() - count) % stride != 0 { return false; }
    rows.windows(count).step_by(stride).all(|group| {
        let mut points = [[0.0f32; 4]; 4];
        for (point, row) in points.iter_mut().zip(group) {
            let Some(values) = row.as_array().filter(|values| values.len() == 13) else { return false; };
            *point = std::array::from_fn(|axis| native_number(&values[axis + 3]));
            if point.iter().any(|value| !value.is_finite()) || point[3] <= 0.0 { return false; }
        }
        let same_axis = |a: [f32; 4], b: [f32; 4], axis: usize| {
            f64::from(a[axis]) * f64::from(b[3]) == f64::from(b[axis]) * f64::from(a[3])
        };
        let same_point = |a, b| same_axis(a, b, 0) && same_axis(a, b, 1);
        let zero_triangle = |indices: [usize; 3]| {
            let [a, b, c] = indices.map(|index| points[index]);
            (0..2).any(|axis| same_axis(a, b, axis) && same_axis(a, c, axis))
                || same_point(a, b) || same_point(a, c) || same_point(b, c)
        };
        zero_triangle([0, 1, 2]) && (count == 3
            // Either native quad diagonal must cover zero area. A repeated
            // corner alone must not conceal the remaining visible triangle.
            || [[0, 2, 3], [0, 1, 3], [1, 2, 3]].into_iter().all(zero_triangle))
    })
}

#[test]
fn native_degenerate_culls_require_exact_projected_collapse() {
    let rows = |points: &[[f32; 4]]| points.iter().map(|p|
        json!([0, 0, 0, p[0], p[1], p[2], p[3], 0, 0, 1, 1, 1, 1])
    ).collect::<Vec<_>>();
    let collapsed = rows(&[[-f32::EPSILON / 2.0, 0.0, 0.0, 1.0]; 4]);
    assert!(fully_degenerate("Quads", &collapsed));
    assert!(fully_degenerate("Triangles", &rows(&[
        [0.5, -1.0, 0.0, 1.0], [1.0, 2.0, 0.0, 2.0], [2.0, 2.0, 0.0, 4.0],
    ])), "equal projected X is sufficient even when homogeneous W differs");
    assert!(fully_degenerate("Quads", &rows(&[
        [-1.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0],
        [2.0, 0.0, 0.0, 1.0], [-2.0, 0.0, 0.0, 1.0],
    ])));
    let a = [-1.0, -1.0, 0.0, 1.0];
    let b = [1.0, 1.0, 0.0, 1.0];
    assert!(fully_degenerate("Quads", &rows(&[a, a, b, b])));
    assert!(!fully_degenerate("Quads", &rows(&[a, a, b, [-1.0, 1.0, 0.0, 1.0]])),
        "a duplicate corner does not erase the remaining triangle");
    let thin = rows(&[
        [0.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0],
        [1.0e-30, 1.0, 0.0, 1.0], [1.0e-30, 0.0, 0.0, 1.0],
    ]);
    assert!(!fully_degenerate("Quads", &thin), "nonzero area has no acceptance epsilon");
    assert!(!fully_degenerate("Triangles", &thin[..3]));
    assert!(!fully_degenerate("Quads", &rows(&[
        [-1.0, -1.0, 0.0, 1.0], [1.0, 1.0, 0.0, 1.0],
        [-1.0, 1.0, 0.0, 1.0], [1.0, -1.0, 0.0, 1.0],
    ])), "opposite signed triangle areas cannot cancel visible geometry");
    let mut mixed = collapsed;
    mixed.extend(thin);
    assert!(!fully_degenerate("Quads", &mixed), "every primitive must collapse");
}

#[test]
fn native_degenerate_culls_reject_invalid_geometry_and_transparency() {
    let row = json!([0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 1, 1]);
    let mut rows = vec![row; 4];
    assert!(!fully_degenerate("Lines", &rows));
    assert!(!fully_degenerate("Quads", &[]));
    assert!(!fully_degenerate("Quads", &rows[..3]));
    for w in [0.0, -1.0] {
        rows[0][6] = json!(w);
        assert!(!fully_degenerate("Quads", &rows));
    }
    rows[0][6] = json!(1.0);
    for axis in 3..7 {
        let before = rows[0][axis].clone();
        rows[0][axis] = json!({"type":"number", "value":"inf"});
        assert!(!fully_degenerate("Quads", &rows));
        rows[0][axis] = before;
    }
    rows[0] = json!([0, 0, 0, 0, 0, 0, 1]);
    assert!(!fully_degenerate("Quads", &rows));
    let transparent = [[-1, -1], [-1, 1], [1, 1], [1, -1]].map(|p|
        json!([0, 0, 0, p[0], p[1], 0, 1, 0, 0, 1, 1, 1, 0]));
    assert!(!fully_degenerate("Quads", &transparent), "alpha zero does not prove a safe depth or blend cull");
}

#[test]
fn native_quad_strip_degeneracy_requires_every_complete_cell_to_collapse() {
    let mut rows = (0..6).map(|i|
        json!([0,0,0, i,0,0,1, 0,0, 1,1,1,1])
    ).collect::<Vec<_>>();
    assert!(fully_degenerate("QuadStrip", &rows));
    assert!(!fully_degenerate("QuadStrip", &rows[..2]));
    assert!(!fully_degenerate("QuadStrip", &rows[..5]));
    rows[5][4] = json!(1);
    assert!(!fully_degenerate("QuadStrip", &rows), "one visible cell keeps the native draw observable");
    rows[5][4] = json!(0);
    rows[5][6] = json!(0);
    assert!(!fully_degenerate("QuadStrip", &rows), "invalid homogeneous input cannot establish a safe cull");
}

fn native_number(v: &Value) -> f32 {
    v.as_f64()
        .map(|x| x as f32)
        .unwrap_or_else(|| match v["value"].as_str() {
            Some("inf") => f32::INFINITY,
            Some("-inf") => f32::NEG_INFINITY,
            _ => f32::NAN,
        })
}
fn close(a: f32, b: f32) -> bool {
    a == b
        || a.is_nan() && b.is_nan()
        || a.is_finite() && b.is_finite() && (a - b).abs() <= 0.002 + b.abs() * 0.00002
}

pub(crate) fn submitted_rows(frame: &RenderFrame) -> Vec<[f32; 10]> {
    let mut rows = Vec::new();
    for op in &frame.ops {
        match op {
            DrawOp::TexturedMesh(run) => {
                for instance in &frame.tmesh_instances[run.instance_start as usize..]
                    [..run.instance_count as usize]
                {
                    let matrix = frame.cameras[run.camera as usize] * instance.transform();
                    for vertex in frame.tmesh_geometries[run.geometry as usize]
                        .vertices
                        .iter()
                    {
                        let clip = project_world(
                            matrix_rows(matrix),
                            [vertex.pos[0], vertex.pos[1], vertex.pos[2], 1.0],
                        );
                        rows.push([
                            clip[0],
                            clip[1],
                            clip[2],
                            clip[3],
                            vertex.uv[0] * instance.uv_scale[0]
                                + instance.uv_offset[0]
                                + instance.uv_tex_shift[0] * (vertex.tex_matrix_scale[0] - 1.0),
                            vertex.uv[1] * instance.uv_scale[1]
                                + instance.uv_offset[1]
                                + instance.uv_tex_shift[1] * (vertex.tex_matrix_scale[1] - 1.0),
                            vertex.color[0] * instance.tint[0],
                            vertex.color[1] * instance.tint[1],
                            vertex.color[2] * instance.tint[2],
                            vertex.color[3] * instance.tint[3],
                        ]);
                    }
                }
            }
            DrawOp::Sprite(run) => {
                for s in &frame.sprite_instances[run.instance_start as usize..]
                    [..run.instance_count as usize]
                {
                    for [x, y] in [[0., 0.], [1., 0.], [1., 1.], [1., 1.], [0., 1.], [0., 0.]] {
                        let (px, py) = ((x - 0.5) * s.size[0], (0.5 - y) * s.size[1]);
                        let [sin, cos] = s.rot_sin_cos;
                        let [os, oc] = s.local_offset_rot_sin_cos;
                        let p = [
                            s.center[0] + cos * px - sin * py + oc * s.local_offset[0]
                                - os * s.local_offset[1],
                            s.center[1]
                                + sin * px
                                + cos * py
                                + os * s.local_offset[0]
                                + oc * s.local_offset[1],
                            s.center[2],
                            1.0,
                        ];
                        let c = project_world(matrix_rows(frame.cameras[run.camera as usize]), p);
                        rows.push([
                            c[0],
                            c[1],
                            c[2],
                            c[3],
                            x * s.uv_scale[0] + s.uv_offset[0],
                            y * s.uv_scale[1] + s.uv_offset[1],
                            s.tint[0],
                            s.tint[1],
                            s.tint[2],
                            s.tint[3],
                        ]);
                    }
                }
            }
            DrawOp::Mesh(run) => {
                for v in
                    &frame.mesh_vertices[run.vertex_start as usize..][..run.vertex_count as usize]
                {
                    let c = project_world(
                        matrix_rows(frame.cameras[run.camera as usize]),
                        [v.pos[0], v.pos[1], 0.0, 1.0],
                    );
                    rows.push([
                        c[0], c[1], c[2], c[3], 0.0, 0.0, v.color[0], v.color[1], v.color[2],
                        v.color[3],
                    ]);
                }
            }
        }
    }
    rows
}
