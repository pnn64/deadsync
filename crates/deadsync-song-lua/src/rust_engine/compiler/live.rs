//! Production-shaped live compilation and retained-VM screen reconstruction.
use super::*;
type LiveOutput<S, V> = Output<S, V, SongLiveScene>;

/// Compile either existing preparation output or a retained live scene through
/// the shared Rust engine. Application routing is changed after its remaining
/// gameplay and presentation consumers have been migrated.
pub fn compile_song_lua_layers<S, V>(
    paths: &[&Path],
    primary: usize,
    context: &SongLuaCompileContext,
    noteskins: SongLuaNoteskinResolver,
    slots: ModelSlots<S>,
    layer: fn(&S) -> Option<SongLuaOverlayModelLayer<V>>,
    visual: impl FnMut(&SongLuaCompileContext, &str) -> Option<(Kind<S, V>, SongLuaOverlayState)>,
) -> Result<Vec<LiveOutput<S, V>>, String> {
    if !context.live {
        return prepare::<S, V, SongLiveScene>(
            paths, primary, context, noteskins, slots, layer, visual,
        );
    }
    compile_live(paths, primary, context, noteskins, slots, layer, None)
}

/// Preserve the process VM, source globals, object identities and PRNG while
/// constructing a replacement gameplay screen. End output precedes new startup.
pub fn reload_song_lua<S, V>(
    scene: SongLiveScene,
    slots: ModelSlots<S>,
    layer: fn(&S) -> Option<SongLuaOverlayModelLayer<V>>,
) -> Result<Vec<LiveOutput<S, V>>, String> {
    let (runtime, context, paths, primary, ending) =
        scene.reload_parts().map_err(|e| e.to_string())?;
    let entries = paths.iter().map(PathBuf::as_path).collect::<Vec<_>>();
    let mut outputs = compile_live(
        &entries,
        primary,
        &context,
        Default::default(),
        slots,
        layer,
        Some(runtime),
    )?;
    let replacement = outputs
        .iter_mut()
        .find_map(|o| o.live_scene.as_mut())
        .ok_or("gameplay reload did not return an owned Lua scene")?;
    replacement.prepend_startup_output(ending);
    Ok(outputs)
}

