//! Differential behavior checks and opt-in old/new song-Lua capture benchmarks.
use deadlib_present::actors::{
    Actor, Background, RetainedActorFrame, SizeSpec, TextAttribute, TextAttributes, TextContent,
};
use deadlib_present::dsl::{SpriteBuilder, TextBuilder};
use deadlib_render_core::{BlendMode, MeshVertex, TexturedMeshVertex};
use deadsync_notefield::song_lua_player_y_fold_actor as fold;
use deadsync_theme_simply_love::screens::gameplay::actor_capture_test_support as current;
use glam::Mat4 as Matrix4;
use std::hint::black_box;
use std::sync::Arc;

const PLAYER_ACTOR_SCRATCH_CAPACITY: usize = current::PLAYER_ACTOR_CAPACITY;

#[path = "actor_capture/baseline.rs"]
mod old;
#[allow(dead_code)]
#[path = "../../../../tests/support/perf.rs"]
mod perf;

fn sprite(index: usize) -> Actor {
    let mut builder = SpriteBuilder::solid();
    builder.xy(index as f32, -17.0);
    builder.z((index % 29) as i16 - 14);
    let mut actor = builder.build(0);
    if let Actor::Sprite {
        tint,
        glow,
        shadow_color,
        scale,
        uv_rect,
        flip_x,
        ..
    } = &mut actor
    {
        *tint = [0.125, 0.25, 0.75, 0.5];
        *glow = [0.75, 0.25, -0.0, 0.125];
        *shadow_color = [0.25, 0.5, 0.125, 0.75];
        *scale = [1.25, 2.5];
        *uv_rect = Some([0.125, 0.25, 0.5, 0.75]);
        *flip_x = true;
    }
    actor
}

fn text() -> Actor {
    let mut builder = TextBuilder::new();
    builder.xy(120.0, -12.0);
    builder.settext(TextContent::Static("capture 東京"));
    let mut actor = builder.build(0);
    if let Actor::Text {
        color,
        stroke_color,
        glow,
        shadow_color,
        scale,
        attributes,
        clip,
        ..
    } = &mut actor
    {
        *color = [0.125, 0.25, 0.75, 0.5];
        *stroke_color = Some([0.25, 0.75, 0.5, 0.125]);
        *glow = [0.75, 0.25, -0.0, 0.125];
        *shadow_color = [0.25, 0.5, 0.125, 0.75];
        *scale = [2.0, 3.0];
        *clip = Some([1.0, 2.0, 300.0, 400.0]);
        *attributes = TextAttributes::Owned(vec![TextAttribute {
            start: 1,
            length: 3,
            color: [0.25; 4],
            vertex_colors: Some([[0.5; 4]; 4]),
            glow: Some([0.75; 4]),
        }]);
    }
    actor
}

fn frame(children: Vec<Actor>) -> Actor {
    Actor::Frame {
        align: [0.25, 0.75],
        offset: [120.0, 40.0],
        size: [SizeSpec::Fill, SizeSpec::Px(32.0)],
        children,
        background: Some(Background::Color([0.25; 4])),
        z: 7,
    }
}

fn retained(children: Vec<Actor>) -> Actor {
    Actor::RetainedFrame {
        align: [0.0; 2],
        offset: [0.0; 2],
        size: [SizeSpec::Fill; 2],
        frame: Arc::new(RetainedActorFrame::new(children)),
        z: 0,
        tint: [1.0; 4],
        blend: None,
        visible: true,
    }
}

