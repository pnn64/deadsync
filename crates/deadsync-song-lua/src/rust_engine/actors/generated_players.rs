//! ScreenGameplay's engine player constructor and life-meter bindings.
//! The player Init phase attaches its visible parts in this same scene.
use super::*;
use sprites::{field, set};

type Statistics = Arc<Mutex<super::super::statistics::State>>;

#[derive(Clone)]
pub(super) struct Build {
    store: Shared,
    statistics: Statistics,
    player: crate::SongLuaPlayerContext,
    index: usize,
    style: String,
    parts: bool,
}
#[derive(Clone)]
struct Method {
    store: Shared,
    statistics: Statistics,
    index: usize,
    name: &'static str,
}

impl Actors {
    // This is the Player constructor phase, before ScreenGameplay::Init attaches
    // its visible parts. Keep it private until the complete screen factory uses it.
    pub(in crate::rust_engine) fn player_base(
        &self,
        vm: &mut Vm,
        context: &crate::SongLuaCompileContext,
        index: usize,
        statistics: Statistics,
        catalog: Value,
    ) -> Result<ActorScene, Error> {
        self.player(vm, context, (index, false), statistics, catalog)
    }

    pub(in crate::rust_engine) fn gameplay_player(
        &self,
        vm: &mut Vm,
        context: &crate::SongLuaCompileContext,
        index: usize,
        statistics: Statistics,
        catalog: Value,
    ) -> Result<ActorScene, Error> {
        self.player(vm, context, (index, true), statistics, catalog)
    }