fn begin_load(runtime: &mut RustSongRuntime) -> Result<(), String> {
    runtime.begin_live_song_load();
    runtime
        .actors
        .reset_live_playback(&mut runtime.vm)
        .map_err(|e| e.to_string())
}
fn compile_live<S, V>(
    paths: &[&Path],
    primary: usize,
    context: &SongLuaCompileContext,
    noteskins: SongLuaNoteskinResolver,
    slots: ModelSlots<S>,
    layer: fn(&S) -> Option<SongLuaOverlayModelLayer<V>>,
    retained: Option<SongLiveRuntime>,
) -> Result<Vec<LiveOutput<S, V>>, String> {
    let mut timer = SongLuaCompileTimer::start();
    let plan = SongLayerPlan::new(paths, primary, context)?;
    let paths = plan.paths().map(Path::to_path_buf).collect::<Vec<_>>();
    let trace = paths
        .get(primary)
        .cloned()
        .unwrap_or_else(|| context.song_dir.clone());
    let replacing = retained.is_some();
    let owner = match retained {
        Some(runtime) => runtime,
        None => SongLiveRuntime::new(
            RustSongRuntime::new_with_noteskin_resolver(context, noteskins)
                .map_err(|e| e.to_string())?,
        ),
    };
    let mut guard = owner.access();
    let runtime = &mut *guard;
    begin_load(runtime)?;
    runtime
        .begin_song_compilation()
        .map_err(|e| e.to_string())?;
    let (mut screen, mut selected) = if replacing {
        runtime.reconstruct_gameplay_screen(context)
    } else {
        runtime.construct_gameplay_screen(context)
    }
    .map_err(|e| e.to_string())?;
    runtime
        .load_gameplay_theme(screen, &selected)
        .map_err(|e| e.to_string())?;
    timer.push_stage("host");
    #[cfg(feature = "test-support")]
    runtime.observations.start();
    let mut reloads = 0_u32;
    let mut prior = Vec::new();
    let (root, mut activation) = loop {
        #[cfg(feature = "test-support")]
        runtime.observations.skin_active(true);
        let root = runtime
            .initialize_song_layers(screen, &plan)
            .map_err(|e| e.to_string())?
            .ok_or("song screen was released during initialization")?;
        timer.push_stage("execute_init");
        let mut activation = runtime
            .activate_song_layers(screen, &selected)
            .map_err(|e| e.to_string())?
            .ok_or("song screen was released during activation")?;
        if let Some(request) = &activation.new_screen {
            if request != "ScreenGameplay" {
                return Err(format!(
                    "song Lua startup requested unsupported screen '{request}'"
                ));
            }
            if reloads == 128 {
                return Err("song Lua exceeded 128 startup gameplay reloads".into());
            }
            prior.append(&mut activation.broadcasts);
            runtime
                .actors
                .compiler_read(&mut runtime.vm, root, Query::End)
                .map_err(|e| e.to_string())?;
            // Native startup reconstruction runs End, without the live scene's
            // compiled Sound-owner stop pass (no scene has been returned yet).
            runtime.actors.suppress_load_sound_retirement(screen);
            runtime
                .retire_actor_scene(screen)
                .map_err(|e| e.to_string())?;
            begin_load(runtime)?;
            (screen, selected) = runtime
                .reconstruct_gameplay_screen(context)
                .map_err(|e| e.to_string())?;
            runtime
                .load_gameplay_theme(screen, &selected)
                .map_err(|e| e.to_string())?;
            reloads += 1;
            continue;
        }
        prior.append(&mut activation.broadcasts);
        activation.broadcasts = prior;
        break (root, activation);
    };
    timer.push_stage("startup_commands");
    #[cfg(feature = "test-support")]
    let _ = runtime.observations.take();
    runtime
        .actors
        .compiler_read(&mut runtime.vm, root, Query::MarkLayers)
        .map_err(|e| e.to_string())?;
    let mut rows = runtime.read_song_overlay_actors(root, &selected, slots, layer, |_| {})?;
    let screen_value = runtime
        .actor_scene_root(screen)
        .ok_or("released song screen")?;
    runtime
        .actors
        .compiler_read(&mut runtime.vm, screen_value, Query::MarkScreen(primary))
        .map_err(|e| e.to_string())?;
    let screen_state = runtime
        .actors
        .compiler_read(&mut runtime.vm, screen_value, Query::LiveState)
        .map_err(|e| e.to_string())?
        .state
        .unwrap();
    let screen_index = rows.actors.len();
    runtime.append_compiler_row(
        &mut rows,
        screen_value,
        SongLuaOverlayActor {
            kind: Kind::ActorFrame,
            name: Some("ScreenGameplay".into()),
            parent_index: None,
            initial_state: screen_state,
            message_commands: Vec::new(),
        },
    )?;
    let mut tracked = runtime
        .read_song_tracked_actors(screen)
        .map_err(|e| e.to_string())?
        .ok_or("released song screen")?;
    let mut hidden = std::array::from_fn(|p| {
        tracked
            .actors
            .iter()
            .find(|a| matches!(a.target,Target::Player(i) if i==p))
            .is_some_and(|a| !a.actor.initial_state.visible)
    });
    finalize_live_kinds(runtime, &mut rows, &mut hidden)?;
    let mut tags = Vec::with_capacity(rows.actors.len());
    let mut actors = Vec::with_capacity(rows.actors.len());
    for row in &mut rows.actors {
        let value = runtime
            .song_overlay_actor_value(row)
            .map_err(|e| e.to_string())?;
        tags.push(
            runtime
                .actors
                .compiler_read(&mut runtime.vm, value, Query::Layer(primary))
                .map_err(|e| e.to_string())?
                .layer,
        );
        actors.push(value);
    }
    let mut out = LiveOutput::<S, V> {
        startup: activation.options,
        loaded_steps: Some(selected.player_step_indices.map(|index| {
            index.and_then(|index| {
                if selected.song_step_chart_indices.is_empty() {
                    Some(index)
                } else {
                    selected
                        .song_step_chart_indices
                        .get(index)
                        .copied()
                        .flatten()
                }
            })
        })),
        loaded_style: Some(selected.style_name.clone()),
        startup_reloads: reloads,
        entry_path: trace.clone(),
        screen_width: selected.screen_width,
        screen_height: selected.screen_height,
        screen_overlay_index: Some(screen_index),
        hidden_players: hidden,
        sound_paths: runtime
            .actors
            .compiler_read(&mut runtime.vm, root, Query::SoundPaths)
            .map_err(|e| e.to_string())?
            .paths,
        ..Default::default()
    };
    for (row, &value) in rows.actors.iter_mut().zip(&actors) {
        row.actor.initial_state = runtime
            .actors
            .compiler_read(&mut runtime.vm, value, Query::LiveState)
            .map_err(|e| e.to_string())?
            .state
            .unwrap();
    }
    let mut tracks = Vec::with_capacity(tracked.actors.len());
    for row in &tracked.actors {
        let value = runtime
            .song_tracked_actor_value(row)
            .map_err(|e| e.to_string())?;
        let state = runtime
            .actors
            .compiler_read(&mut runtime.vm, value, Query::LiveState)
            .map_err(|e| e.to_string())?
            .state
            .unwrap();
        tracks.push((row.target, value));
        match row.target {
            Target::Player(p) => out.player_actors[p].initial_state = state,
            Target::PlayerJudgment(p) => out.player_actors[p].judgment.initial_state = state,
            Target::PlayerCombo(p) => out.player_actors[p].combo.initial_state = state,
            Target::SongForeground => out.song_foreground.initial_state = state,
            Target::ScreenLayer(i) => out.screen_layers[i].initial_state = state,
        }
    }
    out.overlays = std::mem::take(&mut rows.actors)
        .into_iter()
        .map(|a| a.actor)
        .collect();
    runtime
        .release_song_overlay_actors(&mut rows)
        .map_err(|e| e.to_string())?;
    runtime
        .release_song_tracked_actors(&mut tracked)
        .map_err(|e| e.to_string())?;
    drop(guard);
    out.live_scene = Some(
        SongLiveScene::new_shared(
            owner,
            &selected,
            SongLiveSceneBindings {
                screen,
                root,
                actors,
                tracked: tracks,
                entry_paths: paths.clone(),
                primary_index: primary,
                startup_broadcasts: std::mem::take(&mut activation.broadcasts),
            },
        )
        .map_err(|e| e.to_string())?,
    );
    timer.push_stage("live_scene");
    crate::log_song_lua_compile_timing(&trace, &timer);
    if paths.is_empty()
        || (paths.len() == 1
            && tags.iter().all(|&i| i == 0)
            && out
                .overlays
                .iter()
                .enumerate()
                .all(|(i, a)| a.parent_index.is_none_or(|p| p < i)))
    {
        out.live_indices.extend(0..out.overlays.len());
        return Ok(vec![out]);
    }
    crate::compiled_layers::split_compiled_song_lua(out, tags, &paths, primary)
}

