use super::*;
use std::collections::HashMap;
use std::hint::black_box;

#[path = "frame_sampling_baseline.rs"]
mod baseline;

const MODS: &[&str] = &[
    "drunk",
    "dizzy",
    "confusion",
    "flip",
    "invert",
    "tornado",
    "tipsy",
    "tiny",
    "bumpy",
    "beat",
    "boost",
    "brake",
    "wave",
    "expand",
    "boomerang",
    "hidden",
    "sudden",
    "stealth",
    "blink",
    "randomvanish",
    "reverse",
    "split",
    "alternate",
    "cross",
    "centered",
    "dark",
    "blind",
    "cover",
    "movex1",
    "movey1",
    "tiny1",
    "bumpy1",
];

fn options(lua: &Lua, count: usize) -> [Table; LUA_PLAYERS] {
    std::array::from_fn(|player| {
        let owner = lua.create_table().unwrap();
        let state = lua.create_table().unwrap();
        let speeds = lua.create_table().unwrap();
        for (index, key) in MODS.iter().take(count).enumerate() {
            state
                .raw_set(*key, (index + player) as f32 / 100.0)
                .unwrap();
            speeds.raw_set(*key, 0.5).unwrap();
        }
        owner
            .raw_set("__songlua_player_option_state", state)
            .unwrap();
        owner
            .raw_set("__songlua_player_option_speeds", speeds)
            .unwrap();
        owner.raw_set("__songlua_speedmod_active", "xmod").unwrap();
        owner.raw_set("__songlua_speedmod_xmod", 2.0).unwrap();
        owner.raw_set("__songlua_speedmod_cmod", 600.0).unwrap();
        owner.raw_set("__songlua_speedmod_mmod", 500.0).unwrap();
        owner
    })
}

fn bits(state: &impl ModState) -> Vec<(String, u32)> {
    state
        .entries()
        .map(|(key, value)| (key.into(), value.to_bits()))
        .collect()
}

#[test]
fn compact_snapshots_preserve_coercion_order_speed_precedence_and_live_changes() {
    let lua = Lua::new();
    let owners = options(&lua, MODS.len());
    let owner = &owners[0];
    let state = owner
        .raw_get::<Table>("__songlua_player_option_state")
        .unwrap();
    state.raw_set("xmod", 9.0).unwrap();
    state.raw_set(1, 13.0).unwrap();
    state.raw_set("1", 17.0).unwrap();
    state.raw_set("unicode_é", true).unwrap();
    state.raw_set("false", false).unwrap();
    state.raw_set("number_string", " 3.5 ").unwrap();
    state
        .raw_set("ignored", lua.create_table().unwrap())
        .unwrap();
    state.raw_set("nan", f32::NAN).unwrap();
    state.raw_set("negative_zero", -0.0_f32).unwrap();
    state.raw_set("infinite", f32::INFINITY).unwrap();
    let mut scratch = ModSnapshotScratch::default();
    for active in [
        None,
        Some("xmod"),
        Some("cmod"),
        Some("mmod"),
        Some("none"),
        Some("xmod\0"),
    ] {
        owner.raw_set("__songlua_speedmod_active", active).unwrap();
        let old = player_option_sample(owner).unwrap();
        let new = scratch.sample(owner).unwrap();
        assert_eq!(bits(&new), bits(&old));
        for (key, value) in old {
            assert_eq!(new.get(&key).unwrap().to_bits(), value.to_bits());
        }
        assert!(new.get("missing").is_none());
    }
    let prior = scratch.sample(owner).unwrap();
    state.raw_set("drunk", 8.0).unwrap();
    state.raw_set("tipsy", Value::Nil).unwrap();
    let next = scratch.sample(owner).unwrap();
    assert_ne!(prior.get("drunk"), next.get("drunk"));
    assert!(prior.get("tipsy").is_some() && next.get("tipsy").is_none());
    assert_eq!(bits(&next), bits(&player_option_sample(owner).unwrap()));
    assert_eq!(
        bits(&scratch.speeds(&lua, owner).unwrap()),
        bits(&current_update_mod_speeds(&owners).unwrap()[0])
    );
    for (key, _) in &prior.0 {
        if let Some((other, _)) = next.0.iter().find(|(other, _)| other == key) {
            assert!(Rc::ptr_eq(key, other));
        }
    }
}

