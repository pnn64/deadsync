use super::*;
use deadsync_song_lua::{
    TopScreenLuaTables, actor_overlay_initial_state,
    add_actor_child_from_path as add_lua_actor_child_from_path, capture_actor_message_commands,
    capture_function_action_blocks as capture_lua_function_action_blocks,
    capture_overlay_function_eases as capture_lua_overlay_function_eases,
    collect_aft_capture_names, copy_dummy_actor_tags, create_dummy_actor as create_lua_dummy_actor,
    create_named_actor, create_note_field_actor, create_screen_timer_actor,
    create_top_screen_score_actor, create_top_screen_table as create_lua_top_screen_table,
    create_top_screen_theme_actor as create_lua_top_screen_theme_actor,
    create_underlay_theme_actor as create_lua_underlay_theme_actor, current_song_lua_style_name,
    function_action_plan, input_status_actor_text,
    install_actor_methods as install_lua_actor_methods, install_def_globals,
    install_file_loader_globals, message_command_lists_have_listener,
    note_field_column_actors as create_note_field_column_actors, player_child_proxy_name,
    push_unique_compile_detail, read_actor_color_field, read_actor_multi_vertex_mesh,
    read_actor_multi_vertex_texture_path, read_bitmap_font, read_bitmap_text_attributes,
    read_graph_display_body_state, read_graph_display_line_state, read_graph_display_size,
    read_graph_display_values, read_model_path, read_proxy_target_kind,
    read_song_meter_display_state, read_tracked_compile_actors as read_lua_tracked_compile_actors,
    read_update_function_tables, reset_indexed_actor_capture_tables, resolve_actor_asset_path,
    top_screen_score_index,
};

fn create_named_child_actor(lua: &Lua, parent: &Table, name: &str) -> mlua::Result<Table> {
    let parent_type = parent.get::<Option<String>>("__songlua_actor_type")?;
    let player_index = parent.get::<Option<i64>>("__songlua_player_index")?;
    let child = if parent_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("PlayerActor"))
        && let Some(player_index) = player_index
    {
        if name.eq_ignore_ascii_case("NoteField") {
            let style_name = parent
                .get::<Option<String>>("__songlua_style_name")?
                .unwrap_or_else(|| current_song_lua_style_name(lua));
            create_note_field_actor(lua, player_index as usize, &style_name, create_dummy_actor)?
        } else if player_child_proxy_name(name).is_some() {
            create_named_actor(lua, "Actor", name, create_dummy_actor)?
        } else {
            create_dummy_actor(lua, "ChildActor")?
        }
    } else if parent_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("TopScreen"))
        && name.eq_ignore_ascii_case("Timer")
    {
        create_screen_timer_actor(lua, create_dummy_actor)?
    } else if parent_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("TopScreen"))
        && let Some(player_index) = top_screen_score_index(name)
    {
        create_top_screen_score_actor(lua, player_index, create_dummy_actor)?
    } else if parent_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("TopScreen"))
        && let Some(child) = create_lua_top_screen_theme_actor(
            lua,
            parent,
            name,
            create_dummy_actor,
            create_named_child_actor,
        )?
    {
        child
    } else if parent
        .get::<Option<String>>("__songlua_top_screen_child_name")?
        .as_deref()
        .is_some_and(|child_name| child_name.eq_ignore_ascii_case("Underlay"))
        && let Some(child) = create_lua_underlay_theme_actor(lua, parent, name, create_dummy_actor)?
    {
        child
    } else {
        create_dummy_actor(lua, "ChildActor")?
    };
    copy_dummy_actor_tags(parent, &child)?;
    child.set("__songlua_parent", parent.clone())?;
    if parent_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("PlayerActor"))
    {
        child.set("__songlua_player_child_name", name)?;
    } else if parent_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("TopScreen"))
    {
        child.set("__songlua_top_screen_child_name", name)?;
    }
    Ok(child)
}

fn note_field_column_actors(lua: &Lua, note_field: &Table) -> mlua::Result<Table> {
    create_note_field_column_actors(lua, note_field, create_dummy_actor)
}

pub(super) fn create_top_screen_table(
    lua: &Lua,
    context: &SongLuaCompileContext,
    current_sort_order: Table,
    current_song: Table,
) -> mlua::Result<TopScreenLuaTables> {
    create_lua_top_screen_table(
        lua,
        context,
        current_sort_order,
        current_song,
        create_dummy_actor,
        create_named_child_actor,
    )
}

