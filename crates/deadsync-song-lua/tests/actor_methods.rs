//! Production method identities and weak ownership in the Rust VM.
use deadlib_lua::{Value, compile};
use deadsync_song_lua::{
    SongLuaCompileContext,
    {
        ActorScene, SongRuntime,
        test_support::{self, ActorMethodIdentity as Method},
    },
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering::Relaxed},
};

struct Dropped(Arc<AtomicBool>);
impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.store(true, Relaxed);
    }
}

fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile(source.as_bytes()).unwrap())
        .unwrap()
        .to_vec()
}

fn scene() -> (SongRuntime, ActorScene, deadlib_lua::TableId) {
    let context = SongLuaCompileContext::new(".", "method identity fixtures");
    let mut runtime = SongRuntime::new(&context).unwrap();
    let value = execute(&mut runtime, "return Def.ActorFrame{Def.Quad{}}")[0];
    let scene = runtime.instantiate_actor(value).unwrap();
    let root = runtime.actor_scene_root(scene).unwrap();
    runtime.vm_mut().set_global(b"root", root).unwrap();
    let Value::Table(actor) = execute(&mut runtime, "return root[1]")[0] else {
        panic!("child actor")
    };
    (runtime, scene, actor)
}

#[test]
fn metatable_callbacks_keep_their_native_capture_counts() {
    let (mut runtime, _scene, _) = scene();
    assert_eq!(
        execute(
            &mut runtime,
            "local mt=getmetatable(root[1]); return debug.getinfo(mt.__index, 'u').nups, debug.getinfo(mt.__newindex, 'u').nups",
        ),
        [Value::Number(2.), Value::Number(2.)],
    );
}

#[test]
fn method_identity_is_exact_and_does_not_root_replaced_functions() {
    let (mut runtime, _scene, actor) = scene();
    let values = execute(
        &mut runtime,
        "return root[1].__songlua_bound_methods.x,Actor.x,getmetatable(root[1]).__index,getmetatable(root[1]).__newindex,function()end",
    );
    let function = values[0];
    let alias = function;
    let first = runtime.vm_mut().pin(function).unwrap();
    let second = runtime.vm_mut().pin(alias).unwrap();
    assert_eq!(
        test_support::actor_method_identity(&runtime, alias),
        Some(Method::Bound("x", actor))
    );
    assert_eq!(
        test_support::actor_method_identity(&runtime, values[1]),
        Some(Method::Forward("x"))
    );
    assert_eq!(
        test_support::actor_method_identity(&runtime, values[2]),
        Some(Method::Lookup)
    );
    assert_eq!(
        test_support::actor_method_identity(&runtime, values[3]),
        Some(Method::Assign)
    );
    assert_eq!(
        test_support::actor_method_identity(&runtime, values[4]),
        None
    );

    let dropped = Arc::new(AtomicBool::new(false));
    let other = runtime
        .vm_mut()
        .new_host_function_with_data(|_| Ok(()), &[], Dropped(dropped.clone()))
        .unwrap();
    let keep = runtime.vm_mut().pin(other).unwrap();
    assert_eq!(test_support::actor_method_identity(&runtime, other), None);
    runtime.vm_mut().collect_garbage().unwrap();
    assert!(!dropped.load(Relaxed));
    runtime.vm_mut().unpin(keep).unwrap();
    runtime.vm_mut().collect_garbage().unwrap();
    assert!(
        dropped.load(Relaxed),
        "method lookup must not retain an unrelated host closure"
    );

    execute(
        &mut runtime,
        "root[1].__songlua_bound_methods.x=function()end",
    );
    runtime.vm_mut().unpin(first).unwrap();
    runtime.vm_mut().collect_garbage().unwrap();
    assert_eq!(
        test_support::actor_method_identity(&runtime, alias),
        Some(Method::Bound("x", actor))
    );
    runtime.vm_mut().unpin(second).unwrap();
    runtime.vm_mut().collect_garbage().unwrap();
    assert_eq!(
        test_support::actor_method_identity(&runtime, function),
        None
    );
    for _ in 0..128 {
        let replacement = runtime.vm_mut().new_host_function(|_| Ok(()), &[]).unwrap();
        assert_eq!(
            test_support::actor_method_identity(&runtime, replacement),
            None
        );
        runtime.vm_mut().collect_garbage().unwrap();
    }
}