fn variants() -> Vec<Actor> {
    let shared: Arc<[Actor]> = Arc::from([sprite(90), text()]);
    let mesh = vec![MeshVertex {
        pos: [1.0, 2.0],
        color: [0.25; 4],
    }];
    let textured = vec![TexturedMeshVertex::default(); 3];
    vec![
        sprite(120),
        text(),
        Actor::Mesh {
            align: [0.25; 2],
            offset: [110.0, 5.0],
            size: [SizeSpec::Px(32.0); 2],
            tint: [0.75; 4],
            vertices: Arc::from(mesh.clone()),
            visible: true,
            blend: BlendMode::Add,
            z: 12,
        },
        Actor::ReusableMesh {
            align: [0.5; 2],
            offset: [111.0, 6.0],
            size: [SizeSpec::Px(64.0); 2],
            tint: [0.5; 4],
            vertices: Arc::new(mesh),
            visible: false,
            blend: BlendMode::Alpha,
            z: -12,
        },
        Actor::TexturedMesh {
            align: [0.5; 2],
            offset: [112.0, 7.0],
            world_z: 19.0,
            size: [SizeSpec::Fill; 2],
            local_transform: Matrix4::from_scale(glam::Vec3::new(2.0, 3.0, 4.0)),
            texture: Arc::from("fixture"),
            tint: [0.25; 4],
            glow: [0.75; 4],
            vertices: Arc::from(textured.clone()),
            geom_cache_key: 23,
            uv_scale: [2.0; 2],
            uv_offset: [0.25; 2],
            uv_tex_shift: [0.5; 2],
            depth_test: true,
            visible: true,
            blend: BlendMode::Multiply,
            z: i16::MAX,
        },
        Actor::ReusableTexturedMesh {
            align: [0.25; 2],
            offset: [113.0, 8.0],
            world_z: 29.0,
            size: [SizeSpec::Px(16.0); 2],
            local_transform: Matrix4::IDENTITY,
            texture: Arc::from("reusable"),
            tint: [0.5; 4],
            glow: [0.25; 4],
            vertices: Arc::new(textured),
            geom_cache_key: 24,
            uv_scale: [3.0; 2],
            uv_offset: [0.75; 2],
            uv_tex_shift: [0.125; 2],
            depth_test: false,
            visible: false,
            blend: BlendMode::Subtract,
            z: i16::MIN,
        },
        frame(vec![sprite(101), text()]),
        Actor::SharedFrame {
            align: [0.5; 2],
            offset: [114.0, 9.0],
            size: [SizeSpec::Fill; 2],
            children: Arc::clone(&shared),
            background: Some(Background::Texture("background")),
            z: 5,
            tint: [0.75; 4],
            blend: Some(BlendMode::Multiply),
        },
        Actor::SharedTransform {
            transform: Matrix4::from_scale(glam::Vec3::splat(2.0)),
            source_view_proj: Matrix4::IDENTITY,
            children: shared,
            z: -5,
            tint: [0.25; 4],
            blend: None,
        },
        retained(vec![sprite(103), text()]),
        Actor::Camera {
            view_proj: Matrix4::from_scale(glam::Vec3::splat(0.5)),
            children: vec![sprite(104), text()],
        },
        Actor::CameraPush {
            view_proj: Matrix4::IDENTITY,
        },
        Actor::CameraPop,
        Actor::Shadow {
            len: [2.0, -3.0],
            color: [0.75; 4],
            child: Box::new(frame(vec![sprite(105), text()])),
        },
    ]
}

// Debug checks all metadata and geometry; the second fingerprint checks exact
// float bits on every changed field (including signed zero and NaN payloads).
fn live_bits(actor: &Actor, out: &mut Vec<u32>) {
    let mut floats = |values: &[f32]| out.extend(values.iter().map(|value| value.to_bits()));
    match actor {
        Actor::Sprite {
            offset,
            tint,
            glow,
            shadow_color,
            ..
        } => {
            floats(offset);
            floats(tint);
            floats(glow);
            floats(shadow_color);
        }
        Actor::Text {
            offset,
            scale,
            color,
            stroke_color,
            glow,
            shadow_color,
            ..
        } => {
            floats(offset);
            floats(scale);
            floats(color);
            if let Some(color) = stroke_color {
                floats(color);
            }
            floats(glow);
            floats(shadow_color);
        }
        Actor::Mesh { offset, tint, .. } | Actor::ReusableMesh { offset, tint, .. } => {
            floats(offset);
            floats(tint);
        }
        Actor::TexturedMesh {
            offset, tint, glow, ..
        }
        | Actor::ReusableTexturedMesh {
            offset, tint, glow, ..
        } => {
            floats(offset);
            floats(tint);
            floats(glow);
        }
        Actor::Frame {
            offset, children, ..
        } => {
            floats(offset);
            for child in children {
                live_bits(child, out);
            }
        }
        Actor::SharedFrame {
            offset,
            tint,
            children,
            ..
        } => {
            floats(offset);
            floats(tint);
            for child in children.iter() {
                live_bits(child, out);
            }
        }
        Actor::SharedTransform { tint, children, .. } => {
            floats(tint);
            for child in children.iter() {
                live_bits(child, out);
            }
        }
        Actor::RetainedFrame {
            offset,
            tint,
            frame,
            ..
        } => {
            floats(offset);
            floats(tint);
            for child in frame.children() {
                live_bits(child, out);
            }
        }
        Actor::Camera { children, .. } => {
            for child in children {
                live_bits(child, out);
            }
        }
        Actor::Shadow { color, child, .. } => {
            floats(color);
            live_bits(child, out);
        }
        Actor::CameraPush { .. } | Actor::CameraPop => {}
    }
}

