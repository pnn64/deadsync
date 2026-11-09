//! Complete callback CPU measurements for float-to-i16 output conversion.
//! Archive both source variants' release binaries and alternate their order.
use deadlib_audio_core::*;
use std::{hint::black_box, sync::Arc, time::Instant};
#[allow(dead_code)]
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;

struct Fixture {
    stream: AudioStreamHandle,
    renderer: RenderState,
    input: Arc<[i16]>,
    out_i16: Vec<i16>,
    out_f32: Vec<f32>,
    music: bool,
    sfx: bool,
    floating: bool,
    generation: u64,
    anchor: u64,
}
impl Fixture {
    fn new(samples: usize, kind: &str, floating: bool) -> Self {
        reset_music_target_gain();
        snap_music_gain_generation();
        reset_music_stream_clock_state();
        let music = matches!(kind, "music-gain" | "music-unit" | "music-sfx");
        if music {
            activate_music_track();
        } else {
            stop_music_track();
        }
        let controls = Arc::new(MixControls::new());
        controls.set_stream_gain(if kind == "music-gain" { 0.8 } else { 1.0 });
        controls.set_bus_gain(MixBus::new(0), 0.9);
        let (stream, transport) = music_transport(48000, 2);
        let renderer = RenderState::new(transport, controls, 2);
        Self {
            stream,
            renderer,
            input: (0..samples).map(|i| i.wrapping_mul(7919) as i16).collect(),
            out_i16: vec![0; samples],
            out_f32: vec![0.0; samples],
            music,
            sfx: kind == "music-sfx" || kind == "sfx",
            floating,
            generation: music_map_generation(),
            anchor: 1_000_000,
        }
    }
    #[inline(never)]
    fn run(&mut self, iterations: usize) -> (f64, f64) {
        let start_cycles = bench::cycles();
        let start = Instant::now();
        for _ in 0..iterations {
            if self.music {
                let mut accepted = 0;
                while accepted < self.input.len() {
                    let n = self.stream.writer.try_push(
                        black_box(&self.input[accepted..]),
                        MusicBlockTiming {
                            generation: self.generation,
                            music_start_sec: accepted as f64 / 96000.0,
                            music_sec_per_frame: 1.0 / 48000.0,
                        },
                    );
                    assert_ne!(n, 0);
                    accepted += n;
                }
            }
            let queued = self.sfx.then(|| QueuedSfx {
                data: Arc::clone(&self.input),
                bus: MixBus::new(0),
                generation: 0,
                restart_slot: Some(0),
                pan: SfxPan::Center,
                target_stream_frame: 0,
                voice: None,
            });
            let out = if self.floating {
                OutputBufferMut::F32(black_box(&mut self.out_f32))
            } else {
                OutputBufferMut::I16(black_box(&mut self.out_i16))
            };
            black_box(self.renderer.render(
                out,
                CallbackInfo {
                    anchor_nanos: self.anchor,
                    clock: CallbackClockSource::Instant,
                },
                queued,
            ));
            self.anchor += 1_000_000;
            while let Some(map) = self.stream.played_map.pop() {
                black_box(map);
            }
            black_box((&self.out_i16, &self.out_f32));
        }
        (
            (bench::cycles() - start_cycles) as f64 / iterations as f64,
            start.elapsed().as_nanos() as f64 / iterations as f64,
        )
    }
}
fn main() {
    bench::pin_thread();
    let rotation: usize = std::env::args()
        .nth(1)
        .unwrap_or_default()
        .parse()
        .unwrap_or(0);
    let cases = [
        ("i16-music-32", 32, "music-gain", false),
        ("i16-music-128", 128, "music-gain", false),
        ("i16-music-512", 512, "music-gain", false),
        ("i16-music-1024", 1024, "music-gain", false),
        ("i16-music-4096", 4096, "music-gain", false),
        ("i16-unit-512", 512, "music-unit", false),
        ("i16-unit-4096", 4096, "music-unit", false),
        ("i16-sfx-512", 512, "sfx", false),
        ("i16-sfx-4096", 4096, "sfx", false),
        ("i16-mixed-512", 512, "music-sfx", false),
        ("i16-mixed-4096", 4096, "music-sfx", false),
        ("f32-music-control", 4096, "music-gain", true),
        ("f32-sfx-control", 4096, "sfx", true),
        ("i16-silence-control", 4096, "silence", false),
    ];
    println!("case,samples,round,iterations,cycles,ns");
    for i in 0..cases.len() {
        let (name, samples, kind, floating) = cases[(i + rotation) % cases.len()];
        let mut fixture = Fixture::new(samples, kind, floating);
        let iterations = (12_000_000 / samples).clamp(3000, 60000);
        for round in 0..10 {
            let (cycles, ns) = fixture.run(iterations);
            println!("{name},{samples},{round},{iterations},{cycles:.4},{ns:.4}");
        }
    }
}
