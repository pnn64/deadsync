use super::*;
#[path = "visual_assembly_baseline.rs"]
mod baseline;
#[path = "visual_columns_baseline.rs"]
mod columns_baseline;

use crate::perf::{assert_no_churn, measure_sampled};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};

fn parts(mask: u16) -> HoldVisualParts<u32> {
    let slot = |bit: u32| (mask & (1u16 << bit) != 0).then_some(bit + 1);
    HoldVisualParts {
        head_inactive: slot(0),
        head_active: slot(1),
        body_inactive: slot(2),
        body_active: slot(3),
        topcap_inactive: slot(4),
        topcap_active: slot(5),
        bottomcap_inactive: slot(6),
        bottomcap_active: slot(7),
        head_inactive_layers: (mask & 256 != 0).then(|| Arc::from([21, 22])),
        head_active_layers: (mask & 512 != 0).then(|| Arc::from([31, 32])),
    }
}

#[test]
fn hold_and_roll_fallbacks_match_all_part_combinations() {
    // Every present/absent slot and head-layer combination, including layers
    // without a primary head and a primary head without layers.
    for mask in 0..1024 {
        let p = parts(mask);
        let old = baseline::itg_hold_visuals_from_parts(p.clone());
        let new = itg_hold_visuals_from_parts(p.clone());
        assert_eq!(format!("{old:?}"), format!("{new:?}"), "hold {mask}");
        for hold_mask in [0, 1, 2, 85, 170, 255, 256, 512, 1023] {
            let hold = baseline::itg_hold_visuals_from_parts(parts(hold_mask));
            let old = baseline::itg_roll_visuals_from_parts(p.clone(), &hold);
            let new = itg_roll_visuals_from_parts(p.clone(), &hold);
            assert_eq!(
                format!("{old:?}"),
                format!("{new:?}"),
                "roll {mask}/{hold_mask}"
            );
            if let (Some(a), Some(b)) = (&old.head_active_layers, &new.head_active_layers) {
                assert!(Arc::ptr_eq(a, b));
            }
            if let (Some(a), Some(b)) = (&old.head_inactive_layers, &new.head_inactive_layers) {
                assert!(Arc::ptr_eq(a, b));
            }
        }
    }
}

fn owned_parts() -> HoldVisualParts<Vec<u8>> {
    HoldVisualParts {
        head_inactive: Some(vec![1; 128]),
        head_active: Some(vec![2; 128]),
        body_inactive: Some(vec![3; 128]),
        body_active: Some(vec![4; 128]),
        topcap_inactive: Some(vec![5; 128]),
        topcap_active: Some(vec![6; 128]),
        bottomcap_inactive: Some(vec![7; 128]),
        bottomcap_active: Some(vec![8; 128]),
        ..Default::default()
    }
}

#[test]
fn fully_resolved_hold_and_roll_move_owned_payloads_without_churn() {
    for roll in [false, true] {
        let input = owned_parts();
        let pointer = input.head_inactive.as_ref().unwrap().as_ptr();
        let mut output = None;
        let hold = HoldVisuals::default();
        assert_no_churn(|| {
            output = Some(if roll {
                itg_roll_visuals_from_parts(input, &hold)
            } else {
                itg_hold_visuals_from_parts(input)
            });
        });
        let output = output.unwrap();
        assert_eq!(output.head_inactive.as_ref().unwrap().as_ptr(), pointer);
    }
}

fn layers(count: usize, variant: usize) -> Vec<ItgResolvedSprite<u32>> {
    (0..count)
        .map(|i| {
            let element = [
                "Tap Explosion Dim",
                "W2 Tap Explosion Bright",
                "Held Tap Explosion Dim",
                "Explosion",
                "Miss Tap Explosion Bright",
                "  tAp ExPlOsIoN bRiGhT ",
            ][i % 6];
            let mut commands = HashMap::from([
                (
                    "initcommand".into(),
                    "zoom,0.7;diffuse,0.5,0.6,0.7,0.8".into(),
                ),
                ("judgmentcommand".into(), "rotationz,12".into()),
                ("dimcommand".into(), "diffusealpha,0.6".into()),
                ("brightcommand".into(), "blend,add;diffusealpha,1".into()),
                (
                    "holdingoncommand".into(),
                    "glowshift;effectperiod,0.5".into(),
                ),
            ]);
            match variant {
                0 => {
                    commands.insert(
                        "w1command".into(),
                        "linear,0.2;zoom,1.2;diffusealpha,0".into(),
                    );
                }
                1 => {
                    commands.insert(
                        "w2command".into(),
                        "self:linear(0.2); self:diffusealpha(0)".into(),
                    );
                }
                2 => {
                    commands.insert("heldcommand".into(), "   ".into());
                }
                _ => {}
            }
            ItgResolvedSprite {
                element: element.into(),
                slot: i as u32 + 1,
                commands,
            }
        })
        .collect()
}

