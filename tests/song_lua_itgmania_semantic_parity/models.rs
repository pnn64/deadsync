//! Native Model observations and comparisons of the production mesh pipeline.

use super::*;
use deadlib_present::render::{DrawOp, MeshSampler, RenderFrame, SamplerFilter, SamplerWrap, textured_mesh_uvs};
use deadsync_song_lua::playback::actor_conformance::{
    WholeSongComposer, matrix_rows, multiply_matrices, project_world,
};

#[derive(Deserialize)]
pub(super) struct NativeModelTrack {
    actor: String,
    definition_id: String,
    class: String,
    native_loaded: bool,
    sample_layout: Vec<String>,
    samples: Vec<(f64, f64, bool, Vec<NativeModelDraw>)>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VertexBuffers {
    local: usize,
    world: usize,
    view: usize,
    clip: usize,
    ndc: usize,
    screen: usize,
    uv: usize,
    transformed_uv: usize,
    color: usize,
}

#[derive(Deserialize)]
struct NativeModelDraw {
    primitive: String,
    primitive_index: usize,
    model_mesh_index: usize,
    model_mesh_name: String,
    vertex_count: usize,
    vertex_buffers: VertexBuffers,
    normals_buffer: usize,
    texture_matrix_scale_buffer: usize,
    texture_mode: String,
    #[serde(default)] // The native Lua serializer omits an unbound texture.
    texture: Value,
    texture_filtering: bool,
    texture_wrapping: bool,
    sphere_environment: bool,
    blend_mode: usize,
    cull_mode: usize,
    z_test: usize,
    z_write: bool,
    lighting: bool,
    lights: Value,
    material: NativeMaterial,
    texture_matrix: [[f64; 4]; 4],
    render_target: usize,
    viewport: [u32; 2],
}

#[derive(Deserialize)]
struct NativeMaterial {
    ambient: [f64; 4],
    diffuse: [f64; 4],
    emissive: [f64; 4],
    specular: [f64; 4],
    shininess: f64,
}

fn column(
    trace: &NativeTrace,
    id: usize,
    count: usize,
    width: usize,
) -> Result<&[Vec<Option<f64>>], String> {
    let rows = id
        .checked_sub(1)
        .and_then(|index| trace.model_geometry_buffers.get(index))
        .ok_or_else(|| format!("Model buffer {id} is not a valid one-based reference"))?;
    if rows.len() != count || rows.iter().any(|row| row.len() != width) {
        return Err(format!(
            "Model buffer {id} must contain {count} rows of width {width}"
        ));
    }
    Ok(rows)
}

fn validate_draw(trace: &NativeTrace, draw: &NativeModelDraw) -> Result<(), String> {
    let fields = &draw.vertex_buffers;
    for (id, width) in [
        (fields.local, 4),
        (fields.world, 4),
        (fields.view, 4),
        (fields.clip, 4),
        (fields.ndc, 3),
        (fields.screen, 3),
        (fields.uv, 2),
        (fields.transformed_uv, 2),
        (fields.color, 4),
        (draw.normals_buffer, 3),
        (draw.texture_matrix_scale_buffer, 2),
    ] {
        column(trace, id, draw.vertex_count, width)?;
    }
    if draw.primitive != "triangles" || draw.vertex_count % 3 != 0 {
        return Err("native Model observations must retain complete triangle vertices".into());
    }
    if !matches!(draw.texture_mode.as_str(), "modulate" | "glow") {
        return Err(format!("unknown Model texture mode {}", draw.texture_mode));
    }
    if !draw.texture.is_null() && !draw.texture.as_str().is_some_and(|path| !path.is_empty()) {
        return Err("native Model texture binding must be a source path or explicit null".into());
    }
    if draw.viewport.contains(&0) {
        return Err("invalid native Model viewport".into());
    }
    if draw
        .texture_matrix
        .iter()
        .flatten()
        .chain(draw.material.ambient.iter())
        .chain(draw.material.diffuse.iter())
        .chain(draw.material.emissive.iter())
        .chain(draw.material.specular.iter())
        .chain(std::iter::once(&draw.material.shininess))
        .any(|value| !value.is_finite())
    {
        return Err("invalid native Model matrix or material metadata".into());
    }
    Ok(())
}

/// Require complete observations; `compare_models` checks production draws.
pub(super) fn validate_models(trace: &NativeTrace) -> Result<(), String> {
    let mut expected = HashMap::new();
    for definition in trace
        .actor_definitions
        .iter()
        .filter(|definition| definition.class == "Model")
    {
        let actors = definition.runtime_actors.iter().map(String::as_str).chain(
            (definition.runtime_actors.is_empty()
                && trace
                    .runtime_actors
                    .iter()
                    .any(|actor| actor.id == definition.id))
            .then_some(definition.id.as_str()),
        );
        for actor in actors {
            if expected.insert(actor, definition.id.as_str()).is_some() {
                return Err(format!(
                    "duplicate native Model definition identity {actor}"
                ));
            }
        }
    }
    let has_manual = trace
        .manual_draw_frames
        .iter()
        .any(|(_, _, calls)| calls.iter().any(|call| call["class"] == "Model"));
    if expected.is_empty() && trace.model_geometry_tracks.is_empty() && !has_manual {
        return Ok(());
    }
    if trace.capabilities["actor_base_rotation"].as_bool() != Some(true) {
        return Err("native Model reference omits Actor base rotation; recapture with harness 0.1.38 or newer".into());
    }
    if trace.capabilities["model_texture_matrix_scale"].as_bool() != Some(true) {
        return Err("native Model reference omits per-vertex texture matrix scaling; recapture with harness 0.1.39 or newer".into());
    }
    for capability in ["model_hardware_mesh_path", "model_update_order", "model_texture_bindings"] {
        if trace.capabilities[capability].as_bool() != Some(true) {
            return Err(format!("native Model reference omits {capability}; recapture with harness 0.1.42 or newer"));
        }
    }
    if trace.model_texture_units != Some(1) {
        return Err("native Model reference requires the observed desktop one-texture-unit profile".into());
    }
    for dimension in [trace.display.width, trace.display.height] {
        if !dimension.is_finite()
            || dimension <= 0.0
            || dimension.fract() != 0.0
            || f64::from(dimension) > f64::from(u32::MAX)
        {
            return Err("native Model display requires positive integral pixel dimensions".into());
        }
    }
    if trace.model_geometry_encoding.as_deref() != Some("column-buffer-v1") {
        return Err("native Model geometry requires column-buffer-v1 observations".into());
    }
    if trace.model_geometry_sample_clock.as_deref() != Some("update_frames") {
        return Err("native Model geometry must observe every update frame".into());
    }
    let mut seen = HashSet::new();
    for track in &trace.model_geometry_tracks {
        if !seen.insert(track.actor.as_str())
            || expected.get(track.actor.as_str()) != Some(&track.definition_id.as_str())
        {
            return Err(format!(
                "missing, duplicate or mismatched native Model identity {}",
                track.actor
            ));
        }
        if track.class != "Model"
            || !track.native_loaded
            || track.sample_layout != ["beat", "seconds", "visible", "primitives"]
        {
            return Err(format!("invalid native Model track {}", track.actor));
        }
        if trace.update_frames.is_empty() || track.samples.len() != trace.update_frames.len() {
            return Err(format!(
                "Model {} is missing update-frame observations",
                track.actor
            ));
        }
        for (sample, frame) in track.samples.iter().zip(&trace.update_frames) {
            if (sample.0, sample.1) != *frame || !sample.0.is_finite() || !sample.1.is_finite() {
                return Err(format!(
                    "Model {} has a mismatched update clock",
                    track.actor
                ));
            }
            if !sample.2 && !sample.3.is_empty() {
                return Err(format!("invisible Model {} contains draws", track.actor));
            }
            for (index, draw) in sample.3.iter().enumerate() {
                if draw.primitive_index != index {
                    return Err(format!("Model {} loses primitive order", track.actor));
                }
                validate_draw(trace, draw)?;
            }
        }
    }
    if expected.len() != seen.len() {
        return Err("instantiated native Models lack geometry tracks".into());
    }
    let clocks = trace
        .update_frames
        .iter()
        .map(|&(beat, seconds)| (beat.to_bits(), seconds.to_bits()))
        .collect::<HashSet<_>>();
    for (beat, seconds, calls) in &trace.manual_draw_frames {
        for call in calls.iter().filter(|call| call["class"] == "Model") {
            if !clocks.contains(&(beat.to_bits(), seconds.to_bits())) {
                return Err("manual Model draw has no matching native update clock".into());
            }
            let actor = call["actor"]
                .as_str()
                .ok_or("manual Model lacks actor identity")?;
            if !seen.contains(actor) {
                return Err(format!(
                    "manual Model {actor} lacks an ordinary geometry track"
                ));
            }
            let draws: Vec<NativeModelDraw> = serde_json::from_value(call["primitives"].clone())
                .map_err(|error| format!("invalid manual Model primitives: {error}"))?;
            for (index, draw) in draws.iter().enumerate() {
                if draw.primitive_index != index {
                    return Err("manual Model loses primitive order".into());
                }
                validate_draw(trace, draw)?;
            }
        }
    }
    Ok(())
}

fn check_row<const N: usize>(
    parity: &mut Parity,
    reported: &mut HashSet<String>,
    key: &str,
    actual: [f32; N],
    expected: &[Option<f64>],
    clock: f64,
) {
    let mut failed = reported.contains(key);
    for (axis, actual) in actual.into_iter().enumerate() {
        let expected = expected[axis];
        parity.check_once(
            expected.is_some_and(|expected| (actual as f64 - expected).abs() <= EPSILON as f64),
            &mut failed,
            || format!("{key} at {clock:.6}s axis {axis}: {actual} != {expected:?}"),
        );
    }
    if failed {
        reported.insert(key.into());
    }
}

fn check_flag(
    parity: &mut Parity,
    reported: &mut HashSet<String>,
    key: &str,
    ok: bool,
    message: impl FnOnce() -> String,
) {
    let mut failed = reported.contains(key);
    parity.check_once(ok, &mut failed, message);
    if failed {
        reported.insert(key.into());
    }
}

fn model_texture_key(
    trace: &NativeTrace,
    context: &SongLuaCompileContext,
    texture: &Value,
) -> Result<Option<String>, String> {
    // Native disables texture sampling for an unbound material. Our mesh
    // shaders express that with the built-in white texel, never an asset file.
    if texture.is_null() {
        return Ok(Some(deadsync_noteskin::model::MODEL_WHITE_TEXTURE.into()));
    }
    let raw = texture
        .as_str()
        .ok_or_else(|| "native Model texture must be a string or null".to_string())?;
    let path = if let Some(relative) = raw.strip_prefix("noteskin:/") {
        if !trace.noteskin_reference.as_ref().is_some_and(|skin|
            skin.files.iter().any(|file| file.path == Path::new(relative))) {
            return Err(format!("Model texture is absent from the native noteskin inventory: {raw}"));
        }
        Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/noteskins").join(relative)
    } else if let Some(relative) = raw.strip_prefix("judgment:/") {
        if !trace.judgment_reference.as_ref().is_some_and(|graphic| graphic.path == Path::new(relative)) {
            return Err(format!("Model texture is absent from the native judgment inventory: {raw}"));
        }
        Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/graphics/judgements").join(relative)
    } else {
        raw.strip_prefix("song:/").map_or_else(|| {
            let path = PathBuf::from(raw);
            if path.is_absolute() { path } else { context.song_dir.join(path) }
        }, |relative| context.song_dir.join(relative))
    };
    // Native Model requests differ from Sprite RageTextureIDs for this file.
    Ok(Some(deadsync_assets::textures::model_texture_key(
        &deadsync_assets::textures::canonical_texture_key(path))))
}

fn compare_frame(
    trace: &NativeTrace,
    draws: &[NativeModelDraw],
    frame: &RenderFrame,
    matrices: [[[f32; 4]; 4]; 2],
    composer: &WholeSongComposer,
    context: &SongLuaCompileContext,
    actor: &str,
    clock: f64,
    parity: &mut Parity,
    reported: &mut HashSet<String>,
) {
    let actual = frame
        .ops
        .iter()
        .flat_map(|op| match op {
            DrawOp::TexturedMesh(run) => frame.tmesh_instances[run.instance_start as usize..]
                [..run.instance_count as usize]
                .iter()
                .map(|instance| (run, instance))
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect::<Vec<_>>();
    check_flag(
        parity,
        reported,
        &format!("Model {actor} pass count"),
        actual.len() == draws.len()
            && frame
                .ops
                .iter()
                .all(|op| matches!(op, DrawOp::TexturedMesh(_))),
        || {
            format!(
                "Model {actor} at {clock:.6}s has {} mesh passes, expected {}",
                actual.len(),
                draws.len()
            )
        },
    );
    for (pass, ((run, instance), draw)) in actual.iter().zip(draws).enumerate() {
        let vertices = &frame.tmesh_geometries[run.geometry as usize].vertices;
        let prefix = format!("Model {actor} pass {pass}");
        let expected_texture = model_texture_key(trace, context, &draw.texture);
        let actual_texture = (run.texture_handle != 0).then(|| composer.texture_key(run.texture_handle));
        check_flag(parity, reported, &format!("{prefix} texture binding"),
            expected_texture.as_ref().is_ok_and(|expected| actual_texture == expected.as_deref())
                && run.additive_texture == 0,
            || format!("Model {actor} at {clock:.6}s pass {pass} binds {actual_texture:?}, native binds {expected_texture:?}"));
        let expected_sampler = MeshSampler {
            filter: if draw.texture_filtering { SamplerFilter::Linear }
                else { SamplerFilter::Nearest },
            wrap: if draw.texture_wrapping { SamplerWrap::Repeat }
                else { SamplerWrap::Clamp },
        };
        check_flag(parity, reported, &format!("{prefix} sampler"),
            run.sampler == Some(expected_sampler),
            || format!("Model {actor} pass {pass} sampler {:?} differs from native {expected_sampler:?}", run.sampler));
        check_flag(
            parity,
            reported,
            &format!("{prefix} vertex count"),
            vertices.len() == draw.vertex_count,
            || {
                format!(
                    "Model {actor} mesh {:?} pass {pass} loses native vertices",
                    draw.model_mesh_name
                )
            },
        );
        let mode = if instance.texture_mask > 0.5 {
            "glow"
        } else {
            "modulate"
        };
        check_flag(
            parity,
            reported,
            &format!("{prefix} mode"),
            mode == draw.texture_mode,
            || format!("Model {actor} pass {pass} changes native diffuse/glow order"),
        );
        check_flag(
            parity,
            reported,
            &format!("{prefix} depth"),
            run.depth_test == (draw.z_test != 0),
            || format!("Model {actor} pass {pass} changes native depth testing"),
        );
        check_flag(
            parity,
            reported,
            &format!("{prefix} cull"),
            instance.cull_mode
                == match draw.cull_mode {
                    0 => 1.0,
                    1 => 2.0,
                    2 => 0.0,
                    _ => f32::NAN,
                },
            || format!("Model {actor} pass {pass} changes native face culling"),
        );
        check_flag(
            parity,
            reported,
            &format!("{prefix} lighting"),
            !draw.lighting
                && draw
                    .lights
                    .as_object()
                    .is_some_and(|lights| lights.is_empty()),
            || format!("Model {actor} has native lighting that is not compared"),
        );
        check_flag(
            parity,
            reported,
            &format!("{prefix} unsupported state"),
            ((draw.z_write && draw.z_test == 1) || (!draw.z_write && draw.z_test == 0))
                && matches!(draw.cull_mode, 0..=2)
                && draw.render_target == 0,
            || {
                format!(
                    "Model {actor} has native depth/cull/target states that are not represented"
                )
            },
        );
        let blend = match run.blend {
            deadlib_present::render::BlendMode::Alpha => 0,
            deadlib_present::render::BlendMode::Add => 1,
            _ => usize::MAX,
        };
        check_flag(
            parity,
            reported,
            &format!("{prefix} blend"),
            blend == draw.blend_mode,
            || format!("Model {actor} pass {pass} changes native blending"),
        );
        let matrix = matrix_rows(frame.cameras[usize::from(run.camera)] * instance.transform());
        let world_matrix = multiply_matrices(matrices[0], matrix_rows(instance.transform()));
        let fields = &draw.vertex_buffers;
        let local = column(trace, fields.local, draw.vertex_count, 4)
            .expect("validated Model local column");
        let world = column(trace, fields.world, draw.vertex_count, 4)
            .expect("validated Model world column");
        let view =
            column(trace, fields.view, draw.vertex_count, 4).expect("validated Model view column");
        let clip =
            column(trace, fields.clip, draw.vertex_count, 4).expect("validated Model clip column");
        let ndc =
            column(trace, fields.ndc, draw.vertex_count, 3).expect("validated Model NDC column");
        let screen = column(trace, fields.screen, draw.vertex_count, 3)
            .expect("validated Model screen column");
        let uv = column(trace, fields.uv, draw.vertex_count, 2).expect("validated Model UV column");
        let transformed_uv = column(trace, fields.transformed_uv, draw.vertex_count, 2)
            .expect("validated Model transformed UV column");
        let normals = column(trace, draw.normals_buffer, draw.vertex_count, 3)
            .expect("validated Model normals");
        let scale = column(
            trace,
            draw.texture_matrix_scale_buffer,
            draw.vertex_count,
            2,
        )
        .expect("validated Model texture scale");
        let color = column(trace, fields.color, draw.vertex_count, 4)
            .expect("validated Model color column");
        let material = std::array::from_fn::<_, 4, _>(|axis| {
            if axis == 3 {
                draw.material.diffuse[3]
            } else {
                draw.material.diffuse[axis]
                    + (draw.material.emissive[axis] + draw.material.ambient[axis])
            }
            .clamp(0.0, 1.0)
        });
        for (vertex_index, vertex) in vertices.iter().take(draw.vertex_count).enumerate() {
            let sphere = instance.texture_mask <= 0.5 && vertex.normal[3] as u8 & 1 != 0;
            check_flag(parity, reported, &format!("{prefix} sphere state"),
                sphere == draw.sphere_environment,
                || format!("Model {actor} pass {pass} changes native sphere mapping state"));
            let position = [vertex.pos[0], vertex.pos[1], vertex.pos[2], 1.0];
            let actual_world = project_world(world_matrix, position);
            let actual_view = project_world(matrices[1], actual_world);
            let actual_clip = project_world(matrix, position);
            let actual_ndc = std::array::from_fn(|axis| actual_clip[axis] / actual_clip[3]);
            let actual_screen = [
                (actual_ndc[0] + 1.0) * draw.viewport[0] as f32 * 0.5,
                (1.0 - actual_ndc[1]) * draw.viewport[1] as f32 * 0.5,
                actual_ndc[2],
            ];
            for (field, actual, reference) in [
                ("local", position, &local[vertex_index]),
                ("world", actual_world, &world[vertex_index]),
                ("view", actual_view, &view[vertex_index]),
                ("clip", actual_clip, &clip[vertex_index]),
            ] {
                check_row(
                    parity,
                    reported,
                    &format!("Model {actor} mesh {} {field}", draw.model_mesh_index),
                    actual,
                    reference,
                    clock,
                );
            }
            for (field, actual, reference) in [
                ("ndc", actual_ndc, &ndc[vertex_index]),
                ("screen", actual_screen, &screen[vertex_index]),
                (
                    "normal",
                    [vertex.normal[0], vertex.normal[1], vertex.normal[2]],
                    &normals[vertex_index],
                ),
            ] {
                check_row(
                    parity,
                    reported,
                    &format!("Model {actor} mesh {} {field}", draw.model_mesh_index),
                    actual,
                    reference,
                    clock,
                );
            }
            for (field, actual, reference) in [
                ("uv", vertex.uv, &uv[vertex_index]),
                (
                    "transformed_uv",
                    textured_mesh_uvs(*vertex, **instance)[0],
                    &transformed_uv[vertex_index],
                ),
                (
                    "texture_scale",
                    vertex.tex_matrix_scale,
                    &scale[vertex_index],
                ),
            ] {
                check_row(
                    parity,
                    reported,
                    &format!("Model {actor} mesh {} {field}", draw.model_mesh_index),
                    actual,
                    reference,
                    clock,
                );
            }
            let expected_color = std::array::from_fn::<_, 4, _>(|axis| {
                color[vertex_index][axis].map(|value| material[axis] * value / 255.0)
            });
            check_row(
                parity,
                reported,
                &format!("Model {actor} mesh {} color", draw.model_mesh_index),
                std::array::from_fn::<_, 4, _>(|axis| vertex.color[axis] * instance.tint[axis]),
                &expected_color,
                clock,
            );
        }
    }
}

pub(super) fn compare_models(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    context: &SongLuaCompileContext,
    parity: &mut Parity,
) {
    let has_models = trace.actor_definitions.iter().any(|definition| {
        definition.class == "Model"
            && (!definition.runtime_actors.is_empty()
                || trace
                    .runtime_actors
                    .iter()
                    .any(|actor| actor.id == definition.id))
    });
    let has_manual = trace
        .manual_draw_frames
        .iter()
        .any(|(_, _, calls)| calls.iter().any(|call| call["class"] == "Model"));
    if !has_models && trace.model_geometry_tracks.is_empty() && !has_manual {
        return;
    }
    parity.section("native Model meshes");
    if let Err(error) = validate_models(trace) {
        parity.check(false, || error);
        return;
    }
    // Validation above checks finite, positive, integral physical dimensions;
    // logical presentation bounds are configured independently by the adapter.
    deadlib_present::space::set_current_window_px(
        trace.display.width as u32,
        trace.display.height as u32,
    );
    let map = projected_drawable_map(trace, compiled);
    let mut composers = compiled
        .iter()
        .map(|layer| WholeSongComposer::new(&layer.overlays))
        .collect::<Vec<_>>();
    for (composer, layer) in composers.iter_mut().zip(compiled) {
        composer.set_draw_frames(&layer.overlays, &layer.draw_frames);
    }
    let mut reported = HashSet::new();
    // Consume one native update at a time; retaining a composed song state for
    // every sample would multiply memory by both actor count and song duration.
    for (sample_index, &(beat, seconds)) in trace.update_frames.iter().enumerate() {
        let mut states = HashMap::new();
        for track in &trace.model_geometry_tracks {
            let Some(&(layer, index)) = map.get(&track.actor) else {
                let mut failed = reported.contains(&track.actor);
                parity.check_once(false, &mut failed, || {
                    format!("native Model {} has no compiled actor", track.actor)
                });
                reported.insert(track.actor.clone());
                continue;
            };
            let states = states.entry(layer).or_insert_with(|| {
                compiled_overlay_states_at(&compiled[layer], context, beat as f32, seconds as f32)
            });
            let sample = &track.samples[sample_index];
            let frame = composers[layer].render_overlay(
                &compiled[layer].overlays,
                states,
                index,
                [context.screen_width, context.screen_height],
                // Actor tracks use music time; material history uses elapsed time.
                seconds as f32,
                beat as f32,
            );
            compare_frame(
                trace,
                &sample.3,
                &frame,
                composers[layer].model_matrices(
                    states,
                    index,
                    [context.screen_width, context.screen_height],
                ),
                &composers[layer],
                context,
                &track.actor,
                seconds,
                parity,
                &mut reported,
            );
        }
    }
    compare_manual_models(
        trace,
        compiled,
        context,
        parity,
        &map,
        &mut composers,
        &mut reported,
    );
}

fn compare_manual_models(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    context: &SongLuaCompileContext,
    parity: &mut Parity,
    map: &HashMap<String, (usize, usize)>,
    composers: &mut [WholeSongComposer],
    reported: &mut HashSet<String>,
) {
    for (beat, seconds, calls) in &trace.manual_draw_frames {
        let mut rendered = HashMap::new();
        for call in calls.iter().filter(|call| call["class"] == "Model") {
            let actor = call["actor"]
                .as_str()
                .expect("validated manual Model identity");
            let Some(&(layer, index)) = map.get(actor) else {
                parity.check(false, || {
                    format!("manual Model {actor} has no compiled actor")
                });
                continue;
            };
            let frames = rendered.entry(layer).or_insert_with(|| {
                let states = compiled_overlay_states_at(
                    &compiled[layer],
                    context,
                    *beat as f32,
                    *seconds as f32,
                );
                std::collections::VecDeque::from(
                    composers[layer]
                        .render_manual_frame(
                            &compiled[layer].overlays,
                            &states,
                            [context.screen_width, context.screen_height],
                            overlay_update_time(
                                context,
                                SongLuaTimeUnit::Second,
                                *beat as f32,
                                *seconds as f32,
                            ),
                            *beat as f32,
                        )
                        .into_iter()
                        .filter(|(index, _, _)| {
                            kind_name(&compiled[layer].overlays[*index].kind) == "Model"
                        })
                        .collect::<Vec<_>>(),
                )
            });
            let Some((actual_index, frame, matrices)) = frames.pop_front() else {
                parity.check(false, || {
                    format!("manual Model {actor} at {seconds:.6}s has no production draw")
                });
                continue;
            };
            parity.check(actual_index == index, || {
                format!("manual Model {actor} at {seconds:.6}s loses draw order")
            });
            let draws: Vec<NativeModelDraw> = serde_json::from_value(call["primitives"].clone())
                .expect("validated manual Model primitives");
            compare_frame(
                trace, &draws, &frame, matrices, &composers[layer], context,
                actor, *seconds, parity, reported,
            );
        }
        for (layer, frames) in rendered {
            parity.check(frames.is_empty(), || {
                format!("layer {layer} at {seconds:.6}s has extra manual Model draws")
            });
        }
    }
}

#[test]
fn native_model_columns_preserve_nulls() {
    let mut value = model_trace_value();
    value["model_geometry_buffers"][3][0][0] = Value::Null;
    let trace: NativeTrace = serde_json::from_value(value).expect("nullable native Model column");
    validate_models(&trace).expect("null kind must survive shape validation");
    assert_eq!(trace.model_geometry_buffers[3][0][0], None);
    let mut parity = Parity::default();
    parity.section("null Model projection");
    check_row(
        &mut parity,
        &mut HashSet::new(),
        "clip",
        [0.0; 4],
        &trace.model_geometry_buffers[3][0],
        0.0,
    );
    assert!(
        !parity.is_complete(),
        "an undefined native value cannot pass as zero"
    );
}

#[test]
fn native_model_texture_paths_require_reference_inventory() {
    crate::paths::init();
    let mut trace: NativeTrace = serde_json::from_value(model_trace_value()).unwrap();
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/itgmania-song-lua-micro/model-texture-images");
    let context = SongLuaCompileContext::new(&directory, "Model texture identities");
    let texture = serde_json::json!("song:/frame-red.png");
    assert_eq!(
        model_texture_key(&trace, &context, &texture).unwrap(),
        Some(deadsync_assets::textures::model_texture_key(
            &deadsync_assets::textures::canonical_texture_key(directory.join("frame-red.png"))
        ))
    );
    assert_eq!(
        model_texture_key(&trace, &context, &Value::Null).unwrap(),
        Some(deadsync_noteskin::model::MODEL_WHITE_TEXTURE.into())
    );
    for invalid in [
        serde_json::json!(false),
        serde_json::json!(42),
        serde_json::json!({}),
    ] {
        assert!(model_texture_key(&trace, &context, &invalid).is_err());
    }
    let texture = serde_json::json!("noteskin:/dance/cyber/textures/Tap Note parts (mipmaps).png");
    assert!(model_texture_key(&trace, &context, &texture).is_err());
    let relative = PathBuf::from("dance/cyber/textures/Tap Note parts (mipmaps).png");
    trace.noteskin_reference = Some(NativeNoteskin {
        skin: "cyber".into(),
        files: vec![NativeResourceFile {
            path: relative.clone(),
            sha256: "inventory already verified by compilation".into(),
        }],
    });
    assert_eq!(
        model_texture_key(&trace, &context, &texture).unwrap(),
        Some(deadsync_assets::textures::model_texture_key(
            &deadsync_assets::textures::canonical_texture_key(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("assets/noteskins")
                    .join(relative)
            )
        ))
    );
}

#[test]
fn native_model_columns_reject_incomplete_observations() {
    let value = model_trace_value();
    validate_models(&serde_json::from_value(value.clone()).expect("complete control")).unwrap();
    let mut mutations = Vec::new();
    for (path, replacement) in [
        ("/display/width", serde_json::json!(0)),
        ("/display/height", serde_json::json!(480.5)),
        ("/model_geometry_encoding", serde_json::json!("dense")),
        (
            "/model_geometry_sample_clock",
            serde_json::json!("beat_step"),
        ),
        (
            "/model_geometry_tracks/0/actor",
            serde_json::json!("missing"),
        ),
        (
            "/model_geometry_tracks/0/native_loaded",
            serde_json::json!(false),
        ),
        (
            "/model_geometry_tracks/0/samples/0/1",
            serde_json::json!(0.1),
        ),
        (
            "/model_geometry_tracks/0/samples/0/2",
            serde_json::json!(false),
        ),
        (
            "/model_geometry_tracks/0/samples/0/3/0/primitive_index",
            serde_json::json!(1),
        ),
        (
            "/model_geometry_tracks/0/samples/0/3/0/vertex_count",
            serde_json::json!(4),
        ),
        (
            "/model_geometry_tracks/0/samples/0/3/0/vertex_buffers/uv",
            serde_json::json!(0),
        ),
        (
            "/model_geometry_tracks/0/samples/0/3/0/normals_buffer",
            serde_json::json!(99),
        ),
        (
            "/model_geometry_tracks/0/samples/0/3/0/texture_matrix_scale_buffer",
            serde_json::json!(1),
        ),
        (
            "/model_geometry_buffers/6",
            serde_json::json!([[0, 0], [0, 0]]),
        ),
        ("/model_geometry_buffers/0/0", serde_json::json!([0, 0, 0])),
        ("/model_geometry_tracks/0/samples", serde_json::json!([])),
        ("/model_geometry_tracks", serde_json::json!([])),
        ("/capabilities/actor_base_rotation", serde_json::json!(false)),
        ("/capabilities/model_texture_matrix_scale", serde_json::json!(false)),
        ("/capabilities/model_hardware_mesh_path", serde_json::json!(false)),
        ("/capabilities/model_update_order", serde_json::json!(false)),
        ("/capabilities/model_texture_bindings", serde_json::json!(false)),
        ("/model_texture_units", serde_json::json!(0)),
        ("/model_texture_units", serde_json::json!(2)),
        ("/model_geometry_tracks/0/samples/0/3/0/texture", serde_json::json!("")),
        ("/model_geometry_tracks/0/samples/0/3/0/texture", serde_json::json!(true)),
        ("/capabilities", serde_json::json!({})),
    ] {
        let mut changed = value.clone();
        *changed.pointer_mut(path).expect("control mutation path") = replacement;
        mutations.push((path, changed));
    }
    let mut duplicate = value.clone();
    duplicate["model_geometry_tracks"]
        .as_array_mut()
        .unwrap()
        .push(value["model_geometry_tracks"][0].clone());
    mutations.push(("duplicate identity", duplicate));
    let mut duplicate_definition = value.clone();
    duplicate_definition["actor_definitions"]
        .as_array_mut()
        .unwrap()
        .push(value["actor_definitions"][0].clone());
    mutations.push(("duplicate definition identity", duplicate_definition));
    let mut absent = value.clone();
    absent["model_geometry_tracks"][0]["samples"][0][3][0]["vertex_buffers"]
        .as_object_mut()
        .unwrap()
        .remove("view");
    mutations.push(("missing vertex field", absent));
    let mut extra = value.clone();
    extra["model_geometry_tracks"][0]["samples"][0][3][0]["vertex_buffers"]["unknown"] =
        serde_json::json!(1);
    mutations.push(("unknown vertex field", extra));
    for field in ["texture", "texture_filtering", "texture_wrapping", "sphere_environment"] {
        let mut absent = value.clone();
        absent["model_geometry_tracks"][0]["samples"][0][3][0].as_object_mut().unwrap().remove(field);
        mutations.push((field, absent));
    }
    for (name, changed) in mutations {
        let rejected = serde_json::from_value::<NativeTrace>(changed)
            .map(|trace| validate_models(&trace).is_err())
            .unwrap_or(true);
        assert!(rejected, "{name} must not establish native Model coverage");
    }
    let mut manual = value.clone();
    manual["manual_draw_frames"] = serde_json::json!([[0,0,[{"class":"Model","actor":"actor",
        "primitives":value["model_geometry_tracks"][0]["samples"][0][3]}]]]);
    validate_models(&serde_json::from_value(manual.clone()).unwrap())
        .expect("valid manual column references");
    let mut bad_clock = manual.clone();
    bad_clock["manual_draw_frames"][0][0] = serde_json::json!(0.125);
    assert!(validate_models(&serde_json::from_value(bad_clock).unwrap()).is_err());
    manual["manual_draw_frames"][0][2][0]["primitives"][0]["vertex_buffers"]["world"] =
        serde_json::json!(0);
    assert!(validate_models(&serde_json::from_value(manual).unwrap()).is_err());
}

fn model_trace_value() -> Value {
    let buffers = [4, 4, 4, 4, 3, 3, 2, 2, 4, 3, 2].map(|width| vec![vec![Some(0.0); width]; 3]);
    let draw = serde_json::json!({"primitive":"triangles","primitive_index":0,"model_mesh_index":0,
        "model_mesh_name":"Triangle","vertex_count":3,
        "vertex_buffers":{"local":1,"world":2,"view":3,"clip":4,"ndc":5,"screen":6,"uv":7,"transformed_uv":8,"color":9},
        "normals_buffer":10,"texture_matrix_scale_buffer":11,"texture_mode":"modulate",
        "texture":null,"texture_filtering":true,"texture_wrapping":false,"sphere_environment":false,
        "blend_mode":0,"cull_mode":0,"z_test":1,"z_write":true,"lighting":false,"lights":{},
        "material":{"ambient":[0,0,0,1],"diffuse":[1,1,1,1],"emissive":[0,0,0,0],"specular":[0,0,0,0],"shininess":1},
        "texture_matrix":[[1,0,0,0],[0,1,0,0],[0,0,1,0],[0,0,0,1]],"render_target":0,"viewport":[854,480]});
    serde_json::json!({"oracle":"itgmania_native_actor_conformance","title":"model columns","style":"single",
        "simfile":"","roots":["actor"],"runtime_actors":[{"id":"actor","path":"actor"}],
        "actor_definitions":[{"id":"actor","class":"Model","runtime_actors":["actor"]}],
        "timeline_tracks":[],"tween_tracks":[],"end_position":{"seconds":1},
        "display":{"width":854,"height":480,"logical_width":854,"logical_height":480},
        "fixture_context":{"beat_step":0.25},"trace_until_beat":1,
        "capabilities":{"actor_base_rotation":true,"model_texture_matrix_scale":true,
            "model_hardware_mesh_path":true,"model_update_order":true,"model_texture_bindings":true},
        "model_texture_units":1,
        "update_frames":[[0,0],[1,1]],"model_geometry_encoding":"column-buffer-v1",
        "model_geometry_sample_clock":"update_frames","model_geometry_buffers":buffers,
        "model_geometry_tracks":[{"actor":"actor","definition_id":"actor","class":"Model","native_loaded":true,
            "sample_layout":["beat","seconds","visible","primitives"],"samples":[[0,0,true,[draw]],[1,1,false,[]]]}]})
}

#[test]
#[ignore = "requires an explicitly selected complete native Model trace"]
fn native_model_trace_columns_cover_every_update() {
    let path = std::env::var_os(TRACE_ENV).expect("selected native trace");
    let trace = read_trace_file(Path::new(&path));
    validate_models(&trace).expect("complete original Model observations");
    assert!(!trace.model_geometry_tracks.is_empty());
    let samples = trace
        .model_geometry_tracks
        .iter()
        .map(|track| track.samples.len())
        .sum::<usize>();
    eprintln!(
        "native Model capture: {} tracks, {} update frames, {samples} samples, {} column buffers",
        trace.model_geometry_tracks.len(),
        trace.update_frames.len(),
        trace.model_geometry_buffers.len()
    );
}

#[test]
fn native_model_elapsed_clock_ignores_simfile_offset() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/itgmania-song-lua-micro/model-clock-offset");
    let trace = read_trace_file(&root.join("native.json"));
    let (compiled, _, context) = compile_trace_song_at(&trace, &root.join("offset.ssc"));
    let origin = context
        .song_timing
        .as_ref()
        .expect("native song timing")
        .get_time_for_beat_exact(0.0);
    assert!((origin + 0.010).abs() <= 0.000_001);
    assert_eq!(trace.update_frames.len(), 61);
    let mut parity = Parity::default();
    compare_models(&trace, &compiled, &context, &mut parity);
    eprintln!("{}", parity.summary(&trace.title));
    parity.assert_complete("native Model material clock with simfile offset");
    assert!(
        parity.checks() > 10_000,
        "compare complete Model observations"
    );
}

#[test]
#[ignore = "requires an explicitly selected native trace and its original simfile"]
fn native_model_meshes_match_selected_trace() {
    crate::paths::init();
    let trace_path = std::env::var_os(TRACE_ENV).expect("selected native trace");
    let simfile = std::env::var_os(SIMFILE_ENV).expect("selected original simfile");
    let trace = read_trace_file(Path::new(&trace_path));
    let (compiled, _, context) = compile_trace_song_at(&trace, Path::new(&simfile));
    let mut parity = Parity::default();
    compare_models(&trace, &compiled, &context, &mut parity);
    eprintln!("{}", parity.summary(&trace.title));
    parity.assert_complete("selected native Model observations");
    assert!(
        parity.checks() > trace.update_frames.len(),
        "exercise native Model geometry"
    );
}

#[test]
fn native_model_cull_modes_match() {
    crate::paths::init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/itgmania-song-lua-micro/model-cull-modes");
    let trace = read_trace_file(&root.join("native.json"));
    let (compiled, _, context) = compile_trace_song_at(&trace, &root.join("control.ssc"));
    let mut parity = Parity::default();
    compare_models(&trace, &compiled, &context, &mut parity);
    eprintln!("{}", parity.summary(&trace.title));
    parity.assert_complete("native Model culling defaults, setters, commands and updates");
    assert_eq!(trace.model_geometry_tracks.len(), 12);
    assert!(
        parity.checks() > 340_000,
        "compare every native Model observation"
    );
}

#[test]
#[ignore = "diagnoses each Model's first visible observation in a selected original trace"]
fn native_model_initial_frames_match_selected_trace() {
    crate::paths::init();
    let trace_path = std::env::var_os(TRACE_ENV).expect("selected native trace");
    let simfile = std::env::var_os(SIMFILE_ENV).expect("selected original simfile");
    let trace = read_trace_file(Path::new(&trace_path));
    validate_models(&trace).expect("complete original Model observations");
    deadlib_present::space::set_current_window_px(
        trace.display.width as u32,
        trace.display.height as u32,
    );
    let (compiled, _, context) = compile_trace_song_at(&trace, Path::new(&simfile));
    let map = projected_drawable_map(&trace, &compiled);
    let mut parity = Parity::default();
    parity.section("first visible Model frames");
    let mut reported = HashSet::new();
    for track in &trace.model_geometry_tracks {
        let Some((beat, seconds, _, draws)) =
            track.samples.iter().find(|sample| !sample.3.is_empty())
        else {
            continue;
        };
        let &(layer, index) = map.get(&track.actor).expect("compiled original Model");
        let states =
            compiled_overlay_states_at(&compiled[layer], &context, *beat as f32, *seconds as f32);
        let actor = &compiled[layer].overlays[index];
        eprintln!(
            "{} {} state {:?}",
            track.actor,
            kind_name(&actor.kind),
            states[index]
        );
        if let SongLuaOverlayKind::NoteskinActor { slots, .. } = &actor.kind {
            for slot in slots.iter() {
                eprintln!(
                    "slot rotation {} draw {:?}",
                    slot.def.rotation_deg,
                    slot.model_draw_at(*seconds as f32, *beat as f32)
                );
            }
        }
        let mut composer = WholeSongComposer::new(&compiled[layer].overlays);
        let frame = composer.render_overlay(
            &compiled[layer].overlays,
            &states,
            index,
            [context.screen_width, context.screen_height],
            *seconds as f32,
            *beat as f32,
        );
        let matrices = composer.model_matrices(
            &states,
            index,
            [context.screen_width, context.screen_height],
        );
        for op in &frame.ops {
            if let DrawOp::TexturedMesh(run) = op {
                let instance = &frame.tmesh_instances[run.instance_start as usize];
                let vertex = frame.tmesh_geometries[run.geometry as usize].vertices[0];
                let point = project_world(
                    multiply_matrices(matrices[0], matrix_rows(instance.transform())),
                    [vertex.pos[0], vertex.pos[1], vertex.pos[2], 1.0],
                );
                eprintln!(
                    "{} first production world {:?} transform {:?}",
                    track.actor,
                    point,
                    matrix_rows(instance.transform())
                );
            }
        }
        compare_frame(
            &trace,
            draws,
            &frame,
            matrices,
            &composer,
            &context,
            &track.actor,
            *seconds,
            &mut parity,
            &mut reported,
        );
    }
    eprintln!("{}", parity.summary(&trace.title));
    parity.assert_complete("selected first visible Model observations (partial diagnostic)");
    assert!(parity.checks() > 0);
}
