use super::*;

pub(super) struct TopScreenLuaTables {
    pub(super) top_screen: Table,
    pub(super) players: [Table; LUA_PLAYERS],
}

#[inline(always)]
fn song_lua_column_x(column_index: usize) -> f32 {
    SONG_LUA_COLUMN_X.get(column_index).copied().unwrap_or(0.0)
}

pub(super) fn create_arrow_effects_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "GetYOffset",
        lua.create_function(|_, args: MultiValue| {
            Ok(args
                .get(2)
                .cloned()
                .and_then(read_f32)
                .map(|beat| -64.0 * beat)
                .unwrap_or(0.0_f32))
        })?,
    )?;
    table.set(
        "GetYPos",
        lua.create_function(|_, args: MultiValue| {
            Ok(args.get(2).cloned().and_then(read_f32).unwrap_or(0.0_f32))
        })?,
    )?;
    table.set(
        "GetXPos",
        lua.create_function(|_, args: MultiValue| {
            let column_index = args
                .get(1)
                .cloned()
                .and_then(read_f32)
                .map(|value| value as isize - 1)
                .filter(|value| *value >= 0)
                .map(|value| value as usize)
                .unwrap_or(0);
            Ok(song_lua_column_x(column_index))
        })?,
    )?;
    table.set(
        "GetZPos",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    table.set(
        "GetRotationX",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    table.set(
        "GetRotationY",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    table.set(
        "GetRotationZ",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    table.set(
        "GetZoom",
        lua.create_function(|_, _args: MultiValue| Ok(1.0_f32))?,
    )?;
    table.set(
        "GetAlpha",
        lua.create_function(|_, _args: MultiValue| Ok(1.0_f32))?,
    )?;
    table.set(
        "GetGlow",
        lua.create_function(|_, _args: MultiValue| Ok(0.0_f32))?,
    )?;
    Ok(table)
}

fn actor_children(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    if let Some(children) = actor.get::<Option<Table>>("__songlua_children")? {
        return Ok(children);
    }
    let children = lua.create_table()?;
    actor.set("__songlua_children", children.clone())?;
    Ok(children)
}

fn actor_named_children(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    let children = lua.create_table()?;
    for pair in actor_children(lua, actor)?.pairs::<Value, Value>() {
        let (key, value) = pair?;
        children.set(key, value)?;
    }
    merge_actor_sequence_children(lua, actor, &children)?;
    Ok(children)
}

fn actor_direct_children(lua: &Lua, actor: &Table) -> mlua::Result<Vec<Table>> {
    let mut out = Vec::new();
    let mut seen = Vec::new();
    for value in actor.sequence_values::<Value>() {
        let Value::Table(child) = value? else {
            continue;
        };
        push_unique_actor_child(&mut out, &mut seen, child);
    }
    for pair in actor_children(lua, actor)?.pairs::<Value, Value>() {
        let (_, value) = pair?;
        let Value::Table(child) = value else {
            continue;
        };
        if child
            .get::<Option<bool>>("__songlua_child_group")?
            .unwrap_or(false)
        {
            for group_value in child.sequence_values::<Value>() {
                if let Value::Table(group_child) = group_value? {
                    push_unique_actor_child(&mut out, &mut seen, group_child);
                }
            }
        } else {
            push_unique_actor_child(&mut out, &mut seen, child);
        }
    }
    Ok(out)
}

fn actor_child_at(lua: &Lua, actor: &Table, index: usize) -> mlua::Result<Value> {
    Ok(actor_direct_children(lua, actor)?
        .into_iter()
        .nth(index)
        .map_or(Value::Nil, Value::Table))
}

fn read_child_index(value: &Value) -> Option<usize> {
    match value {
        Value::Integer(value) if *value >= 0 => Some(*value as usize),
        Value::Number(value) if value.is_finite() && *value >= 0.0 => Some(*value as usize),
        _ => None,
    }
}

fn push_unique_actor_child(out: &mut Vec<Table>, seen: &mut Vec<usize>, child: Table) {
    let ptr = child.to_pointer() as usize;
    if seen.contains(&ptr) {
        return;
    }
    seen.push(ptr);
    out.push(child);
}

fn merge_actor_sequence_children(lua: &Lua, actor: &Table, children: &Table) -> mlua::Result<()> {
    for value in actor.sequence_values::<Value>() {
        let Value::Table(child) = value? else {
            continue;
        };
        let Some(name) = child.get::<Option<String>>("Name")? else {
            continue;
        };
        if name.trim().is_empty() {
            continue;
        }
        match children.get::<Option<Value>>(name.as_str())? {
            Some(Value::Table(group))
                if group
                    .get::<Option<bool>>("__songlua_child_group")?
                    .unwrap_or(false) =>
            {
                group.raw_set(group.raw_len() + 1, child)?;
            }
            Some(Value::Table(existing)) => {
                let group = lua.create_table()?;
                group.set("__songlua_child_group", true)?;
                group.raw_set(1, existing)?;
                group.raw_set(2, child)?;
                children.set(name.as_str(), group)?;
            }
            Some(_) => {}
            None => {
                children.set(name.as_str(), child)?;
            }
        }
    }
    Ok(())
}

fn actor_wrappers(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    if let Some(wrappers) = actor.get::<Option<Table>>("__songlua_wrappers")? {
        return Ok(wrappers);
    }
    let wrappers = lua.create_table()?;
    actor.set("__songlua_wrappers", wrappers.clone())?;
    Ok(wrappers)
}

fn copy_dummy_actor_tags(from: &Table, into: &Table) -> mlua::Result<()> {
    if let Some(player_index) = from.get::<Option<i64>>("__songlua_player_index")? {
        into.set("__songlua_player_index", player_index)?;
    }
    if let Some(child_name) = from.get::<Option<String>>("__songlua_player_child_name")? {
        into.set("__songlua_player_child_name", child_name)?;
    }
    if let Some(child_name) = from.get::<Option<String>>("__songlua_top_screen_child_name")? {
        into.set("__songlua_top_screen_child_name", child_name)?;
    }
    for key in [
        "__songlua_main_title",
        "__songlua_bpm_text",
        "__songlua_steps_text_1",
        "__songlua_steps_text_2",
    ] {
        if let Some(value) = from.get::<Option<String>>(key)? {
            into.set(key, value)?;
        }
    }
    Ok(())
}

fn create_named_child_actor(lua: &Lua, parent: &Table, name: &str) -> mlua::Result<Table> {
    let parent_type = parent.get::<Option<String>>("__songlua_actor_type")?;
    let player_index = parent.get::<Option<i64>>("__songlua_player_index")?;
    let child = if parent_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("PlayerActor"))
        && name.eq_ignore_ascii_case("NoteField")
        && let Some(player_index) = player_index
    {
        create_note_field_actor(lua, player_index as usize)?
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

const TOP_SCREEN_THEME_CHILD_NAMES: &[&str] = &[
    "Underlay",
    "Overlay",
    "BPMDisplay",
    "LifeFrame",
    "ScoreFrame",
    "Lyrics",
    "SongBackground",
    "SongForeground",
    "StageDisplay",
    "SongTitle",
    "ScoreP1",
    "ScoreP2",
    "SongMeterDisplayP1",
    "SongMeterDisplayP2",
    "StepsDisplayP1",
    "StepsDisplayP2",
    "LifeMeterBarP1",
    "LifeMeterBarP2",
];

const UNDERLAY_THEME_CHILD_NAMES: &[&str] = &[
    "Header",
    "StepStatsPaneP1",
    "StepStatsPaneP2",
    "DangerP1",
    "DangerP2",
    "P1Score",
    "P2Score",
    "SongMeter",
];

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

fn create_note_field_actor(lua: &Lua, player_index: usize) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "NoteField")?;
    actor.set("__songlua_player_index", player_index as i64)?;
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
    for column_index in 0..SONG_LUA_NOTE_COLUMNS {
        let column = create_note_column_actor(lua, note_field, player_index, column_index)?;
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
) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "NoteColumnRenderer")?;
    actor.set("__songlua_parent", note_field.clone())?;
    actor.set("__songlua_player_index", player_index as i64)?;
    actor.set("__songlua_state_x", song_lua_column_x(column_index))?;
    actor.set("__songlua_state_y", 0.0_f32)?;
    actor.set("__songlua_state_z", 0.0_f32)?;

    let pos_handler = create_note_column_spline_handler(lua)?;
    let rot_handler = create_note_column_spline_handler(lua)?;
    let zoom_handler = create_note_column_spline_handler(lua)?;
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
    handler.set("__songlua_spline_mode", "NoteColumnSplineMode_Offset")?;
    handler.set("__songlua_subtract_song_beat", false)?;
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
        create_top_screen_player_actor(lua, players[0].clone(), 0)?,
        create_top_screen_player_actor(lua, players[1].clone(), 1)?,
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

pub(super) const SONG_LUA_SCREEN_PLAYER_OPTIONS_LINE_NAMES: &str = "SpeedModType,SpeedMod,Mini,Perspective,NoteSkinSL,NoteSkinVariant,Judgment,ComboFont,HoldJudgment,BackgroundFilter,NoteFieldOffsetX,NoteFieldOffsetY,VisualDelay,MusicRate,Stepchart,ScreenAfterPlayerOptions";
pub(super) const SONG_LUA_SCREEN_PLAYER_OPTIONS2_LINE_NAMES: &str = "Turn,Scroll,Hide,LifeMeterType,DataVisualizations,TargetScore,ActionOnMissedTarget,GameplayExtras,GameplayExtrasB,GameplayExtrasC,TiltMultiplier,ErrorBar,ErrorBarTrim,ErrorBarOptions,MeasureCounter,MeasureCounterOptions,MeasureLines,TimingWindowOptions,FaPlus,ScreenAfterPlayerOptions2";
pub(super) const SONG_LUA_SCREEN_PLAYER_OPTIONS3_LINE_NAMES: &str =
    "Insert,Remove,Holds,11,12,13,Attacks,Characters,HideLightType,ScreenAfterPlayerOptions3";
pub(super) const SONG_LUA_SCREEN_ATTACK_MENU_LINE_NAMES: &str =
    "SpeedModType,SpeedMod,Mini,Perspective,NoteSkin,MusicRate,Assist,ShowBGChangesPlay";
pub(super) const SONG_LUA_SCREEN_OPTIONS_SERVICE_LINE_NAMES: &str = "SystemOptions,MapControllers,TestInput,InputOptions,GraphicsSoundOptions,VisualOptions,ArcadeOptions,Bookkeeping,AdvancedOptions,MenuTimerOptions,USBProfileOptions,OptionsManageProfiles,ThemeOptions,TournamentModeOptions,GrooveStatsOptions,StepManiaCredits,Reload";
pub(super) const SONG_LUA_SCREEN_SYSTEM_OPTIONS_LINE_NAMES: &str =
    "Game,Theme,Language,Announcer,DefaultNoteSkin,EditorNoteSkin";
pub(super) const SONG_LUA_SCREEN_INPUT_OPTIONS_LINE_NAMES: &str =
    "AutoMap,OnlyDedicatedMenu,OptionsNav,Debounce,ThreeKey,AxisFix";
pub(super) const SONG_LUA_SCREEN_GRAPHICS_SOUND_OPTIONS_LINE_NAMES: &str = "VideoRenderer,DisplayMode,DisplayAspectRatio,DisplayResolution,RefreshRate,FullscreenType,DisplayColorDepth,HighResolutionTextures,MaxTextureResolution";
pub(super) const SONG_LUA_SCREEN_VISUAL_OPTIONS_LINE_NAMES: &str =
    "AppearanceOptions,Set BG Fit Mode,Overscan Correction,CRT Test Patterns";
pub(super) const SONG_LUA_SCREEN_APPEARANCE_OPTIONS_LINE_NAMES: &str = "Center1Player,ShowBanners,BGBrightness,RandomBackgroundMode,NumBackgrounds,ShowLyrics,ShowNativeLanguage,ShowDancingCharacters";
pub(super) const SONG_LUA_SCREEN_ARCADE_OPTIONS_LINE_NAMES: &str = "Event,Coin,CoinsPerCredit,MaxNumCredits,ResetCoinsAtStartup,Premium,SongsPerPlay,Long Time,Marathon Time";
pub(super) const SONG_LUA_SCREEN_ADVANCED_OPTIONS_LINE_NAMES: &str =
    "DefaultFailType,TimingWindowScale,LifeDifficulty,HiddenSongs,EasterEggs,AllowExtraStage";
pub(super) const SONG_LUA_SCREEN_THEME_OPTIONS_LINE_NAMES: &str =
    "VisualStyle,MusicWheelSpeed,MusicWheelStyle,AutoStyle,DefaultGameMode,CasualMaxMeter";
pub(super) const SONG_LUA_SCREEN_MENU_TIMER_OPTIONS_LINE_NAMES: &str =
    "MenuTimer,ScreenSelectMusicMenuTimer,ScreenPlayerOptionsMenuTimer,ScreenEvaluationMenuTimer";
pub(super) const SONG_LUA_SCREEN_USB_PROFILE_OPTIONS_LINE_NAMES: &str = "MemoryCards,CustomSongs,MaxCount,CustomSongsLoadTimeout,CustomSongsMaxSeconds,CustomSongsMaxMegabytes";
pub(super) const SONG_LUA_SCREEN_TOURNAMENT_MODE_OPTIONS_LINE_NAMES: &str =
    "EnableTournamentMode,ScoringSystem,StepStats,EnforceNoCmod";
pub(super) const SONG_LUA_SCREEN_GROOVE_STATS_OPTIONS_LINE_NAMES: &str =
    "EnableGrooveStats,AutoDownloadUnlocks,SeparateUnlocksByPlayer,QRLogin,EnableOnlineLobbies";
const SONG_LUA_TOP_SCREEN_OPTION_ROWS: &[&str] = &[
    "SpeedModType",
    "SpeedMod",
    "Mini",
    "Perspective",
    "NoteSkin",
    "NoteSkinVariant",
    "JudgmentGraphic",
    "ComboFont",
    "HoldJudgment",
    "BackgroundFilter",
    "NoteFieldOffsetX",
    "NoteFieldOffsetY",
    "VisualDelay",
    "MusicRate",
    "Stepchart",
    "ScreenAfterPlayerOptions",
    "Turn",
    "Scroll",
    "Hide",
    "LifeMeterType",
    "DataVisualizations",
    "TargetScore",
    "ActionOnMissedTarget",
    "GameplayExtras",
    "GameplayExtrasB",
    "GameplayExtrasC",
    "TiltMultiplier",
    "ErrorBar",
    "ErrorBarTrim",
    "ErrorBarOptions",
    "MeasureCounter",
    "MeasureCounterOptions",
    "MeasureLines",
    "TimingWindowOptions",
    "TimingWindows",
    "FaPlus",
    "ScoreBoxOptions",
    "StepStatsExtra",
    "FunOptions",
    "LifeBarOptions",
    "ComboColors",
    "ComboMode",
    "TimerMode",
    "JudgmentAnimation",
    "RailBalance",
    "ExtraAesthetics",
    "ScreenAfterPlayerOptions2",
    "Insert",
    "Remove",
    "Holds",
    "Attacks",
    "Characters",
    "HideLightType",
    "ScreenAfterPlayerOptions3",
    "Assist",
    "ShowBGChangesPlay",
    "ScreenAfterPlayerOptions4",
];

fn top_screen_option_row_name(value: Option<Value>) -> String {
    match value {
        Some(Value::String(name)) => name
            .to_str()
            .map(|name| name.to_string())
            .unwrap_or_default(),
        Some(value) => read_i32_value(value)
            .and_then(|index| top_screen_option_row_name_at(index).map(str::to_string))
            .unwrap_or_else(|| SONG_LUA_TOP_SCREEN_OPTION_ROWS[0].to_string()),
        None => SONG_LUA_TOP_SCREEN_OPTION_ROWS[0].to_string(),
    }
}

fn top_screen_option_row_name_at(index: i32) -> Option<&'static str> {
    let index = usize::try_from(index).ok()?;
    SONG_LUA_TOP_SCREEN_OPTION_ROWS
        .get(index)
        .or_else(|| {
            index
                .checked_sub(1)
                .and_then(|index| SONG_LUA_TOP_SCREEN_OPTION_ROWS.get(index))
        })
        .copied()
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

    let items = lua.create_table()?;
    items.set("__songlua_child_group", true)?;
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

fn option_row_default_text(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "speedmodtype" => "X".to_string(),
        "speedmod" => "1".to_string(),
        "mini" => "0%".to_string(),
        "perspective" => "Overhead".to_string(),
        "noteskin" | "noteskinvariant" => "default".to_string(),
        "musicrate" => "1".to_string(),
        _ => custom_option_default_text(name).unwrap_or_default(),
    }
}

fn create_top_screen_player_actor(
    lua: &Lua,
    player: SongLuaPlayerContext,
    player_index: usize,
) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "PlayerActor")?;
    actor.set("Name", top_screen_player_name(player_index))?;
    actor.set("__songlua_player_index", player_index as i64)?;
    actor.set("__songlua_visible", true)?;
    actor.set("__songlua_state_x", player.screen_x)?;
    actor.set("__songlua_state_y", player.screen_y)?;
    Ok(actor)
}

fn top_screen_player_name(player_index: usize) -> &'static str {
    match player_index {
        0 => "PlayerP1",
        1 => "PlayerP2",
        _ => "",
    }
}

fn top_screen_player_index(name: &str) -> Option<usize> {
    match name {
        "PlayerP1" => Some(0),
        "PlayerP2" => Some(1),
        _ => None,
    }
}

fn top_screen_life_meter_index(name: &str) -> Option<usize> {
    match name {
        "LifeP1" => Some(0),
        "LifeP2" => Some(1),
        _ => None,
    }
}

fn top_screen_life_meter_name(player_index: usize) -> &'static str {
    match player_index {
        0 => "LifeP1",
        1 => "LifeP2",
        _ => "",
    }
}

fn top_screen_score_index(name: &str) -> Option<usize> {
    match name {
        "ScoreP1" => Some(0),
        "ScoreP2" => Some(1),
        _ => None,
    }
}

fn top_screen_score_name(player_index: usize) -> &'static str {
    match player_index {
        0 => "ScoreP1",
        1 => "ScoreP2",
        _ => "",
    }
}

fn top_screen_score_percent_name(player_index: usize) -> &'static str {
    match player_index {
        0 => "PercentP1",
        1 => "PercentP2",
        _ => "",
    }
}

fn top_screen_steps_display_index(name: &str) -> Option<usize> {
    match name {
        "StepsDisplayP1" => Some(0),
        "StepsDisplayP2" => Some(1),
        _ => None,
    }
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

fn top_screen_song_meter_display_index(name: &str) -> Option<usize> {
    match name {
        "SongMeterDisplayP1" => Some(0),
        "SongMeterDisplayP2" => Some(1),
        _ => None,
    }
}

fn top_screen_life_meter_bar_index(name: &str) -> Option<usize> {
    match name {
        "LifeMeterBarP1" => Some(0),
        "LifeMeterBarP2" => Some(1),
        _ => None,
    }
}

fn underlay_score_index(name: &str) -> Option<usize> {
    match name {
        "P1Score" => Some(0),
        "P2Score" => Some(1),
        _ => None,
    }
}

fn underlay_score_name(player_index: usize) -> &'static str {
    match player_index {
        0 => "P1Score",
        1 => "P2Score",
        _ => "",
    }
}

fn top_screen_step_stats_pane_index(name: &str) -> Option<usize> {
    match name {
        "StepStatsPaneP1" => Some(0),
        "StepStatsPaneP2" => Some(1),
        _ => None,
    }
}

