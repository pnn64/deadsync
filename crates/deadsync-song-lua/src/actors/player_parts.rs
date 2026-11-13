//! Player::Init's existing engine-owned children, before theme commands load.
use super::*;
use generated_players::dummy;
use sprites::{field, set};

pub(super) fn install(
    call: &mut HostCall<'_>,
    scene: u64,
    player: TableId,
    index: usize,
    style: &str,
) -> Result<(), Error> {
    let combo = combo(call, scene, index)?;
    attach_part(call, player, combo, "Combo", (index, style))?;
    let judgment = judgment(call, scene, player, index)?;
    attach_part(call, player, judgment, "Judgment", (index, style))?;
    hold_judgments(call, scene, player, index, style)?;
    let field = notefields::create(call, scene, index, style)?;
    attach_part(call, player, field, "NoteField", (index, style))?;
    lifecycle::invalidate_plans(call)?;
    Ok(())
}

fn append(call: &mut HostCall<'_>, parent: TableId, child: TableId) -> Result<(), Error> {
    let index = call.raw_table_len(parent)? + 1;
    call.raw_table_set(parent, Value::Number(index as f64), Value::Table(child))?;
    call.store()
        .actors
        .get_mut(&parent)
        .unwrap()
        .children
        .push(child);
    Ok(())
}

fn attach_part(
    call: &mut HostCall<'_>,
    parent: TableId,
    child: TableId,
    name: &str,
    (index, style): (usize, &str),
) -> Result<(), Error> {
    let value = call.try_intern(name.as_bytes())?;
    set(call, child, "Name", value)?;
    call.store().actors.get_mut(&child).unwrap().state.name = name.as_bytes().to_vec();
    set(
        call,
        child,
        "__songlua_player_index",
        Value::Number(index as f64),
    )?;
    let style = call.try_intern(style.as_bytes())?;
    set(call, child, "__songlua_style_name", style)?;
    set(call, child, "__songlua_player_child_name", value)?;
    native_children::adopt(call, child, parent)?;
    native_children::insert(call, parent, name, Value::Table(child))?;
    append(call, parent, child)
}

fn texture(call: &mut HostCall<'_>, actor: TableId, value: Value) -> Result<(), Error> {
    let path = assets::text(call, value)?;
    let path = path.trim();
    if !path.is_empty() {
        let value = call.try_intern(path.as_bytes())?;
        sprites::set_texture(call, actor, value, false)?;
    }
    Ok(())
}

fn judgment(
    call: &mut HostCall<'_>,
    scene: u64,
    player: TableId,
    index: usize,
) -> Result<TableId, Error> {
    let frame = dummy(call, scene, "ActorFrame", "Judgment", None)?;
    let sprite = dummy(call, scene, "Sprite", "JudgmentWithOffsets", Some(frame))?;
    set(
        call,
        sprite,
        "__songlua_sprite_animate",
        Value::Boolean(false),
    )?;
    set(call, sprite, "__songlua_visible", Value::Boolean(false))?;
    call.store().actors.get_mut(&sprite).unwrap().state.visible = false;
    set(
        call,
        sprite,
        "__songlua_judgment_sprite_player",
        Value::Number(index as f64),
    )?;
    let path = field(call, player, "__songlua_judgment_texture")?;
    if path != Value::Nil {
        texture(call, sprite, path)?;
    }
    append(call, frame, sprite)?;
    native_children::insert(call, frame, "JudgmentWithOffsets", Value::Table(sprite))?;
    Ok(frame)
}

fn combo(call: &mut HostCall<'_>, scene: u64, index: usize) -> Result<TableId, Error> {
    let globals = table(call, helper(call, 5)?)?;
    let function = field(call, globals, "GetComboFonts")?;
    if !matches!(function, Value::Function(_) | Value::Native(_)) {
        return Err(call.error(b"GetComboFonts function expected"));
    }
    let fonts = call
        .call(function, &[])?
        .first()
        .copied()
        .unwrap_or(Value::Nil);
    let fonts = table(call, fonts)?;
    call.with_roots(&[Value::Table(fonts)], |call| {
        let sl = field(call, globals, "SL")?;
        call.with_roots(&[sl], |call| {
            let player = field(call, table(call, sl)?, &format!("P{}", index + 1))?;
            call.with_roots(&[player], |call| {
                let mods = field(call, table(call, player)?, "ActiveModifiers")?;
                call.with_roots(&[mods], |call| {
                    let hide = field(call, table(call, mods)?, "HideCombo")?;
                    let hide = !matches!(hide, Value::Nil | Value::Boolean(false));
                    if hide || call.raw_table_len(fonts)? == 0 {
                        let actor = dummy(call, scene, "Actor", "Combo", None)?;
                        let key = call.try_intern(b"__songlua_visible")?;
                        call.raw_table_set(actor, key, Value::Boolean(false))?;
                        call.store().actors.get_mut(&actor).unwrap().state.visible = false;
                        Ok(actor)
                    } else {
                        let frame = dummy(call, scene, "ActorFrame", "Combo", None)?;
                        let number = dummy(call, scene, "BitmapText", "Number", Some(frame))?;
                        append(call, frame, number)?;
                        native_children::insert(call, frame, "Number", Value::Table(number))?;
                        Ok(frame)
                    }
                })
            })
        })
    })
}

