//! Original standalone actor runtime fixtures on deadlib-lua.
use deadlib_lua::{Root, Value};
use deadsync_song_lua::{
    SongLuaCompileContext,
    rust_engine::{RustSongRuntime, test_support},
};
#[path = "support/baked_program.rs"]
mod api;
fn runtime() -> RustSongRuntime {
    RustSongRuntime::new(&SongLuaCompileContext::new(".", "actor runtime unit")).unwrap()
}
fn table_index(host: &mut RustSongRuntime, owner: Value, index: usize) -> Value {
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
fn set_index(host: &mut RustSongRuntime, owner: Value, index: usize, value: Value) {
    let Value::Table(owner) = owner else {
        panic!("table expected")
    };
    host.vm_mut()
        .raw_table_set(owner, Value::Number(index as f64), value)
        .unwrap();
}
fn length(host: &mut RustSongRuntime, table: Value) -> usize {
    let Value::Table(table) = table else {
        panic!("table expected")
    };
    host.vm_mut().raw_table_len(table).unwrap()
}
fn pointer(host: &mut RustSongRuntime, value: Value) -> usize {
    host.vm_mut().object_pointer(value).unwrap()
}
fn instantiate(host: &mut RustSongRuntime, source: &str) -> Value {
    let declaration = api::eval(host.vm_mut(), source)[0];
    let scene = host.instantiate_actor(declaration).unwrap();
    host.actor_scene_root(scene).unwrap()
}
fn call_root(host: &mut RustSongRuntime, function: Root, args: &[Value]) {
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
    host: &mut RustSongRuntime,
    count: usize,
    command_name: &str,
    command_source: &str,
) -> (deadsync_song_lua::rust_engine::ActorScene, Vec<Value>) {
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
