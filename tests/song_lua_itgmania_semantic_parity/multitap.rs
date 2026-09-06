use super::*;
use std::collections::BTreeMap;

// Multitap uses SetUpdateFunction, whose writes are in operation_tracks, not
// the UpdateCommand tween tracks handled by the general comparator.
pub(super) fn compare_multitap(
    trace: &NativeTrace,
    compiled: &[CompiledSongLua],
    context: &SongLuaCompileContext,
    gaps: &mut Vec<String>,
) {
    if !trace.actor_definitions.iter().any(|actor| {
        actor
            .name
            .as_deref()
            .is_some_and(|name| name.starts_with("MultitapFrameP"))
    }) {
        return;
    }
    compare_zoom_hides(trace, compiled, gaps);
    for (layer, compiled) in compiled.iter().enumerate() {
        let mut writes = Vec::new();
        for definition in &trace.actor_definitions {
            let Some(name) = definition.name.as_deref() else {
                continue;
            };
            // The field wrapper is relocated into screen coordinates by the
            // compiler. Its transform is checked by projected geometry instead.
            if !name.starts_with("Multitap") || name.starts_with("MultitapFrame") {
                continue;
            }
            let Some(index) = compiled
                .overlays
                .iter()
                .position(|actor| actor.name.as_deref() == Some(name))
            else {
                gaps.push(format!("layer {layer} multitap actor missing: {name}"));
                continue;
            };
            for track in trace
                .operation_tracks
                .iter()
                .filter(|track| definition.runtime_actors.contains(&track.actor))
            {
                for sample in &track.samples {
                    writes.push((index, name, track, sample));
                }
            }
            if name.starts_with("MultitapExplosion") {
                let actor = &compiled.overlays[index];
                eprintln!(
                    "{name}: kind={}, children={}, message commands={}",
                    kind_name(&actor.kind),
                    compiled
                        .overlays
                        .iter()
                        .filter(|child| child.parent_index == Some(index))
                        .count(),
                    actor.message_commands.len()
                );
            }
        }
        writes.sort_by(|a, b| a.3.2.total_cmp(&b.3.2).then(a.3.0.cmp(&b.3.0)));
        let mut states = Vec::new();
        let mut last_seconds = None;
        let mut checked = 0usize;
        let mut failures = BTreeMap::<(String, String), (usize, String)>::new();
        for (index, name, track, (seq, beat, seconds, args)) in writes {
            if last_seconds != Some(*seconds) {
                states = compiled_local_states_at(compiled, context, *beat, *seconds);
                last_seconds = Some(*seconds);
            }
            let operation = NativeTweenOperation {
                seq: *seq,
                operation: track.operation.clone(),
                args: args.clone(),
            };
            let Some((expected, actual)) =
                operation_values(trace, track, *seconds, &operation, &states[index])
            else {
                continue;
            };
            checked += 1;
            if expected.len() == actual.len()
                && expected
                    .iter()
                    .zip(&actual)
                    .all(|(expected, actual)| render_value_matches(expected, actual))
            {
                continue;
            }
            let family = name
                .split_once(|c: char| c.is_ascii_digit())
                .map_or(name, |(prefix, _)| prefix)
                .to_owned();
            let entry = failures
                .entry((family, track.operation.clone()))
                .or_insert_with(|| {
                    (
                        0,
                        format!(
                            "{name} at beat {beat:.6}: ITGmania {expected:?}, DeadSync {actual:?}"
                        ),
                    )
                });
            entry.0 += 1;
        }
        eprintln!(
            "multitap operation audit: {checked} sampled writes checked, {} mismatching writes in {} actor/property groups",
            failures.values().map(|entry| entry.0).sum::<usize>(),
            failures.len()
        );
        for ((family, operation), (count, first)) in failures {
            gaps.push(format!(
                "{family} {operation}: {count} mismatching writes; first {first}"
            ));
        }
    }
}

