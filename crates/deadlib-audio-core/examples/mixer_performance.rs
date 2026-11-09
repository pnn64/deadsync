//! Before/after benchmark of the public mixer. Build both revisions with the
//! same release profile, then alternate their archived executables.
use deadlib_audio_core::{ActiveSfx, MixBus, MixControls, QueuedSfx, SfxPan, mix_active_sfx};
use std::{hint::black_box, sync::Arc, time::Instant};

#[allow(dead_code)]
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;

struct Fixture {
    active: Vec<ActiveSfx>,
    output: Vec<f32>,
    controls: MixControls,
}

impl Fixture {
    fn new(samples: usize, voices: usize, gain: f32, pan: SfxPan) -> Self {
        let bus = MixBus::new(0);
        let controls = MixControls::new();
        controls.set_bus_gain(bus, gain);
        // Leave a complete frame after the measured buffer so voices survive.
        let data: Arc<[i16]> = (0..samples + 2)
            .map(|i| i.wrapping_mul(7919) as i16)
            .collect();
        let active = (0..voices)
            .map(|_| {
                ActiveSfx::from_queued(QueuedSfx {
                    data: Arc::clone(&data),
                    bus,
                    generation: 0,
                    restart_slot: None,
                    pan,
                    target_stream_frame: 0,
                    voice: None,
                })
            })
            .collect();
        Self {
            active,
            output: vec![0.0; samples],
            controls,
        }
    }

    #[inline(never)]
    fn run(&mut self, iterations: usize) -> (f64, f64) {
        let start_cycles = bench::cycles();
        let start = Instant::now();
        for _ in 0..iterations {
            for voice in &mut self.active {
                voice.cursor = 0;
            }
            // Match a freshly cleared callback buffer and keep values bounded.
            self.output.fill(0.0);
            black_box(mix_active_sfx(
                black_box(&mut self.active),
                black_box(&mut self.output),
                0,
                black_box(2),
                black_box(&self.controls),
            ));
            black_box(&self.output);
        }
        let ns = start.elapsed().as_nanos() as f64 / iterations as f64;
        let cycles = (bench::cycles() - start_cycles) as f64 / iterations as f64;
        (cycles, ns)
    }
}

fn main() {
    bench::pin_thread();
    let rotation: usize = std::env::args()
        .nth(1)
        .unwrap_or_default()
        .parse()
        .unwrap_or(0);
    // Default application levels: master=90%, SFX=100%.
    let default_gain = 90.0_f32 * 0.01 * 100.0 * 0.01;
    let cases = [
        ("default-32", 32, 1, default_gain, SfxPan::Center),
        ("default-128", 128, 1, default_gain, SfxPan::Center),
        ("default-1024", 1024, 1, default_gain, SfxPan::Center),
        ("default-4096", 4096, 1, default_gain, SfxPan::Center),
        ("default-eight", 1024, 8, default_gain, SfxPan::Center),
        ("default-max", 1024, 32, default_gain, SfxPan::Center),
        ("gain-0.7", 1024, 1, 0.7, SfxPan::Center),
        ("unity-32", 32, 1, 1.0, SfxPan::Center),
        ("unity-128", 128, 1, 1.0, SfxPan::Center),
        ("unity-1024", 1024, 1, 1.0, SfxPan::Center),
        ("unity-eight", 1024, 8, 1.0, SfxPan::Center),
        ("exact-gain", 1024, 1, 0.375, SfxPan::Center),
        ("zero-gain", 1024, 1, 0.0, SfxPan::Center),
        ("panned-control", 1024, 1, default_gain, SfxPan::Left),
        ("no-voices-control", 1024, 0, default_gain, SfxPan::Center),
    ];
    println!("case,samples,voices,gain_bits,round,iterations,cycles,ns");
    for i in 0..cases.len() {
        let (name, samples, voices, gain, pan) = cases[(i + rotation) % cases.len()];
        let mut fixture = Fixture::new(samples, voices, gain, pan);
        let iterations = (20_000_000 / (samples * voices.max(1))).max(1000);
        for round in 0..10 {
            let (cycles, ns) = fixture.run(iterations);
            println!(
                "{name},{samples},{voices},{:08x},{round},{iterations},{cycles:.4},{ns:.4}",
                gain.to_bits()
            );
        }
    }
}