pub(super) fn install_def(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<()> {
    let human_player_count = song_lua_human_player_count(context);
    install_def_globals(
        lua,
        human_player_count,
        create_dummy_actor,
        install_actor_methods,
    )
}

pub(super) fn install_file_loaders(lua: &Lua, song_dir: PathBuf) -> mlua::Result<()> {
    install_file_loader_globals(lua, song_dir, create_dummy_actor)
}

pub(super) fn read_update_function_actions(
    lua: &Lua,
    root: &Value,
    overlays: &mut [OverlayCompileActor],
    tracked_actors: &mut [TrackedCompileActor],
    messages: &mut Vec<SongLuaMessageEvent>,
    counter: &mut usize,
    info: &mut SongLuaCompileInfo,
) -> Result<(), String> {
    for table in read_update_function_tables(lua, root, &["mod_actions", "actions"])? {
        read_actions(
            lua,
            Some(table),
            overlays,
            tracked_actors,
            messages,
            counter,
            info,
        )?;
    }
    Ok(())
}

pub(super) fn read_overlay_actors(
    lua: &Lua,
    root: &Value,
    context: &SongLuaCompileContext,
    info: &mut SongLuaCompileInfo,
) -> Result<Vec<OverlayCompileActor>, String> {
    let Value::Table(root) = root else {
        return Ok(Vec::new());
    };
    let mut aft_capture_names = HashSet::new();
    collect_aft_capture_names(root, &mut aft_capture_names)?;
    let mut out = Vec::new();
    read_overlay_actors_from_table(lua, root, None, &aft_capture_names, &mut out, context, info)?;
    Ok(out)
}

fn read_overlay_actors_from_table(
    lua: &Lua,
    actor: &Table,
    parent_index: Option<usize>,
    aft_capture_names: &HashSet<String>,
    out: &mut Vec<OverlayCompileActor>,
    context: &SongLuaCompileContext,
    info: &mut SongLuaCompileInfo,
) -> Result<(), String> {
    let next_parent_index = if let Some(overlay) =
        read_overlay_actor(lua, actor, parent_index, aft_capture_names, context, info)?
    {
        let index = out.len();
        out.push(overlay);
        Some(index)
    } else {
        parent_index
    };
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child.map_err(|err| err.to_string())? else {
            continue;
        };
        read_overlay_actors_from_table(
            lua,
            &child,
            next_parent_index,
            aft_capture_names,
            out,
            context,
            info,
        )?;
    }
    Ok(())
}