#[test]
fn perspective_geometry_survives_noteskin_kind_change() {
    use deadsync_assets::song_lua::SongLuaOverlayActor;
    let mut trace = read_trace_file(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FLIP69_TRACE));
    trace.roots = vec!["root".into()];
    trace.draw_orders.clear();
    trace.actor_definitions = serde_json::from_value(serde_json::json!([
        {"id":"root", "class":"ActorFrame", "children":[{"layer_index":1,"definition_id":"camera"}]},
        {"id":"camera", "class":"ActorFrame", "name":"Camera", "children":[
            {"layer_index":1,"definition_id":"explosion"}, {"layer_index":2,"definition_id":"deco"}]},
        {"id":"explosion", "class":"Sprite", "name":"Explosion"},
        {"id":"deco", "class":"Sprite", "name":"Decoration"}
    ])).expect("native actor tree");
    trace.projected_vertex_tracks.truncate(1);
    let track = &mut trace.projected_vertex_tracks[0];
    track.definition_id = Some("deco".into());
    track.camera_actor = "camera".into();
    // Use the first visible quad recorded by the native flip69 fixture.
    let sample = track
        .samples
        .iter()
        .find(|sample| sample[2] == true)
        .expect("visible decoration")
        .clone();
    track.samples = vec![sample];
    let actor = |name: &str, kind, parent_index, initial_state| SongLuaOverlayActor {
        name: Some(name.into()),
        kind,
        parent_index,
        initial_state,
        message_commands: Vec::new(),
    };
    let mut compiled = CompiledSongLua {
        screen_width: 854.0,
        screen_height: 480.0,
        overlays: vec![
            actor(
                "Camera",
                SongLuaOverlayKind::ActorFrame,
                None,
                SongLuaOverlayState {
                    fov: Some(45.0),
                    vanishpoint: Some([427.0, 240.0]),
                    ..Default::default()
                },
            ),
            actor(
                "Explosion",
                SongLuaOverlayKind::Actor,
                Some(0),
                SongLuaOverlayState::default(),
            ),
            actor(
                "Decoration",
                SongLuaOverlayKind::Quad,
                Some(0),
                SongLuaOverlayState {
                    x: 459.0,
                    y: 636.16444,
                    z: 10.0,
                    zoom: 0.3,
                    ..Default::default()
                },
            ),
        ],
        ..Default::default()
    };
    let context = SongLuaCompileContext::new(Path::new("."), "projection regression");
    let mut gaps = Vec::new();
    compare_projected_geometry(&trace, std::slice::from_ref(&compiled), &context, &mut gaps);
    assert!(
        gaps.is_empty(),
        "native perspective quad must match: {gaps:?}"
    );
    compiled.overlays[2].initial_state.x += 12.0;
    compare_projected_geometry(&trace, &[compiled], &context, &mut gaps);
    assert!(
        gaps.iter()
            .any(|gap| gap.contains("projected center differs")),
        "a missing explosion drawable must not suppress the decoration mismatch: {gaps:?}"
    );
}

fn operation_values(
    trace: &NativeTrace,
    track: &NativeOperationTrack,
    seconds: f32,
    operation: &NativeTweenOperation,
    state: &SongLuaOverlayState,
) -> Option<(
    Vec<SongLuaOverlayUpdateValue>,
    Vec<SongLuaOverlayUpdateValue>,
)> {
    use SongLuaOverlayUpdateValue as V;
    let method = operation.operation.rsplit('.').next()?;
    let vector = |value: &[f32]| value.iter().copied().map(V::F32).collect::<Vec<_>>();
    let args_vector = |value: &Value| {
        value
            .as_array()?
            .iter()
            .map(|value| value_f32(Some(value)).map(V::F32))
            .collect::<Option<Vec<_>>>()
    };
    let pair = match method {
        // Base rotation and ordinary rotation are separate in ITGmania, but
        // DeadSync stores their sum. Check the sum after the ordinary write.
        "baserotationz" => return None,
        "rotationz" => {
            let base = trace
                .operation_tracks
                .iter()
                .find(|base| {
                    base.actor == track.actor && base.operation.ends_with(".baserotationz")
                })
                .and_then(|base| base.samples.iter().rev().find(|sample| sample.2 <= seconds))
                .and_then(|sample| value_f32(sample.3.first()))
                .unwrap_or(0.0);
            (
                vec![V::F32(base + value_f32(operation.args.first())?)],
                vec![V::F32(state.rot_z_deg)],
            )
        }
        "diffusealpha" => (
            vec![V::F32(value_f32(operation.args.first())?)],
            vec![V::F32(state.diffuse[3])],
        ),
        "diffuse" => (
            args_vector(operation.args.first()?)?,
            vector(&state.diffuse),
        ),
        "effectcolor1" => (
            args_vector(operation.args.first()?)?,
            vector(&state.effect_color1),
        ),
        "effectcolor2" => (
            args_vector(operation.args.first()?)?,
            vector(&state.effect_color2),
        ),
        "texturetranslate" => (
            operation
                .args
                .iter()
                .map(|value| value_f32(Some(value)).map(V::F32))
                .collect::<Option<Vec<_>>>()?,
            vector(&state.texcoord_offset.unwrap_or([0.0; 2])),
        ),
        _ => {
            let values = native_render_values(operation);
            if values.is_empty() {
                return None;
            }
            let actual = values
                .iter()
                .map(|(target, _)| overlay_state_render_value(state, *target))
                .collect::<Option<Vec<_>>>()?;
            (values.into_iter().map(|(_, value)| value).collect(), actual)
        }
    };
    Some(pair)
}