#[test]
fn compact_snapshots_preserve_conversion_errors_and_raw_access() {
    let lua = Lua::new();
    for invalid in [
        Value::Boolean(true),
        Value::String(lua.create_string([0xff]).unwrap()),
    ] {
        let owner = lua.create_table().unwrap();
        let state = lua.create_table().unwrap();
        state.raw_set(invalid.clone(), "not a number").unwrap();
        owner
            .raw_set("__songlua_player_option_state", state)
            .unwrap();
        assert_eq!(
            ModSnapshotScratch::default().sample(&owner).err(),
            player_option_sample(&owner).err()
        );
        owner
            .raw_set("__songlua_player_option_state", Value::Nil)
            .unwrap();
        owner.raw_set("__songlua_speedmod_active", invalid).unwrap();
        assert_eq!(
            ModSnapshotScratch::default().sample(&owner).err(),
            player_option_sample(&owner).err()
        );
    }
    let owners = options(&lua, 0);
    let mt = lua.load("return {__index=function(_, key) if key == '__songlua_speedmod_cmod' then return 450 end end, __pairs=function() error('raw pairs required') end}").eval::<Table>().unwrap();
    owners[0].set_metatable(Some(mt)).unwrap();
    owners[0]
        .raw_set("__songlua_speedmod_active", "cmod")
        .unwrap();
    owners[0]
        .raw_set("__songlua_speedmod_cmod", Value::Nil)
        .unwrap();
    let old = player_option_sample(&owners[0]).unwrap();
    let new = ModSnapshotScratch::default().sample(&owners[0]).unwrap();
    assert_eq!(bits(&old), bits(&new));
    assert_eq!(new.get("cmod"), Some(&450.0));
}

fn old_samples(tables: &[Table; LUA_PLAYERS], count: usize) {
    for _ in 0..count {
        black_box(current_update_mod_states(tables).unwrap());
        black_box(current_update_mod_speeds(tables).unwrap());
    }
}

fn new_samples(
    lua: &Lua,
    tables: &[Table; LUA_PLAYERS],
    count: usize,
    scratch: &mut ModSnapshotScratch,
) {
    for _ in 0..count {
        black_box(scratch.sample(&tables[0]).unwrap());
        black_box(scratch.sample(&tables[1]).unwrap());
        black_box(scratch.player_speeds(lua, tables).unwrap());
    }
}

#[test]
fn compact_snapshots_reduce_complete_sampling_allocation_churn() {
    let lua = Lua::new();
    let owners = options(&lua, MODS.len());
    lua.gc_stop();
    let mut scratch = ModSnapshotScratch::default();
    new_samples(&lua, &owners, 1, &mut scratch);
    crate::perf::assert_reduced_churn(
        || old_samples(&owners, 64),
        || new_samples(&lua, &owners, 64, &mut scratch),
    );
    // Two option and two speed snapshots need four exact-size allocations;
    // names, hash storage and collection scratch remain reusable.
    crate::perf::assert_churn_budget(
        4,
        4 * (MODS.len() + 1) * std::mem::size_of::<(Rc<str>, f32)>(),
        || new_samples(&lua, &owners, 1, &mut scratch),
    );
}

fn track_tick<S: std::hash::BuildHasher>(
    tracks: &mut Vec<SongLuaOverlayUpdateTrack>,
    indices: &mut HashMap<(usize, SongLuaOverlayUpdateTarget), usize, S>,
    actors: usize,
    changed: bool,
) {
    for actor in 0..actors {
        for target in [
            SongLuaOverlayUpdateTarget::X,
            SongLuaOverlayUpdateTarget::Y,
            SongLuaOverlayUpdateTarget::ZoomX,
            SongLuaOverlayUpdateTarget::ZoomY,
        ] {
            push_update_overlay_value(
                tracks,
                indices,
                actor,
                target,
                0.0,
                SongLuaOverlayUpdateValue::F32(1.0),
                1.0,
                SongLuaOverlayUpdateValue::F32(if changed { 2.0 } else { 1.0 }),
            );
        }
    }
}

#[test]
fn fast_track_lookup_preserves_first_writes_updates_and_has_no_warm_churn() {
    let (mut old_tracks, mut new_tracks) = (vec![], vec![]);
    let (mut old, mut new) = (HashMap::new(), FxHashMap::default());
    for actors in [32, 128, 512] {
        for changed in [false, true, false] {
            track_tick(&mut old_tracks, &mut old, actors, changed);
            track_tick(&mut new_tracks, &mut new, actors, changed);
            assert_eq!(old_tracks, new_tracks);
            assert_eq!(old.len(), new.len());
            for (key, index) in &old {
                assert_eq!(new.get(key), Some(index));
            }
        }
    }
    crate::perf::assert_no_churn(|| track_tick(&mut new_tracks, &mut new, 512, false));
}