fn read_overlay_actor(
    lua: &Lua,
    actor: &Table,
    parent_index: Option<usize>,
    aft_capture_names: &HashSet<String>,
    context: &SongLuaCompileContext,
    info: &mut SongLuaCompileInfo,
) -> Result<Option<OverlayCompileActor>, String> {
    let Some(actor_type) = actor
        .get::<Option<String>>("__songlua_actor_type")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let initial_state = overlay_state_after_blocks(actor_overlay_initial_state(actor)?, &[], 0.0);
    let captured_commands = capture_actor_message_commands(lua, actor)?;
    for skipped in captured_commands.skipped {
        push_unique_compile_detail(&mut info.skipped_message_command_captures, skipped.clone());
        debug!("Skipping song lua overlay message capture for {}", skipped);
    }
    let message_commands = captured_commands.commands;
    let name = actor
        .get::<Option<String>>("Name")
        .map_err(|err| err.to_string())?;

    let kind = if actor_type.eq_ignore_ascii_case("Actor") {
        if name.is_none()
            && initial_state == SongLuaOverlayState::default()
            && message_commands.is_empty()
        {
            return Ok(None);
        }
        SongLuaOverlayKind::Actor
    } else if actor_type.eq_ignore_ascii_case("ActorFrame") {
        let has_draw_function = actor
            .get::<Option<Function>>("__songlua_draw_function")
            .map_err(|err| err.to_string())?
            .is_some();
        if parent_index.is_none()
            && name.is_none()
            && initial_state == SongLuaOverlayState::default()
            && message_commands.is_empty()
            && !has_draw_function
        {
            return Ok(None);
        }
        SongLuaOverlayKind::ActorFrame
    } else if actor_type.eq_ignore_ascii_case("ActorFrameTexture") {
        SongLuaOverlayKind::ActorFrameTexture
    } else if actor_type.eq_ignore_ascii_case("ActorProxy") {
        let Some(target) = read_proxy_target_kind(actor)? else {
            return Ok(None);
        };
        SongLuaOverlayKind::ActorProxy { target }
    } else if actor_type.eq_ignore_ascii_case("Sprite") {
        if let Some(capture_name) = actor
            .get::<Option<String>>("__songlua_aft_capture_name")
            .map_err(|err| err.to_string())?
            .filter(|name| !name.trim().is_empty())
        {
            SongLuaOverlayKind::AftSprite { capture_name }
        } else {
            let Some(texture) = actor
                .get::<Option<String>>("Texture")
                .map_err(|err| err.to_string())?
            else {
                return Ok(None);
            };
            if aft_capture_names.contains(&texture) {
                SongLuaOverlayKind::AftSprite {
                    capture_name: texture,
                }
            } else {
                let Some(texture_path) = resolve_actor_asset_path(actor, &texture).ok() else {
                    return Ok(None);
                };
                let texture_key = Arc::<str>::from(texture_path.to_string_lossy().into_owned());
                SongLuaOverlayKind::Sprite {
                    texture_path,
                    texture_key,
                }
            }
        }
    } else if actor_type.eq_ignore_ascii_case("Sound") {
        let Some(file) = actor
            .get::<Option<String>>("File")
            .map_err(|err| err.to_string())?
        else {
            return Ok(None);
        };
        let Ok(sound_path) = resolve_actor_asset_path(actor, &file) else {
            return Ok(None);
        };
        SongLuaOverlayKind::Sound { sound_path }
    } else if actor_type.eq_ignore_ascii_case("BitmapText")
        || actor_type.eq_ignore_ascii_case("RollingNumbers")
    {
        let Some((font_name, font_path)) = read_bitmap_font(actor)? else {
            return Ok(None);
        };
        SongLuaOverlayKind::BitmapText {
            font_name,
            font_path,
            text: Arc::<str>::from(
                actor
                    .get::<Option<String>>("Text")
                    .map_err(|err| err.to_string())?
                    .unwrap_or_default(),
            ),
            stroke_color: read_actor_color_field(actor, "__songlua_stroke_color")?
                .or_else(|| read_actor_color_field(actor, "StrokeColor").ok().flatten()),
            attributes: read_bitmap_text_attributes(actor)?,
        }
    } else if actor_type.eq_ignore_ascii_case("DeviceList")
        || actor_type.eq_ignore_ascii_case("InputList")
    {
        let Some((font_name, font_path)) = read_bitmap_font(actor)? else {
            return Ok(None);
        };
        SongLuaOverlayKind::BitmapText {
            font_name,
            font_path,
            text: Arc::<str>::from(input_status_actor_text(&actor_type).unwrap_or_default()),
            stroke_color: None,
            attributes: Arc::<[TextAttribute]>::from([]),
        }
    } else if actor_type.eq_ignore_ascii_case("ActorMultiVertex") {
        let Some(vertices) = read_actor_multi_vertex_mesh(actor)? else {
            return Ok(None);
        };
        let texture_path = read_actor_multi_vertex_texture_path(actor, aft_capture_names)?;
        let texture_key = texture_path
            .as_ref()
            .map(|path| Arc::<str>::from(path.to_string_lossy().into_owned()));
        SongLuaOverlayKind::ActorMultiVertex {
            vertices,
            texture_path,
            texture_key,
        }
    } else if actor_type.eq_ignore_ascii_case("Model") {
        if let Some(slots) = read_noteskin_tap_actor_slots(actor, context)? {
            SongLuaOverlayKind::NoteskinActor { slots }
        } else {
            let Some(layers) = read_model_layers(actor)? else {
                return Ok(None);
            };
            SongLuaOverlayKind::Model { layers }
        }
    } else if actor_type.eq_ignore_ascii_case("SongMeterDisplay") {
        let Some((stream_width, stream_state)) = read_song_meter_display_state(lua, actor)? else {
            return Ok(None);
        };
        SongLuaOverlayKind::SongMeterDisplay {
            stream_width,
            stream_state,
            music_length_seconds: context.music_length_seconds.max(0.0),
        }
    } else if actor_type.eq_ignore_ascii_case("CourseContentsList") {
        SongLuaOverlayKind::ActorFrame
    } else if actor_type.eq_ignore_ascii_case("GraphDisplay") {
        SongLuaOverlayKind::GraphDisplay {
            size: read_graph_display_size(initial_state, context),
            body_values: read_graph_display_values(actor)?,
            body_state: read_graph_display_body_state(lua, actor)?,
            line_state: read_graph_display_line_state(lua, actor)?,
        }
    } else if actor_type.eq_ignore_ascii_case("Quad") {
        SongLuaOverlayKind::Quad
    } else {
        return Ok(None);
    };
    Ok(Some(OverlayCompileActor {
        table: actor.clone(),
        actor: SongLuaOverlayActor {
            kind,
            name,
            parent_index,
            initial_state,
            message_commands,
        },
    }))
}