fn compare_zoom_hides(trace: &NativeTrace, compiled: &[CompiledSongLua], gaps: &mut Vec<String>) {
    let hides = deadsync_gameplay::build_song_lua_note_hide_windows_for_players(
        compiled
            .iter()
            .flat_map(|layer| &layer.note_hides)
            .map(|hide| (hide.player, hide.column, hide.start_beat, hide.end_beat)),
    );
    let mut checked = 0usize;
    let mut hidden = 0usize;
    let mut columns = 0usize;
    for actor in &trace.external_actors {
        let Some((prefix, _)) = actor.path.split_once("/GetZoomHandler/GetSpline") else {
            continue;
        };
        let Some((player, column)) = prefix
            .strip_prefix("ScreenGameplay/PlayerP")
            .and_then(|path| path.split_once("/NoteField/Column"))
        else {
            continue;
        };
        let (Ok(player), Ok(column)) = (player.parse::<usize>(), column.parse::<usize>()) else {
            continue;
        };
        let handler_path = format!("{prefix}/GetZoomHandler");
        let handler = trace
            .external_actors
            .iter()
            .find(|actor| actor.path == handler_path)
            .expect("zoom spline handler");
        let beats_per_t = trace
            .operation_tracks
            .iter()
            .find(|track| track.actor == handler.id && track.operation == "Spline.SetBeatsPerT")
            .and_then(|track| track.samples.last())
            .and_then(|sample| value_f32(sample.3.first()))
            .expect("zoom spline beat spacing");
        let mut points = BTreeMap::new();
        for track in trace
            .operation_tracks
            .iter()
            .filter(|track| track.actor == actor.id && track.operation == "Spline.SetPoint")
        {
            for (seq, _, _, args) in &track.samples {
                let index = args[0].as_u64().expect("spline point index");
                let point = args[1].as_array().expect("spline point vector");
                let hidden = point
                    .iter()
                    .all(|value| value_f32(Some(value)) == Some(-1.0));
                if points
                    .get(&index)
                    .is_none_or(|(previous, _)| seq > previous)
                {
                    points.insert(index, (*seq, hidden));
                }
            }
        }
        let mut differences = 0;
        for (index, (_, expected)) in points {
            let beat = (index - 1) as f32 * beats_per_t;
            let actual =
                deadsync_gameplay::song_lua_note_hidden(&hides[player - 1], column - 1, beat);
            checked += 1;
            hidden += usize::from(expected);
            if actual != expected {
                if differences == 0 {
                    gaps.push(format!("P{player} column {column} note hiding differs at beat {beat:.6}: ITGmania {expected}, DeadSync {actual}"));
                }
                differences += 1;
            }
        }
        columns += 1;
    }
    assert!(
        checked > 0,
        "multitap zoom spline comparison must not be empty"
    );
    eprintln!(
        "multitap zoom audit: {checked} final spline points in {columns} columns checked ({hidden} hidden)"
    );
}
