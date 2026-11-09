//! Owned actor instances and synchronous commands over the Rust host API.
use deadlib_lua::{Error, ErrorKind, HostCall, Root, TableId, Value, Vm, compile_named};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

/// A scene owned by one Rust song runtime. Release it explicitly when retired.
/// Copies identify the same scene; they do not duplicate its actors or callbacks.
#[derive(Clone, Copy, Debug)]
pub struct ActorScene {
    owner: TableId,
    id: u64,
}

/// Native fields currently implemented by the actor host. Lua instance fields
/// are separate: assigning `self.Name` or `self.x` does not change these values.
#[derive(Clone, Debug, PartialEq)]
pub struct ActorSnapshot {
    pub class: &'static str,
    pub name: Vec<u8>,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub position: [f32; 3],
    pub zoom: [f32; 3],
    pub rotation: [f32; 3],
    pub size: [f32; 2],
    pub visible: bool,
    pub draw_order: i32,
    pub aux: f32,
}
struct Actor {
    state: ActorSnapshot,
    parent: Option<TableId>,
    children: Vec<TableId>,
    commands: TableId,
    propagate: bool,
}
struct Scene {
    actors: Vec<TableId>,
    root: Option<TableId>,
}
#[derive(Default)]
struct Store {
    next: u64,
    actors: HashMap<TableId, Actor>,
    scenes: HashMap<u64, Scene>,
    diagnostics: Vec<String>,
}
type Shared = Arc<Mutex<Store>>;

