//! The existing pre-song underlay constructor, before original theme files load.
use super::*;
use generated_players::dummy;
use screen_parts::{append, invoke, named};
use sprites::set;
use std::sync::OnceLock;

pub(super) fn ensure_helpers(vm: &mut Vm, helpers: TableId) -> Result<(), Error> {
    for (key, source, name) in [
        (
            "underlay_header",
            include_bytes!("underlay/header.lua").as_slice(),
            "Shared/Header.lua",
        ),
        (
            "underlay_filter",
            include_bytes!("underlay/filter.lua").as_slice(),
            "PerPlayer/BackgroundFilter.lua",
        ),
        (
            "underlay_difficulty",
            include_bytes!("underlay/difficulty.lua").as_slice(),
            "PerPlayer/DifficultyMeter.lua",
        ),
        (
            "underlay_life",
            include_bytes!("underlay/life.lua").as_slice(),
            "PerPlayer/LifeMeter/Standard.lua",
        ),
        (
            "underlay_bpm",
            include_bytes!("underlay/bpm.lua").as_slice(),
            "Shared/BPMDisplay.lua",
        ),
    ] {
        let key_value = vm.try_intern(key.as_bytes())?;
        if vm.raw_table_get(helpers, key_value)? == Value::Nil {
            let name = format!("@theme:/BGAnimations/ScreenGameplay underlay/{name}");
            let code = compile_named(source, name.as_bytes())?;
            let function = Value::Function(vm.try_load_program(&code)?);
            let root = vm.pin(function)?;
            let result = super::super::put(vm, helpers, key, function);
            vm.unpin(root)?;
            result?;
        }
    }
    Ok(())
}

fn global(call: &mut HostCall<'_>, name: &str) -> Result<Value, Error> {
    field(call, table(call, helper(call, 5)?)?, name)
}

fn source(call: &mut HostCall<'_>, name: &str, args: &[Value]) -> Result<Value, Error> {
    let function = native_children::helper_function(call, name.as_bytes())?;
    Ok(call
        .call(function, args)?
        .first()
        .copied()
        .unwrap_or(Value::Nil))
}

fn field(call: &mut HostCall<'_>, owner: TableId, name: &str) -> Result<Value, Error> {
    call.with_roots(&[Value::Table(owner)], |call| {
        sprites::field(call, owner, name)
    })
}

fn member(call: &mut HostCall<'_>, owner: TableId, name: &str) -> Result<TableId, Error> {
    let value = field(call, owner, name)?;
    table(call, value)
}

fn mods(call: &mut HostCall<'_>, player: usize) -> Result<TableId, Error> {
    let sl = global(call, "SL")?;
    let player = member(call, table(call, sl)?, &format!("P{}", player + 1))?;
    member(call, player, "ActiveModifiers")
}

fn number(call: &mut HostCall<'_>, value: Value) -> Result<f32, Error> {
    sprites::number(call, value).map(|value| value as f32)
}

fn commands(
    call: &mut HostCall<'_>,
    actor: TableId,
    definition: TableId,
    names: &[&str],
    required: bool,
) -> Result<(), Error> {
    for name in names {
        let value = get(call, definition, name.as_bytes())?;
        if value == Value::Nil && !required {
            continue;
        }
        if !matches!(value, Value::Function(_) | Value::Native(_)) {
            return Err(call.error(b"theme command function expected"));
        }
        let key = call.try_intern(name.as_bytes())?;
        call.raw_table_set(actor, key, value)?;
    }
    Ok(())
}

fn run(call: &mut HostCall<'_>, actor: TableId, name: &str) -> Result<(), Error> {
    let function = get(call, actor, name.as_bytes())?;
    if function != Value::Nil {
        call.call(function, &[Value::Table(actor)])?;
    }
    Ok(())
}

pub(super) fn group(call: &mut HostCall<'_>, members: &[TableId]) -> Result<Value, Error> {
    let values = call.try_new_table()?;
    call.with_roots(&[Value::Table(values)], |call| {
        for (i, &actor) in members.iter().enumerate() {
            call.raw_table_set(values, Value::Number((i + 1) as f64), Value::Table(actor))?;
        }
        Ok(call.call(helper(call, 3)?, &[Value::Table(values)])?[0])
    })
}

