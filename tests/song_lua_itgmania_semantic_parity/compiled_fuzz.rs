//! Bounded generated scenes: native submissions versus compiled actor playback.
use super::{
    Parity, compare_compile_info, compiled_local_states_at, compiled_overlay_states_at,
    native_gameplay,
};
use deadlib_present::render::{DrawOp, RenderFrame, SamplerFilter, SamplerWrap};
use deadsync_song_lua::playback::actor_conformance::WholeSongComposer;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

pub(super) fn compare(trace: &Value, trace_path: &Path, simfile: &Path) -> Result<Value, String> {
    let (prepared, context) = native_gameplay::compile_generated(trace, trace_path, simfile)?;
    let compiled: Vec<_> = prepared.compiled_layers().collect();
    if std::env::var_os("DEADSYNC_FUZZ_DEBUG").is_some() {
        for layer in &compiled {
            eprintln!("MESSAGES {:?}", layer.messages);
            for (index, actor) in layer.overlays.iter().enumerate() {
                if actor.name.as_deref().is_some_and(|name| name.starts_with("Fuzz")) {
                    eprintln!("OVERLAY {index}: {actor:?}");
                    for track in layer.overlay_updates.iter().filter(|track| track.overlay_index == index) {
                        eprintln!("TRACK {:?}: first {:?}, last {:?}", track.target, track.samples.first(), track.samples.last());
                    }
                }
            }
        }
    }
    let mut compile_checks = Parity::default();
    for layer in &compiled {
        compare_compile_info(std::slice::from_ref(*layer), &mut compile_checks);
    }
    let frames = trace["update_frames"]
        .as_array()
        .ok_or("missing update frames")?;
    let samples = trace["drawable_samples"]
        .as_array()
        .filter(|v| !v.is_empty())
        .ok_or("empty drawable observations")?;
    let boundaries = trace["drawable_frames"]
        .as_array()
        .filter(|v| !v.is_empty())
        .ok_or("empty drawable frame boundaries")?;
    let definitions = trace["actor_definitions"]
        .as_array()
        .ok_or("missing actor definitions")?;
    let mut bindings = BTreeMap::new();
    // Generated names are unique and source-authored on both sides. Do not bind
    // by observed state or reorder expected geometry to make a comparison pass.
    for definition in definitions {
        if !matches!(definition["class"].as_str(), Some("Quad" | "Sprite")) {
            continue;
        }
        let name = definition["name"]
            .as_str()
            .ok_or("unnamed generated drawable")?;
        let matches: Vec<_> = compiled
            .iter()
            .enumerate()
            .flat_map(|(l, layer)| {
                layer
                    .overlays
                    .iter()
                    .enumerate()
                    .filter_map(move |(i, actor)| {
                        (actor.name.as_deref() == Some(name)).then_some((l, i))
                    })
            })
            .collect();
        if matches.len() != 1 {
            return Err(format!(
                "{name}: expected one compiled actor, found {}",
                matches.len()
            ));
        }
        let id = definition["id"].as_str().ok_or("missing actor id")?;
        bindings.insert(id.to_owned(), matches[0]);
    }
    if bindings.is_empty() {
        return Err("no generated drawable bindings".into());
    }
    let compiled_drawables = compiled
        .iter()
        .flat_map(|l| &l.overlays)
        .filter(|a| {
            a.name
                .as_deref()
                .is_some_and(|name| name.starts_with("FuzzActor"))
        })
        .count();
    if compiled_drawables != bindings.len() {
        return Err("compiled drawable count differs from source definitions".into());
    }
    let mut composers: Vec<_> = compiled
        .iter()
        .map(|l| WholeSongComposer::new(&l.overlays))
        .collect();
    let mut mismatches = Vec::new();
    let mut failures = 0;
    let mut compared = 0;
    let mut cursor = 0;
    let mut culled = 0;
    for boundary in boundaries {
        let sequence = boundary["update_sequence"]
            .as_u64()
            .ok_or("missing frame sequence")?;
        let frame = frames
            .get(sequence.checked_sub(1).ok_or("zero frame sequence")? as usize)
            .ok_or("invalid draw frame anchor")?;
        if frame["scene"] != 1 {
            return Err("generated compiled captures must have one scene".into());
        }
        let second = frame["seconds"].as_f64().ok_or("missing frame seconds")? as f32;
        let beat = frame["beat"].as_f64().ok_or("missing frame beat")? as f32;
        let end = boundary["samples"]
            .as_u64()
            .ok_or("missing sample boundary")? as usize;
        if end < cursor || end > samples.len() {
            return Err("invalid sample boundary".into());
        }
        let mut observed: BTreeMap<&str, Vec<&Value>> = BTreeMap::new();
        for sample in &samples[cursor..end] {
            let id = sample["actor"].as_str().ok_or("missing draw actor")?;
            if !bindings.contains_key(id) {
                return Err(format!("unbound native drawable {id}"));
            }
            observed.entry(id).or_default().push(sample);
        }
        cursor = end;
        // Generated foreground starts at beat zero. Earlier frames must have no song draws.
        if beat < 0.0 && observed.is_empty() {
            continue;
        }
        let states: Vec<_> = compiled
            .iter()
            .map(|l| compiled_overlay_states_at(l, &context, beat, second))
            .collect();
        let locals: Vec<_> = compiled
            .iter()
            .map(|l| compiled_local_states_at(l, &context, beat, second))
            .collect();
        for (id, &(l, i)) in &bindings {
            let layer = &compiled[l];
            let composer = &mut composers[l];
            let mut submitted = composer.render_screen_overlay(
                &layer.overlays,
                &states[l],
                i,
                [context.screen_width, context.screen_height],
                second,
                beat,
                layer.screen_overlay_index.map(|i| locals[l][i]),
            );
            let expected = observed
                .get(id.as_str())
                .map(Vec::as_slice)
                .unwrap_or_default();
            compared += expected.len();
            let offscreen = !expected.is_empty()
                && expected.iter().all(|s| {
                    s["vertices"]
                        .as_array()
                        .is_some_and(|rows| native_gameplay::fully_clipped(rows))
                });
            if submitted.ops.is_empty() && offscreen {
                culled += expected.len();
                continue;
            }
            let ops = std::mem::take(&mut submitted.ops);
            if ops.len() != expected.len() {
                failures += 1;
                if mismatches.len() < 100 {
                    mismatches.push(json!({"field":"draw_pass_count","actor":id,
                    "frame":sequence,"native":expected.len(),"deadsync":ops.len(),
                    "compiled_glow":states[l][i].glow,"ops":format!("{ops:?}")}));
                }
                continue;
            }
            for (op, sample) in ops.into_iter().zip(expected) {
                // Diffuse, glow and shadow are independent native submissions.
                submitted.ops = vec![op];
                if let Some(field) = compare_pass(&submitted, composer, sample, simfile)? {
                    failures += 1;
                    if mismatches.len() < 100 {
                        mismatches.push(json!({"field":field,"actor":id,
                        "frame":sequence,"beat":beat,"seconds":second,
                        "native_texture":sample["state"]["texture"],
                        "compiled_texture":match op { DrawOp::Sprite(r) => Some(composer.texture_key(r.texture_handle)),
                            DrawOp::TexturedMesh(r) => Some(composer.texture_key(r.texture_handle)), _ => None },
                        "first_actual":native_gameplay::submitted_rows(&submitted).first(),
                        "first_native":sample["vertices"][0]}));
                    }
                }
            }
        }
    }
    if cursor != samples.len() || compared != samples.len() {
        return Err("incomplete draw comparison".into());
    }
    Ok(
        json!({"execution_path":"compiled-song-lua","retained_lua_vm":false,
        "compiled_checks_passed":failures == 0 && compile_checks.is_complete(),
        "native_frames":frames.len(),"draw_frames_compared":boundaries.len(),
        "draw_samples_compared":compared,"mismatch_count":failures,"mismatches":mismatches,
        "compile_gaps":compile_checks.gaps,"equivalent_offscreen_culls":culled,
        "pixel_parity_proven":false,
        "pending_coverage":["framebuffer pixels","native modifier and notefield output (use semantic oracle for compiled track checks)",
            "cross-actor draw ordering and blend/cull state",
            "command/query call ordering (compiled playback has no runtime Lua calls)"]}),
    )
}

