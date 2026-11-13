//! Process-owned SCREENMAN publication and ordered screen request observations.
use super::super::SongRuntime;
use super::catalog::{arg, text};
use super::*;
use crate::{LUA_PLAYERS, SongLuaLiveRequest, SongLuaLiveSourceRequest};
use deadlib_lua::{NativeStack, Root};
use std::sync::{Arc, Mutex};

pub(crate) struct Screen {
    globals: Root,
    pub state: Shared,
}
type Shared = Arc<Mutex<Requests>>;
#[derive(Default)]
pub(crate) struct Requests {
    pub next: Option<String>,
    pub actor_names: Option<super::super::actors::ActorNames>,
    #[cfg(feature = "test-support")]
    pub observations: crate::observations::Observations,
    pub transition: Option<String>,
    pub source: Option<Vec<SongLuaLiveSourceRequest>>,
}
struct Binding {
    state: Shared,
    name: &'static str,
}

pub(crate) fn install(
    vm: &mut Vm,
    #[cfg(feature = "test-support")] observations: crate::observations::Observations,
) -> Result<Screen, Error> {
    let globals = vm.get_global(b"_G")?;
    Ok(Screen {
        globals: vm.pin(globals)?,
        state: Arc::new(Mutex::new(Requests {
            #[cfg(feature = "test-support")]
            observations,
            ..Default::default()
        })),
    })
}
impl SongRuntime {
    /// Publish an already constructed screen and its player actors. SCREENMAN
    /// survives replacement; saved GetPlayerAF functions retain their own pair.
    /// Actor construction/lifecycle and input binding are separate host phases.
    pub fn publish_screen(
        &mut self,
        screen: Value,
        players: [Value; LUA_PLAYERS],
    ) -> Result<(), Error> {
        let globals = self.vm.rooted(self.screen.globals)?;
        let state = self.screen.state.clone();
        self.vm
            .with_native_stack(|stack| publish(stack, globals, &state, screen, players))
    }
    /// Enable observation without clearing requests already captured this session.
    pub fn begin_screen_request_capture(&self) {
        self.screen
            .state
            .lock()
            .unwrap()
            .source
            .get_or_insert_with(Vec::new);
    }
    /// Consume the latest target and all authored observations. A pending new
    /// screen wins this drain; an earlier transition remains for the next drain.
    pub fn take_screen_requests(
        &self,
    ) -> (Option<SongLuaLiveRequest>, Vec<SongLuaLiveSourceRequest>) {
        let mut state = self.screen.state.lock().unwrap();
        let request = if let Some(name) = state.next.take() {
            Some(SongLuaLiveRequest::NewScreen(name))
        } else {
            state.transition.take().map(SongLuaLiveRequest::Transition)
        };
        let source = state.source.as_mut().map_or_else(Vec::new, std::mem::take);
        (request, source)
    }
}