fn unnamed(
    call: &mut HostCall<'_>,
    actor: TableId,
    members: &[TableId],
    always_group: bool,
) -> Result<(), Error> {
    let value = match members {
        [] if !always_group => return Ok(()),
        [actor] if !always_group => Value::Table(*actor),
        _ => group(call, members)?,
    };
    native_children::insert(call, actor, "", value)
}

// Same tags as create_named_child_actor. Internal unnamed actors do not inherit
// these tags; they acquire only their explicit role and parent references.
pub(super) fn copy_tags(
    call: &mut HostCall<'_>,
    from: TableId,
    into: TableId,
) -> Result<(), Error> {
    for key in ["__songlua_player1_enabled", "__songlua_player2_enabled"] {
        let value = get(call, from, key.as_bytes())?;
        if value != Value::Nil {
            let key = call.try_intern(key.as_bytes())?;
            call.raw_table_set(into, key, Value::Boolean(value != Value::Boolean(false)))?;
        }
    }
    for key in ["__songlua_player_index", "__songlua_compile_layer"] {
        let value = field(call, from, key)?;
        if value != Value::Nil {
            let value = sprites::number(call, value)?;
            if !(i64::MIN as f64..-(i64::MIN as f64)).contains(&value) {
                return Err(call.error(b"integer out of range"));
            }
            set(call, into, key, Value::Number(value as i64 as f64))?;
        }
    }
    for key in [
        "__songlua_player_child_name",
        "__songlua_top_screen_child_name",
        "__songlua_main_title",
        "__songlua_bpm_text",
        "__songlua_style_name",
    ] {
        let value = field(call, from, key)?;
        if value != Value::Nil {
            let text = assets::text(call, value)?;
            let text = call.try_intern(text.as_bytes())?;
            set(call, into, key, text)?;
        }
    }
    Ok(())
}

pub(super) fn create(
    call: &mut HostCall<'_>,
    store: &Shared,
    scene: u64,
    parent: TableId,
) -> Result<TableId, Error> {
    let underlay = dummy(call, store, scene, "ActorFrame", "Underlay", None)?;
    copy_tags(call, parent, underlay)?;
    let name = call.try_intern(b"Underlay")?;
    set(call, underlay, "__songlua_top_screen_child_name", name)?;
    let mut unnamed_parts = Vec::new();
    for &name in crate::UNDERLAY_THEME_CHILD_NAMES {
        let player = crate::underlay_score_index(name)
            .or_else(|| crate::top_screen_step_stats_pane_index(name))
            .or_else(|| crate::top_screen_danger_index(name))
            .or_else(|| {
                name.strip_prefix("NoteFieldContainerP")
                    .and_then(|s| s.parse::<usize>().ok())
                    .map(|i| i - 1)
            });
        if let Some(player) = player {
            let key = ["__songlua_player1_enabled", "__songlua_player2_enabled"][player];
            if get(call, underlay, key.as_bytes())? == Value::Boolean(false) {
                continue;
            }
        }
        if let Some(player) = crate::top_screen_step_stats_pane_index(name) {
            if let Some(actor) = filter(call, store, scene, underlay, player)? {
                role(call, actor, &format!("filter{player}"))?;
                append(call, store, underlay, actor)?;
                unnamed_parts.push(actor);
            }
            continue;
        }
        // The original StepStatistics, Danger, NoteField and SongMeter files
        // load in the later theme phase, after their complete host is installed.
        if crate::top_screen_danger_index(name).is_some()
            || name == "SongMeter"
            || name.starts_with("NoteFieldContainerP")
        {
            continue;
        }
        let actor = if name == "Header" {
            let actor = dummy(call, store, scene, "Quad", name, Some(underlay))?;
            let definition = source(call, "underlay_header", &[])?;
            call.with_roots(&[definition], |call| {
                commands(
                    call,
                    actor,
                    table(call, definition)?,
                    &["InitCommand"],
                    true,
                )?;
                run(call, actor, "InitCommand")
            })?;
            actor
        } else {
            screen_parts::text(call, store, scene, name, "0.00%")?
        };
        set_name(call, store, actor, name)?;
        copy_tags(call, underlay, actor)?;
        let tag = call.try_intern(name.as_bytes())?;
        set(call, actor, "__songlua_top_screen_child_name", tag)?;
        named(call, store, underlay, name, actor)?;
        if let Some(player) = crate::underlay_score_index(name) {
            let actor = difficulty(call, store, scene, underlay, player)?;
            role(call, actor, &format!("difficulty{player}"))?;
            append(call, store, underlay, actor)?;
            unnamed_parts.push(actor);
            if let Some(actor) = life(call, store, scene, underlay, player)? {
                role(call, actor, &format!("life{player}"))?;
                append(call, store, underlay, actor)?;
                unnamed_parts.push(actor);
            }
        }
    }
    let definition = source(call, "underlay_bpm", &[])?;
    let bpm = call.with_roots(&[definition], |call| {
        bpm_node(call, store, scene, underlay, table(call, definition)?)
    })?;
    role(call, bpm, "bpm")?;
    append(call, store, underlay, bpm)?;
    unnamed_parts.push(bpm);
    unnamed(call, underlay, &unnamed_parts, false)?;
    Ok(underlay)
}

