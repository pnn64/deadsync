use super::*;
use deadsync_song_lua::{
    SONG_LUA_TOP_SCREEN_OPTION_ROWS, SongLuaColumnOffsetBuildParams, SongLuaColumnOffsetSample,
    TOP_SCREEN_THEME_CHILD_NAMES, UNDERLAY_THEME_CHILD_NAMES, actor_active_commands,
    actor_aft_capture_name, actor_child_at, actor_children, actor_command_queue,
    actor_current_capture_block, actor_debug_label, actor_decode_movie, actor_diffuse,
    actor_direct_children, actor_effect_magnitude, actor_glow, actor_halign, actor_named_children,
    actor_overlay_initial_state, actor_shadow_len, actor_texture_path, actor_tween_time_left,
    actor_type_is, actor_update_text_pre_zoom_flags, actor_valign, actor_wrappers, actor_zoom_axis,
    banner_sort_order_path, call_actor_function, call_with_chunk_env, call_with_script_dir,
    call_with_script_path, capture_actor_message_commands, capture_actor_text_attribute,
    capture_actor_vertex_diffuse, capture_block_set_bool, capture_block_set_color,
    capture_block_set_f32, capture_block_set_i32, capture_block_set_size,
    capture_block_set_stretch, capture_block_set_string, capture_block_set_u32,
    capture_block_set_vec2, capture_block_set_vec3, capture_block_set_vec4, capture_block_set_vec5,
    capture_block_set_zoom_axes,
    capture_function_action_blocks as capture_lua_function_action_blocks,
    capture_overlay_function_eases as capture_lua_overlay_function_eases, capture_texture_rect,
    collect_aft_capture_names, column_offset_windows_from_samples, copy_dummy_actor_tags,
    create_actor_child_group, crop_texture_rect, current_song_lua_style_name,
    drain_actor_command_queue, effect_clock_label, flush_actor_capture, format_rolling_number,
    function_action_plan, function_named_upvalue_tables, hurry_actor_tweening, inherit_actor_dirs,
    input_status_actor_text, install_actor_metatable, install_course_contents_list_children,
    make_actor_add_f32_method, make_actor_capture_f32_method, make_actor_chain_method,
    make_actor_finish_tweening_method, make_actor_set_size_method, make_actor_stop_tweening_method,
    make_actor_tween_method, make_actor_wrap_width_method, message_command_lists_have_listener,
    normalize_broadcast_params, note_column_pos_offset_y_from_points,
    note_column_zoom_hide_beats_per_t, note_hide_windows_from_flags, note_zoom_point_hides,
    offset_texture_rect as song_lua_offset_texture_rect, option_row_default_text,
    overlay_text_align_label, parse_sprite_sheet_dims, player_child_proxy_name,
    populate_course_contents_display, position_scroller_items, prepare_capture_scope_actor,
    push_sequence_child_once, push_unique_compile_detail, read_actor_color_field,
    read_actor_multi_vertex_mesh, read_actor_multi_vertex_texture_path, read_bitmap_font,
    read_bitmap_text_attributes, read_child_index, read_color_args, read_graph_display_body_state,
    read_graph_display_line_state, read_graph_display_size, read_graph_display_values,
    read_model_path, read_proxy_target_kind, read_song_meter_display_state,
    record_probe_method_call, register_loader_env, register_song_lua_actor, remove_actor_child,
    remove_all_actor_children, reset_actor_capture, reset_indexed_actor_capture_tables,
    resolve_actor_asset_path, resolve_load_actor_path, rolling_numbers_format,
    run_actor_named_command_with_drain_and_params, run_added_actor_child_commands,
    run_command_on_leaves, run_named_command_on_children_recursively, run_named_command_on_leaves,
    scale_to_rect_plan, set_actor_decode_movie_for_texture, set_actor_effect_defaults,
    set_actor_effect_mode, set_actor_sound_file_from_value, set_actor_sprite_state,
    set_actor_texture_from_path, set_actor_texture_from_path_methods,
    set_actor_texture_from_path_methods_or_fallback, set_actor_texture_from_value,
    set_proxy_target_fields, song_lua_actor_registry, song_lua_halign_value,
    song_lua_screen_center, song_lua_screen_size, song_lua_span_end, song_lua_text_align_value,
    song_lua_valid_sprite_state_index, song_lua_valign_value, sort_note_hide_windows,
    sprite_animation_state_at, sprite_frame_count, sprite_image_frame_size, sprite_texture_rect,
    table_vec2, table_vec4, text_glow_mode_label, texture_pixel_offset_rect,
    top_screen_danger_index, top_screen_life_meter_bar_index, top_screen_life_meter_index,
    top_screen_life_meter_name, top_screen_option_row_name, top_screen_player_index,
    top_screen_player_name, top_screen_score_index, top_screen_score_name,
    top_screen_score_percent_name, top_screen_song_meter_display_index,
    top_screen_step_stats_pane_index, top_screen_steps_display_index, underlay_score_index,
    underlay_score_name,
};

pub(super) struct TopScreenLuaTables {
    pub(super) top_screen: Table,
    pub(super) players: [Table; LUA_PLAYERS],
}

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
            create_note_field_actor(lua, player_index as usize, &style_name)?
        } else if player_child_proxy_name(name).is_some() {
            create_named_actor(lua, "Actor", name)?
        } else {
            create_dummy_actor(lua, "ChildActor")?
        }
    } else if parent_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("TopScreen"))
        && name.eq_ignore_ascii_case("Timer")
    {
        create_screen_timer_actor(lua)?
    } else if parent_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("TopScreen"))
        && let Some(player_index) = top_screen_score_index(name)
    {
        create_top_screen_score_actor(lua, player_index)?
    } else if parent_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("TopScreen"))
        && let Some(child) = create_top_screen_theme_actor(lua, parent, name)?
    {
        child
    } else if parent
        .get::<Option<String>>("__songlua_top_screen_child_name")?
        .as_deref()
        .is_some_and(|child_name| child_name.eq_ignore_ascii_case("Underlay"))
        && let Some(child) = create_underlay_theme_actor(lua, parent, name)?
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

fn can_create_named_child_actor(parent: &Table, name: &str) -> mlua::Result<bool> {
    let parent_type = parent.get::<Option<String>>("__songlua_actor_type")?;
    if parent_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("PlayerActor"))
        && parent
            .get::<Option<i64>>("__songlua_player_index")?
            .is_some()
    {
        return Ok(
            name.eq_ignore_ascii_case("NoteField") || player_child_proxy_name(name).is_some()
        );
    }
    if parent
        .get::<Option<String>>("__songlua_top_screen_child_name")?
        .as_deref()
        .is_some_and(|child_name| child_name.eq_ignore_ascii_case("Underlay"))
    {
        return Ok(underlay_score_index(name).is_some()
            || name.eq_ignore_ascii_case("SongMeter")
            || top_screen_step_stats_pane_index(name).is_some()
            || top_screen_danger_index(name).is_some()
            || name.eq_ignore_ascii_case("Header"));
    }
    Ok(false)
}

fn create_named_actor(lua: &Lua, actor_type: &'static str, name: &str) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, actor_type)?;
    actor.set("Name", name)?;
    Ok(actor)
}

fn create_named_text_actor(
    lua: &Lua,
    actor_type: &'static str,
    name: &str,
    text: String,
) -> mlua::Result<Table> {
    let actor = create_named_actor(lua, actor_type, name)?;
    actor.set("Text", text)?;
    Ok(actor)
}

fn create_top_screen_theme_actor(
    lua: &Lua,
    parent: &Table,
    name: &str,
) -> mlua::Result<Option<Table>> {
    if name.eq_ignore_ascii_case("Underlay") || name.eq_ignore_ascii_case("Overlay") {
        let actor = create_named_actor(lua, "ActorFrame", name)?;
        if name.eq_ignore_ascii_case("Underlay") {
            copy_dummy_actor_tags(parent, &actor)?;
            actor.set("__songlua_top_screen_child_name", "Underlay")?;
            install_underlay_theme_children(lua, &actor)?;
        }
        return Ok(Some(actor));
    }
    if name.eq_ignore_ascii_case("BPMDisplay") {
        return Ok(Some(create_named_text_actor(
            lua,
            "BPMDisplay",
            name,
            parent
                .get::<Option<String>>("__songlua_bpm_text")?
                .unwrap_or_default(),
        )?));
    }
    if name.eq_ignore_ascii_case("SongTitle") {
        return Ok(Some(create_named_text_actor(
            lua,
            "BitmapText",
            name,
            parent
                .get::<Option<String>>("__songlua_main_title")?
                .unwrap_or_default(),
        )?));
    }
    if name.eq_ignore_ascii_case("StageDisplay") {
        return Ok(Some(create_named_text_actor(
            lua,
            "BitmapText",
            name,
            "Stage 1".to_string(),
        )?));
    }
    if let Some(player_index) = top_screen_steps_display_index(name) {
        return Ok(Some(create_named_text_actor(
            lua,
            "StepsDisplay",
            name,
            top_screen_steps_text(parent, player_index)?,
        )?));
    }
    if top_screen_song_meter_display_index(name).is_some() {
        return Ok(Some(create_top_screen_song_meter_display_actor(lua, name)?));
    }
    if name.eq_ignore_ascii_case("LifeFrame")
        || name.eq_ignore_ascii_case("ScoreFrame")
        || name.eq_ignore_ascii_case("Lyrics")
        || name.eq_ignore_ascii_case("SongBackground")
        || name.eq_ignore_ascii_case("SongForeground")
    {
        return Ok(Some(create_named_actor(lua, "ActorFrame", name)?));
    }
    if top_screen_life_meter_bar_index(name).is_some() {
        return Ok(Some(create_named_actor(lua, "LifeMeterBar", name)?));
    }
    Ok(None)
}

fn create_underlay_theme_actor(
    lua: &Lua,
    parent: &Table,
    name: &str,
) -> mlua::Result<Option<Table>> {
    if let Some(player_index) = underlay_score_index(name) {
        return Ok(Some(create_named_text_actor(
            lua,
            "BitmapText",
            underlay_score_name(player_index),
            "0.00%".to_string(),
        )?));
    }
    if name.eq_ignore_ascii_case("SongMeter") {
        let actor = create_named_actor(lua, "ActorFrame", name)?;
        let title = create_named_text_actor(
            lua,
            "BitmapText",
            "SongTitle",
            parent
                .get::<Option<String>>("__songlua_main_title")?
                .unwrap_or_default(),
        )?;
        title.set("__songlua_parent", actor.clone())?;
        actor_children(lua, &actor)?.set("SongTitle", title)?;
        return Ok(Some(actor));
    }
    if top_screen_step_stats_pane_index(name).is_some() {
        return Ok(Some(create_named_actor(lua, "ActorFrame", name)?));
    }
    if top_screen_danger_index(name).is_some() || name.eq_ignore_ascii_case("Header") {
        return Ok(Some(create_named_actor(lua, "Sprite", name)?));
    }
    Ok(None)
}

fn create_top_screen_song_meter_display_actor(lua: &Lua, name: &str) -> mlua::Result<Table> {
    let actor = create_named_actor(lua, "SongMeterDisplay", name)?;
    actor.set("StreamWidth", 0.0_f32)?;
    actor.set("__songlua_stream_width", 0.0_f32)?;
    let stream = create_named_actor(lua, "Quad", "Stream")?;
    stream.set("__songlua_parent", actor.clone())?;
    actor_children(lua, &actor)?.set("Stream", stream)?;
    Ok(actor)
}

fn install_top_screen_theme_children(lua: &Lua, top_screen: &Table) -> mlua::Result<()> {
    install_named_theme_children(lua, top_screen, TOP_SCREEN_THEME_CHILD_NAMES)
}

fn install_underlay_theme_children(lua: &Lua, underlay: &Table) -> mlua::Result<()> {
    install_named_theme_children(lua, underlay, UNDERLAY_THEME_CHILD_NAMES)
}

fn install_named_theme_children(lua: &Lua, parent: &Table, names: &[&str]) -> mlua::Result<()> {
    let children = actor_children(lua, parent)?;
    for &name in names {
        if children.get::<Option<Table>>(name)?.is_some() {
            continue;
        }
        let child = create_named_child_actor(lua, parent, name)?;
        children.set(name, child)?;
    }
    Ok(())
}

fn create_screen_timer_actor(lua: &Lua) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "Timer")?;
    actor.set(
        "GetSeconds",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    Ok(actor)
}

fn create_top_screen_score_actor(lua: &Lua, player_index: usize) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "ActorFrame")?;
    actor.set("Name", top_screen_score_name(player_index))?;
    let percent = create_score_display_percent_actor(lua, player_index)?;
    percent.set("__songlua_parent", actor.clone())?;
    actor_children(lua, &actor)?.set("ScoreDisplayPercentage Percent", percent)?;
    Ok(actor)
}

fn create_score_display_percent_actor(lua: &Lua, player_index: usize) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "PercentageDisplay")?;
    actor.set("Name", "ScoreDisplayPercentage Percent")?;
    let text = create_score_percent_text_actor(lua, player_index)?;
    text.set("__songlua_parent", actor.clone())?;
    actor_children(lua, &actor)?.set(top_screen_score_percent_name(player_index), text)?;
    Ok(actor)
}

fn create_score_percent_text_actor(lua: &Lua, player_index: usize) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "BitmapText")?;
    actor.set("Name", top_screen_score_percent_name(player_index))?;
    actor.set("Text", "0.00%")?;
    Ok(actor)
}

fn create_life_meter_table(lua: &Lua, name: &'static str) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "LifeMeter")?;
    actor.set("Name", name)?;
    actor.set("__songlua_life", SONG_LUA_INITIAL_LIFE)?;
    actor.set(
        "GetLife",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_life")?
                    .unwrap_or(SONG_LUA_INITIAL_LIFE))
            }
        })?,
    )?;
    actor.set(
        "IsFailing",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_life")?
                    .unwrap_or(SONG_LUA_INITIAL_LIFE)
                    <= 0.0)
            }
        })?,
    )?;
    actor.set(
        "IsHot",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_life")?
                    .unwrap_or(SONG_LUA_INITIAL_LIFE)
                    >= 1.0)
            }
        })?,
    )?;
    actor.set(
        "IsInDanger",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_life")?
                    .unwrap_or(SONG_LUA_INITIAL_LIFE)
                    < SONG_LUA_DANGER_LIFE)
            }
        })?,
    )?;
    Ok(actor)
}

fn create_note_field_actor(
    lua: &Lua,
    player_index: usize,
    style_name: &str,
) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "NoteField")?;
    actor.set("__songlua_player_index", player_index as i64)?;
    actor.set("__songlua_style_name", style_name)?;
    actor.set("__songlua_player_child_name", "NoteField")?;
    actor.set("__songlua_beat_bars", false)?;
    actor.set("__songlua_beat_bars_alpha", lua.create_table()?)?;
    actor.set(
        "SetBeatBars",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0) {
                    actor.set("__songlua_beat_bars", truthy(value))?;
                    note_song_lua_side_effect(lua)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetBeatBarsAlpha",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let alpha = lua.create_table()?;
                for index in 0..4 {
                    let value = method_arg(&args, index)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0);
                    alpha.raw_set(index + 1, value)?;
                }
                actor.set("__songlua_beat_bars_alpha", alpha)?;
                note_song_lua_side_effect(lua)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set("__songlua_state_x", 0.0_f32)?;
    actor.set(
        "__songlua_state_y",
        0.5 * (THEME_RECEPTOR_Y_STD + THEME_RECEPTOR_Y_REV),
    )?;
    actor.set("__songlua_state_z", 0.0_f32)?;
    Ok(actor)
}

fn note_field_column_actors(lua: &Lua, note_field: &Table) -> mlua::Result<Table> {
    if let Some(columns) = note_field.get::<Option<Table>>("__songlua_note_columns")? {
        return Ok(columns);
    }
    let columns = lua.create_table()?;
    let player_index = note_field
        .get::<Option<i64>>("__songlua_player_index")?
        .unwrap_or(0) as usize;
    let style_name = note_field
        .get::<Option<String>>("__songlua_style_name")?
        .unwrap_or_else(|| current_song_lua_style_name(lua));
    for column_index in 0..song_lua_style_info(&style_name).columns {
        let column =
            create_note_column_actor(lua, note_field, player_index, column_index, &style_name)?;
        columns.raw_set(column_index + 1, column)?;
    }
    note_field.set("__songlua_note_columns", columns.clone())?;
    Ok(columns)
}