fn assert_actors(actual: &[Actor], expected: &[Actor]) {
    assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
    let (mut a, mut b) = (Vec::new(), Vec::new());
    for actor in actual {
        live_bits(actor, &mut a);
    }
    for actor in expected {
        live_bits(actor, &mut b);
    }
    assert_eq!(a, b);
}

fn storage(actor: &Actor, out: &mut Vec<usize>) {
    match actor {
        Actor::Frame { children, .. } | Actor::Camera { children, .. } => {
            out.extend([children.as_ptr() as usize, children.capacity()]);
            for child in children {
                storage(child, out);
            }
        }
        Actor::Shadow { child, .. } => {
            out.push(child.as_ref() as *const Actor as usize);
            storage(child, out);
        }
        Actor::SharedFrame { children, .. } | Actor::SharedTransform { children, .. } => {
            out.push(children.as_ptr() as usize)
        }
        Actor::RetainedFrame { frame, .. } => out.push(Arc::as_ptr(frame) as usize),
        Actor::Mesh { vertices, .. } => out.push(vertices.as_ptr() as usize),
        Actor::ReusableMesh { vertices, .. } => out.push(Arc::as_ptr(vertices) as usize),
        Actor::TexturedMesh { vertices, .. } => out.push(vertices.as_ptr() as usize),
        Actor::ReusableTexturedMesh { vertices, .. } => out.push(Arc::as_ptr(vertices) as usize),
        Actor::Text {
            attributes: TextAttributes::Owned(values),
            ..
        } => out.push(values.as_ptr() as usize),
        _ => {}
    }
}

#[test]
fn fold_matches_parent_for_all_variants_nested_trees_and_float_edges() {
    let mut cases = variants();
    cases.push(frame(variants()));
    for value in [
        0.0,
        -0.0,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::from_bits(0x7fc01234),
    ] {
        let mut actor = text();
        if let Actor::Text { offset, scale, .. } = &mut actor {
            offset[0] = value;
            scale[0] = value;
        }
        cases.push(actor);
    }
    for actor in &cases {
        for pivot in [100.0, -0.0, f32::MAX, f32::INFINITY, f32::NAN] {
            for angle in [
                0.0,
                -0.0,
                f32::EPSILON,
                2.0 * f32::EPSILON,
                60.0,
                -90.0,
                180.0,
                720.0,
                f32::MAX,
                f32::NAN,
                f32::INFINITY,
            ] {
                assert_actors(
                    &[fold(actor.clone(), pivot, angle)],
                    &[old::song_lua_player_y_fold_actor(
                        actor.clone(),
                        pivot,
                        angle,
                    )],
                );
            }
        }
    }
    let Actor::Text { offset, scale, .. } = fold(text(), 100.0, 60.0) else {
        panic!()
    };
    assert!((offset[0] - 110.0).abs() < 0.0001);
    assert!((scale[0] - 1.0).abs() < 0.0001);
    assert_eq!(scale[1], 3.0);
}

