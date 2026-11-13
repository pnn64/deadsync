//! ScreenGameplay's owned base tree, before the remaining theme files load.
use super::super::managers::screen::Requests;
use super::*;
use generated_players::dummy;
use sprites::set;

#[derive(Clone)]
struct Build {
    context: crate::SongLuaCompileContext,
    statistics: Arc<Mutex<super::super::statistics::State>>,
}

impl Actors {
    pub(crate) fn gameplay_screen(
        &self,
        vm: &mut Vm,
        context: &crate::SongLuaCompileContext,
        statistics: Arc<Mutex<super::super::statistics::State>>,
        catalog: Value,
        requests: Arc<Mutex<Requests>>,
    ) -> Result<(ActorScene, [Value; crate::LUA_PLAYERS]), Error> {
        #[cfg(feature = "test-support")]
        {
            vm.store().trace_dynamic_commands = context.trace_dynamic_commands;
        }
        underlay::ensure_helpers(vm, self.helpers)?;
        screen_parts::ensure_helpers(vm, self.helpers)?;
        let function = vm.new_host_function_with_data(
            construct,
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                catalog,
            ],
            Build {
                context: context.clone(),
                statistics,
            },
        )?;
        let values = vm.call(function, &[])?.to_vec();
        let Value::Number(id) = values[0] else {
            unreachable!()
        };
        let scene = ActorScene {
            owner: self.instantiate,
            id: id as u64,
        };
        let children = [values[1], values[2], values[3], values[4], values[5]];
        if let Err(error) = self.bind_top_screen(
            vm,
            catalog,
            children,
            context.players.each_ref().map(|p| p.enabled),
            requests,
        ) {
            self.release(vm, scene)?;
            return Err(error);
        }
        Ok((scene, [values[2], values[3]]))
    }
}

fn construct(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Build>().unwrap().clone();
    let scene = new_scene(call)?;
    let previous = {
        let store = call.store();
        let capture = !store.clock.live.load(std::sync::atomic::Ordering::Relaxed);
        std::mem::replace(&mut store.constructing_capture, capture)
    };
    let result = assemble(call, &data, scene);
    call.store().constructing_capture = previous;
    let actors = match result {
        Ok(actors) => actors,
        Err(error) => {
            release(call, scene)?;
            return Err(error);
        }
    };
    call.push(Value::Number(scene as f64))?;
    for actor in actors {
        call.push(Value::Table(actor))?;
    }
    Ok(())
}