fn role(call: &mut HostCall<'_>, actor: TableId, role: &str) -> Result<(), Error> {
    let value = call.try_intern(role.as_bytes())?;
    set(call, actor, "__songlua_underlay_role", value)
}

fn filter(
    call: &mut HostCall<'_>,
    store: &Shared,
    scene: u64,
    underlay: TableId,
    player: usize,
) -> Result<Option<TableId>, Error> {
    let pn = call.try_intern(crate::player_number_name(player).as_bytes())?;
    let definition = source(call, "underlay_filter", &[pn])?;
    if definition == Value::Nil {
        return Ok(None);
    }
    call.with_roots(&[definition, pn], |call| {
        let definition = table(call, definition)?;
        let mods = mods(call, player)?;
        let filter = field(call, mods, "BackgroundFilter")?;
        let alpha = match filter {
            Value::Number(value) => value as f32 / 100.,
            Value::String(_) => call.with_roots(&[filter], |call| {
                let function = global(call, "BackgroundFilterValues")?;
                let values = call.call(function, &[])?[0];
                let value = call.with_roots(&[values], |call| call.table_get(values, filter))?;
                Ok(number(call, value)? / 100.)
            })?,
            _ => return Err(call.error(b"invalid initial BackgroundFilter")),
        };
        let actor = dummy(call, store, scene, "Quad", "", Some(underlay))?;
        commands(
            call,
            actor,
            definition,
            &["InitCommand", "OffCommand", "ComboFlashCommand"],
            true,
        )?;
        let function = global(call, "GetNotefieldX")?;
        let value = call.call(function, &[pn])?[0];
        let x = number(call, value)?;
        let function = global(call, "GetNotefieldWidth")?;
        let value = call.call(function, &[])?[0];
        let width = number(call, value)?;
        let value = global(call, "SCREEN_HEIGHT")?;
        let height = number(call, value)?;
        invoke(
            call,
            actor,
            "xy",
            &[
                Value::Number(x as f64),
                Value::Number((height * 0.5) as f64),
            ],
        )?;
        invoke(
            call,
            actor,
            "diffuse",
            &[
                Value::Number(0.),
                Value::Number(0.),
                Value::Number(0.),
                Value::Number(alpha as f64),
            ],
        )?;
        invoke(
            call,
            actor,
            "zoomto",
            &[Value::Number(width as f64), Value::Number(height as f64)],
        )?;
        Ok(Some(actor))
    })
}