fn assert_maps(old: &TapExplosionMap<u32>, new: &TapExplosionMap<u32>) {
    assert_eq!(format!("{old:?}"), format!("{new:?}"));
    for key in TAP_EXPLOSION_KEYS {
        if let (Some(a), Some(b)) = (old.get(key), new.get(key)) {
            assert_eq!(a.duration().to_bits(), b.duration().to_bits());
            for (a, b) in a.layers.iter().zip(b.layers.iter()) {
                for time in [-0.1, 0.0, 0.1, 0.2, 0.3, 2.0, f32::NAN] {
                    assert_eq!(
                        format!("{:?}", a.animation.state_at(time)),
                        format!("{:?}", b.animation.state_at(time))
                    );
                }
            }
        }
    }
}

#[test]
fn tap_maps_preserve_layers_animation_and_ordered_fallback_callbacks() {
    for count in [0, 1, 2, 4, 8, 17, 32] {
        for variant in 0..4 {
            for metrics in [false, true] {
                let input = layers(count, variant);
                let run = |old: bool| {
                    let calls = RefCell::new(Vec::new());
                    let direct = |element: &str| {
                        calls.borrow_mut().push(format!("direct:{element}"));
                        let mut result = layers(3, 3);
                        for sprite in &mut result {
                            sprite.element = element.into();
                        }
                        result
                    };
                    let metric = |mode, key: &str| {
                        calls.borrow_mut().push(format!("metric:{mode:?}:{key}"));
                        metrics.then(|| "decelerate,0.15;zoom,0.8;diffusealpha,0".into())
                    };
                    let map = if old {
                        baseline::itg_tap_explosion_map_from_resolved_layers(&input, direct, metric)
                    } else {
                        itg_tap_explosion_map_from_resolved_layers(&input, direct, metric)
                    };
                    (map, calls.into_inner())
                };
                let (old, old_calls) = run(true);
                let (new, new_calls) = run(false);
                assert_maps(&old, &new);
                assert_eq!(old_calls, new_calls);
            }
        }
    }
}

#[test]
fn empty_tap_sources_do_not_allocate() {
    assert_no_churn(|| {
        black_box(itg_tap_explosion_map_from_resolved_layers::<u32>(
            &[],
            |_| Vec::new(),
            |_, _| panic!("no source"),
        ));
    });
}

#[test]
fn rejected_actor_commands_borrow_metadata_without_churn() {
    for count in [1, 4] {
        let mut input = layers(count, 2);
        // A missing score command still dispatches Judgment and Bright/Dim.
        // Only actors without any such event work are actually rejected.
        for sprite in &mut input {
            sprite.commands.retain(|key, _| {
                !matches!(
                    key.as_str(),
                    "judgmentcommand" | "brightcommand" | "dimcommand"
                )
            });
        }
        assert_no_churn(|| {
            let map = itg_tap_explosion_map_from_resolved_layers(
                &input,
                |_| panic!("actor sources take precedence"),
                |_, _| None,
            );
            assert!(map.is_empty());
        });
    }
}

#[test]
fn owned_source_compatibility_preserves_explicit_mode_overrides() {
    let sources = layers(8, 0)
        .into_iter()
        .map(|sprite| {
            let mut source =
                ItgTapExplosionSource::new(sprite.element, sprite.slot, sprite.commands);
            source.mode = ItgTapExplosionMode::Bright;
            source
        })
        .collect::<Vec<_>>();
    let (dim, bright) = itg_partition_tap_explosion_sources(sources.clone());
    let old =
        baseline::itg_tap_explosion_map_from_partitioned_sources(dim, bright, false, |_, _| None);
    let new = itg_tap_explosion_map_from_sources(sources, |_, _| None);
    assert_maps(&old, &new);
}