fn create_note_column_actor(
    lua: &Lua,
    note_field: &Table,
    player_index: usize,
    column_index: usize,
    style_name: &str,
) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "NoteColumnRenderer")?;
    actor.set("__songlua_parent", note_field.clone())?;
    actor.set("__songlua_player_index", player_index as i64)?;
    actor.set("__songlua_column_index", column_index as i64)?;
    actor.set(
        "__songlua_state_x",
        song_lua_style_column_x(style_name, column_index),
    )?;
    actor.set("__songlua_state_y", 0.0_f32)?;
    actor.set("__songlua_state_z", 0.0_f32)?;

    let pos_handler = create_note_column_spline_handler(lua)?;
    let rot_handler = create_note_column_spline_handler(lua)?;
    let zoom_handler = create_note_column_spline_handler(lua)?;
    actor.set("__songlua_pos_handler", pos_handler.clone())?;
    actor.set("__songlua_rot_handler", rot_handler.clone())?;
    actor.set("__songlua_zoom_handler", zoom_handler.clone())?;
    actor.set(
        "GetPosHandler",
        lua.create_function(move |_, _args: MultiValue| Ok(pos_handler.clone()))?,
    )?;
    actor.set(
        "GetRotHandler",
        lua.create_function(move |_, _args: MultiValue| Ok(rot_handler.clone()))?,
    )?;
    actor.set(
        "GetZoomHandler",
        lua.create_function(move |_, _args: MultiValue| Ok(zoom_handler.clone()))?,
    )?;
    Ok(actor)
}

fn create_note_column_spline_handler(lua: &Lua) -> mlua::Result<Table> {
    let handler = lua.create_table()?;
    let spline = create_cubic_spline_table(lua)?;
    handler.set("__songlua_spline", spline.clone())?;
    handler.set("__songlua_spline_mode", "NoteColumnSplineMode_Disabled")?;
    handler.set("__songlua_subtract_song_beat", true)?;
    handler.set("__songlua_receptor_t", 0.0_f32)?;
    handler.set("__songlua_beats_per_t", 1.0_f32)?;
    handler.set(
        "GetSpline",
        lua.create_function(move |_, _args: MultiValue| Ok(spline.clone()))?,
    )?;
    handler.set(
        "SetSplineMode",
        lua.create_function({
            let handler = handler.clone();
            move |_, args: MultiValue| {
                if let Some(mode) = args.get(1).cloned().and_then(read_string) {
                    handler.set("__songlua_spline_mode", mode)?;
                }
                Ok(handler.clone())
            }
        })?,
    )?;
    handler.set(
        "SetSubtractSongBeat",
        lua.create_function({
            let handler = handler.clone();
            move |_, args: MultiValue| {
                handler.set(
                    "__songlua_subtract_song_beat",
                    args.get(1).is_some_and(truthy),
                )?;
                Ok(handler.clone())
            }
        })?,
    )?;
    handler.set(
        "SetReceptorT",
        lua.create_function({
            let handler = handler.clone();
            move |_, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    handler.set("__songlua_receptor_t", value)?;
                }
                Ok(handler.clone())
            }
        })?,
    )?;
    handler.set(
        "SetBeatsPerT",
        lua.create_function({
            let handler = handler.clone();
            move |_, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    handler.set("__songlua_beats_per_t", value)?;
                }
                Ok(handler.clone())
            }
        })?,
    )?;
    Ok(handler)
}

fn create_cubic_spline_table(lua: &Lua) -> mlua::Result<Table> {
    let spline = lua.create_table()?;
    spline.set("__songlua_spline_size", 0_i64)?;
    spline.set("__songlua_spline_points", lua.create_table()?)?;
    spline.set(
        "SetSize",
        lua.create_function({
            let spline = spline.clone();
            move |_, args: MultiValue| {
                if let Some(size) = args.get(1).cloned().and_then(read_f32) {
                    spline.set("__songlua_spline_size", size.max(0.0).round() as i64)?;
                }
                Ok(spline.clone())
            }
        })?,
    )?;
    spline.set(
        "SetPoint",
        lua.create_function({
            let spline = spline.clone();
            move |lua, args: MultiValue| {
                let Some(index) = args
                    .get(1)
                    .cloned()
                    .and_then(read_f32)
                    .map(|value| value.max(1.0).round() as i64)
                else {
                    return Ok(spline.clone());
                };
                let points = spline.get::<Table>("__songlua_spline_points")?;
                match args.get(2) {
                    Some(Value::Table(point)) => {
                        points.raw_set(index, point.clone())?;
                    }
                    _ => {
                        let point = lua.create_table()?;
                        point.raw_set(1, 0.0_f32)?;
                        point.raw_set(2, 0.0_f32)?;
                        point.raw_set(3, 0.0_f32)?;
                        points.raw_set(index, point)?;
                    }
                }
                Ok(spline.clone())
            }
        })?,
    )?;
    spline.set(
        "Solve",
        lua.create_function({
            let spline = spline.clone();
            move |_, _args: MultiValue| Ok(spline.clone())
        })?,
    )?;
    spline.set(
        "SetPolygonal",
        lua.create_function({
            let spline = spline.clone();
            move |_, _args: MultiValue| Ok(spline.clone())
        })?,
    )?;
    Ok(spline)
}

pub(super) fn create_top_screen_table(
    lua: &Lua,
    context: &SongLuaCompileContext,
    current_sort_order: Table,
    current_song: Table,
) -> mlua::Result<TopScreenLuaTables> {
    let players = context.players.clone();
    let top_screen = create_dummy_actor(lua, "TopScreen")?;
    top_screen.set("Name", "ScreenGameplay")?;
    top_screen.set("__songlua_main_title", context.main_title.as_str())?;
    top_screen.set(
        "__songlua_bpm_text",
        display_bpms_text(context.song_display_bpms, song_music_rate(context)),
    )?;
    top_screen.set("__songlua_steps_text_1", players[0].difficulty.sm_name())?;
    top_screen.set("__songlua_steps_text_2", players[1].difficulty.sm_name())?;
    let top_screen_for_get_child = top_screen.clone();
    let life_meters = [
        create_life_meter_table(lua, "LifeP1")?,
        create_life_meter_table(lua, "LifeP2")?,
    ];
    let player_actors = [
        create_top_screen_player_actor(lua, players[0].clone(), 0, &context.style_name)?,
        create_top_screen_player_actor(lua, players[1].clone(), 1, &context.style_name)?,
    ];
    for player_actor in &player_actors {
        player_actor.set("__songlua_parent", top_screen.clone())?;
    }
    let nameless_children = lua.create_table()?;
    for (player_index, player_actor) in player_actors.iter().enumerate() {
        if players[player_index].enabled {
            nameless_children.raw_set(nameless_children.raw_len() + 1, player_actor.clone())?;
        }
    }
    for (player_index, life_meter) in life_meters.iter().enumerate() {
        life_meter.set("__songlua_parent", top_screen.clone())?;
        life_meter.set(
            "__songlua_top_screen_child_name",
            top_screen_life_meter_name(player_index),
        )?;
    }
    let children = actor_children(lua, &top_screen)?;
    children.set("", nameless_children)?;
    for (player_index, player_actor) in player_actors.iter().enumerate() {
        if players[player_index].enabled {
            children.set(top_screen_player_name(player_index), player_actor.clone())?;
        }
    }
    children.set("LifeP1", life_meters[0].clone())?;
    children.set("LifeP2", life_meters[1].clone())?;
    children.set("LifeMeter", life_meters[0].clone())?;
    install_top_screen_theme_children(lua, &top_screen)?;
    let player_actors_for_get_child = player_actors.clone();
    let life_meters_for_get_child = life_meters.clone();
    let players_for_get_child = players.clone();
    top_screen.set(
        "GetChild",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            if let Some(player_index) = top_screen_player_index(&name) {
                return if players_for_get_child[player_index].enabled {
                    Ok(Value::Table(
                        player_actors_for_get_child[player_index].clone(),
                    ))
                } else {
                    Ok(Value::Nil)
                };
            }
            if let Some(player_index) = top_screen_life_meter_index(&name) {
                return if players_for_get_child[player_index].enabled {
                    Ok(Value::Table(
                        life_meters_for_get_child[player_index].clone(),
                    ))
                } else {
                    Ok(Value::Nil)
                };
            }
            if name.eq_ignore_ascii_case("LifeMeter") {
                return Ok(Value::Table(life_meters_for_get_child[0].clone()));
            }
            let children = actor_children(lua, &top_screen_for_get_child)?;
            if let Some(child) = children.get::<Option<Table>>(name.as_str())? {
                return Ok(Value::Table(child));
            }
            let child = create_named_child_actor(lua, &top_screen_for_get_child, &name)?;
            children.set(name.as_str(), child.clone())?;
            Ok(Value::Table(child))
        })?,
    )?;
    let life_meters_for_get_life_meter = life_meters.clone();
    let players_for_get_life_meter = players.clone();
    top_screen.set(
        "GetLifeMeter",
        lua.create_function(move |_, args: MultiValue| {
            let Some(player_index) = method_arg(&args, 0).and_then(player_index_from_value) else {
                return Ok(Value::Nil);
            };
            if !players_for_get_life_meter[player_index].enabled {
                return Ok(Value::Nil);
            }
            Ok(Value::Table(
                life_meters_for_get_life_meter[player_index].clone(),
            ))
        })?,
    )?;
    top_screen.set(
        "StartTransitioningScreen",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(top_screen.clone())
            }
        })?,
    )?;
    top_screen.set(
        "SetNextScreenName",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, args: MultiValue| {
                let next_screen = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                top_screen.set("__songlua_next_screen_name", next_screen)?;
                note_song_lua_side_effect(lua)?;
                Ok(top_screen.clone())
            }
        })?,
    )?;
    top_screen.set(
        "SetPrevScreenName",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, args: MultiValue| {
                let prev_screen = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                top_screen.set("__songlua_prev_screen_name", prev_screen)?;
                note_song_lua_side_effect(lua)?;
                Ok(top_screen.clone())
            }
        })?,
    )?;
    top_screen.set(
        "GetNextScreenName",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, _args: MultiValue| {
                Ok(Value::String(
                    lua.create_string(
                        top_screen
                            .get::<Option<String>>("__songlua_next_screen_name")?
                            .unwrap_or_default(),
                    )?,
                ))
            }
        })?,
    )?;
    top_screen.set(
        "GetPrevScreenName",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, _args: MultiValue| {
                Ok(Value::String(
                    lua.create_string(
                        top_screen
                            .get::<Option<String>>("__songlua_prev_screen_name")?
                            .unwrap_or_default(),
                    )?,
                ))
            }
        })?,
    )?;
    top_screen.set(
        "GetGoToOptions",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    top_screen.set(
        "GetEditState",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string("EditState_Playing")?))
        })?,
    )?;
    top_screen.set(
        "GetCurrentRowIndex",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |_, _args: MultiValue| {
                Ok(top_screen
                    .get::<Option<i64>>("__songlua_current_row_index")?
                    .unwrap_or(0))
            }
        })?,
    )?;
    top_screen.set(
        "GetNumRows",
        lua.create_function(|_, _args: MultiValue| {
            Ok(SONG_LUA_TOP_SCREEN_OPTION_ROWS.len() as i64)
        })?,
    )?;
    top_screen.set(
        "GetOptionRow",
        lua.create_function(|lua, args: MultiValue| {
            let row_name = top_screen_option_row_name(method_arg(&args, 0).cloned());
            create_option_row_table(lua, &row_name)
        })?,
    )?;
    top_screen.set(
        "SetOptionRowIndex",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, args: MultiValue| {
                if let Some(index) = method_arg(&args, 1)
                    .or_else(|| method_arg(&args, 0))
                    .cloned()
                    .and_then(read_i32_value)
                {
                    top_screen.set("__songlua_current_row_index", i64::from(index.max(0)))?;
                }
                note_song_lua_side_effect(lua)?;
                Ok(top_screen.clone())
            }
        })?,
    )?;
    top_screen.set(
        "RedrawOptions",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(top_screen.clone())
            }
        })?,
    )?;
    top_screen.set(
        "IsPaused",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |_, _args: MultiValue| {
                Ok(top_screen
                    .get::<Option<bool>>("__songlua_paused")?
                    .unwrap_or(false))
            }
        })?,
    )?;
    top_screen.set(
        "PauseGame",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, args: MultiValue| {
                top_screen.set("__songlua_paused", method_arg(&args, 0).is_some_and(truthy))?;
                note_song_lua_side_effect(lua)?;
                Ok(top_screen.clone())
            }
        })?,
    )?;
    top_screen.set(
        "AllAreOnLastRow",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    let music_wheel = create_music_wheel_table(lua, current_sort_order)?;
    top_screen.set(
        "GetMusicWheel",
        lua.create_function(move |_, _args: MultiValue| Ok(music_wheel.clone()))?,
    )?;
    top_screen.set(
        "GetNextCourseSong",
        lua.create_function(move |_, _args: MultiValue| {
            Ok(current_song
                .raw_get::<Option<Table>>(1)?
                .map_or(Value::Nil, Value::Table))
        })?,
    )?;
    for name in [
        "AddInputCallback",
        "RemoveInputCallback",
        "PostScreenMessage",
        "SetProfileIndex",
        "Cancel",
        "Continue",
        "Finish",
        "Load",
        "begin_backing_out",
    ] {
        top_screen.set(
            name,
            lua.create_function({
                let top_screen = top_screen.clone();
                move |lua, _args: MultiValue| {
                    note_song_lua_side_effect(lua)?;
                    Ok(top_screen.clone())
                }
            })?,
        )?;
    }
    Ok(TopScreenLuaTables {
        top_screen,
        players: player_actors,
    })
}

fn create_music_wheel_table(lua: &Lua, current_sort_order: Table) -> mlua::Result<Table> {
    let wheel = lua.create_table()?;
    wheel.set(
        "IsLocked",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    wheel.set(
        "GetSelectedSection",
        lua.create_function(|lua, _args: MultiValue| Ok(Value::String(lua.create_string("")?)))?,
    )?;
    wheel.set(
        "GetSelectedSong",
        lua.create_function(|_, _args: MultiValue| Ok(Value::Nil))?,
    )?;
    wheel.set(
        "GetSelectedType",
        lua.create_function(|lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string("WheelItemDataType_Song")?))
        })?,
    )?;
    wheel.set(
        "GetSortOrder",
        lua.create_function({
            let current_sort_order = current_sort_order.clone();
            move |lua, _args: MultiValue| {
                let sort = current_sort_order
                    .raw_get::<Option<String>>(1)?
                    .unwrap_or_else(|| "SortOrder_Group".to_string());
                Ok(Value::String(lua.create_string(&sort)?))
            }
        })?,
    )?;
    wheel.set(
        "ChangeSort",
        lua.create_function({
            let wheel = wheel.clone();
            let current_sort_order = current_sort_order.clone();
            move |lua, args: MultiValue| {
                if let Some(sort) = method_arg(&args, 0).cloned().and_then(read_string) {
                    current_sort_order.raw_set(1, sort)?;
                }
                note_song_lua_side_effect(lua)?;
                Ok(wheel.clone())
            }
        })?,
    )?;
    wheel.set(
        "SetOpenSection",
        lua.create_function({
            let wheel = wheel.clone();
            move |lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(wheel.clone())
            }
        })?,
    )?;
    wheel.set(
        "Move",
        lua.create_function({
            let wheel = wheel.clone();
            move |lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(wheel.clone())
            }
        })?,
    )?;
    Ok(wheel)
}

fn create_option_row_table(lua: &Lua, name: &str) -> mlua::Result<Table> {
    let row = create_dummy_actor(lua, "OptionRow")?;
    row.set("Name", name)?;
    set_string_method(lua, &row, "GetName", name)?;
    let frame = create_option_row_frame(lua, name)?;
    frame.set("__songlua_parent", row.clone())?;
    actor_children(lua, &row)?.set("", frame)?;
    row.set(
        "GetChoiceInRowWithFocus",
        lua.create_function(|_, _args: MultiValue| Ok(1_i64))?,
    )?;
    Ok(row)
}

fn create_option_row_frame(lua: &Lua, row_name: &str) -> mlua::Result<Table> {
    let frame = create_dummy_actor(lua, "OptionRowFrame")?;
    let title = create_option_row_text_actor(lua, "Title", row_name)?;
    title.set("__songlua_parent", frame.clone())?;
    actor_children(lua, &frame)?.set("Title", title)?;

    let items = create_actor_child_group(lua)?;
    let item_text = option_row_default_text(row_name);
    for index in 1..=LUA_PLAYERS {
        let item = create_option_row_text_actor(lua, "Item", &item_text)?;
        item.set("__songlua_parent", frame.clone())?;
        items.raw_set(index, item)?;
    }
    actor_children(lua, &frame)?.set("Item", items)?;
    Ok(frame)
}

fn create_option_row_text_actor(lua: &Lua, name: &str, text: &str) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "BitmapText")?;
    actor.set("Name", name)?;
    actor.set("Text", text)?;
    Ok(actor)
}