fn read_model_layers(actor: &Table) -> Result<Option<Arc<[SongLuaOverlayModelLayer]>>, String> {
    let Some(model_path) = read_model_path(actor)? else {
        return Ok(None);
    };
    let slots = crate::game::parsing::noteskin::load_itg_model_slots_from_path(&model_path)?;
    let mut layers = Vec::with_capacity(slots.len());
    for slot in slots.iter() {
        let Some(model) = slot.model.as_ref() else {
            continue;
        };
        if model.vertices.is_empty() {
            continue;
        }
        let uv_rect = slot.uv_for_frame_at(0, 0.0);
        let (uv_scale, uv_offset, uv_tex_shift) = slot.model_uv_params(uv_rect);
        layers.push(SongLuaOverlayModelLayer {
            texture_key: slot.texture_key_shared(),
            vertices: crate::game::parsing::noteskin::build_model_geometry(slot),
            model_size: model.size(),
            uv_scale,
            uv_offset,
            uv_tex_shift,
            uv_velocity: slot.uv_velocity,
            uv_cycle_seconds: slot.uv_cycle_seconds,
            draw: song_lua_model_draw(slot.model_draw_at(0.0, 0.0)),
        });
    }
    if layers.is_empty() {
        Ok(None)
    } else {
        Ok(Some(Arc::from(layers.into_boxed_slice())))
    }
}

fn read_noteskin_tap_actor_slots(
    actor: &Table,
    _context: &SongLuaCompileContext,
) -> Result<Option<Arc<[crate::game::parsing::noteskin::SpriteSlot]>>, String> {
    let Some(skin) = actor
        .get::<Option<String>>("__songlua_noteskin_name")
        .map_err(|err| err.to_string())?
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(None);
    };
    let Some(element) = actor
        .get::<Option<String>>("__songlua_noteskin_element")
        .map_err(|err| err.to_string())?
        .filter(|value| value.eq_ignore_ascii_case("Tap Note"))
    else {
        return Ok(None);
    };
    let Some(model_path) = read_model_path(actor)? else {
        return Ok(None);
    };
    crate::game::parsing::noteskin::load_itg_model_slots_from_path(&model_path)
        .map(Some)
        .map_err(|err| format!("failed to load noteskin actor '{skin} {element}': {err}"))
}

fn song_lua_model_draw(draw: deadsync_noteskin::ModelDrawState) -> SongLuaOverlayModelDraw {
    SongLuaOverlayModelDraw {
        pos: draw.pos,
        rot: draw.rot,
        zoom: draw.zoom,
        tint: draw.tint,
        vert_align: draw.vert_align,
        blend_add: draw.blend_add,
        visible: draw.visible,
    }
}

pub(super) fn create_dummy_actor(lua: &Lua, actor_type: &'static str) -> mlua::Result<Table> {
    create_lua_dummy_actor(lua, actor_type, install_actor_methods)
}

fn install_actor_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    install_lua_actor_methods(
        lua,
        actor,
        add_actor_child_from_path,
        note_field_column_actors,
        create_named_child_actor,
        create_dummy_actor,
    )
}

fn add_actor_child_from_path(lua: &Lua, actor: &Table, path: &str) -> mlua::Result<()> {
    add_lua_actor_child_from_path(lua, actor, path, create_dummy_actor)
}