fn compare_pass(
    frame: &RenderFrame,
    composer: &WholeSongComposer,
    sample: &Value,
    simfile: &Path,
) -> Result<Option<&'static str>, String> {
    let rows = native_gameplay::submitted_rows(frame);
    let native = sample["vertices"].as_array().ok_or("missing vertices")?;
    let quads = sample["primitive"] == "Quads";
    let count = if quads && native.len() % 4 == 0 {
        native.len() / 4 * 6
    } else if sample["primitive"] == "Triangles" {
        native.len()
    } else {
        return Err("unsupported primitive".into());
    };
    if rows.len() != count {
        return Ok(Some("vertex_count"));
    }
    let state = &sample["state"];
    let expected_texture = state["texture"].as_str().unwrap_or("");
    let texture = match frame.ops[0] {
        DrawOp::TexturedMesh(r) => Some(composer.texture_key(r.texture_handle)),
        DrawOp::Sprite(r) => Some(composer.texture_key(r.texture_handle)),
        _ => None,
    };
    if let Some(path) = expected_texture.strip_prefix("song:/") {
        let path = std::fs::canonicalize(simfile.parent().unwrap().join(path))
            .map_err(|e| e.to_string())?;
        // Compiler keys may mix native and forward separators on Windows.
        // Compare resolved file identities, not their spelling.
        if texture
            .and_then(|key| std::fs::canonicalize(key).ok())
            .as_ref()
            != Some(&path)
        {
            return Ok(Some("texture"));
        }
    } else if !expected_texture.is_empty() {
        return Err("unexpected external texture in generated scene".into());
    }
    if let DrawOp::TexturedMesh(r) = frame.ops[0] {
        let filter = r.sampler.map_or(SamplerFilter::Linear, |s| s.filter);
        let wrap = r.sampler.map_or(SamplerWrap::Clamp, |s| s.wrap);
        if !expected_texture.is_empty()
            && ((filter == SamplerFilter::Linear) != (state["texture_filtering"] == true)
                || (wrap == SamplerWrap::Repeat) != (state["texture_wrapping"] == true))
        {
            return Ok(Some("sampler"));
        }
        if r.depth_test != (state["z_test"] != 0)
            || r.depth_write != (state["z_write"] == true)
            || r.depth_test_on_fail != (state["z_test"] == 2)
        {
            return Ok(Some("depth"));
        }
    }
    let uv = std::array::from_fn(|axis| {
        let (a, b) = if axis == 0 {
            ("texture_width", "image_width")
        } else {
            ("texture_height", "image_height")
        };
        state[a].as_f64().unwrap_or(1.0) as f32 / state[b].as_f64().unwrap_or(1.0) as f32
    });
    let matches = |actual: &[f32; 10], expected: &Value| {
        compiled_row_matches(
            actual,
            expected,
            uv,
            expected_texture.is_empty(),
            native_gameplay::native_depth_range(state),
        )
    };
    let equal = if quads {
        rows.chunks_exact(6)
            .zip(native.chunks_exact(4))
            .all(|(a, b)| {
                let triangle = |actual: &[[f32; 10]], indices: [usize; 3]| {
                    [
                        [0, 1, 2],
                        [0, 2, 1],
                        [1, 0, 2],
                        [1, 2, 0],
                        [2, 0, 1],
                        [2, 1, 0],
                    ]
                    .iter()
                    .any(|p| (0..3).all(|i| matches(&actual[p[i]], &b[indices[i]])))
                };
                [([0, 1, 2], [0, 2, 3]), ([0, 1, 3], [1, 2, 3])]
                    .iter()
                    .any(|&(x, y)| {
                        triangle(&a[..3], x) && triangle(&a[3..], y)
                            || triangle(&a[..3], y) && triangle(&a[3..], x)
                    })
            })
    } else {
        rows.iter().zip(native).all(|(a, b)| matches(a, b))
    };
    Ok((!equal).then_some("clip_uv_color_or_topology"))
}