fn finalize_live_kinds<S, V>(
    runtime: &mut RustSongRuntime,
    rows: &mut SongOverlayActors<Kind<S, V>>,
    hidden: &mut [bool; LUA_PLAYERS],
) -> Result<(), String> {
    let mut indices = HashMap::new();
    if rows
        .actors
        .iter()
        .any(|r| matches!(r.actor.kind, Kind::ActorProxy { .. }))
    {
        for (i, row) in rows.actors.iter().enumerate() {
            let value = runtime
                .song_overlay_actor_value(row)
                .map_err(|e| e.to_string())?;
            indices.insert(
                runtime
                    .vm
                    .object_pointer(value)
                    .ok_or("overlay identity is missing")?,
                i,
            );
        }
    }
    for row in &mut rows.actors {
        let tag = match &row.actor.kind {
            Kind::Actor => 0,
            Kind::ActorProxy { .. } => 1,
            _ => continue,
        };
        let value = runtime
            .song_overlay_actor_value(row)
            .map_err(|e| e.to_string())?;
        match runtime
            .actors
            .compiler_read(&mut runtime.vm, value, Query::Late(tag))
            .map_err(|e| e.to_string())?
            .kind
        {
            Some(Late::Sprite(texture_path, states)) => {
                row.actor.kind = Kind::Sprite {
                    texture_key: crate::lua_util::texture_path_key(&texture_path),
                    texture_path,
                    states: states.into(),
                    textures: Arc::from([]),
                }
            }
            Some(Late::AftSprite(capture_name)) => {
                row.actor.kind = Kind::AftSprite { capture_name }
            }
            Some(Late::Proxy(mut target, player)) => {
                if let crate::SongLuaProxyTarget::Actor { overlay_index } = &mut target {
                    *overlay_index = indices.get(overlay_index).copied().unwrap_or(usize::MAX);
                }
                if let Some(p) = player.and_then(|p| hidden.get_mut(p)) {
                    *p = true;
                }
                row.actor.kind = Kind::ActorProxy { target };
            }
            _ => (),
        }
    }
    Ok(())
}