#[test]
fn position_identities_keep_setter_target_and_weak_function_roots() {
    for setter in ["visible", "x", "y"] {
        let (mut runtime, scene, actor) = scene();
        let dropped = Arc::new(AtomicBool::new(false));
        let marker = runtime
            .vm_mut()
            .new_host_function_with_data(|_| Ok(()), &[], Dropped(dropped.clone()))
            .unwrap();
        let key = runtime.vm_mut().intern(b"fixture_marker");
        runtime.vm_mut().raw_table_set(actor, key, marker).unwrap();
        let function = execute(
            &mut runtime,
            &format!("kept=root[1].__songlua_bound_methods.{setter};root=nil;return kept"),
        )[0];
        assert_eq!(
            test_support::actor_method_identity(&runtime, function),
            Some(Method::Bound(setter, actor))
        );
        assert!(runtime.retire_actor_scene(scene).unwrap());
        test_support::reset_actor_subscriptions(&mut runtime).unwrap();
        execute(
            &mut runtime,
            "collectgarbage('collect');collectgarbage('collect')",
        );
        assert!(!dropped.load(Relaxed), "saved setter must retain its actor");
        assert_eq!(
            test_support::actor_method_identity(&runtime, function),
            Some(Method::Bound(setter, actor))
        );
        runtime.vm_mut().set_global(b"kept", Value::Nil).unwrap();
        runtime.vm_mut().collect_garbage().unwrap();
        runtime.vm_mut().collect_garbage().unwrap();
        assert!(
            dropped.load(Relaxed),
            "method metadata must not retain the discarded bound actor"
        );
        assert_eq!(
            test_support::actor_method_identity(&runtime, function),
            None
        );
    }
}

