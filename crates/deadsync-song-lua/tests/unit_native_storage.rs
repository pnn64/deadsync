//! Native state promotion and allocation contracts from the original host units.
use deadlib_lua::{Error, HostCall, Root, Value};
use deadsync_song_lua::{
    SongLuaCompileContext, SongLuaOverlayState, {SongRuntime, test_support},
};
use std::sync::{Arc, Mutex};
#[path = "../../../tests/support/allocations.rs"]
mod allocations;
#[path = "support/baked_program.rs"]
mod api;
fn runtime() -> SongRuntime {
    SongRuntime::new(&SongLuaCompileContext::new(".", "native storage unit")).unwrap()
}
fn instantiate(host: &mut SongRuntime, source: &str) -> Value {
    let declaration = api::eval(host.vm_mut(), source)[0];
    let scene = host.instantiate_actor(declaration).unwrap();
    host.actor_scene_root(scene).unwrap()
}
fn read(
    host: &mut SongRuntime,
    reader: Root,
    output: &test_support::FixtureStateOutput,
) -> SongLuaOverlayState {
    let vm = host.vm_mut();
    let reader = vm.rooted(reader).unwrap();
    vm.call(reader, &[]).unwrap();
    output.lock().unwrap().take().unwrap().unwrap()
}

#[test]
fn overlay_reads_keep_native_vectors_without_heap_churn() {
    let mut host = runtime();
    host.set_live(true);
    // The former NativeVector userdata slots are owned render fields now. Feed
    // the same vector values into construction and inspect the actual live reader.
    api::eval(
        host.vm_mut(),
        r#"ORIGINAL_OVERLAY={
        __songlua_actor_type='Quad',
        __songlua_state_text_align='left',
        __songlua_state_effect_mode='pulse',
        __songlua_state_effect_clock='beat',
        __songlua_state_blend='add',
        __songlua_state_stretch_rect={0,0,10,20},
    }"#,
    );
    let actor = instantiate(
        &mut host,
        "ORIGINAL_OVERLAY.__songlua_state_size={12.0,24.0};ORIGINAL_OVERLAY.__songlua_state_shadow_len={3.0,5.0};ORIGINAL_OVERLAY.__songlua_state_base_rotation={7.0,9.0,11.0};return Def.Quad(ORIGINAL_OVERLAY)",
    );
    host.begin_actor_playback();
    let (reader, output) =
        test_support::prepare_actor_state_reader(&mut host, actor, true).unwrap();
    let expected = read(&mut host, reader, &output);
    assert_eq!(expected.size, Some([12., 24.]));
    assert_eq!(expected.shadow_len, [3., 5.]);
    allocations::assert_no_churn(|| {
        for _ in 0..128 {
            assert_eq!(read(&mut host, reader, &output), expected);
        }
    });
}

#[test]
fn native_render_flag_promotion_avoids_authored_reads_and_retains_model_defaults() {
    let mut host = runtime();
    host.set_live(true);
    let loading = api::eval(
        host.vm_mut(),
        r#"
TEXTURE_READS = 0
return setmetatable({__songlua_actor_type='Sprite'}, {
    __index=function(_, key)
        if key == 'Texture' then TEXTURE_READS=TEXTURE_READS+1 end
    end,
})
"#,
    )[0];
    test_support::read_actor_initial_state(&mut host, loading).unwrap();
    host.vm_mut()
        .set_global(b"LOADING_SPRITE", loading)
        .unwrap();
    let actor = instantiate(&mut host, "return Def.Sprite(LOADING_SPRITE)");
    host.vm_mut().set_global(b"SPRITE", actor).unwrap();
    api::eval(
        host.vm_mut(),
        "setmetatable(SPRITE,getmetatable(LOADING_SPRITE))",
    );
    let reads = api::as_number(host.vm_mut().get_global(b"TEXTURE_READS").unwrap()) as u32;
    assert!(reads > 0);
    host.begin_actor_playback();
    assert_eq!(
        api::as_number(host.vm_mut().get_global(b"TEXTURE_READS").unwrap()) as u32,
        reads
    );
    let model = instantiate(&mut host, "return Def.Model{}");
    let state = test_support::read_live_actor_state(&mut host, model).unwrap();
    assert!(
        state.depth_test && state.depth_write && state.texture_wrapping && state.texture_filtering
    );
    host.vm_mut().set_global(b"MODEL", model).unwrap();
    api::eval(host.vm_mut(), "MODEL:ztest(false):texturewrapping(false)");
    assert!(
        !test_support::read_live_actor_state(&mut host, model)
            .unwrap()
            .depth_test
    );
    test_support::init_actor_native_state(&mut host, model).unwrap();
    let state = test_support::read_live_actor_state(&mut host, model).unwrap();
    assert!(
        state.depth_test && state.depth_write && state.texture_wrapping && state.texture_filtering
    );
}

