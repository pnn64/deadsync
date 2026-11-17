//! Background focus/reset/fade and foreground entry at native update boundaries.
use super::*;
use crate::SongLuaCompileContext;

pub(crate) const ENTRY: usize = 2;
pub(crate) const BACKGROUNDS: usize = 3;
pub(crate) const CLOCK: usize = 4;
pub(crate) const EXPIRE: usize = 5;
#[derive(Default)]
struct Layer {
    changes: Vec<(usize, deadsync_chart::song::SongBackgroundLuaActivation)>,
    change_index: Option<usize>,
    current: Option<usize>,
    fading: Option<usize>,
}
struct Foreground {
    actor: TableId,
    start: f32,
    stop: f32,
    started: bool,
    finished: bool,
}
#[derive(Default)]
struct State {
    backgrounds: Vec<TableId>,
    layers: [Layer; 2],
    order: Vec<usize>,
    foregrounds: Vec<Foreground>,
    clock: [f32; 3],
}
#[derive(Clone)]
struct Run {
    scene: u64,
    context: Arc<SongLuaCompileContext>,
    state: Arc<Mutex<Option<State>>>,
}
/// The actors the foreground entry callback acts on. It returns at once for
/// every other actor, so the update traversal calls it for these alone.
pub(super) struct EntryActors {
    function: Value,
    actors: Vec<TableId>,
}
/// Whether the update traversal must call `entry` for `actor`: always for a
/// caller's own callback, and only for a scheduled foreground otherwise.
pub(super) fn enters(store: &Store, entry: Value, actor: TableId) -> bool {
    match &store.entry_actors {
        Some(known) if known.function == entry => known.actors.contains(&actor),
        _ => true,
    }
}
impl Actors {
    pub(crate) fn live_layers(
        &self,
        vm: &mut Vm,
        screen: ActorScene,
        context: &SongLuaCompileContext,
    ) -> Result<Root, Error> {
        let root = {
            let store = vm.store();
            if !context.live || !store.clock.live.load(std::sync::atomic::Ordering::Relaxed) {
                return vm.with_native_stack(|s| {
                    Err(s.error(b"live layer scheduling requires a live runtime and context"))
                });
            }
            let Some(session) = store
                .scenes
                .get(&screen.id)
                .and_then(|s| s.song_session.as_ref())
                .filter(|s| s.activated)
            else {
                return vm.with_native_stack(|s| {
                    Err(s.error(b"live layer scheduling requires activated song layers"))
                });
            };
            session.roots
        };
        let callback = vm.new_host_function_with_data(
            initialize,
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                Value::Table(root),
            ],
            Run {
                scene: screen.id,
                context: Arc::new(context.clone()),
                state: Arc::new(Mutex::new(None)),
            },
        )?;
        let owners = vm.call(callback, &[])?[0];
        vm.pin(owners)
    }
}
fn raw(call: &mut HostCall<'_>, actor: TableId, key: &str) -> Result<Value, Error> {
    let key = call.try_intern(key.as_bytes())?;
    call.raw_table_get(actor, key)
}
fn set(
    call: &mut HostCall<'_>,
    actor: TableId,
    key: &'static str,
    value: Value,
) -> Result<(), Error> {
    song_replay::raw_set(call, actor, key, value)
}
fn optional_number(
    call: &mut HostCall<'_>,
    actor: TableId,
    key: &str,
) -> Result<Option<f32>, Error> {
    let v = raw(call, actor, key)?;
    if v == Value::Nil {
        Ok(None)
    } else {
        sprites::number(call, v).map(|n| Some(n as f32))
    }
}
fn truthy(value: Value) -> bool {
    !matches!(value, Value::Nil | Value::Boolean(false))
}
fn invoke(
    call: &mut HostCall<'_>,
    actor: TableId,
    method: &'static str,
    args: &[Value],
) -> Result<Value, Error> {
    // The receiver and the arguments: kept alive, then passed.
    let values: smallvec::SmallVec<[Value; 4]> = std::iter::once(Value::Table(actor))
        .chain(args.iter().copied())
        .collect();
    call.with_roots(&values, |call| {
        let name = call.intern_static(method.as_bytes())?;
        let f = call.table_get(Value::Table(actor), name)?;
        if !matches!(f, Value::Function(_) | Value::Native(_)) {
            return Err(call.error(format!("{method} function expected").as_bytes()));
        }
        Ok(call
            .call(f, &values)?
            .first()
            .copied()
            .unwrap_or(Value::Nil))
    })
}
fn command(call: &mut HostCall<'_>, actor: TableId, name: &str) -> Result<(), Error> {
    let name = call.try_intern(name.as_bytes())?;
    invoke(call, actor, "playcommand", &[name]).map(|_| ())
}
fn visible(call: &mut HostCall<'_>, actor: TableId, value: bool) -> Result<(), Error> {
    invoke(call, actor, "visible", &[Value::Boolean(value)]).map(|_| ())
}
fn initialize(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Run>().unwrap().clone();
    let root = table(call, call.captures()[2])?;
    let batch = call.try_new_table()?;
    call.with_roots(&[Value::Table(batch)], |call| {
        call.raw_table_set(batch, Value::Number(1.), Value::Table(root))?;
        let mut state = State::default();
        for i in 1usize.. {
            let actor = call.raw_table_get(root, Value::Number(i as f64))?;
            if actor == Value::Nil {
                break;
            }
            let Value::Table(actor) = actor else {
                continue;
            };
            call.raw_table_set(batch, Value::Table(actor), Value::Boolean(true))?;
            if let Some(start) = optional_number(call, actor, "__songlua_background_start_beat")? {
                let owner = state.backgrounds.len();
                let (layer, changes) = data
                    .context
                    .background_activations
                    .get(owner)
                    .cloned()
                    .unwrap_or_else(|| {
                        (
                            0,
                            vec![deadsync_chart::song::SongBackgroundLuaActivation {
                                start_beat: start,
                                rate: 1.,
                                transition: String::new(),
                            }],
                        )
                    });
                let layer = state
                    .layers
                    .get_mut(usize::from(layer))
                    .ok_or_else(|| call.error(b"invalid background layer"))?;
                layer
                    .changes
                    .extend(changes.into_iter().map(|change| (owner, change)));
                state.backgrounds.push(actor);
            }
            if let Some(start) = optional_number(call, actor, "__songlua_foreground_start_beat")? {
                state.foregrounds.push(Foreground {
                    actor,
                    start,
                    stop: 0.,
                    started: false,
                    finished: false,
                });
            }
        }
        for layer in &mut state.layers {
            layer
                .changes
                .sort_by(|a, b| a.1.start_beat.total_cmp(&b.1.start_beat));
        }
        state.order = (0..state.backgrounds.len()).collect();
        let foregrounds = state.foregrounds.iter().map(|f| f.actor).collect();
        *data.state.lock().unwrap() = Some(state);
        for (slot, callback) in [
            (ENTRY, entry as fn(&mut HostCall<'_>) -> Result<(), Error>),
            (BACKGROUNDS, backgrounds),
            (CLOCK, clock),
            (EXPIRE, expire),
        ] {
            let f = call.new_host_function_with_data(
                callback,
                &[
                    call.captures()[0],
                    call.captures()[1],
                    Value::Table(batch),
                    Value::Table(root),
                ],
                data.clone(),
            )?;
            call.raw_table_set(batch, Value::Number(slot as f64), f)?;
        }
        let function = call.raw_table_get(batch, Value::Number(ENTRY as f64))?;
        call.store().entry_actors = Some(EntryActors {
            function,
            actors: foregrounds,
        });
        call.push(Value::Table(batch))
    })
}
fn with_state(
    call: &mut HostCall<'_>,
    f: impl FnOnce(&mut HostCall<'_>, &Run, &mut State) -> Result<(), Error>,
) -> Result<(), Error> {
    let data = call.function_data::<Run>().unwrap().clone();
    let mut state = data
        .state
        .lock()
        .unwrap()
        .take()
        .ok_or_else(|| call.error(b"live layer phase is already active"))?;
    let first = call.store().next;
    let result = f(call, &data, &mut state);
    *data.state.lock().unwrap() = Some(state);
    song_activation::adopt_created(call.store(), data.scene, first);
    result
}
fn read_clock(call: &mut HostCall<'_>) -> Result<[f32; 3], Error> {
    let mut c = [0.; 3];
    for (i, n) in c.iter_mut().enumerate() {
        *n = sprites::number(call, arg(call, i))? as f32;
    }
    Ok(c)
}
fn clock(call: &mut HostCall<'_>) -> Result<(), Error> {
    let c = read_clock(call)?;
    with_state(call, |_, _, s| {
        s.clock = c;
        Ok(())
    })
}
// Ordinary method lookups preserve author overrides and their side effects.
fn options(call: &mut HostCall<'_>) -> Result<(bool, Value), Error> {
    let globals = helper(call, 5)?;
    let key = call.intern_static(b"PREFSMAN")?;
    let prefs = call.table_get(globals, key)?;
    let prefs = table(call, prefs)?;
    let rate_tweens = call.with_roots(&[Value::Table(prefs)], |call| {
        let pref = call.intern_static(b"RateModsAffectFGChanges")?;
        Ok(truthy(invoke(call, prefs, "GetPreference", &[pref])?))
    })?;
    let key = call.intern_static(b"GAMESTATE")?;
    let game = call.table_get(globals, key)?;
    let game = table(call, game)?;
    let options = call.with_roots(&[Value::Table(game)], |call| {
        let level = call.intern_static(b"ModsLevel_Current")?;
        invoke(call, game, "GetSongOptionsObject", &[level])
    })?;
    table(call, options)?;
    Ok((rate_tweens, options))
}
fn music_rate(call: &mut HostCall<'_>, options: TableId) -> Result<f32, Error> {
    let n = invoke(call, options, "MusicRate", &[])?;
    sprites::number(call, n).map(|n| n as f32)
}
fn backgrounds(call: &mut HostCall<'_>) -> Result<(), Error> {
    let [beat, music, last] = read_clock(call)?;
    with_state(call, |call, data, state| {
        if state.backgrounds.is_empty() {
            return Ok(());
        }
        let (rate_tweens, options) = options(call)?;
        let rate = call.with_roots(&[options], |call| {
            if rate_tweens {
                Ok(1.)
            } else {
                music_rate(call, table(call, options)?)
            }
        })?;
        for &actor in &state.backgrounds {
            set(call, actor, "__songlua_live_inactive", Value::Boolean(true))?;
        }
        let mut order = Vec::new();
        for layer in &mut state.layers {
            let next = layer
                .changes
                .iter()
                .rposition(|(_, change)| change.start_beat <= beat);
            let mut delta = music - last;
            if let Some(next) = next.filter(|n| Some(*n) != layer.change_index) {
                let (owner, change) = &layer.changes[next];
                layer.fading = layer.current.filter(|current| current != owner);
                if let Some(fading) = layer.fading {
                    let actor = state.backgrounds[fading];
                    command(call, actor, "LoseFocus")?;
                    transition(call, actor, &change.transition)?;
                }
                layer.current = Some(*owner);
                layer.change_index = Some(next);
                let actor = state.backgrounds[*owner];
                reset_actor(call, actor)?;
                set(call, actor, "__songlua_defer_on", Value::Boolean(false))?;
                song_activation::startup(call, actor)?;
                command(call, actor, "GainFocus")?;
                let start = data.context.song_timing.as_ref().map_or_else(
                    || {
                        crate::song_elapsed_seconds_at(change.start_beat, &data.context)
                            * crate::song_music_rate(&data.context)
                    },
                    |t| t.get_time_for_beat(change.start_beat),
                );
                delta = music - start;
            }
            if let Some(fading) = layer.fading
                && lifecycle::time_left(call, state.backgrounds[fading])? == 0.
            {
                layer.fading = None;
            }
            let delta = (delta / rate).max(0.);
            for (i, owner) in [layer.current, layer.fading].into_iter().enumerate() {
                let Some(owner) = owner else {
                    continue;
                };
                let actor = state.backgrounds[owner];
                let rate = if i == 0 {
                    layer.changes[layer.change_index.expect("current activation")]
                        .1
                        .rate
                } else {
                    optional_number(call, actor, "__songlua_background_rate")?.unwrap_or(1.)
                };
                set(
                    call,
                    actor,
                    "__songlua_background_rate",
                    Value::Number(rate as f64),
                )?;
                set(
                    call,
                    actor,
                    "__songlua_live_delta",
                    Value::Number((delta * rate) as f64),
                )?;
                set(
                    call,
                    actor,
                    "__songlua_live_inactive",
                    Value::Boolean(false),
                )?;
                order.push(owner);
            }
        }
        for owner in 0..state.backgrounds.len() {
            if !order.contains(&owner) {
                order.push(owner);
            }
        }
        if order != state.order {
            let root = table(call, call.captures()[3])?;
            for (i, &owner) in order.iter().enumerate() {
                call.raw_table_set(
                    root,
                    Value::Number((i + 1) as f64),
                    Value::Table(state.backgrounds[owner]),
                )?;
            }
            state.order = order;
            lifecycle::invalidate_plans(call)?;
        }
        Ok(())
    })
}
fn entry(call: &mut HostCall<'_>) -> Result<(), Error> {
    let actor = table(call, arg(call, 0))?;
    with_state(call, |call, data, state| {
        let Some(fg) = state.foregrounds.iter_mut().find(|f| f.actor == actor) else {
            return Ok(());
        };
        let [beat, music, last] = state.clock;
        let (rate_tweens, options) = options(call)?;
        call.with_roots(&[options], |call| {
            let options = table(call, options)?;
            let music_rate = music_rate(call, options)?;
            let rate = if rate_tweens { 1. } else { music_rate };
            let inactive = fg.finished || beat < fg.start;
            set(
                call,
                actor,
                "__songlua_live_inactive",
                Value::Boolean(inactive),
            )?;
            if inactive {
                return Ok(());
            }
            let shown = truthy(invoke(call, actor, "GetVisible", &[])?);
            let delta = if !shown {
                visible(call, actor, true)?;
                set(call, actor, "__songlua_defer_on", Value::Boolean(false))?;
                if fg.started {
                    command(call, actor, "On")?;
                } else {
                    song_activation::startup(call, actor)?;
                }
                let start_rate = self::music_rate(call, options)?;
                let start = data.context.song_timing.as_ref().map_or_else(
                    || {
                        crate::song_elapsed_seconds_at(fg.start, &data.context)
                            * crate::song_music_rate(&data.context)
                    },
                    |t| t.get_song_time_for_beat(fg.start, start_rate),
                );
                let stop = start + lifecycle::time_left(call, actor)?;
                fg.stop = data.context.song_timing.as_ref().map_or_else(
                    || {
                        crate::song_beat_at_elapsed_seconds(
                            stop / crate::song_music_rate(&data.context),
                            &data.context,
                        )
                    },
                    |t| t.get_song_position(stop).beat,
                );
                fg.started = true;
                (music - start).max(0.) / rate
            } else {
                (music - last).max(0.) / rate
            };
            set(
                call,
                actor,
                "__songlua_live_delta",
                Value::Number(delta as f64),
            )
        })
    })
}
fn expire(call: &mut HostCall<'_>) -> Result<(), Error> {
    with_state(call, |call, _, state| {
        for f in &mut state.foregrounds {
            if f.started && !f.finished && state.clock[0] > f.stop {
                visible(call, f.actor, false)?;
                set(
                    call,
                    f.actor,
                    "__songlua_live_inactive",
                    Value::Boolean(true),
                )?;
                f.finished = true;
            }
        }
        Ok(())
    })
}
fn transition(call: &mut HostCall<'_>, actor: TableId, name: &str) -> Result<(), Error> {
    let duration = match name {
        "" => return Ok(()),
        "CrossFade" => 1.,
        "CrossFade_Faster" => 0.75,
        "CrossFade_Fastest" => 0.5,
        _ => {
            return Err(call.error(format!("unsupported background transition: {name}").as_bytes()));
        }
    };
    fn leaves(call: &mut HostCall<'_>, actor: TableId, duration: f64) -> Result<(), Error> {
        let mut any = false;
        native_children::with_children(call, actor, |call, child| {
            any = true;
            leaves(call, child, duration)
        })?;
        if !any {
            invoke(call, actor, "stoptweening", &[])?;
            invoke(call, actor, "linear", &[Value::Number(duration)])?;
            invoke(call, actor, "diffusealpha", &[Value::Number(0.)])?;
        }
        Ok(())
    }
    leaves(call, actor, duration)
}
pub(super) fn reset_actor(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    call.with_roots(&[Value::Table(actor)], |call| {
        let previous = call.store().actors.get(&actor).map(|a| a.state.render);
        for i in 1usize.. {
            let v = call.raw_table_get(actor, Value::Number(i as f64))?;
            if v == Value::Nil {
                break;
            }
            reset_actor(call, table(call, v)?)?;
        }
        let clear = call.new_host_function(clear_fields, &[Value::Table(actor)])?;
        call.call(clear, &[])?;
        let class = raw(call, actor, "__songlua_actor_type")?;
        if initial_state::value_text(call, class)?.is_some_and(|s| s.eq_ignore_ascii_case("Sprite"))
        {
            set(
                call,
                actor,
                "__songlua_state_sprite_state_index",
                Value::Number(0.),
            )?;
            set(
                call,
                actor,
                "__songlua_state_sprite_animate",
                Value::Boolean(true),
            )?;
        }
        let mut render = initial_state::read_reset(call, actor)?;
        if let Some(p) = previous {
            render.draw_by_z_position = p.draw_by_z_position;
            render.mask_source = p.mask_source;
            render.mask_dest = p.mask_dest;
        }
        {
            let guard = call.store();
            if let Some(a) = guard.actors.get_mut(&actor) {
                a.current = render.into();
                a.queue.clear();
                a.replay = Some(Default::default());
                a.capture = None;
                a.preparation_getters = None;
                a.hibernate = 0.;
                a.started = false;
                a.effects = Default::default();
                a.state.render = render;
                a.base_transform = Default::default();
                a.pose_exported = false;
                a.state.visible = render.visible;
                a.state.draw_order = render.draw_order;
                a.state.size = render.size.unwrap_or([1.; 2]);
                a.state.rotation = [render.rot_x_deg, render.rot_y_deg, render.rot_z_deg];
            }
        }
        if call.store().actors.contains_key(&actor) {
            song_replay::sync_getters(call, actor)?;
        }
        Ok(())
    })
}
fn clear_fields(call: &mut HostCall<'_>) -> Result<(), Error> {
    let actor = call.captures()[0];
    let mut stack = call.native_stack()?;
    let base = stack.top();
    stack.push(actor)?;
    let owner = stack.top_offset();
    stack.push(Value::Nil)?;
    let mut keys = Vec::new();
    while stack.next_table(owner)? {
        if let Some(Value::String(_)) = stack.value(-2)
            && let Some(bytes) = stack.to_bytes(-2)?
            && let Ok(name) = std::str::from_utf8(bytes)
            && reset_key(name)
        {
            keys.push(stack.value(-2).unwrap());
        }
        stack.set_top(stack.top() - 1)?;
    }
    for key in keys {
        stack.push(key)?;
        stack.push(Value::Nil)?;
        stack.set_table(owner, true)?;
    }
    stack.set_top(base)?;
    stack.finish(0)
}
fn reset_key(key: &str) -> bool {
    key.strip_prefix("__songlua_state_").is_some_and(|n| {
        matches!(
            n,
            "aux"
                | "x"
                | "y"
                | "z"
                | "z_bias"
                | "draw_order"
                | "halign"
                | "valign"
                | "visible"
                | "zoom"
                | "zoom_x"
                | "zoom_y"
                | "zoom_z"
                | "basezoom"
                | "basezoom_x"
                | "basezoom_y"
                | "basezoom_z"
                | "base_rotation"
                | "base_alpha"
                | "quaternion"
                | "rot_x_deg"
                | "rot_y_deg"
                | "rot_z_deg"
                | "skew_x"
                | "skew_y"
                | "cropleft"
                | "cropright"
                | "croptop"
                | "cropbottom"
                | "fadeleft"
                | "faderight"
                | "fadetop"
                | "fadebottom"
                | "stretch_rect"
                | "diffuse"
                | "vertex_colors"
                | "glow"
                | "blend"
                | "shadow_len"
                | "shadow_color"
                | "depth_test"
                | "depth_write"
                | "depth_test_on_fail"
                | "cull_mode"
                | "texture_filtering"
                | "texture_wrapping"
                | "texcoord_offset"
                | "vibrate"
                | "rainbow"
                | "rainbow_scroll"
                | "spin"
                | "motion_clock"
                | "hibernating"
                | "draw_by_z_position"
                | "mask_source"
                | "mask_dest"
        ) || n.starts_with("effect_")
    }) || matches!(
        key,
        "__songlua_visible"
            | "__songlua_current_x"
            | "__songlua_current_y"
            | "__songlua_current_z"
            | "__songlua_diffuse"
            | "__songlua_aux"
            | "__songlua_hibernate_seconds"
            | "__songlua_startup_commands_ran"
            | "__songlua_startup_command_started"
            | "__songlua_startup_children_walked"
    ) || key.starts_with("__songlua_command_queue")
}
