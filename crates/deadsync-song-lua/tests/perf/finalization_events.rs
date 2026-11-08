use super::*;
use crate::{
    SongLuaJudgmentTexture, SongLuaOverlayCompileActor, SongLuaSpriteTexture, SongLuaTrackedActor,
};
use std::path::PathBuf;

#[path = "finalization_events_before.rs"]
mod before;

type Compiled = DefaultCompiledSongLua<(), ()>;
type Overlay = SongLuaOverlayCompileActor<SongLuaOverlayKind<(), (), TextAttribute>>;

fn sprite_texture(index: usize, second: f32) -> SongLuaSpriteTexture {
    SongLuaSpriteTexture {
        second,
        path: PathBuf::from(format!("textures/sprite-{index}.png")),
        key: format!("sprite-{index}").into(),
        states: [].into(),
        animation_epoch: Some(index as f32),
        frame_sheet: Some((2, 6)),
    }
}

fn judgment_texture(index: usize, second: f32) -> SongLuaJudgmentTexture {
    SongLuaJudgmentTexture {
        second,
        path: PathBuf::from(format!("textures/judgment-{}.png", index % 5)),
        key: format!("judgment-{index}").into(),
        frame_size: [index as f32, 64.0],
        frame_cols: 2,
        frame_rows: 6,
    }
}