pub(super) fn read_tracked_compile_actors(lua: &Lua) -> Result<Vec<TrackedCompileActor>, String> {
    read_lua_tracked_compile_actors(lua, create_named_child_actor)
}

pub(super) fn reset_overlay_capture_tables(
    lua: &Lua,
    overlays: &[OverlayCompileActor],
) -> Result<(), String> {
    let indices: Vec<_> = (0..overlays.len()).collect();
    let tables = overlay_capture_tables_for_indices(overlays, &indices);
    reset_indexed_actor_capture_tables(lua, &tables)
}

fn overlay_capture_tables_for_indices(
    overlays: &[OverlayCompileActor],
    indices: &[usize],
) -> Vec<(usize, Table)> {
    indices
        .iter()
        .filter_map(|&index| {
            overlays
                .get(index)
                .map(|overlay| (index, overlay.table.clone()))
        })
        .collect()
}

fn capture_function_action_blocks(
    lua: &Lua,
    overlays: &[OverlayCompileActor],
    tracked_actors: &[TrackedCompileActor],
    function: &Function,
    beat: f32,
) -> Result<
    (
        Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>,
        Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>,
        Vec<(String, bool)>,
        bool,
    ),
    String,
> {
    let indices: Vec<_> = (0..overlays.len()).collect();
    let overlay_tables = overlay_capture_tables_for_indices(overlays, &indices);
    let capture =
        capture_lua_function_action_blocks(lua, &overlay_tables, tracked_actors, function, beat)?;
    Ok((
        capture.overlay_blocks,
        capture.tracked_blocks,
        capture.broadcasts,
        capture.saw_side_effect,
    ))
}

pub(super) fn compile_function_action(
    lua: &Lua,
    overlays: &mut [OverlayCompileActor],
    tracked_actors: &mut [TrackedCompileActor],
    function: &Function,
    beat: f32,
    persists: bool,
    counter: &mut usize,
    messages: &mut Vec<SongLuaMessageEvent>,
) -> Result<bool, String> {
    let (overlay_captures, tracked_captures, broadcasts, saw_side_effect) =
        capture_function_action_blocks(lua, overlays, tracked_actors, function, beat)?;
    let plan = function_action_plan(
        beat,
        persists,
        *counter,
        overlay_captures,
        tracked_captures,
        broadcasts,
        saw_side_effect,
        |message| song_lua_message_has_listener(overlays, tracked_actors, message),
    );
    *counter = plan.next_counter;
    for (overlay_index, command) in plan.overlay_commands {
        overlays[overlay_index].actor.message_commands.push(command);
    }
    for (tracked_index, command) in plan.tracked_commands {
        tracked_actors[tracked_index]
            .actor
            .message_commands
            .push(command);
    }
    messages.extend(plan.messages);
    Ok(plan.handled)
}

fn song_lua_message_has_listener(
    overlays: &[OverlayCompileActor],
    tracked_actors: &[TrackedCompileActor],
    message: &str,
) -> bool {
    message_command_lists_have_listener(
        overlays
            .iter()
            .map(|overlay| overlay.actor.message_commands.as_slice())
            .chain(
                tracked_actors
                    .iter()
                    .map(|actor| actor.actor.message_commands.as_slice()),
            ),
        message,
    )
}

pub(super) fn compile_overlay_function_ease(
    lua: &Lua,
    overlays: &[OverlayCompileActor],
    function: &Function,
    unit: SongLuaTimeUnit,
    start: f32,
    limit: f32,
    span_mode: SongLuaSpanMode,
    from: f32,
    to: f32,
    easing: Option<String>,
    sustain: Option<f32>,
    opt1: Option<f32>,
    opt2: Option<f32>,
    probe_actor_ptrs: &[usize],
) -> Result<Vec<SongLuaOverlayEase>, String> {
    let indices: Vec<_> = (0..overlays.len()).collect();
    let overlay_tables = overlay_capture_tables_for_indices(overlays, &indices);
    capture_lua_overlay_function_eases(
        lua,
        &overlay_tables,
        function,
        unit,
        start,
        limit,
        span_mode,
        from,
        to,
        easing,
        sustain,
        opt1,
        opt2,
        probe_actor_ptrs,
    )
}
