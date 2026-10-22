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
        // Native records raw music time; command playback uses elapsed time
        // from beat zero. Composition still receives the raw music clock.
        let elapsed = (second - context.song_timing.as_ref()
            .map_or(0.0, |timing| timing.get_time_for_beat_exact(0.0)))
            / deadsync_song_lua::song_music_rate(&context);
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
            .map(|l| compiled_overlay_states_at(l, &context, beat, elapsed))
            .collect();
        let locals: Vec<_> = compiled
            .iter()
            .map(|l| compiled_local_states_at(l, &context, beat, elapsed))
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
            let batched = batch_native_quad_passes(expected, ops.len());
            let batched_refs = batched.as_ref().map(|samples| samples.iter().collect::<Vec<_>>());
            let expected = batched_refs.as_deref().unwrap_or(expected);
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
                        "compiled_pose": {"position":[locals[l][i].x,locals[l][i].y,locals[l][i].z],
                            "rotation":[locals[l][i].rot_x_deg,locals[l][i].rot_y_deg,locals[l][i].rot_z_deg],
                            "quaternion":locals[l][i].quaternion,
                            "alignment":[locals[l][i].halign,locals[l][i].valign]},
                        "native_texture":sample["state"]["texture"],
                        "compiled_texture":match op { DrawOp::Sprite(r) => Some(composer.texture_key(r.texture_handle)),
                            DrawOp::TexturedMesh(r) => Some(composer.texture_key(r.texture_handle)), _ => None },
                        "actual_vertices":(mismatches.is_empty()).then(|| native_gameplay::submitted_rows(&submitted)),
                        "native_vertices":(mismatches.is_empty()).then(|| sample["vertices"].clone()),
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