#[test]
fn native_pose_definition_clones_keep_destination_and_independence() {
    let mut host = runtime();
    host.set_live(true);
    let actor = instantiate(&mut host, "return Def.Quad{authored={score=17}}");
    host.vm_mut().set_global(b"ACTOR", actor).unwrap();
    host.begin_actor_playback();
    api::eval(
        host.vm_mut(),
        "ACTOR:linear(1):diffuse(0.25,0.5,0.75,1):glow(1,0.75,0.5,0.25):diffuseupperleft(0.5,0.5,0.5,0.5):diffuseupperright(0.5,0.5,0.5,0.5):diffuselowerleft(0.5,0.5,0.5,0.5):diffuselowerright(0.5,0.5,0.5,0.5):ztest(true):SetTextureFiltering(false):basezoom(2):basezoomx(1):halign(0.25):x(12):zoomy(3):skewx(0.25):fadeleft(0.5)",
    );
    let copies = api::eval(host.vm_mut(), "return DeepCopy(ACTOR),ACTOR..{}");
    assert_eq!(copies.len(), 2);
    api::eval(host.vm_mut(), "ACTOR:diffuse(0,0,0,0):x(24):ztest(false)");
    for copy in copies {
        let pose = test_support::read_actor_initial_state(&mut host, copy).unwrap();
        assert_eq!(pose.diffuse, [0.25, 0.5, 0.75, 1.]);
        assert_eq!(pose.glow, [1., 0.75, 0.5, 0.25]);
        assert_eq!(pose.vertex_colors, Some([[0.5; 4]; 4]));
        assert!(pose.depth_test && !pose.texture_filtering);
        assert_eq!(
            (pose.basezoom, pose.basezoom_x, pose.halign),
            (2., 1., 0.25)
        );
        assert_eq!(
            (pose.x, pose.zoom_y, pose.skew_x, pose.fadeleft),
            (12., 3., 0.25, 0.5)
        );
        let authored = api::get(host.vm_mut(), copy, b"authored");
        assert_eq!(api::get_number(host.vm_mut(), authored, b"score"), 17.);
        // DeepCopy exports a loading table; CloneTemplate exports a native declaration.
        // Construct each through the same public declaration path before live reads.
        host.vm_mut().set_global(b"COPY", copy).unwrap();
        let copy = instantiate(&mut host, "return Def.Quad(COPY)");
        host.vm_mut().set_global(b"COPY", copy).unwrap();
        test_support::ensure_native_replay(&mut host, copy).unwrap();
        assert!(
            !test_support::read_live_actor_state(&mut host, actor)
                .unwrap()
                .depth_test
        );
        assert!(
            test_support::read_live_actor_state(&mut host, copy)
                .unwrap()
                .depth_test
        );
        assert_eq!(
            api::get(host.vm_mut(), copy, b"__songlua_state_depth_test"),
            Value::Nil
        );
        assert_eq!(
            api::get(host.vm_mut(), copy, b"__songlua_state_basezoom_x"),
            Value::Nil
        );
        let values = api::eval(
            host.vm_mut(),
            "return COPY:GetBaseZoomX(),COPY:GetBaseZoomY()",
        );
        assert_eq!(values, [Value::Number(1.), Value::Number(2.)]);
        api::eval(host.vm_mut(), "COPY:halign(0.75)");
        assert_eq!(
            test_support::read_live_actor_state(&mut host, actor)
                .unwrap()
                .halign,
            0.25
        );
        test_support::set_actor_sampled_value(
            &mut host,
            copy,
            deadsync_song_lua::SongLuaOverlayUpdateTarget::BaseZoomX,
            &deadsync_song_lua::SongLuaOverlayUpdateValue::F32(5.),
        )
        .unwrap();
        assert_eq!(
            api::eval(host.vm_mut(), "return COPY:GetBaseZoomX()"),
            [Value::Number(5.)]
        );
        assert_eq!(
            api::get(host.vm_mut(), copy, b"__songlua_state_basezoom_x"),
            Value::Nil
        );
        api::eval(host.vm_mut(), "COPY:diffuse(0.75,0.75,0.75,0.75):x(36)");
        let current = test_support::read_actor_initial_state(&mut host, actor).unwrap();
        assert_eq!(current.diffuse, [0.; 4]);
        assert_eq!(current.x, 24.);
        assert_eq!(
            test_support::read_live_actor_state(&mut host, copy)
                .unwrap()
                .x,
            36.
        );
    }
}