fn collect_frames(samples: usize, reserved: bool) -> usize {
    let init = |first| {
        if reserved {
            frame_buffer(first, samples)
        } else {
            vec![first]
        }
    };
    // Match all six production buffers and their element sizes.
    let mut beats = init(0.0_f32);
    let mut seconds = init(0.0_f32);
    let mut players = if reserved {
        frame_buffer(
            [SongLuaPerframePlayerState::default(); LUA_PLAYERS],
            samples,
        )
    } else {
        vec![[SongLuaPerframePlayerState::default(); LUA_PLAYERS]]
    };
    let mut mods = if reserved {
        frame_buffer([ModSnapshot::default(), ModSnapshot::default()], samples)
    } else {
        vec![[ModSnapshot::default(), ModSnapshot::default()]]
    };
    let mut speeds = if reserved {
        frame_buffer([ModSnapshot::default(), ModSnapshot::default()], samples)
    } else {
        vec![[ModSnapshot::default(), ModSnapshot::default()]]
    };
    let mut columns: Vec<Vec<crate::SongLuaColumnTransformSample>> = if reserved {
        frame_buffer(vec![], samples)
    } else {
        vec![vec![]]
    };
    for frame in 1..samples {
        beats.push(black_box(frame as f32));
        seconds.push(black_box(frame as f32 / 60.0));
        players.push(black_box(
            [SongLuaPerframePlayerState::default(); LUA_PLAYERS],
        ));
        mods.push(black_box([ModSnapshot::default(), ModSnapshot::default()]));
        speeds.push(black_box([ModSnapshot::default(), ModSnapshot::default()]));
        columns.push(black_box(vec![]));
    }
    assert_eq!(beats.len(), samples);
    black_box((beats, seconds, players, mods, speeds, columns));
    samples
}

#[test]
fn frame_buffers_keep_order_without_growth_for_reference_replay_lengths() {
    for samples in [1, 2, 61, 601, 4097, 8193] {
        let mut frames = frame_buffer(0usize, samples);
        let capacity = frames.capacity();
        crate::perf::assert_no_churn(|| frames.extend(1..samples));
        assert_eq!(capacity, frames.capacity());
        assert!(frames.iter().copied().eq(0..samples));
    }
}

#[test]
fn reserved_frame_buffers_reduce_reallocation_bytes() {
    // Both versions make six initial allocations. The budget rules out all
    // subsequent growth and bounds bytes at the exact sample count.
    const COUNT: usize = 4097;
    let bytes = COUNT
        * (8 + std::mem::size_of::<[SongLuaPerframePlayerState; LUA_PLAYERS]>()
            + 2 * std::mem::size_of::<[ModSnapshot; LUA_PLAYERS]>()
            + std::mem::size_of::<Vec<crate::SongLuaColumnTransformSample>>());
    crate::perf::assert_churn_budget(6, bytes, || {
        collect_frames(COUNT, true);
    });
}

struct CompileFixture {
    lua: Lua,
    context: SongLuaCompileContext,
    root: Value,
    overlays: Vec<SongLuaOverlayCompileActor<()>>,
    messages: Vec<SongLuaMessageEvent>,
}

impl CompileFixture {
    fn new(actors: usize, seconds: f32) -> Self {
        let lua = Lua::new();
        let mut context = SongLuaCompileContext::new("", "Frame sampling");
        context.music_length_seconds = seconds;
        context.song_timing_bpms = vec![(0.0, 120.0), (0.5, 180.0), (2.0, 90.0)];
        context.song_music_rate = 1.25;
        let runtime = crate::create_song_runtime_table(&lua, &context).unwrap();
        lua.globals()
            .raw_set(crate::SONG_LUA_RUNTIME_KEY, runtime)
            .unwrap();
        let owners = options(&lua, MODS.len());
        for (key, owner) in SONG_LUA_PLAYER_OPTIONS_KEYS.iter().zip(&owners) {
            lua.globals().raw_set(*key, owner).unwrap();
        }
        let overlays: Vec<_> = (0..actors)
            .map(|index| {
                let table = lua.create_table().unwrap();
                reset_actor_capture(&lua, &table).unwrap();
                SongLuaOverlayCompileActor {
                    table,
                    actor: crate::SongLuaOverlayActor {
                        kind: (),
                        name: Some(format!("actor{index}")),
                        parent_index: None,
                        initial_state: SongLuaOverlayState::default(),
                        message_commands: vec![crate::SongLuaOverlayMessageCommand {
                            frame_advance: 0.0,
                            message: "Pulse".into(),
                            aux: None,
                            blocks: vec![crate::SongLuaOverlayCommandBlock {
                                progress: None,
                                queued: false,
                                start: 0.0,
                                duration: 0.1,
                                easing: Some("linear".into()),
                                opt1: None,
                                opt2: None,
                                delta: crate::SongLuaOverlayStateDelta {
                                    y: Some(8.0),
                                    ..Default::default()
                                },
                            }],
                        }],
                    },
                    message_sounds: vec![],
                }
            })
            .collect();
        let tables: Vec<_> = overlays
            .iter()
            .map(|overlay| overlay.table.clone())
            .collect();
        let root = lua.create_table().unwrap();
        let update = lua
            .create_function(move |lua, _: mlua::MultiValue| {
                let (beat, _) = compile_song_runtime_values(lua)?;
                for table in &tables {
                    crate::lua_util::capture_block_set_f32(lua, table, "x", beat)?;
                    crate::lua_util::capture_block_set_f32(lua, table, "zoom_x", 1.0)?;
                }
                for owner in &owners {
                    let state = owner.raw_get::<Table>("__songlua_player_option_state")?;
                    state.raw_set("drunk", beat / 10.0)?;
                    owner.raw_set(
                        "__songlua_speedmod_active",
                        if beat < 1.0 {
                            "xmod"
                        } else if beat < 2.0 {
                            "cmod"
                        } else {
                            "mmod"
                        },
                    )?;
                }
                Ok(())
            })
            .unwrap();
        root.raw_set("__songlua_update_function", update).unwrap();
        Self {
            lua,
            context,
            root: Value::Table(root),
            overlays,
            messages: vec![SongLuaMessageEvent {
                beat: 0.75,
                message: "Pulse".into(),
                persists: true,
            }],
        }
    }

