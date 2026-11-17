//! Original standalone actor runtime fixtures on deadlib-lua.
use deadlib_lua::{Root, Value};
use deadsync_song_lua::{
    SongLuaCompileContext, {SongRuntime, test_support},
};
#[path = "support/baked_program.rs"]
mod api;
fn runtime() -> SongRuntime {
    SongRuntime::new(&SongLuaCompileContext::new(".", "actor runtime unit")).unwrap()
}
fn table_index(host: &mut SongRuntime, owner: Value, index: usize) -> Value {
    let Value::Table(owner) = owner else {
        panic!("table expected")
    };
    let value = host
        .vm_mut()
        .raw_table_get(owner, Value::Number(index as f64))
        .unwrap();
    if !matches!(value, Value::Nil | Value::Boolean(_) | Value::Number(_)) {
        host.vm_mut().pin(value).unwrap();
    }
    value
}
fn set_index(host: &mut SongRuntime, owner: Value, index: usize, value: Value) {
    let Value::Table(owner) = owner else {
        panic!("table expected")
    };
    host.vm_mut()
        .raw_table_set(owner, Value::Number(index as f64), value)
        .unwrap();
}
fn length(host: &mut SongRuntime, table: Value) -> usize {
    let Value::Table(table) = table else {
        panic!("table expected")
    };
    host.vm_mut().raw_table_len(table).unwrap()
}
fn pointer(host: &mut SongRuntime, value: Value) -> usize {
    host.vm_mut().object_pointer(value).unwrap()
}
fn instantiate(host: &mut SongRuntime, source: &str) -> Value {
    let declaration = api::eval(host.vm_mut(), source)[0];
    let scene = host.instantiate_actor(declaration).unwrap();
    host.actor_scene_root(scene).unwrap()
}
fn call_root(host: &mut SongRuntime, function: Root, args: &[Value]) {
    let vm = host.vm_mut();
    let function = vm.rooted(function).unwrap();
    vm.call(function, args).unwrap();
}