fn publish(
    stack: &mut NativeStack<'_, '_>,
    globals: Value,
    state: &Shared,
    screen: Value,
    players: [Value; LUA_PLAYERS],
) -> Result<(), Error> {
    let base = stack.top();
    // All three caller-supplied tables must survive reentrant global setters.
    for value in [screen, players[0], players[1]] {
        if !matches!(value, Value::Table(_)) {
            return Err(stack.error(b"screen/player table expected"));
        }
        stack.push(value)?;
    }
    stack.push(globals)?;
    let globals_index = stack.top();
    for (name, value) in [
        ("__songlua_top_screen_player_1", players[0]),
        ("__songlua_top_screen_player_2", players[1]),
        ("__songlua_top_screen", screen),
    ] {
        stack.push_bytes(name.as_bytes())?;
        stack.push(value)?;
        stack.set_table(globals_index as isize, false)?;
    }
    stack.push_bytes(b"GetPlayerAF")?;
    stack.push(players[0])?;
    stack.push(players[1])?;
    stack.push_closure(player_actor, 2, ())?;
    stack.set_table(globals_index as isize, false)?;
    stack.push_bytes(b"SCREENMAN")?;
    stack.get_table(globals_index as isize, true)?;
    if matches!(stack.value(-1), Some(Value::Table(_))) {
        return stack.set_top(base);
    }
    stack.set_top(globals_index)?;
    stack.create_table(0, 0)?;
    let manager = stack.top();
    for name in [
        "GetTopScreen",
        "SystemMessage",
        "SetNewScreen",
        "AddNewScreenToTop",
        "set_input_redirected",
    ] {
        stack.push_bytes(name.as_bytes())?;
        stack.push(globals)?;
        stack.push_closure(
            callback,
            1,
            Binding {
                state: state.clone(),
                name,
            },
        )?;
        stack.set_table(manager as isize, true)?;
    }
    let value = stack.value(manager as isize).unwrap();
    stack.push_bytes(b"SCREENMAN")?;
    stack.push(value)?;
    stack.set_table(globals_index as isize, false)?;
    stack.set_top(base)
}
fn player_actor(call: &mut HostCall<'_>) -> Result<(), Error> {
    let value =
        super::game_state::player(call, arg(call, 0)).map_or(Value::Nil, |i| call.captures()[i]);
    call.push(value)
}
fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Binding>().unwrap();
    let method = data.name;
    let state = data.state.clone();
    let globals = call.captures()[0];
    let name = text(call, arg(call, 0));
    #[cfg(feature = "test-support")]
    if method == "SetNewScreen"
        && let Some(name) = name.as_ref()
    {
        let observations = state.lock().unwrap().observations.clone();
        observations.state_call(
            call,
            globals,
            "ScreenManager.SetNewScreen",
            vec![serde_json::json!(name)],
        )?;
    }
    let mut stack = call.native_stack()?;
    stack.push(globals)?;
    let globals_index = stack.top();
    match method {
        "GetTopScreen" => {
            stack.push_bytes(b"__songlua_screen_loading")?;
            stack.get_table(globals_index as isize, true)?;
            let loading = stack.to_boolean(-1);
            stack.set_top(globals_index)?;
            if loading {
                stack.push(Value::Nil)?;
            } else {
                stack.push_bytes(b"__songlua_top_screen")?;
                stack.get_table(globals_index as isize, true)?;
            }
            return stack.finish(1);
        }
        "SetNewScreen" => {
            if let Some(name) = name {
                capture(
                    &mut stack,
                    &state,
                    globals_index,
                    SongLuaLiveRequest::NewScreen(name.clone()),
                )?;
                state.lock().unwrap().next = Some(name);
            }
        }
        "AddNewScreenToTop" => {
            if let Some(name) = name {
                stack.get_field(globals_index as isize, b"__songlua_top_screen")?;
                if !matches!(stack.value(-1), Some(Value::Table(_))) {
                    return Err(stack.error(b"screen table expected"));
                }
                let top = stack.top();
                let Some(Value::Table(actor)) = stack.value(-1) else {
                    unreachable!("screen table checked")
                };
                stack.push_bytes(b"Name")?;
                stack.push_bytes(name.as_bytes())?;
                stack.set_table(top as isize, false)?;
                // SCREENMAN changes the native name too. Ordinary authored
                // Name assignments keep their existing instance-field policy.
                let names = state.lock().unwrap().actor_names.clone();
                if names.and_then(|names| names.rename(actor, name.as_bytes())) == Some(true) {
                    for key in [b"__songlua_load_name".as_slice(), b"__songlua_native_name"] {
                        if key == b"__songlua_load_name" {
                            stack.push_bytes(key)?;
                            stack.get_table(top as isize, true)?;
                            let exists = stack.value(-1) != Some(Value::Nil);
                            stack.set_top(top)?;
                            if !exists {
                                continue;
                            }
                        }
                        stack.push_bytes(key)?;
                        stack.push_bytes(name.as_bytes())?;
                        stack.set_table(top as isize, true)?;
                    }
                }
            }
        }
        "SystemMessage" | "set_input_redirected" => (),
        _ => unreachable!(),
    }
    stack.get_field(
        globals_index as isize,
        crate::SONG_LUA_SIDE_EFFECT_COUNT_KEY.as_bytes(),
    )?;
    // Match the typed i64 field read for both numbers and numeric strings:
    // reject nonfinite/out-of-range values, then truncate toward zero. This
    // direct conversion introduces no internal tonumber call for debug hooks.
    let count = if stack.value(-1) == Some(Value::Nil) {
        0
    } else {
        let number = stack.number(-1)?;
        if !(i64::MIN as f64..-(i64::MIN as f64)).contains(&number) {
            return Err(stack.error(b"side effect count is out of range"));
        }
        number as i64
    };
    stack.push_bytes(crate::SONG_LUA_SIDE_EFFECT_COUNT_KEY.as_bytes())?;
    stack.push(Value::Number(count.saturating_add(1) as f64))?;
    stack.set_table(globals_index as isize, false)?;
    stack.finish(0)
}
pub(crate) fn capture(
    stack: &mut NativeStack<'_, '_>,
    state: &Shared,
    globals: usize,
    request: SongLuaLiveRequest,
) -> Result<(), Error> {
    if state.lock().unwrap().source.is_none() {
        return Ok(());
    }
    let base = stack.top();
    stack.get_field(globals as isize, crate::SONG_LUA_RUNTIME_KEY.as_bytes())?;
    if !matches!(stack.value(-1), Some(Value::Table(_))) {
        return Err(stack.error(b"runtime table expected"));
    }
    let runtime = stack.top();
    // Read and convert beat first even though only seconds enters this record.
    // Keep the selected runtime rooted across getters that replace it and collect.
    stack.get_field(
        runtime as isize,
        crate::SONG_LUA_RUNTIME_BEAT_KEY.as_bytes(),
    )?;
    stack.number(-1)?;
    stack.get_field(
        runtime as isize,
        crate::SONG_LUA_RUNTIME_SECONDS_KEY.as_bytes(),
    )?;
    let seconds = f64::from(stack.number(-1)? as f32);
    if let Some(source) = state.lock().unwrap().source.as_mut() {
        source.push(SongLuaLiveSourceRequest {
            music_seconds: seconds,
            request,
        });
    }
    stack.set_top(base)
}

#[cfg(feature = "test-support")]
pub fn expose_publish(runtime: &mut SongRuntime) -> Result<(), Error> {
    let globals = runtime.vm.rooted(runtime.screen.globals)?;
    let f = runtime.vm.new_host_function_with_data(
        |call| {
            let state = call.function_data::<Shared>().unwrap().clone();
            let globals = call.captures()[0];
            let args = call.arguments();
            let screen = args.first().copied().unwrap_or(Value::Nil);
            let players = [
                args.get(1).copied().unwrap_or(Value::Nil),
                args.get(2).copied().unwrap_or(Value::Nil),
            ];
            publish(&mut call.native_stack()?, globals, &state, screen, players)
        },
        &[globals],
        runtime.screen.state.clone(),
    )?;
    runtime.vm.set_global(b"PUBLISH", f)
}