// Member reads resolved without calling the lookup handler must be the reads
// the handler itself would produce. An instruction limit disables the inline
// path, so the same script runs through the handler for comparison.
const LOOKUPS: &str = r#"
local out = {}
local function note(label, value)
    local kind = type(value)
    if kind == 'function' then kind = debug.getinfo(value, 'S').what end
    out[#out + 1] = label .. '=' .. kind
end
local function failure(label, read)
    local ok, message = pcall(read)
    out[#out + 1] = label .. '=' .. tostring(ok) .. ':' .. tostring(message)
end
local actors = {root}
for index = 1, 7 do actors[#actors + 1] = root[index] end
local keys = {'zoom', 'x', 'visible', 'GetChild', 'SetUpdateFunction', 'settext', 'SetVertices',
    'SetTarget', 'LoadFromSongBanner', 'EnableDepthBuffer', 'missing', 'InitCommand', 17, true}
Actor.custom = function() end
for index, actor in ipairs(actors) do
    for _, key in ipairs(keys) do
        local value = actor[key]
        note(index .. '.' .. tostring(key), value)
        local class = _G[actor.__songlua_actor_type == 'Quad' and 'Sprite' or actor.__songlua_actor_type]
        out[#out + 1] = tostring(value == Actor[key]) .. tostring(class ~= nil and value == class[key])
    end
    note(index .. '.custom', actor.custom)
end
local quad, sprite = root[1], root[2]
-- A class replaced by a table with a function handler, a non-table and nil.
local original = Sprite
Sprite = setmetatable({}, {__index = function(_, key)
    if key == 'Marker' then return 42 end
    return original[key]
end})
note('hooked.Marker', quad.Marker); note('hooked.zoom', quad.zoom); note('hooked.frame', root.Marker)
Sprite = 5
failure('number class', function() return sprite.zoom end)
Sprite = nil
note('absent class zoom', sprite.zoom); note('absent class Load', sprite.Load)
Sprite = {zoom = 'shadow'}
note('plain class zoom', quad.zoom); note('plain class x', quad.x)
Sprite = original
-- A hook on one actor's own metatable, then an own field that shadows a method.
local mt = getmetatable(quad)
local lookup = mt.__index
mt.__index = function(self, key)
    if key == 'hooked' then return 'yes' end
    return lookup(self, key)
end
note('instance hook', quad.hooked); note('instance zoom', quad.zoom); note('other hook', sprite.hooked)
mt.__index = lookup
quad.zoom = 'own'
note('own zoom', quad.zoom)
quad.zoom = nil
note('restored zoom', quad.zoom)
-- Member names and actor types the handler coerces or rejects.
failure('binary key', function() return quad['\255'] end)
local kind = rawget(quad, '__songlua_actor_type')
rawset(quad, '__songlua_actor_type', 12)
note('numeric type zoom', quad.zoom); note('numeric type Load', quad.Load)
rawset(quad, '__songlua_actor_type', {})
failure('table type', function() return quad.zoom end)
rawset(quad, '__songlua_actor_type', 'Unknown')
note('unknown type zoom', quad.zoom); note('unknown type Load', quad.Load)
rawset(quad, '__songlua_actor_type', nil)
note('absent type zoom', quad.zoom); note('absent type Load', quad.Load)
rawset(quad, '__songlua_actor_type', kind)
return table.concat(out, '\n')
"#;

fn lookups(limit: bool) -> String {
    let context = SongLuaCompileContext::new(".", "lookup resolution fixtures");
    let mut runtime = SongRuntime::new(&context).unwrap();
    let value = execute(
        &mut runtime,
        "return Def.ActorFrame{Def.Quad{},Def.Sprite{},Def.BitmapText{},Def.ActorMultiVertex{},\
            Def.ActorFrameTexture{},Def.ActorProxy{},Def.Actor{}}",
    )[0];
    let scene = runtime.instantiate_actor(value).unwrap();
    let root = runtime.actor_scene_root(scene).unwrap();
    runtime.vm_mut().set_global(b"root", root).unwrap();
    if limit {
        runtime.vm_mut().set_instruction_limit(Some(u64::MAX));
    }
    let Value::String(text) = execute(&mut runtime, LOOKUPS)[0] else {
        panic!("lookup observations")
    };
    String::from_utf8(runtime.vm_mut().string_bytes(text).unwrap().to_vec()).unwrap()
}

#[test]
fn inline_member_resolution_matches_the_lookup_handler() {
    let inline = lookups(false);
    let handler = lookups(true);
    for (inline, handler) in inline.lines().zip(handler.lines()) {
        assert_eq!(inline, handler);
    }
    assert_eq!(inline.lines().count(), handler.lines().count());
    // Spot checks keep the comparison from passing on two empty or broken runs.
    for expected in [
        "1.zoom=C",
        "2.zoom=C",
        "1.GetChild=C",
        "2.GetChild=nil",
        "4.settext=C",
        "2.settext=nil",
        "1.17=nil",
        "3.custom=Lua",
        "hooked.Marker=number",
        "hooked.frame=nil",
        "absent class zoom=C",
        "plain class zoom=string",
        "plain class x=nil",
        "instance hook=string",
        "other hook=nil",
        "own zoom=string",
        "restored zoom=C",
        "unknown type zoom=C",
        "absent type zoom=C",
    ] {
        assert!(
            inline.lines().any(|line| line == expected),
            "missing {expected}\n{inline}"
        );
    }
    for failed in [
        "number class=false:",
        "binary key=false:",
        "table type=false:",
    ] {
        assert!(
            inline.lines().any(|line| line.starts_with(failed)),
            "missing {failed}\n{inline}"
        );
    }
}