type VectorOutput<const N: usize> = Arc<Mutex<Option<Option<[f32; N]>>>>;
fn vector_reader<const N: usize>(call: &mut HostCall<'_>) -> Result<(), Error> {
    let output = call.function_data::<VectorOutput<N>>().unwrap().clone();
    let value = call.arguments()[0];
    let result = test_support::fixture_vector_value::<N>(call, value)?;
    *output.lock().unwrap() = Some(result);
    Ok(())
}
fn vector_probe<const N: usize>(host: &mut SongRuntime) -> (Value, VectorOutput<N>) {
    let output = Arc::new(Mutex::new(None));
    let owner = api::table(host.vm_mut());
    let root = test_support::prepare_actor_reader(host, owner, vector_reader::<N>, output.clone())
        .unwrap();
    (host.vm_mut().rooted(root).unwrap(), output)
}
fn vector<const N: usize>(
    host: &mut SongRuntime,
    f: Value,
    out: &VectorOutput<N>,
    value: Value,
) -> Result<Option<[f32; N]>, Error> {
    host.vm_mut().call(f, &[value])?;
    Ok(out.lock().unwrap().take().unwrap())
}

#[test]
fn native_vectors_reject_foreign_userdata_and_retain_owned_values() {
    let mut host = runtime();
    host.set_live(true);
    let actor = instantiate(
        &mut host,
        "return Def.Actor{__songlua_state_shadow_len={1,2},InitCommand=function(self) self:shadowlengthx(1):shadowlengthy(-2) end}",
    );
    host.begin_actor_playback();
    let (f, out) = vector_probe::<2>(&mut host);
    // The retired typed-userdata borrow guards have no counterpart: vectors are
    // inline Rust fields now, and Rust enforces their borrowing. Foreign Lua
    // userdata must still take the table-conversion error path without touching
    // the actor's native values.
    for number in [1_u32, 42] {
        let value = host.vm_mut().try_new_userdata(4).unwrap();
        host.vm_mut().pin(value).unwrap();
        let Value::Userdata(id) = value else {
            unreachable!()
        };
        host.vm_mut()
            .userdata_bytes_mut(id)
            .unwrap()
            .copy_from_slice(&number.to_ne_bytes());
        let error = vector(&mut host, f, &out, value).unwrap_err();
        assert!(error.to_string().contains("table expected"));
        assert_eq!(
            test_support::native_shadow_storage(&host, actor).1,
            [1., 2.]
        );
    }
    host.vm_mut().set_global(b"ACTOR", actor).unwrap();
    api::eval(host.vm_mut(), "ACTOR:shadowlengthx(5):shadowlengthy(-6)");
    assert_eq!(
        test_support::native_shadow_storage(&host, actor).1,
        [5., 6.]
    );
}

#[test]
fn native_vectors_preserve_table_coercions_and_errors() {
    let mut host = runtime();
    let (f, out) = vector_probe::<3>(&mut host);
    for (expression, expected) in [
        ("{1,'2',3}", Some([1., 2., 3.])),
        ("{1,false,3}", None),
        ("{1}", None),
        ("{}", None),
        (
            "setmetatable({}, {__index=function() error('must use raw reads') end})",
            None,
        ),
    ] {
        let value = api::eval(host.vm_mut(), &format!("return {expression}"))[0];
        assert_eq!(vector(&mut host, f, &out, value).unwrap(), expected);
    }
    for expression in ["false", "12", "'bad'", "function() end"] {
        let value = api::eval(host.vm_mut(), &format!("return {expression}"))[0];
        assert!(
            vector(&mut host, f, &out, value)
                .unwrap_err()
                .to_string()
                .contains("table expected")
        );
    }
    let values = [-0., f32::NAN, f32::INFINITY, -f32::INFINITY, 0.25];
    let table = api::table(host.vm_mut());
    let Value::Table(t) = table else {
        unreachable!()
    };
    for (i, value) in values.into_iter().enumerate() {
        host.vm_mut()
            .raw_table_set(
                t,
                Value::Number((i + 1) as f64),
                Value::Number(value as f64),
            )
            .unwrap();
    }
    let (f5, out5) = vector_probe::<5>(&mut host);
    assert_eq!(
        values.map(f32::to_bits),
        vector(&mut host, f5, &out5, table)
            .unwrap()
            .unwrap()
            .map(f32::to_bits)
    );
    let short = api::eval(host.vm_mut(), "return {1.0,2.0}")[0];
    assert!(vector(&mut host, f, &out, short).unwrap().is_none());
}