    fn compile(&mut self, old: bool) -> String {
        let mut result = if old {
            baseline::compile_update_functions(
                &self.lua,
                &self.root,
                &self.context,
                &mut self.overlays,
                &[],
                &self.messages,
            )
        } else {
            compile_update_functions(
                &self.lua,
                &self.root,
                &self.context,
                &mut self.overlays,
                &mut [],
                &self.messages,
                &mut Vec::new(),
                &mut Vec::new(),
            )
        }
        .unwrap();
        result
            .2
            .sort_by_key(|track| (track.overlay_index, track.target));
        format!("{result:?}")
    }

    fn run(&mut self, old: bool) {
        if old {
            black_box(
                baseline::compile_update_functions(
                    &self.lua,
                    &self.root,
                    &self.context,
                    &mut self.overlays,
                    &[],
                    &self.messages,
                )
                .unwrap(),
            );
        } else {
            black_box(
                compile_update_functions(
                    &self.lua,
                    &self.root,
                    &self.context,
                    &mut self.overlays,
                    &mut [],
                    &self.messages,
                    &mut Vec::new(),
                    &mut Vec::new(),
                )
                .unwrap(),
            );
        }
    }
}

#[test]
fn full_frame_compiler_matches_old_mod_windows_messages_and_actor_tracks() {
    for actors in [0, 1, 32] {
        for seconds in [0.0, 0.02, 1.01, 2.0] {
            let old = CompileFixture::new(actors, seconds).compile(true);
            let new = CompileFixture::new(actors, seconds).compile(false);
            assert_eq!(new, old, "actors={actors}, seconds={seconds}");
        }
    }
}

#[test]
#[ignore = "manual release benchmark; run serially"]
fn frame_sampling_hot_path_bench() {
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for count in [0, 4, 32] {
        let lua = Lua::new();
        let tables = options(&lua, count);
        lua.gc_stop();
        let mut scratch = ModSnapshotScratch::default();
        new_samples(&lua, &tables, 1, &mut scratch);
        for old in order {
            crate::perf::measure_sampled(
                &format!("mod_snapshots_{count}/{}", if old { "old" } else { "new" }),
                32,
                256,
                || {
                    if old {
                        old_samples(&tables, 64);
                    } else {
                        new_samples(&lua, &tables, 64, &mut scratch);
                    }
                },
            );
        }
    }
    for actors in [32, 128, 512] {
        let (mut old_tracks, mut new_tracks) = (vec![], vec![]);
        let (mut old_indices, mut new_indices) = (HashMap::new(), FxHashMap::default());
        track_tick(&mut old_tracks, &mut old_indices, actors, false);
        track_tick(&mut new_tracks, &mut new_indices, actors, false);
        for old in order {
            crate::perf::measure_sampled(
                &format!("track_lookup_{actors}/{}", if old { "old" } else { "new" }),
                256,
                actors * 4,
                || {
                    if old {
                        track_tick(&mut old_tracks, &mut old_indices, actors, false);
                    } else {
                        track_tick(&mut new_tracks, &mut new_indices, actors, false);
                    }
                },
            );
        }
    }
    for samples in [61, 601, 4097, 8193] {
        for old in order {
            crate::perf::measure_sampled(
                &format!(
                    "frame_storage_{samples}/{}",
                    if old { "old" } else { "new" }
                ),
                128,
                samples,
                || collect_frames(samples, !old),
            );
        }
    }
    for old in order {
        crate::perf::measure_sampled_with_setup(
            &format!("full_frame_compile/{}", if old { "old" } else { "new" }),
            8,
            1,
            || CompileFixture::new(32, 2.0),
            |fixture| fixture.run(old),
        );
    }
}
