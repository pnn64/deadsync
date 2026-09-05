//! Run with `cargo test -p deadsync-input --release --test input_pipeline_timing -- --ignored --nocapture`.
use deadsync_input::keymap::InputState;
use deadsync_input::{
    InputBinding, KeyCode, Keymap, PadDir, PadEvent, PadId, RawKeyboardEvent, VirtualAction,
};
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

fn measure(name: &str, mut run: impl FnMut()) {
    const SAMPLES: u32 = 2_000_000;
    for _ in 0..10_000 {
        run();
    }
    let start = Instant::now();
    for _ in 0..SAMPLES {
        run();
    }
    println!(
        "{name}: {:.2} ns/event",
        start.elapsed().as_nanos() as f64 / f64::from(SAMPLES)
    );
}

#[test]
#[ignore = "manual release-mode timing; no machine-dependent assertions"]
fn input_pipeline_timing() {
    let mut km = Keymap::default();
    km.bind(
        VirtualAction::p1_left,
        &[
            InputBinding::Key(KeyCode::ArrowLeft),
            InputBinding::PadDir(PadDir::Left),
        ],
    );
    km.bind(
        VirtualAction::system_fast_forward,
        &[InputBinding::Key(KeyCode::ArrowLeft)],
    );
    let mut input = InputState::new(&km, 0.0);
    let timestamp = Instant::now();
    let mut raw = RawKeyboardEvent {
        code: KeyCode::ArrowLeft,
        pressed: false,
        repeat: false,
        timestamp,
        host_nanos: 1,
    };
    measure("keyboard lookup", || {
        black_box(input.key_event(black_box(raw)));
    });
    let press = input.key_event(RawKeyboardEvent {
        pressed: true,
        ..raw
    });
    let release = input.key_event(raw);
    measure("keyboard debounce + normalize", || {
        raw.pressed = !raw.pressed;
        let key = black_box(if raw.pressed { press } else { release });
        input.map_key(key, || timestamp).for_each(|event| {
            black_box(event);
        });
    });
    input.clear();
    measure("keyboard lookup + clock + debounce + normalize", || {
        raw.pressed = !raw.pressed;
        let key = input.key_event(black_box(raw));
        black_box(key.system_mask);
        input.map_key(key, Instant::now).for_each(|event| {
            black_box(event);
        });
    });
    let pad = PadEvent::Dir {
        id: PadId(0),
        dir: PadDir::Left,
        pressed: true,
        timestamp,
        host_nanos: 1,
    };
    input.map_pad(&pad, Instant::now).for_each(drop);
    measure("settled pad repeat", || {
        input
            .map_pad(black_box(&pad), Instant::now)
            .for_each(|event| {
                black_box(event);
            })
    });
    measure("empty drain", || {
        if black_box(input.has_pending()) {
            let now = Instant::now();
            while let Some(events) = input.next_due(now) {
                events.for_each(|event| {
                    black_box(event);
                });
            }
        }
    });
    input.clear();
    input.set_debounce_seconds(0.02);
    let mut receipt = timestamp;
    measure("short tap + delayed release", || {
        input
            .map_key(black_box(press), || receipt)
            .for_each(|event| {
                black_box(event);
            });
        input
            .map_key(black_box(release), || receipt)
            .for_each(|event| {
                black_box(event);
            });
        receipt += Duration::from_millis(20);
        while let Some(events) = input.next_due(receipt) {
            events.for_each(|event| {
                black_box(event);
            });
        }
        receipt += Duration::from_millis(20);
    });
}