fn difficulty(
    call: &mut HostCall<'_>,
    store: &Shared,
    scene: u64,
    underlay: TableId,
    player: usize,
) -> Result<TableId, Error> {
    let pn = call.try_intern(crate::player_number_name(player).as_bytes())?;
    let definition = source(call, "underlay_difficulty", &[pn])?;
    call.with_roots(&[definition, pn], |call| {
        let definition = table(call, definition)?;
        let frame = dummy(call, store, scene, "ActorFrame", "", Some(underlay))?;
        commands(call, frame, definition, &["InitCommand"], true)?;
        run(call, frame, "InitCommand")?;
        let quad = dummy(call, store, scene, "Quad", "", None)?;
        let text = screen_parts::text(call, store, scene, "", "")?;
        let font = call.try_intern(b"Common Bold")?;
        set(call, text, "Font", font)?;
        for (i, actor) in [quad, text].into_iter().enumerate() {
            let part = call.raw_table_get(definition, Value::Number((i + 1) as f64))?;
            commands(
                call,
                actor,
                table(call, part)?,
                &[
                    "InitCommand",
                    "BeginCommand",
                    "CurrentSongChangedMessageCommand",
                ],
                true,
            )?;
            native_children::adopt(call, store, actor, frame)?;
            run(call, actor, "InitCommand")?;
            append(call, store, frame, actor)?;
        }
        unnamed(call, frame, &[quad, text], true)?;
        let game = global(call, "GAMESTATE")?;
        let function = field(call, table(call, game)?, "GetCurrentSteps")?;
        let steps = call.call(function, &[game, pn])?[0];
        if steps != Value::Nil {
            call.with_roots(&[steps], |call| {
                let function = field(call, table(call, steps)?, "GetDifficulty")?;
                let value = call.call(function, &[steps])?[0];
                let text_value = assets::text(call, value)?;
                let value = call.try_intern(text_value.as_bytes())?;
                let color = call.with_roots(&[value], |call| {
                    let function = global(call, "DifficultyColor")?;
                    Ok(call.call(function, &[value])?[0])
                })?;
                call.with_roots(&[color], |call| invoke(call, quad, "diffuse", &[color]))?;
                let function = field(call, table(call, steps)?, "GetMeter")?;
                let value = call.call(function, &[steps])?[0];
                let value = sprites::number(call, value)?;
                if !(0.0..=(u32::MAX as f64)).contains(&value) {
                    return Err(call.error(b"meter out of range"));
                }
                invoke(call, text, "settext", &[Value::Number(value as u32 as f64)])
            })?;
        }
        Ok(frame)
    })
}

fn swoosh() -> Result<String, String> {
    static PREPARED: OnceLock<Result<String, String>> = OnceLock::new();
    PREPARED.get_or_init(|| {
        let bytes = include_bytes!("../../assets/simply-love/BGAnimations/ScreenGameplay underlay/PerPlayer/LifeMeter/swoosh.png");
        let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).map_err(|error| error.to_string())?;
        let mut image = decoded.into_rgba8();
        let source_size = [image.width(), image.height()];
        let hints = deadlib_assets::parse_texture_hints("swoosh.png");
        deadlib_assets::apply_texture_hints(&mut image, &hints);
        deadlib_assets::fix_hidden_alpha(&mut image);
        let path = format!("{}B/ScreenGameplay underlay/PerPlayer/LifeMeter/swoosh.png", crate::SONG_LUA_THEME_PATH_PREFIX);
        deadlib_assets::register_prepared_generated_texture(&path, image, source_size, hints.sampler_desc());
        Ok(path)
    }).clone()
}

fn life(
    call: &mut HostCall<'_>,
    store: &Shared,
    scene: u64,
    underlay: TableId,
    player: usize,
) -> Result<Option<TableId>, Error> {
    let mods = mods(call, player)?;
    call.with_roots(&[Value::Table(mods)], |call| {
        life_inner(call, store, scene, underlay, player, mods)
    })
}