#[test]
fn broadcast_identity_order() {
    let mut host = runtime();
    api::eval(host.vm_mut(), ORIGINAL_BROADCAST);
    let a = instantiate(&mut host, "return Def.Actor(a)");
    host.vm_mut().set_global(b"a", a).unwrap();
    let b = instantiate(&mut host, "return Def.Actor(b)");
    host.vm_mut().set_global(b"b", b).unwrap();
    let a_identity = pointer(&mut host, a);
    let b_identity = pointer(&mut host, b);
    assert!(host.set_actor_message_identity(a, a_identity));
    assert!(host.set_actor_message_identity(b, b_identity));
    let expected = if a_identity < b_identity {
        [a, b]
    } else {
        [b, a]
    };
    for reversed in [false, true] {
        let seen = api::table(host.vm_mut());
        host.vm_mut().set_global(b"seen", seen).unwrap();
        let registry = api::table(host.vm_mut());
        for (index, actor) in if reversed { [b, a] } else { [a, b] }
            .into_iter()
            .enumerate()
        {
            set_index(&mut host, registry, index + 1, actor);
        }
        host.vm_mut()
            .set_global(b"__songlua_actor_registry", registry)
            .unwrap();
        host.broadcast_message("Go", None).unwrap();
        assert_eq!(length(&mut host, seen), 2);
        for (index, actor) in expected.iter().copied().enumerate() {
            let observed = table_index(&mut host, seen, index + 1);
            assert_eq!(pointer(&mut host, observed), pointer(&mut host, actor));
        }
    }
}
#[test]
fn effect_clock_timer_preserves_itgmania_single_subtraction_and_getters() {
    for native in [false, true] {
        let mut host = runtime();
        host.set_live(native);
        let actor = instantiate(&mut host, "return Def.Actor{}");
        let advance = test_support::prepare_effect_clock(&mut host, actor).unwrap();
        let units = api::eval(
            host.vm_mut(),
            "return function(actor) return actor:GetSecsIntoEffect() end",
        )[0];
        let delta = api::eval(
            host.vm_mut(),
            "return function(actor) return actor:GetEffectDelta() end",
        )[0];
        for (elapsed, expected) in [(0.5, 0.5), (0.5, 1.0), (0.0, 1.0), (2.5, 2.5), (-0.5, 1.0)] {
            call_root(&mut host, advance, &[Value::Number(elapsed)]);
            assert_eq!(
                api::call_number(host.vm_mut(), units, &[actor]) as f32,
                expected
            );
            assert_eq!(
                api::call_number(host.vm_mut(), delta, &[actor]) as f32,
                elapsed as f32
            );
        }
        host.vm_mut().set_global(b"CLOCK_ACTOR", actor).unwrap();
        api::eval(host.vm_mut(), "CLOCK_ACTOR:effectperiod(0)");
        call_root(&mut host, advance, &[Value::Number(2.)]);
        assert_eq!(api::call_number(host.vm_mut(), units, &[actor]) as f32, 3.0);
        api::eval(host.vm_mut(), "CLOCK_ACTOR:effectperiod(-1)");
        call_root(&mut host, advance, &[Value::Number(0.)]);
        assert_eq!(api::call_number(host.vm_mut(), units, &[actor]) as f32, 4.0);
    }
}
#[test]
fn large_live_actor_snapshot_children_share_retained_handles() {
    let mut host = runtime();
    let root = instantiate(
        &mut host,
        r#"INITIAL_CHILDREN={}
local function remember(self) INITIAL_CHILDREN[#INITIAL_CHILDREN+1]=self end
local root=Def.ActorFrame{}
for ordinal=1,6000 do root[ordinal]=Def.Actor{Name='Child'..ordinal,ordinal=ordinal,InitCommand=remember} end
return root"#,
    );
    let initial = host.vm_mut().get_global(b"INITIAL_CHILDREN").unwrap();
    assert_eq!(length(&mut host, initial), 6000);
    let actors = (1..=6000)
        .map(|index| table_index(&mut host, initial, index))
        .collect::<Vec<_>>();
    let named = test_support::actor_named_children(&mut host, root).unwrap();
    host.vm_mut().unpin(named).unwrap();
    let children = test_support::actor_direct_children(&mut host, root)
        .expect("copying native child identities must retain the same handles");
    assert_eq!(children.len(), actors.len());
    for (child, actor) in children.into_iter().zip(actors) {
        assert_eq!(pointer(&mut host, child), pointer(&mut host, actor));
    }
}
#[test]
fn large_snapshot_restores_aliases_and_cells() {
    let mut host = runtime();
    let functions = api::eval(host.vm_mut(), ORIGINAL_LARGE_SNAPSHOT)[0];
    let mutate = table_index(&mut host, functions, 1);
    let verify = table_index(&mut host, functions, 2);
    let owner = host.vm_mut().get_global(b"_G").unwrap();
    let values = test_support::rust_preserving_function_state(&mut host, owner, mutate, &[])
        .expect("large snapshot and rewind retain original graph and cells");
    for root in values {
        host.vm_mut().unpin(root).unwrap();
    }
    api::call(host.vm_mut(), verify, &[]);
}

const ORIGINAL_BROADCAST: &str = r#"
        seen = {}
        local function receive(self) seen[#seen + 1] = self end
        a = { GoMessageCommand = receive }
        b = { GoMessageCommand = receive }
        __songlua_actor_registry = { a, b }
    "#;
const ORIGINAL_LARGE_SNAPSHOT: &str = r#"
local graph = {}
for i = 1, 10000 do graph[i] = {value = i} end
local original = graph
local shared = graph[9999]
graph.alias = shared
graph.self = graph
local function mutate()
    for i = 1, 10000 do graph[i].value = -i end
    graph.alias = {}
    graph.self = nil
    shared.extra = 'changed'
    graph = {replacement = true}
end
local function verify()
    assert(graph == original and graph.self == graph)
    assert(graph.alias == shared and graph[9999] == shared)
    assert(shared.extra == nil)
    for i = 1, 10000 do assert(graph[i].value == i) end
end
return {mutate, verify}
"#;

fn large_owned_tree(
    host: &mut SongRuntime,
    count: usize,
    command_name: &str,
    command_source: &str,
) -> (deadsync_song_lua::ActorScene, Vec<Value>) {
    let command = api::eval(host.vm_mut(), command_source)[0];
    host.vm_mut().set_global(b"TREE_COMMAND", command).unwrap();
    let name = api::string(host.vm_mut(), command_name);
    host.vm_mut()
        .set_global(b"TREE_COMMAND_NAME", name)
        .unwrap();
    host.vm_mut()
        .set_global(b"TREE_COUNT", Value::Number(count as f64))
        .unwrap();
    let declaration = api::eval(
        host.vm_mut(),
        r#"INITIAL_CHILDREN={}
local function remember(self) INITIAL_CHILDREN[#INITIAL_CHILDREN+1]=self end
local root=Def.ActorFrame{}
for ordinal=1,TREE_COUNT do
 root[ordinal]=Def.Actor{Name='Child'..ordinal,ordinal=ordinal,InitCommand=remember,[TREE_COMMAND_NAME]=TREE_COMMAND}
end
return root"#,
    )[0];
    let scene = host.instantiate_actor(declaration).unwrap();
    let initial = host.vm_mut().get_global(b"INITIAL_CHILDREN").unwrap();
    assert_eq!(length(host, initial), count);
    let actors = (1..=count)
        .map(|index| table_index(host, initial, index))
        .collect();
    (scene, actors)
}
#[test]
fn large_live_actor_snapshot_broadcast_preserves_identity_order() {
    let mut host = runtime();
    let seen = api::table(host.vm_mut());
    host.vm_mut().set_global(b"SEEN", seen).unwrap();
    let (_scene, mut actors) = large_owned_tree(
        &mut host,
        6000,
        "GoMessageCommand",
        ORIGINAL_SNAPSHOT_MESSAGE_COMMAND,
    );
    for actor in &actors {
        let rank = pointer(&mut host, *actor);
        assert!(host.set_actor_message_identity(*actor, rank));
    }
    host.broadcast_message("Go", None)
        .expect("native subscriber snapshots reuse retained actor identities");
    actors.sort_unstable_by_key(|actor| pointer(&mut host, *actor));
    assert_eq!(length(&mut host, seen), actors.len());
    for (index, actor) in actors.iter().copied().enumerate() {
        let value = table_index(&mut host, seen, index + 1);
        let ordinal = api::get_number(host.vm_mut(), actor, b"ordinal") as usize;
        assert_eq!(api::as_number(value) as usize, ordinal);
    }
}
#[test]
fn recursive_begin_handles_large_actor_tree_in_order() {
    let mut host = runtime();
    let calls = api::table(host.vm_mut());
    host.vm_mut().set_global(b"BEGIN_CALLS", calls).unwrap();
    let (scene, initial_actors) = large_owned_tree(
        &mut host,
        2800,
        "BeginCommand",
        ORIGINAL_RECURSIVE_BEGIN_COMMAND,
    );
    for actor in &initial_actors {
        let children =
            test_support::actor_named_children(&mut host, *actor).expect("initial child lookup");
        let value = host.vm_mut().rooted(children).unwrap();
        assert_eq!(length(&mut host, value), 0);
        host.vm_mut().unpin(children).unwrap();
    }
    host.start_actor_scene(scene)
        .expect("Begin traverses the complete actor tree");
    assert_eq!(length(&mut host, calls), 2800);
    for (index, actor) in initial_actors.iter().copied().enumerate() {
        let ordinal = index + 1;
        assert_eq!(
            api::get_number(host.vm_mut(), actor, b"ordinal") as usize,
            ordinal
        );
        let value = table_index(&mut host, calls, ordinal);
        assert_eq!(
            api::as_number(value) as usize,
            ordinal,
            "Begin runs once per actor in child order"
        );
    }
}
#[test]
fn tap_note_results_do_not_consume_auxiliary_references_per_note() {
    fn create(call: &mut deadlib_lua::HostCall<'_>) -> Result<(), deadlib_lua::Error> {
        let Value::Table(params) = call.arguments()[0] else {
            return Err(call.error(b"table expected"));
        };
        let note = test_support::fixture_tap_note(call, params)?;
        call.push(note)
    }
    let mut host = runtime();
    api::eval(host.vm_mut(), "collectgarbage('stop')");
    let notes = api::table(host.vm_mut());
    let params = api::table(host.vm_mut());
    api::set_string(host.vm_mut(), params, b"TapNoteScore", "TapNoteScore_W1");
    let function = test_support::prepare_actor_reader(&mut host, Value::Nil, create, ()).unwrap();
    let function = host.vm_mut().rooted(function).unwrap();
    let Value::Table(notes_id) = notes else {
        unreachable!()
    };
    for index in 1..=10_000 {
        api::set(
            host.vm_mut(),
            params,
            b"TapNoteOffset",
            Value::Number((index as f32 / 1000.0) as f64),
        );
        // The owning Lua array retains each note; no new Rust root is needed.
        let note = host.vm_mut().call(function, &[params]).unwrap()[0];
        host.vm_mut()
            .raw_table_set(notes_id, Value::Number(index as f64), note)
            .unwrap();
    }
    for index in [1, 5000, 10_000] {
        let note = table_index(&mut host, notes, index);
        let get = api::get(host.vm_mut(), note, b"GetTapNoteResult");
        let result = api::call_one(host.vm_mut(), get, &[note]);
        let get = api::get(host.vm_mut(), note, b"GetHoldNoteResult");
        let hold = api::call_one(host.vm_mut(), get, &[note]);
        assert_eq!(pointer(&mut host, result), pointer(&mut host, hold));
        let get = api::get(host.vm_mut(), result, b"GetTapNoteOffset");
        assert_eq!(
            api::call_number(host.vm_mut(), get, &[result]) as f32,
            index as f32 / 1000.0
        );
        let get = api::get(host.vm_mut(), result, b"GetTapNoteScore");
        assert_eq!(
            api::call_bytes(host.vm_mut(), get, &[result]),
            b"TapNoteScore_W1"
        );
    }
    api::eval(host.vm_mut(), "collectgarbage('restart')");
}

const ORIGINAL_SNAPSHOT_MESSAGE_COMMAND: &str =
    "return function(self) SEEN[#SEEN + 1] = self.ordinal end";
const ORIGINAL_RECURSIVE_BEGIN_COMMAND: &str =
    "return function(self) BEGIN_CALLS[#BEGIN_CALLS + 1] = self.ordinal end";

#[test]
fn snapshots_preserve_absence_explicit_one_and_float_bits() {
    let mut host = runtime();
    let actor = instantiate(&mut host, "return Def.Actor{__songlua_state_basezoom=2.0}");
    let initial = test_support::read_base_transform_values(&mut host, actor).unwrap();
    let getter = api::eval(
        host.vm_mut(),
        "return function(actor) return actor:GetBaseZoomX() end",
    )[0];
    assert_eq!(
        api::call_number(host.vm_mut(), getter, &[actor]) as f32,
        2.0
    );
    assert_eq!(initial[1], None);
    for value in [1.0, -0.0, f32::INFINITY, f32::NEG_INFINITY, f32::NAN, -1.25] {
        api::set(
            host.vm_mut(),
            actor,
            b"__songlua_state_basezoom_x",
            Value::Number(value as f64),
        );
        let native = test_support::read_base_transform_values(&mut host, actor).unwrap();
        assert_eq!(native[1].unwrap().to_bits(), value.to_bits());
        let copy = api::table(host.vm_mut());
        test_support::copy_live_actor_pose(&mut host, actor, copy).unwrap();
        assert_eq!(
            (api::get_number(host.vm_mut(), copy, b"__songlua_state_basezoom_x") as f32).to_bits(),
            value.to_bits()
        );
        assert!(api::get(host.vm_mut(), copy, b"__songlua_state_basezoom_y") == Value::Nil);
        assert!(api::get(host.vm_mut(), copy, b"__songlua_state_halign") == Value::Nil);
    }
    assert_eq!(test_support::base_transform_size(), 28);
}

// Compares two snapshot entries: the same value, or two groups of the same members.
const SAME_CHILD: &str = r#"return function(a, b)
    if rawequal(a, b) then return true end
    if type(a) ~= 'table' or type(b) ~= 'table' then return false end
    local ma, mb = getmetatable(a), getmetatable(b)
    if not (ma and mb and rawget(ma, '__songlua_child_group') and rawget(mb, '__songlua_child_group')) then
        return false
    end
    local i = 1
    while rawget(a, i) ~= nil or rawget(b, i) ~= nil do
        if not rawequal(rawget(a, i), rawget(b, i)) then return false end
        i = i + 1
    end
    return true
end"#;

const THREE_CHILDREN: &str =
    "return Def.ActorFrame{Def.Quad{Name='A'}, Def.Sprite{Name='B'}, Def.Actor{}}";

// (label, declaration, adjustment returning the table to query, names with
// whether the lookup answers each without the snapshot).
type ChildShape = (
    &'static str,
    &'static str,
    &'static str,
    &'static [(&'static str, bool)],
);
const CHILD_SHAPES: &[ChildShape] = &[
    (
        "unique names",
        THREE_CHILDREN,
        "return root",
        &[("A", true), ("B", true), ("", true), ("Missing", true)],
    ),
    (
        "duplicate names build a group",
        "return Def.ActorFrame{Def.Quad{Name='A'}, Def.Quad{Name='A'}, Def.Quad{Name='B'}}",
        "return root",
        &[("A", false), ("B", true)],
    ),
    (
        "one child listed twice",
        THREE_CHILDREN,
        "rawset(root, #root + 1, root[1]) return root",
        &[("A", true), ("B", true)],
    ),
    (
        "renamed child",
        THREE_CHILDREN,
        "root[1]:name('Renamed') return root",
        &[("A", true), ("Renamed", true)],
    ),
    (
        "registry values",
        THREE_CHILDREN,
        "local r = rawget(root, '__songlua_children')
         r.Alias = root[1]; r.Number = 5; r.Flag = false; r.B = root[2]; r.A = root[3]
         return root",
        &[
            ("Alias", true),
            ("Number", true),
            ("Flag", true),
            ("B", true),
            ("A", false),
            ("", true),
        ],
    ),
    (
        "registry groups",
        THREE_CHILDREN,
        "local r = rawget(root, '__songlua_children')
         r.One = setmetatable({root[1]}, {__songlua_child_group=true})
         r.Twice = setmetatable({root[2], root[2]}, {__songlua_child_group=true})
         r.Two = setmetatable({root[1], root[2]}, {__songlua_child_group=true})
         r.Empty = setmetatable({}, {__songlua_child_group=true})
         r.Unmarked = setmetatable({root[1]}, {__songlua_child_group=false})
         r.B = setmetatable({root[2]}, {__songlua_child_group=true})
         return root",
        &[
            ("One", true),
            ("B", true),
            ("Twice", true),
            ("Two", false),
            ("Empty", false),
            ("Unmarked", true),
            ("A", true),
            ("Missing", true),
        ],
    ),
    (
        "group member that is not a table",
        THREE_CHILDREN,
        "rawget(root, '__songlua_children').Bad = setmetatable({root[1], 7}, {__songlua_child_group=true})
         return root",
        &[("Bad", false), ("A", false), ("Missing", false)],
    ),
    (
        "hidden and inherited metatables",
        THREE_CHILDREN,
        "local r = rawget(root, '__songlua_children')
         r.Hidden = setmetatable({root[1]}, {__metatable='x', __songlua_child_group=true})
         return root",
        &[("Hidden", false), ("A", false)],
    ),
    (
        "group mark inherited by the metatable",
        THREE_CHILDREN,
        "local r = rawget(root, '__songlua_children')
         r.Deep = setmetatable({root[2]}, setmetatable({}, {__index={__songlua_child_group=true}}))
         return root",
        &[("Deep", false), ("B", false)],
    ),
    (
        "owned lookup ignores the registry",
        THREE_CHILDREN,
        "rawget(root, '__songlua_children').Alias = root[1]
         rawset(root, '__songlua_owned_child_lookup', true)
         return root",
        &[("Alias", true), ("A", true)],
    ),
    (
        "child that is not an actor",
        THREE_CHILDREN,
        "rawset(root, #root + 1, {}) return root",
        &[("A", false), ("Missing", false)],
    ),
    (
        "array entry that is not a table",
        THREE_CHILDREN,
        "rawset(root, #root + 1, 5) rawset(root, #root + 1, root[2]) return root",
        &[("A", true), ("B", true)],
    ),
    (
        "registry that is not a table",
        THREE_CHILDREN,
        "rawset(root, '__songlua_children', 5) return root",
        &[("A", false)],
    ),
    (
        "creation order that is not a table",
        THREE_CHILDREN,
        "getmetatable(rawget(root, '__songlua_children')).__songlua_child_names = 5 return root",
        &[("A", false)],
    ),
    (
        "locked registry",
        THREE_CHILDREN,
        "getmetatable(rawget(root, '__songlua_children')).__metatable = 'x' return root",
        &[("A", false)],
    ),
    (
        "plain table of actors",
        THREE_CHILDREN,
        "PLAIN = {root[2], root[1]} return PLAIN",
        &[("A", true), ("B", true), ("Missing", true)],
    ),
];

#[test]
fn single_name_child_lookup_matches_the_snapshot_entry() {
    for &(label, declaration, adjustment, names) in CHILD_SHAPES {
        let mut host = runtime();
        let same = api::eval(host.vm_mut(), SAME_CHILD)[0];
        let root = instantiate(&mut host, declaration);
        host.vm_mut().set_global(b"root", root).unwrap();
        // The first snapshot creates the registry the adjustments edit.
        let first = test_support::actor_named_children(&mut host, root).unwrap();
        host.vm_mut().unpin(first).unwrap();
        let target = api::eval(host.vm_mut(), adjustment)[0];
        // A table queried for the first time gets its registry here.
        if let Ok(first) = test_support::actor_named_children(&mut host, target) {
            host.vm_mut().unpin(first).unwrap();
        }
        for &(name, direct) in names {
            let snapshot = test_support::actor_named_children(&mut host, target);
            let child = test_support::actor_named_child(&mut host, target, name.as_bytes());
            match (snapshot, child) {
                (Ok(snapshot), Ok((child, answered))) => {
                    let table = host.vm_mut().rooted(snapshot).unwrap();
                    let expected = api::get(host.vm_mut(), table, name.as_bytes());
                    let actual = host.vm_mut().rooted(child).unwrap();
                    assert!(
                        api::call_bool(host.vm_mut(), same, &[expected, actual]),
                        "{label}: {name:?} is {actual:?}, the snapshot holds {expected:?}"
                    );
                    assert_eq!(answered, direct, "{label}: {name:?}");
                    host.vm_mut().unpin(snapshot).unwrap();
                    host.vm_mut().unpin(child).unwrap();
                }
                (Err(expected), Err(actual)) => {
                    assert_eq!(
                        actual.to_string(),
                        expected.to_string(),
                        "{label}: {name:?}"
                    );
                    assert!(!direct, "{label}: {name:?}");
                }
                (snapshot, child) => panic!(
                    "{label}: {name:?} snapshot {:?}, lookup {:?}",
                    snapshot.map(|_| ()),
                    child.map(|(_, answered)| answered)
                ),
            }
        }
    }
}

#[test]
fn single_name_child_lookup_returns_the_child_and_creates_the_registry_once() {
    let mut host = runtime();
    let root = instantiate(&mut host, THREE_CHILDREN);
    let first = table_index(&mut host, root, 1);
    let (child, _) = test_support::actor_named_child(&mut host, root, b"A").unwrap();
    let value = host.vm_mut().rooted(child).unwrap();
    assert_eq!(pointer(&mut host, value), pointer(&mut host, first));
    // Once the registry exists the lookup needs no snapshot.
    for (name, present) in [("A", true), ("B", true), ("Missing", false)] {
        let (child, answered) =
            test_support::actor_named_child(&mut host, root, name.as_bytes()).unwrap();
        assert!(answered, "{name}");
        let value = host.vm_mut().rooted(child).unwrap();
        assert_eq!(value != Value::Nil, present, "{name}");
    }
}

// Whether two copies hold the same entries in the same order, and share a
// table with the source exactly where the other does.
const SAME_COPY: &str = r#"
local function same(source, a, b)
    if type(a) ~= 'table' or type(b) ~= 'table' then return rawequal(a, b) end
    if rawequal(a, source) ~= rawequal(b, source) then return false end
    if rawequal(a, b) then return true end
    local ka, va = next(a)
    local kb, vb = next(b)
    while ka ~= nil or kb ~= nil do
        local key = type(ka) == 'table' and type(kb) == 'table' or rawequal(ka, kb)
        local from = nil
        if type(source) == 'table' and ka ~= nil and type(ka) ~= 'table' then
            from = rawget(source, ka)
        end
        if not key or not same(from, va, vb) then return false end
        ka, va = next(a, ka)
        kb, vb = next(b, kb)
    end
    return true
end
return same
"#;
// A label, the table to copy, and whether the production path copies it
// without the Lua helper.
type CopyShape = (&'static str, &'static str, bool);
const COPY_SHAPES: &[CopyShape] = &[
    ("empty", "return {}", true),
    (
        "actor fields",
        r#"return {
            __songlua_state_x = 1, __songlua_state_diffuse = {1, 0.5, 0, 1},
            __songlua_state_vertex_colors = {{1, 0, 0, 1}, {0, 1, 0, 1}, {0, 0, 1, 1}, {1, 1, 1, 1}},
            __songlua_visible = false, __songlua_native_name = 'n', __songlua_hibernate_seconds = 2,
            __songlua_theme_hibernating = true, __songlua_update_rate = 0.5,
            __songlua_current_x = 1, __songlua_current_y = 2, __songlua_current_z = 3,
            __songlua_diffuse = {1, 1, 1, 1}, __songlua_text_attributes = {{1, 2, {3, 4}}},
            __songlua_stroke_color = {0, 0, 0, 1}, __songlua_aux = 4, __songlua_stream_width = 5,
            __songlua_sprite_animation_length_seconds = 6, __songlua_sprite_effect_mode = 'm',
            Text = 't', Texture = 'a.png', File = 'f', StreamWidth = 7,
            __songlua_graph_display_w = 1, __songlua_scroller_x = 2,
            __songlua_aft_capture_name = 'c', __songlua_command_queue = {},
            __songlua_command_queue_deferred = true,
            __songlua_recurring_update_command = 'Update', __songlua_capture_pending = {1, 2},
            -- Not snapshot fields.
            Name = 'actor', text = 'lower', __songlua_actor_type = 'Sprite', __songlua_state = 1,
            __songlua_update_function = function() end,
            'array', {}, [true] = 1, [1.5] = 2, [{}] = 3,
        }"#,
        true,
    ),
    (
        "names at the edge of each prefix",
        r#"return {
            ['__songlua_state_'] = 1, ['__songlua_aft_'] = 2, ['__songlua_scroller_'] = 3,
            ['__songlua_graph_display_'] = 4, ['__songlua_recurring_update_'] = 5,
            ['__songlua_command_queue'] = 6, ['__songlua_command_queueX'] = 7,
            ['__songlua_capture_'] = 8, ['__songlua_capture'] = 9, ['__songlua_stat'] = 10,
            ['__songlua_state'] = 11, ['_songlua_state_x'] = 12, ['__SONGLUA_STATE_X'] = 13,
            Texture = false, texture = 1, ['Text\0'] = 2, [''] = 3,
        }"#,
        true,
    ),
    (
        "wrappers",
        "local a, b = {}, {x = 1}
         return {__songlua_wrappers = {a, b, nil, {}}, __songlua_state_x = a}",
        true,
    ),
    (
        "wrappers that are not a list",
        "return {__songlua_wrappers = 'none', __songlua_state_x = 1}",
        true,
    ),
    (
        "a wrapper that is not a table",
        "return {__songlua_wrappers = {{}, 5}}",
        false,
    ),
    (
        "a value nested too deep",
        "return {__songlua_state_deep = {{{{{{{{1}}}}}}}}}",
        false,
    ),
    (
        "a table used as a key",
        "return {__songlua_state_keyed = {[{}] = 1}}",
        false,
    ),
    (
        "metatables are neither followed nor copied",
        r#"local value = setmetatable({1, 2}, {__index = function() error('index') end})
        return setmetatable({__songlua_state_x = value}, {
            __index = function() error('index') end,
            __newindex = function() error('newindex') end,
        })"#,
        true,
    ),
];