fn create_top_screen_player_actor(
    lua: &Lua,
    player: SongLuaPlayerContext,
    player_index: usize,
    style_name: &str,
) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "PlayerActor")?;
    actor.set("Name", top_screen_player_name(player_index))?;
    actor.set("__songlua_player_index", player_index as i64)?;
    actor.set("__songlua_style_name", style_name)?;
    actor.set("__songlua_visible", true)?;
    actor.set("__songlua_state_x", player.screen_x)?;
    actor.set("__songlua_state_y", player.screen_y)?;
    Ok(actor)
}

fn top_screen_steps_text(parent: &Table, player_index: usize) -> mlua::Result<String> {
    parent
        .get::<Option<String>>(match player_index {
            0 => "__songlua_steps_text_1",
            1 => "__songlua_steps_text_2",
            _ => "",
        })?
        .map_or_else(|| Ok(SongLuaDifficulty::Medium.sm_name().to_string()), Ok)
}

pub(super) fn install_def(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<()> {
    let globals = lua.globals();
    let def = lua.create_table()?;
    let human_player_count = song_lua_human_player_count(context);
    for &(name, actor_type) in &[
        ("Actor", "Actor"),
        ("ActorFrame", "ActorFrame"),
        ("Sprite", "Sprite"),
        ("Banner", "Sprite"),
        ("ActorMultiVertex", "ActorMultiVertex"),
        ("Sound", "Sound"),
        ("BitmapText", "BitmapText"),
        ("RollingNumbers", "RollingNumbers"),
        ("GraphDisplay", "GraphDisplay"),
        ("SongMeterDisplay", "SongMeterDisplay"),
        ("CourseContentsList", "CourseContentsList"),
        ("DeviceList", "DeviceList"),
        ("InputList", "InputList"),
        ("Model", "Model"),
        ("Quad", "Quad"),
        ("ActorProxy", "ActorProxy"),
        ("ActorFrameTexture", "ActorFrameTexture"),
    ] {
        def.set(name, make_actor_ctor(lua, actor_type, human_player_count)?)?;
    }
    globals.set("Def", def)?;
    globals.set("ActorFrame", create_actorframe_class_table(lua)?)?;
    globals.set("Sprite", create_sprite_class_table(lua)?)?;
    globals.set(
        "LoadFont",
        lua.create_function(|lua, args: MultiValue| {
            let font = args
                .front()
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let actor = create_dummy_actor(lua, "BitmapText")?;
            actor.set("Font", font)?;
            Ok(actor)
        })?,
    )?;
    Ok(())
}

fn create_sprite_class_table(lua: &Lua) -> mlua::Result<Table> {
    let class = lua.create_table()?;
    for method_name in [
        "Load",
        "LoadBanner",
        "LoadBackground",
        "LoadFromCached",
        "LoadFromCachedBanner",
        "LoadFromCachedBackground",
        "LoadFromCachedJacket",
        "LoadFromSong",
        "LoadFromCourse",
        "LoadFromSongBackground",
        "LoadFromSongGroup",
        "LoadFromSortOrder",
        "LoadIconFromCharacter",
        "LoadCardFromCharacter",
        "LoadBannerFromUnlockEntry",
        "LoadBackgroundFromUnlockEntry",
        "SetScrolling",
        "GetScrolling",
        "GetPercentScrolling",
        "GetState",
        "SetStateProperties",
        "GetAnimationLengthSeconds",
        "SetSecondsIntoAnimation",
        "SetEffectMode",
        "position",
    ] {
        set_actor_class_forwarder(lua, &class, method_name)?;
    }
    Ok(class)
}

fn set_actor_class_forwarder(
    lua: &Lua,
    class: &Table,
    method_name: &'static str,
) -> mlua::Result<()> {
    class.set(
        method_name,
        lua.create_function(move |_, args: MultiValue| {
            let Some(Value::Table(actor)) = args.front() else {
                return Ok(Value::Nil);
            };
            match actor.get::<Value>(method_name)? {
                Value::Function(method) => method.call::<Value>(args),
                _ => Ok(Value::Nil),
            }
        })?,
    )
}

fn create_actorframe_class_table(lua: &Lua) -> mlua::Result<Table> {
    let class = lua.create_table()?;
    for method_name in [
        "playcommandonchildren",
        "playcommandonleaves",
        "runcommandsonleaves",
        "RunCommandsOnChildren",
        "propagate",
        "fov",
        "SetUpdateRate",
        "GetUpdateRate",
        "SetFOV",
        "vanishpoint",
        "GetChild",
        "GetChildAt",
        "GetChildren",
        "GetNumChildren",
        "SetDrawByZPosition",
        "SetDrawFunction",
        "GetDrawFunction",
        "SetUpdateFunction",
        "SortByDrawOrder",
        "SetAmbientLightColor",
        "SetDiffuseLightColor",
        "SetSpecularLightColor",
        "SetLightDirection",
        "AddChildFromPath",
        "RemoveChild",
        "RemoveAllChildren",
    ] {
        set_actor_class_forwarder(lua, &class, method_name)?;
    }
    class.set(
        "fardistz",
        lua.create_function(|_, args: MultiValue| {
            let Some(actor) = args.front().and_then(|value| match value {
                Value::Table(table) => Some(table.clone()),
                _ => None,
            }) else {
                return Ok(Value::Nil);
            };
            let Value::Function(method) = actor.get::<Value>("fardistz")? else {
                return Ok(Value::Nil);
            };
            let _ = method.call::<Value>(args)?;
            Ok(Value::Table(actor))
        })?,
    )?;
    Ok(class)
}

fn install_graph_display_children(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let body_group = create_actor_child_group(lua)?;
    for index in 1..=2 {
        let child = create_dummy_actor(lua, "GraphDisplayBody")?;
        child.set("__songlua_parent", actor.clone())?;
        body_group.raw_set(index, child)?;
    }
    let line = create_dummy_actor(lua, "GraphDisplayLine")?;
    line.set("Name", "Line")?;
    line.set("__songlua_parent", actor.clone())?;
    let children = actor_children(lua, actor)?;
    children.set("", body_group)?;
    children.set("Line", line)?;
    Ok(())
}

fn capture_graph_display_values(
    lua: &Lua,
    actor: &Table,
    stage_stats: Option<Value>,
    player_stats: Option<Value>,
) -> mlua::Result<()> {
    let total_seconds = stage_stats
        .and_then(|value| match value {
            Value::Table(table) => table
                .get::<Option<f32>>("__songlua_total_steps_seconds")
                .ok()
                .flatten(),
            _ => None,
        })
        .unwrap_or(0.0);
    let values = player_stats
        .and_then(|value| match value {
            Value::Table(table) => {
                let method = table
                    .get::<Option<Function>>("GetLifeRecord")
                    .ok()
                    .flatten()?;
                method
                    .call::<Value>((table, total_seconds, GRAPH_DISPLAY_VALUE_RESOLUTION as i64))
                    .ok()
            }
            _ => None,
        })
        .and_then(|value| match value {
            Value::Table(table) => Some(table),
            _ => None,
        })
        .unwrap_or(create_life_record_table(
            lua,
            GRAPH_DISPLAY_VALUE_RESOLUTION,
            SONG_LUA_INITIAL_LIFE,
        )?);
    actor.set("__songlua_graph_display_values", values)?;
    Ok(())
}

fn install_song_meter_display_children(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let width = actor
        .get::<Option<Value>>("StreamWidth")?
        .and_then(read_f32)
        .unwrap_or(0.0);
    actor.set("__songlua_stream_width", width)?;
    if let Some(stream) = actor.get::<Option<Table>>("Stream")? {
        if stream.get::<Option<String>>("Name")?.is_none() {
            stream.set("Name", "Stream")?;
        }
        stream.set("__songlua_parent", actor.clone())?;
        actor_children(lua, actor)?.set("Stream", stream)?;
    }
    Ok(())
}

fn make_actor_ctor(
    lua: &Lua,
    actor_type: &'static str,
    human_player_count: usize,
) -> mlua::Result<Function> {
    lua.create_function(move |lua, value: Value| {
        let table = match value {
            Value::Table(table) => table,
            _ => lua.create_table()?,
        };
        table.set("__songlua_actor_type", actor_type)?;
        if let Some(script_dir) = lua
            .globals()
            .get::<Option<String>>("__songlua_script_dir")?
        {
            table.set("__songlua_script_dir", script_dir)?;
        }
        if let Some(song_dir) = lua.globals().get::<Option<String>>("__songlua_song_dir")? {
            table.set("__songlua_song_dir", song_dir)?;
        }
        set_actor_decode_movie_for_texture(&table)?;
        install_actor_methods(lua, &table)?;
        install_actor_metatable(lua, &table)?;
        reset_actor_capture(lua, &table)?;
        register_song_lua_actor(lua, &table)?;
        if actor_type.eq_ignore_ascii_case("GraphDisplay") {
            let [width, height] = graph_display_body_size(human_player_count);
            let size = lua.create_table()?;
            size.raw_set(1, width)?;
            size.raw_set(2, height)?;
            table.set("__songlua_state_size", size)?;
            install_graph_display_children(lua, &table)?;
        }
        if actor_type.eq_ignore_ascii_case("SongMeterDisplay") {
            install_song_meter_display_children(lua, &table)?;
        }
        if actor_type.eq_ignore_ascii_case("CourseContentsList") {
            install_course_contents_list_children(&table)?;
        }
        if let Some(text) = input_status_actor_text(actor_type) {
            table.set("Text", text)?;
        }
        Ok(table)
    })
}

pub(super) fn install_file_loaders(lua: &Lua, song_dir: PathBuf) -> mlua::Result<()> {
    let globals = lua.globals();
    let song_dir_for_loadfile = song_dir.clone();
    globals.set(
        "loadfile",
        lua.create_function(move |lua, path: String| {
            let mut out = MultiValue::new();
            match create_loader_function(lua, &song_dir_for_loadfile, &path) {
                Ok(loader) => {
                    out.push_back(Value::Function(loader));
                    out.push_back(Value::Nil);
                }
                Err(err) => {
                    out.push_back(Value::Nil);
                    out.push_back(Value::String(lua.create_string(err.to_string())?));
                }
            }
            Ok(out)
        })?,
    )?;
    let song_dir_for_dofile = song_dir.clone();
    globals.set(
        "dofile",
        lua.create_function(move |lua, path: String| {
            let loader = create_loader_function(lua, &song_dir_for_dofile, &path)?;
            loader.call::<Value>(())
        })?,
    )?;
    globals.set(
        "LoadActor",
        lua.create_function(move |lua, value: Value| match value {
            Value::String(path) => {
                let path = path.to_str()?.to_string();
                match load_actor_path(lua, &song_dir, &path)? {
                    Value::Table(table) => Ok(table),
                    _ => create_dummy_actor(lua, "LoadActor"),
                }
            }
            _ => create_dummy_actor(lua, "LoadActor"),
        })?,
    )?;
    Ok(())
}

fn load_actor_path(lua: &Lua, song_dir: &Path, path: &str) -> mlua::Result<Value> {
    if let Some(actor) = create_theme_path_actor(lua, path)? {
        return Ok(Value::Table(actor));
    }
    let resolved = resolve_load_actor_path(lua, song_dir, path)?;
    if is_song_lua_media_path(&resolved) {
        let actor_type = if is_song_lua_audio_path(&resolved) {
            "Sound"
        } else {
            "Sprite"
        };
        return Ok(Value::Table(create_media_actor(
            lua,
            actor_type,
            path,
            resolved.as_path(),
        )?));
    }
    load_script_file(lua, &resolved, song_dir)?.call::<Value>(())
}

fn create_theme_path_actor(lua: &Lua, path: &str) -> mlua::Result<Option<Table>> {
    let theme_path = path.trim_start_matches('/');
    if !theme_path.starts_with(SONG_LUA_THEME_PATH_PREFIX) {
        return Ok(None);
    }
    let path_ref = Path::new(theme_path);
    let actor_type = if is_song_lua_audio_path(path_ref) {
        "Sound"
    } else if is_song_lua_image_path(path_ref) || is_song_lua_video_path(path_ref) {
        "Sprite"
    } else {
        "Actor"
    };
    let actor = create_dummy_actor(lua, actor_type)?;
    if actor_type.eq_ignore_ascii_case("Sound") {
        actor.set("File", path)?;
    } else if actor_type.eq_ignore_ascii_case("Sprite") {
        actor.set("Texture", path)?;
    }
    Ok(Some(actor))
}

fn create_loader_function(lua: &Lua, song_dir: &Path, path: &str) -> mlua::Result<Function> {
    let path = path.trim();
    let basename = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path);
    if basename.eq_ignore_ascii_case("easing.lua") {
        let ease: Table = lua.globals().get("ease")?;
        return lua.create_function(move |_, _args: MultiValue| Ok(Value::Table(ease.clone())));
    }
    let resolved = resolve_script_path(lua, song_dir, path)?;
    load_script_file(lua, &resolved, song_dir)
}

fn load_script_file(lua: &Lua, path: &Path, song_dir: &Path) -> mlua::Result<Function> {
    let source = fs::read_to_string(path).map_err(mlua::Error::external)?;
    let source = preprocess_lua_cmd_syntax(&source).map_err(mlua::Error::external)?;
    let chunk_env = create_chunk_env_proxy(lua, initial_chunk_environment(lua, path)?)?;
    let chunk = lua
        .load(&source)
        .set_name(path.to_string_lossy().as_ref())
        .set_environment(chunk_env.clone());
    let inner = chunk.into_function()?;
    let script_dir = path.parent().unwrap_or(song_dir).to_path_buf();
    let script_path = file_path_string(path);
    let chunk_env_for_call = chunk_env.clone();
    let wrapper = lua.create_function(move |lua, args: MultiValue| {
        call_with_script_dir(lua, &script_dir, || {
            call_with_script_path(lua, &script_path, || {
                call_with_chunk_env(lua, &chunk_env_for_call, || {
                    inner.call::<Value>(args.clone())
                })
            })
        })
    })?;
    register_loader_env(lua, &wrapper, &chunk_env)?;
    Ok(wrapper)
}

pub(super) fn execute_script_file(lua: &Lua, path: &Path, song_dir: &Path) -> mlua::Result<Value> {
    let loader = load_script_file(lua, path, song_dir)?;
    loader.call::<Value>(())
}

pub(super) fn run_actor_draw_functions(lua: &Lua, root: &Value) {
    let Value::Table(root) = root else {
        return;
    };
    if let Err(err) = run_actor_draw_functions_for_table(lua, root) {
        debug!("Skipping song lua draw function capture: {err}");
    }
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
    let Value::Table(root) = root else {
        return Ok(());
    };
    let globals = lua.globals();
    let Some(debug) = globals
        .get::<Option<Table>>("debug")
        .map_err(|err| err.to_string())?
    else {
        return Ok(());
    };
    let Some(getupvalue) = debug
        .get::<Option<Function>>("getupvalue")
        .map_err(|err| err.to_string())?
    else {
        return Ok(());
    };
    let mut seen_tables = HashSet::new();
    read_update_function_actions_for_table(
        lua,
        root,
        &getupvalue,
        overlays,
        tracked_actors,
        messages,
        counter,
        &mut seen_tables,
        info,
    )
}

pub(super) fn read_note_column_zoom_hides(lua: &Lua) -> Result<Vec<SongLuaNoteHideWindow>, String> {
    let globals = lua.globals();
    let mut out = Vec::new();
    for player in 0..LUA_PLAYERS {
        let key = if player == 0 {
            "__songlua_top_screen_player_1"
        } else {
            "__songlua_top_screen_player_2"
        };
        let Some(player_actor) = globals
            .get::<Option<Table>>(key)
            .map_err(|err| err.to_string())?
        else {
            continue;
        };
        let Some(note_field) = actor_named_children(lua, &player_actor)
            .map_err(|err| err.to_string())?
            .get::<Option<Table>>("NoteField")
            .map_err(|err| err.to_string())?
        else {
            continue;
        };
        let Some(columns) = note_field
            .get::<Option<Table>>("__songlua_note_columns")
            .map_err(|err| err.to_string())?
        else {
            continue;
        };
        for column in columns.sequence_values::<Table>() {
            let column = column.map_err(|err| err.to_string())?;
            let Some(local_col) = column
                .get::<Option<i64>>("__songlua_column_index")
                .map_err(|err| err.to_string())?
                .and_then(|value| usize::try_from(value).ok())
            else {
                continue;
            };
            read_note_column_zoom_hides_for(player, local_col, &column, &mut out)?;
        }
    }
    sort_note_hide_windows(&mut out);
    Ok(out)
}