#[test]
fn styling_matches_parent_for_wrappers_blends_saturation_and_nonfinite_colors() {
    let mut cases = variants();
    cases.push(frame(variants()));
    for color in [
        [0.0, -0.0, f32::INFINITY, f32::from_bits(0x7fc01234)],
        [f32::MAX; 4],
    ] {
        let mut actor = text();
        if let Actor::Text {
            color: diffuse,
            stroke_color,
            ..
        } = &mut actor
        {
            *diffuse = color;
            *stroke_color = None;
        }
        cases.push(actor);
    }
    for actor in &cases {
        for tint in [
            [1.0; 4],
            [0.5, 0.25, 0.125, 0.75],
            [0.0, -0.0, -2.0, f32::INFINITY],
            [f32::NAN; 4],
        ] {
            for blend in [
                None,
                Some(BlendMode::Alpha),
                Some(BlendMode::Add),
                Some(BlendMode::Multiply),
                Some(BlendMode::Subtract),
            ] {
                for z in [i16::MIN, -7, 0, 9, i16::MAX] {
                    assert_actors(
                        &[current::style(actor.clone(), tint, blend, z)],
                        &[old::song_lua_style_capture_actor(
                            actor.clone(),
                            tint,
                            blend,
                            z,
                        )],
                    );
                }
            }
        }
    }
    let Actor::Text {
        color,
        stroke_color,
        z,
        ..
    } = current::style(text(), [0.5; 4], Some(BlendMode::Add), 7)
    else {
        panic!()
    };
    assert_eq!(color, [0.0625, 0.125, 0.375, 0.25]);
    assert_eq!(stroke_color, Some([0.125, 0.375, 0.25, 0.0625]));
    assert_eq!(z, 7);
}

#[test]
fn folding_and_styling_keep_owned_storage_shared_resources_and_zero_churn() {
    let mut actor = Some(frame(variants()));
    let mut before = Vec::new();
    storage(actor.as_ref().unwrap(), &mut before);
    perf::assert_no_churn(|| actor = Some(fold(actor.take().unwrap(), 100.0, 60.0)));
    perf::assert_no_churn(|| {
        actor = Some(current::style(
            actor.take().unwrap(),
            [0.5; 4],
            Some(BlendMode::Add),
            7,
        ))
    });
    let mut after = Vec::new();
    storage(actor.as_ref().unwrap(), &mut after);
    assert_eq!(before, after);
}

fn expansion_fixture(count: usize, tail: usize) -> Vec<Actor> {
    let mut out = vec![sprite(700), retained((0..count).map(sprite).collect())];
    out.extend((0..tail).map(|i| sprite(1000 + i)));
    out
}

#[test]
fn retained_expansion_preserves_order_nested_empty_and_nonidentity_wrappers() {
    let mut cases = vec![
        vec![],
        vec![retained(vec![])],
        expansion_fixture(1, 5),
        expansion_fixture(64, 90),
    ];
    cases.push(vec![
        retained(vec![retained(vec![]), retained(vec![sprite(1), sprite(2)])]),
        retained(vec![]),
        sprite(3),
    ]);
    for field in 0..8 {
        let mut actor = retained(vec![sprite(42)]);
        if let Actor::RetainedFrame {
            align,
            offset,
            size,
            z,
            tint,
            blend,
            visible,
            ..
        } = &mut actor
        {
            match field {
                0 => align[0] = 0.5,
                1 => offset[1] = 1.0,
                2 => size[0] = SizeSpec::Px(1.0),
                3 => *z = 1,
                4 => tint[0] = 0.5,
                5 => *blend = Some(BlendMode::Alpha),
                6 => *visible = false,
                _ => offset[0] = f32::NAN,
            }
        }
        cases.push(vec![actor]);
    }
    for source in cases {
        let snapshot = format!("{source:?}");
        for spare in [0, 1024] {
            let (mut actual, mut expected) = (source.clone(), source.clone());
            actual.reserve(spare);
            expected.reserve(spare);
            current::expand(&mut actual);
            old::song_lua_proxy_expand_retained(&mut expected);
            assert_actors(&actual, &expected);
            assert_eq!(format!("{source:?}"), snapshot);
            // Repeated expansion is idempotent, even for nonidentity wrappers.
            current::expand(&mut actual);
            assert_actors(&actual, &expected);
        }
    }
    let mut actual = expansion_fixture(3, 2);
    current::expand(&mut actual);
    let ids: Vec<_> = actual
        .iter()
        .map(|a| match a {
            Actor::Sprite { offset, .. } => offset[0],
            _ => panic!(),
        })
        .collect();
    assert_eq!(ids, [700.0, 0.0, 1.0, 2.0, 1000.0, 1001.0]);
}