    fn player(
        &self,
        vm: &mut Vm,
        context: &crate::SongLuaCompileContext,
        (index, parts): (usize, bool),
        statistics: Statistics,
        catalog: Value,
    ) -> Result<ActorScene, Error> {
        let Some(player) = context.players.get(index) else {
            return vm.with_native_stack(|stack| {
                Err(stack.error(b"gameplay player index out of range"))
            });
        };
        let function = vm.new_host_function_with_data(
            construct,
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                catalog,
            ],
            Build {
                store: self.store.clone(),
                statistics,
                player: player.clone(),
                index,
                style: context.style_name.clone(),
                parts,
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

impl Build {
    pub(super) fn new(
        store: Shared,
        statistics: Statistics,
        context: &crate::SongLuaCompileContext,
        index: usize,
    ) -> Self {
        Self {
            store,
            statistics,
            player: context.players[index].clone(),
            index,
            style: context.style_name.clone(),
            parts: false,
        }
    }
    fn method(&self) -> Method {
        Method {
            store: self.store.clone(),
            statistics: self.statistics.clone(),
            index: self.index,
            name: "",
        }
    }
    pub(super) fn create_meter(&self, call: &mut HostCall<'_>, id: u64) -> Result<TableId, Error> {
        let store = self.store.clone();
        let method = self.method();
        let meter = dummy(
            call,
            &store,
            id,
            "LifeMeter",
            &format!("LifeP{}", method.index + 1),
            None,
        )?;
        set(
            call,
            meter,
            "__songlua_life",
            Value::Number(crate::SONG_LUA_INITIAL_LIFE as f64),
        )?;
        for name in ["GetLife", "IsFailing", "IsHot", "IsInDanger"] {
            bind(call, meter, &method, name)?;
        }
        // The native internal LifeMeter keeps its bound child queries even
        // though its public method class is Actor, not ActorFrame.
        top_screen::bind_queries(call, meter)?;
        Ok(meter)
    }
    pub(super) fn create_player(&self, call: &mut HostCall<'_>, id: u64) -> Result<TableId, Error> {
        let store = self.store.clone();
        let method = self.method();
        let player = &self.player;
        let style = &self.style;
        let actor = dummy(
            call,
            &store,
            id,
            "PlayerActor",
            &format!("PlayerP{}", method.index + 1),
            None,
        )?;
        set(
            call,
            actor,
            "__songlua_player_index",
            Value::Number(method.index as f64),
        )?;
        let style_value = call.try_intern(style.as_bytes())?;
        set(call, actor, "__songlua_style_name", style_value)?;
        set(call, actor, "__songlua_visible", Value::Boolean(true))?;
        for name in ["get_oitg_zoom_mode", "set_oitg_zoom_mode", "SetLife"] {
            bind(call, actor, &method, name)?;
        }
        let attack = dummy(call, &store, id, "ActorFrame", "", Some(actor))?;
        store
            .lock()
            .unwrap()
            .actors
            .get_mut(&attack)
            .unwrap()
            .current
            .x = -40.;
        call.raw_table_set(actor, Value::Number(1.), Value::Table(attack))?;
        store
            .lock()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .children
            .push(attack);
        native_children::insert(call, actor, "", Value::Table(attack))?;
        set(
            call,
            actor,
            "__songlua_attack_display",
            Value::Table(attack),
        )?;
        {
            let mut guard = store.lock().unwrap();
            let pose = &mut guard.actors.get_mut(&actor).unwrap().current;
            pose.x = player.screen_x;
            pose.y = player.screen_y;
        }
        if let Some(path) = &player.judgment_texture {
            let path = crate::file_path_string(path);
            let path = call.try_intern(path.as_bytes())?;
            set(call, actor, "__songlua_judgment_texture", path)?;
        }
        Ok(actor)
    }
}

fn construct(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Build>().unwrap().clone();
    let store = data.store.clone();
    let style = data.style.clone();
    let parts = data.parts;
    let method = Method {
        store: store.clone(),
        statistics: data.statistics.clone(),
        index: data.index,
        name: "",
    };
    let id = {
        let mut guard = store.lock().unwrap();
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
    let result = (|| {
        let meter = data.create_meter(call, id)?;
        let actor = data.create_player(call, id)?;
        // ScreenGameplay owns the connection, after constructing the player.
        set(call, actor, "__songlua_life_meter", Value::Table(meter))?;
        if parts {
            player_parts::install(call, &store, id, actor, method.index, &style)?;
        }
        store.lock().unwrap().scenes.get_mut(&id).unwrap().root = Some(actor);
        Ok(())
    })();
    if let Err(error) = result {
        release(call, &store, id)?;
        return Err(error);
    }
    call.push(Value::Number(id as f64))
}

pub(super) fn dummy(
    call: &mut HostCall<'_>,
    store: &Shared,
    scene: u64,
    kind: &str,
    name: &str,
    parent: Option<TableId>,
) -> Result<TableId, Error> {
    let attrs = call.try_new_table()?;
    call.with_roots(&[Value::Table(attrs)], |call| {
        let class = call.try_intern(kind.as_bytes())?;
        set(call, attrs, "Class", class)?;
        for key in ["__songlua_script_dir", "__songlua_song_dir"] {
            let value = field(call, table(call, helper(call, 5)?)?, key)?;
            if value != Value::Nil {
                assets::text(call, value)?;
                set(call, attrs, key, value)?;
            }
        }
        let node = call.call(helper(call, 2)?, &[Value::Table(attrs)])?[0];
        let actor = call.with_roots(&[node], |call| {
            Ok(build(call, store, scene, table(call, node)?, parent, false, true)?.unwrap())
        })?;
        // create_dummy_actor does not run the authored Sprite initializer.
        // Its render animation flag remains false until an explicit command.
        store
            .lock()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .state
            .render
            .sprite_animate = false;
        let value = call.try_intern(name.as_bytes())?;
        set(call, actor, "Name", value)?;
        store
            .lock()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .native_children = true;
        store
            .lock()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .state
            .name = name.as_bytes().to_vec();
        if let Some(parent) = parent {
            native_children::adopt(call, store, actor, parent)?;
        }
        Ok(actor)
    })
}

fn bind(
    call: &mut HostCall<'_>,
    actor: TableId,
    method: &Method,
    name: &'static str,
) -> Result<(), Error> {
    let mut data = method.clone();
    data.name = name;
    let captures = [
        call.captures()[0],
        call.captures()[1],
        call.captures()[2],
        // Life queries are bound to this meter. The player's setters and zoom
        // getter use their supplied receiver and must not retain an old player.
        if matches!(name, "GetLife" | "IsFailing" | "IsHot" | "IsInDanger") {
            Value::Table(actor)
        } else {
            Value::Nil
        },
    ];
    let function = call.new_host_function_with_data(invoke, &captures, data)?;
    call.with_roots(&[function], |call| {
        let bound = get(call, actor, b"__songlua_bound_methods")?;
        set(call, table(call, bound)?, name, function)
    })
}

fn raw_bool(call: &mut HostCall<'_>, actor: TableId, name: &[u8]) -> Result<bool, Error> {
    let key = call.try_intern(name)?;
    // The native typed bool conversion follows Lua truthiness; Option only
    // supplies the nil default. Strings and numbers are not conversion errors.
    Ok(!matches!(
        call.raw_table_get(actor, key)?,
        Value::Nil | Value::Boolean(false)
    ))
}
fn raw_set(
    call: &mut HostCall<'_>,
    actor: TableId,
    name: &[u8],
    value: Value,
) -> Result<(), Error> {
    let key = call.try_intern(name)?;
    call.raw_table_set(actor, key, value)
}

fn invoke(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Method>().unwrap().clone();
    let receiver = call.captures()[3];
    match data.name {
        "GetLife" | "IsFailing" | "IsHot" | "IsInDanger" => {
            let live = data
                .statistics
                .lock()
                .unwrap()
                .players
                .as_ref()
                .map(|p| p[data.index].life);
            let mut stack = call.native_stack()?;
            let value = if data.name == "GetLife"
                && let Some(life) = live
            {
                life
            } else {
                stack.push(receiver)?;
                stack.get_field(-1, b"__songlua_life")?;
                if stack.value(-1) == Some(Value::Nil) {
                    crate::SONG_LUA_INITIAL_LIFE
                } else {
                    stack.number(-1)? as f32
                }
            };
            stack.push(match data.name {
                "GetLife" => Value::Number(value as f64),
                "IsFailing" => Value::Boolean(value <= 0.),
                "IsHot" => Value::Boolean(value >= 1.),
                "IsInDanger" => Value::Boolean(value < crate::SONG_LUA_DANGER_LIFE),
                _ => unreachable!(),
            })?;
            stack.finish(1)
        }
        "get_oitg_zoom_mode" => {
            let actor = table(call, arg(call, 0))?;
            let value = raw_bool(call, actor, b"__songlua_oitg_zoom_mode")?;
            call.push(Value::Boolean(value))
        }
        "set_oitg_zoom_mode" => {
            let actor = table(call, arg(call, 0))?;
            let value = !matches!(arg(call, 1), Value::Nil | Value::Boolean(false));
            raw_set(
                call,
                actor,
                b"__songlua_oitg_zoom_mode",
                Value::Boolean(value),
            )?;
            call.push(Value::Table(actor))
        }
        "SetLife" => {
            let actor = table(call, arg(call, 0))?;
            let life = (call.number(1)? as f32).clamp(0., 1.);
            if raw_bool(call, actor, b"__songlua_inside_set_life")? {
                return Err(call.error(b"Do not call SetLife from inside LifeChangedMessageCommand because SetLife causes a LifeChangedMessageCommand."));
            }
            raw_set(
                call,
                actor,
                b"__songlua_inside_set_life",
                Value::Boolean(true),
            )?;
            let result = change_life(call, &data, actor, life);
            raw_set(
                call,
                actor,
                b"__songlua_inside_set_life",
                Value::Boolean(false),
            )?;
            result?;
            // Native typed i64 read, including string coercion and range checks.
            // This path adds no internal tonumber call visible to authored hooks.
            let globals = helper(call, 5)?;
            let mut stack = call.native_stack()?;
            stack.push(globals)?;
            let global_index = stack.top();
            stack.get_field(
                global_index as isize,
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
            stack.set_table(global_index as isize, false)?;
            stack.push(Value::Table(actor))?;
            stack.finish(1)
        }
        _ => unreachable!(),
    }
}

fn change_life(
    call: &mut HostCall<'_>,
    data: &Method,
    actor: TableId,
    life: f32,
) -> Result<(), Error> {
    {
        let mut state = data.statistics.lock().unwrap();
        if let Some(players) = state.players.as_mut() {
            players[data.index].life = life;
            players[data.index].life_record_latest = Some(life);
        }
    }
    let catalog = table(call, call.captures()[2])?;
    let key = call.try_intern(b"live_statistics")?;
    let selected = call.raw_table_get(catalog, key)?;
    let stats = if selected == Value::Nil {
        let key = call.try_intern(b"statistics_players")?;
        let players = table(call, call.raw_table_get(catalog, key)?)?;
        table(
            call,
            call.raw_table_get(players, Value::Number((data.index + 1) as f64))?,
        )?
    } else {
        table(
            call,
            call.raw_table_get(
                table(call, selected)?,
                Value::Number((data.index + 2) as f64),
            )?,
        )?
    };
    raw_set(call, stats, b"__songlua_life", Value::Number(life as f64))?;
    if data
        .store
        .lock()
        .unwrap()
        .clock
        .live
        .load(std::sync::atomic::Ordering::Relaxed)
    {
        data.statistics
            .lock()
            .unwrap()
            .writes
            .push(crate::live::SongLuaLivePlayerWrite::SetLife {
                player: data.index,
                life,
            });
    }
    let key = call.try_intern(b"__songlua_life_meter")?;
    let meter = table(call, call.raw_table_get(actor, key)?)?;
    raw_set(call, meter, b"__songlua_life", Value::Number(life as f64))?;
    let params = call.try_new_table()?;
    call.with_roots(&[Value::Table(params)], |call| {
        let player = call.try_intern(format!("PlayerNumber_P{}", data.index + 1).as_bytes())?;
        raw_set(call, params, b"Player", player)?;
        raw_set(call, params, b"LifeMeter", Value::Table(meter))?;
        messages::dispatch(
            call,
            &data.store,
            b"LifeChanged",
            Some(Value::Table(params)),
            true,
        )
    })
}