fn read_note_column_zoom_hides_for(
    player: usize,
    column: usize,
    actor: &Table,
    out: &mut Vec<SongLuaNoteHideWindow>,
) -> Result<(), String> {
    let Some(handler) = actor
        .get::<Option<Table>>("__songlua_zoom_handler")
        .map_err(|err| err.to_string())?
    else {
        return Ok(());
    };
    let mode = handler
        .get::<Option<String>>("__songlua_spline_mode")
        .map_err(|err| err.to_string())?
        .unwrap_or_default();
    let subtract_song_beat = handler
        .get::<Option<bool>>("__songlua_subtract_song_beat")
        .map_err(|err| err.to_string())?
        .unwrap_or(false);
    let beats_per_t = handler
        .get::<Option<f32>>("__songlua_beats_per_t")
        .map_err(|err| err.to_string())?
        .unwrap_or(1.0);
    let Some(beats_per_t) =
        note_column_zoom_hide_beats_per_t(&mode, subtract_song_beat, beats_per_t)
    else {
        return Ok(());
    };
    let Some(spline) = handler
        .get::<Option<Table>>("__songlua_spline")
        .map_err(|err| err.to_string())?
    else {
        return Ok(());
    };
    let size = spline
        .get::<Option<i64>>("__songlua_spline_size")
        .map_err(|err| err.to_string())?
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0);
    let points = spline
        .get::<Table>("__songlua_spline_points")
        .map_err(|err| err.to_string())?;
    let mut hidden = Vec::with_capacity(size);
    for index in 1..=size {
        hidden.push(
            points
                .raw_get::<Option<Table>>(index)
                .map_err(|err| err.to_string())?
                .is_some_and(|point| note_zoom_point_hides(&point)),
        );
    }
    out.extend(note_hide_windows_from_flags(
        player,
        column,
        beats_per_t,
        &hidden,
    ));
    Ok(())
}

struct NoteColumnHandlerSnapshot {
    handler: Table,
    spline: Table,
    mode: Value,
    subtract_song_beat: Value,
    receptor_t: Value,
    beats_per_t: Value,
    spline_size: Value,
    spline_points: Value,
}

struct NoteFieldColumnSnapshot {
    player_actor: Table,
    note_field: Option<Table>,
    columns: Option<Table>,
    handlers: Vec<NoteColumnHandlerSnapshot>,
}

pub(super) fn compile_note_column_pos_function_ease(
    lua: &Lua,
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
) -> Result<Vec<SongLuaColumnOffsetWindow>, String> {
    let start_beat = start;
    let end_beat = song_lua_span_end(start, limit, span_mode).max(start_beat);
    let from_samples = capture_note_column_pos_samples(lua, function, from, start_beat)?;
    let to_samples = capture_note_column_pos_samples(lua, function, to, end_beat)?;
    if from_samples.is_empty() && to_samples.is_empty() {
        return Ok(Vec::new());
    }
    Ok(column_offset_windows_from_samples(
        &from_samples,
        &to_samples,
        SongLuaColumnOffsetBuildParams {
            unit,
            start,
            limit,
            span_mode,
            easing,
            sustain,
            opt1,
            opt2,
        },
    ))
}

fn capture_note_column_pos_samples(
    lua: &Lua,
    function: &Function,
    value: f32,
    song_beat: f32,
) -> Result<Vec<SongLuaColumnOffsetSample>, String> {
    let previous = compile_song_runtime_values(lua).map_err(|err| err.to_string())?;
    set_compile_song_runtime_beat(lua, song_beat).map_err(|err| err.to_string())?;
    let snapshot = snapshot_note_field_columns(lua).map_err(|err| err.to_string())?;
    let result = function.call::<Value>(value);
    let samples = read_note_column_pos_samples(lua);
    restore_note_field_columns(lua, snapshot).map_err(|err| err.to_string())?;
    set_compile_song_runtime_values(lua, previous.0, previous.1).map_err(|err| err.to_string())?;
    result.map_err(|err| err.to_string())?;
    samples
}

fn snapshot_note_field_columns(lua: &Lua) -> mlua::Result<Vec<NoteFieldColumnSnapshot>> {
    let mut out = Vec::new();
    let globals = lua.globals();
    for player in 0..LUA_PLAYERS {
        let key = if player == 0 {
            "__songlua_top_screen_player_1"
        } else {
            "__songlua_top_screen_player_2"
        };
        let Some(player_actor) = globals.get::<Option<Table>>(key)? else {
            continue;
        };
        let note_field =
            actor_named_children(lua, &player_actor)?.get::<Option<Table>>("NoteField")?;
        let columns = match &note_field {
            Some(note_field) => note_field.get::<Option<Table>>("__songlua_note_columns")?,
            None => None,
        };
        let handlers = match &columns {
            Some(columns) => snapshot_note_column_handlers(lua, columns)?,
            None => Vec::new(),
        };
        out.push(NoteFieldColumnSnapshot {
            player_actor,
            note_field,
            columns,
            handlers,
        });
    }
    Ok(out)
}

fn snapshot_note_column_handlers(
    lua: &Lua,
    columns: &Table,
) -> mlua::Result<Vec<NoteColumnHandlerSnapshot>> {
    let mut out = Vec::new();
    for column in columns.sequence_values::<Table>() {
        let column = column?;
        for key in [
            "__songlua_pos_handler",
            "__songlua_rot_handler",
            "__songlua_zoom_handler",
        ] {
            let Some(handler) = column.get::<Option<Table>>(key)? else {
                continue;
            };
            let Some(spline) = handler.get::<Option<Table>>("__songlua_spline")? else {
                continue;
            };
            out.push(NoteColumnHandlerSnapshot {
                mode: clone_lua_value(lua, handler.get::<Value>("__songlua_spline_mode")?)?,
                subtract_song_beat: clone_lua_value(
                    lua,
                    handler.get::<Value>("__songlua_subtract_song_beat")?,
                )?,
                receptor_t: clone_lua_value(lua, handler.get::<Value>("__songlua_receptor_t")?)?,
                beats_per_t: clone_lua_value(lua, handler.get::<Value>("__songlua_beats_per_t")?)?,
                spline_size: clone_lua_value(lua, spline.get::<Value>("__songlua_spline_size")?)?,
                spline_points: clone_lua_value(
                    lua,
                    spline.get::<Value>("__songlua_spline_points")?,
                )?,
                handler,
                spline,
            });
        }
    }
    Ok(out)
}

fn restore_note_field_columns(
    lua: &Lua,
    snapshots: Vec<NoteFieldColumnSnapshot>,
) -> mlua::Result<()> {
    for snapshot in snapshots {
        match snapshot.note_field {
            Some(note_field) => {
                actor_children(lua, &snapshot.player_actor)?
                    .set("NoteField", note_field.clone())?;
                match snapshot.columns {
                    Some(columns) => note_field.set("__songlua_note_columns", columns.clone())?,
                    None => note_field.set("__songlua_note_columns", Value::Nil)?,
                }
            }
            None => {
                actor_children(lua, &snapshot.player_actor)?.set("NoteField", Value::Nil)?;
            }
        }
        for handler in snapshot.handlers {
            handler.handler.set("__songlua_spline_mode", handler.mode)?;
            handler
                .handler
                .set("__songlua_subtract_song_beat", handler.subtract_song_beat)?;
            handler
                .handler
                .set("__songlua_receptor_t", handler.receptor_t)?;
            handler
                .handler
                .set("__songlua_beats_per_t", handler.beats_per_t)?;
            handler
                .spline
                .set("__songlua_spline_size", handler.spline_size)?;
            handler
                .spline
                .set("__songlua_spline_points", handler.spline_points)?;
        }
    }
    Ok(())
}

fn note_field_tables(lua: &Lua) -> mlua::Result<Vec<Table>> {
    let globals = lua.globals();
    let mut out = Vec::new();
    for player in 0..LUA_PLAYERS {
        let key = if player == 0 {
            "__songlua_top_screen_player_1"
        } else {
            "__songlua_top_screen_player_2"
        };
        let Some(player_actor) = globals.get::<Option<Table>>(key)? else {
            continue;
        };
        let Some(note_field) =
            actor_named_children(lua, &player_actor)?.get::<Option<Table>>("NoteField")?
        else {
            continue;
        };
        out.push(note_field);
    }
    Ok(out)
}

fn read_note_column_pos_samples(lua: &Lua) -> Result<Vec<SongLuaColumnOffsetSample>, String> {
    let mut out = Vec::new();
    for note_field in note_field_tables(lua).map_err(|err| err.to_string())? {
        let Some(columns) = note_field
            .get::<Option<Table>>("__songlua_note_columns")
            .map_err(|err| err.to_string())?
        else {
            continue;
        };
        for column in columns.sequence_values::<Table>() {
            let column = column.map_err(|err| err.to_string())?;
            let Some(player) = column
                .get::<Option<i64>>("__songlua_player_index")
                .map_err(|err| err.to_string())?
                .and_then(|value| usize::try_from(value).ok())
            else {
                continue;
            };
            let Some(local_col) = column
                .get::<Option<i64>>("__songlua_column_index")
                .map_err(|err| err.to_string())?
                .and_then(|value| usize::try_from(value).ok())
            else {
                continue;
            };
            let Some(y) = note_column_pos_offset_y(&column)? else {
                continue;
            };
            out.push(SongLuaColumnOffsetSample {
                player,
                column: local_col,
                y,
            });
        }
    }
    Ok(out)
}

fn note_column_pos_offset_y(column: &Table) -> Result<Option<f32>, String> {
    let Some(handler) = column
        .get::<Option<Table>>("__songlua_pos_handler")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let mode = handler
        .get::<Option<String>>("__songlua_spline_mode")
        .map_err(|err| err.to_string())?
        .unwrap_or_default();
    if mode.eq_ignore_ascii_case("NoteColumnSplineMode_Disabled") {
        return Ok(note_column_pos_offset_y_from_points(&mode, &[]));
    }
    if !mode.eq_ignore_ascii_case("NoteColumnSplineMode_Offset") {
        return Ok(None);
    }
    let Some(spline) = handler
        .get::<Option<Table>>("__songlua_spline")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let size = spline
        .get::<Option<i64>>("__songlua_spline_size")
        .map_err(|err| err.to_string())?
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0);
    if size == 0 {
        return Ok(note_column_pos_offset_y_from_points(&mode, &[]));
    }
    let points = spline
        .get::<Table>("__songlua_spline_points")
        .map_err(|err| err.to_string())?;
    let mut points_out = Vec::with_capacity(size);
    for index in 1..=size {
        let Some(point) = points
            .raw_get::<Option<Table>>(index)
            .map_err(|err| err.to_string())?
        else {
            return Ok(None);
        };
        let x = point.raw_get::<Value>(1).ok().and_then(read_f32);
        let point_y = point.raw_get::<Value>(2).ok().and_then(read_f32);
        let (Some(x), Some(point_y)) = (x, point_y) else {
            return Ok(None);
        };
        points_out.push([x, point_y]);
    }
    Ok(note_column_pos_offset_y_from_points(&mode, &points_out))
}

fn run_actor_draw_functions_for_table(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if let Some(draw) = actor.get::<Option<Function>>("__songlua_draw_function")? {
        let draw_result = call_actor_function(lua, actor, &draw, None);
        let drain_result = drain_actor_command_queue(lua, actor);
        if let Err(err) = draw_result {
            debug!(
                "Skipping song lua draw capture for {}: {}",
                actor_debug_label(actor),
                err
            );
        }
        if let Err(err) = drain_result {
            debug!(
                "Skipping queued song lua draw commands for {}: {}",
                actor_debug_label(actor),
                err
            );
        }
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child? else {
            continue;
        };
        child.set("__songlua_parent", actor.clone())?;
        run_actor_draw_functions_for_table(lua, &child)?;
    }
    Ok(())
}

