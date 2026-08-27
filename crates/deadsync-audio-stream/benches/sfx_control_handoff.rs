use deadlib_audio_core::{MixBus, QueuedSfx, SfxReceiver, SfxSender, sfx_transport};
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::{Arc, Mutex};
use std::time::Instant;

const QUEUE_CAPACITY: usize = 128;
const WARMUP_SAMPLES: usize = 2_048;
const SAMPLES: usize = 16_384;
const OPS_PER_SAMPLE: usize = 64;
const SOUND_PATH: &str = "assets/sounds/assist_tick.ogg";

struct OldControl {
    sounds: Mutex<HashMap<String, Arc<[i16]>>>,
    sender: Mutex<SfxSender>,
}

impl OldControl {
    fn play(&self, path: &str, bus: MixBus, generation: u64) {
        let data = self.sounds.lock().unwrap().get(path).cloned().unwrap();
        let mut sender = self.sender.lock().unwrap();
        let _ = sender.try_send(QueuedSfx {
            data,
            bus,
            generation,
            target_stream_frame: generation,
        });
    }
}

struct NewControl {
    sender: SfxSender,
}

impl NewControl {
    fn play(&mut self, sound: &Arc<[i16]>, bus: MixBus, generation: u64) {
        let _ = self.sender.try_send(QueuedSfx {
            data: Arc::clone(sound),
            bus,
            generation,
            target_stream_frame: generation,
        });
    }
}

fn drain(receiver: &mut SfxReceiver) -> u64 {
    receiver.try_iter().fold(0, |checksum, queued| {
        checksum ^ queued.generation ^ queued.target_stream_frame ^ queued.data.len() as u64
    })
}

fn measure_old() -> (Vec<f64>, u64) {
    let sound: Arc<[i16]> = Arc::from([1, -1, 2, -2]);
    let (sender, mut receiver) = sfx_transport(QUEUE_CAPACITY);
    let mut sounds = HashMap::new();
    sounds.insert(SOUND_PATH.to_owned(), sound);
    let control = OldControl {
        sounds: Mutex::new(sounds),
        sender: Mutex::new(sender),
    };
    let bus = MixBus::new(0);
    let mut checksum = 0;
    for sample in 0..WARMUP_SAMPLES {
        for op in 0..OPS_PER_SAMPLE {
            control.play(SOUND_PATH, bus, (sample * OPS_PER_SAMPLE + op) as u64);
        }
        checksum ^= drain(&mut receiver);
    }
    let mut samples = Vec::with_capacity(SAMPLES);
    for sample in 0..SAMPLES {
        let base = sample * OPS_PER_SAMPLE;
        let started = Instant::now();
        for op in 0..OPS_PER_SAMPLE {
            control.play(SOUND_PATH, bus, (base + op) as u64);
        }
        samples.push(started.elapsed().as_secs_f64() * 1e9 / OPS_PER_SAMPLE as f64);
        checksum ^= drain(&mut receiver);
    }
    (samples, checksum)
}

fn measure_new() -> (Vec<f64>, u64) {
    let sound: Arc<[i16]> = Arc::from([1, -1, 2, -2]);
    let (sender, mut receiver) = sfx_transport(QUEUE_CAPACITY);
    let mut control = NewControl { sender };
    let bus = MixBus::new(0);
    let mut checksum = 0;
    for sample in 0..WARMUP_SAMPLES {
        for op in 0..OPS_PER_SAMPLE {
            control.play(&sound, bus, (sample * OPS_PER_SAMPLE + op) as u64);
        }
        checksum ^= drain(&mut receiver);
    }
    let mut samples = Vec::with_capacity(SAMPLES);
    for sample in 0..SAMPLES {
        let base = sample * OPS_PER_SAMPLE;
        let started = Instant::now();
        for op in 0..OPS_PER_SAMPLE {
            control.play(&sound, bus, (base + op) as u64);
        }
        samples.push(started.elapsed().as_secs_f64() * 1e9 / OPS_PER_SAMPLE as f64);
        checksum ^= drain(&mut receiver);
    }
    (samples, checksum)
}

const fn percentile(sorted: &[f64], percent: usize) -> f64 {
    sorted[(sorted.len() - 1) * percent / 100]
}

fn print(label: &str, mut samples: Vec<f64>) {
    samples.sort_by(f64::total_cmp);
    println!(
        "  {label:<3} p50={:>8.2} ns  p95={:>8.2} ns  p99={:>8.2} ns  worst={:>8.2} ns",
        percentile(&samples, 50),
        percentile(&samples, 95),
        percentile(&samples, 99),
        samples[samples.len() - 1],
    );
}

fn main() {
    let (old, old_checksum) = measure_old();
    let (new, new_checksum) = measure_new();
    black_box((old_checksum, new_checksum));
    println!("preloaded SFX producer handoff ({OPS_PER_SAMPLE} submissions/sample)");
    print("old", old);
    print("new", new);
    println!("  synchronization: old=2 mutex acquisitions/op new=0 mutex acquisitions/op");
    println!("  lookup:          old=1 string hash lookup/op  new=0 hash lookups/op");
}
