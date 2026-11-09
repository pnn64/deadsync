//! ScreenGameplay controls, installed on the screen after its children exist.
use super::super::managers::screen::{Requests, capture};
use super::*;
use deadlib_lua::NativeStack;
use generated_players::dummy;
use sprites::set;

#[derive(Clone)]
struct Binding {
    store: Shared,
    requests: Arc<Mutex<Requests>>,
    enabled: [bool; crate::LUA_PLAYERS],
    method: &'static str,
}

impl Actors {
    // Screen allocation precedes generated player/theme adoption and controls.
    #[cfg(feature = "test-support")]
    pub(crate) fn top_screen_base(
        &self,
        vm: &mut Vm,
        context: &crate::SongLuaCompileContext,
    ) -> Result<ActorScene, Error> {
        let function = vm.new_host_function_with_data(
            create_base,
            &[Value::Table(self.registry), Value::Table(self.helpers)],
            (self.store.clone(), context.clone()),
        )?;
        let Value::Number(id) = vm.call(function, &[])?[0] else {
            unreachable!()
        };
        Ok(ActorScene {
            owner: self.instantiate,
            id: id as u64,
        })
    }

    pub(crate) fn bind_top_screen(
        &self,
        vm: &mut Vm,
        catalog: Value,
        screen_and_children: [Value; 5],
        enabled: [bool; crate::LUA_PLAYERS],
        requests: Arc<Mutex<Requests>>,
    ) -> Result<(), Error> {
        let mut captures = vec![
            Value::Table(self.registry),
            Value::Table(self.helpers),
            catalog,
        ];
        captures.extend(screen_and_children);
        let function = vm.new_host_function_with_data(
            install,
            &captures,
            Binding {
                store: self.store.clone(),
                requests,
                enabled,
                method: "",
            },
        )?;
        vm.call(function, &[]).map(|_| ())
    }
}

#[cfg(feature = "test-support")]
fn create_base(call: &mut HostCall<'_>) -> Result<(), Error> {
    let (store, context) = call
        .function_data::<(Shared, crate::SongLuaCompileContext)>()
        .unwrap()
        .clone();
    let scene = new_scene(call, &store)?;
    let previous = {
        let mut guard = store.lock().unwrap();
        let capture = !guard.clock.live.load(std::sync::atomic::Ordering::Relaxed);
        std::mem::replace(&mut guard.constructing_capture, capture)
    };
    let result = (|| {
        let actor = create(call, &store, scene, &context)?;
        store.lock().unwrap().scenes.get_mut(&scene).unwrap().root = Some(actor);
        Ok(())
    })();
    store.lock().unwrap().constructing_capture = previous;
    if let Err(error) = result {
        release(call, &store, scene)?;
        return Err(error);
    }
    call.push(Value::Number(scene as f64))
}

pub(super) fn create(
    call: &mut HostCall<'_>,
    store: &Shared,
    scene: u64,
    context: &crate::SongLuaCompileContext,
) -> Result<TableId, Error> {
    let actor = dummy(call, store, scene, "TopScreen", "ScreenGameplay", None)?;
    for (key, value) in [
        ("__songlua_player1_enabled", context.players[0].enabled),
        ("__songlua_player2_enabled", context.players[1].enabled),
    ] {
        set(call, actor, key, Value::Boolean(value))?;
    }
    let title = call.try_intern(context.main_title.as_bytes())?;
    set(call, actor, "__songlua_main_title", title)?;
    let bpm = crate::display_bpms_text(context.song_display_bpms, crate::song_music_rate(context));
    let bpm = call.try_intern(bpm.as_bytes())?;
    set(call, actor, "__songlua_bpm_text", bpm)?;
    Ok(actor)
}