fn read_update_function_actions_for_table(
    lua: &Lua,
    actor: &Table,
    getupvalue: &Function,
    overlays: &mut [OverlayCompileActor],
    tracked_actors: &mut [TrackedCompileActor],
    messages: &mut Vec<SongLuaMessageEvent>,
    counter: &mut usize,
    seen_tables: &mut HashSet<usize>,
    info: &mut SongLuaCompileInfo,
) -> Result<(), String> {
    if let Some(update) = actor
        .get::<Option<Function>>("__songlua_update_function")
        .map_err(|err| err.to_string())?
    {
        for table in function_named_upvalue_tables(
            getupvalue,
            &update,
            &["mod_actions", "actions"],
            seen_tables,
        )? {
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
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child.map_err(|err| err.to_string())? else {
            continue;
        };
        read_update_function_actions_for_table(
            lua,
            &child,
            getupvalue,
            overlays,
            tracked_actors,
            messages,
            counter,
            seen_tables,
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

fn actor_sprite_sheet_dims(actor: &Table) -> mlua::Result<Option<(u32, u32)>> {
    if let Some(path) = actor_texture_path(actor)? {
        return Ok(Some(parse_sprite_sheet_dims(
            path.to_string_lossy().as_ref(),
        )));
    }
    let Some(texture) = actor.get::<Option<String>>("Texture")? else {
        return Ok(None);
    };
    let texture = texture.trim();
    if texture.is_empty() {
        return Ok(None);
    }
    Ok(Some(parse_sprite_sheet_dims(texture)))
}

fn actor_texture_rect(actor: &Table) -> mlua::Result<[f32; 4]> {
    let custom_rect = actor
        .get::<Option<Table>>("__songlua_state_custom_texture_rect")?
        .and_then(|value| table_vec4(&value));
    let state_index = actor.get::<Option<u32>>("__songlua_state_sprite_state_index")?;
    Ok(sprite_texture_rect(
        custom_rect,
        state_index,
        actor_sprite_sheet_dims(actor)?,
    ))
}

fn offset_texture_rect(lua: &Lua, actor: &Table, dx: f32, dy: f32) -> mlua::Result<()> {
    let rect = song_lua_offset_texture_rect(actor_texture_rect(actor)?, [dx, dy]);
    capture_texture_rect(lua, actor, rect)
}

fn scale_actor_to_rect(lua: &Lua, actor: &Table, rect: [f32; 4], cover: bool) -> mlua::Result<()> {
    let (base_width, base_height) = actor_base_size(actor)?;
    let align = [actor_halign(actor)?, actor_valign(actor)?];
    let Some(plan) = scale_to_rect_plan(rect, [base_width, base_height], align, cover) else {
        return Ok(());
    };
    capture_block_set_f32(lua, actor, "x", plan.pos[0])?;
    capture_block_set_f32(lua, actor, "y", plan.pos[1])?;
    capture_block_set_f32(lua, actor, "zoom", plan.zoom)?;
    capture_block_set_zoom_axes(lua, actor, plan.zoom, "zoom_x", "zoom_y", "zoom_z")?;
    actor_update_text_pre_zoom_flags(lua, actor, true, true)?;
    if plan.flip_x {
        capture_block_set_f32(lua, actor, "rot_y_deg", 180.0)?;
    }
    if plan.flip_y {
        capture_block_set_f32(lua, actor, "rot_x_deg", 180.0)?;
    }
    Ok(())
}

fn set_actor_seconds_into_animation(lua: &Lua, actor: &Table, seconds: f32) -> mlua::Result<()> {
    let delay = actor
        .get::<Option<f32>>("__songlua_state_sprite_state_delay")?
        .unwrap_or(0.1);
    let frame_count = actor_sprite_frame_count(actor)?;
    let state = sprite_animation_state_at(seconds, delay, frame_count);
    set_actor_sprite_state(lua, actor, state)
}

fn actor_sprite_frame_count(actor: &Table) -> mlua::Result<u32> {
    Ok(sprite_frame_count(actor_sprite_sheet_dims(actor)?))
}

pub(super) fn create_dummy_actor(lua: &Lua, actor_type: &'static str) -> mlua::Result<Table> {
    let actor = lua.create_table()?;
    actor.set("__songlua_actor_type", actor_type)?;
    inherit_actor_dirs(lua, &actor)?;
    install_actor_methods(lua, &actor)?;
    install_actor_metatable(lua, &actor)?;
    reset_actor_capture(lua, &actor)?;
    register_song_lua_actor(lua, &actor)?;
    Ok(actor)
}

fn create_media_actor(
    lua: &Lua,
    actor_type: &'static str,
    path: &str,
    resolved_path: &Path,
) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, actor_type)?;
    if actor_type.eq_ignore_ascii_case("Sound") {
        actor.set("File", path)?;
    } else {
        actor.set("Texture", file_path_string(resolved_path))?;
        set_actor_decode_movie_for_texture(&actor)?;
    }
    Ok(actor)
}

pub(super) fn broadcast_song_lua_message(
    lua: &Lua,
    message: &str,
    params: Option<Value>,
) -> mlua::Result<()> {
    if message.trim().is_empty() {
        return Ok(());
    }
    let command = format!("{message}MessageCommand");
    let registry = song_lua_actor_registry(lua)?;
    let mut actors = Vec::with_capacity(registry.raw_len());
    for value in registry.sequence_values::<Value>() {
        let Value::Table(actor) = value? else {
            continue;
        };
        actors.push(actor);
    }
    let params = normalize_broadcast_params(lua, message, params)?;
    for actor in actors {
        run_actor_named_command_with_drain_and_params(lua, &actor, &command, true, params.clone())?;
    }
    Ok(())
}

fn install_actor_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "SetTarget",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(Value::Table(target)) = args.get(1) {
                    set_proxy_target_fields(&actor, target)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "visible",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).map(truthy) {
                    prepare_capture_scope_actor(lua, &actor)?;
                    actor.set("__songlua_visible", value)?;
                    capture_block_set_bool(lua, &actor, "visible", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "addcommand",
        lua.create_function({
            let actor = actor.clone();
            move |_, (_self, name, function): (Table, String, Function)| {
                actor.set(format!("{name}Command"), function)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "removecommand",
        lua.create_function({
            let actor = actor.clone();
            move |_, (_self, name): (Table, String)| {
                actor.set(format!("{name}Command"), Value::Nil)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetCommand",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(Value::Nil);
                };
                Ok(actor
                    .get::<Option<Function>>(format!("{name}Command"))?
                    .map_or(Value::Nil, Value::Function))
            }
        })?,
    )?;
    actor.set(
        "GetTweenTimeLeft",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| actor_tween_time_left(&actor)
        })?,
    )?;
    actor.set(
        "sleep",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                prepare_capture_scope_actor(lua, &actor)?;
                flush_actor_capture(&actor)?;
                let duration = args
                    .get(1)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0)
                    .max(0.0);
                let cursor = actor
                    .get::<Option<f32>>("__songlua_capture_cursor")?
                    .unwrap_or(0.0);
                actor.set("__songlua_capture_cursor", cursor + duration)?;
                actor.set("__songlua_capture_tween_time_left", cursor + duration)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "hibernate",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                prepare_capture_scope_actor(lua, &actor)?;
                flush_actor_capture(&actor)?;
                let duration = args
                    .get(1)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0)
                    .max(0.0);
                if duration <= f32::EPSILON {
                    return Ok(actor.clone());
                }
                let restore_visible = actor
                    .get::<Option<bool>>("__songlua_visible")?
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "visible", false)?;
                flush_actor_capture(&actor)?;
                let cursor = actor
                    .get::<Option<f32>>("__songlua_capture_cursor")?
                    .unwrap_or(0.0);
                actor.set("__songlua_capture_cursor", cursor + duration)?;
                actor.set("__songlua_capture_tween_time_left", cursor + duration)?;
                capture_block_set_bool(lua, &actor, "visible", restore_visible)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "linear",
        make_actor_tween_method(lua, actor, Some("linear"))?,
    )?;
    actor.set(
        "accelerate",
        make_actor_tween_method(lua, actor, Some("inQuad"))?,
    )?;
    actor.set(
        "decelerate",
        make_actor_tween_method(lua, actor, Some("outQuad"))?,
    )?;
    actor.set(
        "smooth",
        make_actor_tween_method(lua, actor, Some("inOutQuad"))?,
    )?;
    actor.set(
        "spring",
        make_actor_tween_method(lua, actor, Some("outElastic"))?,
    )?;
    actor.set(
        "bouncebegin",
        make_actor_tween_method(lua, actor, Some("inBounce"))?,
    )?;
    actor.set(
        "bounceend",
        make_actor_tween_method(lua, actor, Some("outBounce"))?,
    )?;
    actor.set(
        "queuecommand",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = args.get(1).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                let active = actor_active_commands(lua, &actor)?;
                if active
                    .get::<Option<bool>>(format!("{name}Command"))?
                    .unwrap_or(false)
                {
                    return Ok(actor.clone());
                }
                let queue = actor_command_queue(lua, &actor)?;
                queue.raw_set(queue.raw_len() + 1, name)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "playcommand",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                let params = method_arg(&args, 1).cloned();
                let command_name = format!("{name}Command");
                if actor
                    .get::<Option<bool>>("__songlua_propagate_commands")?
                    .unwrap_or(false)
                {
                    run_named_command_on_children_recursively(lua, &actor, &command_name, params)?;
                } else {
                    run_actor_named_command_with_drain_and_params(
                        lua,
                        &actor,
                        &command_name,
                        true,
                        params,
                    )?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "propagate",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                actor.set(
                    "__songlua_propagate_commands",
                    method_arg(&args, 0).is_some_and(truthy),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "propagatecommand",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                let command = format!("{name}Command");
                run_named_command_on_children_recursively(
                    lua,
                    &actor,
                    &command,
                    method_arg(&args, 1).cloned(),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "playcommandonchildren",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                let params = method_arg(&args, 1).cloned();
                let command = format!("{name}Command");
                for child in actor_direct_children(lua, &actor)? {
                    run_actor_named_command_with_drain_and_params(
                        lua,
                        &child,
                        &command,
                        true,
                        params.clone(),
                    )?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "playcommandonleaves",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                let params = method_arg(&args, 1).cloned();
                run_named_command_on_leaves(lua, &actor, &format!("{name}Command"), params)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "RemoveChild",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                remove_actor_child(lua, &actor, &name)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "RemoveAllChildren",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                remove_all_actor_children(lua, &actor)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetTexture",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                set_actor_texture_from_value(&actor, method_arg(&args, 0), false)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    for name in ["Load", "LoadBanner", "LoadBackground"] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                let method_name = name;
                move |_, args: MultiValue| {
                    if method_name == "Load" && actor_type_is(&actor, "RollingNumbers")? {
                        if let Some(metric) = method_arg(&args, 0).cloned().and_then(read_string) {
                            set_rolling_numbers_metric(&actor, &metric)?;
                        }
                        return Ok(actor.clone());
                    }
                    if method_name == "Load" && actor_type_is(&actor, "GraphDisplay")? {
                        if let Some(metric) = method_arg(&args, 0).cloned().and_then(read_string) {
                            actor.set("__songlua_graph_display_metric", metric)?;
                        }
                        return Ok(actor.clone());
                    }
                    if method_name == "Load" && actor_type_is(&actor, "Sound")? {
                        set_actor_sound_file_from_value(&actor, method_arg(&args, 0), true)?;
                        return Ok(actor.clone());
                    }
                    set_actor_texture_from_value(&actor, method_arg(&args, 0), true)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "LoadFromCached",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let path = method_arg(&args, 1)
                    .or_else(|| method_arg(&args, 0))
                    .filter(|value| !matches!(value, Value::Nil));
                set_actor_texture_from_value(&actor, path, true)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    for name in [
        "LoadFromCachedBanner",
        "LoadFromCachedBackground",
        "LoadFromCachedJacket",
    ] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |_, args: MultiValue| {
                    set_actor_texture_from_value(&actor, method_arg(&args, 0), true)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    for (name, path_methods) in [
        ("LoadFromSong", &["GetBannerPath"][..]),
        (
            "LoadFromCourse",
            &["GetBannerPath", "GetBackgroundPath"][..],
        ),
        ("LoadFromSongBackground", &["GetBackgroundPath"][..]),
    ] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |_, args: MultiValue| {
                    set_actor_texture_from_path_methods(
                        &actor,
                        method_arg(&args, 0),
                        path_methods,
                    )?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "LoadFromSongGroup",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let group = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                let Some(songman) = lua.globals().get::<Option<Table>>("SONGMAN")? else {
                    return Ok(actor.clone());
                };
                let Some(method) = songman.get::<Option<Function>>("GetSongGroupBannerPath")?
                else {
                    return Ok(actor.clone());
                };
                if let Value::String(path) = method.call::<Value>((songman, group))? {
                    if !set_actor_texture_from_path(&actor, &path.to_str()?)? {
                        set_actor_texture_from_path(
                            &actor,
                            &theme_path("G", "Banner", "group fallback"),
                        )?;
                    }
                } else {
                    set_actor_texture_from_path(
                        &actor,
                        &theme_path("G", "Banner", "group fallback"),
                    )?;
                }
                actor.set("__songlua_state_banner_scrolling", false)?;
                actor.set("__songlua_state_banner_scroll_percent", 0.0_f32)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "LoadFromSortOrder",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let sort_order = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_else(|| "SortOrder_Invalid".to_string());
                if let Some(path) = banner_sort_order_path(&sort_order) {
                    set_actor_texture_from_path(&actor, &path)?;
                }
                actor.set("__songlua_state_banner_scrolling", false)?;
                actor.set("__songlua_state_banner_scroll_percent", 0.0_f32)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    const CHARACTER_ICON_PATH_METHODS: &[&str] = &["GetIconPath"];
    const CHARACTER_CARD_PATH_METHODS: &[&str] = &["GetCardPath"];
    const UNLOCK_BANNER_PATH_METHODS: &[&str] = &["GetBannerFile"];
    const UNLOCK_BACKGROUND_PATH_METHODS: &[&str] = &["GetBackgroundFile"];
    for (name, path_methods, fallback_path) in [
        (
            "LoadIconFromCharacter",
            CHARACTER_ICON_PATH_METHODS,
            theme_path("G", "Common", "fallback banner"),
        ),
        (
            "LoadCardFromCharacter",
            CHARACTER_CARD_PATH_METHODS,
            theme_path("G", "Common", "fallback banner"),
        ),
        (
            "LoadBannerFromUnlockEntry",
            UNLOCK_BANNER_PATH_METHODS,
            theme_path("G", "Common", "fallback banner"),
        ),
        (
            "LoadBackgroundFromUnlockEntry",
            UNLOCK_BACKGROUND_PATH_METHODS,
            theme_path("G", "Common", "fallback banner"),
        ),
    ] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |_, args: MultiValue| {
                    set_actor_texture_from_path_methods_or_fallback(
                        &actor,
                        method_arg(&args, 0),
                        path_methods,
                        &fallback_path,
                    )?;
                    actor.set("__songlua_state_banner_scrolling", false)?;
                    actor.set("__songlua_state_banner_scroll_percent", 0.0_f32)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "SetScrolling",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let scrolling = method_arg(&args, 0).is_some_and(truthy);
                let percent = method_arg(&args, 1)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0);
                actor.set("__songlua_state_banner_scrolling", scrolling)?;
                actor.set("__songlua_state_banner_scroll_percent", percent)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetScrolling",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<bool>>("__songlua_state_banner_scrolling")?
                    .unwrap_or(false))
            }
        })?,
    )?;
    actor.set(
        "GetPercentScrolling",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_banner_scroll_percent")?
                    .unwrap_or(0.0))
            }
        })?,
    )?;
    actor.set(
        "SetTextureName",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(name) = args.get(1).cloned().and_then(read_string) {
                    actor.set("__songlua_aft_capture_name", name)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetTexture",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _self: Table| match actor.get::<Value>("Texture")? {
                Value::Nil
                    if !actor
                        .get::<Option<String>>("__songlua_actor_type")?
                        .as_deref()
                        .is_some_and(|kind| kind.eq_ignore_ascii_case("ActorFrameTexture")) =>
                {
                    Ok(Value::Nil)
                }
                _ => Ok(Value::Table(create_texture_proxy(lua, &actor)?)),
            }
        })?,
    )?;
    actor.set(
        "x",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                record_probe_method_call(lua, &actor, "x")?;
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "x", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "y",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                record_probe_method_call(lua, &actor, "y")?;
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "y", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "fov",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "fov", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetFOV",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "fov", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetUpdateRate",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    if value.is_finite() && value > 0.0 {
                        actor.set("__songlua_update_rate", value)?;
                    }
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetUpdateRate",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_update_rate")?
                    .unwrap_or(1.0))
            }
        })?,
    )?;
    actor.set(
        "vanishpoint",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(x) = args.get(1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let Some(y) = args.get(2).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                capture_block_set_vec2(lua, &actor, "vanishpoint", [x, y])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "xy",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(x) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "x", x)?;
                }
                if let Some(y) = args.get(2).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "y", y)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "halign",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).and_then(song_lua_halign_value) {
                    capture_block_set_f32(lua, &actor, "halign", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "valign",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).and_then(song_lua_valign_value) {
                    capture_block_set_f32(lua, &actor, "valign", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "align",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).and_then(song_lua_halign_value) {
                    capture_block_set_f32(lua, &actor, "halign", value)?;
                }
                if let Some(value) = method_arg(&args, 1).and_then(song_lua_valign_value) {
                    capture_block_set_f32(lua, &actor, "valign", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "vertalign",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).and_then(song_lua_valign_value) {
                    capture_block_set_f32(lua, &actor, "valign", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "horizalign",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(raw) = method_arg(&args, 0) else {
                    return Ok(actor.clone());
                };
                if let Some(value) = song_lua_halign_value(raw) {
                    capture_block_set_f32(lua, &actor, "halign", value)?;
                }
                if let Some(value) = song_lua_text_align_value(raw) {
                    capture_block_set_string(
                        lua,
                        &actor,
                        "text_align",
                        overlay_text_align_label(value),
                    )?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "Center",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let (center_x, center_y) = song_lua_screen_center(lua)?;
                capture_block_set_f32(lua, &actor, "x", center_x)?;
                capture_block_set_f32(lua, &actor, "y", center_y)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "CenterX",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let (center_x, _) = song_lua_screen_center(lua)?;
                capture_block_set_f32(lua, &actor, "x", center_x)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "CenterY",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let (_, center_y) = song_lua_screen_center(lua)?;
                capture_block_set_f32(lua, &actor, "y", center_y)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "stretchto",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let rect = [
                    args.get(1).cloned().and_then(read_f32).unwrap_or(0.0),
                    args.get(2).cloned().and_then(read_f32).unwrap_or(0.0),
                    args.get(3).cloned().and_then(read_f32).unwrap_or(0.0),
                    args.get(4).cloned().and_then(read_f32).unwrap_or(0.0),
                ];
                capture_block_set_stretch(lua, &actor, rect)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "FullScreen",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let (width, height) = song_lua_screen_size(lua)?;
                capture_block_set_stretch(lua, &actor, [0.0, 0.0, width, height])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "cropright",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "cropright", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "cropleft",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "cropleft", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "diffusealpha",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(alpha) = args.get(1).cloned().and_then(read_f32) {
                    let block = actor_current_capture_block(lua, &actor)?;
                    let mut diffuse = block
                        .get::<Option<Table>>("diffuse")?
                        .and_then(|value| table_vec4(&value))
                        .unwrap_or(actor_diffuse(&actor)?);
                    diffuse[3] = alpha;
                    capture_block_set_color(lua, &actor, diffuse)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "basezoom",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "basezoom", value)?;
                    capture_block_set_zoom_axes(
                        lua,
                        &actor,
                        value,
                        "basezoom_x",
                        "basezoom_y",
                        "basezoom_z",
                    )?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "basezoomx",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "basezoom_x", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "basezoomy",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "basezoom_y", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "basezoomz",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "basezoom_z", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "zoom",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                record_probe_method_call(lua, &actor, "zoom")?;
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "zoom", value)?;
                    capture_block_set_zoom_axes(lua, &actor, value, "zoom_x", "zoom_y", "zoom_z")?;
                    actor_update_text_pre_zoom_flags(lua, &actor, true, true)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetUpdateFunction",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                match args.get(1).cloned() {
                    Some(Value::Function(function)) => {
                        actor.set("__songlua_update_function", function)?;
                    }
                    _ => {
                        actor.set("__songlua_update_function", Value::Nil)?;
                    }
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "aux",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_aux", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "getaux",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor.get::<Option<f32>>("__songlua_aux")?.unwrap_or(0.0))
            }
        })?,
    )?;
    actor.set(
        "draworder",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).cloned().and_then(read_i32_value) {
                    capture_block_set_i32(lua, &actor, "draw_order", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetDrawFunction",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(Value::Function(function)) = args.get(1).cloned() {
                    actor.set("__songlua_draw_function", function)?;
                } else {
                    actor.set("__songlua_draw_function", Value::Nil)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetDrawFunction",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<Function>>("__songlua_draw_function")?
                    .map_or(Value::Nil, Value::Function))
            }
        })?,
    )?;
    actor.set(
        "Set",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if actor_type_is(&actor, "GraphDisplay")? {
                    actor.set("__songlua_graph_display_set", true)?;
                    let stage_stats = method_arg(&args, 0).cloned();
                    let player_stats = method_arg(&args, 1).cloned();
                    if let Some(stage_stats) = stage_stats.clone() {
                        actor.set("__songlua_graph_display_stage_stats", stage_stats)?;
                    }
                    if let Some(player_stats) = player_stats.clone() {
                        actor.set("__songlua_graph_display_player_stats", player_stats)?;
                    }
                    capture_graph_display_values(lua, &actor, stage_stats, player_stats)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetStreamWidth",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if actor_type_is(&actor, "SongMeterDisplay")?
                    && let Some(width) = method_arg(&args, 0).cloned().and_then(read_f32)
                {
                    actor.set("StreamWidth", width)?;
                    actor.set("__songlua_stream_width", width)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetStreamWidth",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_stream_width")?
                    .unwrap_or(0.0))
            }
        })?,
    )?;
    actor.set(
        "SetFromGameState",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                if actor_type_is(&actor, "CourseContentsList")? {
                    actor.set("__songlua_course_contents_from_gamestate", true)?;
                    actor.set("__songlua_scroller_num_items", 1_i64)?;
                    populate_course_contents_display(lua, &actor)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetCurrentAndDestinationItem",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(item) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_scroller_current_item", item)?;
                    actor.set("__songlua_scroller_destination_item", item)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetCurrentItem",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(item) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_scroller_current_item", item)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetCurrentItem",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_scroller_current_item")?
                    .unwrap_or(0.0))
            }
        })?,
    )?;
    actor.set(
        "SetDestinationItem",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(item) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_scroller_destination_item", item.max(0.0))?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetDestinationItem",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_scroller_destination_item")?
                    .unwrap_or(0.0))
            }
        })?,
    )?;
    actor.set(
        "GetNumItems",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<i64>>("__songlua_scroller_num_items")?
                    .unwrap_or(1)
                    .max(0))
            }
        })?,
    )?;
    actor.set(
        "SetTransformFromFunction",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(Value::Function(function)) = method_arg(&args, 0).cloned() {
                    actor.set("__songlua_scroller_transform_function", function)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetTransformFromHeight",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(height) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_scroller_item_height", height)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetTransformFromWidth",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(width) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_scroller_item_width", width)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "PositionItems",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                position_scroller_items(lua, &actor)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetSecondsToDestination",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                let current = actor
                    .get::<Option<f32>>("__songlua_scroller_current_item")?
                    .unwrap_or(0.0);
                let destination = actor
                    .get::<Option<f32>>("__songlua_scroller_destination_item")?
                    .unwrap_or(current);
                let seconds_per_item = actor
                    .get::<Option<f32>>("__songlua_scroller_seconds_per_item")?
                    .unwrap_or(0.0);
                Ok((destination - current).abs() * seconds_per_item)
            }
        })?,
    )?;
    actor.set(
        "SetSecondsPerItem",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(seconds) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_scroller_seconds_per_item", seconds.max(0.0))?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetLoop",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).map(truthy) {
                    actor.set("__songlua_scroller_loop", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetLoop",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<bool>>("__songlua_scroller_loop")?
                    .unwrap_or(false))
            }
        })?,
    )?;
    actor.set(
        "SetPauseCountdownSeconds",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(seconds) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_scroller_pause_countdown", seconds.max(0.0))?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetSecondsPauseBetweenItems",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(seconds) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_scroller_pause_between", seconds.max(0.0))?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetSecondsPauseBetweenItems",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_scroller_pause_between")?
                    .unwrap_or(0.0))
            }
        })?,
    )?;
    actor.set(
        "SetNumSubdivisions",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(count) = method_arg(&args, 0).cloned().and_then(read_i32_value) {
                    actor.set("__songlua_scroller_num_subdivisions", count.max(0))?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "ScrollThroughAllItems",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                let last_item = actor
                    .get::<Option<i64>>("__songlua_scroller_num_items")?
                    .unwrap_or(1)
                    .saturating_sub(1)
                    .max(0) as f32;
                actor.set("__songlua_scroller_destination_item", last_item)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "ScrollWithPadding",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let before = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0)
                    .max(0.0);
                let after = method_arg(&args, 1)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0)
                    .max(0.0);
                let last_item = actor
                    .get::<Option<i64>>("__songlua_scroller_num_items")?
                    .unwrap_or(1)
                    .saturating_sub(1)
                    .max(0) as f32;
                actor.set("__songlua_scroller_current_item", -before)?;
                actor.set("__songlua_scroller_destination_item", last_item + after)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    for (name, key) in [
        ("SetFastCatchup", "__songlua_scroller_fast_catchup"),
        ("SetWrap", "__songlua_scroller_wrap"),
        ("SetMask", "__songlua_scroller_mask"),
    ] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |_, args: MultiValue| {
                    actor.set(key, method_arg(&args, 0).is_some_and(truthy))?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "SetNumItemsToDraw",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(count) = method_arg(&args, 0).cloned().and_then(read_i32_value) {
                    actor.set("__songlua_scroller_num_items_to_draw", count.max(0))?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetFullScrollLengthSeconds",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                let num_items = actor
                    .get::<Option<i64>>("__songlua_scroller_num_items")?
                    .unwrap_or(1)
                    .max(0) as f32;
                let seconds_per_item = actor
                    .get::<Option<f32>>("__songlua_scroller_seconds_per_item")?
                    .unwrap_or(0.0);
                let pause = actor
                    .get::<Option<f32>>("__songlua_scroller_pause_between")?
                    .unwrap_or(0.0);
                Ok(num_items * seconds_per_item + (num_items - 1.0).max(0.0) * pause)
            }
        })?,
    )?;
    actor.set(
        "SetDrawState",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(Value::Table(state)) = method_arg(&args, 0).cloned() {
                    actor.set("__songlua_draw_state", state.clone())?;
                    if let Some(mode) = state.get::<Option<String>>("Mode")? {
                        actor.set("__songlua_draw_state_mode", mode)?;
                    }
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetDrawState",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                if let Some(state) = actor.get::<Option<Table>>("__songlua_draw_state")? {
                    return Ok(state);
                }
                let state = lua.create_table()?;
                if let Some(mode) = actor.get::<Option<String>>("__songlua_draw_state_mode")? {
                    state.set("Mode", mode)?;
                }
                Ok(state)
            }
        })?,
    )?;
    actor.set(
        "SetLineWidth",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(width) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_line_width", width.max(0.0))?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetLineWidth",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_line_width")?
                    .unwrap_or(1.0))
            }
        })?,
    )?;
    actor.set(
        "SetNumVertices",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let count = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_i32_value)
                    .unwrap_or(0)
                    .max(0);
                actor.set("__songlua_vertex_count", i64::from(count))?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetVertices",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(Value::Table(vertices)) = method_arg(&args, 0).cloned() {
                    actor.set("__songlua_vertex_count", vertices.raw_len() as i64)?;
                    actor.set("__songlua_vertices", vertices)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetNumVertices",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                if let Some(count) = actor.get::<Option<i64>>("__songlua_vertex_count")? {
                    return Ok(count.max(0));
                }
                Ok(actor
                    .get::<Option<Table>>("__songlua_vertices")?
                    .map(|vertices| vertices.raw_len() as i64)
                    .unwrap_or(0))
            }
        })?,
    )?;
    actor.set(
        "cropbottom",
        make_actor_capture_f32_method(lua, actor, "cropbottom", None)?,
    )?;
    actor.set(
        "croptop",
        make_actor_capture_f32_method(lua, actor, "croptop", None)?,
    )?;
    actor.set(
        "fadeleft",
        make_actor_capture_f32_method(lua, actor, "fadeleft", None)?,
    )?;
    actor.set(
        "faderight",
        make_actor_capture_f32_method(lua, actor, "faderight", None)?,
    )?;
    actor.set(
        "fadetop",
        make_actor_capture_f32_method(lua, actor, "fadetop", None)?,
    )?;
    actor.set(
        "fadebottom",
        make_actor_capture_f32_method(lua, actor, "fadebottom", None)?,
    )?;
    actor.set(
        "shadowlength",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                capture_block_set_vec2(lua, &actor, "shadow_len", [value, -value])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "shadowlengthx",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let mut len = actor_shadow_len(lua, &actor)?;
                len[0] = value;
                capture_block_set_vec2(lua, &actor, "shadow_len", len)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "shadowlengthy",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let mut len = actor_shadow_len(lua, &actor)?;
                len[1] = -value;
                capture_block_set_vec2(lua, &actor, "shadow_len", len)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "shadowcolor",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(color) = read_color_args(&args) else {
                    return Ok(actor.clone());
                };
                capture_block_set_vec4(lua, &actor, "shadow_color", color)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "MaskSource",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                capture_block_set_bool(lua, &actor, "mask_source", true)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "MaskDest",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                capture_block_set_bool(lua, &actor, "mask_dest", true)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "rotationx",
        make_actor_capture_f32_method(lua, actor, "rot_x_deg", Some("rotationx"))?,
    )?;
    actor.set(
        "rotationy",
        make_actor_capture_f32_method(lua, actor, "rot_y_deg", Some("rotationy"))?,
    )?;
    actor.set(
        "rotationz",
        make_actor_capture_f32_method(lua, actor, "rot_z_deg", Some("rotationz"))?,
    )?;
    actor.set(
        "baserotationx",
        make_actor_capture_f32_method(lua, actor, "rot_x_deg", Some("rotationx"))?,
    )?;
    actor.set(
        "baserotationy",
        make_actor_capture_f32_method(lua, actor, "rot_y_deg", Some("rotationy"))?,
    )?;
    actor.set(
        "baserotationz",
        make_actor_capture_f32_method(lua, actor, "rot_z_deg", Some("rotationz"))?,
    )?;
    actor.set(
        "zoomx",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                record_probe_method_call(lua, &actor, "zoomx")?;
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "zoom_x", value)?;
                    actor_update_text_pre_zoom_flags(lua, &actor, true, false)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "zoomy",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                record_probe_method_call(lua, &actor, "zoomy")?;
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "zoom_y", value)?;
                    actor_update_text_pre_zoom_flags(lua, &actor, false, true)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "zoomto",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(width) = args.get(1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let Some(height) = args.get(2).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                capture_block_set_size(lua, &actor, [width, height])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set("scaletoclipped", make_actor_set_size_method(lua, actor)?)?;
    actor.set("ScaleToClipped", make_actor_set_size_method(lua, actor)?)?;
    for (name, cover) in [("scaletofit", false), ("scaletocover", true)] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |lua, args: MultiValue| {
                    let Some(left) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                        return Ok(actor.clone());
                    };
                    let Some(top) = method_arg(&args, 1).cloned().and_then(read_f32) else {
                        return Ok(actor.clone());
                    };
                    let Some(right) = method_arg(&args, 2).cloned().and_then(read_f32) else {
                        return Ok(actor.clone());
                    };
                    let Some(bottom) = method_arg(&args, 3).cloned().and_then(read_f32) else {
                        return Ok(actor.clone());
                    };
                    scale_actor_to_rect(lua, &actor, [left, top, right, bottom], cover)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "CropTo",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(width) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let Some(height) = method_arg(&args, 1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                crop_actor_to(lua, &actor, width, height)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "zoomtowidth",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(width) = args.get(1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let (_, height) = actor_base_size(&actor)?;
                capture_block_set_size(lua, &actor, [width, height])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "zoomtoheight",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(height) = args.get(1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let (width, _) = actor_base_size(&actor)?;
                capture_block_set_size(lua, &actor, [width, height])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetSize",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(width) = args.get(1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let Some(height) = args.get(2).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                capture_block_set_size(lua, &actor, [width, height])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set("setsize", make_actor_set_size_method(lua, actor)?)?;
    actor.set(
        "SetWidth",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(width) = args.get(1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let (_, height) = actor_base_size(&actor)?;
                capture_block_set_size(lua, &actor, [width, height])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetHeight",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(height) = args.get(1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let (width, _) = actor_base_size(&actor)?;
                capture_block_set_size(lua, &actor, [width, height])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "z",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                record_probe_method_call(lua, &actor, "z")?;
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "z", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set("addx", make_actor_add_f32_method(lua, actor, "x")?)?;
    actor.set("addy", make_actor_add_f32_method(lua, actor, "y")?)?;
    actor.set("addz", make_actor_add_f32_method(lua, actor, "z")?)?;
    actor.set(
        "addrotationx",
        make_actor_add_f32_method(lua, actor, "rot_x_deg")?,
    )?;
    actor.set(
        "addrotationy",
        make_actor_add_f32_method(lua, actor, "rot_y_deg")?,
    )?;
    actor.set(
        "addrotationz",
        make_actor_add_f32_method(lua, actor, "rot_z_deg")?,
    )?;
    for (name, state_key) in [
        ("skewx", "__songlua_state_skew_x"),
        ("skewy", "__songlua_state_skew_y"),
    ] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                let method_name = name.to_string();
                let state_key = state_key.to_string();
                move |lua, args: MultiValue| {
                    record_probe_method_call(lua, &actor, &method_name)?;
                    if let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) {
                        actor.set(state_key.as_str(), value)?;
                        let block_key = if method_name == "skewx" {
                            "skew_x"
                        } else {
                            "skew_y"
                        };
                        capture_block_set_f32(lua, &actor, block_key, value)?;
                    }
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "zoomz",
        make_actor_capture_f32_method(lua, actor, "zoom_z", Some("zoomz"))?,
    )?;
    actor.set(
        "blend",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(raw) = args.get(1).cloned().and_then(read_string)
                    && let Some(blend) = parse_overlay_blend_mode(raw.as_str())
                {
                    let block = actor_current_capture_block(lua, &actor)?;
                    let label = match blend {
                        SongLuaOverlayBlendMode::Alpha => "alpha",
                        SongLuaOverlayBlendMode::Add => "add",
                        SongLuaOverlayBlendMode::Multiply => "multiply",
                        SongLuaOverlayBlendMode::Subtract => "subtract",
                    };
                    block.set("blend", label)?;
                    block.set("__songlua_has_changes", true)?;
                    actor.set("__songlua_state_blend", label)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "glow",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(color) = read_color_args(&args) else {
                    return Ok(actor.clone());
                };
                capture_block_set_vec4(lua, &actor, "glow", color)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "effectmagnitude",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let block = actor_current_capture_block(lua, &actor)?;
                let value = lua.create_table()?;
                value.raw_set(1, args.get(1).cloned().and_then(read_f32).unwrap_or(0.0))?;
                value.raw_set(2, args.get(2).cloned().and_then(read_f32).unwrap_or(0.0))?;
                value.raw_set(3, args.get(3).cloned().and_then(read_f32).unwrap_or(0.0))?;
                block.set("effect_magnitude", value.clone())?;
                block.set("__songlua_has_changes", true)?;
                actor.set("__songlua_state_effect_magnitude", value)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "customtexturerect",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let rect = [
                    method_arg(&args, 0)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0),
                    method_arg(&args, 1)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0),
                    method_arg(&args, 2)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0),
                    method_arg(&args, 3)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0),
                ];
                capture_texture_rect(lua, &actor, rect)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetCustomImageRect",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let rect = [
                    method_arg(&args, 0)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0),
                    method_arg(&args, 1)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0),
                    method_arg(&args, 2)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0),
                    method_arg(&args, 3)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0),
                ];
                capture_texture_rect(lua, &actor, rect)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "stretchtexcoords",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let dx = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0);
                let dy = method_arg(&args, 1)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0);
                offset_texture_rect(lua, &actor, dx, dy)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "addimagecoords",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some((width, height)) = actor_image_texture_size(&actor)? else {
                    return Ok(actor.clone());
                };
                let dx = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0);
                let dy = method_arg(&args, 1)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0);
                let Some(rect) = texture_pixel_offset_rect(
                    actor_texture_rect(&actor)?,
                    [width, height],
                    [dx, dy],
                ) else {
                    return Ok(actor.clone());
                };
                capture_texture_rect(lua, &actor, rect)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "setstate",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(state_index) = method_arg(&args, 0).cloned().and_then(read_u32_value)
                else {
                    return Ok(actor.clone());
                };
                set_actor_sprite_state(lua, &actor, state_index)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetState",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(i64::from(
                    song_lua_valid_sprite_state_index(
                        actor.get::<Option<u32>>("__songlua_state_sprite_state_index")?,
                    )
                    .unwrap_or(0),
                ))
            }
        })?,
    )?;
    actor.set(
        "SetStateProperties",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(Value::Table(states)) = method_arg(&args, 0) else {
                    return Ok(actor.clone());
                };
                let state_count = states.raw_len();
                if state_count == 0 {
                    return Ok(actor.clone());
                }

                let mut animation_length = 0.0_f32;
                let mut first_frame = None;
                let mut first_delay = None;
                for state in states.sequence_values::<Table>() {
                    let state = state?;
                    if first_frame.is_none() {
                        first_frame = state.get::<Option<u32>>("Frame")?;
                    }
                    let delay = state.get::<Option<f32>>("Delay")?.unwrap_or(0.0).max(0.0);
                    if first_delay.is_none() {
                        first_delay = Some(delay);
                    }
                    animation_length += delay;
                }

                actor.set(
                    "__songlua_sprite_animation_length_seconds",
                    animation_length,
                )?;
                if let Some(delay) = first_delay {
                    capture_block_set_f32(lua, &actor, "sprite_state_delay", delay)?;
                }
                set_actor_sprite_state(lua, &actor, first_frame.unwrap_or(0))?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetAnimationLengthSeconds",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                if let Some(value) =
                    actor.get::<Option<f32>>("__songlua_sprite_animation_length_seconds")?
                {
                    return Ok(value);
                }
                let delay = actor
                    .get::<Option<f32>>("__songlua_state_sprite_state_delay")?
                    .unwrap_or(0.1)
                    .max(0.0);
                Ok(actor_sprite_frame_count(&actor)? as f32 * delay)
            }
        })?,
    )?;
    actor.set(
        "SetSecondsIntoAnimation",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(seconds) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                set_actor_seconds_into_animation(lua, &actor, seconds)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetEffectMode",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(mode) = method_arg(&args, 0).cloned().and_then(read_string) {
                    actor.set("__songlua_sprite_effect_mode", mode)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "animate",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "sprite_animate", value)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "play",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                if actor_type_is(&actor, "Sound")? {
                    capture_block_set_bool(lua, &actor, "sound_play", true)?;
                } else {
                    capture_block_set_bool(lua, &actor, "sprite_animate", true)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "playforplayer",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                if actor_type_is(&actor, "Sound")? {
                    capture_block_set_bool(lua, &actor, "sound_play", true)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "pause",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                if !actor_type_is(&actor, "Sound")? {
                    capture_block_set_bool(lua, &actor, "sprite_animate", false)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "loop",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "sprite_loop", value)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "rate",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "sprite_playback_rate", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    for name in ["SetAllStateDelays", "setallstatedelays"] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |lua, args: MultiValue| {
                    if let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) {
                        capture_block_set_f32(lua, &actor, "sprite_state_delay", value.max(0.0))?;
                    }
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "texcoordvelocity",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let velocity = [
                    args.get(1).cloned().and_then(read_f32).unwrap_or(0.0),
                    args.get(2).cloned().and_then(read_f32).unwrap_or(0.0),
                ];
                capture_block_set_vec2(lua, &actor, "texcoord_velocity", velocity)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "texturetranslate",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let offset = [
                    args.get(1).cloned().and_then(read_f32).unwrap_or(0.0),
                    args.get(2).cloned().and_then(read_f32).unwrap_or(0.0),
                ];
                capture_block_set_vec2(lua, &actor, "texcoord_offset", offset)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "texturewrapping",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "texture_wrapping", value)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetDecodeMovie",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                actor.set("__songlua_state_decode_movie", value)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetDecodeMovie",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| actor_decode_movie(&actor)
        })?,
    )?;
    actor.set(
        "glowshift",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "glowshift",
                    Some(1.0),
                    None,
                    Some([1.0, 1.0, 1.0, 0.2]),
                    Some([1.0, 1.0, 1.0, 0.8]),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "glowblink",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "glowshift",
                    Some(1.0),
                    None,
                    Some([1.0, 1.0, 1.0, 0.2]),
                    Some([1.0, 1.0, 1.0, 0.8]),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "diffuseshift",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "diffuseshift",
                    Some(1.0),
                    None,
                    Some([0.0, 0.0, 0.0, 1.0]),
                    Some([1.0, 1.0, 1.0, 1.0]),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "diffuseblink",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "diffuseshift",
                    Some(1.0),
                    None,
                    Some([0.5, 0.5, 0.5, 0.5]),
                    Some([1.0, 1.0, 1.0, 1.0]),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "diffuseramp",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "diffuseramp",
                    Some(1.0),
                    None,
                    Some([0.0, 0.0, 0.0, 1.0]),
                    Some([1.0, 1.0, 1.0, 1.0]),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "pulse",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "pulse",
                    Some(2.0),
                    Some([0.5, 1.0, 1.0]),
                    None,
                    None,
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "bob",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "bob",
                    Some(2.0),
                    Some([0.0, 20.0, 0.0]),
                    None,
                    None,
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "bounce",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "bounce",
                    Some(2.0),
                    Some([0.0, 20.0, 0.0]),
                    None,
                    None,
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "wag",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "wag",
                    Some(2.0),
                    Some([0.0, 0.0, 20.0]),
                    None,
                    None,
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "spin",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "spin",
                    None,
                    Some([0.0, 0.0, 180.0]),
                    None,
                    None,
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "effectcolor1",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let color = read_color_args(&args).unwrap_or([1.0, 1.0, 1.0, 1.0]);
                capture_block_set_vec4(lua, &actor, "effect_color1", color)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "effectcolor2",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let color = read_color_args(&args).unwrap_or([1.0, 1.0, 1.0, 1.0]);
                capture_block_set_vec4(lua, &actor, "effect_color2", color)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "effectperiod",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "effect_period", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "effectoffset",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "effect_offset", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "effecttiming",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(a) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let Some(b) = method_arg(&args, 1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let Some(c) = method_arg(&args, 2).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let Some(d) = method_arg(&args, 3).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let timing = if let Some(e) = method_arg(&args, 4).cloned().and_then(read_f32) {
                    [a.max(0.0), b.max(0.0), c.max(0.0), e.max(0.0), d.max(0.0)]
                } else {
                    [a.max(0.0), b.max(0.0), c.max(0.0), 0.0, d.max(0.0)]
                };
                let total = timing.iter().sum::<f32>();
                if total > 0.0 {
                    capture_block_set_vec5(lua, &actor, "effect_timing", timing)?;
                    capture_block_set_f32(lua, &actor, "effect_period", total)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "effectclock",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(raw) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                let Some(clock) = parse_overlay_effect_clock(raw.as_str()) else {
                    return Ok(actor.clone());
                };
                capture_block_set_string(lua, &actor, "effect_clock", effect_clock_label(clock))?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "vibrate",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                capture_block_set_bool(lua, &actor, "vibrate", true)?;
                capture_block_set_vec3(lua, &actor, "effect_magnitude", [10.0, 10.0, 10.0])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "stopeffect",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                capture_block_set_bool(lua, &actor, "vibrate", false)?;
                capture_block_set_bool(lua, &actor, "rainbow", false)?;
                set_actor_effect_mode(lua, &actor, "none")?;
                capture_block_set_vec3(lua, &actor, "effect_magnitude", [0.0, 0.0, 0.0])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "hurrytweening",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(factor) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    prepare_capture_scope_actor(lua, &actor)?;
                    hurry_actor_tweening(&actor, factor)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    for name in [
        "clearzbuffer",
        "Draw",
        "EnableAlphaBuffer",
        "EnableDepthBuffer",
        "EnableFloat",
        "EnablePreserveTexture",
        "Create",
        "SetAmbientLightColor",
        "SetDiffuseLightColor",
        "SetLightDirection",
        "SetSpecularLightColor",
        "SortByDrawOrder",
        "fardistz",
        "backfacecull",
        "StartTransitioningScreen",
        "stop",
        "volume",
        "cullmode",
    ] {
        actor.set(name, make_actor_chain_method(lua, actor)?)?;
    }
    actor.set(
        "position",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(seconds) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                set_actor_seconds_into_animation(lua, &actor, seconds)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "zbias",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                capture_block_set_f32(lua, &actor, "z_bias", value)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetDrawByZPosition",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let enabled = method_arg(&args, 0).map_or(true, truthy);
                capture_block_set_bool(lua, &actor, "draw_by_z_position", enabled)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "AddChildFromPath",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(path) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                add_actor_child_from_path(lua, &actor, &path)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "load",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if actor_type_is(&actor, "Sound")? {
                    set_actor_sound_file_from_value(&actor, method_arg(&args, 0), true)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "rainbow",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let enabled = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "rainbow", enabled)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "rainbowscroll",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let enabled = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "rainbow_scroll", enabled)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "jitter",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let enabled = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "text_jitter", enabled)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "distort",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let amount = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0)
                    .max(0.0);
                capture_block_set_f32(lua, &actor, "text_distortion", amount)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "undistort",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                capture_block_set_f32(lua, &actor, "text_distortion", 0.0)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "AddAttribute",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                capture_actor_text_attribute(lua, &actor, &args)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "ClearAttributes",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                actor.set("__songlua_text_attributes", lua.create_table()?)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    for (name, corner_mask) in [
        ("diffuseupperleft", 1 << 0),
        ("diffuseupperright", 1 << 1),
        ("diffuselowerright", 1 << 2),
        ("diffuselowerleft", 1 << 3),
        ("diffusetopedge", (1 << 0) | (1 << 1)),
        ("diffuserightedge", (1 << 1) | (1 << 2)),
        ("diffusebottomedge", (1 << 2) | (1 << 3)),
        ("diffuseleftedge", (1 << 0) | (1 << 3)),
    ] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |lua, args: MultiValue| {
                    capture_actor_vertex_diffuse(lua, &actor, &args, corner_mask)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "SetTextureFiltering",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let enabled = method_arg(&args, 0).map_or(true, truthy);
                capture_block_set_bool(lua, &actor, "texture_filtering", enabled)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    for name in ["zbuffer", "ztest", "zwrite"] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |lua, args: MultiValue| {
                    let enabled = method_arg(&args, 0).map_or(true, truthy);
                    capture_block_set_bool(lua, &actor, "depth_test", enabled)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "ztestmode",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let enabled = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .is_none_or(|mode| {
                        let normalized = mode
                            .trim()
                            .trim_start_matches("ZTestMode_")
                            .to_ascii_lowercase();
                        !matches!(normalized.as_str(), "off" | "none" | "false")
                    });
                capture_block_set_bool(lua, &actor, "depth_test", enabled)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "finishtweening",
        make_actor_finish_tweening_method(lua, actor)?,
    )?;
    actor.set("stoptweening", make_actor_stop_tweening_method(lua, actor)?)?;
    actor.set(
        "strokecolor",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(color) = read_color_args(&args) else {
                    return Ok(actor.clone());
                };
                actor.set("__songlua_stroke_color", make_color_table(lua, color)?)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "getstrokecolor",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let color = read_actor_color_field(&actor, "__songlua_stroke_color")
                    .map_err(mlua::Error::external)?
                    .or_else(|| read_actor_color_field(&actor, "StrokeColor").ok().flatten())
                    .unwrap_or([0.0, 0.0, 0.0, 0.0]);
                make_color_table(lua, color)
            }
        })?,
    )?;
    for name in ["set_mult_attrs_with_diffuse", "mult_attrs_with_diffuse"] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |lua, args: MultiValue| {
                    let value = method_arg(&args, 0)
                        .cloned()
                        .and_then(read_boolish)
                        .unwrap_or(true);
                    capture_block_set_bool(lua, &actor, "mult_attrs_with_diffuse", value)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "get_mult_attrs_with_diffuse",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<bool>>("__songlua_state_mult_attrs_with_diffuse")?
                    .unwrap_or(false))
            }
        })?,
    )?;
    actor.set(
        "textglowmode",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(mode) = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .and_then(|mode| parse_overlay_text_glow_mode(mode.as_str()))
                {
                    capture_block_set_string(
                        lua,
                        &actor,
                        "text_glow_mode",
                        text_glow_mode_label(mode),
                    )?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "settext",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let text = args
                    .get(1)
                    .cloned()
                    .map(lua_text_value)
                    .transpose()?
                    .unwrap_or_default();
                actor.set("Text", text)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "settextf",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                actor.set("Text", lua_format_text(lua, &args)?)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetText",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| lua_text_value(actor.get::<Value>("Text")?)
        })?,
    )?;
    for name in ["targetnumber", "SetTargetNumber"] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |_, args: MultiValue| {
                    if let Some(number) = method_arg(&args, 0).cloned().and_then(read_f32) {
                        actor.set("__songlua_target_number", number)?;
                        actor.set("Text", rolling_numbers_text(&actor, number)?)?;
                    }
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "GetTargetNumber",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_target_number")?
                    .unwrap_or(0.0))
            }
        })?,
    )?;
    actor.set("wrapwidthpixels", make_actor_wrap_width_method(lua, actor)?)?;
    actor.set(
        "_wrapwidthpixels",
        make_actor_wrap_width_method(lua, actor)?,
    )?;
    actor.set(
        "vertspacing",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                capture_block_set_i32(lua, &actor, "vert_spacing", value.round() as i32)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "maxwidth",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                capture_block_set_f32(lua, &actor, "max_width", value)?;
                capture_block_set_bool(lua, &actor, "max_w_pre_zoom", false)?;
                actor.set("__songlua_text_saw_max_width", true)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "maxheight",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                capture_block_set_f32(lua, &actor, "max_height", value)?;
                capture_block_set_bool(lua, &actor, "max_h_pre_zoom", false)?;
                actor.set("__songlua_text_saw_max_height", true)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "max_dimension_use_zoom",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "max_dimension_uses_zoom", value)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "uppercase",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "uppercase", value)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "diffuse",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(color) = read_color_args(&args) else {
                    return Ok(actor.clone());
                };
                capture_block_set_color(lua, &actor, color)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "diffusecolor",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(color) = read_color_args(&args) else {
                    return Ok(actor.clone());
                };
                capture_block_set_color(lua, &actor, color)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetDidTapNoteCallback",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                match args.get(1) {
                    Some(Value::Function(function)) => {
                        actor.set("__songlua_did_tap_note_callback", function.clone())?;
                    }
                    _ => {
                        actor.set("__songlua_did_tap_note_callback", Value::Nil)?;
                    }
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    let set_did_tap_note_callback = actor.get::<Function>("SetDidTapNoteCallback")?;
    actor.set("set_did_tap_note_callback", set_did_tap_note_callback)?;
    actor.set(
        "did_tap_note",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let column = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_i32_value)
                    .unwrap_or(0);
                let score = method_arg(&args, 1).cloned().unwrap_or(Value::Nil);
                let bright = method_arg(&args, 2).is_some_and(truthy);
                actor.set("__songlua_last_tap_note_column", column)?;
                actor.set("__songlua_last_tap_note_score", score.clone())?;
                actor.set("__songlua_last_tap_note_bright", bright)?;
                if let Some(callback) =
                    actor.get::<Option<Function>>("__songlua_did_tap_note_callback")?
                {
                    let mut callback_args = MultiValue::new();
                    callback_args.push_back(Value::Integer(i64::from(column)));
                    callback_args.push_back(score);
                    callback_args.push_back(Value::Boolean(bright));
                    let _ = callback.call::<Value>(callback_args)?;
                }
                note_song_lua_side_effect(lua)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetColumnActors",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let is_note_field = actor
                    .get::<Option<String>>("__songlua_player_child_name")?
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case("NoteField"))
                    || actor
                        .get::<Option<String>>("__songlua_actor_type")?
                        .as_deref()
                        .is_some_and(|kind| kind.eq_ignore_ascii_case("NoteField"));
                if !is_note_field {
                    return Ok(Value::Nil);
                }
                Ok(Value::Table(note_field_column_actors(lua, &actor)?))
            }
        })?,
    )?;
    actor.set(
        "get_column_actors",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                let Value::Function(method) = actor.get::<Value>("GetColumnActors")? else {
                    return Ok(Value::Nil);
                };
                method.call::<Value>(MultiValue::from_vec(vec![Value::Table(actor.clone())]))
            }
        })?,
    )?;
    actor.set(
        "GetChild",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(Value::Nil);
                };
                let children = actor_named_children(lua, &actor)?;
                if let Some(child) = children.get::<Option<Table>>(name.as_str())? {
                    return Ok(Value::Table(child));
                }
                if !can_create_named_child_actor(&actor, &name)? {
                    return Ok(Value::Nil);
                }
                let child = create_named_child_actor(lua, &actor, &name)?;
                actor_children(lua, &actor)?.set(name.as_str(), child.clone())?;
                Ok(Value::Table(child))
            }
        })?,
    )?;
    actor.set(
        "GetChildAt",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(index) = method_arg(&args, 0).and_then(read_child_index) else {
                    return Ok(Value::Nil);
                };
                actor_child_at(lua, &actor, index)
            }
        })?,
    )?;
    actor.set(
        "RunCommandsOnChildren",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(command) = method_arg(&args, 0).cloned().and_then(|value| match value {
                    Value::Function(function) => Some(function),
                    _ => None,
                }) else {
                    return Ok(actor.clone());
                };
                let params = method_arg(&args, 1).cloned();
                for child in actor_direct_children(lua, &actor)? {
                    let _ = match params.clone() {
                        Some(params) => command.call::<Value>((child, params))?,
                        None => command.call::<Value>(child)?,
                    };
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "runcommandsonleaves",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(command) = method_arg(&args, 0).cloned().and_then(|value| match value {
                    Value::Function(function) => Some(function),
                    _ => None,
                }) else {
                    return Ok(actor.clone());
                };
                run_command_on_leaves(lua, &actor, &command)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetWrapperState",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(index) = method_arg(&args, 0).cloned().and_then(|value| match value {
                    Value::Integer(value) => Some(value),
                    Value::Number(value) => Some(value as i64),
                    _ => None,
                }) else {
                    return Ok(Value::Nil);
                };
                let Some(wrapper) = actor_wrappers(lua, &actor)?.get::<Option<Table>>(index)?
                else {
                    return Ok(Value::Nil);
                };
                Ok(Value::Table(wrapper))
            }
        })?,
    )?;
    actor.set(
        "GetNumWrapperStates",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| Ok(actor_wrappers(lua, &actor)?.raw_len() as i64)
        })?,
    )?;
    actor.set(
        "AddWrapperState",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let wrapper = create_dummy_actor(lua, "WrapperState")?;
                copy_dummy_actor_tags(&actor, &wrapper)?;
                let wrappers = actor_wrappers(lua, &actor)?;
                let next_index = wrappers.raw_len() + 1;
                wrappers.raw_set(next_index, wrapper.clone())?;
                Ok(wrapper)
            }
        })?,
    )?;
    actor.set(
        "GetChildren",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| actor_named_children(lua, &actor)
        })?,
    )?;
    actor.set(
        "GetNumChildren",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let mut count = 0_i64;
                for pair in actor_named_children(lua, &actor)?.pairs::<Value, Value>() {
                    let _ = pair?;
                    count += 1;
                }
                Ok(count)
            }
        })?,
    )?;
    actor.set(
        "GetX",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_x")?
                    .unwrap_or(0.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetName",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                Ok(Value::String(lua.create_string(
                    actor.get::<Option<String>>("Name")?.unwrap_or_default(),
                )?))
            }
        })?,
    )?;
    actor.set(
        "GetY",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_y")?
                    .unwrap_or(0.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetDestX",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_x")?
                    .unwrap_or(0.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetDestY",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_y")?
                    .unwrap_or(0.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetDestZ",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_z")?
                    .unwrap_or(0.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetVisible",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<bool>>("__songlua_visible")?
                    .unwrap_or(true))
            }
        })?,
    )?;
    actor.set(
        "GetZoom",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_zoom")?
                    .unwrap_or(1.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetWidth",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| Ok(actor_base_size(&actor)?.0)
        })?,
    )?;
    actor.set(
        "GetZoomedWidth",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                let width = actor_base_size(&actor)?.0;
                Ok(width
                    * actor_zoom_axis(
                        &actor,
                        "__songlua_state_zoom_x",
                        "__songlua_state_basezoom_x",
                    )?)
            }
        })?,
    )?;
    actor.set(
        "GetNumStates",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| Ok(i64::from(actor_sprite_frame_count(&actor)?))
        })?,
    )?;
    actor.set(
        "GetHeight",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| Ok(actor_base_size(&actor)?.1)
        })?,
    )?;
    actor.set(
        "GetZoomedHeight",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                let height = actor_base_size(&actor)?.1;
                Ok(height
                    * actor_zoom_axis(
                        &actor,
                        "__songlua_state_zoom_y",
                        "__songlua_state_basezoom_y",
                    )?)
            }
        })?,
    )?;
    actor.set(
        "GetZ",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_z")?
                    .unwrap_or(0.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetRotationX",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_rot_x_deg")?
                    .unwrap_or(0.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetRotationY",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_rot_y_deg")?
                    .unwrap_or(0.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetRotationZ",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_rot_z_deg")?
                    .unwrap_or(0.0_f32))
            }
        })?,
    )?;
    actor.set(
        "getrotation",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                let x = actor
                    .get::<Option<f32>>("__songlua_state_rot_x_deg")?
                    .unwrap_or(0.0_f32);
                let y = actor
                    .get::<Option<f32>>("__songlua_state_rot_y_deg")?
                    .unwrap_or(0.0_f32);
                let z = actor
                    .get::<Option<f32>>("__songlua_state_rot_z_deg")?
                    .unwrap_or(0.0_f32);
                Ok((x, y, z))
            }
        })?,
    )?;
    actor.set(
        "GetZoomX",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_zoom_x")?
                    .or(actor.get::<Option<f32>>("__songlua_state_zoom")?)
                    .unwrap_or(1.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetZoomY",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_zoom_y")?
                    .or(actor.get::<Option<f32>>("__songlua_state_zoom")?)
                    .unwrap_or(1.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetZoomZ",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_zoom_z")?
                    .or(actor.get::<Option<f32>>("__songlua_state_zoom")?)
                    .unwrap_or(1.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetBaseZoomX",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_basezoom_x")?
                    .or(actor.get::<Option<f32>>("__songlua_state_basezoom")?)
                    .unwrap_or(1.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetBaseZoomY",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_basezoom_y")?
                    .or(actor.get::<Option<f32>>("__songlua_state_basezoom")?)
                    .unwrap_or(1.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetBaseZoomZ",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_basezoom_z")?
                    .or(actor.get::<Option<f32>>("__songlua_state_basezoom")?)
                    .unwrap_or(1.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetAlpha",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| Ok(actor_diffuse(&actor)?[3])
        })?,
    )?;
    actor.set(
        "GetDiffuse",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| make_color_table(lua, actor_diffuse(&actor)?)
        })?,
    )?;
    actor.set(
        "GetGlow",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| make_color_table(lua, actor_glow(&actor)?)
        })?,
    )?;
    actor.set(
        "GetDiffuseAlpha",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| Ok(actor_diffuse(&actor)?[3])
        })?,
    )?;
    actor.set(
        "GetHAlign",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| actor_halign(&actor)
        })?,
    )?;
    actor.set(
        "GetVAlign",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| actor_valign(&actor)
        })?,
    )?;
    actor.set(
        "geteffectmagnitude",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                let [x, y, z] = actor_effect_magnitude(&actor)?;
                Ok((x, y, z))
            }
        })?,
    )?;
    actor.set(
        "GetSecsIntoEffect",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let (beat, seconds) = compile_song_runtime_values(lua)?;
                let clock = actor
                    .get::<Option<String>>("__songlua_state_effect_clock")?
                    .as_deref()
                    .and_then(parse_overlay_effect_clock)
                    .unwrap_or(EffectClock::Time);
                Ok(match clock {
                    EffectClock::Time => seconds,
                    EffectClock::Beat => beat,
                })
            }
        })?,
    )?;
    actor.set(
        "GetEffectDelta",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                let (delta_beat, delta_seconds) = compile_song_runtime_delta_values(lua)?;
                let clock = actor
                    .get::<Option<String>>("__songlua_state_effect_clock")?
                    .as_deref()
                    .and_then(parse_overlay_effect_clock)
                    .unwrap_or(EffectClock::Time);
                Ok(match clock {
                    EffectClock::Time => delta_seconds,
                    EffectClock::Beat => delta_beat,
                })
            }
        })?,
    )?;
    actor.set(
        "GetParent",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| {
                Ok(actor
                    .get::<Option<Table>>("__songlua_parent")?
                    .unwrap_or_else(|| actor.clone()))
            }
        })?,
    )?;
    Ok(())
}