#[test]
fn native_vectors_reuse_slots_and_snapshot_independently() {
    let mut host = runtime();
    host.set_live(true);
    // Shadow length is the current immediate native two-component field; the
    // old generic NativeVector userdata slot no longer exists.
    let actor = instantiate(
        &mut host,
        "return Def.Actor{__songlua_state_shadow_len={12,24},InitCommand=function(self) self:shadowlengthx(12):shadowlengthy(-24) end}",
    );
    host.begin_actor_playback();
    host.vm_mut().set_global(b"ACTOR", actor).unwrap();
    let original = test_support::native_shadow_storage(&host, actor).0;
    let state = test_support::save_native_actor_state(&mut host, actor).unwrap();
    let cloned = api::eval(host.vm_mut(), "return DeepCopy(ACTOR)")[0];
    let class = api::eval(host.vm_mut(), "return Actor")[0];
    let x = api::get(host.vm_mut(), class, b"shadowlengthx");
    let y = api::get(host.vm_mut(), class, b"shadowlengthy");
    allocations::assert_no_churn(|| {
        for i in 0..100 {
            host.vm_mut()
                .call(x, &[actor, Value::Number(i as f64)])
                .unwrap();
            host.vm_mut().call(y, &[actor, Value::Number(-8.)]).unwrap();
            assert_eq!(
                test_support::native_shadow_storage(&host, actor).1,
                [i as f32, 8.]
            );
        }
    });
    assert_eq!(
        original,
        test_support::native_shadow_storage(&host, actor).0
    );
    assert_eq!(
        test_support::read_actor_initial_state(&mut host, cloned)
            .unwrap()
            .shadow_len,
        [12., 24.]
    );
    test_support::restore_native_actor_state(&mut host, state).unwrap();
    assert_eq!(
        test_support::native_shadow_storage(&host, actor).1,
        [12., 24.]
    );
    api::eval(host.vm_mut(), "ACTOR:shadowlengthx(3):shadowlengthy(-4)");
    assert_eq!(
        test_support::read_actor_initial_state(&mut host, cloned)
            .unwrap()
            .shadow_len,
        [12., 24.]
    );
    let foreign = host.vm_mut().try_new_userdata(4).unwrap();
    host.vm_mut().pin(foreign).unwrap();
    let Value::Userdata(id) = foreign else {
        unreachable!()
    };
    host.vm_mut()
        .userdata_bytes_mut(id)
        .unwrap()
        .copy_from_slice(&42_u32.to_ne_bytes());
    host.vm_mut().set_global(b"FOREIGN", foreign).unwrap();
    assert_eq!(
        api::eval(host.vm_mut(), "return DeepCopy(FOREIGN)"),
        [foreign]
    );
}

fn observed_vertices(host: &SongRuntime) -> Arc<[[f32; 4]; 4]> {
    let (len, value) = test_support::observed_vertex_sample(host);
    assert_eq!(len, 1);
    value.expect("owned vertex color observation")
}
fn write_vertices(host: &mut SongRuntime, value: f32) {
    api::eval(
        host.vm_mut(),
        &format!(
            "ACTOR:diffuseupperleft({v},{v},{v},{v}):diffuseupperright({v},{v},{v},{v}):diffuselowerleft({v},{v},{v},{v}):diffuselowerright({v},{v},{v},{v})",
            v = value
        ),
    );
}
#[test]
fn live_vertex_color_observer_preserves_owned_samples_and_queue_rules() {
    let run = |live| {
        let mut host = runtime();
        host.set_live(live);
        let actor = instantiate(&mut host, "return Def.Quad{}");
        host.vm_mut().set_global(b"ACTOR", actor).unwrap();
        host.begin_actor_playback();
        test_support::begin_pose_observation(&mut host, actor).unwrap();
        write_vertices(&mut host, 0.25);
        let first = observed_vertices(&host);
        api::eval(host.vm_mut(), "ACTOR:linear(1)");
        write_vertices(&mut host, 0.5);
        assert_eq!(
            *observed_vertices(&host),
            *first,
            "same-beat queued writes retain the earlier observation"
        );
        api::eval(
            host.vm_mut(),
            "ACTOR:stoptweening():diffusetopedge(1,0.75,0.5,0.25)",
        );
        let second = observed_vertices(&host);
        assert_eq!(second[0], [1., 0.75, 0.5, 0.25]);
        assert_eq!(
            *first, [[0.25; 4]; 4],
            "later writes cannot mutate an owned sample"
        );
        test_support::set_pose_observation_scope(&host, 1, false, None);
        write_vertices(&mut host, 0.75);
        assert_eq!(
            *observed_vertices(&host),
            *second,
            "ordinary queued commands are not update samples"
        );
        test_support::set_pose_observation_scope(&host, 1, true, None);
        write_vertices(&mut host, 1.);
        let third = observed_vertices(&host);
        assert_eq!(*third, [[1.; 4]; 4]);
        test_support::set_pose_observation_scope(&host, 1, true, Some("Tint"));
        write_vertices(&mut host, 0.);
        assert_eq!(
            *observed_vertices(&host),
            *third,
            "broadcast writes are not update samples"
        );
        [*first, *second, *third]
    };
    assert_eq!(
        run(true),
        run(false),
        "live mutation retains capture-backed observation rules"
    );
}
