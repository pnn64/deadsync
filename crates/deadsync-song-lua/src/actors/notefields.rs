//! Generated NoteField, Board and the eagerly owned style-column vector.
use super::*;
use generated_players::dummy;
use sprites::{field, set};

#[derive(Clone)]
struct Binding {
    scene: u64,
    name: &'static str,
}
#[cfg(feature = "test-support")]
struct Build {
    player: usize,
    style: String,
}
impl Actors {
    #[cfg(feature = "test-support")]
    pub(crate) fn note_field(
        &self,
        vm: &mut Vm,
        player: usize,
        style: &str,
    ) -> Result<ActorScene, Error> {
        let function = vm.new_host_function_with_data(
            construct,
            &[Value::Table(self.registry), Value::Table(self.helpers)],
            Build {
                player,
                style: style.to_owned(),
            },
        )?;
        let Value::Number(id) = vm.call(function, &[])?[0] else {
            unreachable!()
        };
        Ok(ActorScene {
            owner: self.instantiate,
            id: id as u64,
        })
    }
}
#[cfg(feature = "test-support")]
fn construct(call: &mut HostCall<'_>) -> Result<(), Error> {
    let build = call.function_data::<Build>().unwrap();
    let player = build.player;
    let style = build.style.clone();
    let id = {
        let guard = call.store();
        if guard.next >= (1u64 << 53) - 1 {
            return Err(call.error(b"actor scene identity exhausted"));
        }
        guard.next += 1;
        let id = guard.next;
        guard.scenes.insert(
            id,
            Scene {
                actors: Vec::new(),
                root: None,
                ended: false,
                sounds_retired: false,
                initial_states: None,
                song_session: None,
            },
        );
        id
    };
    let result = create(call, id, player, &style);
    match result {
        Ok(actor) => call.store().scenes.get_mut(&id).unwrap().root = Some(actor),
        Err(error) => {
            release(call, id)?;
            return Err(error);
        }
    }
    call.push(Value::Number(id as f64))
}

pub(super) fn create(
    call: &mut HostCall<'_>,
    scene: u64,
    player: usize,
    style: &str,
) -> Result<TableId, Error> {
    let actor = dummy(call, scene, "NoteField", "", None)?;
    let board = dummy(call, scene, "Sprite", "Board", Some(actor))?;
    let path =
        crate::blank_texture::load_native_blank_texture().map_err(|e| call.error(e.as_bytes()))?;
    let path = call.try_intern(path.as_bytes())?;
    sprites::set_texture(call, board, path, false)?;
    call.raw_table_set(actor, Value::Number(1.), Value::Table(board))?;
    call.store()
        .actors
        .get_mut(&actor)
        .unwrap()
        .children
        .push(board);
    native_children::insert(call, actor, "Board", Value::Table(board))?;
    set(
        call,
        actor,
        "__songlua_player_index",
        Value::Number(player as f64),
    )?;
    let style = call.try_intern(style.as_bytes())?;
    set(call, actor, "__songlua_style_name", style)?;
    let kind = call.try_intern(b"NoteField")?;
    set(call, actor, "__songlua_player_child_name", kind)?;
    set(call, actor, "__songlua_beat_bars", Value::Boolean(false))?;
    let alpha = call.try_new_table()?;
    set(
        call,
        actor,
        "__songlua_beat_bars_alpha",
        Value::Table(alpha),
    )?;
    for name in [
        "GetColumnActors",
        "get_column_actors",
        "SetBeatBars",
        "SetBeatBarsAlpha",
        "SetDidTapNoteCallback",
        "did_tap_note",
    ] {
        let function = call.new_host_function_with_data(
            method,
            &[call.captures()[0], call.captures()[1], Value::Table(actor)],
            Binding { scene, name },
        )?;
        call.with_roots(&[function], |call| {
            let value = get(call, actor, b"__songlua_bound_methods")?;
            let bound = table(call, value)?;
            set(call, bound, name, function)?;
            if name == "SetDidTapNoteCallback" {
                set(call, bound, "set_did_tap_note_callback", function)?;
            }
            Ok(())
        })?;
    }
    call.store().actors.get_mut(&actor).unwrap().current.y =
        f32::midpoint(crate::THEME_RECEPTOR_Y_STD, crate::THEME_RECEPTOR_Y_REV);
    columns(call, scene, actor)?;
    Ok(actor)
}