#[test]
fn manual_draw_copy_matches_the_lua_helper() {
    for &(label, source, native) in COPY_SHAPES {
        for mutable in [false, true] {
            let mut host = runtime();
            let same = api::eval(host.vm_mut(), SAME_COPY)[0];
            let table = api::eval(host.vm_mut(), source)[0];
            let helper = test_support::actor_state_copy(&mut host, table, mutable, true);
            let production = test_support::actor_state_copy(&mut host, table, mutable, false);
            match (helper, production) {
                (Ok((expected, _)), Ok((actual, answered))) => {
                    let expected = host.vm_mut().rooted(expected).unwrap();
                    let actual = host.vm_mut().rooted(actual).unwrap();
                    assert!(
                        api::call_bool(host.vm_mut(), same, &[table, expected, actual]),
                        "{label}, mutable {mutable}"
                    );
                    assert_eq!(answered, native, "{label}, mutable {mutable}");
                }
                (Err(expected), Err(actual)) => {
                    assert_eq!(actual.to_string(), expected.to_string(), "{label}");
                    assert!(!native, "{label}");
                }
                (helper, production) => panic!(
                    "{label}: helper {:?}, production {:?}",
                    helper.map(|_| ()),
                    production.map(|(_, answered)| answered)
                ),
            }
        }
    }
}

