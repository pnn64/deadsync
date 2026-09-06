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
            // Compare the complete Player/NoteField wrapper composition through
            // projected geometry, since those coordinate spaces are distinct.
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
                let descendants = deadsync_song_lua::overlay_descendants_by_parent(
                    compiled.overlays.len(),
                    index,
                    |child| compiled.overlays[child].parent_index,
                );
                eprintln!(
                    "{name}: kind={}, descendants={}, tree commands={}",
                    kind_name(&actor.kind),
                    descendants.len(),
                    actor.message_commands.len()
                        + descendants
                            .iter()
                            .map(|&child| compiled.overlays[child].message_commands.len())
                            .sum::<usize>()
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
    let decoration = trace
        .actor_definitions
        .iter()
        .find(|actor| actor.name.as_deref() == Some("MultitapDeco1_1"))
        .expect("first multitap decoration")
        .children[0]
        .definition_id
        .clone();
    trace
        .projected_vertex_tracks
        .retain(|track| track.definition_id.as_ref() == Some(&decoration));
    assert_eq!(trace.projected_vertex_tracks.len(), 1);
    trace.roots = vec!["root".into()];
    trace.draw_orders.clear();
    trace.actor_definitions = serde_json::from_value(serde_json::json!([
        {"id":"root", "class":"ActorFrame", "children":[{"layer_index":1,"definition_id":"camera"}]},
        {"id":"camera", "class":"ActorFrame", "name":"Camera", "children":[
            {"layer_index":1,"definition_id":"explosion"}, {"layer_index":2,"definition_id":"deco"}]},
        {"id":"explosion", "class":"Sprite", "name":"Explosion"},
        {"id":"deco", "class":"Sprite", "name":"Decoration"}
    ])).expect("native actor tree");
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

#[test]
fn multitap_boundaries_and_noteskin_commands_match() {
    let song = tempfile::tempdir().expect("song directory");
    let entry = song.path().join("default.lua");
    fs::write(&entry, r#"
local skin = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Current"):NoteSkin()
multitaps = {Challenge = {{lane=1, taps={8,12}, peak=6}}}
MYSTERIOUS_VERSION_DEPENDENT_RADIAN_POLTERGEIST = -180 / math.pi
return Def.ActorFrame{
    Def.ActorFrame{
        InitCommand=function(self) self:xy(427,240):fov(45):vanishpoint(427,240) end,
        Def.ActorFrame{
            Name="MultitapFrameP1", InitCommand=function(self) self:y(10) end,
            NOTESKIN:LoadActorForNoteSkin("Down", "Explosion", skin)..{Name="MultitapExplosionP1_1"},
            Def.ActorFrame{
                Name="MultitapP1_1", OnCommand=function(self) self:visible(false) end,
                NOTESKIN:LoadActorForNoteSkin("Down", "Tap Note", skin)..{Name="MultitapArrowP1_1"},
                Def.ActorFrame{
                    Name="MultitapDeco1_1", OnCommand=function(self) self:visible(false):z(10) end,
                    Def.Quad{Name="Decoration", InitCommand=function(self) self:zoomto(128,128):zoom(0.3) end},
                },
            },
        },
    },
}
"#).expect("write multitap chart");
    let mut context = SongLuaCompileContext::new(song.path(), "multitap boundaries");
    context.players[1].enabled = false;
    context.players[0].difficulty = SongLuaDifficulty::Challenge;
    context.players[0].speedmod = SongLuaSpeedMod::X(1.0);
    context.song_timing_bpms = vec![(0.0, 137.0)];
    for skin in ["cyber", "cel"] {
        context.players[0].noteskin_name = skin.into();
        let compiled = compile_song_lua_layers(&[entry.as_path()], 0, &context)
            .expect("compile multitap")
            .remove(0);
        let named = |name: &str| {
            compiled
                .overlays
                .iter()
                .position(|actor| actor.name.as_deref() == Some(name))
                .expect(name)
        };
        let frame = named("MultitapP1_1");
        let arrow = named("MultitapArrowP1_1");
        let deco = named("Decoration");
        let explosion = named("MultitapExplosionP1_1");
        let explosion_sprites = deadsync_song_lua::overlay_descendants_by_parent(
            compiled.overlays.len(),
            explosion,
            |index| compiled.overlays[index].parent_index,
        )
        .into_iter()
        .filter(|&index| {
            matches!(
                compiled.overlays[index].kind,
                SongLuaOverlayKind::Sprite { .. }
            )
        })
        .collect::<Vec<_>>();
        for (beat, visible, y, zoom_y) in [
            (0.0, false, 0.0, 1.0),
            (0.001, true, 386.936, 1.0),
            (8.0, true, -125.0, 1.0),
            (9.0, true, -53.0, 1.05),
            (10.0, true, -29.0, 0.9),
            (11.0, true, -53.0, 1.05),
            (12.001, false, 0.0, 1.0),
        ] {
            let local = compiled_local_states_at(
                &compiled,
                &context,
                beat,
                song_elapsed_seconds_at(beat, &context),
            );
            assert_eq!(local[frame].visible, visible, "beat {beat}");
            let composed = compose_overlay_states(&compiled.overlays, &local, [854.0, 480.0]);
            for &index in &explosion_sprites {
                if !composed[index].visible {
                    continue;
                }
                let rendered =
                    deadsync_theme_simply_love::screens::gameplay::actor_conformance::effect_sample(
                        composed[index],
                        song_elapsed_seconds_at(beat, &context),
                        beat,
                    );
                assert_eq!(
                    rendered.tint[3], 0.0,
                    "{skin}: no tap judgment at beat {beat}"
                );
                assert_eq!(rendered.glow[3], 0.0, "{skin}: no tap glow at beat {beat}");
            }
            if visible {
                assert!(
                    (local[frame].y - y).abs() < 0.003,
                    "beat {beat}: y={}",
                    local[frame].y
                );
                assert!(
                    (local[frame].zoom_y - zoom_y).abs() < 0.001,
                    "beat {beat}: zoom={}",
                    local[frame].zoom_y
                );
                assert!(
                    local[arrow].rot_x_deg.abs() < 0.001,
                    "legacy rotation workaround must cancel the binding's one-radian alias"
                );
                assert_eq!(composed[deco].z, 10.0, "inherit decoration depth");
                assert!(
                    (composed[deco].y - (250.0 + y)).abs() < 0.003,
                    "preserve NoteField offset"
                );
            }
        }
        assert!(
            compiled.messages.is_empty(),
            "tap effects must wait for judgments"
        );
        let commands = compiled
            .overlays
            .iter()
            .flat_map(|actor| &actor.message_commands)
            .filter(|command| command.message == "__songlua_tap_1_1_W1")
            .collect::<Vec<_>>();
        assert!(!commands.is_empty(), "capture noteskin grade commands");
        assert!(
            commands.iter().any(|command| {
                let lit = deadsync_song_lua::overlay_state_after_blocks(
                    SongLuaOverlayState {
                        diffuse: [1.0, 1.0, 1.0, 0.0],
                        ..Default::default()
                    },
                    &command.blocks,
                    0.0,
                );
                lit.diffuse[3] > 0.99 && lit.zoom > 1.0
            }),
            "native Cyber W1 command lights and scales the sprite"
        );

        for actor in &compiled.overlays {
            for command in actor
                .message_commands
                .iter()
                .filter(|command| command.message == "__songlua_tap_1_1_W1")
            {
                for elapsed in [0.151, 0.175, 0.2, 0.5, 1.0, 10.0] {
                    let state = deadsync_song_lua::overlay_state_after_blocks(
                        actor.initial_state,
                        &command.blocks,
                        elapsed,
                    );
                    let rendered = deadsync_theme_simply_love::screens::gameplay::actor_conformance::effect_sample(state, elapsed, elapsed * 137.0 / 60.0);
                    assert_eq!(
                        rendered.glow[3], 0.0,
                        "{skin}: expired tap glow at {elapsed}"
                    );
                }
            }
        }
    }
}