#[test]
fn retained_expansion_reuses_capacity_without_heap_churn() {
    let mut source = expansion_fixture(128, 256);
    source.reserve(128);
    let ptr = source.as_ptr();
    // Keep the immutable source alive so this check isolates expansion churn.
    let keep = source[1].clone();
    perf::assert_no_churn(|| current::expand(&mut source));
    assert_eq!(source.as_ptr(), ptr);
    assert_eq!(source.len(), 385);
    drop(keep);
}

#[test]
fn proxy_normalization_matches_parent_including_camera_barriers_and_stable_ties() {
    for count in [0, 1, 7, 8, 9, 64, 256] {
        let mut source = expansion_fixture(count, 37);
        source.insert(
            0,
            Actor::CameraPush {
                view_proj: Matrix4::IDENTITY,
            },
        );
        source.push(Actor::CameraPop);
        source.push(frame(vec![retained(vec![sprite(8), sprite(2)]), text()]));
        source.push(Actor::SharedFrame {
            align: [0.0; 2],
            offset: [0.0; 2],
            size: [SizeSpec::Fill; 2],
            children: Arc::from([retained(vec![sprite(4), sprite(3)])]),
            background: None,
            z: 4,
            tint: [1.0; 4],
            blend: None,
        });
        let mut actual = source.clone();
        let mut expected = source.clone();
        current::normalize(&mut actual);
        old::song_lua_proxy_local_children_in_place(&mut expected);
        assert_actors(&actual, &expected);
    }
    let mut source = vec![
        sprite(2),
        sprite(1),
        sprite(30),
        Actor::CameraPop,
        sprite(0),
    ];
    current::normalize(&mut source);
    let ids: Vec<_> = source
        .iter()
        .map(|a| match a {
            Actor::Sprite { offset, z, .. } => {
                assert_eq!(*z, 0);
                offset[0]
            }
            Actor::CameraPop => -1.0,
            _ => panic!(),
        })
        .collect();
    assert_eq!(ids, [1.0, 30.0, 2.0, -1.0, 0.0]);
}

fn bench_pair<T>(
    name: &str,
    iterations: usize,
    units: usize,
    mut before: impl FnMut() -> T,
    mut after: impl FnMut() -> T,
) {
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        perf::measure_sampled(&format!("{name}/new"), iterations, units, &mut after);
        perf::measure_sampled(&format!("{name}/old"), iterations, units, &mut before);
    } else {
        perf::measure_sampled(&format!("{name}/old"), iterations, units, &mut before);
        perf::measure_sampled(&format!("{name}/new"), iterations, units, &mut after);
    }
}

