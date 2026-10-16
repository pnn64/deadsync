//! Independent input and final draw-stream checks for executable captures.
use super::{gameplay, native_creation_order_with_coverage_for_entries};
use deadlib_present::render::{DrawOp, RenderFrame};
use deadsync_assets::song_lua::{SongLuaOverlayActor, SongLuaOverlayKind};
use deadsync_song_lua::playback::actor_conformance::{
    WholeSongComposer, compose_overlay_states, matrix_rows, project_world,
};
use deadsync_song_lua::{SongLuaInputEvent, SongLuaInputType, SongLuaLiveTarget};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

pub(super) fn validate(trace: &Value) -> Result<(), String> {
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

pub(super) fn dispatch(state: &mut gameplay::State, event: &Value) -> Result<(), String> {
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
    gameplay::handle_song_lua_input(state, &input);
    Ok(())
}

#[derive(Default)]
pub(super) struct DrawChecks {
    bindings: BTreeMap<String, SongLuaLiveTarget>,
    composers: BTreeMap<i32, WholeSongComposer>,
    pub samples: usize,
    pub vertices: usize,
    pub text_samples: usize,
    pub failures: usize,
    pub culled_samples: usize,
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

impl DrawChecks {
    pub fn bind(
        &mut self,
        state: &gameplay::State,
        trace: &Value,
        scene: u64,
    ) -> Result<(), String> {
        let owned = state.song_lua.as_ref().ok_or("missing live scene")?;
        let (order, _) = native_creation_order_with_coverage_for_entries(
            trace,
            scene,
            owned.context().background_layer_count,
        )?;
        let paths: BTreeMap<_, _> = order
            .into_iter()
            .map(|(path, pointer)| (pointer, path))
            .collect();
        let targets: BTreeMap<_, _> = owned
            .visual_bindings()
            .map_err(|e| e.to_string())?
            .into_iter()
            .collect();
        self.bindings.clear();
        self.composers.clear();
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
        let mut states = BTreeMap::new();
        for sample in samples {
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
            let composed = states.entry(group).or_insert_with(|| {
                compose_overlay_states(
                    overlays,
                    &overlays.iter().map(|a| a.initial_state).collect::<Vec<_>>(),
                    screen,
                    clock,
                )
            });
            self.samples += 1;
            // Glyph geometry has a separate font oracle. Here compare the live
            // text value, which was previously frozen at scene preparation.
            if let SongLuaOverlayKind::BitmapText { text, .. } = &overlays[index].kind {
                self.text_samples += 1;
                if sample["state"]["text"].as_str() != Some(text.as_ref()) {
                    self.fail(sample, "text", json!(text.as_ref()), mismatches);
                }
                continue;
            }
            let composer = self
                .composers
                .entry(group)
                .or_insert_with(|| WholeSongComposer::new(overlays));
            let frame = composer
                .render_screen_overlay(overlays, composed, index, screen, clock[0], clock[1], None);
            let rows = submitted_rows(&frame);
            let reference = sample["vertices"]
                .as_array()
                .ok_or("draw without vertices")?;
            if rows.is_empty() && fully_clipped(reference) {
                self.culled_samples += 1;
                continue;
            }

            let image_uv_scale: [f32; 2] = std::array::from_fn(|axis| {
                let (image, backing) = if axis == 0 {
                    ("image_width", "texture_width")
                } else {
                    ("image_height", "texture_height")
                };
                let image = sample["state"][image].as_f64().unwrap_or(1.0);
                sample["state"][backing].as_f64().unwrap_or(image) as f32 / image as f32
            });
            let expected_count = match sample["primitive"].as_str() {
                Some("Quads") if reference.len() % 4 == 0 => reference.len() / 4 * 6,
                Some("Triangles") => reference.len(),
                _ => return Err("unsupported native primitive in draw comparison".into()),
            };
            let expected_texture = sample["state"]["texture"].as_str().unwrap_or("");
            let expected_texture = expected_texture.strip_prefix("song:/").map(|p| {
                deadsync_assets::textures::canonical_texture_key(simfile.parent().unwrap().join(p))
            });
            let actual_texture = frame.ops.first().and_then(|op| match op {
                DrawOp::TexturedMesh(r) => Some(composer.texture_key(r.texture_handle)),
                DrawOp::Sprite(r) => Some(composer.texture_key(r.texture_handle)),
                _ => None,
            });
            let texture_ok = expected_texture
                .as_deref()
                .is_none_or(|key| actual_texture == Some(key));
            let native_depth = sample["state"]["z_test"] == 1;
            let depth_ok = frame.ops.iter().all(|op| match op {
                DrawOp::TexturedMesh(r) => r.depth_test == native_depth,
                _ => !native_depth,
            }) && sample["state"]["z_write"] == native_depth;
            let sampler_ok = frame.ops.iter().all(|op| match op {
                DrawOp::TexturedMesh(r) => r.sampler.is_some_and(|s| {
                    (s.filter == deadlib_present::render::SamplerFilter::Linear)
                        == (sample["state"]["texture_filtering"] == true)
                        && (s.wrap == deadlib_present::render::SamplerWrap::Repeat)
                            == (sample["state"]["texture_wrapping"] == true)
                }),
                _ => true,
            });
            if rows.len() != expected_count || !texture_ok || !depth_ok || !sampler_ok {
                self.fail(sample, "draw_shape_resource_or_depth", json!({"vertices":rows.len(),"expected_vertices":expected_count,
                    "texture_ok":texture_ok,"depth_ok":depth_ok,"sampler_ok":sampler_ok,"ops":format!("{:?}",frame.ops)}), mismatches);
                continue;
            }
            self.vertices += rows.len();
            let matches = |actual: &[f32;10], row: &Value| row_matches(actual, row, image_uv_scale, expected_texture.is_none());
            let geometry_ok = if sample["primitive"] == "Quads" {
                rows.chunks_exact(6).zip(reference.chunks_exact(4)).all(|(actual, native)| {
                    // GL_QUADS allows either diagonal. Both emitted triangles
                    // must cover the complete native quad, including attributes.
                    let triangle = |actual: &[[f32;10]], indices: [usize;3]| {
                        [[0,1,2],[0,2,1],[1,0,2],[1,2,0],[2,0,1],[2,1,0]].iter().any(|p|
                            (0..3).all(|i| matches(&actual[p[i]], &native[indices[i]])))
                    };
                    [([0,1,2],[0,2,3]),([0,1,3],[1,2,3])].iter().any(|&(a,b)|
                        triangle(&actual[..3], a) && triangle(&actual[3..],b)
                            || triangle(&actual[..3],b) && triangle(&actual[3..],a))
                })
            } else { rows.iter().zip(reference).all(|(actual,native)|matches(actual,native)) };
            if !geometry_ok {
                let example=rows.iter().enumerate().find(|(vertex, actual)| {
                    let candidates=if sample["primitive"]=="Quads" { &reference[vertex/6*4..vertex/6*4+4] }
                        else { &reference[*vertex..*vertex+1] };
                    !candidates.iter().any(|row|matches(actual,row))
                });
                self.fail(sample,"submitted_clip_uv_color_or_topology",json!({"example_vertex":example,
                    "first_actual_triangle":&rows[..rows.len().min(3)],"first_native_quad":&reference[..reference.len().min(4)]}),mismatches);
            }
        }
        Ok(())
    }

    fn fail(&mut self, sample: &Value, field: &str, actual: Value, mismatches: &mut Vec<Value>) {
        self.failures += 1;
        if self.failures <= 100 {
            mismatches.push(json!({"field":field,"actor":sample["actor"],
            "frame":sample["update_sequence"],"deadsync":actual}));
        }
    }

    pub fn report(&self, inputs: usize) -> Value {
        json!({"input_events_replayed":inputs,"draw_samples_compared":self.samples,
            "submitted_vertices_compared":self.vertices,"live_text_samples_compared":self.text_samples,
            "draw_mismatches":self.failures,"equivalent_offscreen_culls":self.culled_samples,"pixel_parity_proven":false})
    }
}

fn row_matches(actual: &[f32;10], row: &Value, image_uv_scale: [f32;2], untextured: bool) -> bool {
    actual.iter().enumerate().all(|(axis,&actual)|
        untextured && (4..6).contains(&axis) || close(actual, native_number(&row[axis+3])
            * if (4..6).contains(&axis) {image_uv_scale[axis-4]} else {1.0}))
}

// Reject a missing production draw only when native geometry can contribute
// pixels. All corners outside one homogeneous clip plane prove a safe cull.
fn fully_clipped(rows: &[Value]) -> bool {
    if rows.is_empty() || rows.iter().any(|r| (3..7).any(|i| !native_number(&r[i]).is_finite())) { return false; }
    (0..3).any(|axis| rows.iter().all(|r| native_number(&r[axis+3]) > native_number(&r[6])+0.0001)
        || rows.iter().all(|r| native_number(&r[axis+3]) < -native_number(&r[6])-0.0001))
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

fn submitted_rows(frame: &RenderFrame) -> Vec<[f32; 10]> {
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