#[test]
fn manual_draw_copy_keeps_the_snapshot_fields_and_shares_only_wrappers() {
    let mut host = runtime();
    let source = api::eval(
        host.vm_mut(),
        "WRAPPER = {}
         SOURCE = {__songlua_state_diffuse = {1, 2, 3, 4}, __songlua_wrappers = {WRAPPER},
            __songlua_capture_pending = {5}, Name = 'actor', Text = 'text'}
         return SOURCE",
    )[0];
    for mutable in [false, true] {
        let (copy, native) =
            test_support::actor_state_copy(&mut host, source, mutable, false).unwrap();
        assert!(native);
        let copy = host.vm_mut().rooted(copy).unwrap();
        host.vm_mut().set_global(b"COPY", copy).unwrap();
        host.vm_mut()
            .set_global(b"MUTABLE", Value::Boolean(mutable))
            .unwrap();
        api::eval(
            host.vm_mut(),
            "local count = 0
             for _ in pairs(COPY) do count = count + 1 end
             assert(count == (MUTABLE and 4 or 3), count)
             assert(COPY ~= SOURCE and COPY.Name == nil and COPY.Text == 'text')
             local diffuse = COPY.__songlua_state_diffuse
             assert(diffuse ~= SOURCE.__songlua_state_diffuse and #diffuse == 4 and diffuse[4] == 4)
             local wrappers = COPY.__songlua_wrappers
             assert(wrappers ~= SOURCE.__songlua_wrappers and #wrappers == 1)
             assert(wrappers[1] == WRAPPER)
             local pending = COPY.__songlua_capture_pending
             if MUTABLE then
                 assert(pending ~= SOURCE.__songlua_capture_pending and pending[1] == 5)
             else
                 assert(pending == nil)
             end",
        );
    }
}