fn overlay(table: &Table, sprite: bool) -> Overlay {
    Overlay {
        table: table.clone(),
        actor: SongLuaOverlayActor {
            kind: if sprite {
                SongLuaOverlayKind::Sprite {
                    texture_path: "initial.png".into(),
                    texture_key: "initial".into(),
                    states: [].into(),
                    textures: vec![sprite_texture(999, -1.0)].into(),
                }
            } else {
                SongLuaOverlayKind::ActorFrame
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        },
        message_sounds: Vec::new(),
    }
}

fn tracked(lua: &Lua, targets: &[TrackedCompileActorTarget]) -> Vec<SongLuaTrackedActor> {
    targets
        .iter()
        .enumerate()
        .map(|(i, &target)| {
            let mut actor = crate::SongLuaCapturedActor::default();
            actor.initial_state.x = i as f32;
            actor.manual_hud_draw = i % 2 == 0;
            actor.judgment.textures.push(judgment_texture(999, -1.0));
            SongLuaTrackedActor {
                table: lua.create_table().unwrap(),
                actor,
                target,
            }
        })
        .collect()
}

fn sprite_snapshot(overlays: &[Overlay]) -> Vec<String> {
    overlays
        .iter()
        .map(|overlay| {
            let bits: Vec<_> = match &overlay.actor.kind {
                SongLuaOverlayKind::Sprite { textures, .. } => {
                    textures.iter().map(|t| t.second.to_bits()).collect()
                }
                _ => Vec::new(),
            };
            format!("{:?}/{bits:?}", overlay.actor)
        })
        .collect()
}

#[test]
fn texture_finalization_preserves_aliases_order_dedup_and_player_overwrites() {
    let lua = Lua::new();
    let a = lua.create_table().unwrap();
    let b = lua.create_table().unwrap();
    let times = [
        1.0,
        -0.0,
        0.0,
        1.0,
        f32::from_bits(0x7fc0_0017),
        f32::NEG_INFINITY,
        f32::from_bits(0xffc0_0023),
    ];
    for empty in [false, true] {
        let events: Vec<_> = if empty {
            Vec::new()
        } else {
            times
                .iter()
                .enumerate()
                .flat_map(|(i, &second)| {
                    [
                        (a.to_pointer() as usize, sprite_texture(i, second)),
                        (usize::MAX, sprite_texture(i + 100, second)),
                    ]
                })
                .collect()
        };
        let make = || {
            vec![
                overlay(&a, true),
                overlay(&b, true),
                overlay(&a, false),
                overlay(&a, true),
            ]
        };
        let mut old = make();
        let mut new = make();
        before::finalize_sprite_textures(&mut old, events.clone());
        super::finalize_sprite_textures(&mut new, events);
        assert_eq!(sprite_snapshot(&old), sprite_snapshot(&new));
        if !empty {
            let SongLuaOverlayKind::Sprite { textures: left, .. } = &new[0].actor.kind else {
                unreachable!()
            };
            let SongLuaOverlayKind::Sprite {
                textures: right, ..
            } = &new[3].actor.kind
            else {
                unreachable!()
            };
            assert!(
                !Arc::ptr_eq(left, right),
                "aliased actors retain independent lists"
            );
        }
    }
    use TrackedCompileActorTarget::*;
    for targets in [
        vec![Player(0), PlayerJudgment(0), PlayerJudgment(1)],
        vec![PlayerJudgment(0), Player(0)],
        vec![
            PlayerJudgment(0),
            Player(0),
            PlayerJudgment(0),
            PlayerJudgment(0),
        ],
        vec![PlayerCombo(1), SongForeground, ScreenLayer(3)],
    ] {
        for empty in [false, true] {
            let events: Vec<_> = if empty {
                Vec::new()
            } else {
                times
                    .iter()
                    .enumerate()
                    .flat_map(|(i, &second)| {
                        [
                            (0, judgment_texture(i, second)),
                            (1, judgment_texture(i, second)),
                            (usize::MAX, judgment_texture(i, second)),
                            (0, judgment_texture(i, second)),
                        ]
                    })
                    .collect()
            };
            let mut old = Compiled::default();
            let mut new = Compiled::default();
            before::finalize_tracked_actors(&mut old, tracked(&lua, &targets), events.clone());
            super::finalize_tracked_actors(&mut new, tracked(&lua, &targets), events);
            assert_eq!(format!("{old:?}"), format!("{new:?}"));
            for (a, b) in old.player_actors.iter().zip(&new.player_actors) {
                assert_eq!(
                    a.judgment
                        .textures
                        .iter()
                        .map(|t| t.second.to_bits())
                        .collect::<Vec<_>>(),
                    b.judgment
                        .textures
                        .iter()
                        .map(|t| t.second.to_bits())
                        .collect::<Vec<_>>()
                );
            }
        }
    }
}

fn message_snapshot(messages: &[SongLuaMessageEvent]) -> Vec<(u32, &str, bool)> {
    messages
        .iter()
        .map(|e| (e.beat.to_bits(), e.message.as_str(), e.persists))
        .collect()
}

#[test]
fn message_matching_matches_frozen_window_ties_and_float_edges() {
    let mut random = 0x4712_dab3_u32;
    let mut next = || {
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        random
    };
    let beats = [
        f32::NEG_INFINITY,
        -1.0,
        -0.0,
        0.0,
        f32::EPSILON,
        1.0,
        f32::INFINITY,
        f32::from_bits(0x7fc0_1234),
        f32::from_bits(0xffc0_5678),
    ];
    let names = [
        "Pulse",
        "Other",
        "",
        "__songlua_overlay_fn_action_7",
        "missing",
    ];
    for count in [0, 1, 8, 31, 32, 64, 257] {
        for runtime_count in [0, 1, 15, 16, 32, 129] {
            for prefix in [0, 3] {
                for _ in 0..8 {
                    let mut old: Vec<_> = (0..count + prefix)
                        .map(|_| SongLuaMessageEvent {
                            beat: beats[next() as usize % beats.len()],
                            message: names[next() as usize % 4].to_owned(),
                            persists: next() % 2 == 0,
                        })
                        .collect();
                    let runtime: Vec<_> = (0..runtime_count)
                        .map(|_| {
                            (
                                beats[next() as usize % beats.len()],
                                names[next() as usize % names.len()].to_owned(),
                                next() % 2 == 0,
                            )
                        })
                        .collect();
                    let mut new = old.clone();
                    before::merge_runtime_messages(&mut old, prefix, &runtime);
                    super::merge_runtime_messages(&mut new, prefix, &runtime);
                    assert_eq!(
                        message_snapshot(&old),
                        message_snapshot(&new),
                        "count={count}, runtime={runtime_count}, prefix={prefix}"
                    );
                }
            }
        }
    }
}

enum Work {
    Sprites(Vec<Overlay>, Vec<(usize, SongLuaSpriteTexture)>),
    Tracked(
        Compiled,
        Vec<SongLuaTrackedActor>,
        Vec<(usize, SongLuaJudgmentTexture)>,
    ),
    Messages(Vec<SongLuaMessageEvent>, Vec<(f32, String, bool)>),
}

impl Work {
    fn run(&mut self, current: bool) {
        match self {
            Self::Sprites(overlays, events) => {
                let events = std::mem::take(events);
                if current {
                    super::finalize_sprite_textures(overlays, events)
                } else {
                    before::finalize_sprite_textures(overlays, events)
                }
            }
            Self::Tracked(out, actors, events) => {
                let actors = std::mem::take(actors);
                let events = std::mem::take(events);
                if current {
                    super::finalize_tracked_actors(out, actors, events)
                } else {
                    before::finalize_tracked_actors(out, actors, events)
                }
            }
            Self::Messages(messages, runtime) => {
                if current {
                    super::merge_runtime_messages(messages, 0, runtime)
                } else {
                    before::merge_runtime_messages(messages, 0, runtime)
                }
            }
        }
    }
}

fn prepare(lua: &Lua, group: usize, actors: usize, events: usize, names: usize) -> Work {
    match group {
        0 => {
            let tables: Vec<_> = (0..actors).map(|_| lua.create_table().unwrap()).collect();
            let overlays = tables
                .iter()
                .enumerate()
                .map(|(i, _)| overlay(&tables[if names == 1 { 0 } else { i }], true))
                .collect();
            let events = (0..events)
                .map(|i| {
                    (
                        if names == 2 {
                            usize::MAX
                        } else {
                            tables[if names == 1 { 0 } else { i % actors }].to_pointer() as usize
                        },
                        sprite_texture(i, ((events - i) / 2) as f32),
                    )
                })
                .collect();
            Work::Sprites(overlays, events)
        }
        1 => {
            let targets: Vec<_> = (0..actors)
                .map(|i| TrackedCompileActorTarget::PlayerJudgment(i % crate::LUA_PLAYERS))
                .collect();
            let events = (0..events)
                .map(|i| {
                    (
                        i % crate::LUA_PLAYERS,
                        judgment_texture(i, ((events - i) / 2) as f32),
                    )
                })
                .collect();
            Work::Tracked(Compiled::default(), tracked(lua, &targets), events)
        }
        _ => {
            let messages = (0..actors)
                .map(|i| SongLuaMessageEvent {
                    beat: (i / 2) as f32,
                    message: format!("message-{}", i % names),
                    persists: i % 2 == 0,
                })
                .collect();
            let runtime = (0..events)
                .map(|i| {
                    (
                        actors as f32 + i as f32,
                        format!("message-{}", if group == 3 { names + i } else { i % names }),
                        true,
                    )
                })
                .collect();
            Work::Messages(messages, runtime)
        }
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_finalization_events() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    let _affinity = pin_cpu2();
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for (name, group, actors, events, names) in [
        ("sprites_empty128", 0, 128, 0, 0),
        ("sprites_one8", 0, 1, 8, 0),
        ("sprites_distinct32_512", 0, 32, 512, 0),
        ("sprites_alias64_512", 0, 64, 512, 1),
        ("sprites_orphan128_512", 0, 128, 512, 2),
        ("judgment_empty2", 1, 2, 0, 0),
        ("judgment_one128", 1, 1, 128, 0),
        ("judgment_two512", 1, 2, 512, 0),
        ("judgment_alias8_512", 1, 8, 512, 0),
        ("messages_empty512", 2, 512, 0, 8),
        ("messages_small8_8", 2, 8, 8, 4),
        ("messages_edge32_16", 2, 32, 16, 4),
        ("messages_32_32", 2, 32, 32, 8),
        ("messages_64_64", 2, 64, 64, 8),
        ("messages_one512_512", 2, 512, 512, 1),
        ("messages_many512_512", 2, 512, 512, 64),
        ("messages_missing512_512", 3, 512, 512, 64),
    ] {
        for current in order {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let lua = Lua::new();
                let mut work: Vec<_> = (0..16)
                    .map(|_| prepare(&lua, group, actors, events, names))
                    .collect();
                lua.gc_stop();
                let start = Instant::now();
                let begin = cycles();
                let (_, churn) = allocations::measure(|| {
                    for item in &mut work {
                        item.run(current);
                        black_box(&*item);
                    }
                });
                let used = cycles() - begin;
                let nanos = start.elapsed().as_nanos();
                if sample > 0 {
                    samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":16,
                        "allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
                }
            }
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload":name,"variant":if current{"after"}else{"before"},
                "unit":"finalizations","baseline_commit":"87e58ae89","samples":samples})
            );
        }
    }
}
