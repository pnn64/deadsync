//! Original private host fixtures, using the current native operations.
use deadlib_lua::{Error, HostCall, NativeCallStatus, Root, Value};
use deadsync_song_lua::{
    SongLuaActorMultiVertexPoint as Point, SongLuaCompileContext,
    rust_engine::{RustSongRuntime, test_support},
};
use std::sync::{Arc, Mutex};
#[path = "support/baked_program.rs"]
mod api;
fn runtime() -> RustSongRuntime {
    RustSongRuntime::new(&SongLuaCompileContext::new(".", "host state unit")).unwrap()
}
fn construct(
    host: &mut RustSongRuntime,
    source: &str,
) -> (deadsync_song_lua::rust_engine::ActorScene, Value) {
    let declaration = api::eval(host.vm_mut(), source)[0];
    let scene = host.instantiate_actor(declaration).unwrap();
    (scene, host.actor_scene_root(scene).unwrap())
}
fn call_root(host: &mut RustSongRuntime, f: Root) -> Result<(), Error> {
    let vm = host.vm_mut();
    let f = vm.rooted(f)?;
    vm.call(f, &[]).map(|_| ())
}
fn events(host: &mut RustSongRuntime) -> Vec<f64> {
    let table = host.vm_mut().get_global(b"events").unwrap();
    let Value::Table(table) = table else {
        panic!("events table")
    };
    (1..=host.vm_mut().raw_table_len(table).unwrap())
        .map(|i| {
            api::as_number(
                host.vm_mut()
                    .raw_table_get(table, Value::Number(i as f64))
                    .unwrap(),
            )
        })
        .collect()
}