fn life_inner(
    call: &mut HostCall<'_>,
    store: &Shared,
    scene: u64,
    underlay: TableId,
    player: usize,
    mods: TableId,
) -> Result<Option<TableId>, Error> {
    if !matches!(
        field(call, mods, "HideLifebar")?,
        Value::Nil | Value::Boolean(false)
    ) {
        return Ok(None);
    }
    let sl = global(call, "SL")?;
    let globals = member(call, table(call, sl)?, "Global")?;
    let value = field(call, globals, "GameMode")?;
    if assets::text(call, value)? != "ITG" {
        return Ok(None);
    }
    let value = field(call, mods, "LifeMeterType")?;
    if assets::text(call, value)? != "Standard" {
        return Ok(None);
    }
    let pn = call.try_intern(crate::player_number_name(player).as_bytes())?;
    let game = global(call, "GAMESTATE")?;
    let function = field(call, table(call, game)?, "GetPlayerState")?;
    let state = call.call(function, &[game, pn])?[0];
    let position = call.with_roots(&[state], |call| {
        let function = field(call, table(call, state)?, "GetSongPosition")?;
        Ok(call.call(function, &[state])?[0])
    })?;
    let definition = call.with_roots(&[position], |call| {
        source(call, "underlay_life", &[pn, position])
    })?;
    call.with_roots(&[definition], |call| {
        let definition = table(call, definition)?;
        let meter = dummy(call, store, scene, "ActorFrame", "", Some(underlay))?;
        commands(call, meter, definition, &["InitCommand", "OnCommand"], true)?;
        run(call, meter, "InitCommand")?;
        let mut unnamed_parts = Vec::new();
        for (i, kind, name) in [
            (1, "Quad", ""),
            (2, "Quad", ""),
            (3, "Quad", "MeterFill"),
            (4, "Sprite", "MeterSwoosh"),
        ] {
            let actor = dummy(call, store, scene, kind, name, Some(meter))?;
            if kind == "Sprite" {
                let path = swoosh().map_err(|error| call.error(error.as_bytes()))?;
                let path = call.try_intern(path.as_bytes())?;
                sprites::set_texture(call, store, actor, path, false)?;
            }
            let part = call.raw_table_get(definition, Value::Number(i as f64))?;
            commands(
                call,
                actor,
                table(call, part)?,
                &[
                    "InitCommand",
                    "OnCommand",
                    "LifeChangedMessageCommand",
                    "HealthStateChangedMessageCommand",
                ],
                false,
            )?;
            run(call, actor, "InitCommand")?;
            run(call, actor, "OnCommand")?;
            append(call, store, meter, actor)?;
            if name.is_empty() {
                unnamed_parts.push(actor);
            } else {
                named(call, store, meter, name, actor)?;
            }
        }
        unnamed(call, meter, &unnamed_parts, true)?;
        set(
            call,
            meter,
            "__songlua_owned_child_lookup",
            Value::Boolean(true),
        )?;
        run(call, meter, "OnCommand")?;
        Ok(Some(meter))
    })
}

fn bpm_node(
    call: &mut HostCall<'_>,
    store: &Shared,
    scene: u64,
    parent: TableId,
    definition: TableId,
) -> Result<TableId, Error> {
    let kind = get(call, definition, b"__songlua_builtin_kind")?;
    let kind = assets::text(call, kind)?;
    let name = get(call, definition, b"Name")?;
    let name = if name == Value::Nil {
        String::new()
    } else {
        assets::text(call, name)?
    };
    let actor = if kind == "BitmapText" {
        let text = get(call, definition, b"Text")?;
        let text = if text == Value::Nil {
            String::new()
        } else {
            assets::text(call, text)?
        };
        let actor = screen_parts::text(call, store, scene, &name, &text)?;
        let font = get(call, definition, b"Font")?;
        let font = assets::text(call, font)?;
        let font = call.try_intern(font.as_bytes())?;
        set(call, actor, "Font", font)?;
        actor
    } else {
        dummy(call, store, scene, "ActorFrame", &name, None)?
    };
    native_children::adopt(call, store, actor, parent)?;
    // These unchanged definitions contain only Init commands. Their update
    // closures are installed by those commands and retain their own displays.
    commands(call, actor, definition, &["InitCommand"], false)?;
    run(call, actor, "InitCommand")?;
    let mut unnamed_parts = Vec::new();
    for index in 1..=call.raw_table_len(definition)? {
        let part = call.raw_table_get(definition, Value::Number(index as f64))?;
        let child = call.with_roots(&[part], |call| {
            bpm_node(call, store, scene, actor, table(call, part)?)
        })?;
        let name = actor_name(call, store, child)?;
        let name =
            String::from_utf8(name).map_err(|error| call.error(error.to_string().as_bytes()))?;
        append(call, store, actor, child)?;
        if name.is_empty() {
            unnamed_parts.push(child);
        } else {
            named(call, store, actor, &name, child)?;
        }
    }
    unnamed(call, actor, &unnamed_parts, false)?;
    if kind == "ActorFrame" {
        set(
            call,
            actor,
            "__songlua_owned_child_lookup",
            Value::Boolean(true),
        )?;
    }
    Ok(actor)
}

pub(super) fn set_name(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    name: &str,
) -> Result<(), Error> {
    let value = call.try_intern(name.as_bytes())?;
    let key = call.try_intern(b"__songlua_load_name")?;
    if call.raw_table_get(actor, key)? != Value::Nil {
        call.raw_table_set(actor, key, value)?;
    }
    let key = call.try_intern(b"__songlua_native_name")?;
    call.raw_table_set(actor, key, value)?;
    set(call, actor, "Name", value)?;
    store
        .lock()
        .unwrap()
        .actors
        .get_mut(&actor)
        .unwrap()
        .state
        .name = name.as_bytes().to_vec();
    Ok(())
}