#[test]
#[ignore = "manual release benchmark; run alone with --nocapture --test-threads=1"]
fn actor_capture_bench() {
    eprintln!(
        "Actor bytes={}, proxy capacity={PLAYER_ACTOR_SCRATCH_CAPACITY}",
        std::mem::size_of::<Actor>()
    );
    for count in [1, 32, 256, 1024] {
        let source = frame((0..count).map(sprite).collect());
        bench_pair(
            &format!("fold_{count}"),
            256,
            count,
            || {
                old::song_lua_player_y_fold_actor(
                    black_box(source.clone()),
                    black_box(100.0),
                    black_box(60.0),
                )
            },
            || fold(black_box(source.clone()), black_box(100.0), black_box(60.0)),
        );
        bench_pair(
            &format!("style_{count}"),
            256,
            count,
            || {
                old::song_lua_style_capture_actor(
                    black_box(source.clone()),
                    black_box([0.5; 4]),
                    black_box(Some(BlendMode::Add)),
                    black_box(7),
                )
            },
            || {
                current::style(
                    black_box(source.clone()),
                    black_box([0.5; 4]),
                    black_box(Some(BlendMode::Add)),
                    black_box(7),
                )
            },
        );
    }
    let mixed = frame(variants());
    bench_pair(
        "fold_mixed",
        512,
        14,
        || old::song_lua_player_y_fold_actor(black_box(mixed.clone()), 100.0, 60.0),
        || fold(black_box(mixed.clone()), 100.0, 60.0),
    );
    bench_pair(
        "style_mixed",
        512,
        14,
        || old::song_lua_style_capture_actor(black_box(mixed.clone()), [0.5; 4], None, 7),
        || current::style(black_box(mixed.clone()), [0.5; 4], None, 7),
    );
    bench_pair(
        "fold_identity",
        512,
        14,
        || old::song_lua_player_y_fold_actor(black_box(mixed.clone()), 100.0, black_box(0.0)),
        || fold(black_box(mixed.clone()), 100.0, black_box(0.0)),
    );
    for (count, tail) in [
        (0, 0),
        (1, 8),
        (32, 128),
        (128, 8),
        (128, 256),
        (512, 8),
        (512, 1024),
    ] {
        let source = expansion_fixture(count, tail);
        for spare in [false, true] {
            let setup = || {
                let mut out = Vec::with_capacity(source.len() + if spare { count } else { 0 });
                out.extend_from_slice(black_box(&source));
                out
            };
            bench_pair(
                &format!(
                    "expand_{count}_{tail}_{}",
                    if spare { "spare" } else { "grow" }
                ),
                128,
                count + tail + 1,
                || {
                    let mut out = setup();
                    old::song_lua_proxy_expand_retained(black_box(&mut out));
                    out
                },
                || {
                    let mut out = setup();
                    current::expand(black_box(&mut out));
                    out
                },
            );
        }
    }
    let source = (0..256).map(sprite).collect::<Vec<_>>();
    bench_pair(
        "fold_flat_256",
        256,
        256,
        || {
            source
                .clone()
                .into_iter()
                .map(|actor| {
                    old::song_lua_player_y_fold_actor(
                        black_box(actor),
                        black_box(100.0),
                        black_box(60.0),
                    )
                })
                .collect::<Vec<_>>()
        },
        || {
            source
                .clone()
                .into_iter()
                .map(|actor| fold(black_box(actor), black_box(100.0), black_box(60.0)))
                .collect::<Vec<_>>()
        },
    );
    bench_pair(
        "style_flat_256",
        256,
        256,
        || {
            source
                .clone()
                .into_iter()
                .map(|actor| {
                    old::song_lua_style_capture_actor(
                        black_box(actor),
                        black_box([0.5; 4]),
                        black_box(Some(BlendMode::Add)),
                        black_box(7),
                    )
                })
                .collect::<Vec<_>>()
        },
        || {
            source
                .clone()
                .into_iter()
                .map(|actor| {
                    current::style(
                        black_box(actor),
                        black_box([0.5; 4]),
                        black_box(Some(BlendMode::Add)),
                        black_box(7),
                    )
                })
                .collect::<Vec<_>>()
        },
    );
    bench_pair(
        "expand_no_retained",
        256,
        256,
        || {
            let mut out = source.clone();
            old::song_lua_proxy_expand_retained(black_box(&mut out));
            out
        },
        || {
            let mut out = source.clone();
            current::expand(black_box(&mut out));
            out
        },
    );
    let source = expansion_fixture(128, 256);
    bench_pair(
        "proxy_capture_385",
        128,
        385,
        || {
            let mut out = source.clone();
            old::song_lua_proxy_local_children_in_place(&mut out);
            old::song_lua_style_capture_actor(
                old::song_lua_player_y_fold_actor(frame(out), 100.0, 60.0),
                [0.5; 4],
                Some(BlendMode::Add),
                7,
            )
        },
        || {
            let mut out = source.clone();
            current::normalize(&mut out);
            current::style(
                fold(frame(out), 100.0, 60.0),
                [0.5; 4],
                Some(BlendMode::Add),
                7,
            )
        },
    );
}