#[test]
fn compile_update_slots_preserve_alias_visits_and_plan_rebuilds() {
    let mut host = runtime();
    // Keep the original authored callback and rate source. The two parent paths
    // below share one owned instance through the real scene append operation.
    api::eval(
        host.vm_mut(),
        r#"
            events = {}
            local function update(_, delta) events[#events+1] = delta end
            local shared = { __songlua_update_function = update }
            local root = {
                { shared, __songlua_update_rate = 2 },
                { shared, __songlua_update_rate = 3 },
            }
            ORIGINAL_ROOT=root
            return root
        "#,
    );
    let (root_scene, root) = construct(
        &mut host,
        "return Def.ActorFrame{Def.ActorFrame{InitCommand=function(self) self:SetUpdateRate(ORIGINAL_ROOT[1].__songlua_update_rate) end},Def.ActorFrame{InitCommand=function(self) self:SetUpdateRate(ORIGINAL_ROOT[2].__songlua_update_rate) end}}",
    );
    let parents = test_support::actor_direct_children(&mut host, root).unwrap();
    let (shared_scene, shared) = construct(
        &mut host,
        "return Def.ActorFrame{InitCommand=function(self) self:SetUpdateFunction(ORIGINAL_ROOT[1][1].__songlua_update_function) end}",
    );
    for parent in &parents {
        assert!(host.append_actor_scene(shared_scene, *parent).unwrap());
    }
    assert_eq!(
        test_support::actor_direct_children(&mut host, parents[0]).unwrap(),
        [shared]
    );
    assert_eq!(
        test_support::actor_direct_children(&mut host, parents[1]).unwrap(),
        [shared]
    );
    host.begin_actor_playback();
    test_support::advance_live_actor(&mut host, root, 1.).unwrap();
    assert_eq!(
        events(&mut host),
        [2., 3.],
        "shared actors retain each parent's chronological delta"
    );
    api::eval(
        host.vm_mut(),
        r#"NEW_ACTOR={ __songlua_update_function = function(_, delta)
            events[#events+1] = delta
        end, __songlua_update_rate = 5 }"#,
    );
    let (new_scene, _) = construct(
        &mut host,
        "return Def.ActorFrame{InitCommand=function(self) self:SetUpdateFunction(NEW_ACTOR.__songlua_update_function):SetUpdateRate(NEW_ACTOR.__songlua_update_rate) end}",
    );
    assert!(host.append_actor_scene(new_scene, root).unwrap());
    test_support::invalidate_actor_update_plans(&mut host).unwrap();
    test_support::advance_live_actor(&mut host, root, 0.5).unwrap();
    assert_eq!(
        events(&mut host),
        [2., 3., 1., 1.5, 2.5],
        "rebuilds resize scratch without retaining old deltas"
    );
    assert!(host.actor_scene_root(root_scene).is_some());
}

#[test]
fn capture_reset_preserves_compiler_snapshots_and_reuses_live_blocks() {
    let mut host = runtime();
    let (_, actor) = construct(&mut host, "return Def.Actor{}");
    let blocks = api::eval(host.vm_mut(), "return {42}")[0];
    api::set(host.vm_mut(), actor, b"__songlua_capture_blocks", blocks);
    api::set(
        host.vm_mut(),
        actor,
        b"__songlua_capture_cursor",
        Value::Number(3.),
    );
    api::set(host.vm_mut(), actor, b"__songlua_capture_block", blocks);
    let reset = test_support::prepare_capture_reset(&mut host, actor).unwrap();
    call_root(&mut host, reset).unwrap();
    let Value::Table(table) = blocks else {
        unreachable!()
    };
    assert_eq!(
        host.vm_mut()
            .raw_table_get(table, Value::Number(1.))
            .unwrap(),
        Value::Number(42.),
        "saved compiler captures retain their data"
    );
    let fresh = api::get(host.vm_mut(), actor, b"__songlua_capture_blocks");
    assert_ne!(fresh, blocks);
    assert_eq!(
        api::get_number(host.vm_mut(), actor, b"__songlua_capture_cursor"),
        0.
    );
    assert_eq!(
        api::get(host.vm_mut(), actor, b"__songlua_capture_block"),
        Value::Nil
    );
    let Value::Table(table) = fresh else {
        panic!("fresh capture table")
    };
    host.vm_mut()
        .raw_table_set(table, Value::Number(1.), Value::Number(17.))
        .unwrap();
    host.set_live(true);
    call_root(&mut host, reset).unwrap();
    assert_eq!(
        api::get(host.vm_mut(), actor, b"__songlua_capture_blocks"),
        fresh
    );
    assert_eq!(host.vm_mut().raw_table_len(table).unwrap(), 0);
    api::set(
        host.vm_mut(),
        actor,
        b"__songlua_capture_blocks",
        Value::Boolean(false),
    );
    assert!(
        call_root(&mut host, reset).is_err(),
        "live reset retains type validation"
    );
}

type Points = Arc<Mutex<Option<Vec<Point>>>>;
fn mesh_reader(call: &mut HostCall<'_>) -> Result<(), Error> {
    let output = call.function_data::<Points>().unwrap().clone();
    let Value::Table(actor) = call.captures()[2] else {
        unreachable!()
    };
    let Value::Table(vertices) = call.arguments()[0] else {
        return Err(call.error(b"vertices table expected"));
    };
    let packed = call.arguments()[1] == Value::Boolean(true);
    *output.lock().unwrap() = Some(test_support::fixture_mesh_points(
        call, actor, vertices, packed,
    )?);
    Ok(())
}
fn point_bits(p: &Point) -> [u32; 9] {
    [
        p.pos[0], p.pos[1], p.z, p.color[0], p.color[1], p.color[2], p.color[3], p.uv[0], p.uv[1],
    ]
    .map(f32::to_bits)
}
#[test]
fn packed_vertex_upload_matches_general_reader_and_restores_stack() {
    let mut host = runtime();
    let (_, actor) = construct(&mut host, "return Def.ActorMultiVertex{}");
    let output: Points = Arc::new(Mutex::new(None));
    let reader =
        test_support::prepare_actor_reader(&mut host, actor, mesh_reader, output.clone()).unwrap();
    for expression in [
        "{}",
        "{{{1,2,3},{0.25,0.5,0.75,1},{0.1,0.2}}}",
        "{{{0/0,2,-3},{1,1,1,1},{0,1}}}",
        "{{{1,2},{1,0,0},{0,1}}}",
        "{{{1,2,3},'#ff0000',{0/0,1}}}",
        "{{{'1','2','3'},{'0.5',1,1,1},{'0.1','0.2'}}}",
        "{false,{{1,2,3},{1,1,1,1},{0,1}}}",
    ] {
        let vertices = api::eval(host.vm_mut(), &format!("return {expression}"))[0];
        let vm = host.vm_mut();
        let f = vm.rooted(reader).unwrap();
        vm.call(f, &[vertices, Value::Boolean(false)]).unwrap();
        let expected = output.lock().unwrap().take().unwrap();
        vm.with_native_stack(|stack| {
            stack.set_top(0)?;
            stack.push(Value::Number(42.))?;
            stack.push(f)?;
            stack.push(vertices)?;
            stack.push(Value::Boolean(true))?;
            assert_eq!(
                stack.protected_call(2, Some(0), None)?,
                NativeCallStatus::Ok
            );
            assert_eq!(stack.top(), 1);
            assert_eq!(stack.value(1), Some(Value::Number(42.)));
            Ok(())
        })
        .unwrap();
        let actual = output.lock().unwrap().take().unwrap();
        assert_eq!(actual.len(), expected.len(), "{expression}");
        for (actual, expected) in actual.iter().zip(&expected) {
            assert_eq!(point_bits(actual), point_bits(expected), "{expression}")
        }
        assert_eq!(
            api::as_number(api::eval(host.vm_mut(), "return 6*7")[0]) as u32,
            42
        );
    }
}