#[derive(Clone, Copy)]
enum Op {
    Instantiate,
    Release,
    Name,
    GetName,
    GetParent,
    GetChildren,
    GetChild,
    GetNumChildren,
    Sort,
    AddCommand,
    GetCommand,
    PlayCommand,
    Recursive,
    OnChildren,
    OnLeaves,
    NamedChildren,
    NamedLeaves,
    Propagate,
    Set(&'static str),
    Get(&'static str),
}
#[derive(Clone)]
struct Binding {
    store: Shared,
    op: Op,
}

pub(super) struct Actors {
    registry: TableId,
    store: Shared,
    instantiate: Root,
    release: Root,
}
impl Actors {
    pub(super) fn install(vm: &mut Vm) -> Result<Self, Error> {
        let store = Arc::new(Mutex::new(Store::default()));
        let registry = vm.try_new_table()?;
        let registry_root = vm.pin(Value::Table(registry))?;
        // Every callback captures this private table; it owns all Lua actor and
        // command values until release. Rust metadata contains no untraced owner.
        let helpers = vm.try_new_table()?;
        let helpers_root = vm.pin(Value::Table(helpers))?;
        let captures = [Value::Table(registry), Value::Table(helpers)];
        let methods = vm.try_new_table()?;
        let methods_root = vm.pin(Value::Table(methods))?;
        let actor = vm.try_new_table()?;
        super::put(vm, methods, "Actor", Value::Table(actor))?;
        let frame = vm.try_new_table()?;
        super::put(vm, methods, "ActorFrame", Value::Table(frame))?;
        for (name, op) in [
            ("name", Op::Name),
            ("GetName", Op::GetName),
            ("GetParent", Op::GetParent),
            ("addcommand", Op::AddCommand),
            ("GetCommand", Op::GetCommand),
            ("playcommand", Op::PlayCommand),
            ("RunCommandsRecursively", Op::Recursive),
        ] {
            install_method(vm, actor, name, op, &store, &captures)?;
        }
        for name in [
            "x",
            "y",
            "z",
            "xy",
            "addx",
            "addy",
            "addz",
            "zoom",
            "zoomx",
            "zoomy",
            "zoomz",
            "rotationx",
            "rotationy",
            "rotationz",
            "addrotationx",
            "addrotationy",
            "addrotationz",
            "SetWidth",
            "SetHeight",
            "setsize",
            "aux",
            "visible",
            "draworder",
        ] {
            install_method(vm, actor, name, Op::Set(name), &store, &captures)?;
        }
        for name in [
            "GetX",
            "GetY",
            "GetZ",
            "GetDestX",
            "GetDestY",
            "GetDestZ",
            "GetZoom",
            "GetZoomX",
            "GetZoomY",
            "GetZoomZ",
            "GetRotationX",
            "GetRotationY",
            "GetRotationZ",
            "GetWidth",
            "GetHeight",
            "GetZoomedWidth",
            "GetZoomedHeight",
            "getaux",
            "GetVisible",
        ] {
            install_method(vm, actor, name, Op::Get(name), &store, &captures)?;
        }
        for (name, op) in [
            ("GetChild", Op::GetChild),
            ("GetChildren", Op::GetChildren),
            ("GetNumChildren", Op::GetNumChildren),
            ("SortByDrawOrder", Op::Sort),
            ("RunCommandsOnChildren", Op::OnChildren),
            ("runcommandsonleaves", Op::OnLeaves),
            ("playcommandonchildren", Op::NamedChildren),
            ("playcommandonleaves", Op::NamedLeaves),
            ("propagate", Op::Propagate),
        ] {
            install_method(vm, frame, name, op, &store, &captures)?;
        }
        let setup = compile_named(
            include_bytes!("actors.lua"),
            b"=DeadSync actor instance bindings",
        )?;
        let setup = vm.execute(&setup)?[0];
        let values = vm.call(setup, &[Value::Table(methods)])?.to_vec();
        for (i, value) in values.into_iter().enumerate() {
            vm.raw_table_set(helpers, Value::Number((i + 1) as f64), value)?;
        }
        let instantiate = vm.new_host_function_with_data(
            callback,
            &captures,
            Binding {
                store: store.clone(),
                op: Op::Instantiate,
            },
        )?;
        let instantiate = vm.pin(instantiate)?;
        let release = vm.new_host_function_with_data(
            callback,
            &captures,
            Binding {
                store: store.clone(),
                op: Op::Release,
            },
        )?;
        let release = vm.pin(release)?;
        vm.unpin(methods_root)?;
        vm.unpin(helpers_root)?;
        vm.unpin(registry_root)?;
        Ok(Self {
            registry,
            store,
            instantiate,
            release,
        })
    }
    pub(super) fn instantiate(&self, vm: &mut Vm, declaration: Value) -> Result<ActorScene, Error> {
        let function = vm.rooted(self.instantiate)?;
        let Value::Number(id) = vm.call(function, &[declaration])?[0] else {
            unreachable!()
        };
        Ok(ActorScene {
            owner: self.registry,
            id: id as u64,
        })
    }
    pub(super) fn root(&self, scene: ActorScene) -> Option<Value> {
        if scene.owner != self.registry {
            return None;
        }
        self.store
            .lock()
            .unwrap()
            .scenes
            .get(&scene.id)
            .map(|s| s.root.map_or(Value::Nil, Value::Table))
    }
    pub(super) fn release(&self, vm: &mut Vm, scene: ActorScene) -> Result<bool, Error> {
        if scene.owner != self.registry {
            return Ok(false);
        }
        let function = vm.rooted(self.release)?;
        Ok(vm.call(function, &[Value::Number(scene.id as f64)])?[0] == Value::Boolean(true))
    }
    pub(super) fn snapshot(&self, scene: ActorScene) -> Option<Vec<ActorSnapshot>> {
        if scene.owner != self.registry {
            return None;
        }
        let store = self.store.lock().unwrap();
        let scene = store.scenes.get(&scene.id)?;
        let ids: HashMap<_, _> = scene
            .actors
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect();
        Some(
            scene
                .actors
                .iter()
                .map(|id| {
                    let actor = &store.actors[id];
                    let mut state = actor.state.clone();
                    state.parent = actor.parent.map(|id| ids[&id]);
                    state.children = actor.children.iter().map(|id| ids[id]).collect();
                    state
                })
                .collect(),
        )
    }
    pub(super) fn take_diagnostics(&self) -> Vec<String> {
        std::mem::take(&mut self.store.lock().unwrap().diagnostics)
    }
}
fn install_method(
    vm: &mut Vm,
    table: TableId,
    name: &str,
    op: Op,
    store: &Shared,
    captures: &[Value],
) -> Result<(), Error> {
    let function = vm.new_host_function_with_data(
        callback,
        captures,
        Binding {
            store: store.clone(),
            op,
        },
    )?;
    super::put(vm, table, name, function)
}
fn table(call: &HostCall<'_>, value: Value) -> Result<TableId, Error> {
    if let Value::Table(id) = value {
        Ok(id)
    } else {
        Err(call.error(b"actor table expected"))
    }
}
fn get(call: &mut HostCall<'_>, t: TableId, key: &[u8]) -> Result<Value, Error> {
    let key = call.try_intern(key)?;
    call.raw_table_get(t, key)
}
fn bytes(call: &mut HostCall<'_>, value: Value) -> Result<Vec<u8>, Error> {
    let id = call
        .coerce_string(value)?
        .ok_or_else(|| call.error(b"string expected"))?;
    Ok(call.string_bytes(id).unwrap().to_vec())
}
fn arg(call: &HostCall<'_>, i: usize) -> Value {
    call.arguments().get(i).copied().unwrap_or(Value::Nil)
}
fn helper(call: &HostCall<'_>, index: usize) -> Result<Value, Error> {
    let Value::Table(helpers) = call.captures()[1] else {
        unreachable!()
    };
    call.raw_table_get(helpers, Value::Number(index as f64))
}
fn children(store: &Shared, actor: TableId) -> Vec<TableId> {
    store.lock().unwrap().actors[&actor].children.clone()
}
fn is_frame(store: &Shared, actor: TableId) -> bool {
    store.lock().unwrap().actors[&actor].state.class == "ActorFrame"
}
fn sort(store: &Shared, actor: TableId) {
    let mut store = store.lock().unwrap();
    let mut list = store.actors[&actor].children.clone();
    list.sort_by_key(|id| store.actors[id].state.draw_order);
    store.actors.get_mut(&actor).unwrap().children = list;
}
fn command(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    name: &[u8],
) -> Result<Value, Error> {
    let commands = store.lock().unwrap().actors[&actor].commands;
    get(call, commands, name)
}
fn run(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    function: Value,
    params: Value,
) -> Result<(), Error> {
    if is_frame(store, actor) && store.lock().unwrap().actors[&actor].propagate {
        for child in children(store, actor) {
            run(call, store, child, function, params)?;
        }
        return Ok(());
    }
    if let Err(error) = call.call(function, &[Value::Table(actor), params]) {
        if error.kind() != ErrorKind::Runtime {
            return Err(error);
        }
        store.lock().unwrap().diagnostics.push(error.to_string());
    }
    Ok(())
}
fn play(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    name: &[u8],
    params: Value,
    recurse: bool,
) -> Result<(), Error> {
    let function = command(call, store, actor, name)?;
    if function != Value::Nil {
        call.with_roots(&[function, params], |call| {
            run(call, store, actor, function, params)
        })?;
    }
    if recurse {
        for child in children(store, actor) {
            play(call, store, child, name, params, true)?;
        }
    }
    Ok(())
}
fn walk(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    function: Value,
    params: Value,
    leaves: bool,
) -> Result<(), Error> {
    let children = children(store, actor);
    for &child in &children {
        walk(call, store, child, function, params, leaves)?;
    }
    if !leaves || !is_frame(store, actor) {
        run(call, store, actor, function, params)?;
    }
    Ok(())
}
fn release(call: &mut HostCall<'_>, store: &Shared, id: u64) -> Result<bool, Error> {
    let scene = store.lock().unwrap().scenes.remove(&id);
    let Some(scene) = scene else { return Ok(false) };
    let registry = table(call, call.captures()[0])?;
    for actor in scene.actors {
        call.raw_table_set(registry, Value::Table(actor), Value::Nil)?;
        store.lock().unwrap().actors.remove(&actor);
    }
    Ok(true)
}
fn build(
    call: &mut HostCall<'_>,
    store: &Shared,
    scene: u64,
    node: TableId,
    parent: Option<TableId>,
) -> Result<Option<TableId>, Error> {
    let attrs = table(call, call.raw_table_get(node, Value::Number(1.0))?)?;
    if get(call, attrs, b"Condition")? == Value::Boolean(false) {
        return Ok(None);
    }
    let mut kind = get(call, attrs, b"Class")?;
    if kind == Value::Nil {
        kind = get(call, attrs, b"Type")?;
    }
    let class = if kind == Value::Nil {
        Vec::new()
    } else {
        bytes(call, kind)?
    };
    let dummy = class.is_empty();
    let class = match class.as_slice() {
        b"" | b"Actor" => "Actor",
        b"ActorFrame" => "ActorFrame",
        b"Quad" => "Quad",
        _ => {
            return Err(call.error(
                format!(
                    "actor class '{}' has no Rust instance implementation yet",
                    String::from_utf8_lossy(&class)
                )
                .as_bytes(),
            ));
        }
    };
    let create = helper(call, 1)?;
    let kind = call.try_intern(class.as_bytes())?;
    let actor = call.call(create, &[kind, parent.map_or(Value::Nil, Value::Table)])?[0];
    let actor = table(call, actor)?;
    let commands = call.try_new_table()?;
    let registry = table(call, call.captures()[0])?;
    call.raw_table_set(registry, Value::Table(actor), Value::Table(commands))?;
    {
        let mut store = store.lock().unwrap();
        store.scenes.get_mut(&scene).unwrap().actors.push(actor);
        store.actors.insert(
            actor,
            Actor {
                state: ActorSnapshot {
                    class,
                    name: Vec::new(),
                    parent: None,
                    children: Vec::new(),
                    position: [0.; 3],
                    zoom: [1.; 3],
                    rotation: [0.; 3],
                    size: [1.; 2],
                    visible: dummy || class != "Actor",
                    draw_order: 0,
                    aux: 0.,
                },
                parent,
                children: Vec::new(),
                commands,
                propagate: false,
            },
        );
    }
    if dummy {
        return Ok(Some(actor));
    }
    if class == "ActorFrame" {
        let nodes = table(call, call.raw_table_get(node, Value::Number(2.))?)?;
        let mut selected = nodes;
        let mut array_only = true;
        for i in 1..=call.raw_table_len(nodes)? {
            let entry = table(call, call.raw_table_get(nodes, Value::Number(i as f64))?)?;
            let key = call.raw_table_get(entry, Value::Number(1.))?;
            if bytes(call, key)? == b"children" {
                let node = table(call, call.raw_table_get(entry, Value::Number(2.))?)?;
                selected = table(call, call.raw_table_get(node, Value::Number(2.))?)?;
                array_only = false;
                break;
            }
        }
        for i in 1..=call.raw_table_len(selected)? {
            let entry = table(call, call.raw_table_get(selected, Value::Number(i as f64))?)?;
            let name = call.raw_table_get(entry, Value::Number(1.))?;
            let name = bytes(call, name)?;
            if array_only && (name.is_empty() || !name.iter().all(u8::is_ascii_digit)) {
                continue;
            }
            let child = table(call, call.raw_table_get(entry, Value::Number(2.))?)?;
            if let Some(child) = build(call, store, scene, child, Some(actor))? {
                store
                    .lock()
                    .unwrap()
                    .actors
                    .get_mut(&actor)
                    .unwrap()
                    .children
                    .push(child);
            }
        }
        sort(store, actor);
    }
    let name = get(call, attrs, b"Name")?;
    if name != Value::Nil {
        store
            .lock()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .state
            .name = bytes(call, name)?;
    }
    let keys = call.call(helper(call, 5)?, &[Value::Table(attrs)])?[0];
    call.with_roots(&[keys], |call| {
        let keys = table(call, keys)?;
        for i in 1..=call.raw_table_len(keys)? {
            let key = call.raw_table_get(keys, Value::Number(i as f64))?;
            let name = bytes(call, key)?;
            if let Some(name) = name.strip_suffix(b"Command") {
                let name = name.strip_suffix(b"Message").unwrap_or(name);
                let value = call.raw_table_get(attrs, key)?;
                let name = call.try_intern(name)?;
                call.raw_table_set(commands, name, value)?;
            }
        }
        Ok(())
    })?;
    let params = Value::Table(call.try_new_table()?);
    call.with_roots(&[params], |call| {
        play(call, store, actor, b"Init", params, false)
    })?;
    Ok(Some(actor))
}
fn group(call: &mut HostCall<'_>, store: &Shared, ids: &[TableId]) -> Result<Value, Error> {
    match ids {
        [] => Ok(Value::Nil),
        [actor] => Ok(Value::Table(*actor)),
        _ => {
            let list = call.try_new_table()?;
            for (i, id) in ids.iter().enumerate() {
                call.raw_table_set(list, Value::Number((i + 1) as f64), Value::Table(*id))?;
            }
            let classes = table(call, helper(call, 4)?)?;
            let class = store.lock().unwrap().actors[ids.last().unwrap()]
                .state
                .class;
            let class = get(call, classes, class.as_bytes())?;
            Ok(call.call(helper(call, 3)?, &[Value::Table(list), class])?[0])
        }
    }
}
fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
    let Binding { store, op } = call.function_data::<Binding>().unwrap().clone();
    if matches!(op, Op::Instantiate) {
        let declaration = table(call, arg(call, 0))?;
        let node = call.call(helper(call, 2)?, &[Value::Table(declaration)])?[0];
        return call.with_roots(&[node], |call| {
            let id = {
                let mut store = store.lock().unwrap();
                if store.next >= (1u64 << 53) - 1 {
                    return Err(call.error(b"actor scene identity exhausted"));
                }
                store.next += 1;
                let id = store.next;
                store.scenes.insert(
                    id,
                    Scene {
                        actors: Vec::new(),
                        root: None,
                    },
                );
                id
            };
            match build(call, &store, id, table(call, node)?, None) {
                Ok(root) => {
                    store.lock().unwrap().scenes.get_mut(&id).unwrap().root = root;
                    call.push(Value::Number(id as f64))
                }
                Err(error) => {
                    release(call, &store, id)?;
                    Err(error)
                }
            }
        });
    }
    if matches!(op, Op::Release) {
        let id = call.number(0)? as u64;
        let removed = release(call, &store, id)?;
        return call.push(Value::Boolean(removed));
    }
    let actor = table(call, arg(call, 0))?;
    if !store.lock().unwrap().actors.contains_key(&actor) {
        return Err(call.error(b"actor instance is not owned by this runtime or has been released"));
    }
    let frame_method = matches!(
        op,
        Op::GetChildren
            | Op::GetChild
            | Op::GetNumChildren
            | Op::Sort
            | Op::OnChildren
            | Op::OnLeaves
            | Op::NamedChildren
            | Op::NamedLeaves
            | Op::Propagate
    );
    if frame_method && !is_frame(&store, actor) {
        return Err(call.error(b"ActorFrame expected"));
    }
    match op {
        Op::GetName => {
            let name = store.lock().unwrap().actors[&actor].state.name.clone();
            let name = call.try_intern(&name)?;
            return call.push(name);
        }
        Op::Name => {
            let name = bytes(call, arg(call, 1))?;
            store
                .lock()
                .unwrap()
                .actors
                .get_mut(&actor)
                .unwrap()
                .state
                .name = name;
        }
        Op::GetParent => {
            let parent = store.lock().unwrap().actors[&actor].parent;
            return call.push(parent.map_or(Value::Nil, Value::Table));
        }
        Op::GetNumChildren => {
            return call.push(Value::Number(children(&store, actor).len() as f64));
        }
        Op::GetChild => {
            let name = bytes(call, arg(call, 1))?;
            let ids = children(&store, actor)
                .into_iter()
                .filter(|id| store.lock().unwrap().actors[id].state.name == name)
                .collect::<Vec<_>>();
            let value = group(call, &store, &ids)?;
            return call.push(value);
        }
        Op::GetChildren => {
            let result = Value::Table(call.try_new_table()?);
            return call.with_roots(&[result], |call| {
                let mut names: Vec<(Vec<u8>, Vec<TableId>)> = Vec::new();
                for id in children(&store, actor) {
                    let name = store.lock().unwrap().actors[&id].state.name.clone();
                    if let Some((_, ids)) = names.iter_mut().find(|(n, _)| *n == name) {
                        ids.push(id)
                    } else {
                        names.push((name, vec![id]));
                    }
                }
                for (name, ids) in names {
                    let value = group(call, &store, &ids)?;
                    let key = call.try_intern(&name)?;
                    call.raw_table_set(table(call, result)?, key, value)?;
                }
                call.push(result)
            });
        }
        Op::Sort => sort(&store, actor),
        Op::Propagate => {
            store
                .lock()
                .unwrap()
                .actors
                .get_mut(&actor)
                .unwrap()
                .propagate = truthy_integer(call, 1)?;
        }
        Op::AddCommand | Op::GetCommand => {
            let name = bytes(call, arg(call, 1))?;
            if matches!(op, Op::GetCommand) {
                let value = command(call, &store, actor, &name)?;
                return call.push(value);
            }
            let name = name.strip_suffix(b"Message").unwrap_or(&name);
            let key = call.try_intern(name)?;
            let commands = store.lock().unwrap().actors[&actor].commands;
            // AddCommand takes the last stack value, including extra arguments.
            let value = call.arguments().last().copied().unwrap_or(Value::Nil);
            call.raw_table_set(commands, key, value)?;
        }
        Op::PlayCommand => {
            let name = bytes(call, arg(call, 1))?;
            let params = arg(call, 2);
            check_params(call, params)?;
            play(call, &store, actor, &name, params, true)?;
        }
        Op::Recursive | Op::OnChildren | Op::OnLeaves | Op::NamedChildren | Op::NamedLeaves => {
            let named = matches!(op, Op::NamedChildren | Op::NamedLeaves);
            let function = if named {
                let name = bytes(call, arg(call, 1))?;
                command(call, &store, actor, &name)?
            } else {
                arg(call, 1)
            };
            let params = if matches!(op, Op::OnLeaves | Op::NamedChildren | Op::NamedLeaves) {
                Value::Nil
            } else {
                arg(call, 2)
            };
            check_params(call, params)?;
            if !named && !matches!(function, Value::Function(_) | Value::Native(_)) {
                return Err(call.error(b"function expected"));
            }
            if function != Value::Nil {
                call.with_roots(&[function, params], |call| match op {
                    Op::Recursive => walk(call, &store, actor, function, params, false),
                    Op::OnLeaves | Op::NamedLeaves => {
                        walk(call, &store, actor, function, params, true)
                    }
                    _ => {
                        for child in children(&store, actor) {
                            run(call, &store, child, function, params)?;
                        }
                        Ok(())
                    }
                })?;
            }
        }
        Op::Set(name) => set_scalar(call, &store, actor, name)?,
        Op::Get(name) => return get_scalar(call, &store, actor, name),
        Op::Instantiate | Op::Release => unreachable!(),
    }
    call.push(Value::Table(actor))
}
fn check_params(call: &HostCall<'_>, value: Value) -> Result<(), Error> {
    if matches!(value, Value::Nil | Value::Table(_)) {
        Ok(())
    } else {
        Err(call.error(b"table or nil expected"))
    }
}
fn truthy_integer(call: &HostCall<'_>, i: usize) -> Result<bool, Error> {
    match arg(call, i) {
        Value::Boolean(b) => Ok(b),
        Value::Number(n) => Ok(n as i64 != 0),
        _ => Err(call.error(b"boolean expected")),
    }
}
fn set_scalar(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    name: &str,
) -> Result<(), Error> {
    let first = if name == "visible" {
        truthy_integer(call, 1)? as u8 as f32
    } else {
        call.number(1)? as f32
    };
    let second = if matches!(name, "xy" | "setsize") {
        call.number(2)? as f32
    } else {
        0.
    };
    let mut store = store.lock().unwrap();
    let s = &mut store.actors.get_mut(&actor).unwrap().state;
    match name {
        "x" => s.position[0] = first,
        "y" => s.position[1] = first,
        "z" => s.position[2] = first,
        "xy" => {
            s.position[0] = first;
            s.position[1] = second;
        }
        "addx" => s.position[0] += first,
        "addy" => s.position[1] += first,
        "addz" => s.position[2] += first,
        "zoom" => s.zoom = [first; 3],
        "zoomx" => s.zoom[0] = first,
        "zoomy" => s.zoom[1] = first,
        "zoomz" => s.zoom[2] = first,
        "rotationx" => s.rotation[0] = first,
        "rotationy" => s.rotation[1] = first,
        "rotationz" => s.rotation[2] = first,
        "addrotationx" => s.rotation[0] += first,
        "addrotationy" => s.rotation[1] += first,
        "addrotationz" => s.rotation[2] += first,
        "SetWidth" => s.size[0] = first,
        "SetHeight" => s.size[1] = first,
        "setsize" => s.size = [first, second],
        "aux" => s.aux = first,
        "visible" => s.visible = first != 0.,
        "draworder" => s.draw_order = call.number(1)? as i32,
        _ => unreachable!(),
    }
    Ok(())
}
fn get_scalar(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    name: &str,
) -> Result<(), Error> {
    let store = store.lock().unwrap();
    let s = &store.actors[&actor].state;
    let value = match name {
        "GetX" | "GetDestX" => s.position[0],
        "GetY" | "GetDestY" => s.position[1],
        "GetZ" | "GetDestZ" => s.position[2],
        "GetZoom" | "GetZoomX" => s.zoom[0],
        "GetZoomY" => s.zoom[1],
        "GetZoomZ" => s.zoom[2],
        "GetRotationX" => s.rotation[0],
        "GetRotationY" => s.rotation[1],
        "GetRotationZ" => s.rotation[2],
        "GetWidth" => s.size[0],
        "GetHeight" => s.size[1],
        "GetZoomedWidth" => s.size[0] * s.zoom[0],
        "GetZoomedHeight" => s.size[1] * s.zoom[1],
        "getaux" => s.aux,
        "GetVisible" => return call.push(Value::Boolean(s.visible)),
        _ => unreachable!(),
    };
    call.push(Value::Number(f64::from(value)))
}