fn data() -> itg::NoteskinData {
    itg::NoteskinData {
        overrides: Vec::new(),
        name: "fixture".into(),
        metrics: itg::IniData::default(),
        search_dirs: Vec::new(),
    }
}

fn columns(num_cols: usize) -> ItgRuntimeColumns<u32> {
    ItgRuntimeColumns {
        notes: Vec::new(),
        note_layers: Vec::new(),
        lift_note_layers: Vec::new(),
        receptor_off: Vec::new(),
        receptor_glow: Vec::new(),
        receptor_idle_glow_layers: Vec::new(),
        receptor_off_reverse: Vec::new(),
        receptor_glow_reverse: Vec::new(),
        receptor_idle_glow_reverse: Vec::new(),
        receptor_step_behaviors: Vec::new(),
        receptor_idle_glow: ReceptorIdleGlow::None,
        mines: Vec::new(),
        mine_frames: Vec::new(),
        receptor_pulse_command: None,
        hold_columns: vec![HoldVisuals::default(); num_cols],
        roll_columns: vec![HoldVisuals::default(); num_cols],
    }
}

type RuntimeFixture = (
    itg::NoteskinData,
    crate::Style,
    compiled::CompiledLoader,
    ItgRuntimeColumns<u32>,
);
fn runtime_fixture(num_cols: usize, blank: bool) -> RuntimeFixture {
    let data = data();
    let style = crate::Style {
        num_cols,
        num_players: 1,
    };
    let mut loader = compiled::CompiledLoader::default();
    for button in [
        "Left",
        "Down",
        "Up",
        "Right",
        "Center",
        "DownLeft",
        "DownRight",
        "UpLeft",
        "UpRight",
    ] {
        for element in ["Hold Explosion", "Roll Explosion"] {
            loader.entries.push(compiled::CompiledLoaderEntry {
                button: button.into(),
                element: element.into(),
                load_button: button.into(),
                load_element: element.into(),
                blank,
                rotation_x: None,
                rotation_y: None,
                rotation_z: None,
                init_command: None,
            });
        }
    }
    (data, style, loader, columns(num_cols))
}

fn runtime_run(
    variant: u8,
    num_cols: usize,
    input: &[ItgResolvedSprite<u32>],
    load: RuntimeLoad,
    blank: bool,
    record: bool,
) -> (NoteskinRuntime<u32>, Vec<String>) {
    runtime_prepared(
        variant,
        &runtime_fixture(num_cols, blank),
        input,
        load,
        record,
    )
}

fn runtime_prepared(
    variant: u8,
    fixture: &RuntimeFixture,
    input: &[ItgResolvedSprite<u32>],
    load: RuntimeLoad,
    record: bool,
) -> (NoteskinRuntime<u32>, Vec<String>) {
    let (data, style, loader, columns) = fixture;
    let style = *style;
    let calls = RefCell::new(Vec::new());
    let resolve = |button: &str, element: &str| {
        if record {
            calls
                .borrow_mut()
                .push(format!("resolve:{button}:{element}"));
        }
        if element == "Explosion" {
            input.to_vec()
        } else {
            Vec::new()
        }
    };
    let hold = |wrapper: &[ItgResolvedSprite<u32>],
                source: &[ItgResolvedSprite<u32>],
                button: &str,
                active: &str,
                hint: &str,
                blank,
                asset: Option<&str>,
                prefix: Option<&str>,
                fallback: Option<&u32>| {
        if record {
            calls.borrow_mut().push(format!("hold:{wrapper:?}:{source:?}:{button}:{active}:{hint}:{blank}:{asset:?}:{prefix:?}:{fallback:?}"));
        }
        if blank {
            None
        } else {
            wrapper.first().map(|s| s.slot).or(fallback.copied())
        }
    };
    let runtime = match variant {
        0 => baseline::itg_noteskin_runtime_selected(
            data,
            style,
            loader,
            NoteDisplayMetrics::default(),
            true,
            columns.clone(),
            resolve,
            hold,
            |_, _| None,
            |_, _| None,
            |mines| mines.to_vec(),
            |slot| format!("slot{slot}"),
            |slot, _, _| *slot,
            load,
        ),
        1 => columns_baseline::itg_noteskin_runtime_selected(
            data,
            style,
            loader,
            NoteDisplayMetrics::default(),
            true,
            columns.clone(),
            resolve,
            hold,
            |_, _| None,
            |_, _| None,
            |mines| mines.to_vec(),
            |slot| format!("slot{slot}"),
            |slot, _, _| *slot,
            load,
        ),
        _ => itg_noteskin_runtime_selected(
            data,
            style,
            loader,
            NoteDisplayMetrics::default(),
            true,
            columns.clone(),
            resolve,
            hold,
            |_, _| None,
            |_, _| None,
            |mines| mines.to_vec(),
            |slot| format!("slot{slot}"),
            |slot, _, _| *slot,
            load,
        ),
    };
    (runtime, calls.into_inner())
}

