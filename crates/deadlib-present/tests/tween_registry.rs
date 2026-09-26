//! Compare the live tween registry with its frame-stamped predecessor.
use anim::{Step, TweenState};
pub use deadlib_present::anim;
use std::{cell::Cell, hint::black_box};

#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;

// Both implementations expose the same observations and exercise the same
// reentrant source-program builders. No timing or ordering logic lives here.
macro_rules! probes {
    () => {
        pub fn snapshot() -> (usize, Vec<(u64, Vec<u32>, String)>) {
            REG.with(|r| {
                let r = r.borrow();
                assert_eq!(r.entries.len(), r.indices.len());
                assert!(r.materialize_cursor <= r.entries.len());
                let entries = r
                    .entries
                    .iter()
                    .enumerate()
                    .map(|(i, e)| {
                        assert_eq!(r.indices.get(&e.id), Some(&i));
                        (
                            e.id,
                            super::state_bits(e.seq.state()),
                            format!("{:?}", e.seq),
                        )
                    })
                    .collect();
                (r.materialize_cursor, entries)
            })
        }

        pub fn reentrant(
            id: u64,
            initial: TweenState,
            mode: u8,
            calls: &std::cell::Cell<usize>,
        ) -> TweenState {
            materialize_lazy(id, initial, || {
                calls.set(calls.get() + 1);
                match mode {
                    0 => {
                        let _ = materialize(id, TweenState::default(), &[crate::anim::sleep(0.25)]);
                    }
                    1 => {
                        let _ = materialize(id + 1000, initial, &[crate::anim::sleep(0.5)]);
                    }
                    2 => tick(0.125),
                    3 => clear_all(),
                    _ => unreachable!(),
                }
                super::program(id)
            })
        }

        pub fn entry_size() -> usize {
            std::mem::size_of::<Entry>()
        }
    };
}

#[allow(dead_code)]
mod before {
    include!("tween_registry/baseline.rs");
    probes!();
    pub fn reset(frame: u64) {
        clear_all();
        REG.with(|r| r.borrow_mut().frame = frame);
    }
}

#[allow(dead_code)]
mod after {
    include!("../src/runtime.rs");
    probes!();
}

fn state_bits(state: &TweenState) -> Vec<u32> {
    [
        state.x,
        state.y,
        state.z,
        state.w,
        state.h,
        state.hx,
        state.vy,
        state.rot_x,
        state.rot_y,
        state.rot_z,
        state.skew_x,
        state.skew_y,
        state.crop_l,
        state.crop_r,
        state.crop_t,
        state.crop_b,
        state.fade_l,
        state.fade_r,
        state.fade_t,
        state.fade_b,
    ]
    .into_iter()
    .chain(state.tint)
    .chain(state.glow)
    .chain(state.scale)
    .map(f32::to_bits)
    .chain([
        state.visible as u32,
        state.flip_x as u32,
        state.flip_y as u32,
    ])
    .collect()
}

fn initial(id: u64) -> TweenState {
    TweenState {
        x: id as f32,
        y: -(id as f32),
        z: [0.0, -0.0, f32::NAN, f32::INFINITY][id as usize % 4],
        flip_x: id & 1 != 0,
        ..TweenState::default()
    }
}

fn program(id: u64) -> [Step; 4] {
    [
        anim::sleep(0.125),
        anim::linear(0.25).addx(id as f32).alpha(0.5).build(),
        anim::linear(0.0).x(-0.0).set_visible(false).build(),
        anim::smooth(0.5).addx(3.0).rotationz(90.0).build(),
    ]
}

fn compare() {
    assert_eq!(before::snapshot(), after::snapshot());
}

