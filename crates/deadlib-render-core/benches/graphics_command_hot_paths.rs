use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP: usize = 5_000;
const SAMPLES: usize = 100;
const OPS_PER_SAMPLE: usize = 1_000;
const ALLOC_OPS: usize = 10_000;
const METAL_UPLOADS: usize = 16;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    alloc_bytes: AtomicU64,
    realloc_bytes: AtomicU64,
    free_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            alloc_bytes: AtomicU64::new(0),
            realloc_bytes: AtomicU64::new(0),
            free_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            alloc_bytes: self.alloc_bytes.load(Ordering::Relaxed),
            realloc_bytes: self.realloc_bytes.load(Ordering::Relaxed),
            free_bytes: self.free_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every operation delegates unchanged to `System`; the counters only
// observe successful allocator calls while the benchmark gate is enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is supplied by the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.alloc_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
            self.free_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: this pointer-layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.realloc_bytes
                .fetch_add((old.size() + new_size) as u64, Ordering::Relaxed);
        }
        out
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    alloc_bytes: u64,
    realloc_bytes: u64,
    free_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            realloc_bytes: self.realloc_bytes - before.realloc_bytes,
            free_bytes: self.free_bytes - before.free_bytes,
        }
    }

    const fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

struct Measurement {
    ns_per_op: f64,
    cycles_per_op: Option<f64>,
    p95_ns: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure_pair(mut old: impl FnMut() -> u64, mut new: impl FnMut() -> u64) -> [Measurement; 2] {
    let mut ops: [&mut dyn FnMut() -> u64; 2] = [&mut old, &mut new];
    for round in 0..WARMUP {
        black_box(ops[round % 2]());
        black_box(ops[(round + 1) % 2]());
    }

    let mut elapsed = [Duration::ZERO; 2];
    let mut cycles = [Some(0u64); 2];
    let mut checksums = [0u64; 2];
    let mut samples: [Vec<Duration>; 2] = std::array::from_fn(|_| Vec::with_capacity(SAMPLES));
    for sample in 0..SAMPLES {
        for offset in 0..2 {
            let index = (sample + offset) % 2;
            let cycle_start = cycle_counter();
            let started = Instant::now();
            let mut checksum = 0u64;
            for _ in 0..OPS_PER_SAMPLE {
                checksum = checksum.wrapping_add(black_box(ops[index]()));
            }
            let sample_elapsed = started.elapsed();
            let cycle_end = cycle_counter();
            elapsed[index] += sample_elapsed;
            samples[index].push(sample_elapsed);
            checksums[index] = checksums[index].wrapping_add(checksum);
            cycles[index] = cycles[index]
                .zip(cycle_start.zip(cycle_end))
                .map(|(total, (start, end))| total.wrapping_add(end.wrapping_sub(start)));
        }
    }

    let allocated: [AllocSnapshot; 2] = std::array::from_fn(|index| {
        let before = ALLOC.snapshot();
        ALLOC.enabled.store(true, Ordering::Relaxed);
        for _ in 0..ALLOC_OPS {
            black_box(ops[index]());
        }
        ALLOC.enabled.store(false, Ordering::Relaxed);
        ALLOC.snapshot().delta(before)
    });
    let operations = (SAMPLES * OPS_PER_SAMPLE) as f64;
    std::array::from_fn(|index| {
        samples[index].sort_unstable();
        Measurement {
            ns_per_op: elapsed[index].as_secs_f64() * 1_000_000_000.0 / operations,
            cycles_per_op: cycles[index].map(|value| value as f64 / operations),
            p95_ns: samples[index][SAMPLES * 95 / 100].as_secs_f64() * 1_000_000_000.0
                / OPS_PER_SAMPLE as f64,
            allocated: allocated[index],
            checksum: checksums[index],
        }
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Completion {
    present_id: u32,
    host_ns: u64,
    interval_ns: u64,
    refresh_ns: u64,
}

struct LegacyCompletions {
    tx: mpsc::Sender<Completion>,
    rx: mpsc::Receiver<Completion>,
    last: Completion,
    next_id: u32,
}

impl LegacyCompletions {
    fn cycle(&mut self) -> u64 {
        let present_id = self.next_id;
        self.next_id += 1;
        let host_ns = 1_000_000_000 + u64::from(present_id) * 16_666_667;
        let tx = self.tx.clone();
        tx.send(Completion {
            present_id,
            host_ns,
            ..Completion::default()
        })
        .expect("completion receiver");
        while let Ok(done) = self.rx.try_recv() {
            let interval_ns = if self.last.host_ns == 0 {
                0
            } else {
                done.host_ns.saturating_sub(self.last.host_ns)
            };
            let refresh_ns = if interval_ns == 0 {
                self.last.refresh_ns
            } else if self.last.refresh_ns == 0 {
                interval_ns
            } else {
                (self.last.refresh_ns.saturating_mul(3) + interval_ns) / 4
            };
            self.last = Completion {
                interval_ns,
                refresh_ns,
                ..done
            };
        }
        completion_checksum(self.last)
    }
}

struct CompletionCell {
    version: AtomicU32,
    present_id: AtomicU32,
    host_ns: AtomicU64,
    interval_ns: AtomicU64,
    refresh_ns: AtomicU64,
}

impl CompletionCell {
    const fn new() -> Self {
        Self {
            version: AtomicU32::new(0),
            present_id: AtomicU32::new(0),
            host_ns: AtomicU64::new(0),
            interval_ns: AtomicU64::new(0),
            refresh_ns: AtomicU64::new(0),
        }
    }

    fn publish(&self, present_id: u32, host_ns: u64) {
        let version = loop {
            let version = self.version.load(Ordering::Relaxed);
            if version & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }
            if self
                .version
                .compare_exchange_weak(
                    version,
                    version.wrapping_add(1),
                    Ordering::Acquire,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                break version;
            }
        };
        let previous_host = self.host_ns.load(Ordering::Relaxed);
        let previous_refresh = self.refresh_ns.load(Ordering::Relaxed);
        let interval_ns = if host_ns == 0 || previous_host == 0 {
            0
        } else {
            host_ns.saturating_sub(previous_host)
        };
        let refresh_ns = if interval_ns == 0 {
            previous_refresh
        } else if previous_refresh == 0 {
            interval_ns
        } else {
            (previous_refresh.saturating_mul(3) + interval_ns) / 4
        };
        self.present_id.store(present_id, Ordering::Relaxed);
        if host_ns != 0 {
            self.host_ns.store(host_ns, Ordering::Relaxed);
        }
        self.interval_ns.store(interval_ns, Ordering::Relaxed);
        self.refresh_ns.store(refresh_ns, Ordering::Relaxed);
        self.version
            .store(version.wrapping_add(2), Ordering::Release);
    }

    fn load(&self) -> Completion {
        loop {
            let before = self.version.load(Ordering::Acquire);
            if before & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }
            let completion = Completion {
                present_id: self.present_id.load(Ordering::Relaxed),
                host_ns: self.host_ns.load(Ordering::Relaxed),
                interval_ns: self.interval_ns.load(Ordering::Relaxed),
                refresh_ns: self.refresh_ns.load(Ordering::Relaxed),
            };
            if self.version.load(Ordering::Acquire) == before {
                return completion;
            }
        }
    }
}

struct AtomicCompletions {
    cell: Arc<CompletionCell>,
    next_id: u32,
}

impl AtomicCompletions {
    fn cycle(&mut self) -> u64 {
        let present_id = self.next_id;
        self.next_id += 1;
        let host_ns = 1_000_000_000 + u64::from(present_id) * 16_666_667;
        let cell = Arc::clone(&self.cell);
        cell.publish(present_id, host_ns);
        completion_checksum(self.cell.load())
    }
}

fn completion_checksum(completion: Completion) -> u64 {
    u64::from(completion.present_id)
        .wrapping_add(completion.host_ns)
        .wrapping_add(completion.interval_ns.rotate_left(7))
        .wrapping_add(completion.refresh_ns.rotate_left(13))
}

#[inline(never)]
const fn driver_call(work: u64, call: u64) -> u64 {
    black_box(
        work.rotate_left(7)
            .wrapping_add(call)
            .wrapping_mul(0x9e37_79b9),
    )
}

fn legacy_gl_upload(sequence: &mut u64) -> u64 {
    let mut work = *sequence;
    for call in 0..7 {
        work = driver_call(work, call);
    }
    *sequence = sequence.wrapping_add(1);
    black_box(work);
    upload_checksum(*sequence)
}

fn retained_gl_upload(sequence: &mut u64) -> u64 {
    let mut work = *sequence;
    for call in [0, 5] {
        work = driver_call(work, call);
    }
    *sequence = sequence.wrapping_add(1);
    black_box(work);
    upload_checksum(*sequence)
}

const fn upload_checksum(sequence: u64) -> u64 {
    sequence.rotate_left(11).wrapping_add(0xfeed_beef)
}

fn legacy_metal_batch(sequence: &mut u64) -> u64 {
    let mut work = *sequence;
    for upload in 0..METAL_UPLOADS {
        work = driver_call(work, 1); // command buffer
        work = driver_call(work, 2); // blit encoder
        work = driver_call(work, 3); // copy
        if upload % 2 == 0 {
            work = driver_call(work, 4); // mipmaps
        }
        work = driver_call(work, 5); // end encoding
        work = driver_call(work, 6); // commit
    }
    *sequence = sequence.wrapping_add(1);
    black_box(work);
    upload_checksum(*sequence)
}

fn batched_metal_upload(sequence: &mut u64) -> u64 {
    let mut work = *sequence;
    work = driver_call(work, 1);
    work = driver_call(work, 2);
    for upload in 0..METAL_UPLOADS {
        work = driver_call(work, 3);
        if upload % 2 == 0 {
            work = driver_call(work, 4);
        }
    }
    work = driver_call(work, 5);
    work = driver_call(work, 6);
    *sequence = sequence.wrapping_add(1);
    black_box(work);
    upload_checksum(*sequence)
}

fn print_result(label: &str, result: &Measurement, items: usize) {
    println!(
        "  {label:<27} {:>9.2} ns/op {:>9.2} cycles/op {:>9.2} ns p95 \
         {:>8.2} Mitem/s {:>5.3} alloc {:>5.3} realloc {:>5.3} free {:>9.1} churn B/op",
        result.ns_per_op,
        result.cycles_per_op.unwrap_or(f64::NAN),
        result.p95_ns,
        items as f64 * 1_000.0 / result.ns_per_op,
        result.allocated.allocs as f64 / ALLOC_OPS as f64,
        result.allocated.reallocs as f64 / ALLOC_OPS as f64,
        result.allocated.frees as f64 / ALLOC_OPS as f64,
        result.allocated.churn_bytes() as f64 / ALLOC_OPS as f64,
    );
}

fn print_change(old: &Measurement, new: &Measurement) {
    println!(
        "  old -> new                  {:>8.2}% latency {:>8.2}% cycles {:>8.2}% p95 {:>8.2}% churn",
        percent_change(old.ns_per_op, new.ns_per_op),
        percent_change(
            old.cycles_per_op.unwrap_or(f64::NAN),
            new.cycles_per_op.unwrap_or(f64::NAN),
        ),
        percent_change(old.p95_ns, new.p95_ns),
        percent_change(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );
}

fn percent_change(old: f64, new: f64) -> f64 {
    if old == 0.0 && new == 0.0 {
        return 0.0;
    }
    (new / old - 1.0) * 100.0
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}

fn main() {
    let (tx, rx) = mpsc::channel();
    let mut old_completion = LegacyCompletions {
        tx,
        rx,
        last: Completion::default(),
        next_id: 1,
    };
    let mut new_completion = AtomicCompletions {
        cell: Arc::new(CompletionCell::new()),
        next_id: 1,
    };
    for _ in 0..64 {
        assert_eq!(old_completion.cycle(), new_completion.cycle());
    }
    let [old_completion_result, new_completion_result] =
        measure_pair(|| old_completion.cycle(), || new_completion.cycle());
    assert_eq!(
        old_completion_result.checksum,
        new_completion_result.checksum
    );
    println!("wgpu completion publish + drain (one completion per frame)");
    print_result("old: unbounded channel", &old_completion_result, 1);
    print_result("new: atomic latest cell", &new_completion_result, 1);
    print_change(&old_completion_result, &new_completion_result);

    let mut old_gl_sequence = 0;
    let mut new_gl_sequence = 0;
    assert_eq!(
        legacy_gl_upload(&mut old_gl_sequence),
        retained_gl_upload(&mut new_gl_sequence)
    );
    let [old_gl_result, new_gl_result] = measure_pair(
        || legacy_gl_upload(&mut old_gl_sequence),
        || retained_gl_upload(&mut new_gl_sequence),
    );
    assert_eq!(old_gl_result.checksum, new_gl_result.checksum);
    println!("\nOpenGL dynamic texture update (7 -> 2 driver calls)");
    print_result("old: restate + unbind", &old_gl_result, 1);
    print_result("new: retained unpack state", &new_gl_result, 1);
    print_change(&old_gl_result, &new_gl_result);

    let mut old_metal_sequence = 0;
    let mut new_metal_sequence = 0;
    assert_eq!(
        legacy_metal_batch(&mut old_metal_sequence),
        batched_metal_upload(&mut new_metal_sequence)
    );
    let [old_metal_result, new_metal_result] = measure_pair(
        || legacy_metal_batch(&mut old_metal_sequence),
        || batched_metal_upload(&mut new_metal_sequence),
    );
    assert_eq!(old_metal_result.checksum, new_metal_result.checksum);
    println!(
        "\nMetal texture upload batch ({METAL_UPLOADS} uploads, alternating mipmaps, 88 -> 28 driver calls)"
    );
    print_result("old: command per upload", &old_metal_result, METAL_UPLOADS);
    print_result("new: one blit batch", &new_metal_result, METAL_UPLOADS);
    print_change(&old_metal_result, &new_metal_result);
}