// Bound methods retain the original screen, enabled players and backing slots.
// Replacing the published screen or GAMESTATE does not retarget saved methods.
fn install(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Binding>().unwrap().clone();
    data.requests.lock().unwrap().actor_names = Some(ActorNames(Arc::downgrade(&data.store)));
    let catalog = table(call, call.captures()[2])?;
    let slots = get(call, catalog, b"game_state_slots")?;
    let screen = table(call, call.captures()[3])?;
    let mut captures = call.captures().to_vec();
    captures[2] = slots;
    let wheel = call.try_new_table()?;
    call.with_roots(&[Value::Table(wheel)], |call| {
        captures.push(Value::Table(wheel));
        captures.push(helper(call, 5)?);
        for method in [
            "IsLocked",
            "GetSelectedSection",
            "GetSelectedSong",
            "GetSelectedType",
            "GetSortOrder",
            "ChangeSort",
            "SetOpenSection",
            "Move",
        ] {
            let method_captures = method_captures(call, &captures, method)?;
            let function = call.new_host_function_with_data(
                callback,
                &method_captures,
                Binding {
                    method,
                    ..data.clone()
                },
            )?;
            set(call, wheel, method, function)?;
        }
        for method in [
            "GetScreenType",
            "SetMinSecondsToMusic",
            "GetChild",
            "GetLifeMeter",
            "StartTransitioningScreen",
            "SetNextScreenName",
            "SetPrevScreenName",
            "GetNextScreenName",
            "GetPrevScreenName",
            "GetGoToOptions",
            "GetEditState",
            "GetCurrentRowIndex",
            "GetNumRows",
            "GetOptionRow",
            "SetOptionRowIndex",
            "RedrawOptions",
            "IsPaused",
            "PauseGame",
            "AllAreOnLastRow",
            "GetMusicWheel",
            "GetNextCourseSong",
            "PostScreenMessage",
            "SetProfileIndex",
            "Cancel",
            "Continue",
            "Finish",
            "Load",
            "begin_backing_out",
        ] {
            let method_captures = method_captures(call, &captures, method)?;
            let function = call.new_host_function_with_data(
                callback,
                &method_captures,
                Binding {
                    method,
                    ..data.clone()
                },
            )?;
            let bound = get(call, screen, b"__songlua_bound_methods")?;
            set(call, table(call, bound)?, method, function)?;
        }
        Ok(())
    })
}

fn argument(call: &HostCall<'_>, index: usize) -> Value {
    let offset = usize::from(matches!(call.arguments().first(), Some(Value::Table(_))));
    arg(call, index + offset)
}
fn string(call: &HostCall<'_>, value: Value) -> Option<String> {
    let Value::String(id) = value else {
        return None;
    };
    std::str::from_utf8(call.string_bytes(id)?)
        .ok()
        .map(str::to_owned)
}
fn integer(call: &HostCall<'_>, value: Value) -> Option<i32> {
    match value {
        Value::Number(value) if value.is_finite() => {
            Some(value.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32)
        }
        Value::String(_) => {
            let value = string(call, value)?;
            value
                .trim()
                .parse()
                .ok()
                .or_else(|| value.trim().parse::<f32>().ok().map(|n| n.round() as i32))
        }
        _ => None,
    }
}

fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Binding>().unwrap().clone();
    let screen_value = call.captures()[3];
    let first = argument(call, 0);
    match data.method {
        "SetMinSecondsToMusic" => {
            let receiver = table(call, arg(call, 0))?;
            let seconds = call.number(1)? as f32;
            if !seconds.is_finite() || !(0.0..=60.0).contains(&seconds) {
                return Err(call.error(b"music lead-in must be between 0 and 60 seconds"));
            }
            let key = call.try_intern(b"__songlua_min_seconds_to_music")?;
            call.raw_table_set(receiver, key, Value::Number(seconds as f64))?;
            return call.push(Value::Table(receiver));
        }
        "GetOptionRow" => {
            let name = if matches!(first, Value::String(_)) {
                string(call, first).unwrap_or_default()
            } else {
                integer(call, first)
                    .and_then(crate::top_screen_option_row_name_at)
                    .unwrap_or(crate::SONG_LUA_TOP_SCREEN_OPTION_ROWS[0])
                    .to_owned()
            };
            let scene = new_scene(call, &data.store)?;
            data.store.lock().unwrap().constructor_scenes.insert(scene);
            let previous = {
                let mut guard = data.store.lock().unwrap();
                let capture = !guard.clock.live.load(std::sync::atomic::Ordering::Relaxed);
                std::mem::replace(&mut guard.constructing_capture, capture)
            };
            let row = option_row(call, &data.store, scene, &name);
            data.store.lock().unwrap().constructing_capture = previous;
            let row = match row {
                Ok(row) => row,
                Err(error) => {
                    release(call, &data.store, scene)?;
                    return Err(error);
                }
            };
            data.store
                .lock()
                .unwrap()
                .scenes
                .get_mut(&scene)
                .unwrap()
                .root = Some(row);
            return call.push(Value::Table(row));
        }
        "GetChild" => {
            let screen = table(call, screen_value)?;
            let Some(name) = string(call, first) else {
                return call.push(Value::Nil);
            };
            let player = crate::top_screen_player_index(&name)
                .map(|i| (i, 4 + i))
                .or_else(|| crate::top_screen_life_meter_index(&name).map(|i| (i, 6 + i)));
            if let Some((index, slot)) = player {
                return call.push(if data.enabled[index] {
                    call.captures()[slot]
                } else {
                    Value::Nil
                });
            }
            let named = registry(call, screen)?;
            return call.with_roots(&[named], |call| {
                let key = call.try_intern(name.as_bytes())?;
                let value = call.table_get(named, key)?;
                if !matches!(value, Value::Nil | Value::Table(_)) {
                    return Err(call.error(b"screen child table expected"));
                }
                call.push(value)
            });
        }
        "GetLifeMeter" => {
            let index = match first {
                Value::Number(n) if n >= 0. && n < crate::LUA_PLAYERS as f64 && n.fract() == 0. => {
                    Some(n as usize)
                }
                Value::String(id) => call.string_bytes(id).and_then(|s| {
                    if s.eq_ignore_ascii_case(b"P1") || s.eq_ignore_ascii_case(b"PlayerNumber_P1") {
                        Some(0)
                    } else if s.eq_ignore_ascii_case(b"P2")
                        || s.eq_ignore_ascii_case(b"PlayerNumber_P2")
                    {
                        Some(1)
                    } else {
                        None
                    }
                }),
                _ => None,
            };
            return call.push(
                index
                    .filter(|&i| data.enabled[i])
                    .map_or(Value::Nil, |i| call.captures()[6 + i]),
            );
        }
        "StartTransitioningScreen" => {
            let screen = table(call, screen_value)?;
            let globals = call.captures()[9];
            let live = data
                .store
                .lock()
                .unwrap()
                .clock
                .live
                .load(std::sync::atomic::Ordering::Relaxed);
            let message = string(call, first);
            {
                let mut stack = call.native_stack()?;
                stack.push(globals)?;
                let globals_index = stack.top();
                side_effect(&mut stack, globals_index)?;
                if live {
                    let message = message
                        .as_ref()
                        .ok_or_else(|| stack.error(b"expected a screen transition message"))?;
                    capture(
                        &mut stack,
                        &data.requests,
                        globals_index,
                        crate::SongLuaLiveRequest::Transition(message.clone()),
                    )?;
                }
            }
            if live {
                for child in children(call, &data.store, screen)? {
                    play(call, &data.store, child, b"Off", None, true)?;
                }
                let named = registry(call, screen)?;
                let out =
                    call.with_roots(&[named], |call| get(call, table(call, named)?, b"Out"))?;
                if out != Value::Nil {
                    transitions::start(call, &data.store, table(call, out)?)?;
                }
                data.requests.lock().unwrap().transition = message;
            }
            return call.push(screen_value);
        }
        _ => (),
    }
    let second = argument(call, 1);
    let arg_count = call.arguments().len()
        - usize::from(matches!(call.arguments().first(), Some(Value::Table(_))));
    let first_text = string(call, first);
    let row = integer(call, if arg_count > 1 { second } else { first });
    let slots = call.captures()[2];
    let wheel = call.captures()[8];
    let globals = call.captures()[9];
    let mut stack = call.native_stack()?;
    stack.push(screen_value)?;
    let owner = stack.top();
    let mut result = screen_value;
    let mut effect = false;
    match data.method {
        "GetScreenType" => stack.push_bytes(b"ScreenType_Gameplay")?,
        "GetGoToOptions" | "AllAreOnLastRow" | "IsLocked" => stack.push(Value::Boolean(false))?,
        "GetEditState" => stack.push_bytes(b"EditState_Playing")?,
        "GetSelectedType" => stack.push_bytes(b"WheelItemDataType_Song")?,
        "GetSelectedSection" => stack.push_bytes(b"")?,
        "GetSelectedSong" => stack.push(Value::Nil)?,
        "GetMusicWheel" => stack.push(wheel)?,
        "GetNumRows" => stack.push(Value::Number(
            crate::SONG_LUA_TOP_SCREEN_OPTION_ROWS.len() as f64
        ))?,
        "GetNextScreenName" | "GetPrevScreenName" | "GetCurrentRowIndex" | "IsPaused" => {
            let key = match data.method {
                "GetNextScreenName" => b"__songlua_next_screen_name".as_slice(),
                "GetPrevScreenName" => b"__songlua_prev_screen_name",
                "GetCurrentRowIndex" => b"__songlua_current_row_index",
                _ => b"__songlua_paused",
            };
            stack.get_field(owner as isize, key)?;
            if data.method == "IsPaused" {
                stack.push(Value::Boolean(stack.to_boolean(-1)))?;
            } else if data.method == "GetCurrentRowIndex" {
                let n = if stack.value(-1) == Some(Value::Nil) {
                    0
                } else {
                    checked_i64(&stack, -1)?
                };
                stack.push(Value::Number(n as f64))?;
            } else if stack.value(-1) == Some(Value::Nil) {
                stack.push_bytes(b"")?;
            } else {
                let text = string_bytes(&mut stack, -1)?;
                stack.push_bytes(&text)?;
            }
        }
        "SetNextScreenName" | "SetPrevScreenName" | "PauseGame" | "SetOptionRowIndex" => {
            let key = match data.method {
                "SetNextScreenName" => b"__songlua_next_screen_name".as_slice(),
                "SetPrevScreenName" => b"__songlua_prev_screen_name",
                "PauseGame" => b"__songlua_paused",
                _ => b"__songlua_current_row_index",
            };
            if data.method != "SetOptionRowIndex" || row.is_some() {
                stack.push_bytes(key)?;
                match data.method {
                    "PauseGame" => stack.push(Value::Boolean(!matches!(
                        first,
                        Value::Nil | Value::Boolean(false)
                    )))?,
                    "SetOptionRowIndex" => stack.push(Value::Number(row.unwrap().max(0) as f64))?,
                    _ => stack.push_bytes(first_text.as_deref().unwrap_or_default().as_bytes())?,
                }
                stack.set_table(owner as isize, false)?;
            }
            effect = true;
        }
        "GetSortOrder" | "GetNextCourseSong" | "ChangeSort" => {
            stack.push(slots)?;
            let slot = stack.top();
            if data.method == "ChangeSort" {
                if let Some(name) = first_text {
                    stack.push(Value::Number(1.))?;
                    stack.push_bytes(name.as_bytes())?;
                    stack.set_table(slot as isize, true)?;
                }
                effect = true;
                result = wheel;
            } else {
                stack.push(Value::Number(1.))?;
                stack.get_table(slot as isize, true)?;
                if data.method == "GetSortOrder" {
                    if stack.value(-1) == Some(Value::Nil) {
                        stack.push_bytes(b"SortOrder_Group")?;
                    } else {
                        let bytes = string_bytes(&mut stack, -1)?;
                        stack.push_bytes(&bytes)?;
                    }
                } else if !matches!(stack.value(-1), Some(Value::Nil | Value::Table(_))) {
                    return Err(stack.error(b"song table expected"));
                }
            }
        }
        "SetOpenSection" | "Move" => {
            effect = true;
            result = wheel;
        }
        "RedrawOptions" | "PostScreenMessage" | "SetProfileIndex" | "Cancel" | "Continue"
        | "Finish" | "Load" | "begin_backing_out" => effect = true,
        _ => unreachable!(),
    }
    if effect {
        stack.push(globals)?;
        let index = stack.top();
        side_effect(&mut stack, index)?;
        stack.push(result)?;
    }
    stack.finish(1)
}