fn batch_native_quad_passes(expected: &[&Value], passes: usize) -> Option<Vec<Value>> {
    if !matches!(passes, 1 | 2) || expected.len() <= passes || expected.len() % passes != 0 {
        return None;
    }
    if !expected.iter().enumerate().all(|(i, sample)| {
        sample["primitive"] == "Quads"
            && sample["state"] == expected[i % passes]["state"]
            && sample["vertices"].as_array().is_some_and(|v| v.len() == 4)
    }) {
        return None;
    }
    // Sprite::DrawPrimitives calls DrawTexture (diffuse, then glow) for the
    // center and each fade edge. Production batches the diffuse cells and the
    // glow cells separately. Only reorder disjoint cells, keeping both passes
    // at each cell in order. Overlapping geometry, shadows, missing passes and
    // render-state changes must still fail the normal pass-count comparison.
    if passes == 2 {
        let mut cells: Vec<[f64; 5]> = Vec::new();
        for pair in expected.chunks_exact(2) {
            let rows = pair[0]["vertices"].as_array()?;
            let other = pair[1]["vertices"].as_array()?;
            if !rows
                .iter()
                .zip(other)
                .all(|(a, b)| (0..9).all(|i| a[i] == b[i]))
            {
                return None;
            }
            let mut bounds = [
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                rows[0][2].as_f64()?,
            ];
            for row in rows {
                let x = row[0].as_f64()?;
                let y = row[1].as_f64()?;
                if !x.is_finite() || !y.is_finite() || row[2].as_f64()? != bounds[4] {
                    return None;
                }
                bounds[0] = bounds[0].min(x);
                bounds[1] = bounds[1].max(x);
                bounds[2] = bounds[2].min(y);
                bounds[3] = bounds[3].max(y);
            }
            if bounds[0] >= bounds[1] || bounds[2] >= bounds[3] {
                return None;
            }
            for x in &bounds[..2] {
                for y in &bounds[2..4] {
                    if rows
                        .iter()
                        .filter(|r| r[0].as_f64() == Some(*x) && r[1].as_f64() == Some(*y))
                        .count()
                        != 1
                    {
                        return None;
                    }
                }
            }
            for previous in &cells {
                // Native computes shared fade edges via different float
                // expressions. Allow only their rounding seam here; vertex
                // comparison below retains its original tolerances.
                let scale = bounds[..4]
                    .iter()
                    .chain(&previous[..4])
                    .fold(1.0_f64, |s, v| s.max(v.abs()));
                let seam = scale * f64::from(f32::EPSILON) * 2.0;
                let overlap_x = bounds[1].min(previous[1]) - bounds[0].max(previous[0]);
                let overlap_y = bounds[3].min(previous[3]) - bounds[2].max(previous[2]);
                if bounds[4] != previous[4] || (overlap_x > seam && overlap_y > seam) {
                    return None;
                }
            }
            cells.push(bounds);
        }
    }
    Some(
        (0..passes)
            .map(|pass| {
                let mut sample = expected[pass].clone();
                sample["vertices"] = Value::Array(
                    expected
                        .iter()
                        .skip(pass)
                        .step_by(passes)
                        .flat_map(|s| s["vertices"].as_array().unwrap().iter().cloned())
                        .collect(),
                );
                sample
            })
            .collect(),
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
    let equal = geometry_matches(&rows, native, quads, |actual, expected| {
        compiled_row_matches(actual, expected, uv, expected_texture.is_empty(),
            native_gameplay::native_depth_range(state))
    });
    Ok((!equal).then_some("clip_uv_color_or_topology"))
}

fn geometry_matches(
    rows: &[[f32; 10]],
    native: &[Value],
    quads: bool,
    matches: impl Fn(&[f32; 10], &Value) -> bool,
) -> bool {
    if !quads {
        return rows.len() == native.len() && rows.iter().zip(native).all(|(a,b)| matches(a,b));
    }
    if native.len() % 4 != 0 || rows.len() != native.len() / 4 * 6 { return false; }
    let mut used = vec![false; native.len() / 4];
    rows.chunks_exact(6).all(|a| {
        native.chunks_exact(4).enumerate().any(|(index, b)| {
            if used[index] { return false; }
            let triangle = |actual: &[[f32; 10]], indices: [usize; 3]| {
                [[0,1,2], [0,2,1], [1,0,2], [1,2,0], [2,0,1], [2,1,0]]
                    .iter().any(|p| (0..3).all(|i| matches(&actual[p[i]], &b[indices[i]])))
            };
            let equal = [([0,1,2], [0,2,3]), ([0,1,3], [1,2,3])].iter().any(|&(x,y)|
                triangle(&a[..3], x) && triangle(&a[3..], y)
                || triangle(&a[..3], y) && triangle(&a[3..], x));
            used[index] = equal;
            equal
        })
    })
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
fn batched_diffuse_glow_requires_disjoint_paired_cells_and_matching_state() {
    let quad = |left: f32, right: f32, color: f32| {
        json!({
            "primitive": "Quads", "state": {"texture": "song:/texture.png", "blend": 0},
            "vertices": [[left,0,0,left,0,0,1,0,0,color,0,0,1],
                         [left,1,0,left,1,0,1,0,1,color,0,0,1],
                         [right,1,0,right,1,0,1,1,1,color,0,0,1],
                         [right,0,0,right,0,0,1,1,0,color,0,0,1]]
        })
    };
    let input = vec![
        quad(0.0, 1.0, 0.25),
        quad(0.0, 1.0, 0.75),
        quad(1.0, 2.0, 0.25),
        quad(1.0, 2.0, 0.75),
    ];
    let batch = |input: &[Value]| batch_native_quad_passes(&input.iter().collect::<Vec<_>>(), 2);
    let merged = batch(&input).unwrap();
    assert_eq!(merged.len(), 2);
    for pass in 0..2 {
        let expected: Vec<_> = input
            .iter()
            .skip(pass)
            .step_by(2)
            .flat_map(|v| v["vertices"].as_array().unwrap().iter().cloned())
            .collect();
        assert_eq!(merged[pass]["vertices"], json!(expected));
    }
    assert!(batch(&input[..3]).is_none(), "missing glow pass");
    let mut changed = input.clone();
    changed[2]["state"]["blend"] = json!(1);
    assert!(batch(&changed).is_none(), "different render state");
    changed = input.clone();
    changed[1]["vertices"][0][3] = json!(0.5);
    assert!(batch(&changed).is_none(), "shadow or shifted glow");
    changed = input.clone();
    changed[2] = quad(0.5, 1.5, 0.25);
    changed[3] = quad(0.5, 1.5, 0.75);
    assert!(
        batch(&changed).is_none(),
        "overlapping cells cannot commute"
    );
    changed[2] = input[0].clone();
    changed[3] = input[1].clone();
    assert!(batch(&changed).is_none(), "duplicate cell");
    changed = input.clone();
    changed.swap(1, 2);
    assert!(batch(&changed).is_none(), "unpaired passes");
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

#[test]
fn batched_quads_require_every_native_vertex_once() {
    let native: Vec<_> = (0..8).map(|i| json!(i)).collect();
    let row = |i: usize| { let mut row = [0.0; 10]; row[0] = i as f32; row };
    let mut actual: Vec<_> = [4,5,6,4,6,7,0,1,2,0,2,3].into_iter().map(row).collect();
    let matches = |a: &[f32;10], b: &Value| a[0] == b.as_f64().unwrap() as f32;
    assert!(geometry_matches(&actual, &native, true, matches));
    assert!(!geometry_matches(&actual[..6], &native, true, matches));
    actual[6..].copy_from_slice(&[row(4),row(5),row(6),row(4),row(6),row(7)]);
    assert!(!geometry_matches(&actual, &native, true, matches), "duplicate quad must not replace the missing quad");
    actual[6..].copy_from_slice(&[row(0),row(1),row(2),row(0),row(2),row(7)]);
    assert!(!geometry_matches(&actual, &native, true, matches), "changed corner must fail");
}