fn text(call: &HostCall<'_>, value: Value) -> Result<String, Error> {
    let Value::String(id) = value else {
        return Err(call.error(b"string field expected"));
    };
    std::str::from_utf8(
        call.string_bytes(id)
            .ok_or_else(|| call.error(b"string is not retained"))?,
    )
    .map(str::to_owned)
    .map_err(|_| call.error(b"UTF-8 string expected"))
}
fn tolerate<T>(result: Result<T, Error>) -> Result<Option<T>, Error> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == ErrorKind::Runtime => Ok(None),
        Err(error) => Err(error),
    }
}
fn current_style(call: &mut HostCall<'_>) -> Result<String, Error> {
    let globals = helper(call, 5)?;
    let selected = tolerate((|| {
        let key = call.try_intern(b"GAMESTATE")?;
        let state = call.table_get(globals, key)?;
        call.with_roots(&[state], |call| {
            let key = call.try_intern(b"__songlua_current_style")?;
            let slots = table(call, call.raw_table_get(table(call, state)?, key)?)?;
            call.raw_table_get(slots, Value::Number(1.))
        })
    })())?;
    let selected = if let Some(selected) = selected {
        Some(selected)
    } else {
        tolerate((|| {
            let key = call.try_intern(b"GAMESTATE")?;
            let state = call.table_get(globals, key)?;
            call.with_roots(&[state], |call| {
                let function = field(call, table(call, state)?, "GetCurrentStyle")?;
                if !matches!(function, Value::Function(_) | Value::Native(_)) {
                    return Err(call.error(b"style getter expected"));
                }
                Ok(call
                    .call(function, &[state])?
                    .first()
                    .copied()
                    .unwrap_or(Value::Nil))
            })
        })())?
    };
    let Some(Value::Table(style)) = selected else {
        return Ok("single".into());
    };
    call.with_roots(&[Value::Table(style)], |call| {
        let key = call.try_intern(b"__songlua_style_key")?;
        if let Some(key) = tolerate(text(call, call.raw_table_get(style, key)?))? {
            return Ok(key);
        }
        let value = tolerate((|| {
            let function = field(call, style, "GetName")?;
            if !matches!(function, Value::Function(_) | Value::Native(_)) {
                return Err(call.error(b"style getter expected"));
            }
            let value = call
                .call(function, &[Value::Table(style)])?
                .first()
                .copied()
                .unwrap_or(Value::Nil);
            text(call, value)
        })())?;
        Ok(value.unwrap_or_else(|| "single".into()))
    })
}

fn columns(call: &mut HostCall<'_>, scene: u64, actor: TableId) -> Result<Value, Error> {
    let cached = field(call, actor, "__songlua_note_columns")?;
    if cached != Value::Nil {
        table(call, cached)?;
        return Ok(cached);
    }
    if !call.store().scenes.contains_key(&scene) {
        return Err(call.error(b"NoteField scene has been released"));
    }
    let columns = call.try_new_table()?;
    call.with_roots(&[Value::Table(columns)], |call| {
        let value = field(call, actor, "__songlua_player_index")?;
        let player = if value == Value::Nil {
            0
        } else {
            let n = sprites::number(call, value)?;
            if !(i64::MIN as f64..-(i64::MIN as f64)).contains(&n) {
                return Err(call.error(b"player index out of range"));
            }
            n as i64 as usize
        };
        let value = field(call, actor, "__songlua_style_name")?;
        let style = if value == Value::Nil {
            current_style(call)?
        } else {
            text(call, value)?
        };
        for index in 0..crate::song_lua_style_info(&style).columns {
            let column = dummy(call, scene, "NoteColumnRenderer", "", Some(actor))?;
            set(
                call,
                column,
                "__songlua_player_index",
                Value::Number(player as i64 as f64),
            )?;
            set(
                call,
                column,
                "__songlua_column_index",
                Value::Number(index as f64),
            )?;
            let x = crate::song_lua_style_column_x(&style, index);
            if !call
                .store()
                .clock
                .live
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                call.store().actors.get_mut(&column).unwrap().current.x = x;
            }
            for (key, value) in [
                ("__songlua_state_x", x),
                ("__songlua_state_y", 0.),
                ("__songlua_state_z", 0.),
            ] {
                set(call, column, key, Value::Number(value as f64))?;
            }
            for (field_name, name, alias) in [
                ("__songlua_pos_handler", "GetPosHandler", "get_pos_handler"),
                ("__songlua_rot_handler", "GetRotHandler", "get_rot_handler"),
                (
                    "__songlua_zoom_handler",
                    "GetZoomHandler",
                    "get_zoom_handler",
                ),
            ] {
                let handler = splines::handler(call)?;
                set(call, column, field_name, Value::Table(handler))?;
                let getter = call.new_host_function(
                    |call| call.push(call.captures()[0]),
                    &[Value::Table(handler)],
                )?;
                let value = get(call, column, b"__songlua_bound_methods")?;
                let bound = table(call, value)?;
                set(call, bound, name, getter)?;
                set(call, bound, alias, getter)?;
            }
            call.raw_table_set(
                columns,
                Value::Number((index + 1) as f64),
                Value::Table(column),
            )?;
        }
        set(call, actor, "__songlua_note_columns", Value::Table(columns))?;
        lifecycle::invalidate_plans(call)?;
        Ok(Value::Table(columns))
    })
}