fn actor_base_size(actor: &Table) -> mlua::Result<(f32, f32)> {
    if let Some(size) = actor
        .get::<Option<Table>>("__songlua_state_size")?
        .and_then(|value| table_vec2(&value))
    {
        return Ok((size[0].abs(), size[1].abs()));
    }
    if let Some(rect) = actor
        .get::<Option<Table>>("__songlua_state_stretch_rect")?
        .and_then(|value| table_vec4(&value))
    {
        return Ok(((rect[2] - rect[0]).abs(), (rect[3] - rect[1]).abs()));
    }
    if let Some(size) = actor_image_frame_size(actor)? {
        return Ok(size);
    }
    if actor_type_is(actor, "GraphDisplay")? {
        return Ok((
            theme_metric_number("GraphDisplay", "BodyWidth").unwrap_or(300.0),
            theme_metric_number("GraphDisplay", "BodyHeight").unwrap_or(64.0),
        ));
    }
    Ok((1.0, 1.0))
}

fn actor_image_texture_size(actor: &Table) -> mlua::Result<Option<(f32, f32)>> {
    if let Some(path) = actor_texture_path(actor)?
        && is_song_lua_image_path(&path)
        && let Ok((width, height)) = image_dimensions(&path)
    {
        return Ok(Some((width as f32, height as f32)));
    }
    Ok(None)
}