#[test]
fn runtime_assembly_preserves_dance_pump_blanks_and_previews() {
    for num_cols in [0, 1, 4, 5, 8, 10] {
        for count in [0, 1, 8] {
            for blank in [false, true] {
                for load in [
                    RuntimeLoad::GAMEPLAY,
                    RuntimeLoad {
                        parts: SkinParts::default().with(SkinPart::TapExplosions),
                        preview: true,
                    },
                    RuntimeLoad {
                        parts: SkinParts::default().with(SkinPart::HoldExplosions),
                        preview: true,
                    },
                    RuntimeLoad {
                        parts: SkinParts::default(),
                        preview: true,
                    },
                ] {
                    let input = layers(count, 0);
                    let (old, old_calls) = runtime_run(0, num_cols, &input, load, blank, true);
                    let (new, new_calls) = runtime_run(2, num_cols, &input, load, blank, true);
                    assert_eq!(
                        format!("{old:?}"),
                        format!("{new:?}"),
                        "{num_cols}/{count}/{blank}"
                    );
                    assert_eq!(old_calls, new_calls);
                }
            }
        }
    }
}

// Same relevant costs as SpriteSlot: fresh clone identity, four Arc resources,
// and inline state. This is a surrogate, not a full asset or renderer benchmark.
#[derive(Debug)]
struct Slot {
    id: u64,
    resources: [Arc<[u8]>; 4],
    state: [f32; 48],
}
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
impl Clone for Slot {
    fn clone(&self) -> Self {
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            resources: self.resources.clone(),
            state: self.state,
        }
    }
}
fn slot_parts(active: bool) -> HoldVisualParts<Slot> {
    let resource: Arc<[u8]> = Arc::from([0; 64]);
    let slot = || Slot {
        id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        resources: std::array::from_fn(|_| resource.clone()),
        state: [1.0; 48],
    };
    HoldVisualParts {
        head_inactive: Some(slot()),
        head_active: active.then(slot),
        body_inactive: Some(slot()),
        body_active: active.then(slot),
        topcap_inactive: Some(slot()),
        topcap_active: active.then(slot),
        bottomcap_inactive: Some(slot()),
        bottomcap_active: active.then(slot),
        head_inactive_layers: Some(Arc::from([slot(), slot()])),
        head_active_layers: active.then(|| Arc::from([slot(), slot()])),
    }
}

#[test]
fn moved_slots_retain_identity_and_fallback_clones_stay_distinct() {
    let p = slot_parts(true);
    let id = p.head_inactive.as_ref().unwrap().id;
    let hold = itg_hold_visuals_from_parts(p);
    assert_eq!(hold.head_inactive.as_ref().unwrap().id, id);
    let p = slot_parts(false);
    let id = p.head_inactive.as_ref().unwrap().id;
    let roll = itg_roll_visuals_from_parts(p, &hold);
    assert_eq!(roll.head_inactive.as_ref().unwrap().id, id);
    assert_ne!(roll.head_active.as_ref().unwrap().id, id);
    assert!(Arc::ptr_eq(
        roll.head_active_layers.as_ref().unwrap(),
        roll.head_inactive_layers.as_ref().unwrap()
    ));
}