fn hold_judgments(
    call: &mut HostCall<'_>,
    scene: u64,
    player: TableId,
    index: usize,
    style: &str,
) -> Result<(), Error> {
    // These two arrays are native forwarding groups, including when empty.
    let empty = call.try_new_table()?;
    let frames = call.call(helper(call, 3)?, &[Value::Table(empty)])?[0];
    call.with_roots(&[frames], |call| {
        let empty = call.try_new_table()?;
        let unnamed = call.call(helper(call, 3)?, &[Value::Table(empty)])?[0];
        call.with_roots(&[unnamed], |call| {
            let frames = table(call, frames)?;
            let unnamed = table(call, unnamed)?;
            let attack = get_static(call, player, b"__songlua_attack_display")?;
            if attack != Value::Nil {
                table(call, attack)?;
                call.raw_table_set(unnamed, Value::Number(1.), attack)?;
            }
            // Native holds use raw SL lookup but ordinary THEME method lookup.
            let globals = table(call, helper(call, 5)?)?;
            let sl = get_static(call, globals, b"SL")?;
            let mods = call.with_roots(&[sl], |call| {
                let player = get(
                    call,
                    table(call, sl)?,
                    if index == 0 { b"P1" } else { b"P2" },
                )?;
                call.with_roots(&[player], |call| {
                    get_static(call, table(call, player)?, b"ActiveModifiers")
                })
            })?;
            let label = call.with_roots(&[mods], |call| {
                let label = get_static(call, table(call, mods)?, b"HoldJudgment")?;
                if label == Value::Nil {
                    Ok("None 1x2.png".to_owned())
                } else {
                    assets::text(call, label)
                }
            })?;
            let theme = get_static(call, globals, b"THEME")?;
            let path = call.with_roots(&[theme], |call| {
                let function = field(call, table(call, theme)?, "GetPathG")?;
                call.with_roots(&[function], |call| {
                    if !matches!(function, Value::Function(_) | Value::Native(_)) {
                        return Err(call.error(b"THEME.GetPathG function expected"));
                    }
                    let empty = call.try_intern(b"")?;
                    let path = call.try_intern(format!("_HoldJudgments/{label}").as_bytes())?;
                    let value = call
                        .call(function, &[theme, empty, path])?
                        .first()
                        .copied()
                        .unwrap_or(Value::Nil);
                    assets::text(call, value)
                })
            })?;
            for column in 0..crate::song_lua_style_info(style).columns {
                let frame = dummy(call, scene, "HoldJudgment", "", Some(player))?;
                let sprite = dummy(call, scene, "Sprite", "HoldJudgment", Some(frame))?;
                set(
                    call,
                    sprite,
                    "__songlua_sprite_animate",
                    Value::Boolean(false),
                )?;
                let path = call.try_intern(path.as_bytes())?;
                texture(call, sprite, path)?;
                let alpha = field(call, sprite, "diffusealpha")?;
                if !matches!(alpha, Value::Function(_) | Value::Native(_)) {
                    return Err(call.error(b"diffusealpha function expected"));
                }
                call.call(alpha, &[Value::Table(sprite), Value::Number(0.)])?;
                append(call, frame, sprite)?;
                native_children::insert(call, frame, "HoldJudgment", Value::Table(sprite))?;
                call.raw_table_set(
                    frames,
                    Value::Number((column + 1) as f64),
                    Value::Table(frame),
                )?;
                let ordinal = call.raw_table_len(unnamed)? + 1;
                call.raw_table_set(unnamed, Value::Number(ordinal as f64), Value::Table(frame))?;
                append(call, player, frame)?;
            }
            native_children::insert(call, player, "", Value::Table(unnamed))?;
            let key = call.try_intern(b"__songlua_hold_judgments")?;
            call.raw_table_set(player, key, Value::Table(frames))
        })
    })
}

// Reuse Player::Init's actual HUD constructors for compiler fallback children.
pub(super) fn tracked_child(
    call: &mut HostCall<'_>,
    scene: u64,
    player: TableId,
    index: usize,
    name: &str,
) -> Result<TableId, Error> {
    let child = match name {
        "Judgment" => judgment(call, scene, player, index)?,
        "Combo" => combo(call, scene, index)?,
        _ => unreachable!(),
    };
    underlay::copy_tags(call, player, child)?;
    let value = call.try_intern(name.as_bytes())?;
    set(call, child, "__songlua_player_child_name", value)?;
    native_children::adopt(call, child, player)?;
    native_children::insert(call, player, name, Value::Table(child))?;
    append(call, player, child)?;
    lifecycle::invalidate_plans(call)?;
    Ok(child)
}