fn actor_image_frame_size(actor: &Table) -> mlua::Result<Option<(f32, f32)>> {
    let sheet_dims = actor_texture_path(actor)?
        .as_ref()
        .map(|path| parse_sprite_sheet_dims(path.to_string_lossy().as_ref()));
    Ok(sprite_image_frame_size(
        actor_image_texture_size(actor)?,
        actor
            .get::<Option<bool>>("__songlua_state_sprite_animate")?
            .unwrap_or(false),
        actor.get::<Option<u32>>("__songlua_state_sprite_state_index")?,
        sheet_dims,
    ))
}

fn actor_crop_source_size(lua: &Lua, actor: &Table) -> mlua::Result<Option<(f32, f32)>> {
    if let Some(size) = actor_image_frame_size(actor)? {
        return Ok(Some(size));
    }
    let Some(path) = actor_texture_path(actor)? else {
        return Ok(None);
    };
    if is_song_lua_video_path(&path) {
        return song_lua_screen_size(lua).map(Some);
    }
    Ok(None)
}

fn crop_actor_to(lua: &Lua, actor: &Table, width: f32, height: f32) -> mlua::Result<()> {
    let target = [width, height];
    if !target.iter().all(|value| value.is_finite() && *value > 0.0) {
        return Ok(());
    }
    let Some((source_width, source_height)) = actor_crop_source_size(lua, actor)? else {
        return Ok(());
    };
    let Some(rect) = crop_texture_rect([source_width, source_height], target) else {
        return Ok(());
    };
    capture_block_set_size(lua, actor, target)?;
    capture_block_set_u32(
        lua,
        actor,
        "sprite_state_index",
        SONG_LUA_SPRITE_STATE_CLEAR,
    )?;
    capture_block_set_vec4(lua, actor, "custom_texture_rect", rect)?;
    capture_block_set_f32(lua, actor, "zoom", 1.0)?;
    capture_block_set_zoom_axes(lua, actor, 1.0, "zoom_x", "zoom_y", "zoom_z")?;
    Ok(())
}