fn method(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Binding>().unwrap().clone();
    let actor = table(call, call.captures()[2])?;
    let offset = usize::from(matches!(arg(call, 0), Value::Table(_)));
    match data.name {
        "GetColumnActors" => {
            let child = field(call, actor, "__songlua_player_child_name")?;
            let mut valid =
                child != Value::Nil && text(call, child)?.eq_ignore_ascii_case("NoteField");
            if !valid {
                let kind = field(call, actor, "__songlua_actor_type")?;
                valid = kind != Value::Nil && text(call, kind)?.eq_ignore_ascii_case("NoteField");
            }
            let value = if valid {
                columns(call, data.scene, actor)?
            } else {
                Value::Nil
            };
            return call.push(value);
        }
        "get_column_actors" => {
            let function = field(call, actor, "GetColumnActors")?;
            let value = if matches!(function, Value::Function(_) | Value::Native(_)) {
                call.call(function, &[Value::Table(actor)])?
                    .first()
                    .copied()
                    .unwrap_or(Value::Nil)
            } else {
                Value::Nil
            };
            return call.push(value);
        }
        "SetBeatBars" => {
            if let Some(value) = call.arguments().get(offset).copied() {
                set(
                    call,
                    actor,
                    "__songlua_beat_bars",
                    Value::Boolean(!matches!(value, Value::Nil | Value::Boolean(false))),
                )?;
                return finish_effect(call, actor);
            }
        }
        "SetBeatBarsAlpha" => {
            let alpha = call.try_new_table()?;
            call.with_roots(&[Value::Table(alpha)], |call| {
                for index in 0..4 {
                    let value =
                        splines::optional_float(call, arg(call, offset + index)).unwrap_or(0.);
                    call.raw_table_set(
                        alpha,
                        Value::Number((index + 1) as f64),
                        Value::Number(value as f64),
                    )?;
                }
                set(
                    call,
                    actor,
                    "__songlua_beat_bars_alpha",
                    Value::Table(alpha),
                )
            })?;
            return finish_effect(call, actor);
        }
        "SetDidTapNoteCallback" => {
            let value = arg(call, 1);
            let value = if matches!(value, Value::Function(_) | Value::Native(_)) {
                value
            } else {
                Value::Nil
            };
            set(call, actor, "__songlua_did_tap_note_callback", value)?;
        }
        "did_tap_note" => {
            let column = match arg(call, offset) {
                Value::Number(n) if n.is_finite() => {
                    n.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32
                }
                Value::String(id) => call
                    .string_bytes(id)
                    .and_then(|s| std::str::from_utf8(s).ok())
                    .and_then(|s| {
                        s.trim()
                            .parse::<i32>()
                            .ok()
                            .or_else(|| s.trim().parse::<f32>().ok().map(|n| n.round() as i32))
                    })
                    .unwrap_or(0),
                _ => 0,
            };
            let score = arg(call, offset + 1);
            let bright = !matches!(arg(call, offset + 2), Value::Nil | Value::Boolean(false));
            set(
                call,
                actor,
                "__songlua_last_tap_note_column",
                Value::Number(column as f64),
            )?;
            set(call, actor, "__songlua_last_tap_note_score", score)?;
            set(
                call,
                actor,
                "__songlua_last_tap_note_bright",
                Value::Boolean(bright),
            )?;
            let callback = field(call, actor, "__songlua_did_tap_note_callback")?;
            if callback != Value::Nil {
                if !matches!(callback, Value::Function(_) | Value::Native(_)) {
                    return Err(call.error(b"tap callback must be a function"));
                }
                call.call(
                    callback,
                    &[Value::Number(column as f64), score, Value::Boolean(bright)],
                )?;
            }
            return finish_effect(call, actor);
        }
        _ => unreachable!(),
    }
    call.push(Value::Table(actor))
}

fn finish_effect(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let globals = helper(call, 5)?;
    let mut stack = call.native_stack()?;
    stack.push(globals)?;
    let target = stack.top();
    stack.get_field(
        target as isize,
        crate::SONG_LUA_SIDE_EFFECT_COUNT_KEY.as_bytes(),
    )?;
    let count = if stack.value(-1) == Some(Value::Nil) {
        0
    } else {
        let value = stack.number(-1)?;
        if !(i64::MIN as f64..-(i64::MIN as f64)).contains(&value) {
            return Err(stack.error(b"side effect count is out of range"));
        }
        value as i64
    };
    stack.push_bytes(crate::SONG_LUA_SIDE_EFFECT_COUNT_KEY.as_bytes())?;
    stack.push(Value::Number(count.saturating_add(1) as f64))?;
    stack.set_table(target as isize, false)?;
    stack.push(Value::Table(actor))?;
    stack.finish(1)
}

pub(super) fn update_columns(
    call: &mut HostCall<'_>,
    actor: TableId,
) -> Result<Vec<TableId>, Error> {
    let key = call.try_intern(b"__songlua_note_columns")?;
    let columns = call.raw_table_get(actor, key)?;
    if columns == Value::Nil {
        return Ok(Vec::new());
    }
    let columns = table(call, columns)?;
    let mut result = Vec::new();
    for index in 1usize.. {
        let value = call.raw_table_get(columns, Value::Number(index as f64))?;
        if value == Value::Nil {
            break;
        }
        result.push(table(call, value)?);
    }
    Ok(result)
}