#[test]
fn registry_matches_frame_stamps_through_mixed_traces() {
    let mut operations = 0;
    for frame in [0, u64::MAX - 1, u64::MAX] {
        for seed in 1..=24_u64 {
            before::reset(frame);
            after::clear_all();
            let mut random = seed;
            let old_calls = Cell::new(0);
            let new_calls = Cell::new(0);
            for _ in 0..1024 {
                random ^= random << 13;
                random ^= random >> 7;
                random ^= random << 17;
                let id = (random >> 8) % 32;
                match random % 16 {
                    0..=3 => {
                        let dt = [0.0, -0.1, f32::NAN, 0.125, 0.5, f32::INFINITY]
                            [(random >> 16) as usize % 6];
                        before::tick(dt);
                        after::tick(dt);
                    }
                    4 => {
                        before::clear_all();
                        after::clear_all();
                    }
                    5..=8 => {
                        let mode = (random % 16 - 5) as u8;
                        assert_eq!(
                            state_bits(&before::reentrant(id, initial(id), mode, &old_calls)),
                            state_bits(&after::reentrant(id, initial(id), mode, &new_calls))
                        );
                        assert_eq!(old_calls, new_calls);
                    }
                    _ => {
                        let steps = program(id);
                        assert_eq!(
                            state_bits(&before::materialize(id, initial(id), &steps)),
                            state_bits(&after::materialize(id, initial(id), &steps))
                        );
                    }
                }
                compare();
                operations += 1;
            }
            // Drive retained programs to completion and then expire every actor.
            for _ in 0..16 {
                before::tick(0.125);
                after::tick(0.125);
                for id in 0..32 {
                    let _ = before::materialize(id, initial(id), &program(id));
                    let _ = after::materialize(id, initial(id), &program(id));
                }
                compare();
            }
            before::tick(0.0);
            after::tick(0.0);
            compare();
            before::tick(0.0);
            after::tick(0.0);
            compare();
            assert!(after::snapshot().1.is_empty());
        }
    }
    eprintln!("{operations} mixed operations matched, plus completion/expiry frames");
}

#[derive(Clone, Copy)]
struct Api {
    name: &'static str,
    clear: fn(),
    tick: fn(f32),
    materialize: fn(u64, TweenState, &[Step]) -> TweenState,
}

fn variants() -> [Api; 2] {
    let mut variants = [
        Api {
            name: "before",
            clear: before::clear_all,
            tick: before::tick,
            materialize: before::materialize,
        },
        Api {
            name: "after",
            clear: after::clear_all,
            tick: after::tick,
            materialize: after::materialize,
        },
    ];
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        variants.reverse();
    }
    black_box(variants)
}

fn populate(api: Api, count: usize, active: bool) {
    (api.clear)();
    let steps = [anim::linear(1_000_000.0).x(100.0).alpha(0.0).build()];
    for id in 0..count {
        black_box((api.materialize)(
            id as u64,
            TweenState::default(),
            if active { &steps } else { &[] },
        ));
    }
    (api.tick)(0.001);
    for id in 0..count {
        black_box((api.materialize)(id as u64, TweenState::default(), &[]));
    }
}

fn walk(api: Api, count: usize, reverse: bool, duplicate: bool) {
    (api.tick)(black_box(0.001));
    for i in 0..count {
        let id = if reverse { count - 1 - i } else { i } as u64;
        black_box((api.materialize)(id, TweenState::default(), &[]));
        if duplicate {
            black_box((api.materialize)(id, TweenState::default(), &[]));
        }
    }
}

#[test]
fn warmed_registry_frames_do_not_allocate() {
    for api in variants() {
        for active in [false, true] {
            populate(api, 128, active);
            perf::assert_no_churn(|| {
                for frame in 0..128 {
                    walk(api, 128, frame & 1 != 0, true);
                }
            });
            (api.clear)();
        }
    }
}

#[test]
#[ignore = "manual paired release benchmark; --ignored --nocapture --test-threads=1"]
fn benchmark_tween_registry() {
    eprintln!(
        "Entry bytes: before {}, after {}",
        before::entry_size(),
        after::entry_size()
    );
    for (name, count, active, reorder, duplicate) in [
        ("idle16", 16, false, false, false),
        ("idle128", 128, false, false, false),
        ("idle1024", 1024, false, false, false),
        ("active128", 128, true, false, false),
        ("reorder128", 128, false, true, false),
        ("duplicate128", 128, false, false, true),
    ] {
        for api in variants() {
            populate(api, count, active);
            let mut frame = 0;
            perf::measure_sampled(&format!("{name}/{}", api.name), 8192, count, || {
                frame += 1;
                walk(api, count, reorder && frame & 1 != 0, duplicate);
            });
            (api.clear)();
        }
    }
    for keep in [0, 64] {
        for api in variants() {
            perf::measure_sampled_with_setup(
                &format!("expire128_keep{keep}/{}", api.name),
                512,
                128,
                || {
                    populate(api, 128, true);
                    (api.tick)(0.001);
                    for id in 0..keep {
                        black_box((api.materialize)(id, TweenState::default(), &[]));
                    }
                },
                |_| (api.tick)(black_box(0.001)),
            );
            (api.clear)();
        }
    }
}