fn string_bytes(stack: &mut NativeStack<'_, '_>, index: isize) -> Result<Vec<u8>, Error> {
    let value = match stack.to_bytes(index)? {
        Some(bytes) => bytes.to_vec(),
        None => return Err(stack.error(b"string expected")),
    };
    if std::str::from_utf8(&value).is_err() {
        return Err(stack.error(b"UTF-8 string expected"));
    }
    Ok(value)
}

fn checked_i64(stack: &NativeStack<'_, '_>, index: isize) -> Result<i64, Error> {
    let value = stack.number(index)?;
    if !(i64::MIN as f64..-(i64::MIN as f64)).contains(&value) {
        return Err(stack.error(b"integer out of range"));
    }
    Ok(value as i64)
}
fn side_effect(stack: &mut NativeStack<'_, '_>, globals: usize) -> Result<(), Error> {
    stack.get_field(
        globals as isize,
        crate::SONG_LUA_SIDE_EFFECT_COUNT_KEY.as_bytes(),
    )?;
    let count = if stack.value(-1) == Some(Value::Nil) {
        0
    } else {
        checked_i64(stack, -1)?
    };
    stack.push_bytes(crate::SONG_LUA_SIDE_EFFECT_COUNT_KEY.as_bytes())?;
    stack.push(Value::Number(count.saturating_add(1) as f64))?;
    stack.set_table(globals as isize, false)
}