fn create_texture_proxy(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    let texture = lua.create_table()?;
    let (screen_width, screen_height) = song_lua_screen_size(lua)?;
    let frame_count = actor_sprite_frame_count(actor)?;
    if actor
        .get::<Option<String>>("__songlua_actor_type")?
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("ActorFrameTexture"))
    {
        if let Some(capture_name) = actor_aft_capture_name(actor)? {
            texture.set("__songlua_aft_capture_name", capture_name)?;
        }
        install_texture_proxy_methods(
            lua,
            &texture,
            actor,
            String::new(),
            screen_width,
            screen_height,
            screen_width,
            screen_height,
            frame_count,
        )?;
        return Ok(texture);
    }

    let raw_texture = actor.get::<Option<String>>("Texture")?.unwrap_or_default();
    let resolved = actor_texture_path(actor)?;
    let path = resolved
        .as_deref()
        .map(file_path_string)
        .unwrap_or_else(|| raw_texture.clone());
    let (source_width, source_height) = texture_source_size(
        lua,
        actor,
        resolved
            .as_deref()
            .unwrap_or_else(|| Path::new(&raw_texture)),
    )?;
    install_texture_proxy_methods(
        lua,
        &texture,
        actor,
        path,
        source_width,
        source_height,
        source_width,
        source_height,
        frame_count,
    )?;
    Ok(texture)
}

fn install_texture_proxy_methods(
    lua: &Lua,
    texture: &Table,
    actor: &Table,
    path: String,
    source_width: f32,
    source_height: f32,
    texture_width: f32,
    texture_height: f32,
    frame_count: u32,
) -> mlua::Result<()> {
    texture.set("__songlua_texture_path", path.clone())?;
    texture.set(
        "GetPath",
        lua.create_function(move |lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string(&path)?))
        })?,
    )?;
    texture.set(
        "GetSourceWidth",
        lua.create_function(move |_, _args: MultiValue| Ok(source_width))?,
    )?;
    texture.set(
        "GetSourceHeight",
        lua.create_function(move |_, _args: MultiValue| Ok(source_height))?,
    )?;
    texture.set(
        "GetTextureWidth",
        lua.create_function(move |_, _args: MultiValue| Ok(texture_width))?,
    )?;
    texture.set(
        "GetTextureHeight",
        lua.create_function(move |_, _args: MultiValue| Ok(texture_height))?,
    )?;
    texture.set(
        "GetNumFrames",
        lua.create_function(move |_, _args: MultiValue| Ok(i64::from(frame_count)))?,
    )?;
    texture.set(
        "loop",
        lua.create_function({
            let actor = actor.clone();
            let texture = texture.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "sprite_loop", value)?;
                Ok(Value::Table(texture.clone()))
            }
        })?,
    )?;
    texture.set(
        "rate",
        lua.create_function({
            let actor = actor.clone();
            let texture = texture.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "sprite_playback_rate", value)?;
                }
                Ok(Value::Table(texture.clone()))
            }
        })?,
    )?;
    Ok(())
}

fn texture_source_size(lua: &Lua, actor: &Table, path: &Path) -> mlua::Result<(f32, f32)> {
    if is_song_lua_image_path(path)
        && let Ok((width, height)) = image_dimensions(path)
    {
        return Ok((width as f32, height as f32));
    }
    if is_song_lua_video_path(path) {
        return song_lua_screen_size(lua);
    }
    actor_base_size(actor)
}

fn add_actor_child_from_path(lua: &Lua, actor: &Table, path: &str) -> mlua::Result<()> {
    let Some(song_dir) =
        actor
            .get::<Option<String>>("__songlua_song_dir")?
            .or(lua.globals().get::<Option<String>>("__songlua_song_dir")?)
    else {
        return Ok(());
    };
    let Ok(Value::Table(child)) = load_actor_path(lua, Path::new(&song_dir), path) else {
        return Ok(());
    };
    child.set("__songlua_parent", actor.clone())?;
    if !push_sequence_child_once(actor, child.clone())? {
        return Ok(());
    }
    run_added_actor_child_commands(lua, actor, &child)
}

pub(super) fn read_tracked_compile_actors(lua: &Lua) -> Result<Vec<TrackedCompileActor>, String> {
    let globals = lua.globals();
    let top_screen = globals
        .get::<Table>("__songlua_top_screen")
        .map_err(|err| err.to_string())?;
    let children = actor_children(lua, &top_screen).map_err(|err| err.to_string())?;
    let song_foreground = if let Some(actor) = children
        .get::<Option<Table>>("SongForeground")
        .map_err(|err| err.to_string())?
    {
        actor
    } else {
        let actor = create_named_child_actor(lua, &top_screen, "SongForeground")
            .map_err(|err| err.to_string())?;
        children
            .set("SongForeground", actor.clone())
            .map_err(|err| err.to_string())?;
        actor
    };
    Ok(vec![
        tracked_compile_actor(
            globals
                .get::<Table>("__songlua_top_screen_player_1")
                .map_err(|err| err.to_string())?,
            TrackedCompileActorTarget::Player(0),
        )?,
        tracked_compile_actor(
            globals
                .get::<Table>("__songlua_top_screen_player_2")
                .map_err(|err| err.to_string())?,
            TrackedCompileActorTarget::Player(1),
        )?,
        tracked_compile_actor(song_foreground, TrackedCompileActorTarget::SongForeground)?,
    ])
}

fn tracked_compile_actor(
    table: Table,
    target: TrackedCompileActorTarget,
) -> Result<TrackedCompileActor, String> {
    Ok(TrackedCompileActor {
        actor: SongLuaCapturedActor {
            initial_state: actor_overlay_initial_state(&table)?,
            message_commands: Vec::new(),
        },
        table,
        target,
    })
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

fn set_rolling_numbers_metric(actor: &Table, metric: &str) -> mlua::Result<()> {
    actor.set("__songlua_rolling_numbers_metric", metric)?;
    actor.set(
        "__songlua_rolling_numbers_format",
        rolling_numbers_format(metric),
    )?;
    Ok(())
}

fn rolling_numbers_text(actor: &Table, number: f32) -> mlua::Result<String> {
    let format = actor
        .get::<Option<String>>("__songlua_rolling_numbers_format")?
        .unwrap_or_else(|| "%.0f".to_string());
    Ok(format_rolling_number(&format, number))
}