#[test]
#[ignore = "manual release comparison; run after all builds finish"]
fn benchmark_visual_assembly() {
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for active in [true, false] {
        let input = slot_parts(active);
        let hold = itg_hold_visuals_from_parts(input.clone());
        for roll in [false, true] {
            for old in order {
                measure_sampled(
                    &format!(
                        "{}_{}_{}/{}",
                        if roll { "roll" } else { "hold" },
                        if active { "complete" } else { "fallback" },
                        "slots",
                        if old { "old" } else { "new" }
                    ),
                    4096,
                    1,
                    || {
                        if roll {
                            if old {
                                baseline::itg_roll_visuals_from_parts(
                                    black_box(input.clone()),
                                    black_box(&hold),
                                )
                            } else {
                                itg_roll_visuals_from_parts(
                                    black_box(input.clone()),
                                    black_box(&hold),
                                )
                            }
                        } else if old {
                            baseline::itg_hold_visuals_from_parts(black_box(input.clone()))
                        } else {
                            itg_hold_visuals_from_parts(black_box(input.clone()))
                        }
                    },
                );
            }
        }
    }
    for count in [0, 1, 4, 8, 32] {
        for direct in [false, true] {
            let input = layers(count, if direct { 3 } else { 0 });
            for old in order {
                let fallback = |_: &str| input.clone();
                let metric = |_, _: &str| None;
                measure_sampled(
                    &format!(
                        "tap_{}_{count}/{}",
                        if direct { "direct" } else { "actor" },
                        if old { "old" } else { "new" }
                    ),
                    128,
                    1,
                    || {
                        if old {
                            baseline::itg_tap_explosion_map_from_resolved_layers(
                                black_box(&input),
                                fallback,
                                metric,
                            )
                        } else {
                            itg_tap_explosion_map_from_resolved_layers(
                                black_box(&input),
                                fallback,
                                metric,
                            )
                        }
                    },
                );
            }
        }
    }
    for count in [1, 4, 8, 32] {
        let input = layers(count, 3);
        for old in order {
            let fallback = |_: &str| input.clone();
            let metric = |_, _: &str| Some("linear,0.2;zoom,1.2;diffusealpha,0".to_owned());
            measure_sampled(
                &format!(
                    "tap_metric_direct_{count}/{}",
                    if old { "old" } else { "new" }
                ),
                128,
                1,
                || {
                    if old {
                        baseline::itg_tap_explosion_map_from_resolved_layers(
                            black_box(&input),
                            fallback,
                            metric,
                        )
                    } else {
                        itg_tap_explosion_map_from_resolved_layers(
                            black_box(&input),
                            fallback,
                            metric,
                        )
                    }
                },
            );
        }
    }
    for count in [1, 8, 32] {
        let sources = layers(count, 0)
            .into_iter()
            .map(|sprite| ItgTapExplosionSource::new(sprite.element, sprite.slot, sprite.commands))
            .collect::<Vec<_>>();
        for old in order {
            measure_sampled(
                &format!("tap_owned_{count}/{}", if old { "old" } else { "new" }),
                128,
                1,
                || {
                    if old {
                        let (dim, bright) =
                            itg_partition_tap_explosion_sources(black_box(sources.clone()));
                        baseline::itg_tap_explosion_map_from_partitioned_sources(
                            dim,
                            bright,
                            false,
                            |_, _| None,
                        )
                    } else {
                        itg_tap_explosion_map_from_sources(black_box(sources.clone()), |_, _| None)
                    }
                },
            );
        }
    }
    for num_cols in [4, 5, 8, 10] {
        let input = layers(8, 0);
        let fixture = runtime_fixture(num_cols, false);
        for combined in [false, true] {
            for old in order {
                measure_sampled(
                    &format!(
                        "runtime_{}_{num_cols}/{}",
                        if combined { "combined" } else { "columns" },
                        if old { "old" } else { "new" }
                    ),
                    32,
                    1,
                    || {
                        runtime_prepared(
                            if old { if combined { 0 } else { 1 } } else { 2 },
                            black_box(&fixture),
                            black_box(&input),
                            RuntimeLoad::GAMEPLAY,
                            false,
                        )
                    },
                );
            }
        }
    }
}