fn option_row(
    call: &mut HostCall<'_>,
    store: &Shared,
    scene: u64,
    name: &str,
) -> Result<TableId, Error> {
    let row = dummy(call, store, scene, "OptionRow", name, None)?;
    let name_value = call.try_intern(name.as_bytes())?;
    let get_name = call.new_host_function(|call| call.push(call.captures()[0]), &[name_value])?;
    let bound = get(call, row, b"__songlua_bound_methods")?;
    set(call, table(call, bound)?, "GetName", get_name)?;
    bind_queries(call, row)?;
    let focus = call.new_host_function(|call| call.push(Value::Number(1.)), &[])?;
    set(call, table(call, bound)?, "GetChoiceInRowWithFocus", focus)?;
    let frame = dummy(call, store, scene, "OptionRowFrame", "", Some(row))?;
    bind_queries(call, frame)?;
    native_children::insert(call, row, "", Value::Table(frame))?;
    let title = screen_parts::text(call, store, scene, "Title", name)?;
    screen_parts::named(call, store, frame, "Title", title)?;
    let members = call.try_new_table()?;
    let group = call.with_roots(&[Value::Table(members)], |call| {
        Ok(call.call(helper(call, 3)?, &[Value::Table(members)])?[0])
    })?;
    call.with_roots(&[group], |call| {
        let text = crate::option_row_default_text(name);
        for index in 1..=crate::LUA_PLAYERS {
            let item = screen_parts::text(call, store, scene, "Item", &text)?;
            native_children::adopt(call, store, item, frame)?;
            call.raw_table_set(
                table(call, group)?,
                Value::Number(index as f64),
                Value::Table(item),
            )?;
        }
        native_children::insert(call, frame, "Item", group)
    })?;
    Ok(row)
}

fn registry(call: &mut HostCall<'_>, actor: TableId) -> Result<Value, Error> {
    let registry = native_children::helper_function(call, b"native_registry")?;
    Ok(call.call(registry, &[Value::Table(actor)])?[0])
}
pub(super) fn bind_queries(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let methods = native_children::helper_function(call, b"model_methods")?;
    let bound = get(call, actor, b"__songlua_bound_methods")?;
    for name in ["GetChild", "GetChildren", "GetNumChildren"] {
        let method = get(call, table(call, methods)?, name.as_bytes())?;
        let captures = [
            call.captures()[0],
            call.captures()[1],
            Value::Table(actor),
            method,
        ];
        let function = call.new_host_function_with_data(invoke_bound, &captures, name)?;
        set(call, table(call, bound)?, name, function)?;
    }
    Ok(())
}
fn method_captures(
    call: &mut HostCall<'_>,
    source: &[Value],
    method: &str,
) -> Result<[Value; 10], Error> {
    let mut captures = [Value::Nil; 10];
    let indices: &[usize] = match method {
        "GetScreenType"
        | "GetGoToOptions"
        | "GetEditState"
        | "GetNumRows"
        | "AllAreOnLastRow"
        | "SetMinSecondsToMusic"
        | "IsLocked"
        | "GetSelectedSection"
        | "GetSelectedSong"
        | "GetSelectedType" => &[],
        "GetOptionRow" => &[0, 1],
        "GetChild" => &[0, 1, 3, 4, 5, 6, 7],
        "GetLifeMeter" => &[6, 7],
        "StartTransitioningScreen" => &[0, 1, 3, 9],
        "GetMusicWheel" => &[8],
        "GetSortOrder" | "GetNextCourseSong" => &[],
        "ChangeSort" | "SetOpenSection" | "Move" => &[8, 9],
        "GetNextScreenName" | "GetPrevScreenName" | "GetCurrentRowIndex" | "IsPaused" => &[3],
        _ => &[3, 9],
    };
    for &i in indices {
        captures[i] = source[i];
    }
    if matches!(method, "GetSortOrder" | "ChangeSort" | "GetNextCourseSong") {
        captures[2] = get(
            call,
            table(call, source[2])?,
            if method == "GetNextCourseSong" {
                b"song"
            } else {
                b"sort"
            },
        )?;
    }
    Ok(captures)
}