fn top_screen_danger_index(name: &str) -> Option<usize> {
    match name {
        "DangerP1" => Some(0),
        "DangerP2" => Some(1),
        _ => None,
    }
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
    let body_group = lua.create_table()?;
    body_group.set("__songlua_child_group", true)?;
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

fn install_course_contents_list_children(actor: &Table) -> mlua::Result<()> {
    actor.set("__songlua_scroller_current_item", 0.0_f32)?;
    actor.set("__songlua_scroller_destination_item", 0.0_f32)?;
    actor.set("__songlua_scroller_num_items", 1_i64)?;
    if let Some(display) = actor.get::<Option<Table>>("Display")? {
        if display.get::<Option<String>>("Name")?.is_none() {
            display.set("Name", "Display")?;
        }
        display.set("__songlua_parent", actor.clone())?;
        push_sequence_child_once(actor, display)?;
    }
    Ok(())
}

fn position_scroller_items(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let Some(display) = actor_named_children(lua, actor)?.get::<Option<Table>>("Display")? else {
        return Ok(());
    };
    let num_items = actor
        .get::<Option<i64>>("__songlua_scroller_num_items")?
        .unwrap_or(1)
        .max(1);
    if let Some(transform) =
        actor.get::<Option<Function>>("__songlua_scroller_transform_function")?
    {
        transform.call::<()>((display, 0.0_f32, 0_i64, num_items))?;
    } else if let Some(height) = actor.get::<Option<f32>>("__songlua_scroller_item_height")? {
        display.set("Y", 0.0_f32 * height)?;
    } else if let Some(width) = actor.get::<Option<f32>>("__songlua_scroller_item_width")? {
        display.set("X", 0.0_f32 * width)?;
    }
    Ok(())
}

fn push_sequence_child_once(actor: &Table, child: Table) -> mlua::Result<bool> {
    let child_ptr = child.to_pointer();
    for index in 1..=actor.raw_len() {
        if let Some(Value::Table(existing)) = actor.raw_get::<Option<Value>>(index)?
            && existing.to_pointer() == child_ptr
        {
            return Ok(false);
        }
    }
    actor.raw_set(actor.raw_len() + 1, child)?;
    Ok(true)
}

fn populate_course_contents_display(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let Some(display) = actor_named_children(lua, actor)?.get::<Option<Table>>("Display")? else {
        return Ok(());
    };
    for player_index in 0..LUA_PLAYERS {
        let params = create_course_contents_params(lua, player_index)?;
        let params = Some(Value::Table(params));
        run_actor_named_command_with_drain_and_params(
            lua,
            &display,
            "SetSongCommand",
            true,
            params.clone(),
        )?;
        run_named_command_on_leaves(lua, &display, "SetSongCommand", params)?;
    }
    Ok(())
}

fn create_course_contents_params(lua: &Lua, player_index: usize) -> mlua::Result<Table> {
    let params = lua.create_table()?;
    params.set("Number", 0_i64)?;
    params.set("PlayerNumber", player_number_name(player_index))?;
    params.set("Song", current_song_value(lua)?)?;
    let steps = current_steps_value(lua, player_index)?;
    params.set("Steps", steps.clone())?;
    set_course_contents_steps_params(lua, &params, steps)?;
    Ok(params)
}

pub(super) fn current_song_value(lua: &Lua) -> mlua::Result<Value> {
    current_gamestate_value(lua, "GetCurrentSong")
}

pub(super) fn current_gamestate_value(lua: &Lua, method_name: &str) -> mlua::Result<Value> {
    let Some(gamestate) = lua.globals().get::<Option<Table>>("GAMESTATE")? else {
        return Ok(Value::Nil);
    };
    let Some(method) = gamestate.get::<Option<Function>>(method_name)? else {
        return Ok(Value::Nil);
    };
    method.call::<Value>(gamestate)
}

pub(super) fn current_steps_value(lua: &Lua, player_index: usize) -> mlua::Result<Value> {
    current_gamestate_player_value(lua, "GetCurrentSteps", player_index)
}

pub(super) fn current_gamestate_player_value(
    lua: &Lua,
    method_name: &str,
    player_index: usize,
) -> mlua::Result<Value> {
    let Some(gamestate) = lua.globals().get::<Option<Table>>("GAMESTATE")? else {
        return Ok(Value::Nil);
    };
    let Some(method) = gamestate.get::<Option<Function>>(method_name)? else {
        return Ok(Value::Nil);
    };
    method.call::<Value>((gamestate, player_number_name(player_index)))
}

fn set_course_contents_steps_params(lua: &Lua, params: &Table, steps: Value) -> mlua::Result<()> {
    let Value::Table(steps) = steps else {
        params.set("Meter", "?")?;
        params.set("Difficulty", SongLuaDifficulty::Medium.sm_name())?;
        return Ok(());
    };
    let meter = call_table_method(&steps, "GetMeter")?;
    params.set(
        "Meter",
        if matches!(meter, Value::Nil) {
            Value::String(lua.create_string("?")?)
        } else {
            meter
        },
    )?;
    let difficulty = call_table_method(&steps, "GetDifficulty")?;
    params.set(
        "Difficulty",
        if matches!(difficulty, Value::Nil) {
            Value::String(lua.create_string(SongLuaDifficulty::Medium.sm_name())?)
        } else {
            difficulty
        },
    )
}

fn call_table_method(table: &Table, method_name: &str) -> mlua::Result<Value> {
    let Some(method) = table.get::<Option<Function>>(method_name)? else {
        return Ok(Value::Nil);
    };
    method.call::<Value>(table.clone())
}

fn input_status_actor_text(actor_type: &str) -> Option<&'static str> {
    if actor_type.eq_ignore_ascii_case("DeviceList") {
        Some("No input devices")
    } else if actor_type.eq_ignore_ascii_case("InputList") {
        Some("No unmapped inputs")
    } else {
        None
    }
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

fn resolve_load_actor_path(lua: &Lua, song_dir: &Path, path: &str) -> mlua::Result<PathBuf> {
    if let Ok(resolved) = resolve_script_path(lua, song_dir, path) {
        if resolved.is_dir() {
            return resolve_load_actor_directory(&resolved, song_dir, path);
        }
        if resolved.is_file() {
            return Ok(resolved);
        }
    }
    resolve_load_actor_with_extensions(lua, song_dir, path)
}

fn resolve_load_actor_directory(dir: &Path, song_dir: &Path, path: &str) -> mlua::Result<PathBuf> {
    for candidate in [dir.join("default.lua"), dir.join("default.xml")] {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(mlua::Error::external(format!(
        "script '{}' not found relative to '{}'",
        path,
        song_dir.display()
    )))
}

fn resolve_load_actor_with_extensions(
    lua: &Lua,
    song_dir: &Path,
    path: &str,
) -> mlua::Result<PathBuf> {
    let raw = Path::new(path.trim());
    if raw.extension().is_some() {
        return Err(mlua::Error::external(format!(
            "script '{}' not found relative to '{}'",
            path,
            song_dir.display()
        )));
    }

    const LOAD_ACTOR_EXTENSIONS: &[&str] = &[
        "lua", "xml", "png", "jpg", "jpeg", "gif", "bmp", "webp", "apng", "mp4", "avi", "m4v",
        "mov", "webm", "mkv", "mpg", "mpeg", "ogg", "mp3", "wav", "flac", "opus", "m4a", "aac",
    ];

    for base_dir in load_actor_search_dirs(lua, song_dir)? {
        let base = base_dir.join(path);
        if base.is_dir()
            && let Ok(resolved) = resolve_load_actor_directory(&base, song_dir, path)
        {
            return Ok(resolved);
        }
        for ext in LOAD_ACTOR_EXTENSIONS {
            let candidate = base.with_extension(ext);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    Err(mlua::Error::external(format!(
        "script '{}' not found relative to '{}'",
        path,
        song_dir.display()
    )))
}

fn load_actor_search_dirs(lua: &Lua, song_dir: &Path) -> mlua::Result<Vec<PathBuf>> {
    let globals = lua.globals();
    let mut out = Vec::with_capacity(2);
    if let Some(current_dir) = globals
        .get::<Option<String>>("__songlua_script_dir")?
        .filter(|dir| !dir.trim().is_empty())
    {
        let current_dir = PathBuf::from(current_dir);
        if !out.iter().any(|dir| dir == &current_dir) {
            out.push(current_dir);
        }
    }
    if !out.iter().any(|dir| dir == song_dir) {
        out.push(song_dir.to_path_buf());
    }
    Ok(out)
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
    let chunk_env = create_chunk_env_proxy(lua, initial_chunk_environment(lua, path)?)?;
    let chunk = lua
        .load(&source)
        .set_name(path.to_string_lossy().as_ref())
        .set_environment(chunk_env.clone());
    let inner = chunk.into_function()?;
    let script_dir = path.parent().unwrap_or(song_dir).to_path_buf();
    let script_path = file_path_string(path);
    let chunk_env_for_call = chunk_env;
    lua.create_function(move |lua, args: MultiValue| {
        call_with_script_dir(lua, &script_dir, || {
            call_with_script_path(lua, &script_path, || {
                call_with_chunk_env(lua, &chunk_env_for_call, || {
                    inner.call::<Value>(args.clone())
                })
            })
        })
    })
}

pub(super) fn execute_script_file(lua: &Lua, path: &Path, song_dir: &Path) -> mlua::Result<Value> {
    let loader = load_script_file(lua, path, song_dir)?;
    loader.call::<Value>(())
}

pub(super) fn run_actor_init_commands(lua: &Lua, root: &Value) -> mlua::Result<()> {
    let Value::Table(root) = root else {
        return Ok(());
    };
    run_actor_init_commands_for_table(lua, root)
}

pub(super) fn run_actor_startup_commands(lua: &Lua, root: &Value) -> mlua::Result<()> {
    let Value::Table(root) = root else {
        return Ok(());
    };
    run_actor_startup_commands_for_table(lua, root)
}

pub(super) fn run_actor_update_functions(lua: &Lua, root: &Value) -> mlua::Result<()> {
    let Value::Table(root) = root else {
        return Ok(());
    };
    run_actor_update_functions_for_table(lua, root, 1.0_f64 / 60.0)
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

fn run_actor_init_commands_for_table(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if actor
        .get::<Option<bool>>("__songlua_init_commands_ran")?
        .unwrap_or(false)
    {
        return Ok(());
    }
    run_actor_init_command(lua, actor)?;
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child? else {
            continue;
        };
        child.set("__songlua_parent", actor.clone())?;
        run_actor_init_commands_for_table(lua, &child)?;
    }
    run_song_meter_stream_init_command(lua, actor)?;
    actor.set("__songlua_init_commands_ran", true)?;
    Ok(())
}

fn run_actor_startup_commands_for_table(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if actor
        .get::<Option<bool>>("__songlua_startup_commands_ran")?
        .unwrap_or(false)
    {
        return Ok(());
    }
    actor.set("__songlua_startup_command_started", true)?;
    actor.set("__songlua_startup_children_walked", false)?;
    if actor_runs_startup_commands(actor)? {
        run_actor_named_command_with_drain(lua, actor, "OnCommand", false)?;
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child? else {
            continue;
        };
        child.set("__songlua_parent", actor.clone())?;
        run_actor_startup_commands_for_table(lua, &child)?;
    }
    run_song_meter_stream_startup_command(lua, actor)?;
    actor.set("__songlua_startup_children_walked", true)?;
    drain_actor_command_queue(lua, actor)?;
    actor.set("__songlua_startup_commands_ran", true)?;
    Ok(())
}

fn song_meter_stream_child(lua: &Lua, actor: &Table) -> mlua::Result<Option<Table>> {
    if !actor_type_is(actor, "SongMeterDisplay")? {
        return Ok(None);
    }
    let Some(stream) = actor_named_children(lua, actor)?.get::<Option<Table>>("Stream")? else {
        return Ok(None);
    };
    stream.set("__songlua_parent", actor.clone())?;
    Ok(Some(stream))
}

fn run_song_meter_stream_init_command(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let Some(stream) = song_meter_stream_child(lua, actor)? else {
        return Ok(());
    };
    run_actor_init_commands_for_table(lua, &stream)
}

fn run_song_meter_stream_startup_command(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let Some(stream) = song_meter_stream_child(lua, actor)? else {
        return Ok(());
    };
    run_actor_startup_commands_for_table(lua, &stream)
}

fn actor_update_rate(actor: &Table) -> mlua::Result<f64> {
    let rate = actor
        .get::<Option<f64>>("__songlua_update_rate")?
        .unwrap_or(1.0);
    Ok(if rate.is_finite() && rate > 0.0 {
        rate
    } else {
        1.0
    })
}

fn run_actor_update_functions_for_table(
    lua: &Lua,
    actor: &Table,
    parent_delta_seconds: f64,
) -> mlua::Result<()> {
    let delta_seconds = parent_delta_seconds * actor_update_rate(actor)?;
    if let Some(update) = actor.get::<Option<Function>>("__songlua_update_function")? {
        call_actor_function(lua, actor, &update, Some(Value::Number(delta_seconds)))?;
        drain_actor_command_queue(lua, actor)?;
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child? else {
            continue;
        };
        child.set("__songlua_parent", actor.clone())?;
        run_actor_update_functions_for_table(lua, &child, delta_seconds)?;
    }
    if let Some(stream) = song_meter_stream_child(lua, actor)? {
        run_actor_update_functions_for_table(lua, &stream, delta_seconds)?;
    }
    Ok(())
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
        for table in update_function_action_tables(getupvalue, &update, seen_tables)? {
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

fn update_function_action_tables(
    getupvalue: &Function,
    function: &Function,
    seen_tables: &mut HashSet<usize>,
) -> Result<Vec<Table>, String> {
    let mut out = Vec::new();
    for index in 1..=function.info().num_upvalues {
        let (name, value): (Value, Value) = getupvalue
            .call((function.clone(), i64::from(index)))
            .map_err(|err| err.to_string())?;
        let Value::String(name) = name else {
            continue;
        };
        let name = name.to_str().map_err(|err| err.to_string())?;
        if !matches!(name.as_ref(), "mod_actions" | "actions") {
            continue;
        }
        let Value::Table(table) = value else {
            continue;
        };
        if seen_tables.insert(table.to_pointer() as usize) {
            out.push(table);
        }
    }
    Ok(out)
}

fn run_actor_init_command(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    run_actor_named_command(lua, actor, "InitCommand")
}

fn run_actor_named_command(lua: &Lua, actor: &Table, name: &str) -> mlua::Result<()> {
    run_actor_named_command_with_drain(lua, actor, name, true)
}

fn run_actor_named_command_with_drain(
    lua: &Lua,
    actor: &Table,
    name: &str,
    drain_queue: bool,
) -> mlua::Result<()> {
    run_actor_named_command_with_drain_and_params(lua, actor, name, drain_queue, None)
}

fn run_actor_named_command_with_drain_and_params(
    lua: &Lua,
    actor: &Table,
    name: &str,
    drain_queue: bool,
    params: Option<Value>,
) -> mlua::Result<()> {
    let Some(command) = actor.get::<Option<Function>>(name)? else {
        return Ok(());
    };
    run_guarded_actor_command(lua, actor, name, &command, drain_queue, params)
}

fn actor_runs_startup_commands(actor: &Table) -> mlua::Result<bool> {
    let _ = actor;
    Ok(true)
}

fn actor_command_args(actor: &Table, params: Option<Value>) -> MultiValue {
    let mut args = MultiValue::new();
    args.push_back(Value::Table(actor.clone()));
    if let Some(params) = params {
        args.push_back(params);
    }
    args
}

fn call_actor_function(
    lua: &Lua,
    actor: &Table,
    command: &Function,
    params: Option<Value>,
) -> mlua::Result<()> {
    if let Some(script_dir) = actor
        .get::<Option<String>>("__songlua_script_dir")?
        .filter(|dir| !dir.trim().is_empty())
    {
        return call_with_script_dir(lua, Path::new(&script_dir), || {
            command.call::<()>(actor_command_args(actor, params))
        });
    }
    command.call::<()>(actor_command_args(actor, params))
}

fn run_guarded_actor_command(
    lua: &Lua,
    actor: &Table,
    name: &str,
    command: &Function,
    drain_queue: bool,
    params: Option<Value>,
) -> mlua::Result<()> {
    let active = actor_active_commands(lua, actor)?;
    if active.get::<Option<bool>>(name)?.unwrap_or(false) {
        return Ok(());
    }
    active.set(name, true)?;
    let result = call_actor_function(lua, actor, command, params).map_err(|err| {
        mlua::Error::external(format!(
            "{} failed for {}: {err}",
            name,
            actor_debug_label(actor)
        ))
    });
    active.set(name, Value::Nil)?;
    result?;
    if drain_queue {
        drain_actor_command_queue(lua, actor)?;
    }
    Ok(())
}

fn actor_active_commands(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    if let Some(active) = actor.get::<Option<Table>>("__songlua_active_commands")? {
        return Ok(active);
    }
    let active = lua.create_table()?;
    actor.set("__songlua_active_commands", active.clone())?;
    Ok(active)
}

fn actor_command_queue(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    if let Some(queue) = actor.get::<Option<Table>>("__songlua_command_queue")? {
        return Ok(queue);
    }
    let queue = lua.create_table()?;
    actor.set("__songlua_command_queue", queue.clone())?;
    Ok(queue)
}

fn drain_actor_command_queue(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let queue = actor_command_queue(lua, actor)?;
    while queue.raw_len() > 0 {
        let Some(name) = queue.raw_get::<Option<String>>(1)? else {
            break;
        };
        let len = queue.raw_len();
        for index in 1..len {
            let value = queue.raw_get::<Value>(index + 1)?;
            queue.raw_set(index, value)?;
        }
        queue.raw_set(len, Value::Nil)?;
        run_actor_named_command(lua, actor, &format!("{name}Command"))?;
    }
    Ok(())
}

fn actor_debug_label(actor: &Table) -> String {
    let actor_type = actor
        .get::<Option<String>>("__songlua_actor_type")
        .ok()
        .flatten()
        .unwrap_or_else(|| "Actor".to_string());
    let name = actor.get::<Option<String>>("Name").ok().flatten();
    match name {
        Some(name) if !name.trim().is_empty() => format!("{actor_type} '{name}'"),
        _ => actor_type,
    }
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
    let mut message_commands = Vec::new();
    for pair in actor.clone().pairs::<Value, Value>() {
        let (key, value) = pair.map_err(|err| err.to_string())?;
        let Some(name) = read_string(key) else {
            continue;
        };
        if !name.ends_with("MessageCommand") || !matches!(value, Value::Function(_)) {
            continue;
        }
        let message = name.trim_end_matches("MessageCommand").to_string();
        let blocks = match capture_actor_command_preserving_state(lua, actor, name.as_str()) {
            Ok(blocks) => blocks,
            Err(err) => {
                let skipped = format!("{}.{}: {err}", actor_debug_label(actor), name);
                push_unique_compile_detail(
                    &mut info.skipped_message_command_captures,
                    skipped.clone(),
                );
                debug!("Skipping song lua overlay message capture for {}", skipped);
                continue;
            }
        };
        if !blocks.is_empty() {
            message_commands.push(SongLuaOverlayMessageCommand { message, blocks });
        }
    }
    flush_actor_capture(actor).map_err(|err| err.to_string())?;
    let startup_sound_blocks: Vec<_> = read_actor_capture_blocks(actor)?
        .into_iter()
        .filter(|block| block.delta.sound_play == Some(true))
        .collect();
    if !startup_sound_blocks.is_empty() {
        message_commands.push(SongLuaOverlayMessageCommand {
            message: SONG_LUA_STARTUP_MESSAGE.to_string(),
            blocks: startup_sound_blocks,
        });
    }
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
                SongLuaOverlayKind::Sprite { texture_path }
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
        SongLuaOverlayKind::ActorMultiVertex {
            vertices,
            texture_path: read_actor_multi_vertex_texture_path(actor, aft_capture_names)?,
        }
    } else if actor_type.eq_ignore_ascii_case("Model") {
        let Some(layers) = read_model_layers(actor)? else {
            return Ok(None);
        };
        SongLuaOverlayKind::Model { layers }
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

fn read_actor_multi_vertex_texture_path(
    actor: &Table,
    aft_capture_names: &HashSet<String>,
) -> Result<Option<PathBuf>, String> {
    if actor
        .get::<Option<String>>("__songlua_aft_capture_name")
        .map_err(|err| err.to_string())?
        .is_some()
    {
        return Ok(None);
    }
    let Some(texture) = actor
        .get::<Option<String>>("Texture")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    if aft_capture_names.contains(&texture) {
        return Ok(None);
    }
    Ok(resolve_actor_asset_path(actor, &texture).ok())
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
        let (uv_scale, uv_offset, uv_tex_shift) = song_lua_model_uv_params(slot, uv_rect);
        layers.push(SongLuaOverlayModelLayer {
            texture_key: slot.texture_key_shared(),
            vertices: crate::game::parsing::noteskin::build_model_geometry(slot),
            model_size: model.size(),
            uv_scale,
            uv_offset,
            uv_tex_shift,
            draw: song_lua_model_draw(slot.model_draw_at(0.0, 0.0)),
        });
    }
    if layers.is_empty() {
        Ok(None)
    } else {
        Ok(Some(Arc::from(layers.into_boxed_slice())))
    }
}

fn read_model_path(actor: &Table) -> Result<Option<PathBuf>, String> {
    for key in ["Meshes", "Materials", "Bones"] {
        let Some(raw) = actor
            .get::<Option<String>>(key)
            .map_err(|err| err.to_string())?
            .filter(|path| !path.trim().is_empty())
        else {
            continue;
        };
        if let Ok(path) = resolve_actor_asset_path(actor, &raw) {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

fn song_lua_model_uv_params(
    slot: &crate::game::parsing::noteskin::SpriteSlot,
    uv_rect: [f32; 4],
) -> ([f32; 2], [f32; 2], [f32; 2]) {
    let uv_scale = [uv_rect[2] - uv_rect[0], uv_rect[3] - uv_rect[1]];
    let uv_offset = [uv_rect[0], uv_rect[1]];
    let uv_tex_shift = match slot.source.as_ref() {
        crate::game::parsing::noteskin::SpriteSource::Atlas { tex_dims, .. } => {
            let tw = tex_dims.0.max(1) as f32;
            let th = tex_dims.1.max(1) as f32;
            let base_u0 = slot.def.src[0] as f32 / tw;
            let base_v0 = slot.def.src[1] as f32 / th;
            [uv_offset[0] - base_u0, uv_offset[1] - base_v0]
        }
        crate::game::parsing::noteskin::SpriteSource::Animated { .. } => [0.0, 0.0],
    };
    (uv_scale, uv_offset, uv_tex_shift)
}

fn song_lua_model_draw(
    draw: crate::game::parsing::noteskin::ModelDrawState,
) -> SongLuaOverlayModelDraw {
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

fn read_graph_display_size(
    state: SongLuaOverlayState,
    context: &SongLuaCompileContext,
) -> [f32; 2] {
    let fallback = graph_display_body_size(song_lua_human_player_count(context));
    let size = state.size.unwrap_or(fallback);
    [size[0].abs().max(1.0), size[1].abs().max(1.0)]
}

fn read_graph_display_line_state(lua: &Lua, actor: &Table) -> Result<SongLuaOverlayState, String> {
    let Some(line) = actor_named_children(lua, actor)
        .map_err(|err| err.to_string())?
        .get::<Option<Table>>("Line")
        .map_err(|err| err.to_string())?
    else {
        return Ok(SongLuaOverlayState::default());
    };
    actor_overlay_initial_state(&line)
}

fn read_graph_display_values(actor: &Table) -> Result<Arc<[f32]>, String> {
    let values = actor
        .get::<Option<Table>>("__songlua_graph_display_values")
        .map_err(|err| err.to_string())?;
    let Some(values) = values else {
        return Ok(default_graph_display_values());
    };
    let mut out = Vec::with_capacity(GRAPH_DISPLAY_VALUE_RESOLUTION);
    for index in 1..=GRAPH_DISPLAY_VALUE_RESOLUTION {
        let value = values
            .raw_get::<Option<Value>>(index)
            .map_err(|err| err.to_string())?
            .and_then(read_f32)
            .unwrap_or(SONG_LUA_INITIAL_LIFE)
            .clamp(0.0, 1.0);
        out.push(value);
    }
    Ok(Arc::from(out.into_boxed_slice()))
}

fn default_graph_display_values() -> Arc<[f32]> {
    Arc::from([SONG_LUA_INITIAL_LIFE; GRAPH_DISPLAY_VALUE_RESOLUTION])
}

fn read_graph_display_body_state(lua: &Lua, actor: &Table) -> Result<SongLuaOverlayState, String> {
    let Some(body_group) = actor_named_children(lua, actor)
        .map_err(|err| err.to_string())?
        .get::<Option<Table>>("")
        .map_err(|err| err.to_string())?
    else {
        return Ok(SongLuaOverlayState::default());
    };
    let Some(body) = body_group
        .raw_get::<Option<Table>>(2)
        .map_err(|err| err.to_string())?
        .or_else(|| body_group.raw_get::<Option<Table>>(1).ok().flatten())
    else {
        return Ok(SongLuaOverlayState::default());
    };
    actor_overlay_initial_state(&body)
}

fn read_song_meter_display_state(
    lua: &Lua,
    actor: &Table,
) -> Result<Option<(f32, SongLuaOverlayState)>, String> {
    let stream_width = actor
        .get::<Option<f32>>("__songlua_stream_width")
        .map_err(|err| err.to_string())?
        .or_else(|| {
            actor
                .get::<Option<Value>>("StreamWidth")
                .ok()
                .flatten()
                .and_then(read_f32)
        })
        .unwrap_or(0.0)
        .max(0.0);
    if stream_width <= f32::EPSILON {
        return Ok(None);
    }
    let Some(stream) = actor_named_children(lua, actor)
        .map_err(|err| err.to_string())?
        .get::<Option<Table>>("Stream")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let stream_state = actor_overlay_initial_state(&stream)?;
    Ok(Some((stream_width, stream_state)))
}

fn read_actor_multi_vertex_mesh(
    actor: &Table,
) -> Result<Option<Arc<[SongLuaOverlayMeshVertex]>>, String> {
    let Some(vertices) = actor
        .get::<Option<Table>>("__songlua_vertices")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let source = read_actor_multi_vertex_points(&vertices)?;
    let Some((start, end)) = read_actor_multi_vertex_range(actor, source.len())? else {
        return Ok(None);
    };
    let draw_mode = actor
        .get::<Option<String>>("__songlua_draw_state_mode")
        .map_err(|err| err.to_string())?
        .unwrap_or_else(|| "DrawMode_Triangles".to_string());
    let line_width = actor
        .get::<Option<f32>>("__songlua_line_width")
        .map_err(|err| err.to_string())?
        .unwrap_or(1.0)
        .max(0.0);
    let mesh = actor_multi_vertex_triangles(&source[start..end], draw_mode.as_str(), line_width);
    if mesh.is_empty() {
        Ok(None)
    } else {
        Ok(Some(Arc::from(mesh.into_boxed_slice())))
    }
}

fn read_actor_multi_vertex_range(
    actor: &Table,
    vertex_count: usize,
) -> Result<Option<(usize, usize)>, String> {
    if vertex_count == 0 {
        return Ok(None);
    }
    let state = actor
        .get::<Option<Table>>("__songlua_draw_state")
        .map_err(|err| err.to_string())?;
    let first = state
        .as_ref()
        .and_then(|state| state.get::<Option<Value>>("First").ok().flatten())
        .and_then(read_i32_value)
        .unwrap_or(1)
        .max(1) as usize;
    let start = first.saturating_sub(1).min(vertex_count);
    let num = state
        .as_ref()
        .and_then(|state| state.get::<Option<Value>>("Num").ok().flatten())
        .and_then(read_i32_value)
        .unwrap_or(-1);
    let end = if num < 0 {
        vertex_count
    } else {
        start.saturating_add(num as usize).min(vertex_count)
    };
    Ok((end > start).then_some((start, end)))
}

fn read_actor_multi_vertex_points(
    vertices: &Table,
) -> Result<Vec<SongLuaActorMultiVertexPoint>, String> {
    let mut out = Vec::with_capacity(vertices.raw_len());
    for value in vertices.sequence_values::<Value>() {
        let Some(vertex) = read_actor_multi_vertex_point(value.map_err(|err| err.to_string())?)?
        else {
            continue;
        };
        out.push(vertex);
    }
    Ok(out)
}

fn read_actor_multi_vertex_point(
    value: Value,
) -> Result<Option<SongLuaActorMultiVertexPoint>, String> {
    let Value::Table(vertex) = value else {
        return Ok(None);
    };
    let Value::Table(position) = vertex.raw_get::<Value>(1).map_err(|err| err.to_string())? else {
        return Ok(None);
    };
    let Some(x) = read_f32(
        position
            .raw_get::<Value>(1)
            .map_err(|err| err.to_string())?,
    ) else {
        return Ok(None);
    };
    let Some(y) = read_f32(
        position
            .raw_get::<Value>(2)
            .map_err(|err| err.to_string())?,
    ) else {
        return Ok(None);
    };
    let color = read_color_value(vertex.raw_get::<Value>(2).map_err(|err| err.to_string())?)
        .unwrap_or([1.0; 4]);
    let uv = match vertex.raw_get::<Value>(3).map_err(|err| err.to_string())? {
        Value::Table(table) => [
            table
                .raw_get::<Value>(1)
                .ok()
                .and_then(read_f32)
                .unwrap_or(0.0),
            table
                .raw_get::<Value>(2)
                .ok()
                .and_then(read_f32)
                .unwrap_or(0.0),
        ],
        _ => [0.0, 0.0],
    };
    Ok(Some(SongLuaActorMultiVertexPoint {
        pos: [x, y],
        color,
        uv,
    }))
}

fn actor_multi_vertex_triangles(
    vertices: &[SongLuaActorMultiVertexPoint],
    draw_mode: &str,
    line_width: f32,
) -> Vec<SongLuaOverlayMeshVertex> {
    if draw_mode.eq_ignore_ascii_case("DrawMode_Quads") {
        return actor_multi_vertex_quads(vertices);
    }
    if draw_mode.eq_ignore_ascii_case("DrawMode_QuadStrip") {
        return actor_multi_vertex_quad_strip(vertices);
    }
    if draw_mode.eq_ignore_ascii_case("DrawMode_LineStrip") {
        return actor_multi_vertex_line_strip(vertices, line_width);
    }
    if draw_mode.eq_ignore_ascii_case("DrawMode_Fan") {
        return actor_multi_vertex_fan(vertices);
    }
    actor_multi_vertex_triangle_list(vertices)
}

fn actor_multi_vertex_triangle_list(
    vertices: &[SongLuaActorMultiVertexPoint],
) -> Vec<SongLuaOverlayMeshVertex> {
    let mut out = Vec::with_capacity(vertices.len() / 3 * 3);
    for chunk in vertices.chunks_exact(3) {
        push_actor_multi_vertex_triangle(&mut out, chunk[0], chunk[1], chunk[2]);
    }
    out
}

fn actor_multi_vertex_quads(
    vertices: &[SongLuaActorMultiVertexPoint],
) -> Vec<SongLuaOverlayMeshVertex> {
    let mut out = Vec::with_capacity(vertices.len() / 4 * 6);
    for chunk in vertices.chunks_exact(4) {
        push_actor_multi_vertex_quad(&mut out, chunk[0], chunk[1], chunk[2], chunk[3]);
    }
    out
}

fn actor_multi_vertex_quad_strip(
    vertices: &[SongLuaActorMultiVertexPoint],
) -> Vec<SongLuaOverlayMeshVertex> {
    let mut out = Vec::with_capacity(vertices.len().saturating_sub(2) / 2 * 6);
    let mut index = 0usize;
    while index + 3 < vertices.len() {
        push_actor_multi_vertex_quad(
            &mut out,
            vertices[index],
            vertices[index + 1],
            vertices[index + 3],
            vertices[index + 2],
        );
        index += 2;
    }
    out
}

fn actor_multi_vertex_fan(
    vertices: &[SongLuaActorMultiVertexPoint],
) -> Vec<SongLuaOverlayMeshVertex> {
    if vertices.len() < 3 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(vertices.len().saturating_sub(2) * 3);
    for index in 1..vertices.len() - 1 {
        push_actor_multi_vertex_triangle(
            &mut out,
            vertices[0],
            vertices[index],
            vertices[index + 1],
        );
    }
    out
}

fn actor_multi_vertex_line_strip(
    vertices: &[SongLuaActorMultiVertexPoint],
    line_width: f32,
) -> Vec<SongLuaOverlayMeshVertex> {
    if vertices.len() < 2 || line_width <= f32::EPSILON {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(vertices.len().saturating_sub(1) * 6);
    let half_width = 0.5 * line_width;
    let mut offsets = Vec::with_capacity(vertices.len());
    for index in 0..vertices.len() {
        let offset = if index == 0 {
            actor_multi_vertex_line_normal(vertices[0], vertices[1], half_width)
        } else if index + 1 == vertices.len() {
            actor_multi_vertex_line_normal(vertices[index - 1], vertices[index], half_width)
        } else {
            actor_multi_vertex_line_join_offset(
                vertices[index - 1],
                vertices[index],
                vertices[index + 1],
                half_width,
            )
        };
        offsets.push(offset);
    }
    for index in 0..vertices.len().saturating_sub(1) {
        let a = vertices[index];
        let b = vertices[index + 1];
        if actor_multi_vertex_segment_len(a, b) <= f32::EPSILON {
            continue;
        }
        let a0 = actor_multi_vertex_offset_point(a, offsets[index], 1.0);
        let a1 = actor_multi_vertex_offset_point(a, offsets[index], -1.0);
        let b0 = actor_multi_vertex_offset_point(b, offsets[index + 1], 1.0);
        let b1 = actor_multi_vertex_offset_point(b, offsets[index + 1], -1.0);
        push_actor_multi_vertex_triangle(&mut out, a0, b0, b1);
        push_actor_multi_vertex_triangle(&mut out, a0, b1, a1);
    }
    out
}

fn actor_multi_vertex_segment_len(
    a: SongLuaActorMultiVertexPoint,
    b: SongLuaActorMultiVertexPoint,
) -> f32 {
    (b.pos[0] - a.pos[0]).hypot(b.pos[1] - a.pos[1])
}

fn actor_multi_vertex_line_normal(
    a: SongLuaActorMultiVertexPoint,
    b: SongLuaActorMultiVertexPoint,
    half_width: f32,
) -> [f32; 2] {
    let dx = b.pos[0] - a.pos[0];
    let dy = b.pos[1] - a.pos[1];
    let len = dx.hypot(dy);
    if len <= f32::EPSILON {
        return [0.0, 0.0];
    }
    [-dy / len * half_width, dx / len * half_width]
}

fn actor_multi_vertex_line_join_offset(
    prev: SongLuaActorMultiVertexPoint,
    current: SongLuaActorMultiVertexPoint,
    next: SongLuaActorMultiVertexPoint,
    half_width: f32,
) -> [f32; 2] {
    let prev_normal = actor_multi_vertex_line_normal(prev, current, 1.0);
    let next_normal = actor_multi_vertex_line_normal(current, next, 1.0);
    let miter = [
        prev_normal[0] + next_normal[0],
        prev_normal[1] + next_normal[1],
    ];
    let miter_len = miter[0].hypot(miter[1]);
    if miter_len <= f32::EPSILON {
        return [prev_normal[0] * half_width, prev_normal[1] * half_width];
    }
    let miter = [miter[0] / miter_len, miter[1] / miter_len];
    let denom = miter[0] * prev_normal[0] + miter[1] * prev_normal[1];
    if denom.abs() <= 0.1 {
        return [next_normal[0] * half_width, next_normal[1] * half_width];
    }
    let scale = (half_width / denom).clamp(-half_width * 4.0, half_width * 4.0);
    [miter[0] * scale, miter[1] * scale]
}

fn actor_multi_vertex_offset_point(
    vertex: SongLuaActorMultiVertexPoint,
    offset: [f32; 2],
    sign: f32,
) -> SongLuaActorMultiVertexPoint {
    SongLuaActorMultiVertexPoint {
        pos: [
            vertex.pos[0] + offset[0] * sign,
            vertex.pos[1] + offset[1] * sign,
        ],
        color: vertex.color,
        uv: vertex.uv,
    }
}

fn push_actor_multi_vertex_quad(
    out: &mut Vec<SongLuaOverlayMeshVertex>,
    a: SongLuaActorMultiVertexPoint,
    b: SongLuaActorMultiVertexPoint,
    c: SongLuaActorMultiVertexPoint,
    d: SongLuaActorMultiVertexPoint,
) {
    push_actor_multi_vertex_triangle(out, a, b, c);
    push_actor_multi_vertex_triangle(out, c, d, a);
}

fn push_actor_multi_vertex_triangle(
    out: &mut Vec<SongLuaOverlayMeshVertex>,
    a: SongLuaActorMultiVertexPoint,
    b: SongLuaActorMultiVertexPoint,
    c: SongLuaActorMultiVertexPoint,
) {
    out.push(actor_multi_vertex_mesh_vertex(a));
    out.push(actor_multi_vertex_mesh_vertex(b));
    out.push(actor_multi_vertex_mesh_vertex(c));
}

#[inline(always)]
fn actor_multi_vertex_mesh_vertex(
    vertex: SongLuaActorMultiVertexPoint,
) -> SongLuaOverlayMeshVertex {
    SongLuaOverlayMeshVertex {
        pos: vertex.pos,
        color: vertex.color,
        uv: vertex.uv,
    }
}

fn collect_aft_capture_names(actor: &Table, out: &mut HashSet<String>) -> Result<(), String> {
    if actor
        .get::<Option<String>>("__songlua_actor_type")
        .map_err(|err| err.to_string())?
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("ActorFrameTexture"))
        && let Some(capture_name) = actor_aft_capture_name(actor).map_err(|err| err.to_string())?
    {
        out.insert(capture_name);
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child.map_err(|err| err.to_string())? else {
            continue;
        };
        collect_aft_capture_names(&child, out)?;
    }
    Ok(())
}

fn actor_aft_capture_name(actor: &Table) -> mlua::Result<Option<String>> {
    if let Some(capture_name) = actor
        .get::<Option<String>>("__songlua_aft_capture_name")?
        .filter(|name| !name.trim().is_empty())
    {
        return Ok(Some(capture_name));
    }
    Ok(actor
        .get::<Option<String>>("Name")?
        .filter(|name| !name.trim().is_empty()))
}

pub(super) fn actor_overlay_initial_state(actor: &Table) -> Result<SongLuaOverlayState, String> {
    let mut state = SongLuaOverlayState::default();
    if let Some(visible) = actor
        .get::<Option<bool>>("__songlua_visible")
        .map_err(|err| err.to_string())?
    {
        state.visible = visible;
    }
    if let Some(diffuse) = actor
        .get::<Option<Table>>("__songlua_state_diffuse")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.diffuse = diffuse;
    }
    if let Some(colors) = actor
        .get::<Option<Table>>("__songlua_state_vertex_colors")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vertex_colors(&value))
    {
        state.vertex_colors = Some(colors);
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_x")
        .map_err(|err| err.to_string())?
    {
        state.x = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_y")
        .map_err(|err| err.to_string())?
    {
        state.y = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_z")
        .map_err(|err| err.to_string())?
    {
        state.z = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_z_bias")
        .map_err(|err| err.to_string())?
    {
        state.z_bias = value;
    }
    if let Some(value) = actor
        .get::<Option<i32>>("__songlua_state_draw_order")
        .map_err(|err| err.to_string())?
    {
        state.draw_order = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_draw_by_z_position")
        .map_err(|err| err.to_string())?
    {
        state.draw_by_z_position = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_halign")
        .map_err(|err| err.to_string())?
    {
        state.halign = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_valign")
        .map_err(|err| err.to_string())?
    {
        state.valign = value;
    }
    if let Some(value) = actor
        .get::<Option<String>>("__songlua_state_text_align")
        .map_err(|err| err.to_string())?
        .as_deref()
        .and_then(parse_overlay_text_align)
    {
        state.text_align = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_uppercase")
        .map_err(|err| err.to_string())?
    {
        state.uppercase = value;
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_shadow_len")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec2(&value))
    {
        state.shadow_len = value;
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_shadow_color")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.shadow_color = value;
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_glow")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.glow = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_fov")
        .map_err(|err| err.to_string())?
    {
        state.fov = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_vanishpoint")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec2(&value))
    {
        state.vanishpoint = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_cropleft")
        .map_err(|err| err.to_string())?
    {
        state.cropleft = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_cropright")
        .map_err(|err| err.to_string())?
    {
        state.cropright = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_croptop")
        .map_err(|err| err.to_string())?
    {
        state.croptop = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_cropbottom")
        .map_err(|err| err.to_string())?
    {
        state.cropbottom = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_fadeleft")
        .map_err(|err| err.to_string())?
    {
        state.fadeleft = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_faderight")
        .map_err(|err| err.to_string())?
    {
        state.faderight = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_fadetop")
        .map_err(|err| err.to_string())?
    {
        state.fadetop = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_fadebottom")
        .map_err(|err| err.to_string())?
    {
        state.fadebottom = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_mask_source")
        .map_err(|err| err.to_string())?
    {
        state.mask_source = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_mask_dest")
        .map_err(|err| err.to_string())?
    {
        state.mask_dest = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_depth_test")
        .map_err(|err| err.to_string())?
    {
        state.depth_test = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_zoom")
        .map_err(|err| err.to_string())?
    {
        state.zoom = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_zoom_x")
        .map_err(|err| err.to_string())?
    {
        state.zoom_x = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_zoom_y")
        .map_err(|err| err.to_string())?
    {
        state.zoom_y = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_zoom_z")
        .map_err(|err| err.to_string())?
    {
        state.zoom_z = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_basezoom")
        .map_err(|err| err.to_string())?
    {
        state.basezoom = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_basezoom_x")
        .map_err(|err| err.to_string())?
    {
        state.basezoom_x = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_basezoom_y")
        .map_err(|err| err.to_string())?
    {
        state.basezoom_y = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_basezoom_z")
        .map_err(|err| err.to_string())?
    {
        state.basezoom_z = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_rot_x_deg")
        .map_err(|err| err.to_string())?
    {
        state.rot_x_deg = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_rot_y_deg")
        .map_err(|err| err.to_string())?
    {
        state.rot_y_deg = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_rot_z_deg")
        .map_err(|err| err.to_string())?
    {
        state.rot_z_deg = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_skew_x")
        .map_err(|err| err.to_string())?
    {
        state.skew_x = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_skew_y")
        .map_err(|err| err.to_string())?
    {
        state.skew_y = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_vibrate")
        .map_err(|err| err.to_string())?
    {
        state.vibrate = value;
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_effect_magnitude")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec3(&value))
    {
        state.effect_magnitude = value;
    }
    if let Some(value) = actor
        .get::<Option<String>>("__songlua_state_effect_clock")
        .map_err(|err| err.to_string())?
        .as_deref()
        .and_then(parse_overlay_effect_clock)
    {
        state.effect_clock = value;
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_effect_color1")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.effect_color1 = value;
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_effect_color2")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.effect_color2 = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_effect_period")
        .map_err(|err| err.to_string())?
    {
        state.effect_period = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_effect_offset")
        .map_err(|err| err.to_string())?
    {
        state.effect_offset = value;
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_effect_timing")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec5(&value))
    {
        state.effect_timing = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_rainbow")
        .map_err(|err| err.to_string())?
    {
        state.rainbow = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_rainbow_scroll")
        .map_err(|err| err.to_string())?
    {
        state.rainbow_scroll = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_text_jitter")
        .map_err(|err| err.to_string())?
    {
        state.text_jitter = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_text_distortion")
        .map_err(|err| err.to_string())?
    {
        state.text_distortion = value;
    }
    if let Some(value) = actor
        .get::<Option<String>>("__songlua_state_text_glow_mode")
        .map_err(|err| err.to_string())?
        .as_deref()
        .and_then(parse_overlay_text_glow_mode)
    {
        state.text_glow_mode = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_mult_attrs_with_diffuse")
        .map_err(|err| err.to_string())?
    {
        state.mult_attrs_with_diffuse = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_sprite_animate")
        .map_err(|err| err.to_string())?
    {
        state.sprite_animate = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_sprite_loop")
        .map_err(|err| err.to_string())?
    {
        state.sprite_loop = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_sprite_playback_rate")
        .map_err(|err| err.to_string())?
    {
        state.sprite_playback_rate = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_sprite_state_delay")
        .map_err(|err| err.to_string())?
    {
        state.sprite_state_delay = value;
    }
    if let Some(value) = actor
        .get::<Option<i32>>("__songlua_state_vert_spacing")
        .map_err(|err| err.to_string())?
    {
        state.vert_spacing = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<i32>>("__songlua_state_wrap_width_pixels")
        .map_err(|err| err.to_string())?
    {
        state.wrap_width_pixels = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_max_width")
        .map_err(|err| err.to_string())?
    {
        state.max_width = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_max_height")
        .map_err(|err| err.to_string())?
    {
        state.max_height = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_max_w_pre_zoom")
        .map_err(|err| err.to_string())?
    {
        state.max_w_pre_zoom = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_max_h_pre_zoom")
        .map_err(|err| err.to_string())?
    {
        state.max_h_pre_zoom = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_max_dimension_uses_zoom")
        .map_err(|err| err.to_string())?
    {
        state.max_dimension_uses_zoom = value;
    }
    if let Some(value) = actor
        .get::<Option<u32>>("__songlua_state_sprite_state_index")
        .map_err(|err| err.to_string())?
    {
        state.sprite_state_index = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_decode_movie")
        .map_err(|err| err.to_string())?
    {
        state.decode_movie = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_texture_filtering")
        .map_err(|err| err.to_string())?
    {
        state.texture_filtering = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_texture_wrapping")
        .map_err(|err| err.to_string())?
    {
        state.texture_wrapping = value;
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_texcoord_offset")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec2(&value))
    {
        state.texcoord_offset = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_custom_texture_rect")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.custom_texture_rect = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_texcoord_velocity")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec2(&value))
    {
        state.texcoord_velocity = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_size")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec2(&value))
    {
        state.size = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_stretch_rect")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.stretch_rect = Some(value);
    }
    if let Some(raw) = actor
        .get::<Option<String>>("__songlua_state_blend")
        .map_err(|err| err.to_string())?
        .as_deref()
        .and_then(parse_overlay_blend_mode)
    {
        state.blend = raw;
    }
    if let Some(raw) = actor
        .get::<Option<String>>("__songlua_state_effect_mode")
        .map_err(|err| err.to_string())?
        .as_deref()
        .and_then(parse_overlay_effect_mode)
    {
        state.effect_mode = raw;
    }
    Ok(state)
}

fn read_proxy_target_kind(actor: &Table) -> Result<Option<SongLuaProxyTarget>, String> {
    let Some(raw_kind) = actor
        .get::<Option<String>>("__songlua_proxy_target_kind")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let player_index = actor
        .get::<Option<i64>>("__songlua_proxy_player_index")
        .map_err(|err| err.to_string())?
        .and_then(|value| usize::try_from(value).ok());
    Ok(match raw_kind.as_str() {
        "player" => player_index.map(|player_index| SongLuaProxyTarget::Player { player_index }),
        "notefield" => {
            player_index.map(|player_index| SongLuaProxyTarget::NoteField { player_index })
        }
        "judgment" => {
            player_index.map(|player_index| SongLuaProxyTarget::Judgment { player_index })
        }
        "combo" => player_index.map(|player_index| SongLuaProxyTarget::Combo { player_index }),
        "underlay" => Some(SongLuaProxyTarget::Underlay),
        "overlay" => Some(SongLuaProxyTarget::Overlay),
        _ => None,
    })
}

fn resolve_actor_asset_path(actor: &Table, raw: &str) -> Result<PathBuf, String> {
    let raw_path = Path::new(raw.trim());
    if raw_path.is_absolute() && raw_path.is_file() {
        return Ok(raw_path.to_path_buf());
    }
    if let Some(script_dir) = actor
        .get::<Option<String>>("__songlua_script_dir")
        .map_err(|err| err.to_string())?
        .filter(|dir| !dir.trim().is_empty())
    {
        let candidate = Path::new(&script_dir).join(raw);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(format!("actor asset '{}' could not be resolved", raw))
}

fn read_bitmap_font(actor: &Table) -> Result<Option<(&'static str, PathBuf)>, String> {
    let Some(font) = actor
        .get::<Option<String>>("Font")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    if let Ok(font_path) = resolve_actor_asset_path(actor, &font) {
        return Ok(Some((song_lua_font_name(font_path.as_path()), font_path)));
    }
    if song_lua_uses_builtin_theme_font(&font) {
        return Ok(Some((
            "miso",
            PathBuf::from("assets/fonts/miso/_miso light.ini"),
        )));
    }
    Ok(None)
}

fn read_bitmap_text_attributes(actor: &Table) -> Result<Arc<[TextAttribute]>, String> {
    let Some(attributes) = actor
        .get::<Option<Table>>("__songlua_text_attributes")
        .map_err(|err| err.to_string())?
    else {
        return Ok(Arc::<[TextAttribute]>::from([]));
    };
    let mut out = Vec::with_capacity(attributes.raw_len());
    for entry in attributes.sequence_values::<Table>() {
        let entry = entry.map_err(|err| err.to_string())?;
        let start_value = entry
            .raw_get::<Value>("start")
            .map_err(|err| err.to_string())?;
        let start = read_i32_value(start_value).unwrap_or(0).max(0) as usize;
        let length_value = entry
            .raw_get::<Value>("length")
            .map_err(|err| err.to_string())?;
        let length_raw = read_i32_value(length_value).unwrap_or(1);
        if length_raw == 0 {
            continue;
        }
        let length = if length_raw < 0 {
            usize::MAX
        } else {
            length_raw as usize
        };
        let color = entry
            .raw_get::<Table>("color")
            .ok()
            .and_then(|color| table_vec4(&color))
            .unwrap_or([1.0, 1.0, 1.0, 1.0]);
        let vertex_colors = entry
            .raw_get::<Table>("vertex_colors")
            .ok()
            .and_then(|colors| table_vertex_colors(&colors));
        let glow = entry
            .raw_get::<Table>("glow")
            .ok()
            .and_then(|color| table_vec4(&color));
        out.push(TextAttribute {
            start,
            length,
            color,
            vertex_colors,
            glow,
        });
    }
    Ok(Arc::from(out.into_boxed_slice()))
}

fn song_lua_uses_builtin_theme_font(raw: &str) -> bool {
    let trimmed = raw.trim().trim_start_matches('/');
    if trimmed.is_empty() {
        return false;
    }
    let normalized = trimmed.replace('\\', "/");
    normalized.starts_with(SONG_LUA_THEME_PATH_PREFIX)
        || (!normalized.contains('/') && Path::new(&normalized).extension().is_none())
}

fn read_actor_color_field(actor: &Table, key: &str) -> Result<Option<[f32; 4]>, String> {
    Ok(actor
        .get::<Option<Table>>(key)
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value)))
}

fn actor_shadow_len(lua: &Lua, actor: &Table) -> mlua::Result<[f32; 2]> {
    let block = actor_current_capture_block(lua, actor)?;
    if let Some(value) = block
        .get::<Option<Table>>("shadow_len")?
        .and_then(|value| table_vec2(&value))
    {
        return Ok(value);
    }
    Ok(actor
        .get::<Option<Table>>("__songlua_state_shadow_len")?
        .and_then(|value| table_vec2(&value))
        .unwrap_or([0.0, 0.0]))
}

fn song_lua_font_name(font_path: &Path) -> &'static str {
    static SONG_LUA_FONT_NAMES: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let canonical = font_path.to_string_lossy().replace('\\', "/");
    let cache = SONG_LUA_FONT_NAMES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(&name) = guard.get(&canonical) {
        return name;
    }
    let leaked = Box::leak(format!("songlua_font:{canonical}").into_boxed_str());
    guard.insert(canonical, leaked);
    leaked
}

fn capture_actor_command(
    lua: &Lua,
    actor: &Table,
    command_name: &str,
) -> Result<Vec<SongLuaOverlayCommandBlock>, String> {
    let Some(command) = actor
        .get::<Option<Function>>(command_name)
        .map_err(|err| err.to_string())?
    else {
        return Ok(Vec::new());
    };
    reset_actor_capture(lua, actor).map_err(|err| err.to_string())?;
    let params =
        default_message_command_params(lua, command_name).map_err(|err| err.to_string())?;
    call_actor_function(lua, actor, &command, params).map_err(|err| err.to_string())?;
    flush_actor_capture(actor).map_err(|err| err.to_string())?;
    read_actor_capture_blocks(actor)
}

fn capture_actor_command_preserving_state(
    lua: &Lua,
    actor: &Table,
    command_name: &str,
) -> Result<Vec<SongLuaOverlayCommandBlock>, String> {
    let snapshot = snapshot_actor_mutable_state(lua, actor).map_err(|err| err.to_string())?;
    let captured = capture_actor_command(lua, actor, command_name);
    restore_actor_mutable_state(actor, snapshot).map_err(|err| err.to_string())?;
    captured
}

fn is_actor_mutable_state_key(key: &str) -> bool {
    key.starts_with("__songlua_state_")
        || key.starts_with("__songlua_capture_")
        || key.starts_with("__songlua_graph_display_")
        || key.starts_with("__songlua_scroller_")
        || matches!(
            key,
            "__songlua_visible"
                | "__songlua_diffuse"
                | "__songlua_text_attributes"
                | "__songlua_stroke_color"
                | "__songlua_aux"
                | "__songlua_aft_capture_name"
                | "__songlua_stream_width"
                | "__songlua_sprite_animation_length_seconds"
                | "__songlua_sprite_effect_mode"
                | "Text"
                | "Texture"
                | "File"
                | "StreamWidth"
        )
}

fn is_actor_semantic_state_key(key: &str) -> bool {
    is_actor_mutable_state_key(key) && !key.starts_with("__songlua_capture_")
}

fn snapshot_actor_mutable_state(lua: &Lua, actor: &Table) -> mlua::Result<Vec<(String, Value)>> {
    snapshot_actor_state(lua, actor, is_actor_mutable_state_key)
}

fn snapshot_actor_semantic_state(lua: &Lua, actor: &Table) -> mlua::Result<Vec<(String, Value)>> {
    snapshot_actor_state(lua, actor, is_actor_semantic_state_key)
}

fn snapshot_actor_state(
    lua: &Lua,
    actor: &Table,
    keep_key: fn(&str) -> bool,
) -> mlua::Result<Vec<(String, Value)>> {
    let mut out = Vec::new();
    for pair in actor.clone().pairs::<Value, Value>() {
        let (key, value) = pair?;
        let Value::String(key) = key else {
            continue;
        };
        let key = key.to_str()?.to_string();
        if keep_key(&key) {
            out.push((key, clone_lua_value(lua, value)?));
        }
    }
    Ok(out)
}

fn restore_actor_mutable_state(actor: &Table, snapshot: Vec<(String, Value)>) -> mlua::Result<()> {
    restore_actor_state(actor, snapshot, is_actor_mutable_state_key)
}

fn restore_actor_semantic_state(actor: &Table, snapshot: Vec<(String, Value)>) -> mlua::Result<()> {
    restore_actor_state(actor, snapshot, is_actor_semantic_state_key)
}

fn restore_actor_state(
    actor: &Table,
    snapshot: Vec<(String, Value)>,
    clear_key: fn(&str) -> bool,
) -> mlua::Result<()> {
    let mut keys = Vec::new();
    for pair in actor.clone().pairs::<Value, Value>() {
        let (key, _) = pair?;
        let Value::String(key) = key else {
            continue;
        };
        let key = key.to_str()?.to_string();
        if clear_key(&key) {
            keys.push(key);
        }
    }
    for key in keys {
        actor.set(key, Value::Nil)?;
    }
    for (key, value) in snapshot {
        actor.set(key, value)?;
    }
    Ok(())
}

fn snapshot_actors_semantic_state(
    lua: &Lua,
    actors: &[Table],
) -> mlua::Result<Vec<(Table, Vec<(String, Value)>)>> {
    actors
        .iter()
        .map(|actor| Ok((actor.clone(), snapshot_actor_semantic_state(lua, actor)?)))
        .collect()
}

fn restore_actors_semantic_state(
    snapshots: Vec<(Table, Vec<(String, Value)>)>,
) -> mlua::Result<()> {
    for (actor, snapshot) in snapshots {
        restore_actor_semantic_state(&actor, snapshot)?;
    }
    Ok(())
}

fn snapshot_actor_semantic_state_table(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    let snapshot = lua.create_table()?;
    for (index, (key, value)) in snapshot_actor_semantic_state(lua, actor)?
        .into_iter()
        .enumerate()
    {
        let entry = lua.create_table()?;
        entry.raw_set(1, key)?;
        entry.raw_set(2, value)?;
        snapshot.raw_set(index + 1, entry)?;
    }
    Ok(snapshot)
}

fn read_actor_semantic_state_table(snapshot: &Table) -> mlua::Result<Vec<(String, Value)>> {
    let mut out = Vec::with_capacity(snapshot.raw_len());
    for entry in snapshot.sequence_values::<Table>() {
        let entry = entry?;
        out.push((entry.raw_get(1)?, entry.raw_get(2)?));
    }
    Ok(out)
}

fn reset_actor_capture(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set("__songlua_capture_cursor", 0.0_f32)?;
    actor.set("__songlua_capture_duration", 0.0_f32)?;
    actor.set("__songlua_capture_tween_time_left", 0.0_f32)?;
    actor.set("__songlua_capture_easing", Value::Nil)?;
    actor.set("__songlua_capture_opt1", Value::Nil)?;
    actor.set("__songlua_capture_opt2", Value::Nil)?;
    actor.set("__songlua_capture_blocks", lua.create_table()?)?;
    actor.set("__songlua_capture_block", Value::Nil)?;
    Ok(())
}

fn actor_current_capture_block(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    record_probe_actor_call(lua, actor)?;
    prepare_capture_scope_actor(lua, actor)?;
    if let Some(block) = actor.get::<Option<Table>>("__songlua_capture_block")? {
        return Ok(block);
    }
    let block = lua.create_table()?;
    let start = actor
        .get::<Option<f32>>("__songlua_capture_cursor")?
        .unwrap_or(0.0);
    let duration = actor
        .get::<Option<f32>>("__songlua_capture_duration")?
        .unwrap_or(0.0)
        .max(0.0);
    block.set("start", start)?;
    block.set("duration", duration)?;
    block.set("easing", actor.get::<Value>("__songlua_capture_easing")?)?;
    block.set("opt1", actor.get::<Value>("__songlua_capture_opt1")?)?;
    block.set("opt2", actor.get::<Value>("__songlua_capture_opt2")?)?;
    block.set("__songlua_has_changes", false)?;
    actor.set("__songlua_capture_block", block.clone())?;
    Ok(block)
}

fn flush_actor_capture(actor: &Table) -> mlua::Result<()> {
    let Some(block) = actor.get::<Option<Table>>("__songlua_capture_block")? else {
        return Ok(());
    };
    let duration = block
        .get::<Option<f32>>("duration")?
        .unwrap_or(0.0)
        .max(0.0);
    let cursor = block.get::<Option<f32>>("start")?.unwrap_or(0.0);
    if block
        .get::<Option<bool>>("__songlua_has_changes")?
        .unwrap_or(false)
    {
        let blocks: Table = actor.get("__songlua_capture_blocks")?;
        let index = blocks.raw_len() + 1;
        blocks.raw_set(index, block.clone())?;
    }
    actor.set("__songlua_capture_cursor", cursor + duration)?;
    actor.set("__songlua_capture_duration", 0.0_f32)?;
    actor.set("__songlua_capture_easing", Value::Nil)?;
    actor.set("__songlua_capture_opt1", Value::Nil)?;
    actor.set("__songlua_capture_opt2", Value::Nil)?;
    actor.set("__songlua_capture_block", Value::Nil)?;
    Ok(())
}

fn actor_tween_time_left(actor: &Table) -> mlua::Result<f32> {
    Ok(actor
        .get::<Option<f32>>("__songlua_capture_tween_time_left")?
        .unwrap_or(0.0)
        .max(0.0))
}

fn scale_actor_capture_f32(actor: &Table, key: &str, scale: f32) -> mlua::Result<()> {
    let value = actor.get::<Option<f32>>(key)?.unwrap_or(0.0);
    actor.set(key, value * scale)
}

fn hurry_actor_tweening(actor: &Table, factor: f32) -> mlua::Result<()> {
    if !factor.is_finite() || factor <= f32::EPSILON {
        return Ok(());
    }
    flush_actor_capture(actor)?;
    let scale = factor.recip();
    if let Some(blocks) = actor.get::<Option<Table>>("__songlua_capture_blocks")? {
        for block in blocks.sequence_values::<Table>() {
            let block = block?;
            let start = block.get::<Option<f32>>("start")?.unwrap_or(0.0);
            let duration = block.get::<Option<f32>>("duration")?.unwrap_or(0.0);
            block.set("start", start * scale)?;
            block.set("duration", duration * scale)?;
        }
    }
    scale_actor_capture_f32(actor, "__songlua_capture_cursor", scale)?;
    scale_actor_capture_f32(actor, "__songlua_capture_duration", scale)?;
    scale_actor_capture_f32(actor, "__songlua_capture_tween_time_left", scale)
}

fn is_capture_block_meta_key(key: &str) -> bool {
    matches!(
        key,
        "start" | "duration" | "easing" | "opt1" | "opt2" | "__songlua_has_changes"
    )
}

fn finish_actor_tweening(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    flush_actor_capture(actor)?;
    let final_block = lua.create_table()?;
    let mut has_changes = false;
    if let Some(blocks) = actor.get::<Option<Table>>("__songlua_capture_blocks")? {
        for block in blocks.sequence_values::<Table>() {
            for pair in block?.pairs::<Value, Value>() {
                let (key, value) = pair?;
                if matches!(
                    &key,
                    Value::String(text)
                        if text
                            .to_str()
                            .ok()
                            .is_some_and(|name| is_capture_block_meta_key(&name))
                ) {
                    continue;
                }
                final_block.set(clone_lua_value(lua, key)?, clone_lua_value(lua, value)?)?;
                has_changes = true;
            }
        }
    }

    let blocks = lua.create_table()?;
    if has_changes {
        final_block.set("start", 0.0_f32)?;
        final_block.set("duration", 0.0_f32)?;
        final_block.set("easing", Value::Nil)?;
        final_block.set("opt1", Value::Nil)?;
        final_block.set("opt2", Value::Nil)?;
        final_block.set("__songlua_has_changes", true)?;
        blocks.raw_set(1, final_block)?;
    }
    actor.set("__songlua_capture_blocks", blocks)?;
    actor.set("__songlua_capture_block", Value::Nil)?;
    actor.set("__songlua_capture_cursor", 0.0_f32)?;
    actor.set("__songlua_capture_duration", 0.0_f32)?;
    actor.set("__songlua_capture_tween_time_left", 0.0_f32)?;
    actor.set("__songlua_capture_easing", Value::Nil)?;
    actor.set("__songlua_capture_opt1", Value::Nil)?;
    actor.set("__songlua_capture_opt2", Value::Nil)?;
    Ok(())
}

fn capture_block_set_f32(lua: &Lua, actor: &Table, key: &str, value: f32) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    block.set(key, value)?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

fn capture_block_set_bool(lua: &Lua, actor: &Table, key: &str, value: bool) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    block.set(key, value)?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

fn capture_block_set_color(lua: &Lua, actor: &Table, color: [f32; 4]) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = lua.create_table()?;
    value.raw_set(1, color[0])?;
    value.raw_set(2, color[1])?;
    value.raw_set(3, color[2])?;
    value.raw_set(4, color[3])?;
    block.set("diffuse", value.clone())?;
    block.set("__songlua_has_changes", true)?;
    actor.set("__songlua_diffuse", value.clone())?;
    actor.set("__songlua_state_diffuse", value)?;
    Ok(())
}

fn actor_diffuse(actor: &Table) -> mlua::Result<[f32; 4]> {
    Ok(actor
        .get::<Option<Table>>("__songlua_diffuse")?
        .and_then(|value| table_vec4(&value))
        .unwrap_or([1.0, 1.0, 1.0, 1.0]))
}

fn actor_vertex_colors(actor: &Table) -> mlua::Result<[[f32; 4]; 4]> {
    Ok(actor
        .get::<Option<Table>>("__songlua_state_vertex_colors")?
        .and_then(|value| table_vertex_colors(&value))
        .unwrap_or([[1.0, 1.0, 1.0, 1.0]; 4]))
}

fn capture_actor_vertex_diffuse(
    lua: &Lua,
    actor: &Table,
    args: &MultiValue,
    corner_mask: u8,
) -> mlua::Result<()> {
    let Some(color) = read_color_args(args) else {
        return Ok(());
    };
    let mut colors = actor_vertex_colors(actor)?;
    for (index, vertex_color) in colors.iter_mut().enumerate() {
        if corner_mask & (1 << index) != 0 {
            *vertex_color = color;
        }
    }
    capture_block_set_vertex_colors(lua, actor, colors)
}

fn actor_text_attributes_table(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    if let Some(attributes) = actor.get::<Option<Table>>("__songlua_text_attributes")? {
        return Ok(attributes);
    }
    let attributes = lua.create_table()?;
    actor.set("__songlua_text_attributes", attributes.clone())?;
    Ok(attributes)
}

fn capture_actor_text_attribute(lua: &Lua, actor: &Table, args: &MultiValue) -> mlua::Result<()> {
    let Some(start) = method_arg(args, 0).cloned().and_then(read_i32_value) else {
        return Ok(());
    };
    let Some(Value::Table(params)) = method_arg(args, 1).cloned() else {
        return Ok(());
    };
    let length = text_attribute_value(&params, &["Length", "length"])?
        .and_then(read_i32_value)
        .unwrap_or(1);
    if length == 0 {
        return Ok(());
    }
    let mut vertex_colors = text_attribute_value(&params, &["Diffuses", "diffuses"])?
        .and_then(read_vertex_colors_value)
        .unwrap_or([[1.0, 1.0, 1.0, 1.0]; 4]);
    if let Some(color) = text_attribute_value(
        &params,
        &[
            "Diffuse",
            "diffuse",
            "DiffuseColor",
            "diffusecolor",
            "Color",
            "color",
        ],
    )?
    .and_then(read_color_value)
    {
        vertex_colors = [color; 4];
    }
    let color = vertex_colors[0];
    let glow = text_attribute_value(&params, &["Glow", "glow"])?.and_then(read_color_value);

    let attr = lua.create_table()?;
    attr.raw_set("start", start.max(0))?;
    attr.raw_set("length", length)?;
    attr.raw_set("color", make_color_table(lua, color)?)?;
    if vertex_colors != [color; 4] {
        attr.raw_set(
            "vertex_colors",
            make_vertex_color_table(lua, vertex_colors)?,
        )?;
    }
    if let Some(glow) = glow {
        attr.raw_set("glow", make_color_table(lua, glow)?)?;
    }
    let attributes = actor_text_attributes_table(lua, actor)?;
    for existing in attributes.sequence_values::<Table>() {
        if text_attribute_matches(&existing?, start.max(0), length, color, vertex_colors, glow)? {
            return Ok(());
        }
    }
    attributes.raw_set(attributes.raw_len() + 1, attr)?;
    Ok(())
}

fn text_attribute_matches(
    attr: &Table,
    start: i32,
    length: i32,
    color: [f32; 4],
    vertex_colors: [[f32; 4]; 4],
    glow: Option<[f32; 4]>,
) -> mlua::Result<bool> {
    if attr.raw_get::<Option<i32>>("start")?.unwrap_or(-1) != start
        || attr.raw_get::<Option<i32>>("length")?.unwrap_or(0) != length
    {
        return Ok(false);
    }
    let existing_color = attr
        .raw_get::<Option<Table>>("color")?
        .and_then(|value| table_vec4(&value))
        .unwrap_or([1.0; 4]);
    let existing_vertex_colors = attr
        .raw_get::<Option<Table>>("vertex_colors")?
        .and_then(|value| table_vertex_colors(&value))
        .unwrap_or([existing_color; 4]);
    let existing_glow = attr
        .raw_get::<Option<Table>>("glow")?
        .and_then(|value| table_vec4(&value));
    Ok(existing_color == color && existing_vertex_colors == vertex_colors && existing_glow == glow)
}

fn text_attribute_value(params: &Table, keys: &[&str]) -> mlua::Result<Option<Value>> {
    for key in keys {
        let value = params.get::<Value>(*key)?;
        if !matches!(value, Value::Nil) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn actor_glow(actor: &Table) -> mlua::Result<[f32; 4]> {
    Ok(actor
        .get::<Option<Table>>("__songlua_state_glow")?
        .and_then(|value| table_vec4(&value))
        .unwrap_or([0.0, 0.0, 0.0, 0.0]))
}

fn actor_effect_magnitude(actor: &Table) -> mlua::Result<[f32; 3]> {
    Ok(actor
        .get::<Option<Table>>("__songlua_state_effect_magnitude")?
        .and_then(|value| table_vec3(&value))
        .unwrap_or([0.0, 0.0, 0.0]))
}

fn capture_block_set_vec4(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value4: [f32; 4],
) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = lua.create_table()?;
    value.raw_set(1, value4[0])?;
    value.raw_set(2, value4[1])?;
    value.raw_set(3, value4[2])?;
    value.raw_set(4, value4[3])?;
    block.set(key, value.clone())?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

fn capture_block_set_vertex_colors(
    lua: &Lua,
    actor: &Table,
    colors: [[f32; 4]; 4],
) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = make_vertex_color_table(lua, colors)?;
    block.set("vertex_colors", value.clone())?;
    block.set("__songlua_has_changes", true)?;
    actor.set("__songlua_state_vertex_colors", value)?;
    Ok(())
}

fn capture_block_set_vec5(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value5: [f32; 5],
) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = lua.create_table()?;
    value.raw_set(1, value5[0])?;
    value.raw_set(2, value5[1])?;
    value.raw_set(3, value5[2])?;
    value.raw_set(4, value5[3])?;
    value.raw_set(5, value5[4])?;
    block.set(key, value.clone())?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

fn capture_block_set_u32(lua: &Lua, actor: &Table, key: &str, value: u32) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    block.set(key, value)?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

fn capture_block_set_i32(lua: &Lua, actor: &Table, key: &str, value: i32) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    block.set(key, value)?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

fn capture_block_set_vec2(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value2: [f32; 2],
) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = lua.create_table()?;
    value.raw_set(1, value2[0])?;
    value.raw_set(2, value2[1])?;
    block.set(key, value.clone())?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

fn capture_block_set_stretch(lua: &Lua, actor: &Table, rect: [f32; 4]) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = lua.create_table()?;
    value.raw_set(1, rect[0])?;
    value.raw_set(2, rect[1])?;
    value.raw_set(3, rect[2])?;
    value.raw_set(4, rect[3])?;
    block.set("stretch_rect", value.clone())?;
    block.set("__songlua_has_changes", true)?;
    actor.set("__songlua_state_stretch_rect", value)?;
    Ok(())
}

fn capture_block_set_size(lua: &Lua, actor: &Table, size: [f32; 2]) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = lua.create_table()?;
    value.raw_set(1, size[0])?;
    value.raw_set(2, size[1])?;
    block.set("size", value.clone())?;
    block.set("__songlua_has_changes", true)?;
    actor.set("__songlua_state_size", value)?;
    Ok(())
}

fn capture_block_set_zoom_axes(
    lua: &Lua,
    actor: &Table,
    zoom: f32,
    zoom_x_key: &str,
    zoom_y_key: &str,
    zoom_z_key: &str,
) -> mlua::Result<()> {
    capture_block_set_f32(lua, actor, zoom_x_key, zoom)?;
    capture_block_set_f32(lua, actor, zoom_y_key, zoom)?;
    capture_block_set_f32(lua, actor, zoom_z_key, zoom)?;
    Ok(())
}

fn capture_block_set_vec3(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value3: [f32; 3],
) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    let value = lua.create_table()?;
    value.raw_set(1, value3[0])?;
    value.raw_set(2, value3[1])?;
    value.raw_set(3, value3[2])?;
    block.set(key, value.clone())?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

fn capture_block_set_string(lua: &Lua, actor: &Table, key: &str, value: &str) -> mlua::Result<()> {
    let block = actor_current_capture_block(lua, actor)?;
    block.set(key, value)?;
    block.set("__songlua_has_changes", true)?;
    actor.set(format!("__songlua_state_{key}"), value)?;
    Ok(())
}

#[inline(always)]
fn effect_clock_label(clock: EffectClock) -> &'static str {
    match clock {
        EffectClock::Time => "time",
        EffectClock::Beat => "beat",
    }
}

#[inline(always)]
fn text_glow_mode_label(mode: SongLuaTextGlowMode) -> &'static str {
    match mode {
        SongLuaTextGlowMode::Inner => "inner",
        SongLuaTextGlowMode::Stroke => "stroke",
        SongLuaTextGlowMode::Both => "both",
    }
}

#[inline(always)]
fn song_lua_valid_sprite_state_index(index: Option<u32>) -> Option<u32> {
    index.filter(|&value| value != SONG_LUA_SPRITE_STATE_CLEAR)
}

fn actor_sprite_sheet_dims(actor: &Table) -> mlua::Result<Option<(u32, u32)>> {
    if let Some(path) = actor_texture_path(actor)? {
        return Ok(Some(crate::assets::parse_sprite_sheet_dims(
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
    Ok(Some(crate::assets::parse_sprite_sheet_dims(texture)))
}

#[inline(always)]
fn sprite_sheet_rect(index: u32, cols: u32, rows: u32) -> [f32; 4] {
    let cols = cols.max(1);
    let rows = rows.max(1);
    let col = index % cols;
    let row = (index / cols).min(rows.saturating_sub(1));
    let width = 1.0 / cols as f32;
    let height = 1.0 / rows as f32;
    let left = col as f32 * width;
    let top = row as f32 * height;
    [left, top, left + width, top + height]
}

fn actor_texture_rect(actor: &Table) -> mlua::Result<[f32; 4]> {
    if let Some(rect) = actor
        .get::<Option<Table>>("__songlua_state_custom_texture_rect")?
        .and_then(|value| table_vec4(&value))
    {
        return Ok(rect);
    }
    if let Some(state_index) = song_lua_valid_sprite_state_index(
        actor.get::<Option<u32>>("__songlua_state_sprite_state_index")?,
    ) && let Some((cols, rows)) = actor_sprite_sheet_dims(actor)?
    {
        return Ok(sprite_sheet_rect(state_index, cols, rows));
    }
    Ok([0.0, 0.0, 1.0, 1.0])
}

fn capture_texture_rect(lua: &Lua, actor: &Table, rect: [f32; 4]) -> mlua::Result<()> {
    capture_block_set_u32(
        lua,
        actor,
        "sprite_state_index",
        SONG_LUA_SPRITE_STATE_CLEAR,
    )?;
    capture_block_set_vec4(lua, actor, "custom_texture_rect", rect)
}

fn offset_texture_rect(lua: &Lua, actor: &Table, dx: f32, dy: f32) -> mlua::Result<()> {
    let [u0, v0, u1, v1] = actor_texture_rect(actor)?;
    capture_texture_rect(lua, actor, [u0 + dx, v0 + dy, u1 + dx, v1 + dy])
}

fn actor_halign(actor: &Table) -> mlua::Result<f32> {
    Ok(actor
        .get::<Option<f32>>("__songlua_state_halign")?
        .unwrap_or(0.5))
}

fn actor_valign(actor: &Table) -> mlua::Result<f32> {
    Ok(actor
        .get::<Option<f32>>("__songlua_state_valign")?
        .unwrap_or(0.5))
}

fn scale_actor_to_rect(lua: &Lua, actor: &Table, rect: [f32; 4], cover: bool) -> mlua::Result<()> {
    let width = rect[2] - rect[0];
    let height = rect[3] - rect[1];
    let (base_width, base_height) = actor_base_size(actor)?;
    if base_width.abs() <= f32::EPSILON || base_height.abs() <= f32::EPSILON {
        return Ok(());
    }
    let zoom_x = (width / base_width).abs();
    let zoom_y = (height / base_height).abs();
    let zoom = if cover {
        zoom_x.max(zoom_y)
    } else {
        zoom_x.min(zoom_y)
    };
    if !zoom.is_finite() {
        return Ok(());
    }

    capture_block_set_f32(lua, actor, "x", rect[0] + width * actor_halign(actor)?)?;
    capture_block_set_f32(lua, actor, "y", rect[1] + height * actor_valign(actor)?)?;
    capture_block_set_f32(lua, actor, "zoom", zoom)?;
    capture_block_set_zoom_axes(lua, actor, zoom, "zoom_x", "zoom_y", "zoom_z")?;
    actor_update_text_pre_zoom_flags(lua, actor, true, true)?;
    if width < 0.0 {
        capture_block_set_f32(lua, actor, "rot_y_deg", 180.0)?;
    }
    if height < 0.0 {
        capture_block_set_f32(lua, actor, "rot_x_deg", 180.0)?;
    }
    Ok(())
}

fn set_actor_sprite_state(lua: &Lua, actor: &Table, state_index: u32) -> mlua::Result<()> {
    capture_block_set_u32(lua, actor, "sprite_state_index", state_index)?;
    let block = actor_current_capture_block(lua, actor)?;
    block.set("custom_texture_rect", Value::Nil)?;
    block.set("__songlua_has_changes", true)?;
    actor.set("__songlua_state_custom_texture_rect", Value::Nil)?;
    Ok(())
}

fn set_actor_seconds_into_animation(lua: &Lua, actor: &Table, seconds: f32) -> mlua::Result<()> {
    let delay = actor
        .get::<Option<f32>>("__songlua_state_sprite_state_delay")?
        .unwrap_or(0.1)
        .max(0.0);
    let frame_count = actor_sprite_frame_count(actor)?.max(1);
    let state = if delay <= f32::EPSILON {
        0
    } else {
        ((seconds.max(0.0) / delay).floor() as u32) % frame_count
    };
    set_actor_sprite_state(lua, actor, state)
}

fn set_actor_effect_mode(lua: &Lua, actor: &Table, mode: &str) -> mlua::Result<()> {
    capture_block_set_string(lua, actor, "effect_mode", mode)
}

fn set_actor_effect_defaults(
    lua: &Lua,
    actor: &Table,
    mode: &str,
    period: Option<f32>,
    magnitude: Option<[f32; 3]>,
    color1: Option<[f32; 4]>,
    color2: Option<[f32; 4]>,
) -> mlua::Result<()> {
    set_actor_effect_mode(lua, actor, mode)?;
    if let Some(value) = period {
        capture_block_set_f32(lua, actor, "effect_period", value)?;
    }
    if let Some(value) = magnitude {
        capture_block_set_vec3(lua, actor, "effect_magnitude", value)?;
    }
    if let Some(value) = color1 {
        capture_block_set_vec4(lua, actor, "effect_color1", value)?;
    }
    if let Some(value) = color2 {
        capture_block_set_vec4(lua, actor, "effect_color2", value)?;
    }
    Ok(())
}

fn actor_sprite_frame_count(actor: &Table) -> mlua::Result<u32> {
    let Some((cols, rows)) = actor_sprite_sheet_dims(actor)? else {
        return Ok(1);
    };
    Ok(cols.max(1).saturating_mul(rows.max(1)).max(1))
}

fn read_actor_capture_blocks(actor: &Table) -> Result<Vec<SongLuaOverlayCommandBlock>, String> {
    let Some(blocks) = actor
        .get::<Option<Table>>("__songlua_capture_blocks")
        .map_err(|err| err.to_string())?
    else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for value in blocks.sequence_values::<Value>() {
        let Value::Table(block) = value.map_err(|err| err.to_string())? else {
            continue;
        };
        let start = block
            .get::<Option<f32>>("start")
            .map_err(|err| err.to_string())?
            .unwrap_or(0.0);
        let duration = block
            .get::<Option<f32>>("duration")
            .map_err(|err| err.to_string())?
            .unwrap_or(0.0)
            .max(0.0);
        let easing = block
            .get::<Option<String>>("easing")
            .map_err(|err| err.to_string())?;
        out.push(SongLuaOverlayCommandBlock {
            start,
            duration,
            easing,
            opt1: block
                .get::<Option<f32>>("opt1")
                .map_err(|err| err.to_string())?,
            opt2: block
                .get::<Option<f32>>("opt2")
                .map_err(|err| err.to_string())?,
            delta: SongLuaOverlayStateDelta {
                x: block
                    .get::<Option<f32>>("x")
                    .map_err(|err| err.to_string())?,
                y: block
                    .get::<Option<f32>>("y")
                    .map_err(|err| err.to_string())?,
                z: block
                    .get::<Option<f32>>("z")
                    .map_err(|err| err.to_string())?,
                z_bias: block
                    .get::<Option<f32>>("z_bias")
                    .map_err(|err| err.to_string())?,
                draw_order: block
                    .get::<Option<i32>>("draw_order")
                    .map_err(|err| err.to_string())?,
                draw_by_z_position: block
                    .get::<Option<bool>>("draw_by_z_position")
                    .map_err(|err| err.to_string())?,
                halign: block
                    .get::<Option<f32>>("halign")
                    .map_err(|err| err.to_string())?,
                valign: block
                    .get::<Option<f32>>("valign")
                    .map_err(|err| err.to_string())?,
                text_align: block
                    .get::<Option<String>>("text_align")
                    .map_err(|err| err.to_string())?
                    .as_deref()
                    .and_then(parse_overlay_text_align),
                uppercase: block
                    .get::<Option<bool>>("uppercase")
                    .map_err(|err| err.to_string())?,
                shadow_len: block
                    .get::<Option<Table>>("shadow_len")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec2(&value)),
                shadow_color: block
                    .get::<Option<Table>>("shadow_color")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                glow: block
                    .get::<Option<Table>>("glow")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                fov: block
                    .get::<Option<f32>>("fov")
                    .map_err(|err| err.to_string())?,
                vanishpoint: block
                    .get::<Option<Table>>("vanishpoint")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec2(&value)),
                diffuse: block
                    .get::<Option<Table>>("diffuse")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                vertex_colors: block
                    .get::<Option<Table>>("vertex_colors")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vertex_colors(&value)),
                visible: block
                    .get::<Option<bool>>("visible")
                    .map_err(|err| err.to_string())?,
                cropleft: block
                    .get::<Option<f32>>("cropleft")
                    .map_err(|err| err.to_string())?,
                cropright: block
                    .get::<Option<f32>>("cropright")
                    .map_err(|err| err.to_string())?,
                croptop: block
                    .get::<Option<f32>>("croptop")
                    .map_err(|err| err.to_string())?,
                cropbottom: block
                    .get::<Option<f32>>("cropbottom")
                    .map_err(|err| err.to_string())?,
                fadeleft: block
                    .get::<Option<f32>>("fadeleft")
                    .map_err(|err| err.to_string())?,
                faderight: block
                    .get::<Option<f32>>("faderight")
                    .map_err(|err| err.to_string())?,
                fadetop: block
                    .get::<Option<f32>>("fadetop")
                    .map_err(|err| err.to_string())?,
                fadebottom: block
                    .get::<Option<f32>>("fadebottom")
                    .map_err(|err| err.to_string())?,
                mask_source: block
                    .get::<Option<bool>>("mask_source")
                    .map_err(|err| err.to_string())?,
                mask_dest: block
                    .get::<Option<bool>>("mask_dest")
                    .map_err(|err| err.to_string())?,
                depth_test: block
                    .get::<Option<bool>>("depth_test")
                    .map_err(|err| err.to_string())?,
                zoom: block
                    .get::<Option<f32>>("zoom")
                    .map_err(|err| err.to_string())?,
                zoom_x: block
                    .get::<Option<f32>>("zoom_x")
                    .map_err(|err| err.to_string())?,
                zoom_y: block
                    .get::<Option<f32>>("zoom_y")
                    .map_err(|err| err.to_string())?,
                zoom_z: block
                    .get::<Option<f32>>("zoom_z")
                    .map_err(|err| err.to_string())?,
                basezoom: block
                    .get::<Option<f32>>("basezoom")
                    .map_err(|err| err.to_string())?,
                basezoom_x: block
                    .get::<Option<f32>>("basezoom_x")
                    .map_err(|err| err.to_string())?,
                basezoom_y: block
                    .get::<Option<f32>>("basezoom_y")
                    .map_err(|err| err.to_string())?,
                basezoom_z: block
                    .get::<Option<f32>>("basezoom_z")
                    .map_err(|err| err.to_string())?,
                rot_x_deg: block
                    .get::<Option<f32>>("rot_x_deg")
                    .map_err(|err| err.to_string())?,
                rot_y_deg: block
                    .get::<Option<f32>>("rot_y_deg")
                    .map_err(|err| err.to_string())?,
                rot_z_deg: block
                    .get::<Option<f32>>("rot_z_deg")
                    .map_err(|err| err.to_string())?,
                skew_x: block
                    .get::<Option<f32>>("skew_x")
                    .map_err(|err| err.to_string())?,
                skew_y: block
                    .get::<Option<f32>>("skew_y")
                    .map_err(|err| err.to_string())?,
                blend: block
                    .get::<Option<String>>("blend")
                    .map_err(|err| err.to_string())?
                    .as_deref()
                    .and_then(parse_overlay_blend_mode),
                vibrate: block
                    .get::<Option<bool>>("vibrate")
                    .map_err(|err| err.to_string())?,
                effect_magnitude: block
                    .get::<Option<Table>>("effect_magnitude")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec3(&value)),
                effect_clock: block
                    .get::<Option<String>>("effect_clock")
                    .map_err(|err| err.to_string())?
                    .as_deref()
                    .and_then(parse_overlay_effect_clock),
                effect_mode: block
                    .get::<Option<String>>("effect_mode")
                    .map_err(|err| err.to_string())?
                    .as_deref()
                    .and_then(parse_overlay_effect_mode),
                effect_color1: block
                    .get::<Option<Table>>("effect_color1")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                effect_color2: block
                    .get::<Option<Table>>("effect_color2")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                effect_period: block
                    .get::<Option<f32>>("effect_period")
                    .map_err(|err| err.to_string())?,
                effect_offset: block
                    .get::<Option<f32>>("effect_offset")
                    .map_err(|err| err.to_string())?,
                effect_timing: block
                    .get::<Option<Table>>("effect_timing")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec5(&value)),
                rainbow: block
                    .get::<Option<bool>>("rainbow")
                    .map_err(|err| err.to_string())?,
                rainbow_scroll: block
                    .get::<Option<bool>>("rainbow_scroll")
                    .map_err(|err| err.to_string())?,
                text_jitter: block
                    .get::<Option<bool>>("text_jitter")
                    .map_err(|err| err.to_string())?,
                text_distortion: block
                    .get::<Option<f32>>("text_distortion")
                    .map_err(|err| err.to_string())?,
                text_glow_mode: block
                    .get::<Option<String>>("text_glow_mode")
                    .map_err(|err| err.to_string())?
                    .as_deref()
                    .and_then(parse_overlay_text_glow_mode),
                mult_attrs_with_diffuse: block
                    .get::<Option<bool>>("mult_attrs_with_diffuse")
                    .map_err(|err| err.to_string())?,
                sprite_animate: block
                    .get::<Option<bool>>("sprite_animate")
                    .map_err(|err| err.to_string())?,
                sprite_loop: block
                    .get::<Option<bool>>("sprite_loop")
                    .map_err(|err| err.to_string())?,
                sprite_playback_rate: block
                    .get::<Option<f32>>("sprite_playback_rate")
                    .map_err(|err| err.to_string())?,
                sprite_state_delay: block
                    .get::<Option<f32>>("sprite_state_delay")
                    .map_err(|err| err.to_string())?,
                sprite_state_index: block
                    .get::<Option<u32>>("sprite_state_index")
                    .map_err(|err| err.to_string())?,
                vert_spacing: block
                    .get::<Option<i32>>("vert_spacing")
                    .map_err(|err| err.to_string())?,
                wrap_width_pixels: block
                    .get::<Option<i32>>("wrap_width_pixels")
                    .map_err(|err| err.to_string())?,
                max_width: block
                    .get::<Option<f32>>("max_width")
                    .map_err(|err| err.to_string())?,
                max_height: block
                    .get::<Option<f32>>("max_height")
                    .map_err(|err| err.to_string())?,
                max_w_pre_zoom: block
                    .get::<Option<bool>>("max_w_pre_zoom")
                    .map_err(|err| err.to_string())?,
                max_h_pre_zoom: block
                    .get::<Option<bool>>("max_h_pre_zoom")
                    .map_err(|err| err.to_string())?,
                max_dimension_uses_zoom: block
                    .get::<Option<bool>>("max_dimension_uses_zoom")
                    .map_err(|err| err.to_string())?,
                texture_filtering: block
                    .get::<Option<bool>>("texture_filtering")
                    .map_err(|err| err.to_string())?,
                texture_wrapping: block
                    .get::<Option<bool>>("texture_wrapping")
                    .map_err(|err| err.to_string())?,
                texcoord_offset: block
                    .get::<Option<Table>>("texcoord_offset")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec2(&value)),
                custom_texture_rect: block
                    .get::<Option<Table>>("custom_texture_rect")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                texcoord_velocity: block
                    .get::<Option<Table>>("texcoord_velocity")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec2(&value)),
                size: block
                    .get::<Option<Table>>("size")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec2(&value)),
                stretch_rect: block
                    .get::<Option<Table>>("stretch_rect")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                sound_play: block
                    .get::<Option<bool>>("sound_play")
                    .map_err(|err| err.to_string())?,
            },
        });
    }
    Ok(out)
}

fn table_vec4(table: &Table) -> Option<[f32; 4]> {
    Some([
        table.raw_get::<f32>(1).ok()?,
        table.raw_get::<f32>(2).ok()?,
        table.raw_get::<f32>(3).ok()?,
        table.raw_get::<f32>(4).ok()?,
    ])
}

fn table_vertex_colors(table: &Table) -> Option<[[f32; 4]; 4]> {
    Some([
        table_vec4(&table.raw_get::<Table>(1).ok()?)?,
        table_vec4(&table.raw_get::<Table>(2).ok()?)?,
        table_vec4(&table.raw_get::<Table>(3).ok()?)?,
        table_vec4(&table.raw_get::<Table>(4).ok()?)?,
    ])
}

fn make_vertex_color_table(lua: &Lua, colors: [[f32; 4]; 4]) -> mlua::Result<Table> {
    let out = lua.create_table()?;
    for (index, color) in colors.into_iter().enumerate() {
        out.raw_set(index + 1, make_color_table(lua, color)?)?;
    }
    Ok(out)
}

fn table_vec5(table: &Table) -> Option<[f32; 5]> {
    Some([
        table.raw_get::<f32>(1).ok()?,
        table.raw_get::<f32>(2).ok()?,
        table.raw_get::<f32>(3).ok()?,
        table.raw_get::<f32>(4).ok()?,
        table.raw_get::<f32>(5).ok()?,
    ])
}

fn table_vec2(table: &Table) -> Option<[f32; 2]> {
    Some([table.raw_get::<f32>(1).ok()?, table.raw_get::<f32>(2).ok()?])
}

fn table_vec3(table: &Table) -> Option<[f32; 3]> {
    Some([
        table.raw_get::<f32>(1).ok()?,
        table.raw_get::<f32>(2).ok()?,
        table.raw_get::<f32>(3).ok()?,
    ])
}

fn song_lua_halign_value(value: &Value) -> Option<f32> {
    read_f32(value.clone()).or_else(|| {
        read_string(value.clone()).and_then(|raw| {
            match song_lua_align_token(raw.as_str()).as_str() {
                "left" => Some(0.0),
                "center" | "middle" => Some(0.5),
                "right" => Some(1.0),
                _ => None,
            }
        })
    })
}

fn song_lua_valign_value(value: &Value) -> Option<f32> {
    read_f32(value.clone()).or_else(|| {
        read_string(value.clone()).and_then(|raw| {
            match song_lua_align_token(raw.as_str()).as_str() {
                "top" => Some(0.0),
                "center" | "middle" => Some(0.5),
                "bottom" => Some(1.0),
                _ => None,
            }
        })
    })
}

fn song_lua_align_token(raw: &str) -> String {
    raw.trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase()
        .trim_start_matches("horizalign_")
        .trim_start_matches("vertalign_")
        .to_string()
}

fn song_lua_text_align_value(value: &Value) -> Option<TextAlign> {
    read_string(value.clone()).and_then(|raw| parse_overlay_text_align(raw.as_str()))
}

fn overlay_text_align_label(value: TextAlign) -> &'static str {
    match value {
        TextAlign::Left => "left",
        TextAlign::Center => "center",
        TextAlign::Right => "right",
    }
}

fn actor_type_is(actor: &Table, expected: &str) -> mlua::Result<bool> {
    Ok(actor
        .get::<Option<String>>("__songlua_actor_type")?
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case(expected)))
}

fn actor_is_bitmap_text(actor: &Table) -> mlua::Result<bool> {
    Ok(actor
        .get::<Option<String>>("__songlua_actor_type")?
        .as_deref()
        .is_some_and(|kind| {
            kind.eq_ignore_ascii_case("BitmapText") || kind.eq_ignore_ascii_case("RollingNumbers")
        }))
}

fn actor_update_text_pre_zoom_flags(
    lua: &Lua,
    actor: &Table,
    update_width: bool,
    update_height: bool,
) -> mlua::Result<()> {
    if !actor_is_bitmap_text(actor)? {
        return Ok(());
    }
    if update_width && actor.get::<Option<bool>>("__songlua_text_saw_max_width")? == Some(true) {
        capture_block_set_bool(lua, actor, "max_w_pre_zoom", true)?;
    }
    if update_height && actor.get::<Option<bool>>("__songlua_text_saw_max_height")? == Some(true) {
        capture_block_set_bool(lua, actor, "max_h_pre_zoom", true)?;
    }
    Ok(())
}

pub(super) fn resolve_script_path(lua: &Lua, song_dir: &Path, path: &str) -> mlua::Result<PathBuf> {
    let raw = Path::new(path);
    if raw.is_absolute() && raw.exists() {
        return Ok(raw.to_path_buf());
    }
    let globals = lua.globals();
    if let Some(current_dir) = globals
        .get::<Option<String>>("__songlua_script_dir")?
        .filter(|dir| !dir.trim().is_empty())
    {
        let candidate = Path::new(&current_dir).join(path);
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    let candidate = song_dir.join(path);
    if candidate.exists() {
        return Ok(candidate);
    }
    Err(mlua::Error::external(format!(
        "script '{}' not found relative to '{}'",
        path,
        song_dir.display()
    )))
}

fn call_with_script_dir<T>(
    lua: &Lua,
    script_dir: &Path,
    f: impl FnOnce() -> mlua::Result<T>,
) -> mlua::Result<T> {
    let globals = lua.globals();
    let previous = globals.get::<Value>("__songlua_script_dir")?;
    globals.set(
        "__songlua_script_dir",
        script_dir.to_string_lossy().as_ref(),
    )?;
    let result = f();
    globals.set("__songlua_script_dir", previous)?;
    result
}

fn call_with_script_path<T>(
    lua: &Lua,
    script_path: &str,
    f: impl FnOnce() -> mlua::Result<T>,
) -> mlua::Result<T> {
    let globals = lua.globals();
    let previous = globals.get::<Value>("__songlua_current_script_path")?;
    globals.set("__songlua_current_script_path", script_path)?;
    let result = f();
    globals.set("__songlua_current_script_path", previous)?;
    result
}

fn call_with_chunk_env<T>(
    lua: &Lua,
    chunk_env: &Table,
    f: impl FnOnce() -> mlua::Result<T>,
) -> mlua::Result<T> {
    let globals = lua.globals();
    let previous = globals.get::<Value>("__songlua_current_chunk_env")?;
    globals.set("__songlua_current_chunk_env", chunk_env.clone())?;
    let result = f();
    globals.set("__songlua_current_chunk_env", previous)?;
    result
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

fn song_lua_actor_registry(lua: &Lua) -> mlua::Result<Table> {
    let globals = lua.globals();
    if let Some(registry) = globals.get::<Option<Table>>("__songlua_actor_registry")? {
        return Ok(registry);
    }
    let registry = lua.create_table()?;
    globals.set("__songlua_actor_registry", registry.clone())?;
    Ok(registry)
}

fn register_song_lua_actor(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let registry = song_lua_actor_registry(lua)?;
    let actor_ptr = actor.to_pointer() as usize;
    for value in registry.sequence_values::<Value>() {
        let Value::Table(existing) = value? else {
            continue;
        };
        if existing.to_pointer() as usize == actor_ptr {
            return Ok(());
        }
    }
    registry.raw_set(registry.raw_len() + 1, actor.clone())?;
    Ok(())
}

fn normalize_broadcast_params(
    lua: &Lua,
    message: &str,
    params: Option<Value>,
) -> mlua::Result<Option<Value>> {
    match params {
        Some(Value::Table(table)) => {
            if message.eq_ignore_ascii_case("Judgment") {
                normalize_judgment_params(lua, &table)?;
            }
            Ok(Some(Value::Table(table)))
        }
        params => Ok(params),
    }
}

fn normalize_judgment_params(lua: &Lua, params: &Table) -> mlua::Result<()> {
    if matches!(params.get::<Value>("Player")?, Value::Nil) {
        params.set("Player", player_number_name(0))?;
    }
    if matches!(params.get::<Value>("TapNoteScore")?, Value::Nil) {
        params.set("TapNoteScore", "TapNoteScore_W3")?;
    }
    if matches!(params.get::<Value>("HoldNoteScore")?, Value::Nil) {
        params.set("HoldNoteScore", "HoldNoteScore_None")?;
    }
    if matches!(params.get::<Value>("TapNoteOffset")?, Value::Nil) {
        params.set("TapNoteOffset", 0.0_f32)?;
    }

    let notes = match params.get::<Value>("Notes")? {
        Value::Table(notes) => notes,
        _ => {
            let notes = lua.create_table()?;
            let first_track = table_i32_field(params, &["FirstTrack", "Column"])?.unwrap_or(0);
            notes.raw_set(
                i64::from(first_track.max(0)) + 1,
                create_tap_note_table(lua, params, None)?,
            )?;
            params.set("Notes", notes.clone())?;
            notes
        }
    };
    if table_has_entries(&notes)? {
        let entries = notes
            .pairs::<Value, Value>()
            .collect::<mlua::Result<Vec<_>>>()?;
        for (key, value) in entries {
            let note = match value {
                Value::Table(note) => normalize_tap_note_table(lua, params, note)?,
                value => create_tap_note_table(lua, params, Some(value))?,
            };
            notes.set(key, note)?;
        }
    } else {
        notes.raw_set(1, create_tap_note_table(lua, params, None)?)?;
    }
    Ok(())
}

fn default_message_command_params(lua: &Lua, command_name: &str) -> mlua::Result<Option<Value>> {
    let Some(message) = command_name.strip_suffix("MessageCommand") else {
        return Ok(None);
    };
    let params = lua.create_table()?;
    match message {
        "Judgment" => {
            params.set("Player", player_number_name(0))?;
            params.set("TapNoteScore", "TapNoteScore_W3")?;
            params.set("TapNoteOffset", 0.0_f32)?;
            params.set("FirstTrack", 0_i64)?;
            normalize_judgment_params(lua, &params)?;
        }
        "EarlyHit" => {
            params.set("Player", player_number_name(0))?;
            params.set("TapNoteScore", "TapNoteScore_W3")?;
            params.set("TapNoteOffset", -0.02_f32)?;
            params.set("Early", true)?;
        }
        "LifeChanged" => {
            params.set("Player", player_number_name(0))?;
            params.set("LifeMeter", create_life_meter_param_table(lua)?)?;
        }
        "HealthStateChanged" => {
            params.set("Player", player_number_name(0))?;
            params.set("PlayerNumber", player_number_name(0))?;
            params.set("HealthState", "HealthState_Alive")?;
        }
        "ExCountsChanged" => {
            params.set("Player", player_number_name(0))?;
            params.set("ExCounts", create_ex_counts_param_table(lua)?)?;
            params.set("ExScore", 0.0_f32)?;
            params.set("ActualPoints", 0.0_f32)?;
            params.set("ActualPossible", 1.0_f32)?;
            params.set("CurrentPossible", 1.0_f32)?;
        }
        "PlayerOptionsChanged" => {
            params.set("Player", player_number_name(0))?;
            params.set("PlayerNumber", player_number_name(0))?;
        }
        _ => return Ok(None),
    }
    Ok(Some(Value::Table(params)))
}

fn create_life_meter_param_table(lua: &Lua) -> mlua::Result<Table> {
    let meter = lua.create_table()?;
    meter.set(
        "GetLife",
        lua.create_function(|_, _args: MultiValue| Ok(SONG_LUA_INITIAL_LIFE))?,
    )?;
    meter.set(
        "IsFailing",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    meter.set(
        "IsHot",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    meter.set(
        "IsInDanger",
        lua.create_function(|_, _args: MultiValue| Ok(false))?,
    )?;
    Ok(meter)
}

fn create_ex_counts_param_table(lua: &Lua) -> mlua::Result<Table> {
    let counts = lua.create_table()?;
    for key in [
        "W0", "W1", "W2", "W3", "W4", "W5", "Miss", "Held", "LetGo", "HitMine",
    ] {
        counts.set(key, 0_i64)?;
    }
    Ok(counts)
}

fn table_has_entries(table: &Table) -> mlua::Result<bool> {
    Ok(table.pairs::<Value, Value>().next().transpose()?.is_some())
}

fn normalize_tap_note_table(lua: &Lua, params: &Table, note: Table) -> mlua::Result<Table> {
    if !matches!(note.get::<Value>("GetTapNoteType")?, Value::Nil) {
        return Ok(note);
    }
    let result = match note.get::<Value>("TapNoteResult")? {
        Value::Table(result) => normalize_tap_note_result_table(lua, params, result, Some(&note))?,
        _ => create_tap_note_result_table(lua, params, Some(&note))?,
    };
    note.set("TapNoteResult", result.clone())?;
    note.set("HoldNoteResult", result.clone())?;
    install_tap_note_methods(lua, params, &note, result)?;
    Ok(note)
}

fn create_tap_note_table(
    lua: &Lua,
    params: &Table,
    note_value: Option<Value>,
) -> mlua::Result<Table> {
    let note = lua.create_table()?;
    if let Some(value) = note_value {
        note.set("TapNoteType", value)?;
    }
    let result = create_tap_note_result_table(lua, params, Some(&note))?;
    note.set("TapNoteResult", result.clone())?;
    note.set("HoldNoteResult", result.clone())?;
    install_tap_note_methods(lua, params, &note, result)?;
    Ok(note)
}

fn install_tap_note_methods(
    lua: &Lua,
    params: &Table,
    note: &Table,
    result: Table,
) -> mlua::Result<()> {
    let note_type = table_string_field(note, &["TapNoteType", "Type", "NoteType"])?
        .or(table_string_field(
            params,
            &["TapNoteType", "Type", "NoteType"],
        )?)
        .unwrap_or_else(|| "TapNoteType_Tap".to_string());
    let source = table_string_field(note, &["TapNoteSource", "Source"])?
        .unwrap_or_else(|| "TapNoteSource_Original".to_string());
    let subtype = table_string_field(note, &["TapNoteSubType", "SubType"])?
        .unwrap_or_else(|| "TapNoteSubType_Hold".to_string());
    let player = table_string_field(params, &["Player"])?
        .unwrap_or_else(|| player_number_name(0).to_string());
    let hold_duration = table_f32_field(note, &["HoldDuration"])?.unwrap_or(0.0);
    let attack_duration = table_f32_field(note, &["AttackDuration"])?.unwrap_or(0.0);
    let attack_mods = table_string_field(note, &["AttackModifiers"])?.unwrap_or_default();
    let keysound = table_i32_field(note, &["KeysoundIndex"])?.unwrap_or(0);

    set_string_method(lua, note, "GetTapNoteType", &note_type)?;
    set_string_method(lua, note, "GetTapNoteSource", &source)?;
    set_string_method(lua, note, "GetTapNoteSubType", &subtype)?;
    set_string_method(lua, note, "GetPlayerNumber", &player)?;
    set_string_method(lua, note, "GetAttackModifiers", &attack_mods)?;
    note.set(
        "GetTapNoteResult",
        lua.create_function({
            let result = result.clone();
            move |_, _args: MultiValue| Ok(result.clone())
        })?,
    )?;
    note.set(
        "GetHoldNoteResult",
        lua.create_function({
            let result = result.clone();
            move |_, _args: MultiValue| Ok(result.clone())
        })?,
    )?;
    note.set(
        "GetHoldDuration",
        lua.create_function(move |_, _args: MultiValue| Ok(hold_duration))?,
    )?;
    note.set(
        "GetAttackDuration",
        lua.create_function(move |_, _args: MultiValue| Ok(attack_duration))?,
    )?;
    note.set(
        "GetKeysoundIndex",
        lua.create_function(move |_, _args: MultiValue| Ok(keysound))?,
    )?;
    Ok(())
}

fn create_tap_note_result_table(
    lua: &Lua,
    params: &Table,
    note: Option<&Table>,
) -> mlua::Result<Table> {
    let result = lua.create_table()?;
    install_tap_note_result_methods(lua, params, note, &result)?;
    Ok(result)
}

fn normalize_tap_note_result_table(
    lua: &Lua,
    params: &Table,
    result: Table,
    note: Option<&Table>,
) -> mlua::Result<Table> {
    if matches!(result.get::<Value>("GetHeld")?, Value::Nil) {
        install_tap_note_result_methods(lua, params, note, &result)?;
    }
    Ok(result)
}

fn install_tap_note_result_methods(
    lua: &Lua,
    params: &Table,
    note: Option<&Table>,
    result: &Table,
) -> mlua::Result<()> {
    let held = match note {
        Some(note) => table_bool_field(note, &["Held", "held"])?,
        None => None,
    }
    .unwrap_or(false);
    let hidden = match note {
        Some(note) => table_bool_field(note, &["Hidden", "hidden"])?,
        None => None,
    }
    .unwrap_or(false);
    let offset = match note {
        Some(note) => table_f32_field(note, &["TapNoteOffset", "Offset"])?,
        None => None,
    }
    .or(table_f32_field(params, &["TapNoteOffset", "Offset"])?)
    .unwrap_or(0.0);
    let score = match note {
        Some(note) => table_string_field(note, &["TapNoteScore", "Score"])?,
        None => None,
    }
    .or(table_string_field(params, &["TapNoteScore", "Score"])?)
    .unwrap_or_else(|| "TapNoteScore_None".to_string());

    result.set(
        "GetHeld",
        lua.create_function(move |_, _args: MultiValue| Ok(held))?,
    )?;
    result.set(
        "GetHidden",
        lua.create_function(move |_, _args: MultiValue| Ok(hidden))?,
    )?;
    result.set(
        "GetTapNoteOffset",
        lua.create_function(move |_, _args: MultiValue| Ok(offset))?,
    )?;
    set_string_method(lua, result, "GetTapNoteScore", &score)?;
    Ok(())
}

fn table_string_field(table: &Table, names: &[&str]) -> mlua::Result<Option<String>> {
    for name in names {
        if let Some(value) = read_string(table.get::<Value>(*name)?) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn table_f32_field(table: &Table, names: &[&str]) -> mlua::Result<Option<f32>> {
    for name in names {
        if let Some(value) = read_f32(table.get::<Value>(*name)?) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn table_i32_field(table: &Table, names: &[&str]) -> mlua::Result<Option<i32>> {
    for name in names {
        if let Some(value) = read_i32_value(table.get::<Value>(*name)?) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn table_bool_field(table: &Table, names: &[&str]) -> mlua::Result<Option<bool>> {
    for name in names {
        if let Some(value) = read_boolish(table.get::<Value>(*name)?) {
            return Ok(Some(value));
        }
    }
    Ok(None)
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

fn inherit_actor_dirs(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if let Some(script_dir) = lua
        .globals()
        .get::<Option<String>>("__songlua_script_dir")?
    {
        actor.set("__songlua_script_dir", script_dir)?;
    }
    if let Some(song_dir) = lua.globals().get::<Option<String>>("__songlua_song_dir")? {
        actor.set("__songlua_song_dir", song_dir)?;
    }
    Ok(())
}

fn set_proxy_target_fields(actor: &Table, target: &Table) -> mlua::Result<()> {
    actor.set("__songlua_proxy_target_kind", Value::Nil)?;
    actor.set("__songlua_proxy_player_index", Value::Nil)?;
    if let Some(child_name) = target.get::<Option<String>>("__songlua_top_screen_child_name")? {
        let kind = if child_name.eq_ignore_ascii_case("Underlay") {
            Some("underlay")
        } else if child_name.eq_ignore_ascii_case("Overlay") {
            Some("overlay")
        } else {
            None
        };
        if let Some(kind) = kind {
            actor.set("__songlua_proxy_target_kind", kind)?;
        }
        return Ok(());
    }
    let Some(player_index) = target.get::<Option<i64>>("__songlua_player_index")? else {
        return Ok(());
    };
    actor.set("__songlua_proxy_player_index", player_index)?;
    let kind = match target
        .get::<Option<String>>("__songlua_player_child_name")?
        .as_deref()
    {
        Some(name) if name.eq_ignore_ascii_case("NoteField") => "notefield",
        Some(name) if name.eq_ignore_ascii_case("Judgment") => "judgment",
        Some(name) if name.eq_ignore_ascii_case("Combo") => "combo",
        _ => "player",
    };
    actor.set("__songlua_proxy_target_kind", kind)?;
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
                if width <= f32::EPSILON || height <= f32::EPSILON {
                    return Ok(actor.clone());
                }
                let dx = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0);
                let dy = method_arg(&args, 1)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0);
                offset_texture_rect(lua, &actor, dx / width, dy / height)?;
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
    if let Some((mut width, mut height)) = actor_image_texture_size(actor)? {
        if actor
            .get::<Option<bool>>("__songlua_state_sprite_animate")?
            .unwrap_or(false)
            || song_lua_valid_sprite_state_index(
                actor.get::<Option<u32>>("__songlua_state_sprite_state_index")?,
            )
            .is_some()
        {
            if let Some(path) = actor_texture_path(actor)? {
                let (cols, rows) =
                    crate::assets::parse_sprite_sheet_dims(path.to_string_lossy().as_ref());
                width /= cols.max(1) as f32;
                height /= rows.max(1) as f32;
            }
        }
        return Ok(Some((width, height)));
    }
    Ok(None)
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

fn crop_texture_rect(source: [f32; 2], target: [f32; 2]) -> Option<[f32; 4]> {
    if !source.iter().all(|value| value.is_finite() && *value > 0.0) {
        return None;
    }
    let scale = (target[0] / source[0]).max(target[1] / source[1]);
    if !scale.is_finite() || scale <= f32::EPSILON {
        return None;
    }
    let zoomed = [source[0] * scale, source[1] * scale];
    if zoomed[0] > target[0] + 0.01 {
        let cut = ((zoomed[0] - target[0]) / zoomed[0]).max(0.0) * 0.5;
        return Some([cut, 0.0, 1.0 - cut, 1.0]);
    }
    let cut = ((zoomed[1] - target[1]) / zoomed[1]).max(0.0) * 0.5;
    Some([0.0, cut, 1.0, 1.0 - cut])
}

fn actor_zoom_axis(actor: &Table, zoom_key: &str, basezoom_key: &str) -> mlua::Result<f32> {
    let zoom = actor
        .get::<Option<f32>>(zoom_key)?
        .or(actor.get::<Option<f32>>("__songlua_state_zoom")?)
        .unwrap_or(1.0);
    let basezoom = actor
        .get::<Option<f32>>(basezoom_key)?
        .or(actor.get::<Option<f32>>("__songlua_state_basezoom")?)
        .unwrap_or(1.0);
    Ok(basezoom * zoom)
}

fn actor_texture_is_video(actor: &Table) -> mlua::Result<bool> {
    if let Some(path) = actor_texture_path(actor)? {
        return Ok(is_song_lua_video_path(&path));
    }
    Ok(actor
        .get::<Option<String>>("Texture")?
        .is_some_and(|texture| is_song_lua_video_path(Path::new(texture.trim()))))
}

fn set_actor_decode_movie_for_texture(actor: &Table) -> mlua::Result<()> {
    actor.set(
        "__songlua_state_decode_movie",
        actor_texture_is_video(actor)?,
    )
}

fn set_actor_texture_from_path(actor: &Table, path: &str) -> mlua::Result<bool> {
    let path = path.trim();
    if path.is_empty() {
        return Ok(false);
    }
    actor.set("Texture", path.to_string())?;
    actor.set("__songlua_aft_capture_name", Value::Nil)?;
    set_actor_decode_movie_for_texture(actor)?;
    Ok(true)
}

fn set_actor_texture_from_path_methods(
    actor: &Table,
    value: Option<&Value>,
    method_names: &[&str],
) -> mlua::Result<bool> {
    let Some(Value::Table(source)) = value else {
        return Ok(false);
    };
    for method_name in method_names {
        if let Value::String(path) = call_table_method(source, method_name)? {
            if set_actor_texture_from_path(actor, &path.to_str()?)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn set_actor_texture_from_path_methods_or_fallback(
    actor: &Table,
    value: Option<&Value>,
    method_names: &[&str],
    fallback_path: &str,
) -> mlua::Result<()> {
    if !set_actor_texture_from_path_methods(actor, value, method_names)? {
        set_actor_texture_from_path(actor, fallback_path)?;
    }
    Ok(())
}

fn banner_sort_order_path(sort_order: &str) -> Option<String> {
    let short = sort_order
        .trim()
        .split_once('_')
        .map(|(_, short)| short)
        .unwrap_or(sort_order.trim());
    let short = short
        .strip_suffix("_P1")
        .or_else(|| short.strip_suffix("_P2"))
        .unwrap_or(short);
    match short {
        "Group" => None,
        "" | "Invalid" => Some(theme_path("G", "Common", "fallback banner")),
        sort => Some(theme_path("G", "Banner", sort)),
    }
}

fn set_actor_texture_from_value(
    actor: &Table,
    value: Option<&Value>,
    clear_on_nil: bool,
) -> mlua::Result<()> {
    match value {
        Some(Value::String(texture)) => {
            actor.set("Texture", texture.to_str()?.to_string())?;
            actor.set("__songlua_aft_capture_name", Value::Nil)?;
            set_actor_decode_movie_for_texture(actor)?;
        }
        Some(Value::Table(texture)) => {
            if let Some(capture_name) =
                texture.get::<Option<String>>("__songlua_aft_capture_name")?
            {
                actor.set("__songlua_aft_capture_name", capture_name)?;
                actor.set("Texture", Value::Nil)?;
                set_actor_decode_movie_for_texture(actor)?;
            } else if let Some(texture_path) = texture
                .get::<Option<String>>("__songlua_texture_path")?
                .filter(|path| !path.is_empty())
            {
                actor.set("Texture", texture_path)?;
                actor.set("__songlua_aft_capture_name", Value::Nil)?;
                set_actor_decode_movie_for_texture(actor)?;
            }
        }
        Some(Value::Nil) | None if clear_on_nil => {
            actor.set("Texture", Value::Nil)?;
            actor.set("__songlua_aft_capture_name", Value::Nil)?;
            actor.set("__songlua_state_decode_movie", false)?;
        }
        _ => {}
    }
    Ok(())
}

fn set_actor_sound_file_from_value(
    actor: &Table,
    value: Option<&Value>,
    clear_on_nil: bool,
) -> mlua::Result<()> {
    match value {
        Some(Value::String(file)) => actor.set("File", file.to_str()?.to_string())?,
        Some(Value::Nil) | None if clear_on_nil => actor.set("File", Value::Nil)?,
        _ => {}
    }
    Ok(())
}

fn actor_decode_movie(actor: &Table) -> mlua::Result<bool> {
    Ok(actor
        .get::<Option<bool>>("__songlua_state_decode_movie")?
        .unwrap_or(actor_texture_is_video(actor)?))
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

fn song_lua_screen_size(lua: &Lua) -> mlua::Result<(f32, f32)> {
    let globals = lua.globals();
    let width = globals.get::<Option<i32>>("SCREEN_WIDTH")?.unwrap_or(640) as f32;
    let height = globals.get::<Option<i32>>("SCREEN_HEIGHT")?.unwrap_or(480) as f32;
    Ok((width, height))
}

fn song_lua_screen_center(lua: &Lua) -> mlua::Result<(f32, f32)> {
    let globals = lua.globals();
    let center_x = globals
        .get::<Option<f32>>("SCREEN_CENTER_X")?
        .unwrap_or(song_lua_screen_size(lua)?.0 * 0.5);
    let center_y = globals
        .get::<Option<f32>>("SCREEN_CENTER_Y")?
        .unwrap_or(song_lua_screen_size(lua)?.1 * 0.5);
    Ok((center_x, center_y))
}

fn actor_texture_path(actor: &Table) -> mlua::Result<Option<PathBuf>> {
    let Some(texture) = actor.get::<Option<String>>("Texture")? else {
        return Ok(None);
    };
    let texture = texture.trim();
    if texture.is_empty() {
        return Ok(None);
    }
    let raw = Path::new(texture);
    if raw.is_absolute() && raw.exists() {
        return Ok(Some(raw.to_path_buf()));
    }
    if let Some(script_dir) = actor.get::<Option<String>>("__songlua_script_dir")? {
        let candidate = Path::new(&script_dir).join(texture);
        if candidate.exists() {
            return Ok(Some(candidate));
        }
    }
    if let Some(song_dir) = actor.get::<Option<String>>("__songlua_song_dir")? {
        let candidate = Path::new(&song_dir).join(texture);
        if candidate.exists() {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

fn install_actor_metatable(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let mt = lua.create_table()?;
    let actor_clone = actor.clone();
    mt.set(
        "__concat",
        lua.create_function(move |lua, (_lhs, rhs): (Value, Value)| {
            if let Value::Table(rhs) = rhs {
                merge_actor_concat(lua, &actor_clone, &rhs)?;
            }
            Ok(actor_clone.clone())
        })?,
    )?;
    let actor_clone = actor.clone();
    mt.set(
        "__tostring",
        lua.create_function(move |_, _self: Value| Ok(actor_debug_label(&actor_clone)))?,
    )?;
    let _ = actor.set_metatable(Some(mt));
    Ok(())
}

fn merge_actor_concat(_lua: &Lua, actor: &Table, rhs: &Table) -> mlua::Result<()> {
    let next_index = actor.raw_len() + 1;
    let mut append_index = next_index;
    for value in rhs.sequence_values::<Value>() {
        actor.raw_set(append_index, value?)?;
        append_index += 1;
    }
    for pair in rhs.pairs::<Value, Value>() {
        let (key, value) = pair?;
        let is_sequence_key = match key {
            Value::Integer(index) => index >= 1,
            Value::Number(index) => index.is_finite() && index >= 1.0 && index.fract() == 0.0,
            _ => false,
        };
        if is_sequence_key {
            continue;
        }
        if matches!(
            &key,
            Value::String(text)
                if text.to_str().ok().is_some_and(|name|
                    name == "__songlua_actor_type"
                        || name == "__songlua_script_dir"
                        || name == "__songlua_song_dir")
        ) {
            continue;
        }
        actor.set(key, value)?;
    }
    Ok(())
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

fn run_added_actor_child_commands(lua: &Lua, parent: &Table, child: &Table) -> mlua::Result<()> {
    if parent
        .get::<Option<bool>>("__songlua_init_commands_ran")?
        .unwrap_or(false)
    {
        run_actor_init_commands_for_table(lua, child)?;
    }
    let startup_already_needs_child = parent
        .get::<Option<bool>>("__songlua_startup_commands_ran")?
        .unwrap_or(false)
        || parent
            .get::<Option<bool>>("__songlua_startup_children_walked")?
            .unwrap_or(false);
    if startup_already_needs_child {
        run_actor_startup_commands_for_table(lua, child)?;
    }
    Ok(())
}

fn make_actor_chain_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |_, _args: MultiValue| Ok(actor.clone()))
}

fn make_actor_stop_tweening_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, _args: MultiValue| {
        prepare_capture_scope_actor(lua, &actor)?;
        flush_actor_capture(&actor)?;
        reset_actor_capture(lua, &actor)?;
        Ok(actor.clone())
    })
}

fn make_actor_finish_tweening_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, _args: MultiValue| {
        prepare_capture_scope_actor(lua, &actor)?;
        finish_actor_tweening(lua, &actor)?;
        Ok(actor.clone())
    })
}

fn make_actor_wrap_width_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, args: MultiValue| {
        let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
            return Ok(actor.clone());
        };
        let wrap = value as i32;
        if wrap >= 0 {
            capture_block_set_i32(lua, &actor, "wrap_width_pixels", wrap)?;
        }
        Ok(actor.clone())
    })
}

fn make_actor_set_size_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, args: MultiValue| {
        let Some(width) = method_arg(&args, 0).cloned().and_then(read_f32) else {
            return Ok(actor.clone());
        };
        let Some(height) = method_arg(&args, 1).cloned().and_then(read_f32) else {
            return Ok(actor.clone());
        };
        capture_block_set_size(lua, &actor, [width, height])?;
        Ok(actor.clone())
    })
}

fn run_command_on_leaves(lua: &Lua, actor: &Table, command: &Function) -> mlua::Result<()> {
    let mut saw_child = false;
    for child in actor_direct_children(lua, actor)? {
        saw_child = true;
        run_command_on_leaves(lua, &child, command)?;
    }
    if !saw_child {
        let _ = command.call::<Value>(actor.clone())?;
    }
    Ok(())
}

fn run_named_command_on_leaves(
    lua: &Lua,
    actor: &Table,
    command: &str,
    params: Option<Value>,
) -> mlua::Result<()> {
    let mut saw_child = false;
    for child in actor_direct_children(lua, actor)? {
        saw_child = true;
        run_named_command_on_leaves(lua, &child, command, params.clone())?;
    }
    if !saw_child {
        run_actor_named_command_with_drain_and_params(lua, actor, command, true, params)?;
    }
    Ok(())
}

fn run_named_command_on_children_recursively(
    lua: &Lua,
    actor: &Table,
    command: &str,
    params: Option<Value>,
) -> mlua::Result<()> {
    for child in actor_direct_children(lua, actor)? {
        run_actor_named_command_with_drain_and_params(lua, &child, command, true, params.clone())?;
        run_named_command_on_children_recursively(lua, &child, command, params.clone())?;
    }
    Ok(())
}

fn remove_actor_child(lua: &Lua, actor: &Table, name: &str) -> mlua::Result<()> {
    actor_children(lua, actor)?.set(name, Value::Nil)?;
    let mut write = 1;
    for read in 1..=actor.raw_len() {
        let value = actor.raw_get::<Value>(read)?;
        let remove = match &value {
            Value::Table(child) => child
                .get::<Option<String>>("Name")?
                .is_some_and(|child_name| child_name == name),
            _ => false,
        };
        if remove {
            continue;
        }
        if write != read {
            actor.raw_set(write, value)?;
        }
        write += 1;
    }
    for index in write..=actor.raw_len() {
        actor.raw_set(index, Value::Nil)?;
    }
    Ok(())
}

fn remove_all_actor_children(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    for index in 1..=actor.raw_len() {
        actor.raw_set(index, Value::Nil)?;
    }
    let children = actor_children(lua, actor)?;
    let mut keys = Vec::new();
    for pair in children.pairs::<Value, Value>() {
        let (key, _) = pair?;
        keys.push(key);
    }
    for key in keys {
        children.set(key, Value::Nil)?;
    }
    Ok(())
}

fn make_actor_capture_f32_method(
    lua: &Lua,
    actor: &Table,
    key: &'static str,
    probe_name: Option<&'static str>,
) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, args: MultiValue| {
        if let Some(probe_name) = probe_name {
            record_probe_method_call(lua, &actor, probe_name)?;
        }
        if let Some(value) = args.get(1).cloned().and_then(read_f32) {
            capture_block_set_f32(lua, &actor, key, value)?;
        }
        Ok(actor.clone())
    })
}

fn make_actor_add_f32_method(
    lua: &Lua,
    actor: &Table,
    key: &'static str,
) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, args: MultiValue| {
        let Some(delta) = args.get(1).cloned().and_then(read_f32) else {
            return Ok(actor.clone());
        };
        let block = actor_current_capture_block(lua, &actor)?;
        let current = block
            .get::<Option<f32>>(key)?
            .or(actor.get::<Option<f32>>(format!("__songlua_state_{key}"))?)
            .unwrap_or(0.0);
        capture_block_set_f32(lua, &actor, key, current + delta)?;
        Ok(actor.clone())
    })
}

fn make_actor_tween_method(
    lua: &Lua,
    actor: &Table,
    easing: Option<&'static str>,
) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, args: MultiValue| {
        prepare_capture_scope_actor(lua, &actor)?;
        flush_actor_capture(&actor)?;
        let cursor = actor
            .get::<Option<f32>>("__songlua_capture_cursor")?
            .unwrap_or(0.0);
        let duration = args
            .get(1)
            .cloned()
            .and_then(read_f32)
            .unwrap_or(0.0)
            .max(0.0);
        actor.set("__songlua_capture_duration", duration)?;
        actor.set("__songlua_capture_tween_time_left", cursor + duration)?;
        actor.set("__songlua_capture_easing", easing)?;
        actor.set("__songlua_capture_opt1", Value::Nil)?;
        actor.set("__songlua_capture_opt2", Value::Nil)?;
        Ok(actor.clone())
    })
}

fn read_color_args(args: &MultiValue) -> Option<[f32; 4]> {
    if let Some(color) = method_arg(args, 0).cloned().and_then(read_color_value) {
        return Some(color);
    }
    let r = method_arg(args, 0).cloned().and_then(read_f32)?;
    let g = method_arg(args, 1).cloned().and_then(read_f32)?;
    let b = method_arg(args, 2).cloned().and_then(read_f32)?;
    let a = method_arg(args, 3)
        .cloned()
        .and_then(read_f32)
        .unwrap_or(1.0);
    Some([r, g, b, a])
}

fn record_probe_method_call(lua: &Lua, actor: &Table, method_name: &str) -> mlua::Result<()> {
    record_probe_actor_call(lua, actor)?;
    let globals = lua.globals();
    let Some(calls) = globals.get::<Option<Table>>(SONG_LUA_PROBE_METHODS_KEY)? else {
        return Ok(());
    };
    calls.raw_set(
        calls.raw_len() + 1,
        format!("{}.{}", probe_target_kind(actor)?, method_name),
    )?;
    Ok(())
}

fn record_probe_actor_call(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let globals = lua.globals();
    let Some(actors) = globals.get::<Option<Table>>(SONG_LUA_PROBE_ACTORS_KEY)? else {
        return Ok(());
    };
    let Some(seen) = globals.get::<Option<Table>>(SONG_LUA_PROBE_ACTOR_SET_KEY)? else {
        return Ok(());
    };
    if seen
        .raw_get::<Option<bool>>(actor.clone())?
        .unwrap_or(false)
    {
        return Ok(());
    }
    seen.raw_set(actor.clone(), true)?;
    actors.raw_set(actors.raw_len() + 1, actor.clone())?;
    Ok(())
}

fn prepare_capture_scope_actor(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let globals = lua.globals();
    let Some(actors) = globals.get::<Option<Table>>(SONG_LUA_CAPTURE_ACTORS_KEY)? else {
        return Ok(());
    };
    let Some(seen) = globals.get::<Option<Table>>(SONG_LUA_CAPTURE_ACTOR_SET_KEY)? else {
        return Ok(());
    };
    let Some(snapshots) = globals.get::<Option<Table>>(SONG_LUA_CAPTURE_SNAPSHOTS_KEY)? else {
        return Ok(());
    };
    if seen
        .raw_get::<Option<bool>>(actor.clone())?
        .unwrap_or(false)
    {
        return Ok(());
    }
    seen.raw_set(actor.clone(), true)?;
    actors.raw_set(actors.raw_len() + 1, actor.clone())?;

    let snapshot = lua.create_table()?;
    snapshot.raw_set(1, actor.clone())?;
    snapshot.raw_set(2, snapshot_actor_semantic_state_table(lua, actor)?)?;
    snapshots.raw_set(snapshots.raw_len() + 1, snapshot)?;

    reset_actor_capture(lua, actor)?;
    Ok(())
}

fn probe_target_kind(actor: &Table) -> mlua::Result<&'static str> {
    let Some(_player_index) = actor.get::<Option<i64>>("__songlua_player_index")? else {
        return Ok("overlay");
    };
    Ok(
        match actor
            .get::<Option<String>>("__songlua_player_child_name")?
            .as_deref()
        {
            None => "player",
            Some(name) if name.eq_ignore_ascii_case("NoteField") => "notefield",
            Some(name) if name.eq_ignore_ascii_case("Judgment") => "judgment",
            Some(name) if name.eq_ignore_ascii_case("Combo") => "combo",
            _ => "player-child",
        },
    )
}

pub(super) fn probe_function_ease_target(
    lua: &Lua,
    function: &Function,
) -> mlua::Result<(Option<SongLuaEaseTarget>, Vec<String>, Vec<usize>)> {
    let globals = lua.globals();
    let previous_time = compile_song_runtime_values(lua)?;
    set_compile_song_runtime_beat(lua, 0.0)?;
    let previous_methods = globals.get::<Value>(SONG_LUA_PROBE_METHODS_KEY)?;
    let previous_actors = globals.get::<Value>(SONG_LUA_PROBE_ACTORS_KEY)?;
    let previous_actor_set = globals.get::<Value>(SONG_LUA_PROBE_ACTOR_SET_KEY)?;
    let calls = lua.create_table()?;
    let actors = lua.create_table()?;
    globals.set(SONG_LUA_PROBE_METHODS_KEY, calls.clone())?;
    globals.set(SONG_LUA_PROBE_ACTORS_KEY, actors.clone())?;
    globals.set(SONG_LUA_PROBE_ACTOR_SET_KEY, lua.create_table()?)?;
    let result = function.call::<Value>(1.0_f32);
    let methods = probe_call_names(&calls)?;
    let actor_ptrs = probe_actor_pointers(&actors)?;
    let classify = classify_function_ease_probe(&calls);
    globals.set(SONG_LUA_PROBE_METHODS_KEY, previous_methods)?;
    globals.set(SONG_LUA_PROBE_ACTORS_KEY, previous_actors)?;
    globals.set(SONG_LUA_PROBE_ACTOR_SET_KEY, previous_actor_set)?;
    set_compile_song_runtime_values(lua, previous_time.0, previous_time.1)?;
    match result {
        Ok(_) => Ok((classify?, methods, actor_ptrs)),
        Err(_) => Ok((None, methods, actor_ptrs)),
    }
}

fn classify_function_ease_probe(calls: &Table) -> mlua::Result<Option<SongLuaEaseTarget>> {
    let mut saw_x = false;
    let mut saw_y = false;
    let mut saw_z = false;
    let mut saw_rotation_x = false;
    let mut saw_rotation_z = false;
    let mut saw_rotation_y = false;
    let mut saw_skew_x = false;
    let mut saw_skew_y = false;
    let mut saw_zoom = false;
    let mut saw_zoom_x = false;
    let mut saw_zoom_y = false;
    let mut saw_zoom_z = false;
    for value in calls.sequence_values::<String>() {
        let value = value?;
        let (target_kind, method_name) =
            value.split_once('.').unwrap_or(("player", value.as_str()));
        if !matches!(target_kind, "player" | "notefield" | "overlay") {
            return Ok(None);
        }
        match method_name {
            "x" if target_kind == "player" => saw_x = true,
            "y" if target_kind == "player" => saw_y = true,
            "z" if target_kind != "overlay" => saw_z = true,
            "rotationx" if target_kind != "overlay" => saw_rotation_x = true,
            "rotationz" if target_kind != "overlay" => saw_rotation_z = true,
            "rotationy" if target_kind != "overlay" => saw_rotation_y = true,
            "skewx" => saw_skew_x = true,
            "skewy" => saw_skew_y = true,
            "zoom" if target_kind != "overlay" => saw_zoom = true,
            "zoomx" if target_kind != "overlay" => saw_zoom_x = true,
            "zoomy" if target_kind != "overlay" => saw_zoom_y = true,
            "zoomz" if target_kind != "overlay" => saw_zoom_z = true,
            _ => return Ok(None),
        }
    }
    Ok(
        match (
            saw_x,
            saw_y,
            saw_z,
            saw_rotation_x,
            saw_rotation_z,
            saw_rotation_y,
            saw_skew_x,
            saw_skew_y,
            saw_zoom,
            saw_zoom_x,
            saw_zoom_y,
            saw_zoom_z,
        ) {
            (true, false, false, false, false, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerX)
            }
            (false, true, false, false, false, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerY)
            }
            (false, false, true, false, false, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerZ)
            }
            (false, false, false, true, false, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerRotationX)
            }
            (false, false, false, false, true, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerRotationZ)
            }
            (false, false, false, false, false, true, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerRotationY)
            }
            (false, false, false, false, false, false, true, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerSkewX)
            }
            (false, false, false, false, false, false, false, true, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerSkewY)
            }
            (false, false, false, false, false, false, false, false, true, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerZoom)
            }
            (false, false, false, false, false, false, false, false, false, true, false, false) => {
                Some(SongLuaEaseTarget::PlayerZoomX)
            }
            (false, false, false, false, false, false, false, false, false, false, true, false) => {
                Some(SongLuaEaseTarget::PlayerZoomY)
            }
            (false, false, false, false, false, false, false, false, false, false, false, true) => {
                Some(SongLuaEaseTarget::PlayerZoomZ)
            }
            _ => None,
        },
    )
}

fn probe_call_names(calls: &Table) -> mlua::Result<Vec<String>> {
    let mut out = Vec::new();
    for value in calls.sequence_values::<String>() {
        out.push(value?);
    }
    Ok(out)
}

fn probe_actor_pointers(actors: &Table) -> mlua::Result<Vec<usize>> {
    let mut out = Vec::new();
    for value in actors.sequence_values::<Table>() {
        out.push(value?.to_pointer() as usize);
    }
    Ok(out)
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
    reset_overlay_capture_tables_for_indices(lua, overlays, &indices)
}

fn reset_overlay_capture_tables_for_indices(
    lua: &Lua,
    overlays: &[OverlayCompileActor],
    indices: &[usize],
) -> Result<(), String> {
    for &index in indices {
        let Some(overlay) = overlays.get(index) else {
            continue;
        };
        reset_actor_capture(lua, &overlay.table).map_err(|err| err.to_string())?;
    }
    Ok(())
}

pub(super) fn reset_tracked_capture_tables(
    lua: &Lua,
    tracked_actors: &[TrackedCompileActor],
) -> Result<(), String> {
    let indices: Vec<_> = (0..tracked_actors.len()).collect();
    reset_tracked_capture_tables_for_indices(lua, tracked_actors, &indices)
}

fn reset_tracked_capture_tables_for_indices(
    lua: &Lua,
    tracked_actors: &[TrackedCompileActor],
    indices: &[usize],
) -> Result<(), String> {
    for &index in indices {
        let Some(actor) = tracked_actors.get(index) else {
            continue;
        };
        reset_actor_capture(lua, &actor.table).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn collect_overlay_capture_blocks_for_indices(
    overlays: &[OverlayCompileActor],
    indices: &[usize],
) -> Result<Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>, String> {
    let mut out = Vec::new();
    for &idx in indices {
        let Some(overlay) = overlays.get(idx) else {
            continue;
        };
        flush_actor_capture(&overlay.table).map_err(|err| err.to_string())?;
        let blocks = read_actor_capture_blocks(&overlay.table)?;
        if !blocks.is_empty() {
            out.push((idx, blocks));
        }
    }
    Ok(out)
}

fn collect_tracked_capture_blocks_for_indices(
    tracked_actors: &[TrackedCompileActor],
    indices: &[usize],
) -> Result<Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>, String> {
    let mut out = Vec::new();
    for &idx in indices {
        let Some(actor) = tracked_actors.get(idx) else {
            continue;
        };
        flush_actor_capture(&actor.table).map_err(|err| err.to_string())?;
        let blocks = read_actor_capture_blocks(&actor.table)?;
        if !blocks.is_empty() {
            out.push((idx, blocks));
        }
    }
    Ok(out)
}

struct SongLuaActionCaptureScope {
    actors: Table,
    snapshots: Table,
    previous_actors: Value,
    previous_actor_set: Value,
    previous_snapshots: Value,
}

fn begin_action_capture_scope(lua: &Lua) -> mlua::Result<SongLuaActionCaptureScope> {
    let globals = lua.globals();
    let previous_actors = globals.get::<Value>(SONG_LUA_CAPTURE_ACTORS_KEY)?;
    let previous_actor_set = globals.get::<Value>(SONG_LUA_CAPTURE_ACTOR_SET_KEY)?;
    let previous_snapshots = globals.get::<Value>(SONG_LUA_CAPTURE_SNAPSHOTS_KEY)?;
    let actors = lua.create_table()?;
    let snapshots = lua.create_table()?;
    globals.set(SONG_LUA_CAPTURE_ACTORS_KEY, actors.clone())?;
    globals.set(SONG_LUA_CAPTURE_ACTOR_SET_KEY, lua.create_table()?)?;
    globals.set(SONG_LUA_CAPTURE_SNAPSHOTS_KEY, snapshots.clone())?;
    Ok(SongLuaActionCaptureScope {
        actors,
        snapshots,
        previous_actors,
        previous_actor_set,
        previous_snapshots,
    })
}

fn restore_action_capture_scope(lua: &Lua, scope: SongLuaActionCaptureScope) -> mlua::Result<()> {
    let globals = lua.globals();
    globals.set(SONG_LUA_CAPTURE_ACTORS_KEY, scope.previous_actors)?;
    globals.set(SONG_LUA_CAPTURE_ACTOR_SET_KEY, scope.previous_actor_set)?;
    globals.set(SONG_LUA_CAPTURE_SNAPSHOTS_KEY, scope.previous_snapshots)?;
    Ok(())
}

fn capture_scope_actor_pointers(actors: &Table) -> mlua::Result<HashSet<usize>> {
    let mut out = HashSet::with_capacity(actors.raw_len());
    for actor in actors.sequence_values::<Table>() {
        out.insert(actor?.to_pointer() as usize);
    }
    Ok(out)
}

fn capture_scope_actor_tables(actors: &Table) -> mlua::Result<Vec<Table>> {
    let mut out = Vec::with_capacity(actors.raw_len());
    for actor in actors.sequence_values::<Table>() {
        out.push(actor?);
    }
    Ok(out)
}

fn capture_scope_snapshots(snapshots: &Table) -> mlua::Result<Vec<(Table, Vec<(String, Value)>)>> {
    let mut out = Vec::with_capacity(snapshots.raw_len());
    for snapshot in snapshots.sequence_values::<Table>() {
        let snapshot = snapshot?;
        let actor = snapshot.raw_get(1)?;
        let state = read_actor_semantic_state_table(&snapshot.raw_get(2)?)?;
        out.push((actor, state));
    }
    Ok(out)
}

fn reset_actor_capture_tables(lua: &Lua, actors: &[Table]) -> Result<(), String> {
    for actor in actors {
        reset_actor_capture(lua, actor).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn overlay_indices_for_actor_pointers(
    overlays: &[OverlayCompileActor],
    actor_ptrs: &HashSet<usize>,
) -> Vec<usize> {
    overlays
        .iter()
        .enumerate()
        .filter(|(_, overlay)| actor_ptrs.contains(&(overlay.table.to_pointer() as usize)))
        .map(|(index, _)| index)
        .collect()
}

fn tracked_indices_for_actor_pointers(
    tracked_actors: &[TrackedCompileActor],
    actor_ptrs: &HashSet<usize>,
) -> Vec<usize> {
    tracked_actors
        .iter()
        .enumerate()
        .filter(|(_, actor)| actor_ptrs.contains(&(actor.table.to_pointer() as usize)))
        .map(|(index, _)| index)
        .collect()
}

fn capture_overlay_function_blocks(
    lua: &Lua,
    overlays: &[OverlayCompileActor],
    overlay_indices: &[usize],
    function: &Function,
    arg: Option<f32>,
    song_beat: Option<f32>,
) -> Result<Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>, String> {
    let previous = compile_song_runtime_values(lua).map_err(|err| err.to_string())?;
    if let Some(song_beat) = song_beat {
        set_compile_song_runtime_beat(lua, song_beat).map_err(|err| err.to_string())?;
    }
    let snapshot_tables: Vec<_> = overlay_indices
        .iter()
        .filter_map(|&index| overlays.get(index))
        .map(|overlay| overlay.table.clone())
        .collect();
    let state_snapshot =
        snapshot_actors_semantic_state(lua, &snapshot_tables).map_err(|err| err.to_string())?;
    reset_overlay_capture_tables_for_indices(lua, overlays, overlay_indices)?;
    let result = match arg {
        Some(value) => function.call::<Value>(value),
        None => function.call::<Value>(()),
    };
    let blocks = collect_overlay_capture_blocks_for_indices(overlays, overlay_indices);
    reset_overlay_capture_tables_for_indices(lua, overlays, overlay_indices)?;
    restore_actors_semantic_state(state_snapshot).map_err(|err| err.to_string())?;
    set_compile_song_runtime_values(lua, previous.0, previous.1).map_err(|err| err.to_string())?;
    let blocks = blocks?;
    result.map_err(|err| err.to_string())?;
    Ok(blocks)
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
    let previous = compile_song_runtime_values(lua).map_err(|err| err.to_string())?;
    let side_effect_before = song_lua_side_effect_count(lua).map_err(|err| err.to_string())?;
    let globals = lua.globals();
    let previous_broadcasts = globals
        .get::<Value>(SONG_LUA_BROADCASTS_KEY)
        .map_err(|err| err.to_string())?;
    let broadcast_table = lua.create_table().map_err(|err| err.to_string())?;
    globals
        .set(SONG_LUA_BROADCASTS_KEY, broadcast_table.clone())
        .map_err(|err| err.to_string())?;
    set_compile_song_runtime_beat(lua, beat).map_err(|err| err.to_string())?;
    let capture_scope = begin_action_capture_scope(lua).map_err(|err| err.to_string())?;
    let result = function.call::<Value>(());
    let touched_actors =
        capture_scope_actor_tables(&capture_scope.actors).map_err(|err| err.to_string())?;
    let actor_ptrs =
        capture_scope_actor_pointers(&capture_scope.actors).map_err(|err| err.to_string())?;
    let state_snapshot =
        capture_scope_snapshots(&capture_scope.snapshots).map_err(|err| err.to_string())?;
    restore_action_capture_scope(lua, capture_scope).map_err(|err| err.to_string())?;
    let overlay_indices = overlay_indices_for_actor_pointers(overlays, &actor_ptrs);
    let tracked_indices = tracked_indices_for_actor_pointers(tracked_actors, &actor_ptrs);
    let overlay_blocks = collect_overlay_capture_blocks_for_indices(overlays, &overlay_indices);
    let tracked_blocks =
        collect_tracked_capture_blocks_for_indices(tracked_actors, &tracked_indices);
    let broadcasts = read_song_lua_broadcasts(&broadcast_table).map_err(|err| err.to_string());
    reset_actor_capture_tables(lua, &touched_actors)?;
    restore_actors_semantic_state(state_snapshot).map_err(|err| err.to_string())?;
    globals
        .set(SONG_LUA_BROADCASTS_KEY, previous_broadcasts)
        .map_err(|err| err.to_string())?;
    set_compile_song_runtime_values(lua, previous.0, previous.1).map_err(|err| err.to_string())?;
    let overlay_blocks = overlay_blocks?;
    let tracked_blocks = tracked_blocks?;
    let broadcasts = broadcasts?;
    let saw_side_effect =
        song_lua_side_effect_count(lua).map_err(|err| err.to_string())? > side_effect_before;
    result.map_err(|err| err.to_string())?;
    Ok((overlay_blocks, tracked_blocks, broadcasts, saw_side_effect))
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
    if !broadcasts.is_empty() && broadcasts.iter().all(|(_, has_params)| !*has_params) {
        let mut emitted = false;
        for (message, _) in broadcasts {
            if !song_lua_message_has_listener(overlays, tracked_actors, &message) {
                continue;
            }
            messages.push(SongLuaMessageEvent {
                beat,
                message,
                persists,
            });
            emitted = true;
        }
        if emitted {
            return Ok(true);
        }
    }
    if overlay_captures.is_empty() && tracked_captures.is_empty() {
        return Ok(saw_side_effect);
    }
    let message = format!("__songlua_overlay_fn_action_{}", *counter);
    *counter += 1;
    for (overlay_index, blocks) in overlay_captures {
        overlays[overlay_index]
            .actor
            .message_commands
            .push(SongLuaOverlayMessageCommand {
                message: message.clone(),
                blocks,
            });
    }
    for (tracked_index, blocks) in tracked_captures {
        tracked_actors[tracked_index]
            .actor
            .message_commands
            .push(SongLuaOverlayMessageCommand {
                message: message.clone(),
                blocks,
            });
    }
    messages.push(SongLuaMessageEvent {
        beat,
        message,
        persists,
    });
    Ok(true)
}

fn song_lua_message_has_listener(
    overlays: &[OverlayCompileActor],
    tracked_actors: &[TrackedCompileActor],
    message: &str,
) -> bool {
    overlays.iter().any(|overlay| {
        overlay
            .actor
            .message_commands
            .iter()
            .any(|command| command.message == message)
    }) || tracked_actors.iter().any(|actor| {
        actor
            .actor
            .message_commands
            .iter()
            .any(|command| command.message == message)
    })
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
    let start_beat = start;
    let end_beat = song_lua_span_end(start, limit, span_mode).max(start_beat);
    let overlay_indices = overlay_function_ease_indices(overlays, probe_actor_ptrs);
    let from_blocks = capture_overlay_function_blocks(
        lua,
        overlays,
        &overlay_indices,
        function,
        Some(from),
        Some(start_beat),
    )?;
    let to_blocks = capture_overlay_function_blocks(
        lua,
        overlays,
        &overlay_indices,
        function,
        Some(to),
        Some(end_beat),
    )?;
    if from_blocks.is_empty() && to_blocks.is_empty() {
        return Ok(Vec::new());
    }

    let mut from_deltas = HashMap::new();
    for (overlay_index, blocks) in from_blocks {
        if let Some(delta) = overlay_delta_from_blocks(&blocks) {
            from_deltas.insert(overlay_index, delta);
        }
    }
    let mut to_deltas = HashMap::new();
    for (overlay_index, blocks) in to_blocks {
        if let Some(delta) = overlay_delta_from_blocks(&blocks) {
            to_deltas.insert(overlay_index, delta);
        }
    }

    let mut out = Vec::new();
    for overlay_index in 0..overlays.len() {
        let Some((from_delta, to_delta)) = from_deltas
            .get(&overlay_index)
            .zip(to_deltas.get(&overlay_index))
            .and_then(|(from_delta, to_delta)| overlay_delta_intersection(from_delta, to_delta))
        else {
            continue;
        };
        out.push(SongLuaOverlayEase {
            overlay_index,
            unit,
            start,
            limit,
            span_mode,
            from: from_delta,
            to: to_delta,
            easing: easing.clone(),
            sustain,
            opt1,
            opt2,
        });
    }
    Ok(out)
}

fn overlay_function_ease_indices(
    overlays: &[OverlayCompileActor],
    probe_actor_ptrs: &[usize],
) -> Vec<usize> {
    if probe_actor_ptrs.is_empty() {
        return (0..overlays.len()).collect();
    }
    let probe_actor_ptrs: HashSet<_> = probe_actor_ptrs.iter().copied().collect();
    let out: Vec<_> = overlays
        .iter()
        .enumerate()
        .filter(|(_, overlay)| probe_actor_ptrs.contains(&(overlay.table.to_pointer() as usize)))
        .map(|(index, _)| index)
        .collect();
    if out.is_empty() {
        (0..overlays.len()).collect()
    } else {
        out
    }
}

#[inline(always)]
fn song_lua_span_end(start: f32, limit: f32, span_mode: SongLuaSpanMode) -> f32 {
    match span_mode {
        SongLuaSpanMode::Len => start + limit.max(0.0),
        SongLuaSpanMode::End => limit,
    }
}

fn set_rolling_numbers_metric(actor: &Table, metric: &str) -> mlua::Result<()> {
    actor.set("__songlua_rolling_numbers_metric", metric)?;
    actor.set(
        "__songlua_rolling_numbers_format",
        rolling_numbers_format(metric),
    )?;
    Ok(())
}

fn rolling_numbers_format(metric: &str) -> &'static str {
    if metric.eq_ignore_ascii_case("RollingNumbersEvaluationB") {
        "%03.0f"
    } else if metric.eq_ignore_ascii_case("RollingNumbersEvaluationA")
        || metric.eq_ignore_ascii_case("RollingNumbersEvaluationNoDecentsWayOffs")
        || metric.eq_ignore_ascii_case("RollingNumbersEvaluation")
    {
        "%04.0f"
    } else {
        "%.0f"
    }
}

fn rolling_numbers_text(actor: &Table, number: f32) -> mlua::Result<String> {
    let format = actor
        .get::<Option<String>>("__songlua_rolling_numbers_format")?
        .unwrap_or_else(|| "%.0f".to_string());
    Ok(format_rolling_number(&format, number))
}

fn format_rolling_number(format: &str, number: f32) -> String {
    let rounded = number.round().clamp(i64::MIN as f32, i64::MAX as f32) as i64;
    if format.contains("%04") {
        format!("{rounded:04}")
    } else if format.contains("%03") {
        format!("{rounded:03}")
    } else if format.contains("%.2") {
        format!("{number:.2}")
    } else {
        rounded.to_string()
    }
}