fn assemble(call: &mut HostCall<'_>, data: &Build, scene: u64) -> Result<[TableId; 5], Error> {
    let context = &data.context;
    let top = top_screen::create(call, scene, context)?;
    call.store().scenes.get_mut(&scene).unwrap().root = Some(top);
    let builds =
        [0, 1].map(|index| generated_players::Build::new(data.statistics.clone(), context, index));
    // The native screen constructs both meters, then both players, even when
    // only one player is enabled. Parts are attached after screen parenting.
    let meters = [
        builds[0].create_meter(call, scene)?,
        builds[1].create_meter(call, scene)?,
    ];
    let players = [
        builds[0].create_player(call, scene)?,
        builds[1].create_player(call, scene)?,
    ];
    for (index, &player) in players.iter().enumerate() {
        native_children::adopt(call, player, top)?;
        set(
            call,
            player,
            "__songlua_life_meter",
            Value::Table(meters[index]),
        )?;
        player_parts::install(call, scene, player, index, &context.style_name)?;
    }
    for (index, &meter) in meters.iter().enumerate() {
        native_children::adopt(call, meter, top)?;
        set(
            call,
            meter,
            "__songlua_theme_hibernating",
            Value::Boolean(true),
        )?;
        tag(call, meter, crate::top_screen_life_meter_name(index))?;
    }
    for (index, &player) in players.iter().enumerate() {
        if context.players[index].enabled {
            native_children::insert(
                call,
                top,
                crate::top_screen_player_name(index),
                Value::Table(player),
            )?;
        }
    }
    for (index, &meter) in meters.iter().enumerate() {
        if context.players[index].enabled {
            native_children::insert(
                call,
                top,
                crate::top_screen_life_meter_name(index),
                Value::Table(meter),
            )?;
        }
    }
    for &name in crate::TOP_SCREEN_THEME_CHILD_NAMES {
        let player = crate::top_screen_score_index(name)
            .or_else(|| crate::top_screen_steps_display_index(name))
            .or_else(|| {
                ["StepsDescription", "PlayerOptions", "OniGameOver"]
                    .iter()
                    .find_map(|prefix| {
                        name.strip_prefix(prefix).and_then(|side| match side {
                            "P1" => Some(0),
                            "P2" => Some(1),
                            _ => None,
                        })
                    })
            });
        if player.is_some_and(|index| !context.players[index].enabled) {
            continue;
        }
        let child = if name == "Underlay" {
            underlay::create(call, scene, top)?
        } else {
            screen_parts::create(call, scene, name)?
        };
        underlay::set_name(call, child, name)?;
        underlay::copy_tags(call, top, child)?;
        native_children::adopt(call, child, top)?;
        tag(call, child, name)?;
        if crate::top_screen_score_index(name).is_some()
            || crate::top_screen_steps_display_index(name).is_some()
            || name.starts_with("StepsDescription")
            || matches!(name, "Header" | "Footer")
        {
            set(
                call,
                child,
                "__songlua_theme_hibernating",
                Value::Boolean(true),
            )?;
        }
        if let Some(index) = crate::top_screen_steps_display_index(name) {
            let value = call.try_intern(context.players[index].difficulty.sm_name().as_bytes())?;
            set(call, child, "__songlua_preferred_difficulty", value)?;
        }
        native_children::insert(call, top, name, Value::Table(child))?;
    }
    let registry = native_children::helper_function(call, b"native_registry")?;
    let registry = call.call(registry, &[Value::Table(top)])?[0];
    let mut ordered = call.with_roots(&[registry], |call| {
        let registry = table(call, registry)?;
        let mut ordered = Vec::new();
        for name in [
            "SongBackground",
            "Underlay",
            "PlayerP1",
            "PlayerP2",
            "LifeP1",
            "LifeP2",
            "ScoreP1",
            "ScoreP2",
            "StepsDisplayP1",
            "StepsDisplayP2",
            "Overlay",
            "SongForeground",
            "In",
            "Debug",
        ]
        .into_iter()
        .chain(crate::TOP_SCREEN_THEME_CHILD_NAMES.iter().copied())
        {
            let value = get(call, registry, name.as_bytes())?;
            if value != Value::Nil {
                let child = table(call, value)?;
                if !ordered.contains(&child) {
                    ordered.push(child);
                }
            }
        }
        Ok(ordered)
    })?;
    write_order(call, top, &ordered)?;
    // These native messages reach the already-constructed theme. They bypass
    // authored broadcast recording and never enter a song's startup output.
    song_capture::without_live_broadcasts(call, |call| {
        for (index, &meter) in meters.iter().enumerate() {
            if !context.players[index].enabled {
                continue;
            }
            let params = call.try_new_table()?;
            call.with_roots(&[Value::Table(params)], |call| {
                let player = call.try_intern(crate::player_number_name(index).as_bytes())?;
                set(call, params, "Player", player)?;
                set(call, params, "LifeMeter", Value::Table(meter))?;
                messages::dispatch(call, b"LifeChanged", Some(Value::Table(params)), false)
            })?;
        }
        Ok(())
    })?;
    let mut unnamed = Vec::new();
    for role in ["Toasty", "NextSong", "Ready", "Go", "Failed"] {
        let actor = dummy(call, scene, "ActorFrame", "", Some(top))?;
        set(call, actor, "__songlua_transition_state", Value::Number(0.))?;
        let value = call.try_intern(role.as_bytes())?;
        set(call, actor, "__songlua_screen_actor_role", value)?;
        if role == "Toasty" {
            screen_parts::invoke(call, actor, "visible", &[Value::Boolean(false)])?;
        }
        unnamed.push(actor);
        ordered.push(actor);
    }
    let unnamed = underlay::group(call, &unnamed)?;
    native_children::insert(call, top, "", unnamed)?;
    set(
        call,
        top,
        "__songlua_owned_child_lookup",
        Value::Boolean(true),
    )?;
    // Stable sorting first restores native construction order and then draw
    // order. In particular all players precede their OniGameOver displays.
    let construction = [
        "Underlay",
        "Overlay",
        "Header",
        "Footer",
        "In",
        "Out",
        "Cancel",
        "SongBackground",
        "SongForeground",
        "Toasty",
        "PlayerP1",
        "PlayerP2",
        "OniGameOverP1",
        "OniGameOverP2",
        "NextSong",
        "LifeP1",
        "LifeP2",
        "ScoreP1",
        "ScoreP2",
        "StepsDescriptionP1",
        "PlayerOptionsP1",
        "StepsDisplayP1",
        "StepsDescriptionP2",
        "PlayerOptionsP2",
        "StepsDisplayP2",
        "SongOptions",
        "ActiveAttackListP1",
        "ActiveAttackListP2",
        "LyricDisplay",
        "Ready",
        "Go",
        "Failed",
        "Debug",
    ];
    let mut keys = Vec::new();
    for &actor in &ordered {
        let name = actor_name(call, actor)?;
        let name =
            String::from_utf8(name).map_err(|error| call.error(error.to_string().as_bytes()))?;
        let role = get(call, actor, b"__songlua_screen_actor_role")?;
        let role = if role == Value::Nil {
            None
        } else {
            Some(assets::text(call, role)?)
        };
        let rank = construction
            .iter()
            .position(|known| *known == role.as_deref().unwrap_or(&name))
            .unwrap_or(construction.len());
        let order = match name.as_str() {
            "SongBackground" => -200,
            "Underlay" => -100,
            "Overlay" => 100,
            "SongForeground" => 101,
            "Debug" => 199,
            "In" | "Out" | "Cancel" => 200,
            _ if matches!(role.as_deref(), Some("NextSong" | "Failed")) => 199,
            _ => 0,
        };
        let key = call.try_intern(b"__songlua_state_draw_order")?;
        call.raw_table_set(actor, key, Value::Number(order as f64))?;
        call.store()
            .actors
            .get_mut(&actor)
            .unwrap()
            .state
            .draw_order = order;
        keys.push((actor, rank, order));
    }
    keys.sort_by_key(|&(_, rank, _)| rank);
    keys.sort_by_key(|&(_, _, order)| order);
    ordered = keys.into_iter().map(|(actor, _, _)| actor).collect();
    write_order(call, top, &ordered)?;
    lifecycle::invalidate_plans(call)?;
    Ok([top, players[0], players[1], meters[0], meters[1]])
}

fn tag(call: &mut HostCall<'_>, actor: TableId, name: &str) -> Result<(), Error> {
    let value = call.try_intern(name.as_bytes())?;
    set(call, actor, "__songlua_top_screen_child_name", value)
}

fn write_order(call: &mut HostCall<'_>, actor: TableId, ordered: &[TableId]) -> Result<(), Error> {
    for (index, &child) in ordered.iter().enumerate() {
        call.raw_table_set(
            actor,
            Value::Number((index + 1) as f64),
            Value::Table(child),
        )?;
    }
    call.store().actors.get_mut(&actor).unwrap().children = ordered.to_vec();
    Ok(())
}