fn compiled_row_matches(
    actual: &[f32; 10],
    expected: &Value,
    uv: [f32; 2],
    untextured: bool,
    depth: [f32; 2],
) -> bool {
    let mut submitted = *actual;
    // Native RageTypes.h FTOC truncates color * 256, clamps to a byte, and
    // the trace expands that byte / 255. Compare in that observable space;
    // do not loosen geometry or UV tolerances to accommodate RGBA8 colors.
    for value in &mut submitted[6..] {
        if value.is_finite() {
            *value = (*value * 256.0).trunc().clamp(0.0, 255.0) / 255.0;
        }
    }
    native_gameplay::row_matches(&submitted, expected, uv, untextured, depth)
}

#[test]
fn native_color_quantization_does_not_hide_geometry_or_color_changes() {
    let expected = json!([
        0,
        0,
        0,
        0.1,
        0.2,
        0.3,
        1,
        0.25,
        0.5,
        192.0 / 255.0,
        128.0 / 255.0,
        1,
        0
    ]);
    let mut actual = [0.1, 0.2, 0.3, 1.0, 0.25, 0.5, 0.75, 0.5, 1.2, -0.1];
    assert!(compiled_row_matches(
        &actual,
        &expected,
        [1.0; 2],
        false,
        [1.0, 0.0]
    ));
    actual[6] = 0.70;
    assert!(!compiled_row_matches(
        &actual,
        &expected,
        [1.0; 2],
        false,
        [1.0, 0.0]
    ));
    actual[6] = 0.75;
    actual[0] += 0.01;
    assert!(!compiled_row_matches(
        &actual,
        &expected,
        [1.0; 2],
        false,
        [1.0, 0.0]
    ));
    actual[0] -= 0.01;
    actual[4] += 0.01;
    assert!(!compiled_row_matches(
        &actual,
        &expected,
        [1.0; 2],
        false,
        [1.0, 0.0]
    ));
}
